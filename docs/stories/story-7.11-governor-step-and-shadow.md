# Story 7.11: The governor step and the shadow

**Status:** Approved

## Story

As the author who wants to see what the governor would do before it does anything,
I want the window to run the decision on every tick while I set the governor to shadow, journal every would-launch with the inputs it used and every change of hold causes, and book each would-launch as if it had run,
so that a week of shadow shows how often the conditions pass together on my own days, with cadences and caps followed as a real launch would, while nothing is launched (FR-11 in shadow, FR-14's shadow mode, PRD section 7's Slice A rows).

## Acceptance Criteria

1. **The step** (AD-137, AD-4 and AD-21 as amended, D-327). `governor::step` runs on the collector thread under `window-authority` while the mode is shadow or on: at every recompute, every `GOVERNOR_TICK` (60 s) and at the last decision's `next_change_ms`. It assembles the input, calls `decide` with the offset read at that step, journals, sends a probe job on `Probe`, and books a would-launch. It never waits. With the mode off it does not run. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{the_governor_step_runs_each_tick_in_shadow, it_does_not_run_when_off, it_wakes_at_next_change, the_board_keeps_its_pace_with_the_governor_in_shadow}`.
2. **The input from disk** (AD-128, AD-137). The step reads `[governor]` and the orders from the approved snapshot only (a pending change gives `catalogue_changed`), the board's counts, the registry's evidence (stories 7.4 to 7.6), the newest observation of each source (7.7, 7.8), the probe state (7.8), this week's autonomous points and the review figures (7.9), the readiness facts (7.10), each order's latest booking whatever its age and at most 14 bookings per order of the last 7 days (D-316, D-352), all journal folds read from the ledger (D-348, review F14), which this story extends with bookings, launch outcomes, hold and would-launch lines and `ceiling_crossed` lines. `InputsState` (D-344, review F4) marks the registry, the ledger, the board and the readiness cache known or unknown: the board is unknown while stale or until `claude agents` first answers after window start, the readiness cache until the `workshop` thread first caches facts. Proof: host unit `crates/officina-host/src/governor/step.rs::tests::{orders_come_from_the_approved_snapshot_only, a_pending_floor_change_gives_catalogue_changed, the_latest_booking_is_read_whatever_its_age}`; CLI frozen-clock `crates/officina/tests/governor.rs::{an_unreadable_registry_holds, an_unseeded_ledger_holds, a_board_before_agents_answers_holds, a_stale_board_holds, no_readiness_facts_hold}`.
3. **Modes off and shadow** (AD-140, FR-14, D-328). `GovernorMode` (declared in story 7.4, D-310) is stored in `setting.governor_mode` with source `author`; `set_governor_mode(off | shadow)`: shadow is pointer only and refused by the writer unless the approved floor has a valid `[governor]` and a standing order, checked when it writes; off is always accepted and keyboard-reachable; journal `governor_mode` (from, to, `by`). The governor ships off. The command joins `build.rs`, the capabilities, `ipc.ts` and `e2e-mock.ts`. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::{shadow_is_refused_without_a_valid_governor_table, shadow_is_refused_without_a_standing_order, off_is_always_accepted, the_governor_ships_off}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`; core `relay/journal.rs::tests::governor_mode_round_trips`.
4. **Would-launch and booking** (AD-138, FR-11, D-316). On `WouldLaunch` the step journals `governor_would_launch` (the `LaunchRecord`, `officina_version`) and books it: the order's last launch at `now_ms`, a shadow run live until the record's deadline, the template's review estimate pending until the end of the local day, its weekly allowance in this week's autonomous points; and, as a run would (D-349, review F15), the last autonomous run's end at its deadline (`no_post_run_observation`) and its 5-hour window as used (`one_per_window`). Bookings are read back from those lines through the ledger, so they survive a restart. Proof: CLI frozen-clock `crates/officina/tests/governor.rs::{a_shadow_week_follows_cadence_and_caps, a_booking_holds_autonomous_full_until_its_deadline, a_booking_needs_a_post_run_observation, a_fallback_shadow_would_launches_once_per_5_hour_window, bookings_survive_a_restart, a_720_hour_order_would_launch_once_in_a_month}`; core `relay/journal.rs::tests::governor_would_launch_round_trips`.
5. **Shadow is inert** (AD-138, PRD section 7's Shadow inert row, D-326). A simulated shadow week with the fake engine and the fake claude, several would-launch decisions included, creates zero container (`create` and `start` counts per role), spawns zero `--bg` session and inserts zero `work_order` row. Proof: CLI frozen-clock `governor.rs::a_shadow_week_creates_no_container_no_session_and_no_work_order`.
6. **Hold lines** (AD-137, FR-11 as amended by amendment 7, D-327). `governor_hold` (mode, causes with numbers, per-order causes, `heartbeat`, `next_change_ms`) is written when the set of failing causes changes, at most one line per minute (a change within the minute is written at its end if it still holds), and every `HOLD_HEARTBEAT_MS` (15 minutes) while unchanged with `heartbeat: true`. Proof: CLI frozen-clock `governor.rs::{a_cause_change_writes_one_line, two_changes_in_a_minute_write_one_line, a_heartbeat_follows_15_unchanged_minutes}`; core `relay/journal.rs::tests::governor_hold_round_trips`.
7. **Probes asked by the step** (AD-137 step 12). `Probe` sends one job to `governor-probe` (story 7.8) unless one runs; its observation triggers a new step; a would-launch rests on a probe anchored in the last 2 minutes. Proof: CLI frozen-clock `governor.rs::{a_would_launch_rests_on_a_probe_under_2_minutes, a_probe_job_is_not_sent_twice}`.
8. **Observations and ceiling lines** (AD-133, AD-136, D-317, D-323). The step journals `quota_observed` through story 7.7's `journal_new` for each observation it uses, and `ceiling_crossed` from story 7.9's `crossings`, in shadow and on, inside the window only. Proof: CLI frozen-clock `governor.rs::{an_observation_used_twice_is_journaled_once, ceiling_crossed_is_written_inside_the_window_only, a_window_start_writes_the_current_side}`; core `relay/journal.rs::tests::ceiling_crossed_round_trips`.
9. **Replay and size** (AD-148 replay row, D-327). Every `governor_hold` and `governor_would_launch` line written by the host tests is parsed back and its `input` replayed through `decide`: same kind, same causes. The largest input (16 orders, 14 bookings each, 8 scoped rows, every list full; D-352, review F28) gives a line under 16 KiB. Proof: CLI `governor.rs::every_decision_line_replays_to_the_same_decision`; core `relay/journal.rs::tests::the_largest_decision_line_stays_under_16_kib`.
10. **Shadow evidence** (AD-144, PRD section 7). `governor::evidence`: the share of window minutes per hold cause, failing first and failing at all, over the last 5 listed days, each line counting until the next and at most `HOLD_HEARTBEAT_MS + GOVERNOR_TICK` past itself, so a heartbeat written up to a tick late leaves no minute unattributed (D-327, review F26); the share of window minutes under the ceiling from `ceiling_crossed` lines; the would-launch decisions of the last 14 days. Proof: core `crates/officina-core/src/governor/evidence.rs::tests::{a_line_counts_at_most_15_minutes_and_a_tick, a_late_heartbeat_leaves_no_gap, a_closed_window_does_not_inflate_a_share, shares_cover_listed_days_only, the_ceiling_share_starts_from_the_window_start_line}`.
11. **Doctor** (AD-147, D-341). The governor's mode, and any self-stop and its cause (none can exist before story 7.15, which writes the first, `held_expired`, D-345; the item reads the setting). Proof: host `crates/officina-host/src/governor/doctor.rs::tests::the_governor_mode_is_reported`.
12. **Zero write** (D-80). A shadow step with a probe joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
13. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: the governor in shadow. README "The governor (v0.7, in progress)": modes off and shadow, that shadow launches nothing and follows cadences and caps, what the journal records. Proof: review.

## Tasks

- [ ] `crates/officina-host/src/governor/step.rs`: triggers, input assembly with `InputsState`, journaling, bookings, probe jobs (AC 1, 2, 4, 7, 8)
- [ ] Ledger folds for bookings, launches, hold, would-launch and ceiling lines (AC 2, 10)
- [ ] `GovernorMode`, `set_governor_mode` writer and command; `JournalKind` variants of this story (AC 3, 4, 6, 8)
- [ ] Hold line cadence and heartbeat (AC 6)
- [ ] `crates/officina-core/src/governor/evidence.rs` (AC 10)
- [ ] Frozen-clock cases in `crates/officina/tests/governor.rs`, replay and size tests (AC 4 to 9)
- [ ] Doctor; zero write; `CHANGELOG.md`, README (AC 11 to 13)

## Dev Notes

- Requirements: FR-11 as amended (shadow), FR-14 (off and shadow), FR-17's shadow evidence, PRD section 7's Would launch, Hold causes, Ceiling room and Shadow inert rows. Decisions: AD-3, AD-4, AD-5, AD-21, AD-133, AD-136, AD-137, AD-138, AD-140 (modes), AD-143. Risks: R77. Epic decisions: D-25, D-72, D-80 (Epics 2 and 3); D-135 (Epic 5); D-223 (Epic 6); P-65, D-300, D-310, D-316, D-317, D-323, D-326, D-327, D-328, D-339, D-340, D-341, D-344, D-348, D-349, D-352.
- Seams (D-300): the collector's `AuthorRequest`, `Emit` and timers (1.5, 3.5, 5.7), the journal append (2.5), the fake engine (6.3 onward) and the fake claude (5.2 onward).
- Waits for: story 7.10, and stories 7.4 to 7.9 behind it.
- Frozen clocks (D-339): the tests drive the step with explicit `now_ms` and offset through `window-authority` from the dev-dependency; the shipped binary has no clock override.
- What runs outside the step (D-327): story 7.6's transitions; from story 7.15, releases and expiries. Neither depends on the mode.
- The shadow uses probes within the same budget as on (AD-138): gate 1's day bounds what a shadow week costs.
- In Slice A the decision has no `Launch`, so mode on cannot be set (story 7.16) and an input in mode on would give `WouldLaunch` anyway (D-326).

## Testing

- Core unit: `crates/officina-core/src/{governor/evidence,relay/journal}.rs`.
- Host unit: `crates/officina-host/src/{governor/step,governor/writers,governor/doctor}.rs`.
- CLI integration: `crates/officina/tests/{watch,governor,quota_doctor}.rs`. App crate: `app/src-tauri/src/main.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: unknown registry, ledger, board or readiness hold through `InputsState` (F4); journal folds from the ledger (F14); bookings set the last run end and use their 5-hour window (F15); a hold line counts a heartbeat plus a tick (F26); the largest input's bounds (F28); `GovernorMode` from story 7.4 (F8)
