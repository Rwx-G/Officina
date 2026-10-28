//! The registry's schema and its migrations (AD-40, AD-41).
//!
//! Entry `i` of [`MIGRATIONS`] moves a database from version `i` to `i + 1`.
//! A shipped entry never changes: `tests/registry_schema.rs` pins the
//! SHA-256 of each one. Until v0.3 ships, an Epic 3 story that must change
//! schema 1 edits it and its pinned hash in one commit (P-14); from the
//! release on, migrations are append-only.
//!
//! Every column holding a registry enum carries a `CHECK` list of the enum's
//! text forms; the schema test reads those lists from `sqlite_master` and
//! compares each with the enum's `ALL`, so a new variant fails until a
//! migration widens the check. `session_fact.fact` has none on purpose: a
//! new fact needs no migration (AD-40, D-88).

/// Schema version 1 (AD-40, with the additions of D-88).
///
/// Beyond AD-40's table, `event_file.rewrites` counts the times a file was
/// found shorter than its offset and read again from its start (D-84), so
/// doctor and the view can report it; `triage_call` keeps the case a call
/// was about and the hash of its inputs, so a refused case is not queued
/// again until its inputs change across a restart (D-87), and why a call
/// was refused, which the view and doctor show (D-65). Two triggers delete a
/// session's `model_answer` row whenever its excerpts are deleted or their
/// text changes (D-68): archive, expiry, `no_excerpts` and a newer excerpt
/// all take the model's untrusted text with them, whichever path clears
/// them.
const V1: &str = "\
CREATE TABLE event_file (
    name TEXT PRIMARY KEY,
    ingested_bytes INTEGER NOT NULL DEFAULT 0,
    complete_at_ms INTEGER,
    corrupt_lines INTEGER NOT NULL DEFAULT 0,
    rewrites INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE subject (
    id INTEGER PRIMARY KEY,
    project TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'to_resume', 'archived')),
    status_src TEXT NOT NULL CHECK (status_src IN ('agents', 'hook', 'rules', 'model', 'author')),
    status_at_ms INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    routine INTEGER NOT NULL DEFAULT 0 CHECK (routine IN (0, 1)),
    merged_into INTEGER REFERENCES subject (id)
) STRICT;

CREATE TABLE session (
    session_id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL,
    subject_id INTEGER REFERENCES subject (id),
    subject_src TEXT CHECK (subject_src IN ('agents', 'hook', 'rules', 'model', 'author')),
    subject_at_ms INTEGER
) STRICT;

CREATE TABLE session_fact (
    session_id TEXT NOT NULL REFERENCES session (session_id) ON DELETE CASCADE,
    fact TEXT NOT NULL,
    value ANY,
    src TEXT NOT NULL CHECK (src IN ('agents', 'hook', 'rules', 'model', 'author')),
    at_ms INTEGER NOT NULL,
    PRIMARY KEY (session_id, fact)
) STRICT, WITHOUT ROWID;

CREATE TABLE session_excerpt (
    session_id TEXT PRIMARY KEY REFERENCES session (session_id) ON DELETE CASCADE,
    prompt TEXT,
    prompt_at_ms INTEGER,
    answer_head TEXT,
    answer_tail TEXT,
    answer_at_ms INTEGER
) STRICT;

CREATE TABLE triage_call (
    id INTEGER PRIMARY KEY,
    purpose TEXT NOT NULL CHECK (purpose IN ('proposal', 'dry_run')),
    case_key TEXT,
    input_hash TEXT,
    started_at_ms INTEGER NOT NULL,
    duration_ms INTEGER,
    outcome TEXT NOT NULL CHECK (outcome IN ('answer', 'refused', 'failed', 'timeout', 'invalid')),
    refusal TEXT CHECK (refusal IN ('claude_not_found', 'cmd_shim', 'no_excerpts', 'workdir_not_empty')),
    input_tokens INTEGER,
    output_tokens INTEGER,
    cost_microusd INTEGER
) STRICT;

CREATE TABLE confirmation (
    id INTEGER PRIMARY KEY,
    subject_id INTEGER NOT NULL REFERENCES subject (id),
    session_id TEXT REFERENCES session (session_id),
    proposed_verdict TEXT NOT NULL CHECK (proposed_verdict IN ('finished', 'to_resume', 'unsure')),
    proposed_source TEXT NOT NULL CHECK (proposed_source IN ('state', 'rules', 'model')),
    proposed_reason TEXT NOT NULL CHECK (proposed_reason IN ('state_done', 'state_failed', 'state_stopped', 'cut_mid_turn', 'default_to_resume', 'empty_subject', 'model')),
    call_id INTEGER,
    verdict TEXT NOT NULL CHECK (verdict IN ('finished', 'to_resume')),
    auto INTEGER NOT NULL DEFAULT 0 CHECK (auto IN (0, 1)),
    confirmed_at_ms INTEGER NOT NULL,
    reopened_at_ms INTEGER
) STRICT;

CREATE TABLE confirmation_input (
    confirmation_id INTEGER PRIMARY KEY REFERENCES confirmation (id) ON DELETE CASCADE,
    prompt TEXT,
    answer_head TEXT,
    answer_tail TEXT,
    end_reason TEXT,
    cut_mid_turn INTEGER NOT NULL CHECK (cut_mid_turn IN (0, 1)),
    expires_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE model_answer (
    session_id TEXT PRIMARY KEY REFERENCES session (session_id) ON DELETE CASCADE,
    verdict TEXT NOT NULL CHECK (verdict IN ('finished', 'to_resume', 'unsure')),
    reason TEXT NOT NULL,
    call_id INTEGER,
    at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE model_evaluation (
    id INTEGER PRIMARY KEY,
    at_ms INTEGER NOT NULL,
    cases INTEGER NOT NULL,
    model_correct INTEGER NOT NULL,
    default_correct INTEGER NOT NULL,
    model_false_finished INTEGER NOT NULL,
    unanswered INTEGER NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cost_microusd INTEGER
) STRICT;

CREATE TABLE setting (
    key TEXT PRIMARY KEY CHECK (key IN ('restore_notice', 'triage_model', 'triage_model_pause', 'auto_archive')),
    value ANY,
    src TEXT NOT NULL CHECK (src IN ('agents', 'hook', 'rules', 'model', 'author')),
    at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX session_by_subject ON session (subject_id);
CREATE INDEX unfiled_session_by_project ON session (project) WHERE subject_id IS NULL;
CREATE INDEX subject_by_project_and_status ON subject (project, status);
CREATE INDEX confirmation_by_source ON confirmation (proposed_source, confirmed_at_ms);
CREATE INDEX confirmation_by_subject ON confirmation (subject_id);

CREATE TRIGGER model_answer_leaves_with_its_excerpts AFTER DELETE ON session_excerpt
BEGIN
    DELETE FROM model_answer WHERE session_id = old.session_id;
END;

CREATE TRIGGER model_answer_leaves_with_changed_excerpts AFTER UPDATE ON session_excerpt
WHEN old.prompt IS NOT new.prompt OR old.answer_head IS NOT new.answer_head
    OR old.answer_tail IS NOT new.answer_tail
BEGIN
    DELETE FROM model_answer WHERE session_id = old.session_id;
END;
";

/// Every migration, in order: entry `i` moves version `i` to `i + 1`.
pub const MIGRATIONS: &[&str] = &[V1];

/// The schema version this build reads and writes: the number of
/// migrations.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// The version a database is at, compared with the version a build expects
/// (AD-41's table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionState {
    /// `user_version` 0: no registry yet.
    Absent,
    /// Written by an older Officina: the window migrates it.
    Older,
    /// The version this build expects.
    Equal,
    /// Written by a newer Officina: read-only for the window, unread by the
    /// command line.
    Newer,
}

impl VersionState {
    /// Where `found` stands against `expected`.
    pub fn of(found: i64, expected: i64) -> VersionState {
        match found {
            0 => VersionState::Absent,
            v if v < expected => VersionState::Older,
            v if v == expected => VersionState::Equal,
            _ => VersionState::Newer,
        }
    }
}

#[cfg(feature = "window-authority")]
pub use migrate::migrate;

/// The writing half: running migrations (D-83).
#[cfg(feature = "window-authority")]
mod migrate {
    use rusqlite::{Connection, TransactionBehavior};

    /// Moves the database from its `user_version` to `migrations.len()`, one
    /// transaction per entry that also sets `user_version`: a failed entry
    /// leaves the version, and every table, as they were before it
    /// (`user_version` rolls back with its transaction, verified in the
    /// addendum). Returns the version reached. A database already at or past
    /// the last entry is left alone.
    pub fn migrate(conn: &mut Connection, migrations: &[&str]) -> rusqlite::Result<i64> {
        let mut version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let target = i64::try_from(migrations.len()).unwrap_or(i64::MAX);
        while version < target {
            let Some(sql) = usize::try_from(version)
                .ok()
                .and_then(|index| migrations.get(index))
            else {
                break;
            };
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version + 1)?;
            tx.commit()?;
            version += 1;
        }
        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_states_follow_ad_41() {
        assert_eq!(VersionState::of(0, 1), VersionState::Absent);
        assert_eq!(VersionState::of(1, 2), VersionState::Older);
        assert_eq!(VersionState::of(2, 2), VersionState::Equal);
        assert_eq!(VersionState::of(3, 2), VersionState::Newer);
    }
}
