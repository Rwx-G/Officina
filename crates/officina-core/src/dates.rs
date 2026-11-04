//! UTC civil dates for event file names (AD-9). Local dates would need a
//! timezone database; the 24 h reading window is exact with UTC files too.

/// One hour, in milliseconds: the one copy every module uses.
pub const HOUR_MS: i64 = 3_600_000;

/// One day, in milliseconds: the one copy every module uses.
pub const DAY_MS: i64 = 24 * HOUR_MS;

/// Milliseconds from 1601-01-01, where Windows counts a process's creation
/// time, to 1970-01-01.
const FILETIME_EPOCH_MS: i64 = 11_644_473_600_000;

/// A Windows creation time, in 100 ns ticks since 1601, as Unix
/// milliseconds: how `orchestrator_start.pid_created_at_ms` stores it
/// (D-144), so a reused pid's later creation never compares equal.
///
/// ```
/// use officina_core::dates::filetime_ms;
/// assert_eq!(filetime_ms(116_444_736_000_000_000), 0);
/// assert_eq!(filetime_ms(116_444_736_000_019_999), 1);
/// ```
pub fn filetime_ms(ticks: u64) -> i64 {
    i64::try_from(ticks / 10_000).unwrap_or(i64::MAX) - FILETIME_EPOCH_MS
}

/// `(year, month, day)` of the UTC day containing `unix_ms`.
///
/// Howard Hinnant's `civil_from_days`, valid for the whole `i64` day range
/// Officina can meet.
pub fn utc_date(unix_ms: i64) -> (i64, u32, u32) {
    let days = unix_ms.div_euclid(DAY_MS);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The UTC day containing `unix_ms`, written `YYYY-MM-DD`.
///
/// ```
/// assert_eq!(officina_core::dates::utc_day_text(0), "1970-01-01");
/// ```
pub fn utc_day_text(unix_ms: i64) -> String {
    let (y, m, d) = utc_date(unix_ms);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The day a `YYYY-MM-DD` text stands for; `None` for any other text.
///
/// ```
/// use officina_core::dates::parse_utc_day;
/// assert_eq!(parse_utc_day("2026-10-27"), Some((2026, 10, 27)));
/// assert_eq!(parse_utc_day("2026-13-01"), None);
/// assert_eq!(parse_utc_day("26-10-27"), None);
/// ```
pub fn parse_utc_day(text: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return None;
    };
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i64>(), m.parse::<u32>(), d.parse::<u32>()) else {
        return None;
    };
    if y.to_string().len() != 4 || m == 0 || m > 12 || d == 0 || d > 31 {
        return None;
    }
    Some((y, m, d))
}

/// Unix milliseconds at which the UTC day `day` begins.
///
/// ```
/// use officina_core::dates::{day_start_ms, utc_date};
/// assert_eq!(day_start_ms((1970, 1, 2)), 86_400_000);
/// let now = 1_793_102_400_000;
/// assert_eq!(utc_date(day_start_ms(utc_date(now))), utc_date(now));
/// ```
pub fn day_start_ms(day: (i64, u32, u32)) -> i64 {
    // Howard Hinnant's `days_from_civil`, the inverse of `utc_date`.
    let (y, m, d) = day;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * DAY_MS
}

/// Unix milliseconds at which the UTC ISO week holding `unix_ms` begins:
/// its Monday at 00:00 UTC (the Workshop share's week, D-214).
///
/// ```
/// use officina_core::dates::{iso_week_start_ms, day_start_ms};
/// // 2026-11-04 is a Wednesday; its week begins on Monday 2026-11-02.
/// let wednesday = day_start_ms((2026, 11, 4)) + 5;
/// assert_eq!(iso_week_start_ms(wednesday), day_start_ms((2026, 11, 2)));
/// ```
pub fn iso_week_start_ms(unix_ms: i64) -> i64 {
    let days = unix_ms.div_euclid(DAY_MS);
    // 1970-01-01 was a Thursday, the fourth day of an ISO week.
    let weekday = (days + 3).rem_euclid(7);
    (days - weekday) * DAY_MS
}

/// The UTC ISO week holding `unix_ms`, written `YYYY-Www`: the year is the
/// one its Thursday falls in, as ISO 8601 numbers weeks.
///
/// ```
/// use officina_core::dates::{iso_week_text, day_start_ms};
/// assert_eq!(iso_week_text(day_start_ms((2026, 11, 4))), "2026-W45");
/// // 2027-01-01 is a Friday: its week is the last of 2026.
/// assert_eq!(iso_week_text(day_start_ms((2027, 1, 1))), "2026-W53");
/// assert_eq!(iso_week_text(day_start_ms((2024, 12, 30))), "2025-W01");
/// ```
pub fn iso_week_text(unix_ms: i64) -> String {
    let thursday = iso_week_start_ms(unix_ms) + 3 * DAY_MS;
    let (year, _, _) = utc_date(thursday);
    let week = (thursday - day_start_ms((year, 1, 1))) / DAY_MS / 7 + 1;
    format!("{year:04}-W{week:02}")
}

/// An RFC 3339 time as the engine prints it (`2026-11-02T10:00:00.5Z`,
/// `...+01:00`) in Unix milliseconds; fractions past milliseconds are cut.
///
/// ```
/// use officina_core::dates::parse_rfc3339_ms;
/// assert_eq!(parse_rfc3339_ms("1970-01-02T00:00:01.25Z"), Some(86_401_250));
/// assert_eq!(parse_rfc3339_ms("1970-01-01T01:00:00+01:00"), Some(0));
/// assert_eq!(parse_rfc3339_ms("yesterday"), None);
/// ```
pub fn parse_rfc3339_ms(text: &str) -> Option<i64> {
    let (date, time) = text.split_once(['T', 't'])?;
    let day = parse_utc_day(date)?;
    let (clock, offset_ms) = if let Some(clock) = time.strip_suffix(['Z', 'z']) {
        (clock, 0)
    } else {
        let at = time.rfind(['+', '-'])?;
        let (clock, offset) = time.split_at(at);
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let (h, m) = offset.get(1..)?.split_once(':')?;
        let minutes = h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?;
        (clock, sign * minutes * 60_000)
    };
    let (whole, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut parts = whole.split(':').map(|p| p.parse::<i64>().ok());
    let (h, m, s) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || h > 23 || m > 59 || s > 60 {
        return None;
    }
    if !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let millis = format!("{:0<3}", fraction.get(..3).unwrap_or(fraction))
        .parse::<i64>()
        .ok()?;
    Some(day_start_ms(day) + ((h * 60 + m) * 60 + s) * 1000 + millis - offset_ms)
}

/// Event file name for the UTC day containing `unix_ms`: `YYYY-MM-DD.jsonl`.
///
/// ```
/// assert_eq!(officina_core::dates::event_file_name(0), "1970-01-01.jsonl");
/// ```
pub fn event_file_name(unix_ms: i64) -> String {
    format!("{}.jsonl", utc_day_text(unix_ms))
}

/// The event files a reader needs at `now_ms`: yesterday's and today's, in
/// that order, so the last 24 h are always covered.
pub fn event_files_to_read(now_ms: i64) -> [String; 2] {
    [event_file_name(now_ms - DAY_MS), event_file_name(now_ms)]
}

/// The UTC day a `YYYY-MM-DD.jsonl` file name stands for; `None` for any
/// other name.
pub fn dated_file_day(name: &str) -> Option<(i64, u32, u32)> {
    parse_utc_day(name.strip_suffix(".jsonl")?)
}

/// True when a `YYYY-MM-DD.jsonl` file is dated before the UTC day `days`
/// days back from `now_ms`, so it holds no line of the last `days` days.
/// Names that are not dated files are never eligible, so a stray file in
/// the directory is left alone.
///
/// ```
/// use officina_core::dates::is_dated_file_older_than;
/// let now = 1_793_102_400_000; // 2026-10-27
/// assert!(is_dated_file_older_than("2026-10-25.jsonl", now, 1));
/// assert!(!is_dated_file_older_than("2026-10-26.jsonl", now, 1));
/// assert!(!is_dated_file_older_than(".lock", now, 1));
/// ```
pub fn is_dated_file_older_than(name: &str, now_ms: i64, days: i64) -> bool {
    dated_file_day(name).is_some_and(|day| day < utc_date(now_ms - days * DAY_MS))
}

/// True when an event file name is older than yesterday (UTC) and may be
/// deleted by the retention pass. Names that are not event files are never
/// eligible, so a stray file in the directory is left alone.
pub fn is_expired_event_file(name: &str, now_ms: i64) -> bool {
    is_dated_file_older_than(name, now_ms, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(utc_date(0), (1970, 1, 1));
        // 2026-10-27T12:00:00Z
        assert_eq!(utc_date(1_793_102_400_000), (2026, 10, 27));
        // 2024-02-29T23:59:59.999Z, a leap day boundary.
        assert_eq!(utc_date(1_709_251_199_999), (2024, 2, 29));
        assert_eq!(utc_date(1_709_251_200_000), (2024, 3, 1));
        assert_eq!(utc_date(-1), (1969, 12, 31));
    }

    #[test]
    fn every_day_of_a_leap_cycle_round_trips_in_order() {
        let mut previous = utc_date(0);
        for day in 1..(366 * 4 + 1) {
            let current = utc_date(day * DAY_MS);
            assert!(current > previous, "{current:?} after {previous:?}");
            assert_eq!(day_start_ms(current), day * DAY_MS);
            assert_eq!(parse_utc_day(&utc_day_text(day * DAY_MS)), Some(current));
            previous = current;
        }
        assert_eq!(day_start_ms(utc_date(-1)), -DAY_MS);
    }

    #[test]
    fn reader_window_and_retention() {
        let now = 1_793_102_400_000;
        assert_eq!(
            event_files_to_read(now),
            ["2026-10-26.jsonl".to_owned(), "2026-10-27.jsonl".to_owned()]
        );
        assert!(is_expired_event_file("2026-10-25.jsonl", now));
        assert!(!is_expired_event_file("2026-10-26.jsonl", now));
        assert!(!is_expired_event_file("2026-10-27.jsonl", now));
        assert!(!is_expired_event_file("notes.txt", now));
        assert!(!is_expired_event_file("2026-13-01.jsonl", now));
        assert!(!is_expired_event_file("26-10-01.jsonl", now));
    }
}
