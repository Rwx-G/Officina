//! The registry's part of the hourly pass (AD-44, D-48, D-84, D-85, D-91,
//! D-94): ingestion first, then the daily copy when due, then retention,
//! then the WAL truncated. Compiled only with the `window-authority`
//! feature: no command line deletes anything (AD-53).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use officina_core::dates::{dated_file_day, utc_date};
use officina_core::registry::retention::{
    AUTO_REVIEW_TTL_MS, COPIES_KEPT, ORCHESTRATOR_START_TTL_MS, PROPOSAL_TTL_MS,
    TRIAGE_CALL_TTL_MS, WORK_ORDER_TTL_MS, event_file_deletable, excerpt_cutoff, kept,
};

use super::ingest::{IngestContext, IngestReport};
use super::store::{
    Writer, copy_is_valid, daily_copies, pre_migration_day, quarantined, read_only_connection,
};
use super::write::RegistryError;

/// What one hourly pass did.
#[derive(Debug, Default)]
pub struct PassReport {
    pub ingested: IngestReport,
    /// The daily copy taken, when one was due.
    pub copy: Option<PathBuf>,
    /// Why the copy failed, when it did: the pass goes on without it, and
    /// deletion relies on the copies already kept.
    pub copy_error: Option<String>,
    /// Event files deleted, by name.
    pub deleted_event_files: Vec<String>,
    /// Copies, pre-migration copies and quarantined databases deleted.
    pub deleted_copies: Vec<PathBuf>,
    /// Sessions whose excerpts were cleared.
    pub cleared_excerpts: usize,
    /// `triage_call` rows past their 90 days (AD-44).
    pub deleted_calls: usize,
    /// `work_order` rows past their 90 days (D-134).
    pub deleted_work_orders: usize,
    /// `work_order` rows whose `printed` and `error` were cleared (D-134).
    pub cleared_work_order_texts: usize,
    /// `auto_review` rows past their 90 days (D-134).
    pub deleted_auto_reviews: usize,
    /// `auto_review` rows whose reason was cleared (D-134).
    pub cleared_review_reasons: usize,
    /// `orchestrator_start` rows past their 90 days (D-134).
    pub deleted_orchestrator_starts: usize,
    /// `proposal` rows past their 90 days (D-134).
    pub deleted_proposals: usize,
    /// `proposal` rows whose texts were cleared (D-134).
    pub cleared_proposal_texts: usize,
    /// False when a reader kept the WAL from being truncated.
    pub checkpointed: bool,
}

/// The UTC day of a quarantine folder, named by its time in milliseconds.
fn quarantine_day(path: &Path) -> Option<(i64, u32, u32)> {
    let ms: i64 = path.file_name()?.to_str()?.parse().ok()?;
    Some(utc_date(ms))
}

/// The names of the event files a copy holds complete; `None` when the
/// copy cannot be read.
fn complete_in(copy: &Path) -> Option<BTreeSet<String>> {
    let conn = read_only_connection(copy).ok()?;
    let mut statement = conn
        .prepare("SELECT name FROM event_file WHERE complete_at_ms IS NOT NULL")
        .ok()?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .ok()?
        .collect::<rusqlite::Result<_>>()
        .ok()?;
    Some(names)
}

fn remove(path: &Path) -> Result<(), RegistryError> {
    let removed = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    removed.map_err(|e| RegistryError::io(path, e))
}

impl Writer {
    /// The registry's hourly pass, also run at the window's start before
    /// the first board (AD-44, D-85): ingest every incomplete file, take
    /// the daily copy when due, delete what has outlived its rule, clear
    /// the expired excerpts (every excerpt under `no_excerpts`), and
    /// truncate the WAL whatever was done (D-91).
    pub fn hourly_pass(
        &mut self,
        ctx: &IngestContext,
        now_ms: i64,
    ) -> Result<PassReport, RegistryError> {
        let mut report = PassReport {
            ingested: self.ingest(ctx, now_ms)?,
            ..PassReport::default()
        };
        match self.copy_if_due(now_ms) {
            Ok(copy) => report.copy = copy,
            Err(error) if error.is_corruption() => return Err(error),
            Err(error) => report.copy_error = Some(error.to_string()),
        }
        report.deleted_copies = self.retire_copies(now_ms)?;
        report.deleted_event_files = self.retire_event_files(ctx.events_dir, now_ms)?;
        report.cleared_excerpts = self.clear_excerpts(ctx.no_excerpts, now_ms)?;
        report.deleted_calls = self.retire_triage_calls(now_ms)?;
        (report.deleted_work_orders, report.cleared_work_order_texts) =
            self.retire_work_orders(now_ms)?;
        (report.deleted_auto_reviews, report.cleared_review_reasons) =
            self.retire_auto_reviews(now_ms)?;
        report.deleted_orchestrator_starts = self.conn.execute(
            "DELETE FROM orchestrator_start WHERE started_at_ms <= ?1",
            [now_ms - ORCHESTRATOR_START_TTL_MS],
        )?;
        (report.deleted_proposals, report.cleared_proposal_texts) =
            self.retire_proposals(now_ms)?;
        report.checkpointed = self.checkpoint()?;
        Ok(report)
    }

    /// Deletes the daily copies beyond the [`COPIES_KEPT`] newest valid
    /// ones and those past their lifetime, then the quarantined databases
    /// and pre-migration copies beyond the two newest or past their
    /// lifetime, all by the date in their name (D-94).
    fn retire_copies(&self, now_ms: i64) -> Result<Vec<PathBuf>, RegistryError> {
        let mut retired = Vec::new();
        let daily: Vec<_> = daily_copies(self.paths())
            .into_iter()
            .filter(|copy| {
                copy_is_valid(&copy.path) || {
                    retired.push(copy.path.clone());
                    false
                }
            })
            .collect();
        let days: Vec<_> = daily.iter().map(|c| c.day).collect();
        for (copy, keep) in daily.iter().zip(kept(&days, now_ms)) {
            if !keep {
                retired.push(copy.path.clone());
            }
        }
        let mut pre_migration: Vec<(PathBuf, (i64, u32, u32))> =
            fs::read_dir(self.paths().backup_dir())
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| {
                    let day = pre_migration_day(&entry.file_name().to_string_lossy())?;
                    Some((entry.path(), day))
                })
                .collect();
        let mut set_aside: Vec<(PathBuf, (i64, u32, u32))> = quarantined(self.paths())
            .into_iter()
            .filter_map(|path| quarantine_day(&path).map(|day| (path, day)))
            .collect();
        for group in [&mut pre_migration, &mut set_aside] {
            group.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
            let days: Vec<_> = group.iter().map(|(_, day)| *day).collect();
            for ((path, _), keep) in group.iter().zip(kept(&days, now_ms)) {
                if !keep {
                    retired.push(path.clone());
                }
            }
        }
        for path in &retired {
            remove(path)?;
        }
        Ok(retired)
    }

    /// Deletes the event files the registry and each kept valid daily copy
    /// hold complete, dated before yesterday (AD-44 as amended by D-48).
    fn retire_event_files(
        &self,
        events_dir: &Path,
        now_ms: i64,
    ) -> Result<Vec<String>, RegistryError> {
        let copies: Vec<BTreeSet<String>> = daily_copies(self.paths())
            .into_iter()
            .filter(|copy| copy_is_valid(&copy.path))
            .take(COPIES_KEPT)
            .filter_map(|copy| complete_in(&copy.path))
            .collect();
        let here: BTreeSet<String> = self
            .event_files()?
            .into_iter()
            .filter(|row| row.complete_at_ms.is_some())
            .map(|row| row.name)
            .collect();
        let mut deleted = Vec::new();
        let names: Vec<String> = fs::read_dir(events_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| dated_file_day(name).is_some())
            .collect();
        for name in names {
            if here.contains(&name) && event_file_deletable(&name, now_ms, &copies) {
                let file = events_dir.join(&name);
                fs::remove_file(&file).map_err(|e| RegistryError::io(&file, e))?;
                deleted.push(name);
            }
        }
        deleted.sort();
        Ok(deleted)
    }

    /// Deletes the `triage_call` rows started [`TRIAGE_CALL_TTL_MS`] or
    /// more before `now_ms` (AD-44).
    fn retire_triage_calls(&self, now_ms: i64) -> Result<usize, RegistryError> {
        Ok(self.conn.execute(
            "DELETE FROM triage_call WHERE started_at_ms <= ?1",
            [now_ms - TRIAGE_CALL_TTL_MS],
        )?)
    }

    /// Deletes the `work_order` rows launched [`WORK_ORDER_TTL_MS`] or more
    /// before `now_ms`, and clears the untrusted texts of those launched
    /// [`EXCERPT_TTL_MS`](officina_core::registry::retention::EXCERPT_TTL_MS)
    /// or more before it, so D-85's bound covers them (D-134). Returns the
    /// rows deleted and the rows whose texts were cleared.
    fn retire_work_orders(&mut self, now_ms: i64) -> Result<(usize, usize), RegistryError> {
        let tx = self.conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM work_order WHERE launched_at_ms <= ?1",
            [now_ms - WORK_ORDER_TTL_MS],
        )?;
        let cleared = tx.execute(
            "UPDATE work_order SET printed = NULL, error = NULL
             WHERE launched_at_ms <= ?1 AND (printed IS NOT NULL OR error IS NOT NULL)",
            [excerpt_cutoff(now_ms)],
        )?;
        tx.commit()?;
        Ok((deleted, cleared))
    }

    /// Deletes the `auto_review` rows requested [`AUTO_REVIEW_TTL_MS`] or
    /// more before `now_ms`, and clears the reason, model text, of those
    /// requested
    /// [`EXCERPT_TTL_MS`](officina_core::registry::retention::EXCERPT_TTL_MS)
    /// or more before it (D-134). Returns the rows deleted and the rows
    /// whose reason was cleared.
    fn retire_auto_reviews(&mut self, now_ms: i64) -> Result<(usize, usize), RegistryError> {
        let tx = self.conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM auto_review WHERE requested_at_ms <= ?1",
            [now_ms - AUTO_REVIEW_TTL_MS],
        )?;
        let cleared = tx.execute(
            "UPDATE auto_review SET reason = NULL
             WHERE requested_at_ms <= ?1 AND reason IS NOT NULL",
            [excerpt_cutoff(now_ms)],
        )?;
        tx.commit()?;
        Ok((deleted, cleared))
    }

    /// Deletes the `proposal` rows received [`PROPOSAL_TTL_MS`] or more
    /// before `now_ms`, and clears the texts, the orchestrator's, of those
    /// received
    /// [`EXCERPT_TTL_MS`](officina_core::registry::retention::EXCERPT_TTL_MS)
    /// or more before it (D-134). Returns the rows deleted and the rows
    /// whose texts were cleared.
    fn retire_proposals(&mut self, now_ms: i64) -> Result<(usize, usize), RegistryError> {
        let tx = self.conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM proposal WHERE received_at_ms <= ?1",
            [now_ms - PROPOSAL_TTL_MS],
        )?;
        let cleared = tx.execute(
            "UPDATE proposal SET texts_json = NULL
             WHERE received_at_ms <= ?1 AND texts_json IS NOT NULL",
            [excerpt_cutoff(now_ms)],
        )?;
        tx.commit()?;
        Ok((deleted, cleared))
    }

    /// Clears the excerpts whose event is older than the TTL, counted from
    /// the event (D-50), and every excerpt when `no_excerpts` is on (D-85),
    /// in `session_excerpt` and in `confirmation_input`; returns how many
    /// sessions lost one.
    fn clear_excerpts(&mut self, no_excerpts: bool, now_ms: i64) -> Result<usize, RegistryError> {
        let tx = self.conn.transaction()?;
        let cleared = if no_excerpts {
            tx.execute("UPDATE confirmation_input SET prompt = NULL, answer_head = NULL, answer_tail = NULL", [])?;
            tx.execute("DELETE FROM session_excerpt", [])?
        } else {
            let cutoff = excerpt_cutoff(now_ms);
            tx.execute(
                "DELETE FROM confirmation_input WHERE expires_at_ms <= ?1",
                [now_ms],
            )?;
            let prompts = tx.execute(
                "UPDATE session_excerpt SET prompt = NULL, prompt_at_ms = NULL WHERE prompt_at_ms <= ?1",
                [cutoff],
            )?;
            let answers = tx.execute(
                "UPDATE session_excerpt SET answer_head = NULL, answer_tail = NULL, answer_at_ms = NULL
                 WHERE answer_at_ms <= ?1",
                [cutoff],
            )?;
            tx.execute(
                "DELETE FROM session_excerpt WHERE prompt IS NULL AND answer_head IS NULL AND answer_tail IS NULL",
                [],
            )?;
            prompts + answers
        };
        tx.commit()?;
        Ok(cleared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_names_give_their_day() {
        assert_eq!(
            pre_migration_day("pre-v2-2026-10-27.db"),
            Some((2026, 10, 27))
        );
        for other in [
            "pre-v2.db",
            "pre-vx-2026-10-27.db",
            "registry-2026-10-27.db",
            "pre-v2-2026-10-27.db.tmp",
        ] {
            assert_eq!(pre_migration_day(other), None, "{other}");
        }
        assert_eq!(
            quarantine_day(Path::new("quarantine/1793102400000")),
            Some((2026, 10, 27))
        );
        assert_eq!(quarantine_day(Path::new("quarantine/notes")), None);
    }
}
