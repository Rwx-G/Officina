//! The journal's files (AD-32): locked appends, spills and the two-day
//! reader.
//!
//! Every writer (`officina run`, the gate, the window) takes an exclusive
//! lock on `journal/.lock` with `File::try_lock`, retried every
//! [`LOCK_RETRY`] up to [`LOCK_TIMEOUT`], writes its lines with one
//! `append_line`, then releases it. A separate lock file means readers never
//! meet a locked range, and lines of any length never interleave. Lines that
//! record what already happened cannot be given up when the lock stays held:
//! [`record`] then spills them to a file of their own, which [`read_recent`]
//! reads with its day (D-760). The window alone deletes old files
//! ([`prune`]).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use officina_core::dates::{DAY_MS, event_file_name, event_files_to_read};
use officina_core::relay::journal::{
    AutoModeDenied, JournalEntry, JournalLine, ParsedLine, is_expired_journal_file, parse_line,
    spill_file_name, spilled_day_file,
};

/// What `officina hook` branches on, which does not link `officina-core`
/// itself.
pub use officina_core::relay::journal::DENIED_EVENT;

use super::home::RelayHome;
use crate::files::append_line;

/// How often a writer retries a held lock.
pub const LOCK_RETRY: Duration = Duration::from_millis(5);
/// How long a writer waits for the lock before giving up (AD-32).
pub const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// Why nothing was journaled.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("the journal stayed locked for {} s", LOCK_TIMEOUT.as_secs())]
    LockTimeout,
    #[error("cannot write the journal: {0}")]
    Io(#[from] io::Error),
}

/// The exclusive lock on `journal/.lock`, released on drop. Holding it
/// across a decision and its line (the approval written, then journaled)
/// keeps the two together: a writer that cannot lock writes neither.
#[derive(Debug)]
pub struct JournalLock {
    file: File,
    home: RelayHome,
}

impl JournalLock {
    /// Takes the lock, waiting up to [`LOCK_TIMEOUT`].
    pub fn acquire(home: &RelayHome) -> Result<JournalLock, JournalError> {
        Self::acquire_within(home, LOCK_TIMEOUT)
    }

    /// Takes the lock, waiting up to `timeout`.
    pub fn acquire_within(
        home: &RelayHome,
        timeout: Duration,
    ) -> Result<JournalLock, JournalError> {
        fs::create_dir_all(home.journal_dir())?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(home.journal_lock_file())?;
        let deadline = Instant::now() + timeout;
        loop {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(JournalLock {
                        file,
                        home: home.clone(),
                    });
                }
                Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(fs::TryLockError::WouldBlock) => return Err(JournalError::LockTimeout),
                Err(fs::TryLockError::Error(e)) => return Err(JournalError::Io(e)),
            }
        }
    }

    /// Appends `entries`, stamped `now_ms`, to today's file (UTC) with one
    /// write.
    pub fn append(&self, entries: &[JournalEntry], now_ms: i64) -> io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        append_line(
            &self.home.journal_dir().join(event_file_name(now_ms)),
            &lines_text(entries, now_ms),
        )
    }
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        // Closing the handle releases the lock too; unlocking first frees
        // the next writer without waiting for the close.
        let _ = self.file.unlock();
    }
}

fn lines_text(entries: &[JournalEntry], now_ms: i64) -> String {
    entries
        .iter()
        .map(|entry| JournalLine::new(now_ms, entry.clone()).to_line())
        .collect()
}

/// Takes the lock, appends `entries` with one write and releases it. With
/// nothing to write it takes no lock and touches no file: the gate calls it
/// on every gated tool call and locks only to record something (AD-28).
/// For a line written before acting, so that a writer that cannot lock does
/// not act; what already happened goes through [`record`].
pub fn append(home: &RelayHome, entries: &[JournalEntry], now_ms: i64) -> Result<(), JournalError> {
    if entries.is_empty() {
        return Ok(());
    }
    JournalLock::acquire(home)?.append(entries, now_ms)?;
    Ok(())
}

/// Journals `entries` that record something already done or decided (a
/// gate deny, a refusal, a run's end), which a held lock must not lose
/// (D-760): [`append`] first; when the lock stays held past
/// [`LOCK_TIMEOUT`] or the locked write fails, one write to a spill file of
/// their own beside the day file, created new so it has a single writer and
/// needs no lock. Fails only when the spill fails too.
pub fn record(home: &RelayHome, entries: &[JournalEntry], now_ms: i64) -> io::Result<()> {
    match append(home, entries, now_ms) {
        Ok(()) => Ok(()),
        Err(_) => spill(home, entries, now_ms),
    }
}

/// The hook's `PermissionDenied` path (AD-181, story 5.23): one
/// `auto_mode_denied` line, recorded as the gate records its denies, since
/// auto mode already decided (D-760), its reason cut to the rule name when
/// excerpts are off (D-911); nothing for a payload without a session id.
/// Prints nothing: Officina never asks for a retry.
pub fn record_denial(
    home: &RelayHome,
    payload: &serde_json::Value,
    with_excerpts: bool,
    now_ms: i64,
) -> io::Result<()> {
    let Some(denied) = AutoModeDenied::from_payload(payload) else {
        return Ok(());
    };
    let denied = if with_excerpts {
        denied
    } else {
        denied.without_excerpts()
    };
    record(home, &[JournalEntry::AutoModeDenied(denied)], now_ms)
}

fn spill(home: &RelayHome, entries: &[JournalEntry], now_ms: i64) -> io::Result<()> {
    let dir = home.journal_dir();
    fs::create_dir_all(&dir)?;
    let text = lines_text(entries, now_ms);
    let pid = std::process::id();
    let mut attempt = 0u32;
    loop {
        let name = spill_file_name(now_ms, &format!("{now_ms}-{pid}-{attempt}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(name))
        {
            Ok(mut file) => return file.write_all(text.as_bytes()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => attempt += 1,
            Err(e) => return Err(e),
        }
    }
}

/// The window's retention pass over the journal (AD-32): deletes the day
/// files and spill files older than [`JOURNAL_RETENTION_DAYS`] days, never
/// the lock file or any other name. Returns how many files went. Only the
/// window calls it: `officina run` and the gate never delete.
///
/// [`JOURNAL_RETENTION_DAYS`]: officina_core::relay::journal::JOURNAL_RETENTION_DAYS
pub fn prune(home: &RelayHome, now_ms: i64) -> usize {
    let Ok(entries) = fs::read_dir(home.journal_dir()) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| is_expired_journal_file(&entry.file_name().to_string_lossy(), now_ms))
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| fs::remove_file(entry.path()).is_ok())
        .count()
}

/// The journal lines of the last two UTC days, plus the number of lines
/// that could not be read (AD-10, AD-32): each day's file in file order,
/// then that day's spill files (D-760) in name order, which starts with
/// the millisecond they were written in. A partial last line (a writer killed mid-write), a
/// line of another version and a line of a `kind` this version does not
/// know are skipped without being counted.
pub fn read_recent(home: &RelayHome, now_ms: i64) -> (Vec<JournalLine>, u32) {
    read_named(home, &event_files_to_read(now_ms))
}

/// The journal lines of the last `days` UTC days, today included, read as
/// [`read_recent`] reads its two: what the Relay view's decisions per day
/// count (AD-92).
pub fn read_days(home: &RelayHome, now_ms: i64, days: i64) -> (Vec<JournalLine>, u32) {
    let names: Vec<String> = (0..days)
        .rev()
        .map(|back| event_file_name(now_ms - back * DAY_MS))
        .collect();
    read_named(home, &names)
}

/// The lines of the day files `day_files`, each followed by its spill
/// files, with the count of lines that could not be read.
fn read_named(home: &RelayHome, day_files: &[String]) -> (Vec<JournalLine>, u32) {
    let dir = home.journal_dir();
    let mut spills: Vec<(String, String)> = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    spilled_day_file(&name).map(|day_file| (day_file, name))
                })
                .collect()
        })
        .unwrap_or_default();
    spills.sort();
    let mut lines = Vec::new();
    let mut corrupt = 0u32;
    for day_file in day_files {
        let of_the_day = spills
            .iter()
            .filter(|(day, _)| day == day_file)
            .map(|(_, name)| name.as_str());
        for name in std::iter::once(day_file.as_str()).chain(of_the_day) {
            read_file(&dir.join(name), &mut lines, &mut corrupt);
        }
    }
    (lines, corrupt)
}

fn read_file(path: &std::path::Path, lines: &mut Vec<JournalLine>, corrupt: &mut u32) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return,
        Err(_) => {
            *corrupt += 1;
            return;
        }
    };
    let ends_with_newline = bytes.ends_with(b"\n");
    let raw_lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
    let last_index = raw_lines.len().saturating_sub(1);
    for (index, raw) in raw_lines.iter().enumerate() {
        let partial_tail = index == last_index && !ends_with_newline;
        let parsed = match std::str::from_utf8(raw) {
            Ok(text) if text.trim().is_empty() => continue,
            Ok(text) => parse_line(text.trim_end_matches('\r')),
            Err(_) => ParsedLine::Corrupt,
        };
        match parsed {
            ParsedLine::Line(line) => lines.push(line),
            ParsedLine::Foreign => {}
            ParsedLine::Corrupt if partial_tail => {}
            ParsedLine::Corrupt => *corrupt += 1,
        }
    }
}
