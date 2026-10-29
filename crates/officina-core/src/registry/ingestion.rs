//! How event lines enter the registry (AD-43, D-49, D-84, D-97): where a
//! pass starts reading a file and what it does once it reached the end
//! ([`open_event_file`], [`close_event_file`]), which lines are left out,
//! and what a line adds to its session's excerpts. The host reads the files
//! and writes the rows.

use serde::{Deserialize, Serialize};

use crate::dates::{DAY_MS, dated_file_day, day_start_ms, utc_date};
use crate::events::{EXCERPT_CHARS, EventLine, MAX_LINE_BYTES, excerpt, tail_excerpt};
use crate::quota::normalise_path;

use super::types::EventFileRecord;

/// A chunk, one transaction, holds at most this many lines (AD-43).
pub const CHUNK_MAX_LINES: usize = 2000;

/// A chunk holds at most this many bytes (AD-43).
pub const CHUNK_MAX_BYTES: usize = 1 << 20;

/// How long after the end of its UTC day a file may still grow: the hook
/// takes `ts` on receipt and appends within its 5 s timeout (AD-16), so a
/// line dated 23:59:59 can land after midnight (D-49).
pub const COMPLETE_GRACE_MS: i64 = 60_000;

/// The folder, under the Officina home, where the triage call runs (AD-51);
/// lines from it are never ingested (AD-43).
pub const TRIAGE_DIR: &str = "triage";

/// True when the event file `name`, read to its end at `now_ms`, is
/// complete: dated before today and past the grace after the end of its
/// day (D-49). A name that is no event file never is.
///
/// ```
/// use officina_core::registry::ingestion::complete_once_read;
/// let now = 1_793_102_400_000; // 2026-10-27 12:00
/// assert!(complete_once_read("2026-10-26.jsonl", now));
/// assert!(!complete_once_read("2026-10-27.jsonl", now));
/// assert!(!complete_once_read("2026-10-26.jsonl", 1_793_059_200_000 + 59_999));
/// ```
pub fn complete_once_read(name: &str, now_ms: i64) -> bool {
    dated_file_day(name).is_some_and(|day| now_ms >= day_start_ms(day) + DAY_MS + COMPLETE_GRACE_MS)
}

/// Where a pass starts on an event file whose stored row is `row` and whose
/// length is now `file_len` (AD-43, D-84).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum FileOpening {
    /// Found complete before: not read again.
    Complete,
    /// Read from `row`'s offset, as `row` now says. `rewritten` when the
    /// file was shorter than its offset: its offset and corrupt count
    /// restart from zero and its rewrites count one more (D-84).
    Read {
        row: EventFileRecord,
        rewritten: bool,
    },
}

/// How a pass opens an event file (AD-43, D-84).
///
/// ```
/// use officina_core::registry::ingestion::{FileOpening, open_event_file};
/// use officina_core::registry::types::EventFileRecord;
/// let row = EventFileRecord { name: "2026-10-27.jsonl".into(), ingested_bytes: 100, ..Default::default() };
/// let FileOpening::Read { row: shorter, rewritten } = open_event_file(&row, 40) else { panic!() };
/// assert!(rewritten);
/// assert_eq!((shorter.ingested_bytes, shorter.rewrites), (0, 1));
/// ```
pub fn open_event_file(row: &EventFileRecord, file_len: u64) -> FileOpening {
    if row.complete_at_ms.is_some() {
        return FileOpening::Complete;
    }
    let mut row = row.clone();
    let rewritten = file_len < row.ingested_bytes;
    if rewritten {
        row.ingested_bytes = 0;
        row.corrupt_lines = 0;
        row.rewrites += 1;
    }
    FileOpening::Read { row, rewritten }
}

/// What a pass does once it read every complete line of an event file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileClosing {
    /// The unfinished last line counts as one corrupt line and is read
    /// past (AD-43): the file is dated before yesterday.
    pub tail_corrupt: bool,
    /// The file is complete: fully read, dated before today and past the
    /// grace (D-49).
    pub complete: bool,
}

/// How a pass closes the event file `name`, read to its last newline with
/// `partial_bytes` after it, at `now_ms` (AD-43, D-49): an unfinished line
/// stays unread, unless the file is dated before yesterday; a file is
/// complete once nothing is left unread and its grace has passed.
///
/// ```
/// use officina_core::registry::ingestion::close_event_file;
/// let now = 1_793_102_400_000; // 2026-10-27 12:00
/// assert!(close_event_file("2026-10-25.jsonl", 7, now).tail_corrupt);
/// assert!(!close_event_file("2026-10-26.jsonl", 7, now).complete, "a line may still end");
/// assert!(close_event_file("2026-10-26.jsonl", 0, now).complete);
/// ```
pub fn close_event_file(name: &str, partial_bytes: u64, now_ms: i64) -> FileClosing {
    let tail_corrupt = partial_bytes > 0 && partial_tail_is_corrupt(name, now_ms);
    let fully_read = partial_bytes == 0 || tail_corrupt;
    FileClosing {
        tail_corrupt,
        complete: fully_read && complete_once_read(name, now_ms),
    }
}

/// True when an unfinished last line of the event file `name` is one
/// corrupt line rather than a line still being written: the file is dated
/// before yesterday (AD-43).
pub fn partial_tail_is_corrupt(name: &str, now_ms: i64) -> bool {
    dated_file_day(name).is_some_and(|day| day < utc_date(now_ms - DAY_MS))
}

/// The time a line of the event file `name` is recorded at, at `now_ms`:
/// its `ts`, held within the file's UTC day (up to [`COMPLETE_GRACE_MS`]
/// after its end, D-49) and never past `now_ms`. Excerpt lifetimes and the
/// "newer wins" rule count from it (D-50), so a line dated in the future (a
/// wrong clock, a forged line) can neither outlive the privacy bound nor
/// freeze newer observations; a line dated before its file's day cannot
/// pass for older than the file. Unchanged for a name that is no event
/// file.
///
/// ```
/// use officina_core::registry::ingestion::recorded_at;
/// let now = 1_793_102_400_000; // 2026-10-27 12:00
/// assert_eq!(recorded_at(4_102_444_800_000, "2026-10-27.jsonl", now), now);
/// assert_eq!(recorded_at(now - 1, "2026-10-27.jsonl", now), now - 1);
/// assert_eq!(recorded_at(0, "2026-10-27.jsonl", now), 1_793_059_200_000);
/// ```
pub fn recorded_at(ts: i64, name: &str, now_ms: i64) -> i64 {
    let Some(day) = dated_file_day(name) else {
        return ts.min(now_ms);
    };
    let start = day_start_ms(day);
    let end = (start + DAY_MS + COMPLETE_GRACE_MS).min(now_ms);
    ts.min(end).max(start.min(end))
}

/// True when a line of `next_bytes` must open a new chunk rather than join
/// one holding `lines` lines and `bytes` bytes (AD-43): a chunk holds at
/// most [`CHUNK_MAX_LINES`] lines and [`CHUNK_MAX_BYTES`] bytes, and a
/// longer line is a chunk of its own.
///
/// ```
/// use officina_core::registry::ingestion::{CHUNK_MAX_BYTES, CHUNK_MAX_LINES, starts_a_chunk};
/// assert!(!starts_a_chunk(0, 0, CHUNK_MAX_BYTES + 1), "an empty chunk takes any line");
/// assert!(starts_a_chunk(CHUNK_MAX_LINES, 10, 1));
/// assert!(starts_a_chunk(1, CHUNK_MAX_BYTES, 1));
/// assert!(!starts_a_chunk(1, 10, 1));
/// ```
pub fn starts_a_chunk(lines: usize, bytes: usize, next_bytes: usize) -> bool {
    lines > 0 && (lines >= CHUNK_MAX_LINES || bytes + next_bytes > CHUNK_MAX_BYTES)
}

/// True when `cwd` lies in `<officina_home>/triage/`, compared as
/// [`normalise_path`] does, so case and slashes do not matter (D-97).
///
/// ```
/// use officina_core::registry::ingestion::in_triage_workdir;
/// let home = r"C:\Users\user01\.officina";
/// assert!(in_triage_workdir(r"c:/users/USER01/.officina/triage/empty", home));
/// assert!(!in_triage_workdir(r"C:\Users\user01\.officina\triage-notes", home));
/// ```
pub fn in_triage_workdir(cwd: &str, officina_home: &str) -> bool {
    let workdir = format!("{}/{TRIAGE_DIR}", normalise_path(officina_home));
    let cwd = normalise_path(cwd);
    cwd == workdir
        || cwd
            .strip_prefix(&workdir)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// True when `cwd` is exactly `<officina_home>/review/empty`, the review
/// call's folder, compared as [`normalise_path`] does (AD-43 as amended,
/// D-166): a session working elsewhere under `review/` is recorded like any
/// other.
///
/// ```
/// use officina_core::registry::ingestion::in_review_workdir;
/// let home = r"C:\Users\user01\.officina";
/// assert!(in_review_workdir(r"c:/users/USER01/.officina/review/empty", home));
/// assert!(!in_review_workdir(r"C:\Users\user01\.officina\review\notes", home));
/// ```
pub fn in_review_workdir(cwd: &str, officina_home: &str) -> bool {
    in_officina_dir(cwd, officina_home, crate::spawn::OfficinaDir::ReviewWorkdir)
}

/// True when `cwd` is exactly `<officina_home>/orchestrator/workdir`, the
/// orchestrator's folder, compared as [`normalise_path`] does (AD-43 as
/// amended, D-166): a session working elsewhere under `orchestrator/` is
/// recorded like any other.
///
/// ```
/// use officina_core::registry::ingestion::in_orchestrator_workdir;
/// let home = r"C:\Users\user01\.officina";
/// assert!(in_orchestrator_workdir(r"c:/users/USER01/.officina/orchestrator/workdir", home));
/// assert!(!in_orchestrator_workdir(r"C:\Users\user01\.officina\orchestrator\notes", home));
/// ```
pub fn in_orchestrator_workdir(cwd: &str, officina_home: &str) -> bool {
    in_officina_dir(
        cwd,
        officina_home,
        crate::spawn::OfficinaDir::OrchestratorWorkdir,
    )
}

/// True when `cwd` is exactly `dir` under `officina_home`, both
/// normalised.
pub(crate) fn in_officina_dir(
    cwd: &str,
    officina_home: &str,
    dir: crate::spawn::OfficinaDir,
) -> bool {
    let segments = dir.segments().join("/");
    normalise_path(cwd) == format!("{}/{segments}", normalise_path(officina_home))
}

/// What one event line adds to its session's excerpts (AD-40, FR-4): the
/// latest prompt, or the head and tail of the latest answer, each kept
/// only when it is newer than the one stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcerptObservation {
    Prompt { text: String },
    Answer { head: String, tail: Option<String> },
}

/// The excerpt `line` carries, if any, held to the bounds the hook writes
/// with ([`EXCERPT_CHARS`], AD-45): a line written by anything else cannot
/// make the registry, the view or a triage call carry more. Text the hook
/// wrote is unchanged.
///
/// ```
/// use officina_core::events::{EXCERPT_CHARS, EventLine};
/// use officina_core::registry::ingestion::{ExcerptObservation, excerpt_observation};
/// let line: EventLine = serde_json::from_value(serde_json::json!({
///     "v": 1, "ts": 5, "event": "UserPromptSubmit", "prompt": "x".repeat(10 * EXCERPT_CHARS),
/// })).unwrap();
/// let Some(ExcerptObservation::Prompt { text }) = excerpt_observation(&line) else { panic!() };
/// assert_eq!(text.chars().count(), EXCERPT_CHARS + 1);
/// ```
pub fn excerpt_observation(line: &EventLine) -> Option<ExcerptObservation> {
    match line.event.as_str() {
        "UserPromptSubmit" => line
            .prompt
            .as_deref()
            .map(|text| ExcerptObservation::Prompt {
                text: excerpt(text, EXCERPT_CHARS),
            }),
        "Stop" => line
            .last_message
            .as_deref()
            .map(|head| ExcerptObservation::Answer {
                head: excerpt(head, EXCERPT_CHARS),
                tail: line.last_message_tail.as_deref().map(|tail| {
                    tail_excerpt(tail, EXCERPT_CHARS).unwrap_or_else(|| tail.to_owned())
                }),
            }),
        _ => None,
    }
}

/// True when a raw line of an event file, its newline included, is longer
/// than the hook ever writes ([`MAX_LINE_BYTES`], one byte allowed for a
/// carriage return): it is counted corrupt and never parsed, so nothing else that
/// writes to the events folder sets the size of a session id, a folder or
/// a reason the registry stores.
///
/// ```
/// use officina_core::events::MAX_LINE_BYTES;
/// use officina_core::registry::ingestion::line_too_long;
/// assert!(!line_too_long(MAX_LINE_BYTES + 1));
/// assert!(line_too_long(MAX_LINE_BYTES + 2));
/// ```
pub fn line_too_long(raw_len: usize) -> bool {
    raw_len > MAX_LINE_BYTES + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_102_400_000;

    #[test]
    fn completion_waits_for_the_grace_after_the_day() {
        let end_of_yesterday = day_start_ms(utc_date(NOW));
        assert!(!complete_once_read("2026-10-26.jsonl", end_of_yesterday));
        assert!(!complete_once_read(
            "2026-10-26.jsonl",
            end_of_yesterday + COMPLETE_GRACE_MS - 1
        ));
        assert!(complete_once_read(
            "2026-10-26.jsonl",
            end_of_yesterday + COMPLETE_GRACE_MS
        ));
        assert!(!complete_once_read("notes.txt", NOW));
    }

    #[test]
    fn a_partial_tail_is_corrupt_before_yesterday_only() {
        assert!(partial_tail_is_corrupt("2026-10-25.jsonl", NOW));
        assert!(!partial_tail_is_corrupt("2026-10-26.jsonl", NOW));
        assert!(!partial_tail_is_corrupt("2026-10-27.jsonl", NOW));
    }

    /// Security audit M1: a line's time stays within its file's day and
    /// never passes now, whatever it says.
    #[test]
    fn a_line_is_recorded_within_its_files_day_and_never_after_now() {
        let today = day_start_ms(utc_date(NOW));
        let yesterday = today - DAY_MS;
        for (ts, name, expected) in [
            (i64::MAX, "2026-10-27.jsonl", NOW),
            (NOW + DAY_MS, "2026-10-27.jsonl", NOW),
            (NOW - 5, "2026-10-27.jsonl", NOW - 5),
            (i64::MIN, "2026-10-27.jsonl", today),
            (today - 1, "2026-10-27.jsonl", today),
            (NOW, "2026-10-26.jsonl", today + COMPLETE_GRACE_MS),
            (yesterday + 10, "2026-10-26.jsonl", yesterday + 10),
            (NOW + 1, "notes.jsonl", NOW),
        ] {
            assert_eq!(recorded_at(ts, name, NOW), expected, "{ts} in {name}");
        }
        // A file dated in the future records nothing after now either.
        assert_eq!(recorded_at(NOW + 3 * DAY_MS, "2026-10-30.jsonl", NOW), NOW);
    }

    #[test]
    fn the_triage_workdir_matches_whatever_the_spelling() {
        let home = r"C:\Users\user01\.officina";
        for cwd in [
            r"C:\Users\user01\.officina\triage",
            r"C:\Users\user01\.officina\triage\empty",
            "C:/Users/user01/.officina/triage/empty/",
            r"c:\USERS\user01\.OFFICINA\Triage\Empty",
        ] {
            assert!(in_triage_workdir(cwd, home), "{cwd}");
        }
        for cwd in [
            r"C:\Users\user01\.officina",
            r"C:\Users\user01\.officina\triagex",
            r"C:\work\triage",
        ] {
            assert!(!in_triage_workdir(cwd, home), "{cwd}");
        }
    }
}
