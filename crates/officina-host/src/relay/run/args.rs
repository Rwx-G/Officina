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

/// Why the arguments of `officina run` are not a request: a usage error,
/// which the caller reports with exit 64.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    #[error("missing action")]
    MissingAction,
    #[error("missing action: the first argument is a parameter")]
    ParameterFirst,
    #[error("--list takes no other argument")]
    ListTakesNothing,
    #[error("unknown option {0}")]
    UnknownOption(String),
    #[error("--wait takes seconds from {} to {}", WAIT_RANGE_S.start(), WAIT_RANGE_S.end())]
    WaitOutOfRange,
    #[error("--wait is given twice")]
    WaitTwice,
    #[error("{0:?} is not name=value")]
    NotNameValue(String),
    #[error("parameter {0} is given twice")]
    ParameterTwice(String),
}

/// Parses the arguments after `run` by hand (AD-29): `--list` alone, or an
/// action then `name=value` tokens split at the first `=`, and `--wait <s>`
/// once, in [`WAIT_RANGE_S`] ([`DEFAULT_WAIT_S`] when absent). Anything else
/// is a [`UsageError`].
pub fn parse_args(args: &[String]) -> Result<RunArgs, UsageError> {
    let Some((action, tokens)) = args.split_first() else {
        return Err(UsageError::MissingAction);
    };
    if action == "--list" {
        return if tokens.is_empty() {
            Ok(RunArgs::List)
        } else {
            Err(UsageError::ListTakesNothing)
        };
    }
    if action.starts_with('-') {
        return Err(UsageError::UnknownOption(action.clone()));
    }
    if action.contains('=') {
        return Err(UsageError::ParameterFirst);
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
                .ok_or(UsageError::WaitOutOfRange)?;
            if wait_s.replace(seconds).is_some() {
                return Err(UsageError::WaitTwice);
            }
            continue;
        }
        let Some((name, value)) = token.split_once('=') else {
            return Err(UsageError::NotNameValue(token.clone()));
        };
        if params.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err(UsageError::ParameterTwice(name.to_owned()));
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
    }

    #[test]
    fn each_usage_error_names_its_cause() {
        let past = (WAIT_RANGE_S.end() + 1).to_string();
        let below = (WAIT_RANGE_S.start() - 1).to_string();
        for (usage, error) in [
            (&[][..], UsageError::MissingAction),
            (
                &["check", "group"],
                UsageError::NotNameValue("group".into()),
            ),
            (
                &["check", "group=a", "group=b"],
                UsageError::ParameterTwice("group".into()),
            ),
            (&["group=lab-web"], UsageError::ParameterFirst),
            (&["--list", "extra"], UsageError::ListTakesNothing),
            (
                &["--wait", "10"],
                UsageError::UnknownOption("--wait".into()),
            ),
            (&["apply", "--wait"], UsageError::WaitOutOfRange),
            (&["apply", "--wait", "x"], UsageError::WaitOutOfRange),
            (&["apply", "--wait", &past], UsageError::WaitOutOfRange),
            (&["apply", "--wait", &below], UsageError::WaitOutOfRange),
            (
                &["apply", "--wait", "5", "--wait", "6"],
                UsageError::WaitTwice,
            ),
        ] {
            assert_eq!(parse_args(&args(usage)), Err(error), "{usage:?}");
        }
        assert_eq!(
            UsageError::WaitOutOfRange.to_string(),
            format!(
                "--wait takes seconds from {} to {}",
                WAIT_RANGE_S.start(),
                WAIT_RANGE_S.end()
            )
        );
    }
}
