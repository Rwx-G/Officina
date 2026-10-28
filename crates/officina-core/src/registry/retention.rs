//! What the registry keeps, and for how long (AD-42, AD-44, D-48, D-85,
//! D-94). The host lists the files and deletes; these rules decide.

use std::collections::BTreeSet;

use crate::dates::{day_start_ms, is_expired_event_file};

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// No excerpt survives this long, in the registry or in a copy of it
/// (FR-4 as amended).
pub const PRIVACY_BOUND_MS: i64 = 30 * DAY_MS;

/// The hourly pass's interval (AD-44): an expiry is applied at most this
/// late.
pub const PASS_INTERVAL_MS: i64 = HOUR_MS;

/// How long a daily copy, a quarantined database or a pre-migration copy is
/// kept, from the start of the UTC day in its name (AD-44, D-94).
pub const COPY_LIFETIME_MS: i64 = 2 * DAY_MS;

/// How long an excerpt is kept after its event time (D-85): what the
/// privacy bound leaves once a copy's lifetime and one pass interval are
/// taken off, since an excerpt can sit in a copy that long after its expiry.
pub const EXCERPT_TTL_MS: i64 = PRIVACY_BOUND_MS - COPY_LIFETIME_MS - PASS_INTERVAL_MS;

/// How many daily copies are kept: the newest valid ones, so a damaged copy
/// never pushes out a valid one (D-94); also how many quarantined databases
/// and pre-migration copies are kept.
pub const COPIES_KEPT: usize = 2;

/// True when an excerpt recorded for an event at `event_ms` has expired at
/// `now_ms` (D-50: counted from the event, never from ingestion).
pub fn excerpt_expired(event_ms: i64, now_ms: i64) -> bool {
    now_ms - event_ms >= EXCERPT_TTL_MS
}

/// The latest event time an excerpt may have to be cleared at `now_ms`.
pub fn excerpt_cutoff(now_ms: i64) -> i64 {
    now_ms - EXCERPT_TTL_MS
}

/// True when a copy dated `day` is past its lifetime at `now_ms` (D-94).
pub fn copy_expired(day: (i64, u32, u32), now_ms: i64) -> bool {
    now_ms >= day_start_ms(day) + COPY_LIFETIME_MS
}

/// Which of `days`, newest first, are kept: the first [`COPIES_KEPT`]
/// that are past nothing. The rest go.
pub fn kept(days: &[(i64, u32, u32)], now_ms: i64) -> Vec<bool> {
    let mut kept_so_far = 0;
    days.iter()
        .map(|day| {
            let keep = kept_so_far < COPIES_KEPT && !copy_expired(*day, now_ms);
            kept_so_far += usize::from(keep);
            keep
        })
        .collect()
}

/// True when the event file `name` may be deleted at `now_ms` (AD-44 as
/// amended by D-48): it is dated before yesterday, and each of the kept
/// daily copies, valid and as many as [`COPIES_KEPT`], holds that file's
/// row complete. `copies` gives, per kept valid copy, the names of the
/// files it holds complete.
///
/// ```
/// use std::collections::BTreeSet;
/// use officina_core::registry::retention::event_file_deletable;
/// let now = 1_793_102_400_000; // 2026-10-27
/// let holds = BTreeSet::from(["2026-10-25.jsonl".to_owned()]);
/// assert!(event_file_deletable("2026-10-25.jsonl", now, &[holds.clone(), holds.clone()]));
/// assert!(!event_file_deletable("2026-10-25.jsonl", now, &[holds]));
/// ```
pub fn event_file_deletable(name: &str, now_ms: i64, copies: &[BTreeSet<String>]) -> bool {
    is_expired_event_file(name, now_ms)
        && copies.len() >= COPIES_KEPT
        && copies.iter().all(|complete| complete.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::utc_date;

    const NOW: i64 = 1_793_102_400_000;

    /// D-85: an excerpt is cleared at most one pass after its TTL, and a
    /// copy taken just before holds it at most a copy lifetime longer.
    #[test]
    fn ttl_copy_lifetime_and_pass_interval_stay_within_30_days() {
        const { assert!(EXCERPT_TTL_MS + COPY_LIFETIME_MS + PASS_INTERVAL_MS <= PRIVACY_BOUND_MS) };
        assert_eq!(
            EXCERPT_TTL_MS,
            28 * DAY_MS - HOUR_MS,
            "AD-44's 28 days, less one hour"
        );
        assert!(!excerpt_expired(NOW - EXCERPT_TTL_MS + 1, NOW));
        assert!(excerpt_expired(NOW - EXCERPT_TTL_MS, NOW));
        assert!(excerpt_expired(excerpt_cutoff(NOW), NOW));
    }

    #[test]
    fn event_file_is_deletable_only_when_every_kept_copy_holds_it_complete() {
        let name = "2026-10-25.jsonl";
        let holds = || BTreeSet::from([name.to_owned()]);
        let lacks = || BTreeSet::from(["2026-10-24.jsonl".to_owned()]);
        assert!(event_file_deletable(name, NOW, &[holds(), holds()]));
        assert!(!event_file_deletable(name, NOW, &[]), "no copy at all");
        assert!(
            !event_file_deletable(name, NOW, &[holds()]),
            "one valid copy"
        );
        assert!(!event_file_deletable(name, NOW, &[holds(), lacks()]));
        assert!(!event_file_deletable(name, NOW, &[lacks(), holds()]));
        let yesterday = "2026-10-26.jsonl";
        let both = || BTreeSet::from([yesterday.to_owned()]);
        assert!(
            !event_file_deletable(yesterday, NOW, &[both(), both()]),
            "yesterday's file stays for the board"
        );
        let stray = || BTreeSet::from(["notes.txt".to_owned()]);
        assert!(!event_file_deletable("notes.txt", NOW, &[stray(), stray()]));
    }

    #[test]
    fn copies_are_kept_two_and_for_two_days() {
        let today = utc_date(NOW);
        let yesterday = utc_date(NOW - DAY_MS);
        let before = utc_date(NOW - 2 * DAY_MS);
        assert_eq!(kept(&[today, yesterday, before], NOW), [true, true, false]);
        assert_eq!(
            kept(&[yesterday, before], NOW),
            [true, false],
            "past its lifetime"
        );
        assert_eq!(kept(&[today, today, yesterday], NOW), [true, true, false]);
        assert!(!copy_expired(yesterday, NOW));
        assert!(copy_expired(yesterday, day_start_ms(today) + DAY_MS));
    }
}
