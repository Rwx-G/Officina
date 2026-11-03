# Story 6.17: Stop all, the stop flags, Resume and kept containers

**Status:** Review

## Story

As the author who must be able to stop every worker at once,
I want one action, in the window or from any terminal, that stops every worker and gateway within 10 seconds, stops the relay, keeps the containers for examination and survives a restart until I resume from the window,
so that a looping or misbehaving run is stopped without touching my own sessions, and nothing starts again behind my back.

## Acceptance Criteria

1. **Flags** (AD-116, AD-34 as amended). `<home>/workshop/stopped` and `relay/stopped` hold `{"since_ms", "by"}` with `by` `window` or `cli`; a writer creates a missing flag and leaves a present one; AD-34's `is_stopped` stays a per-file predicate and each consumer reads its own flag with it, a flag that cannot be read counting as set: the relay reads `relay/stopped`, the workshop `workshop/stopped`, so the workshop flag alone never stops the relay and the Relay view's own toggle keeps its meaning (P-50, review F42); window start reads both before anything else. Proof: core `crates/officina-core/src/workshop/run.rs::tests::{an_unreadable_flag_counts_as_set, a_present_flag_is_left_alone}`; host `crates/officina-host/src/relay/home.rs::tests::{both_flags_are_read_at_start, the_cli_never_removes_a_flag, the_workshop_flag_alone_does_not_stop_the_relay}`.
2. **Stop all in the window** (AD-116, FR-14). `stop_workshop` (keyboard-reachable; this story owns its `build.rs` entry, capabilities, `every_declared_command_is_allowed`, `ipc.ts` wrapper and `e2e-mock.ts` answer, review F33) runs on its command's blocking thread, never behind a job: writes both flags; sets the in-memory latch that thread `workshop` reads immediately before each `start` (D-241, review F20); journals `relay_stopped` and `workshop_stopped` (scope, `by`, containers); `docker ps -q --filter label=officina.managed=1 --filter label=officina.floor=<key>`, then one `docker stop -t 3` with every id; terminates the job of any running build or battery and of the `workshop` thread's current engine call; waits, bounded inside the 10 s budget, for the thread's acknowledgement of the latch; then a second `ps` and `stop` pass catches a `start` the daemon accepted before the latch; live runs become `stopped` with `stopped_at_ms` (a `launching` run ends `workshop_stopped` and its created objects are removed; a `collecting` run becomes `stopped` and is collected again at Resume, the packer emptying `out/` first). Containers are kept; the author's own sessions are untouched. Proof: CLI with the fake engine (stop latency mode) `crates/officina/tests/workshop_stop.rs::{stop_all_ends_every_run_within_10_s, a_slow_engine_past_10_s_fails_the_budget_check, flags_survive_a_restart, a_launch_racing_stop_all_is_stopped, nothing_launches_builds_or_collects_while_set, followers_keep_reading_while_stopped, import_and_discard_stay_possible_while_stopped, a_collecting_run_is_collected_again_at_resume}` (the second is the control: the budget assertion can fail; the race case gives the fake engine a `start` latency so the launch's `start` lands after the first `ps`); core `crates/officina-core/src/relay/journal.rs::tests::{workshop_stopped_round_trips, workshop_resumed_round_trips}`.
3. **`officina stop --all`** (AD-116, amendment 11, D-215). No authority: writes the same flags (never removes one), `ps` and `stop` through the read half for every `officina.managed` container of the engine, of any floor, one journal line; exit 0, or 1 when the engine could not be reached (the flags are written and the message says containers may still run); 64 on usage, never 2. `readme.rs` and `quota_doctor.rs` cover it. Proof: CLI `crates/officina/tests/workshop_stop.rs::{cli_stop_writes_both_flags_and_stops_every_floor, cli_stop_with_the_engine_down_exits_1_with_flags_written, cli_stop_never_removes_a_flag}`; `crates/officina/tests/readme.rs::readme_mentions_every_subcommand`; `quota_doctor.rs::no_command_writes_under_the_claude_config_dir` (with `stop --all`).
4. **Resume** (AD-116, FR-14, D-28). `resume_workshop(choices, resume_relay)` (window only, pointer only; its plumbing owned here, review F33): the dialog's payload lists the kept runs, each to collect (the default) or remove, and offers to resume the relay too (checked by default; the Relay view's own toggle stays); confirming deletes the workshop flag (and the relay's when checked), journals `workshop_resumed` with the counts, then collects (story 6.13) or removes (state `discarded`, reason `not_collected`, `workshop_removed` cause `resume`, `workshop_finished` with outcome `not_collected`). Proof: CLI `workshop_stop.rs::{resume_collects_by_default, resume_removes_what_the_author_chose, resume_can_leave_the_relay_stopped}`.
5. **Kept containers** (AD-116, FR-14, D-213). The hourly pass queues on thread `workshop` the removal of kept containers older than 7 days after `stopped_at_ms`, even while stopped (removal only reduces): `workshop_removed` cause `kept_expired`, state `discarded` reason `not_collected`. An unreachable engine leaves the removal queued and the row unchanged. Proof: core `run.rs::tests::kept_containers_expire_after_7_days`; CLI `workshop_stop.rs::{kept_containers_are_removed_after_7_days, an_engine_down_delays_the_removal}`.
6. **Remove a run** (AD-121, D-215). `remove_run(run_id)` (kept or orphan; keyboard-reachable; its plumbing owned here, review F33) validates 32 lowercase hex and acts only on objects labelled with that run and the window's floor. Proof: CLI `workshop_stop.rs::{remove_run_refuses_a_malformed_id, remove_run_never_touches_another_floor}`.
7. **Doctor** (AD-123). Both flags; kept containers and their age. Proof: host `crates/officina-host/src/doctor.rs::tests::{both_flags_are_reported, kept_containers_show_their_age}`.
8. **Documentation** (D-226). README: Stop all, `officina stop --all` (any floor, never removes a flag), Resume from the window only, kept containers hold the credential in their engine configuration until removed (7 days); `CHANGELOG.md`. Proof: review.

## Tasks

- [x] Flags and `is_stopped` amended (AC 1)
- [x] `stop_workshop` on its own blocking thread; the latch, its acknowledgement and the second pass; job termination; state changes; plumbing (AC 2)
- [x] `officina stop --all` in `crates/officina/src/main.rs` with the read half (AC 3)
- [x] `crates/officina-host/src/workshop/resume.rs`, `resume_workshop` (AC 4)
- [x] Kept-container expiry (AC 5); `remove_run` (AC 6)
- [x] Doctor (AC 7); README, `CHANGELOG.md` (AC 8)

## Dev Notes

- Requirements: FR-14 as amended (amendment 11), J4, journey 4, PRD section 7 Stop. Decisions: AD-21, AD-34 as amended, AD-116, AD-120, AD-121. Risks: R49. Epic decisions: D-28, D-30 (Epic 2); D-47 (Epic 3); P-50, D-200, D-213, D-215, D-223, D-241, D-249.
- Seams (D-200): AD-34's stop flag and `is_stopped` (2.5, 2.7), `officina/main.rs` usage and `readme.rs` (2.10), `quota_doctor.rs`, the hourly pass (3.2).
- Waits for: stories 6.12 and 6.13.
- The CLI cannot know the window's floor (D-30), and stopping more is safe; the window stops its own floor's containers.
- Stopping is safe from any process; resuming is not, so only the window resumes (AD-21).
- Why a latch and a second pass (D-241, review F20): a launch that read the flag before it was written can issue `start` after the first `ps`, and killing `docker.exe` mid-call does not undo a start the daemon accepted; without them the window could read stopped while a worker runs (J4).
- The once-on-the-workstation check of section 7's Stop row is story 6.20's.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/run.rs`, `relay/journal.rs`; host `crates/officina-host/src/{relay/home,doctor}.rs`; CLI with the fake engine `crates/officina/tests/{workshop_stop,readme,quota_doctor}.rs`.

## Dev Agent Record

### Debug Log

- Every new Rust test passed on its first run; the doctor age expectation was corrected once (an age of 48 h prints as "2 days"). The reduced-motion e2e case was checked against a mutant that ignores reduced motion (it fails).
- The first gate failed two steps, both real: the Linux job's `workshop_follow.rs::a_token_in_a_mid_run_report_tail_is_replaced_before_the_view` read a run in `collecting` before its texts, a torn write of the run's move (fixed, D-1199, own `fix(registry)` commit); and the zero-write test's `stop --all` resolved the machine's own Docker through the registry (it stopped nothing; the test now names a missing engine, D-1199).
- The motion spec's flaky positive control (gated with this story, own `test(ui)` commit): Svelte 5 starts each transition as a zero-length placeholder animation and creates the timed one in that placeholder's `onfinish`, a frame later. The spec read its record as soon as the relay chip was visible, so under load the alert band's and the chip's timed animations were not recorded yet and the count landed on 5 (reproduced locally with `--repeat-each 4`: one run saw 5, the same run 1 s later 8). It now polls the count, then checks durations and properties once every transition ended; under reduced motion it requires no animation at all, placeholder included.

### Completion Notes

- AC 1: core `run.rs::tests::{an_unreadable_flag_counts_as_set, a_present_flag_is_left_alone}`; host `relay/home.rs::tests::{both_flags_are_read_at_start, the_cli_never_removes_a_flag, the_workshop_flag_alone_does_not_stop_the_relay}`; the window reads both flags first and latches thread `workshop` (D-1198). `StopFlag` gains `by` (D-1193).
- AC 2: CLI `workshop_stop.rs::{stop_all_ends_every_run_within_10_s, a_slow_engine_past_10_s_fails_the_budget_check, flags_survive_a_restart, a_launch_racing_stop_all_is_stopped, nothing_launches_builds_or_collects_while_set, followers_keep_reading_while_stopped, import_and_discard_stay_possible_while_stopped, a_collecting_run_is_collected_again_at_resume}`; core `relay/journal.rs::tests::{workshop_stopped_round_trips, workshop_resumed_round_trips}`, `run.rs::tests::stop_targets_are_running_managed_containers_of_the_floor`, the `run` vector case for `collecting` on `stop_all` (`workshop_vectors.rs::every_allowed_edge_has_a_case`); host `process.rs::tests::{firing_ends_the_running_child_and_spares_the_next, an_unwatched_thread_runs_to_its_exit}`, `threads.rs::tests::the_acknowledgement_waits_for_a_start_in_flight`. Plumbing: `build.rs`, capabilities, `every_declared_command_is_allowed`, `ipc.ts` `stopWorkshop`, `e2e-mock.ts`. D-1190, D-1191, D-1192, D-1194.
- AC 3: CLI `workshop_stop.rs::{cli_stop_writes_both_flags_and_stops_every_floor, cli_stop_with_the_engine_down_exits_1_with_flags_written, cli_stop_never_removes_a_flag}` (the second also checks usage exit 64); `readme.rs::readme_mentions_every_subcommand`; `quota_doctor.rs::no_command_writes_under_the_claude_config_dir` runs `stop --all` last. D-1193.
- AC 4: CLI `workshop_stop.rs::{resume_collects_by_default, resume_removes_what_the_author_chose, resume_can_leave_the_relay_stopped}`; core `run.rs::tests::resume_collects_unless_told_otherwise`. `resume_workshop` lives in `crates/officina-host/src/workshop/resume.rs`; plumbing as AC 2. D-1195.
- AC 5: core `run.rs::tests::kept_containers_expire_after_7_days`; CLI `workshop_stop.rs::{kept_containers_are_removed_after_7_days, an_engine_down_delays_the_removal}`. The hourly pass hands the removal to thread `workshop` through the collector's `HourlyWorkshop`, which the window sets (D-1196); no automated test drives the window's own hourly timer.
- AC 6: CLI `workshop_stop.rs::{remove_run_refuses_a_malformed_id, remove_run_never_touches_another_floor}`; core `run.rs::tests::run_objects_are_labelled_with_the_run_and_the_floor`. D-1195.
- AC 7: host `workshop/doctor.rs::tests::{both_flags_are_reported, kept_containers_show_their_age}` (in the workshop section's module, D-1197).
- AC 8: README "Stop all and Resume" (Stop all, `officina stop --all` on any floor never removing a flag, Resume from the window only, kept containers holding the credential until removed at 7 days) and the command table; `CHANGELOG.md`.

## File List

- `crates/officina-core/src/workshop/run.rs`, `crates/officina-core/src/workshop/lifecycle.rs`, `crates/officina-core/src/workshop/doctor.rs`, `crates/officina-core/src/relay/mod.rs`, `crates/officina-core/src/relay/journal.rs`, `crates/officina-core/src/relay/view.rs`, `crates/officina-core/tests/typescript_bindings.rs`, `tests/vectors/workshop.json`
- `crates/officina-host/src/workshop/stop.rs` (new), `crates/officina-host/src/workshop/resume.rs` (new), `crates/officina-host/src/workshop/mod.rs`, `crates/officina-host/src/workshop/threads.rs`, `crates/officina-host/src/workshop/launch/window.rs`, `crates/officina-host/src/workshop/collect.rs`, `crates/officina-host/src/workshop/follow.rs`, `crates/officina-host/src/workshop/doctor.rs`, `crates/officina-host/src/process.rs`, `crates/officina-host/src/spawn.rs`, `crates/officina-host/src/collector.rs`, `crates/officina-host/src/watch.rs`, `crates/officina-host/src/relay/home.rs`, `crates/officina-host/src/relay/authority.rs`, `crates/officina-host/src/registry/read.rs`, `crates/officina-host/src/registry/window.rs`, `crates/officina-host/src/registry/workshop.rs`
- `crates/officina/src/main.rs`, `crates/officina/src/bin/officina-fake-docker.rs`, `crates/officina/tests/workshop_stop.rs` (new), `crates/officina/tests/quota_doctor.rs`, `crates/officina/tests/common/workshop_home.rs`
- `app/src-tauri/build.rs`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/src/main.rs`, `app/src/lib/ipc.ts`, `app/src/e2e-mock.ts`, `app/src/e2e-globals.d.ts`, `app/src/lib/generated/` (`StopAnswer`, `ResumeChoice`, `ResumeAnswer`, `KeptChoice`, `RemoveRunAnswer`, `RemoveRunRefusal`), `app/e2e/motion.spec.ts` (test fix)
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-6-decisions.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: a latch read before each `start`, an acknowledgement and a second pass (F20); each consumer reads its own flag (F42); command plumbing owned here (F33)
- 2026-11-03 implemented (D-1190 to D-1199); status review
