//! Folding each successful `claude agents` refresh into the registry
//! (AD-46, D-51, D-78, D-79): the collector keeps the previous snapshot in
//! memory, seeded from the registry at start; the core diffs; this writes
//! what changed in one transaction. Compiled only with the
//! `window-authority` feature: only the window writes.

use std::collections::{BTreeMap, BTreeSet};

use officina_core::agents::AgentRecord;
use officina_core::board::ProjectRule;
use officina_core::registry::confirm::wake;
use officina_core::registry::fold::{AgentsChanges, AgentsSnapshot, fold_agents};
use officina_core::registry::types::SessionRecord;
use rusqlite::{Connection, TransactionBehavior};

use super::ingest::ensure_session;
use super::read;
use super::store::Writer;
use super::write::{RegistryError, apply_wake, prune_excerpts, write_fact};

/// The records of `ids` the registry holds, with their successors, so the
/// core can route a record to its chain head.
fn known_records(
    conn: &Connection,
    ids: impl IntoIterator<Item = String>,
) -> rusqlite::Result<BTreeMap<String, SessionRecord>> {
    let mut known = BTreeMap::new();
    let mut wanted: Vec<String> = ids.into_iter().collect();
    while let Some(id) = wanted.pop() {
        if known.contains_key(&id) {
            continue;
        }
        if let Some(record) = read::session_record(conn, &id)? {
            if let Some(next) = &record.successor {
                wanted.push(next.value.clone());
            }
            known.insert(id, record);
        }
    }
    Ok(known)
}

/// The snapshot a starting window assumes (D-51): read once, at the first
/// refresh.
fn seed(conn: &Connection) -> rusqlite::Result<AgentsSnapshot> {
    Ok(AgentsSnapshot::seeded(&read::session_records(conn)?))
}

impl Writer {
    /// Folds one successful `claude agents` refresh into the registry, at
    /// `now_ms`, in one transaction (AD-46): a session first seen here is
    /// created through the one creation function (D-79), changed facts are
    /// written with source `agents`, and a session gone without an end gets
    /// `interrupted`. `snapshot` is the previous refresh's, seeded from the
    /// registry when there is none yet (D-51); it moves on only once the
    /// transaction commits, so a failed write leaves it as it was.
    pub fn apply_agents(
        &mut self,
        snapshot: &mut Option<AgentsSnapshot>,
        records: &[AgentRecord],
        projects: &[ProjectRule],
        now_ms: i64,
    ) -> Result<AgentsChanges, RegistryError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = match snapshot {
            Some(previous) => previous.clone(),
            None => seed(&tx)?,
        };
        let ids = previous
            .session_ids()
            .map(str::to_owned)
            .chain(records.iter().filter_map(|r| r.session_id.clone()))
            .collect::<Vec<_>>();
        let known = known_records(&tx, ids)?;
        let (next, changes) = fold_agents(&previous, records, &known, now_ms);
        for listed in &changes.newly_listed {
            ensure_session(
                &tx,
                &listed.session_id,
                listed.cwd.as_deref(),
                now_ms,
                projects,
            )?;
        }
        for write in &changes.writes {
            write_fact(&tx, write)?;
        }
        for session_id in &changes.touched {
            tx.prepare_cached(
                "UPDATE session SET last_seen_ms = max(last_seen_ms, ?2) WHERE session_id = ?1",
            )?
            .execute(rusqlite::params![session_id, now_ms])?;
        }
        let written: BTreeSet<&str> = changes
            .writes
            .iter()
            .map(|w| w.session_id.as_str())
            .collect();
        let subjects = if written.is_empty() {
            Vec::new()
        } else {
            read::subjects(&tx)?
        };
        for session_id in written {
            let Some(after) = read::session_record(&tx, session_id)? else {
                continue;
            };
            // D-77: listed live again, or a background state no longer
            // ended, wakes the subject.
            if let Some(before) = known.get(session_id)
                && let Some(wake) = wake(before, &after, &subjects)
            {
                apply_wake(&tx, &wake, now_ms)?;
            }
            // D-50: a session newly listed is one the author drives, so its
            // excerpts now leave its subject the latest only.
            let newly_listed = changes
                .newly_listed
                .iter()
                .any(|n| n.session_id == session_id);
            if newly_listed && let Some(subject) = after.filed_subject() {
                prune_excerpts(&tx, subject)?;
            }
        }
        tx.commit()?;
        *snapshot = Some(next);
        Ok(changes)
    }
}
