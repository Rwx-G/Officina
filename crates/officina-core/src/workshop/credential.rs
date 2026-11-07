//! A floor's credential state (D-607, story 6.26): folded from the
//! journal's `workshop_credential` lines and moved on by three signals, a
//! run ending `auth_failed`, the auth probe's answer and a replaced token.
//! Nothing here retries: each signal comes from a run the author launched,
//! a click, or the window's start (D-607's "no loop").

use serde::{Deserialize, Serialize};

use super::floor::CredentialKind;
use super::launch::CredentialState;
use crate::relay::journal::{JournalEntry, JournalLine};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What moved a credential state (D-607).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum CredentialCause {
        /// A run of the floor finished `auth_failed`.
        RunAuthFailed,
        /// The auth probe found the stored login.
        ProbePassed,
        /// The auth probe found no usable stored login.
        ProbeFailed,
        /// A token kind's Credential Manager entry was written after the
        /// state was set.
        CredentialReplaced,
    }
}

/// A signal that may move a floor's credential state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSignal {
    /// A run of the floor finished `auth_failed` (FR-8).
    RunAuthFailed,
    /// The auth probe's answer (D-608), and whether the author asked for
    /// it with Check login rather than the window starting it.
    Probe { logged_in: bool, by_click: bool },
    /// The floor's Credential Manager entry was written after the last
    /// line (a token kind's `LastWritten`).
    Replaced,
}

/// The state `signal` moves `current` to for a floor of `kind`, with its
/// cause; `None` when nothing changes, so nothing is journaled.
///
/// A run's authentication failure only ever sets a `needs_*` state: a
/// forged one from a steered worker stops more, never less (R50). The
/// probe reads the stored file only and calls a revoked login logged in
/// (D-1135), so it moves the login kind between `missing` and `unchecked`,
/// and clears `needs_login` only on the author's Check login click, his
/// statement that he logged in again (D-1240); the window's own probes
/// never undo a run's evidence.
pub fn next(
    kind: CredentialKind,
    current: CredentialState,
    signal: CredentialSignal,
) -> Option<(CredentialState, CredentialCause)> {
    let login = kind == CredentialKind::Login;
    match signal {
        CredentialSignal::RunAuthFailed => {
            let target = if login {
                CredentialState::NeedsLogin
            } else {
                CredentialState::NeedsNewCredential
            };
            (current != target).then_some((target, CredentialCause::RunAuthFailed))
        }
        CredentialSignal::Replaced => (!login && current.needs_new()).then_some((
            CredentialState::Unchecked,
            CredentialCause::CredentialReplaced,
        )),
        CredentialSignal::Probe { .. } if !login => None,
        CredentialSignal::Probe {
            logged_in: false, ..
        } => matches!(current, CredentialState::Unchecked | CredentialState::Ok)
            .then_some((CredentialState::Missing, CredentialCause::ProbeFailed)),
        CredentialSignal::Probe {
            logged_in: true,
            by_click,
        } => (current == CredentialState::Missing
            || (by_click && current == CredentialState::NeedsLogin))
            .then_some((CredentialState::Unchecked, CredentialCause::ProbePassed)),
    }
}

/// The last `workshop_credential` line about `floor` and `kind` among
/// `lines`, in their order: its state and when it was written. Lines of
/// another floor, or of another kind (the floor's `auth` changed), say
/// nothing about this one.
pub fn last(
    lines: &[JournalLine],
    floor: &str,
    kind: CredentialKind,
) -> Option<(CredentialState, i64)> {
    lines.iter().rev().find_map(|line| match &line.entry {
        JournalEntry::WorkshopCredential(moved)
            if moved.floor == floor && moved.credential_kind == kind =>
        {
            Some((moved.state, line.ts))
        }
        _ => None,
    })
}

/// The credential state of `floor` for `kind` (D-607): the last line's,
/// `unchecked` when there is none.
pub fn fold(lines: &[JournalLine], floor: &str, kind: CredentialKind) -> CredentialState {
    last(lines, floor, kind)
        .map(|(state, _)| state)
        .unwrap_or_default()
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why Log in for the workshop or Check login did nothing.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum LoginRefusal {
        /// The workshop's stop flag is set (AD-116).
        WorkshopStopped,
        /// No approved floor with a valid `[workshop]`.
        NoFloor,
        /// The floor's `auth` is not `login`.
        NotLoginFloor,
        EngineUnavailable,
        /// The base image is not built: the probe runs it.
        BaseImageMissing,
    }
}

/// What `prepare_login` answers (D-605): the login command on one line,
/// the login volume existing, or why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PrepareLoginAnswer {
    Ready(String),
    Refused(LoginRefusal),
}

/// What Check login found (D-608): only `logged_in` and `auth_method`,
/// whether the line read, and the state after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LoginCheck {
    pub logged_in: bool,
    pub auth_method: Option<String>,
    /// The probe's line did not read: a failed probe (detail
    /// `unreadable`).
    pub unreadable: bool,
    pub state: CredentialState,
}

/// What `check_login` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CheckLoginAnswer {
    Checked(LoginCheck),
    Refused(LoginRefusal),
}

/// The one line `officina-entry auth-status` prints (D-608): whether
/// `claude auth status` exited 0 on the stored login, and its
/// `authMethod`. The email and organisation never leave the container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthStatusLine {
    pub logged_in: bool,
    pub auth_method: Option<String>,
}

/// The most of an `authMethod` kept: a short code of letters, digits and
/// `._-`; anything else is dropped as unreadable.
pub const MAX_AUTH_METHOD_CHARS: usize = 64;

impl AuthStatusLine {
    /// The line as printed, on one line.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Reads the probe's output: its first line, which must be an
    /// [`AuthStatusLine`] with a short method code; `None` otherwise, and
    /// the caller fails closed (`probe_failed`, detail `unreadable`).
    pub fn parse(output: &str) -> Option<AuthStatusLine> {
        let line: AuthStatusLine = serde_json::from_str(output.lines().next()?.trim()).ok()?;
        line.auth_method
            .as_deref()
            .is_none_or(method_is_readable)
            .then_some(line)
    }
}

/// Whether `method` is a short code (letters, digits, `._-`).
pub fn method_is_readable(method: &str) -> bool {
    method.len() <= MAX_AUTH_METHOD_CHARS
        && method
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::journal::WorkshopCredential;

    const FLOOR: &str = "0123456789abcdef";

    fn line(ts: i64, floor: &str, kind: CredentialKind, state: CredentialState) -> JournalLine {
        JournalLine::new(
            ts,
            JournalEntry::WorkshopCredential(WorkshopCredential {
                floor: floor.to_owned(),
                credential_kind: kind,
                state,
                cause: CredentialCause::RunAuthFailed,
            }),
        )
    }

    #[test]
    fn the_last_line_wins() {
        let kind = CredentialKind::OauthToken;
        let lines = vec![
            line(1, FLOOR, kind, CredentialState::NeedsNewCredential),
            line(2, FLOOR, kind, CredentialState::Unchecked),
            line(3, FLOOR, kind, CredentialState::NeedsNewCredential),
        ];
        assert_eq!(
            fold(&lines, FLOOR, kind),
            CredentialState::NeedsNewCredential
        );
        assert_eq!(
            last(&lines, FLOOR, kind),
            Some((CredentialState::NeedsNewCredential, 3))
        );
        assert_eq!(fold(&lines[..2], FLOOR, kind), CredentialState::Unchecked);
        assert_eq!(fold(&[], FLOOR, kind), CredentialState::Unchecked);
    }

    #[test]
    fn another_floors_lines_are_ignored() {
        let kind = CredentialKind::Login;
        let lines = vec![line(
            1,
            "fedcba9876543210",
            kind,
            CredentialState::NeedsLogin,
        )];
        assert_eq!(fold(&lines, FLOOR, kind), CredentialState::Unchecked);
        assert_eq!(
            fold(&lines, "fedcba9876543210", kind),
            CredentialState::NeedsLogin
        );
    }

    #[test]
    fn a_changed_kind_starts_unchecked() {
        let lines = vec![line(
            1,
            FLOOR,
            CredentialKind::Login,
            CredentialState::NeedsLogin,
        )];
        for kind in CredentialKind::ALL
            .iter()
            .filter(|k| **k != CredentialKind::Login)
        {
            assert_eq!(fold(&lines, FLOOR, *kind), CredentialState::Unchecked);
        }
    }

    #[test]
    fn a_forged_auth_failure_cannot_clear_a_state() {
        for kind in CredentialKind::ALL {
            for current in CredentialState::ALL {
                match next(*kind, *current, CredentialSignal::RunAuthFailed) {
                    Some((state, cause)) => {
                        assert!(state.needs_new(), "{kind:?} {current:?} -> {state:?}");
                        assert_eq!(cause, CredentialCause::RunAuthFailed);
                    }
                    None => assert!(current.needs_new(), "{kind:?} {current:?} kept"),
                }
            }
        }
    }

    /// Every state under every signal, for every kind: the move is the one
    /// D-607, D-1135 and D-1240 give, and every cause is reachable.
    #[test]
    fn every_state_and_cause_has_a_case() {
        use CredentialState::{Missing, NeedsLogin, NeedsNewCredential, Ok, Unchecked};
        let signals = [
            CredentialSignal::RunAuthFailed,
            CredentialSignal::Replaced,
            CredentialSignal::Probe {
                logged_in: false,
                by_click: false,
            },
            CredentialSignal::Probe {
                logged_in: false,
                by_click: true,
            },
            CredentialSignal::Probe {
                logged_in: true,
                by_click: false,
            },
            CredentialSignal::Probe {
                logged_in: true,
                by_click: true,
            },
        ];
        let mut causes = std::collections::BTreeSet::new();
        for kind in CredentialKind::ALL {
            let login = *kind == CredentialKind::Login;
            for current in CredentialState::ALL {
                for signal in signals {
                    let expected = match (signal, *current) {
                        (CredentialSignal::RunAuthFailed, NeedsLogin) if login => None,
                        (CredentialSignal::RunAuthFailed, _) if login => Some(NeedsLogin),
                        (CredentialSignal::RunAuthFailed, NeedsNewCredential) => None,
                        (CredentialSignal::RunAuthFailed, _) => Some(NeedsNewCredential),
                        (CredentialSignal::Replaced, NeedsLogin | NeedsNewCredential) if !login => {
                            Some(Unchecked)
                        }
                        (CredentialSignal::Replaced, _) => None,
                        (CredentialSignal::Probe { .. }, _) if !login => None,
                        (
                            CredentialSignal::Probe {
                                logged_in: false, ..
                            },
                            Unchecked | Ok,
                        ) => Some(Missing),
                        (
                            CredentialSignal::Probe {
                                logged_in: false, ..
                            },
                            _,
                        ) => None,
                        (
                            CredentialSignal::Probe {
                                logged_in: true, ..
                            },
                            Missing,
                        ) => Some(Unchecked),
                        (
                            CredentialSignal::Probe {
                                logged_in: true,
                                by_click: true,
                            },
                            NeedsLogin,
                        ) => Some(Unchecked),
                        (
                            CredentialSignal::Probe {
                                logged_in: true, ..
                            },
                            _,
                        ) => None,
                    };
                    let moved = next(*kind, *current, signal);
                    assert_eq!(
                        moved.map(|(s, _)| s),
                        expected,
                        "{kind:?} {current:?} {signal:?}"
                    );
                    if let Some((_, cause)) = moved {
                        causes.insert(cause.code());
                    }
                }
            }
        }
        for cause in CredentialCause::ALL {
            assert!(causes.contains(cause.code()), "{cause:?} unreachable");
        }
    }

    #[test]
    fn the_probe_line_reads_only_its_two_fields() {
        let written = AuthStatusLine {
            logged_in: true,
            auth_method: Some("claude.ai".to_owned()),
        };
        assert_eq!(AuthStatusLine::parse(&written.to_line()), Some(written));
        assert_eq!(
            AuthStatusLine::parse(r#"{"logged_in":false,"auth_method":null}"#),
            Some(AuthStatusLine {
                logged_in: false,
                auth_method: None
            })
        );
        for unreadable in [
            "",
            "not json",
            r#"{"logged_in":true,"auth_method":null,"email":"user@example.com"}"#,
            r#"{"logged_in":true,"auth_method":"has space"}"#,
            &format!(
                r#"{{"logged_in":true,"auth_method":"{}"}}"#,
                "a".repeat(MAX_AUTH_METHOD_CHARS + 1)
            ),
        ] {
            assert_eq!(AuthStatusLine::parse(unreadable), None, "{unreadable}");
        }
    }
}
