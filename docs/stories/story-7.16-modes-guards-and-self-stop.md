# Story 7.16: Mode on, Stop autonomy, the live guards, the window's end and the remaining self-stops

**Status:** Approved

## Story

As the author who turns autonomy on and wants it to stop by itself before it costs me,
I want mode on offered only while its conditions hold, autonomous runs stopped at my window's end and whenever a fresh reading approaches my reserve or my 5-hour room, one action that stops autonomy and nothing else, and a governor that turns itself off on the rules,
so that autonomous spend never enters my reserve, every autonomous run ends with my window, and my own clicked runs and relay are never touched (FR-13, FR-14, J5).

## Entry conditions

Implemented only once story 7.13's Slice B decision entry records **go** (D-331; the conditions are listed in story 7.14), and after stories 7.14 and 7.15: mode on is offered only once scope quarantine and held packages exist (D-345, review F5). The window-end stop (AC 4) is built only when the v0.7 gate 4 entry passes (D-303); the stream stop (AC 5) only when the v0.7 gate 3 entry recorded `rate_limit_event.utilization` with its window mapping. One regime is built (D-1204), so the quota guards of AC 5 always are.

## Acceptance Criteria

1. **Mode on and per-order switches** (AD-140, FR-14, D-314, D-328). `set_governor_mode(on)`, pointer only, refused by the writer unless, when it writes: the approved floor has a valid `[governor]`, at least one order is eligible, a passing escape battery is fresh (v0.6 FR-15), the quota source is usable (the probe passed gate 1 and is not paused, or a file observation is fresh), and no self-stop holds. `set_standing_order(name, on | off)`: on is pointer only and refused unless the order is eligible but for its own state and, after a suspension, a clicked run of the template settled after the state's `at_ms`; off applies from `on` and is keyboard-reachable; off on a suspended or paused order is answered `already_off` and writes nothing. Both commands join `build.rs`, the capabilities, `ipc.ts` and `e2e-mock.ts`. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::{on_is_refused_for_each_missing_condition, on_is_refused_while_a_self_stop_holds, an_order_on_needs_a_clicked_run_after_its_suspension, off_on_a_suspended_order_is_already_off, an_order_off_is_keyboard_reachable_and_always_accepted}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
2. **The per-run stop** (AD-140, v0.6 AD-109). The governor stops an autonomous run by v0.6's per-run stop (stop the worker, record the reason, collect), a run still in `launching` included (D-335, review F35); `FinishCause::AutonomyStopped` joins v0.6's enum (no migration: its column has no `CHECK`, D-310); `AutonomyStopCause` (`window_end` and `local_time_unknown` when the v0.7 gate 4 entry passed, `reserve_reached`, `five_hour_high`, `stop_autonomy`) is a `closed_enum!`; `autonomous_stopped` (run, cause) is journaled; the package is kept and follows story 7.15; with no commit past the base the run is `failed` with that cause, which `view::is_environment_failure` never counts and the pause skips (D-314). Proof: core `crates/officina-core/src/workshop/run.rs::tests::autonomy_stopped_is_not_an_environment_failure`; a case in the finish-cause section of `tests/vectors/workshop.json`; core `relay/journal.rs::tests::autonomous_stopped_round_trips`.
3. **Stop autonomy** (AD-140, FR-14, D-335). `stop_autonomy`, keyboard-reachable since it only reduces: mode off (`by: window`), then a per-run stop of every live autonomous run with cause `stop_autonomy`; clicked runs and the relay go on; v0.6's Stop all still stops everything. Proof: CLI frozen-clock `crates/officina/tests/governor.rs::{stop_autonomy_stops_every_autonomous_run_and_no_clicked_run, stop_autonomy_stops_a_run_still_launching, the_relay_keeps_running_after_stop_autonomy, stop_all_still_stops_autonomous_runs}` (fake engine stop counts per run).
4. **The window's end** (AD-140, FR-13, D-335, the v0.7 gate 4 entry). At each governor step, with that step's offset, an autonomous run live past the window's end is stopped (`window_end`); the collector wakes at the end instant through `next_change_ms`. An offset that cannot be read while an autonomous run is live stops it with `local_time_unknown`, since the window's end cannot be evaluated (D-354, review F41). Proof: CLI frozen-clock `governor.rs::{a_run_live_past_the_window_end_is_stopped_within_a_tick, a_daylight_saving_change_moves_the_end, a_clicked_run_is_not_stopped_at_the_window_end, an_unknown_offset_stops_live_autonomous_runs}`.
5. **Live guards** (AD-140, FR-13 as amended by amendment 9, D-335). While an autonomous run is live, every fresh observation from either source is checked as it arrives, and a probe runs every 10 minutes within D-346's budget of 8 an hour, checked at dispatch and again at spawn (`asked_by: live_guard`); with the offset unknown, the hourly cap alone applies (D-354). A weekly or scoped weekly reading at or above 100 minus the reserve stops every autonomous run (`reserve_reached`) and self-stops the governor; a 5-hour reading at or above `five_hour_max_pct` plus 10 stops them (`five_hour_high`). With the v0.7 gate 3 entry's mapping, a worker's `rate_limit_event` reading at or above a stop threshold also stops the runs; a forged low reading never removes a probe. Proof: CLI frozen-clock `governor.rs::{a_reserve_reading_stops_runs_within_60_s_and_self_stops, a_five_hour_reading_stops_runs_without_self_stop, the_live_probe_runs_every_10_minutes, a_60_minute_run_keeps_its_live_probes_and_its_second_bracket_observation, a_forged_low_rate_limit_event_does_not_silence_the_probe}`.
6. **The reserve mutant's live site** (D-338 as amended by review F10). `mutant-governor-skip-reserve` also drops the reserve from the live reserve guard; the `mutants` job runs `cargo test -p officina --test governor --features mutant-governor-skip-reserve`, which must fail with `a_reserve_reading_stops_runs_within_60_s_and_self_stops` besides story 7.14's launch case. Proof: the CI job `mutants`.
7. **Self-stops** (AD-140, FR-14, D-335, D-345). Through story 7.15's writer, this story adds `reserve_reached` (AC 5) and `credential_found`: a credential found in an autonomous package (v0.6 AD-112) self-stops at collection. Turning on again needs mode on's conditions and the author's click, and after `credential_found` also a stored credential whose `LastWritten`, read with AD-105's credential read and the value dropped at once, is later than the self-stop. Proof: host unit `writers.rs::tests::{a_self_stop_holds_until_a_click_with_on_conditions, credential_found_needs_a_newer_credential}` (a `CredentialSource` injected, as story 6.11 does); CLI `governor.rs::a_credential_in_an_autonomous_package_self_stops`.
8. **Window start** (AD-140). The persisted mode and self-stop hold; v0.6's reconciliation runs first; then a first step journals the current state, and any autonomous run past the window's end is stopped. Proof: CLI `governor.rs::window_start_keeps_mode_and_self_stop_and_stops_late_runs`.
9. **In the window** (AD-145, FR-17, D-28). `GovernorMode.svelte` adds on (pointer only) and Stop autonomy (keyboard-reachable); `StandingOrderTable.svelte` adds the per-order on (pointer only) and off (keyboard-reachable); D-1209's header slot reads "autonomy on" (neutral) while the mode is on, story 7.15's "governor stopped" taking its place while a self-stop holds; `governor-text.ts` gains exhaustive records for `AutonomyStopCause` and `SelfStopCause`. Proof: Vitest `app/src/components/governor/GovernorMode.test.ts::{on_ignores_a_keyboard_click, stop_autonomy_is_keyboard_reachable}`, `app/src/components/governor/StandingOrderTable.test.ts::{an_order_on_ignores_a_keyboard_click, an_order_off_is_keyboard_reachable}`; Playwright `app/e2e/governor.spec.ts::{a_reserve_self_stop_shows_its_cause, the_autonomy_on_slot_is_neutral_in_both_themes}`.
10. **The view** (AD-144). `GovernorView` gains the per-order on and off, the self-stop with its cause and numbers, and the live autonomous runs with their stop causes; its schema follows P-63. Proof: core `crates/officina-core/src/governor/view.rs::tests::a_self_stop_is_shown_with_its_numbers`.
11. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added. README "The governor (v0.7, in progress)": mode on and its conditions; Stop autonomy against Stop all; the window's end (or, when the v0.7 gate 4 entry failed, the deadline cut to it); the live guards; the self-stops and how to clear each, naming `AutonomyStopCause` without listing it. Proof: review.
12. **Journal writes** (D-1207). `governor_mode` and the per-order switches are appended before their setting is written; `autonomous_stopped` and the self-stops are recorded. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::a_held_lock_refuses_mode_on`.

## Tasks

- [ ] `set_governor_mode(on)`, `set_standing_order`; writers and commands (AC 1)
- [ ] `FinishCause::AutonomyStopped`, `AutonomyStopCause`, the per-run stop; `JournalKind::AutonomousStopped` (AC 2)
- [ ] `stop_autonomy` (AC 3)
- [ ] Window-end stop (AC 4, if the v0.7 gate 4 entry passed)
- [ ] Live guards, the 10-minute probe within the budget, the stream stop if the v0.7 gate 3 entry allows; the unknown-offset stop (AC 4, 5)
- [ ] The reserve mutant's live site (AC 6)
- [ ] `reserve_reached` and `credential_found` through story 7.15's writer; the clearing rules and the credential check (AC 7)
- [ ] Window start (AC 8)
- [ ] Svelte controls, the header slot's on state, texts; view fields (AC 9, 10)
- [ ] `CHANGELOG.md`, README (AC 11)

## Dev Notes

- Reworked 2026-11-02 (D-1201, D-1207, D-1209, D-1213): the header's single slot; journal writes per D-760; a run ending `auth_failed` stops further launches through readiness, so no credential self-stop is added beyond `credential_found`. GitHub CI is off (D-1201): every CI job named here is that job replayed by the local full gate.
- Requirements: FR-13 as amended (amendment 9), FR-14, J5, PRD section 7's Reserve, Window and Stop rows. Decisions: AD-35, AD-105, AD-109, AD-112, AD-116, AD-140, AD-145. Facts: the v0.7 gate 3 and 4 entries. Risks: R71, R75, R76. Epic decisions: D-28, D-69 (Epics 2 and 3); D-148 (Epic 5); D-217, D-223 (Epic 6); P-65, D-300, D-303, D-304, D-310, D-314, D-328, D-331, D-335, D-338, D-340, D-345, D-346, D-354.
- Seams (D-300): the per-run stop, `FinishCause` and reconciliation (6.12), the stop flags and Stop all (6.17), the credential read (6.11, AD-105), `credential_found` (6.14), `GovernorMode.svelte` and `StandingOrderTable.svelte` (7.12).
- Waits for: stories 7.14 and 7.15 (D-345): no build offers mode on before the order's scope and the ceiling's hold exist.
- Stop autonomy is the framing's per-profile emergency stop and what the shortcut does (story 7.17): a stray press must not halt the relay or the author's clicked runs (FR-16).
- The stream is untrusted (v0.6 AD-109): its `rate_limit_event` can only add a stop, never silence the probe, and since the event names no window, it is read only with the v0.7 gate 3 entry's recorded mapping.
- A reserve reading while a run is live or within 10 minutes after its end, without that run stopped within 60 s, is what section 7's Reserve row counts (story 7.20).

## Testing

- Core unit: `crates/officina-core/src/{workshop/run,governor/view,relay/journal}.rs`; core integration: `crates/officina-core/tests/workshop_vectors.rs`.
- Host unit: `crates/officina-host/src/governor/writers.rs`.
- CLI integration: `crates/officina/tests/governor.rs`. App crate: `app/src-tauri/src/main.rs`.
- Frontend: Vitest in `app/src/components/governor/`, Playwright `app/e2e/governor.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: renumbered after scope and held packages, so mode on cannot ship before them; the self-stop writer now comes from story 7.15 (F5); the live guard within the hourly budget of 8 (F9); the reserve mutant's live site with its named case (F10); a run still launching is stopped (F35); an unknown offset stops live autonomous runs (F41)
- 2026-11-02 reworked against v0.4 to v0.6 as built: the header slot reads "autonomy on" (D-1209, AC 9); journal writes per D-760 (D-1207, AC 12); local gate (D-1201)
