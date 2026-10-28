//! The arguments of `officina run` (AD-29), parsed by hand.

use std::collections::BTreeMap;

use officina_core::relay::approvals::DEFAULT_WAIT_S;
use officina_core::relay::classes::WAIT_RANGE_S;

/// What `officina run` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunArgs {
    List,
    Action {
        action: String,
        params: BTreeMap<String, String>,
        /// `--wait <s>`: how long a `human` action waits for the author's
        /// decision (AD-33).
        wait_s: u32,
    },
}

/// Parses the arguments after `run` by hand (AD-29): `--list` alone, or an
/// action then `name=value` tokens split at the first `=`, and `--wait <s>`
/// once, in [`WAIT_RANGE_S`] ([`DEFAULT_WAIT_S`] when absent). A token
/// without `=`, a duplicate name, a missing action, an unknown option or a
/// `--wait` out of range is a usage error, which the caller reports with
/// exit 64.
pub fn parse_args(args: &[String]) -> Result<RunArgs, String> {
    let Some((action, tokens)) = args.split_first() else {
        return Err("missing action".to_owned());
    };
    if action == "--list" {
        return if tokens.is_empty() {
            Ok(RunArgs::List)
        } else {
            Err("--list takes no other argument".to_owned())
        };
    }
    if action.starts_with('-') {
        return Err(format!("unknown option {action}"));
    }
    if action.contains('=') {
        return Err("missing action: the first argument is a parameter".to_owned());
    }
    let mut params = BTreeMap::new();
    let mut wait_s = None;
    let mut tokens = tokens.iter();
    while let Some(token) = tokens.next() {
        if token == "--wait" {
            let seconds = tokens
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|seconds| WAIT_RANGE_S.contains(seconds))
                .ok_or_else(|| {
                    format!(
                        "--wait takes seconds from {} to {}",
                        WAIT_RANGE_S.start(),
                        WAIT_RANGE_S.end()
                    )
                })?;
            if wait_s.replace(seconds).is_some() {
                return Err("--wait is given twice".to_owned());
            }
            continue;
        }
        let Some((name, value)) = token.split_once('=') else {
            return Err(format!("{token:?} is not name=value"));
        };
        if params.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err(format!("parameter {name} is given twice"));
        }
    }
    Ok(RunArgs::Action {
        action: action.clone(),
        params,
        wait_s: wait_s.unwrap_or(DEFAULT_WAIT_S),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn arguments_split_at_the_first_equals() {
        assert_eq!(parse_args(&args(&["--list"])), Ok(RunArgs::List));
        assert_eq!(
            parse_args(&args(&["check", "group=lab-web", "q=a=b", "empty="])),
            Ok(RunArgs::Action {
                action: "check".to_owned(),
                params: BTreeMap::from([
                    ("group".to_owned(), "lab-web".to_owned()),
                    ("q".to_owned(), "a=b".to_owned()),
                    ("empty".to_owned(), String::new()),
                ]),
                wait_s: DEFAULT_WAIT_S,
            })
        );
        let max = WAIT_RANGE_S.end().to_string();
        assert_eq!(
            parse_args(&args(&["apply", "--wait", &max, "group=lab-web"])),
            Ok(RunArgs::Action {
                action: "apply".to_owned(),
                params: BTreeMap::from([("group".to_owned(), "lab-web".to_owned())]),
                wait_s: *WAIT_RANGE_S.end(),
            })
        );
        let past = (WAIT_RANGE_S.end() + 1).to_string();
        let below = (WAIT_RANGE_S.start() - 1).to_string();
        for usage in [
            &[][..],
            &["check", "group"],
            &["check", "group=a", "group=b"],
            &["group=lab-web"],
            &["--list", "extra"],
            &["--wait", "10"],
            &["apply", "--wait"],
            &["apply", "--wait", "x"],
            &["apply", "--wait", &past],
            &["apply", "--wait", &below],
            &["apply", "--wait", "5", "--wait", "6"],
        ] {
            assert!(parse_args(&args(usage)).is_err(), "{usage:?}");
        }
    }
}
