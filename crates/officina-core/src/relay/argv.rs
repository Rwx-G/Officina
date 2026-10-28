//! Argument vectors (AD-30): a validated action and its parameter values
//! become the exact launch, for both runners. No shell ever sees it: each
//! `args` item yields exactly one argument, whatever the value holds.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::catalogue::{Action, ArgTemplate, Runner};
use super::params::{self, ValueRefusal};

/// File name of the WSL launcher, joined to the system directory the host
/// reads with `GetSystemDirectoryW`: never `PATH` or `%SystemRoot%`, which a
/// session can set (AD-30).
pub const WSL_EXE: &str = "wsl.exe";
/// Sets the action's `env` inside Linux, by absolute path (D-6).
pub const WSL_ENV_PROGRAM: &str = "/usr/bin/env";
/// Bounds the Linux process by absolute path (D-6), a second line behind the
/// job object (AD-31).
pub const WSL_TIMEOUT_PROGRAM: &str = "/usr/bin/timeout";
/// `timeout -k`: seconds between its TERM and its KILL.
pub const WSL_KILL_AFTER_S: u32 = 10;
/// Added to `timeout_s` for the inner timeout, so the outer kill fires first
/// and the journal says `timeout` (D-6).
pub const WSL_TIMEOUT_MARGIN_S: u32 = 5;
/// Removed from `wsl.exe`'s environment: it would forward Windows variables
/// into Linux behind the catalogue's back (D-6).
pub const WSL_REMOVED_ENV: &[&str] = &["WSLENV"];

/// A class of characters E13 verified to reach a Linux program intact
/// through `wsl.exe --exec`.
pub struct CharClass {
    pub name: &'static str,
    /// True when a value holds a character of this class.
    pub matches: fn(&str) -> bool,
}

/// The classes of E13. Values on the `wsl` runner follow the same rules as
/// `native` ones only because of that verification, so the vector set keeps
/// one `wsl` case per class: a WSL release that changes the handling shows up
/// as a failing case.
pub const WSL_VERIFIED_CLASSES: &[CharClass] = &[
    CharClass {
        name: "space",
        matches: |v| v.contains(' '),
    },
    CharClass {
        name: "double_quote",
        matches: |v| v.contains('"'),
    },
    CharClass {
        name: "dollar",
        matches: |v| v.contains('$'),
    },
    CharClass {
        name: "asterisk",
        matches: |v| v.contains('*'),
    },
    CharClass {
        name: "semicolon",
        matches: |v| v.contains(';'),
    },
    CharClass {
        name: "inner_backslash",
        matches: |v| v.trim_end_matches('\\').contains('\\'),
    },
    CharClass {
        name: "trailing_backslash",
        matches: |v| v.ends_with('\\'),
    },
    CharClass {
        name: "accented_letter",
        matches: |v| v.chars().any(|c| c.is_alphabetic() && !c.is_ascii()),
    },
];

/// What to start: the host spawns `program` with `args` in `cwd`, adds `env`
/// and removes `env_remove` from the inherited environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub env_remove: Vec<String>,
}

/// Why parameters do not fit an action (AD-29 step 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArgvRefusal {
    #[error("parameter {param} is missing")]
    MissingParameter { param: String },
    #[error("parameter {param} is not declared by this action")]
    UnknownParameter { param: String },
    #[error("parameter {param}: {refusal}")]
    Value {
        param: String,
        refusal: ValueRefusal,
    },
}

/// Checks `params` against the action (every declared one present, none
/// extra, each accepted) and builds the launch (AD-30):
///
/// - `native`: the program itself, the action's `env` added;
/// - `wsl`: `<system_dir>\wsl.exe [-d <distro>] --cd <project_dir> --exec
///   /usr/bin/env K=V ... /usr/bin/timeout -k 10 <timeout_s + 5> <program>
///   <args...>`, with `WSLENV` removed.
///
/// `project_dir` is the project folder as the floor declares it, not its
/// lowercased comparison form (D-6). Both runners start in it.
pub fn build(
    action: &Action,
    params: &BTreeMap<String, String>,
    project_dir: &str,
    system_dir: &str,
) -> Result<Launch, ArgvRefusal> {
    if let Some(param) = action.params.keys().find(|p| !params.contains_key(*p)) {
        return Err(ArgvRefusal::MissingParameter {
            param: param.clone(),
        });
    }
    if let Some(param) = params.keys().find(|p| !action.params.contains_key(*p)) {
        return Err(ArgvRefusal::UnknownParameter {
            param: param.clone(),
        });
    }
    for (name, param) in &action.params {
        let value = params.get(name).map_or("", String::as_str);
        params::accept(param, action.runner, value).map_err(|refusal| ArgvRefusal::Value {
            param: name.clone(),
            refusal,
        })?;
    }
    let args = action.args.iter().map(|item| match item {
        ArgTemplate::Literal(text) => text.clone(),
        ArgTemplate::Placeholder {
            prefix,
            param,
            suffix,
        } => format!(
            "{prefix}{}{suffix}",
            params.get(param).map_or("", String::as_str)
        ),
    });
    Ok(match action.runner {
        Runner::Native => Launch {
            program: action.program.clone(),
            args: args.collect(),
            cwd: project_dir.to_owned(),
            env: action.env.clone(),
            env_remove: Vec::new(),
        },
        Runner::Wsl => {
            let mut wsl_args = Vec::new();
            if let Some(distro) = &action.distro {
                wsl_args.extend(["-d".to_owned(), distro.clone()]);
            }
            wsl_args.extend([
                "--cd".to_owned(),
                project_dir.to_owned(),
                "--exec".to_owned(),
                WSL_ENV_PROGRAM.to_owned(),
            ]);
            wsl_args.extend(action.env.iter().map(|(k, v)| format!("{k}={v}")));
            wsl_args.extend([
                WSL_TIMEOUT_PROGRAM.to_owned(),
                "-k".to_owned(),
                WSL_KILL_AFTER_S.to_string(),
                (action.timeout_s + WSL_TIMEOUT_MARGIN_S).to_string(),
                action.program.clone(),
            ]);
            wsl_args.extend(args);
            Launch {
                program: format!("{}\\{WSL_EXE}", system_dir.trim_end_matches(['\\', '/'])),
                args: wsl_args,
                cwd: project_dir.to_owned(),
                env: BTreeMap::new(),
                env_remove: WSL_REMOVED_ENV.iter().map(|k| (*k).to_owned()).collect(),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::catalogue::action_from_value;
    use crate::relay::fixtures::params;

    fn action(toml_text: &str) -> Action {
        action_from_value(&toml::from_str(toml_text).unwrap()).unwrap()
    }

    #[test]
    fn a_value_never_splits_or_merges_arguments() {
        let probe = action(
            "name = \"p\"\nclass = \"free\"\nrunner = \"wsl\"\nprogram = \"/usr/bin/tool\"\nargs = [\"--q={q}\", \"tail\"]\n[params.q]\npattern = \".+\"\n",
        );
        let launch = build(
            &probe,
            &params(&[("q", "a b;c \"d\"")]),
            "C:/w",
            r"C:\Windows\System32",
        )
        .unwrap();
        let tail = &launch.args[launch.args.len() - 2..];
        assert_eq!(tail, ["--q=a b;c \"d\"", "tail"]);
    }

    #[test]
    fn missing_comes_before_unknown_and_values() {
        let probe = action(
            "name = \"p\"\nclass = \"free\"\nprogram = \"C:/t.exe\"\nargs = [\"{a}\", \"{b}\"]\n[params.a]\nvalues = [\"x\"]\n[params.b]\nvalues = [\"y\"]\n",
        );
        assert_eq!(
            build(&probe, &params(&[("a", "-x"), ("z", "1")]), "C:/w", "C:/s"),
            Err(ArgvRefusal::MissingParameter { param: "b".into() })
        );
        assert_eq!(
            build(
                &probe,
                &params(&[("a", "-x"), ("b", "y"), ("z", "1")]),
                "C:/w",
                "C:/s"
            ),
            Err(ArgvRefusal::UnknownParameter { param: "z".into() })
        );
        assert_eq!(
            build(&probe, &params(&[("a", "-x"), ("b", "y")]), "C:/w", "C:/s"),
            Err(ArgvRefusal::Value {
                param: "a".into(),
                refusal: ValueRefusal::LeadingDash
            })
        );
    }

    #[test]
    fn system_dir_separator_is_not_doubled() {
        let probe = action(
            "name = \"p\"\nclass = \"free\"\nrunner = \"wsl\"\nprogram = \"/usr/bin/tool\"\n",
        );
        for dir in [
            r"C:\Windows\System32",
            r"C:\Windows\System32\",
            "C:/Windows/System32/",
        ] {
            let launch = build(&probe, &BTreeMap::new(), "C:/w", dir).unwrap();
            assert!(
                launch.program.ends_with("System32\\wsl.exe"),
                "{}",
                launch.program
            );
        }
    }
}
