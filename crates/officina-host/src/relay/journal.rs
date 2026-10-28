//! The journal's files (AD-32): locked appends and the two-day reader.
//!
//! Every writer (`officina run`, the gate, the window) takes an exclusive
//! lock on `journal/.lock` with `File::try_lock`, retried every
//! [`LOCK_RETRY`] up to [`LOCK_TIMEOUT`], writes its lines with one
//! `append_line`, then releases it. A separate lock file means readers never
//! meet a locked range, and lines of any length never interleave. The
//! window alone deletes old day files ([`prune`]).

use std::fs::{self, File, OpenOptions};
use std::io;
use std::time::{Duration, Instant};

use officina_core::dates::{event_file_name, event_files_to_read};
use officina_core::relay::journal::{
    JournalEntry, JournalLine, ParsedLine, is_expired_journal_file, parse_line,
};

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
        let text: String = entries
            .iter()
            .map(|entry| JournalLine::new(now_ms, entry.clone()).to_line())
            .collect();
        append_line(
            &self.home.journal_dir().join(event_file_name(now_ms)),
            &text,
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

/// Takes the lock, appends `entries` with one write and releases it. With
/// nothing to write it takes no lock and touches no file: the gate calls it
/// on every gated tool call and locks only to record something (AD-28).
pub fn append(home: &RelayHome, entries: &[JournalEntry], now_ms: i64) -> Result<(), JournalError> {
    if entries.is_empty() {
        return Ok(());
    }
    JournalLock::acquire(home)?.append(entries, now_ms)?;
    Ok(())
}

/// The window's retention pass over the journal (AD-32): deletes the day
/// files older than [`JOURNAL_RETENTION_DAYS`] days, never the lock file or
/// a name that is not a day file. Returns how many files went. Only the
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

/// The journal lines of the last two UTC days, in file order, plus the
/// number of lines that could not be read (AD-10, AD-32). A partial last
/// line (a writer killed mid-write), a line of another version and a line of
/// a `kind` this version does not know are skipped without being counted.
pub fn read_recent(home: &RelayHome, now_ms: i64) -> (Vec<JournalLine>, u32) {
    let mut lines = Vec::new();
    let mut corrupt = 0u32;
    for name in event_files_to_read(now_ms) {
        let bytes = match fs::read(home.journal_dir().join(&name)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                corrupt += 1;
                continue;
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
                ParsedLine::Corrupt => corrupt += 1,
            }
        }
    }
    (lines, corrupt)
}
