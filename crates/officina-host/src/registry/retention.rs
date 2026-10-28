//! The registry's part of the hourly pass (AD-44, D-48, D-84, D-85, D-91,
//! D-94): ingestion first, then the daily copy when due, then retention,
//! then the WAL truncated. Compiled only with the `window-authority`
//! feature: no command line deletes anything (AD-53).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use officina_core::dates::{dated_file_day, parse_utc_day, utc_date};
use officina_core::registry::retention::{COPIES_KEPT, event_file_deletable, excerpt_cutoff, kept};

use super::ingest::{IngestContext, IngestReport};
use super::store::{Writer, copy_is_valid, daily_copies, quarantined, read_only_connection};
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
    /// False when a reader kept the WAL from being truncated.
    pub checkpointed: bool,
}

/// The UTC day of a pre-migration copy, `pre-v<n>-<date>.db`.
fn pre_migration_day(name: &str) -> Option<(i64, u32, u32)> {
    let rest = name.strip_prefix("pre-v")?.strip_suffix(".db")?;
    let (version, date) = rest.split_once('-')?;
    version.parse::<u32>().ok()?;
    parse_utc_day(date)
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
