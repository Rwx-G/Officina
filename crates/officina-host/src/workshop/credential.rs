//! The credential state on the host (D-607, story 6.26): the journal's
//! `workshop_credential` lines folded once at start, moved on by the
//! signals the window sees, each move journaled so the state survives a
//! restart with no registry column (D-212).

use std::sync::{Arc, Mutex};

use officina_core::relay::journal::{
    JOURNAL_RETENTION_DAYS, JournalEntry, JournalLine, WorkshopCredential,
};
use officina_core::workshop::credential::{CredentialSignal, fold, last, next};
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::launch::CredentialState;

use crate::relay::credential::CredentialStore;
use crate::relay::home::RelayHome;
use crate::workshop::launch::{CredentialStateSource, credential_store_name};

/// Who holds a [`JournalStates`]: the window journals the moves it sees;
/// a reader (`officina workshop --json`, doctor) folds the same lines and
/// writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatesRole {
    Window,
    Reader,
}

/// The credential states folded from the journal (D-607). A line older
/// than the journal's retention is gone with its day file, so a state
/// nobody touched for that long reads `unchecked` again.
pub struct JournalStates {
    relay: RelayHome,
    store: Arc<dyn CredentialStore + Send + Sync>,
    role: StatesRole,
    now_ms: fn() -> i64,
    lines: Mutex<Vec<JournalLine>>,
}

impl JournalStates {
    /// Folds the journal of `relay` once (the window's start, or a
    /// reader's one read).
    pub fn load(
        relay: RelayHome,
        store: Arc<dyn CredentialStore + Send + Sync>,
        role: StatesRole,
        now_ms: fn() -> i64,
    ) -> JournalStates {
        let lines = credential_lines(&relay, now_ms());
        JournalStates {
            relay,
            store,
            role,
            now_ms,
            lines: Mutex::new(lines),
        }
    }

    /// Journals the move to `state` (window only) and keeps it in memory.
    fn moved(
        &self,
        lines: &mut Vec<JournalLine>,
        floor_key: &str,
        kind: CredentialKind,
        (state, cause): (
            CredentialState,
            officina_core::workshop::credential::CredentialCause,
        ),
    ) {
        let entry = JournalEntry::WorkshopCredential(WorkshopCredential {
            floor: floor_key.to_owned(),
            credential_kind: kind,
            state,
            cause,
        });
        let now = (self.now_ms)();
        if self.role == StatesRole::Window {
            let _ = crate::relay::journal::record(&self.relay, std::slice::from_ref(&entry), now);
        }
        lines.push(JournalLine::new(now, entry));
    }
}

/// The journal's `workshop_credential` lines within its retention.
fn credential_lines(relay: &RelayHome, now_ms: i64) -> Vec<JournalLine> {
    let (all, _) = crate::relay::journal::read_days(relay, now_ms, JOURNAL_RETENTION_DAYS);
    all.into_iter()
        .filter(|line| matches!(line.entry, JournalEntry::WorkshopCredential(_)))
        .collect()
}

/// A token kind's `needs_new_credential` whose Credential Manager entry
/// was written after the state was set (D-607): the author stored a new
/// token, so the state is `unchecked` again. Read from the entry's
/// metadata, never its value.
fn replaced(
    store: &dyn CredentialStore,
    lines: &[JournalLine],
    floor_key: &str,
    kind: CredentialKind,
) -> bool {
    if kind == CredentialKind::Login {
        return false;
    }
    let Some((state, since_ms)) = last(lines, floor_key, kind) else {
        return false;
    };
    state.needs_new()
        && store
            .last_written_ms(&credential_store_name(floor_key))
            .ok()
            .flatten()
            .is_some_and(|written| written > since_ms)
}

/// The state of `floor_key` for `kind` as a reader folds it now (doctor):
/// the journal's lines and a replaced token, nothing written.
pub fn read_state(
    relay: &RelayHome,
    store: &dyn CredentialStore,
    floor_key: &str,
    kind: CredentialKind,
    now_ms: i64,
) -> CredentialState {
    let lines = credential_lines(relay, now_ms);
    let folded = fold(&lines, floor_key, kind);
    if replaced(store, &lines, floor_key, kind)
        && let Some((state, _)) = next(kind, folded, CredentialSignal::Replaced)
    {
        return state;
    }
    folded
}

impl CredentialStateSource for JournalStates {
    fn state(&self, floor_key: &str, kind: CredentialKind) -> CredentialState {
        let Ok(mut lines) = self.lines.lock() else {
            return CredentialState::Unchecked;
        };
        if replaced(self.store.as_ref(), &lines, floor_key, kind)
            && let Some(moved) = next(
                kind,
                fold(&lines, floor_key, kind),
                CredentialSignal::Replaced,
            )
        {
            self.moved(&mut lines, floor_key, kind, moved);
        }
        fold(&lines, floor_key, kind)
    }

    fn signal(
        &self,
        floor_key: &str,
        kind: CredentialKind,
        signal: CredentialSignal,
    ) -> CredentialState {
        let Ok(mut lines) = self.lines.lock() else {
            return CredentialState::Unchecked;
        };
        if let Some(moved) = next(kind, fold(&lines, floor_key, kind), signal) {
            self.moved(&mut lines, floor_key, kind, moved);
        }
        fold(&lines, floor_key, kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::credential::{Secret, StoreError};

    const FLOOR: &str = "0123456789abcdef";

    struct Written(Option<i64>);

    impl CredentialStore for Written {
        fn read(&self, _name: &str) -> Result<Option<Secret>, StoreError> {
            Ok(None)
        }
        fn last_written_ms(&self, _name: &str) -> Result<Option<i64>, StoreError> {
            Ok(self.0)
        }
    }

    fn at_1000() -> i64 {
        1_000
    }

    fn states(home: &std::path::Path, written: Option<i64>, role: StatesRole) -> JournalStates {
        JournalStates::load(
            RelayHome::new(home, None),
            Arc::new(Written(written)),
            role,
            at_1000,
        )
    }

    #[test]
    fn a_move_survives_a_reload_and_a_reader_writes_nothing() {
        let home = tempfile::tempdir().unwrap();
        let reader = states(home.path(), None, StatesRole::Reader);
        reader.signal(
            FLOOR,
            CredentialKind::Login,
            CredentialSignal::RunAuthFailed,
        );
        assert_eq!(
            reader.state(FLOOR, CredentialKind::Login),
            CredentialState::NeedsLogin
        );
        let reloaded = states(home.path(), None, StatesRole::Reader);
        assert_eq!(
            reloaded.state(FLOOR, CredentialKind::Login),
            CredentialState::Unchecked
        );

        let window = states(home.path(), None, StatesRole::Window);
        window.signal(
            FLOOR,
            CredentialKind::Login,
            CredentialSignal::RunAuthFailed,
        );
        let reloaded = states(home.path(), None, StatesRole::Reader);
        assert_eq!(
            reloaded.state(FLOOR, CredentialKind::Login),
            CredentialState::NeedsLogin
        );
    }

    #[test]
    fn a_token_written_after_the_state_returns_it_to_unchecked() {
        let home = tempfile::tempdir().unwrap();
        let kind = CredentialKind::OauthToken;
        states(home.path(), None, StatesRole::Window).signal(
            FLOOR,
            kind,
            CredentialSignal::RunAuthFailed,
        );
        assert_eq!(
            states(home.path(), Some(999), StatesRole::Reader).state(FLOOR, kind),
            CredentialState::NeedsNewCredential
        );
        assert_eq!(
            states(home.path(), Some(1_001), StatesRole::Window).state(FLOOR, kind),
            CredentialState::Unchecked
        );
        assert_eq!(
            states(home.path(), Some(1_001), StatesRole::Reader).state(FLOOR, kind),
            CredentialState::Unchecked
        );
    }
}
