//! How event lines enter the registry (AD-43, D-49, D-84, D-97): when a
//! file is complete, when its unfinished last line counts as corrupt, which
//! lines are left out, and what a line adds to its session's excerpts. The
//! host reads the files and writes the rows.

use crate::dates::{dated_file_day, day_start_ms, utc_date};
use crate::events::EventLine;
use crate::quota::normalise_path;

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

const DAY_MS: i64 = 86_400_000;

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

/// What one event line adds to its session's excerpts (AD-40, FR-4): the
/// latest prompt, or the head and tail of the latest answer, each kept
/// only when it is newer than the one stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcerptObservation {
    Prompt { text: String },
    Answer { head: String, tail: Option<String> },
}

/// The excerpt `line` carries, if any.
pub fn excerpt_observation(line: &EventLine) -> Option<ExcerptObservation> {
    match line.event.as_str() {
        "UserPromptSubmit" => line
            .prompt
            .clone()
            .map(|text| ExcerptObservation::Prompt { text }),
        "Stop" => line
            .last_message
            .clone()
            .map(|head| ExcerptObservation::Answer {
                head,
                tail: line.last_message_tail.clone(),
            }),
        _ => None,
    }
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
