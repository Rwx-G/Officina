# Story 8.11: What the window runs for a worker (Slice B)

**Status:** Approved

## Story

As the author whose worker asks for a read it may have,
I want the window to run exactly the admitted action from my own tree and project folder, under the same runner, caps and timeouts as a session's `officina run`, stoppable by every stop I have, and journaled as the worker's,
so that a worker gets the output of my approved read and nothing else: never its own code, never a path in its clone, never a write (FR-7, J4).

## Acceptance Criteria

1. **Entry condition** (FR-0, P-72, D-403). 8.10 has merged; v0.6's Slice A has merged; the gate 3 entry's first half is recorded and, when it made question 7's dedicated key a precondition, the owner has set it up and the README shows it. Proof: the gate 3 entry and `epic-8-decisions.md`.
2. **Authority** (AD-150, AD-21 as amended, D-9). `officina-host::workshop::relay` compiles only under `window-authority`, its error type carrying `AUTHORITY_MARKER`; built without the feature it does not exist, so `officina.exe` holds no path that runs an action for a worker. The `Command::new` sites stay D-204's four. Proof: host test compiled only without the feature, `crates/officina-host/src/workshop/mod.rs::tests::the_relay_module_needs_window_authority` (reads the module declaration's attribute), and a CI step running it on both jobs; the existing lint test's count of four sites.
3. **Serving an admission** (AD-158, D-409, D-418; review F5, F13, F16). `workshop::relay::serve(Admission, &RunRelayFacts, frames)` on the run's `workshop-relay` thread: an `ack` first, carrying a duration in milliseconds equal to the timeouts of every targets action to resolve plus the action's, capped at the time left to the run's deadline; for `ResolveThen`, `resolve_targets` (D-410) for each write it lists, then `worker::after_targets`, any out of scope refused `order_required`; for an `after-check` read, `run::check_step` (`check_required`); just before spawn, both stop flags, the current floor's hash and the run's state read again (`stopped`, `catalogue_changed`, `not_live`); then the action through `run_child` with `JobMode::ChildOnly`, a capture sink and `RunContext::Workshop { run, relay_seq }`, in the **host** project folder resolved from the current relay floor as `officina run` resolves it, the floor's hash being the approved one; timeout the least of `timeout_s` and the time left to the run's deadline; output capped at `max_output_kib` with AD-31's marker; a `result` frame with the exit code and the bytes. `run_child` returns, so the window never exits or joins its child's job. An HTTP read runs through story 2.11's execution with the same sink (D-407). Proof: host `crates/officina-host/tests/workshop_relay_serve.rs::{an_ack_comes_before_the_run, the_ack_covers_the_resolutions_and_the_action, a_read_runs_in_the_host_project_folder, the_timeout_is_cut_to_the_run_deadline, output_is_capped_with_the_marker, an_after_check_read_without_its_check_is_check_required, a_failed_resolution_is_order_required, every_listed_write_is_resolved, a_catalogue_approved_during_the_resolution_refuses, a_run_ending_during_the_resolution_refuses_not_live, the_window_survives_its_child, an_http_read_uses_the_capture_sink}` (fake tool, real `relay::run`).
4. **Path containment** (FR-8's host test as P-70 reads it, AD-159, D-418, review F18). For a worker request: the working directory equals the host project folder; every argument filled from a request value is relative with no `..` segment; no argument names a path under `<home>/workshop/`. The program, `wsl.exe` and `/usr/bin/env` are the author's absolute paths and are not checked against the folder. Proof: host `workshop_relay_serve.rs::{the_working_directory_is_the_host_project_folder, request_values_are_relative_without_dotdot, no_argument_names_the_workshop_folder}` (the fake tool logs its working directory and arguments).
5. **Run ids** (D-430, review F12). Up to `max_workers` `workshop-relay` threads of one window starting runs in the same millisecond get distinct ids, so `satisfying_check` never reads one run's outcome for another and kept outputs never overwrite each other. Proof: host `workshop_relay_serve.rs::concurrent_runs_get_distinct_ids`.
6. **Stops** (AD-158, AD-34, v0.6 FR-14, D-418, P-73). While a job runs, the thread reads both stop flags every `WAIT_TICK_MS`; a set flag, Stop all in the window and the per-run stop terminate the job; `run_finished` says `stopped` (`RunOutcome` gains it; the TypeScript bindings are regenerated). Proof: host `workshop_relay_serve.rs::{a_flag_set_from_another_process_stops_the_job, stop_all_terminates_the_job, the_per_run_stop_terminates_the_job}`; core `crates/officina-core/src/relay/journal.rs::tests::run_finished_stopped_round_trips`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types` (review F29).
7. **Journal** (AD-160, P-73, D-419). `worker_relay` joins `JournalKind` with outcome `ran` (`workshop_run`, `seq`, `received_ms`, `action`, `params`, `run_id`, `targets_run_id`, `refusals`); `run_started` gains `workshop_run` and `relay_seq` with `session_id` null, and `scope` (`read` or `in`); a targets run carries `resolves`. No line carries an output, a resolved name or a refused value. Proof: core `journal.rs::tests::{worker_relay_round_trips, run_started_for_a_worker_round_trips}`; host `workshop_relay_serve.rs::no_line_carries_an_output_or_a_name` (marker output and marker host name searched in the day's journal).
8. **A worker's dry run counts** (AD-158, AD-33 as amended, R95, amendment 8). A check action run for a worker keeps its output and records the tree (D-170), so a session's after-check or `human` request on the same values and tree is satisfied by it, and the card shows its kept output. Proof: CLI `crates/officina/tests/workshop_relay.rs::a_worker_dry_run_satisfies_a_session_after_check` (the serve API called from the test harness, then `officina run` of the after-check write).
9. **Gate 3, second half** (AD-166 row 3, D-402). `crates/officina-host/examples/worker_relay_probe.rs` (dev build, `window-authority`) runs the scratch actions `agent-probe` and `config-probe` through `serve`; the owner runs it at work with every WSL terminal closed and records exit codes, line counts, the SHA-256 of the `config file =` line and `WSL_DISTRO_NAME`, against the first half's `officina run` values. A difference is a fact recorded with the code that follows it (D-403). Proof: the gate 3 entry (second half).
10. **Zero write** (D-80, D-422). A worker read served with the fake tool leaves the Claude config dir unchanged, and the project repository's `git status --porcelain` and index unchanged. Proof: CLI `crates/officina/tests/quota_doctor.rs::a_worker_read_writes_nothing_under_the_config_dir`; `workshop_relay.rs::a_worker_read_leaves_the_repository_unchanged`.
11. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: worker reads run on the host. README "Relay scope and workers (v0.8, in progress)": where a worker's read runs, under whose identity (gate 3's outcome), what a worker can never make the host run (its own code, a path in its clone, a write), and the exposure note. Proof: review.

## Tasks

- [ ] `crates/officina-host/src/workshop/relay.rs`: `serve`, the ack's duration, the pre-spawn rechecks, `run_child` with `ChildOnly`, timeout, cap, stops (AC 2, 3, 6)
- [ ] `RunOutcome::Stopped`, bindings; `worker_relay` kind with `ran`; `run_started` fields (AC 6, 7)
- [ ] `workshop_relay_serve.rs` with the path containment and run id cases (AC 3 to 7)
- [ ] `workshop_relay.rs` R95 case; zero-write cases (AC 8, 10)
- [ ] `examples/worker_relay_probe.rs`; the owner's run (AC 9)
- [ ] `CHANGELOG.md`, README (AC 11)

## Dev Notes

- Requirements: FR-7 as amended (amendment 8), FR-8's last sentence, PRD section 1 "Where the boundary ends". Decisions: AD-3, AD-21, AD-29, AD-31, AD-33, AD-34, AD-150, AD-153, AD-158, AD-160, AD-166. Gates: the gate 3 entry. Risks: R84, R88, R95, R96. Epic decisions: D-9, D-64, D-80, D-170, D-204; P-70, P-72, P-73, D-400, D-402, D-403, D-407, D-409, D-410, D-418, D-419, D-422, D-430.
- Seams (D-400): host `relay::run::run_child` and its sink (8.5), `run::check_step` and `classes::satisfying_check` (2.6, 2.8), `window-authority` and `AUTHORITY_MARKER` (D-9), story 2.11's HTTP execution, the stop flags and Stop all (2.5, 2.7, 6.17), the per-run stop (6.12), `quota_doctor.rs` (D-80).
- Waits for: the entry condition (AC 1). 8.12 connects this to the mailbox; until then nothing calls `serve` outside tests and the probe.
- The child joins its job right after spawn (D-64's pattern); between the two a grandchild could escape the job (AD-98's accepted race, R96); D-6's inner `timeout` stays the second line. `JobMode::ChildOnly` never puts the window itself in a job: story 2.5's `ProcessTree` does, and terminating it would end the window (review F5).
- Why the ack covers the resolutions (review F16): an ack naming only the action's timeout would let the client give up with 124 while the host still resolves targets and then runs the action.
- "The author's tree" is his working copy now, uncommitted edits included (PRD FR-7): a read sees it, which is why writes are refused, not deferred.
- No employer content: scratch repositories, the fake tool, `lab`, `example.org`.

## Testing

- Core unit: `crates/officina-core/src/relay/journal.rs`; core integration: `crates/officina-core/tests/typescript_bindings.rs`.
- Host integration: `crates/officina-host/tests/workshop_relay_serve.rs`; the no-feature test and CI step.
- CLI integration: `crates/officina/tests/{workshop_relay,quota_doctor}.rs`.
- Manual: the gate 3 entry's second half (`examples/worker_relay_probe.rs`).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the action runs through `run_child` with `ChildOnly` and returns to the window (F5, D-409); concurrent run ids distinct (F12, D-430); every listed write resolved (F13, D-427); the ack's duration covers the resolutions, and floor hash and run state are read again before spawn (F16, D-418); path containment as three checks that can pass (F18, D-418, P-70); bindings named (F29)
