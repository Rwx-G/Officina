# Story 7.3: Local time, the autonomy window and the week's listed hours

**Status:** Approved

## Story

As the author who sets my autonomy window in my own local time and works on chosen days,
I want the governor to read the local offset from Windows at every step, to know whether it is inside my window, and to measure the week's pace on my working days only,
so that a daylight saving change moves the window as my clock does, and the weekly projection does not mistake a weekend for idleness (FR-6, FR-11, J5).

## Acceptance Criteria

1. **Civil time in core** (AD-129, D-308). `dates::utc_civil(now_ms)` returns the UTC civil time (year, month, day, hour, minute, second, millisecond); `dates::offset_minutes(&utc, &local)` returns local minus UTC in minutes, across a day, month and year boundary in both directions. Proof: core `crates/officina-core/src/dates.rs::tests::{utc_civil_round_trips_leap_days, offset_crosses_midnight_and_new_year_both_ways}`.
2. **The offset on Windows** (AD-129, D-308). `platform::windows::local_offset_min(now_ms) -> Option<i32>` fills a `SYSTEMTIME` from `utc_civil`, calls `GetDynamicTimeZoneInformation` then `SystemTimeToTzSpecificLocalTimeEx`, and returns `offset_minutes`; any failing call returns `None`; `platform::other` returns `None`. `windows-sys` gains feature `Win32_System_Time` (approved with the addendum). Every `unsafe` block carries its `// SAFETY:` comment. Proof: host unit, Windows only, `crates/officina-host/src/platform/windows.rs::tests::{the_offset_of_now_matches_get_local_time_against_get_system_time, gate_6_instants_convert_in_a_named_zone_with_daylight_saving}` (the first an independent Win32 path read at the same instant; the second, review F27, finds a zone with daylight saving through `EnumDynamicTimeZoneInformation` and converts gate 6's four instants with `SystemTimeToTzSpecificLocalTimeEx`, so a sign or daylight saving error fails on every Windows build although runners run in UTC); `crates/officina-host/src/platform/mod.rs::tests::other_platforms_read_no_offset` (compiled off Windows, run on `core-linux`).
3. **The window** (AD-129, D-308). `clock::window_at(&GovernorConfig, now_ms, offset) -> WindowState`: inside or outside; a range belongs to the local day it starts, so `22:00-06:00` starting on a listed day ends at 06:00 the next day; the current range's start and end as instants; the next start; ending when under `WINDOW_ENDING_MS` (20 minutes) to the end. Proof: the `clock` section of `tests/vectors/governor.json`, with cases at fixed offsets, a range crossing midnight, an unlisted day after a listed one, both daylight saving directions (the offset differs between two calls), and each `WindowState` variant, through `governor_vectors.rs::{clock_cases_match, every_window_state_has_a_case}`.
4. **The week's listed hours** (AD-129, FR-6). `clock::listed_share(&days, weekly_resets_at_ms, now_ms, offset) -> Share`: the weekly window runs from its reset minus 7 days to its reset; the span is cut at local midnights with the current offset; elapsed and total are the milliseconds on listed days; `f = max(elapsed, one listed day) / total`, carried as a numerator and a denominator, never a float. Cases: all seven days listed (calendar share), weekdays only with a reset on a Wednesday (the requirement challenge's example), a reset on an unlisted day, the minimum of one listed day just after a reset, and a weekly window whose start lies after `now_ms` (clock skew), where elapsed time clamps at 0 (review, Arithmetic). Proof: `clock` section cases; `crates/officina-core/src/governor/clock.rs::tests::the_share_is_never_below_one_listed_day`.
5. **Gate 6, second half** (AD-149 row 6, D-302, D-303). `crates/officina-host/examples/local_offset_probe.rs` prints `local_offset_min` for now and for 2026-10-25T00:59Z, 2026-10-25T01:00Z, 2027-03-28T00:59Z and 2027-03-28T01:00Z beside the clock's local time; the agent runs it on the workstation and E63 gains the result. Pass: the current offset matches the clock and the four instants match the first half. On failure, the host passes offset 0 and the README says the window is read in UTC (D-303). Proof: manual, research notes entry E63 (second half).
6. **Doctor** (AD-147, D-341). The local UTC offset the governor uses and the next window start, or a warning when the offset cannot be read. Proof: host `crates/officina-host/src/governor/doctor.rs::tests::{the_local_offset_and_next_window_are_reported, an_unknown_offset_warns}` (offset, configuration and `now_ms` as arguments).
7. **Documentation** (D-340). README "The governor (v0.7, in progress)": the window is local time and follows daylight saving as the clock does; a range belongs to the day it starts; the weekly pace counts listed days only. `CHANGELOG.md` `[Unreleased]` under Added. Proof: review.

## Tasks

- [ ] `dates.rs`: `utc_civil`, `offset_minutes` (AC 1)
- [ ] `platform::windows::local_offset_min`, `platform::other`, the `windows-sys` feature (AC 2)
- [ ] `crates/officina-core/src/governor/clock.rs`: `window_at`, `listed_share`; `clock` section (AC 3, 4)
- [ ] `examples/local_offset_probe.rs`; run it; E63 (AC 5)
- [ ] Doctor; README and `CHANGELOG.md` (AC 6, 7)

## Dev Notes

- Requirements: FR-6 (the projection's `f`), FR-11 (the window, the offset at each tick), FR-13 (the window's end), FR-18. Decisions: AD-1 (core takes `now_ms`), AD-129. Facts: addendum section 1, items 6 and 7. Risks: R75. Epic decisions: P-65, D-300, D-302, D-303, D-308, D-340, D-341.
- Seams (D-300): `dates.rs` (1.2), `platform::windows` (1.7, 6.3, 6.11).
- Waits for: story 7.1 (E63's first half).
- Core does civil arithmetic; Windows converts the instant. No `chrono`, `time` or `jiff` (addendum section 3, ruled out).
- One offset per computation (AD-129's "gives up"): a daylight saving change inside a weekly window shifts a day boundary by at most an hour in 168, and the window-end stop (story 7.16) reads the offset at each step.
- CI runners run in UTC, so one Windows test compares two Win32 readings of the same instant and another converts gate 6's instants in a named zone with daylight saving (D-308 as amended by review F27); AC 5 confirms the workstation's own zone.

## Testing

- Core unit: `crates/officina-core/src/{dates,governor/clock}.rs`; core integration: `crates/officina-core/tests/governor_vectors.rs`.
- Host unit: `crates/officina-host/src/{platform/windows,platform/mod,governor/doctor}.rs`.
- Manual: `examples/local_offset_probe.rs`, research notes entry E63.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: a named zone's daylight saving instants tested on every Windows build (F27); elapsed time clamped at 0 (Arithmetic); doctor item in `governor/doctor.rs` (F30); gate 6 is E63 (F7, superseded by the owner's range)
