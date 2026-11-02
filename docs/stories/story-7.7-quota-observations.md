# Story 7.7: Quota observations: anchoring, freshness and combination

**Status:** Approved

## Story

As the author whose statusline rewrites old numbers on triggers that fetch nothing,
I want the governor to date a quota file by the hook events of the turn that produced its numbers, to treat anything older than 30 minutes as stale, and to keep the higher reading when two sources disagree,
so that it never decides on numbers older than it believes, a forged low file cannot lower what it sees beside a probe, and two sources describing different windows hold it instead of being averaged (FR-4, principle 3).

## Acceptance Criteria

1. **The observation** (AD-133, D-317). `governor::observe::Observation`: source (`file`, `probe`), `anchored_ms`, `written_at_ms` for a file, and per window an optional `WindowReading { used_milli, resets_at_ms }` for the 5-hour and weekly windows, plus scoped weekly readings (`used_milli`, `resets_at_ms`, scope kind, a display label cut to 32 printable characters). A file's `used_percentage` becomes thousandths: snapped to the nearest thousandth when within 1e-6 of it, else rounded up, since a plain `ceil` of an `f64` turns 64.9 into 64.901 (D-317 as amended by review F16); its seconds become milliseconds. A probe's whole-point reading is an interval (D-322), built in story 7.8. Proof: core `crates/officina-core/src/governor/observe.rs::tests::{file_percentages_snap_or_round_up_to_thousandths, a_label_is_cut_to_32_printable_characters}`, with cases for 64.9, 69.9, 16.1 and a value a hair above a thousandth.
2. **Anchoring a file** (AD-133, FR-4 as amended by amendment 3, D-317). `observe::anchor_file(&QuotaFile, &[EventLine], now_ms)`: among event lines whose `session_id` equals the file's, event `UserPromptSubmit` or `Stop`, and `ts` at most `written_at` times 1000 plus 999, the latest gives `anchored_ms`; none within `FRESH_MS` (30 minutes) before `written_at` gives no observation; a file without `session_id` never anchors. Proof: the `observe` section of `tests/vectors/governor.json`, with an event in the same second as `written_at`, one a millisecond after that second, another session's event, a `Notification` event (ignored), none in 30 minutes and a file without `session_id`, through `crates/officina-core/tests/governor_vectors.rs::observe_cases_match`.
3. **Fresh** (AD-133, FR-4). `FRESH_MS` is defined from `quota::STALE_AFTER_MS` (D-317, review F33): the board judges a file by `written_at`, the governor by its anchor, and one constant holds the framing's 30 minutes for both. An observation is fresh when `anchored_ms` is within `FRESH_MS` of `now_ms`, the weekly window is present, and no window of it has `resets_at_ms` at or before `now_ms`. A fresh observation without a 5-hour window means none is active. Proof: `observe` cases at 29:59.999 and 30:00.000, a past 5-hour reset, a missing weekly window, and a file carrying the weekly window alone (D-1042), fresh with no active five-hour window (D-1217).
4. **Combination and conflict** (AD-133, FR-4 as amended). `observe::combine(&[Observation], now_ms) -> Combined` over fresh observations, per window the highest `used_milli`; reset times of one window that differ by more than `RESET_TOLERANCE_MS` (15 minutes, or the v0.7 gate 2 entry's value, D-303), or a 5-hour window active in one fresh observation and absent from another, give a conflict; within tolerance, the earliest 5-hour reset and the latest weekly reset are kept. `Combined`'s outcome is a `closed_enum!`. Proof: `observe` cases, among them a forged low file beside a higher probe that keeps the probe's value, 15:00 and 15:01 of reset difference, and an active-against-absent 5-hour window; `governor_vectors.rs::every_combined_outcome_has_a_case`.
5. **The host's observation** (AD-133, D-317). The collector builds the file observation from the floor's quota file (AD-7, AD-8) and the event lines it already holds (two UTC days), on each recompute that reads either. `governor::observations::journal_new(&mut LastJournaled, &Observation)` writes `quota_observed` (source, `anchored_ms`, `written_at_ms`, values; never text) once per distinct observation keyed by (source, `anchored_ms`, `written_at_ms`), its memory seeded at window start from the journal's last lines; story 7.11's step calls it for each observation it uses. Proof: host unit `crates/officina-host/src/governor/observations.rs::tests::{an_observation_is_journaled_once, a_restart_seeded_from_the_journal_does_not_repeat_it}`; core `crates/officina-core/src/relay/journal.rs::tests::quota_observed_round_trips`.
6. **The contract holds** (FR-4, AD-8). The quota file contract does not change: its vector set and the statusline writer are untouched. Proof: the existing quota contract vectors pass unchanged; review of the diff.
7. **Doctor** (AD-147, D-341). The quota file's `written_at`, whether it carries a `session_id`, its anchored time or why none, and whether its windows are present (absent: no subscription rate limits in the statusline, or no response yet, S1); no file at all names the `officina quota write --tee` wiring the author adds in his own settings. Proof: host `crates/officina-host/src/governor/doctor.rs::tests::{the_quota_file_reports_its_anchor, a_file_without_windows_warns, no_file_names_the_tee_wiring}` (file, event lines and `now_ms` as arguments).
8. **Documentation** (D-340). README "The governor (v0.7, in progress)": what fresh means (anchored on the turn's hook events, 30 minutes), that the author wires `officina quota write --tee` himself and Officina never writes his statusline settings, that the higher of two readings wins. `CHANGELOG.md` `[Unreleased]` under Added. Proof: review.
9. **The quota folder is protected** (D-1208). `relay::gate::PROTECTED_DIRS` gains the quota folder: a shell command in a guarded project naming it is denied `protected_dir`; `officina quota write` from a statusline is unaffected. Proof: core `crates/officina-core/src/relay/gate.rs::tests::protected_dirs_follow_the_home` (iterates the constant); a `tests/vectors/gate.json` case writing a forged quota file; `crates/officina/tests/quota_doctor.rs::doctor_passes_once_the_block_is_merged` unchanged.
10. **Journal writes** (D-1207). `quota_observed` is written through `relay::journal::record`. Proof: `crates/officina-host/src/governor/observations.rs::tests::an_observation_is_recorded_when_the_lock_stays_held` (a spill file holds it).

## Tasks

- [ ] `crates/officina-core/src/governor/observe.rs`: types, `anchor_file`, freshness, `combine`; `observe` section (AC 1 to 4)
- [ ] `crates/officina-host/src/governor/observations.rs`: the file observation, `journal_new`, start seeding; `JournalKind::QuotaObserved` (AC 5)
- [ ] Doctor; README, `CHANGELOG.md` (AC 7, 8)
- [ ] The quota folder in `PROTECTED_DIRS`, its `gate.json` case (AC 9)

## Dev Notes

- Reworked 2026-11-02 (D-1202, D-1204, D-1207, D-1208, D-1217): the quota contract on `main` writes a missing window as an absent key and doctor's quota line names the windows a file holds (`QuotaFile::window_names`, D-1042); this story reuses both. A file with the weekly window alone is fresh and has no active five-hour window, so the governor holds `no_active_window` (R129). The event buffer the anchor reads is already protected from guarded sessions (D-961); the quota folder joins it.
- Requirements: FR-4 as amended (amendment 3), FR-18's quota sources, PRD section 7's Fresh quota row. Decisions: AD-7, AD-8, AD-10, AD-43, AD-133. Facts: S1 (the statusline's triggers), E3, the v0.7 gate 2 entry. Risks: R68, R73. Epic decisions: P-65, D-300, D-303, D-304, D-317, D-322, D-340, D-341.
- Seams (D-300): `quota.rs` (1.9), the collector's event lines (1.5), the journal append (2.5).
- Waits for: story 7.1's v0.7 gate 2 entry (tolerance and fractions, D-1203); built on `oauth-token` and `login` floors, the one regime (D-1204).
- `ts` is when `officina hook` received the payload, in milliseconds; `written_at` is in seconds, so an event of the same second counts up to its last millisecond (amendment 3). AD-10's skip rule can only make an anchor older, which errs toward caution.
- Without the probe, a process running as the author can forge a low file (R73); story 7.10's `one_per_window` bounds that case, and the weekly cap never reads the file.
- Nothing here decides: story 7.10 reads `Combined`.

## Testing

- Core unit and integration: `crates/officina-core/src/{governor/observe,relay/journal}.rs`, `crates/officina-core/tests/governor_vectors.rs`.
- Host unit: `crates/officina-host/src/{governor/observations,governor/doctor}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: file percentages snap to a thousandth before rounding up (F16); `FRESH_MS` derived from `STALE_AFTER_MS` (F33); doctor items in `governor/doctor.rs` (F30)
- 2026-11-02 reworked against v0.4 to v0.6 as built: the weekly-window-only file of D-1042 (D-1217); the quota folder protected (D-1208, AC 9); `record` for observations (D-1207, AC 10); one regime (D-1204); gate 2 only (D-1203)
