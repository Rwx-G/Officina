# Story 6.12: Following a run, its limits and restart reconciliation

**Status:** Review

## Story

As the author whose worker runs unattended under the engine, with or without the window open,
I want the window to read the worker's and the gateway's output with bounds, keep only the facts it needs as untrusted data, stop a run on its deadline or on a sign it cannot be trusted to continue, and find every run again after a restart,
so that no line a container prints can keep a run going or mark it done, and a run the engine lost reads unknown, never finished.

## Acceptance Criteria

1. **States and transitions** (AD-109). `RunState` (story 6.4) gets its transitions in core, `run::transition(state, event) -> Result<RunState, _>`, with a vector case per allowed edge and a refusal for every other pair. Proof: section `run` of `tests/vectors/workshop.json` through `crates/officina-core/tests/workshop_vectors.rs::{every_allowed_edge_has_a_case, every_other_pair_is_refused}` (the second iterates `RunState::ALL` squared).
2. **The stream fold** (AD-109, FR-8 as amended by amendment 7, the gate 5 entry). `stream::fold(&mut StreamFacts, &StreamLine)` parses permissively the fields of `STREAM_FIELDS`, the canary constants the gate 5 entry recorded (AD-12): the first `system/init` only (model, tools, MCP servers); hook events counted per event and outcome; `system/api_retry` error categories and the run of consecutive `authentication_failed`; the last `result` (subtype, error flag, cost, turns, duration, session id), display only; the last 4 KiB of assistant text as `Untrusted<String>`. Non-JSON lines and lines over 1 MiB are counted and skipped. Every text kept is untrusted (D-130); the session id is never used to resume or address anything. Proof: section `fold` of `tests/vectors/stream.json` (formatter-exempt; real lines from the gate 5 entry's saved stream, values replaced by placeholders) through `crates/officina-core/tests/stream_vectors.rs::{fold_cases_match, a_second_init_is_ignored, a_forged_result_changes_no_outcome, every_stream_field_is_read_by_a_case}` (the last iterates `STREAM_FIELDS`).
3. **What the stream may do** (AD-109, R50). The stream can only stop a run sooner: `unexpected_tool` (a name of `WORKER_DISALLOWED_TOOLS` in the first init's tools), `hook_failed` (a hook event with exit code 126 or 127; built only if the gate 5 entry found exit codes, D-203), `auth_failed` (three `authentication_failed` in a row, plus any other shape the login entry recorded for an expired or revoked login or an invalid token, such as a first `result` with the error flag and no retry, read through `STREAM_FIELDS`, D-607, D-610). No limit relies on the stream to let a run continue. Proof: core `crates/officina-core/src/workshop/stream.rs::tests::{a_disallowed_tool_stops, exit_126_and_127_stop, three_auth_failures_in_a_row_stop, interleaved_success_resets_the_auth_run}`; section `stops` of `stream.json`, with the login entry's revoked-login and invalid-token streams (values replaced) as `auth_failed` cases.
4. **Followers** (AD-109, D-216, D-223). Per live container (worker and gateway), one `docker logs --follow --timestamps` child in a kill-on-close job, two reader threads with a line reader bounded at 1 MiB + 1 bytes that discards to the next newline; lines go to the collector with their stream. The entry point's stderr lines (`EntryLine`) and the gateway's lines (`GatewayLine`) are parsed with their own core types, each at most 4 KiB; per-host allowed counts keep 256 hosts, then `other`; each `refused` line is journaled `gateway_refused`. At window start, or when a follower ends while its container runs, it restarts from the beginning and the run's facts are rebuilt from nothing; no clock is folded from a replayed line (the skew is measured at launch, D-242, review F21). Fake engine mode: `logs --follow` replays fixture streams per role, gate 3's saved stream among them (D-223). Proof: CLI `crates/officina/tests/workshop_follow.rs::{facts_rebuild_identically_after_a_restart, an_oversized_line_is_skipped_and_counted, a_flood_of_allowed_wildcard_hosts_keeps_256_entries, each_refusal_is_journaled_once}`.
5. **The credential never leaves through a line** (D-230, review F2). Every untrusted text of a run goes through `scan::replace` with the run's needles (story 6.11) before it is journaled, stored or emitted: refused hosts and wildcard-allowed hosts, the report tail, stream texts and entry lines. At reconciliation the needles are rebuilt from a fresh credential read; when none can be built (the credential absent, or always for a `login` run, whose login the host never reads, D-609), the run's host names and texts are held in memory, bounded by D-216, and journaled, stored or emitted only after collection's scan (story 6.14). Proof: CLI `workshop_follow.rs::{a_token_in_a_refused_host_is_replaced_before_the_journal, a_token_in_a_wildcard_allowed_host_is_replaced_before_the_view, a_token_in_a_mid_run_report_tail_is_replaced_before_the_view, without_a_credential_hosts_wait_for_collection, a_login_run_holds_its_texts_until_collection}` (needle forms derived from a seeded value; the journal and every serialized `WorkshopView` and `Board` searched).
6. **Limits and causes** (AD-109, FR-8). `FinishCause` (a `closed_enum!`, `hook_failed` present only per the gate 5 entry) is decided by core from the engine's exit code and `OOMKilled`, the window's stop reason, the gateway's exit code and the result subtype (`run::finish_cause`), with a vector case per variant. The deadline: the window stops the worker at the deadline plus 60 s, at window start too; the output cap: 64 MiB of stdout and stderr counted by the follower (`output_cap`); stopping one run is `docker stop` of the worker, the reason recorded, collection next (story 6.13); no flag is set. `ENVIRONMENT_FAILURE_CAUSES` and `view::is_environment_failure` follow D-217. Proof: section `finish` of `workshop.json` through `workshop_vectors.rs::every_finish_cause_has_a_case` (iterates `FinishCause::ALL`); CLI `workshop_follow.rs::{the_deadline_stops_the_worker, the_output_cap_stops_the_worker, a_stream_stop_stops_the_worker, a_limit_sets_no_flag}`; core `crates/officina-core/src/workshop/view.rs::tests::environment_failures_follow_d217`.
7. **Reconciliation** (AD-109, FR-8, principle 3, D-216). Core's `run::reconcile(rows, containers, now_ms)` returns actions: a running row with its container running is followed (and stopped if past its deadline plus 60 s); with its container exited, followed to the end, then collected; a `launching` row older than 2 minutes becomes `failed` and its objects are removed; a row whose container the engine no longer knows, with no package on the host, becomes `unknown` (`engine_lost`); a labelled container of this floor with no row is an orphan, listed, removable on a click (D-215), never adopted; only the run roles (`Worker`, `Gateway`, `Packer`) are judged so: a job's containers (`Inventory`, `Battery`, `BatteryGateway`, `Decoy`, `AuthProbe`, `Scanner`) belong to their job, and the owner's own login container (`officina.role=login`, D-605) to him, so a login in progress is never listed as an orphan (Stop all still stops it). An unreachable engine changes no row: thread `workshop` asks `version` every 30 s while any run is not terminal and reconciles on the first answer. Proof: section `reconcile` of `workshop.json` through `workshop_vectors.rs::reconcile_cases_match`; CLI `workshop_follow.rs::{a_window_restart_finds_and_follows_a_live_run, an_engine_down_at_start_changes_no_row, an_engine_back_reconciles, a_lost_container_is_unknown_never_finished, an_orphan_is_listed_not_adopted, job_and_login_containers_are_never_orphans}` (the last iterates `ContainerRole::ALL`).
8. **Documentation** (D-226). README: the limits, that stream lines are untrusted and can only stop a run, that the window may close while a run goes on, and that the first-init check is best-effort after log rotation: with the `local` driver's 3 x 32 MiB, a worker writing over 96 MiB while the window is closed rotates the real `system/init` away, so the first one read after a restart can be forged, while a followed run meets `output_cap` first (review F55); `CHANGELOG.md`. Proof: review.

## Tasks

- [x] `crates/officina-core/src/workshop/{run,stream}.rs`: transitions, fold, `STREAM_FIELDS` from the gate 5 entry, stops, `FinishCause`, reconcile; `tests/vectors/stream.json`, `stream_vectors.rs`; `run`, `finish` and `reconcile` sections of `workshop.json` (AC 1 to 3, 6, 7)
- [x] `crates/officina-host/src/workshop/follow.rs`: followers, bounded readers, deadline and cap stops, journal of refusals; fake engine `logs` mode (AC 4, 6)
- [x] Reconciliation at start and after the engine returns; orphans; needles rebuilt (AC 5, 7)
- [x] `scan::replace` on every untrusted text before journal, store or emit; holding when no needles (AC 5)
- [x] `ENVIRONMENT_FAILURE_CAUSES`, `is_environment_failure` (AC 6); README (AC 8)

## Dev Notes

- Requirements: FR-8 as amended (amendment 7), FR-4 (refusal lines), PRD section 7 Environment failures. Decisions: AD-12, AD-98 (followers, threads), AD-109, AD-120 (`gateway_refused`). Facts: the gate 5 entry (fields, exit codes). Risks: R50, R57. Epic decisions: D-130 (Epic 5); P-50, D-200, D-203, D-215, D-216, D-217, D-223, D-230, D-242.
- Seams (D-200): the collector's `AuthorRequest` and state (3.5, 5.7); `untrusted` (5.3); `JournalKind` (2.5).
- Waits for: the gate 5 entry; story 6.11 merged.
- Why replace at the source (D-230, review F2): the journal is append-only (AD-32), so a worker sending `CONNECT` to a host name carrying the token split into labels would write it to disk before collection's scan, and "replaced in every text Officina stores" (AD-112) could not be honoured for that line.
- Why followers re-read from the start (AD-109): the fold is deterministic, so no line counts twice; re-reading at most 96 MiB once per restart is simpler than a cursor kept exact across log rotation.
- Any process in the container runs as the entry point's uid and can write to its stdout: only the first `system/init` counts, `result` is display only, and the stream can only shorten a run (R50).
- The `hook_failed` variant and its cases exist only if the gate 5 entry recorded exit codes (D-203); otherwise `hook_program_missing` (story 6.9) is the only hook check, and this story's README says so.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/{run,stream,view}.rs`, `crates/officina-core/tests/{workshop_vectors,stream_vectors}.rs`; CLI with the fake engine `crates/officina/tests/workshop_follow.rs`.
- Workstation: exercised by gate 3's window half (story 6.19) and gate 4's (story 6.20).

## Dev Agent Record

### Debug Log

- 2026-11-02, first agent: core's half on `wip/story-6.12` (`workshop::lifecycle`, `workshop::stream`, the `run`, `finish` and `reconcile` sections of `workshop.json`, `stream.json`).
- 2026-11-02, second agent: the host's half. The note to confirm `unexpected_tool` at gate 3's window half is closed by the lead's verified fact (research notes E101, D-1166): a denied tool is absent from the first init, which the fold finds by subtype after any hook lines.
- The fold runs beside the followers rather than in the collector (D-1160); a follower's end is judged by inspecting its container (D-1163).

### Completion Notes

- AC 1: `workshop_vectors.rs::{every_allowed_edge_has_a_case, every_other_pair_is_refused}` (first agent).
- AC 2: `stream_vectors.rs::{fold_cases_match, a_second_init_is_ignored, a_forged_result_changes_no_outcome, every_stream_field_is_read_by_a_case}` (first agent).
- AC 3: `stream.rs::tests::{a_disallowed_tool_stops, exit_126_and_127_stop, three_auth_failures_in_a_row_stop, interleaved_success_resets_the_auth_run}`; `stream_vectors.rs::stop_cases_match` with the invalid-token stream. `hook_failed` is built: the gate 5 entry (E96) found exit codes.
- AC 4: `workshop_follow.rs::{facts_rebuild_identically_after_a_restart, an_oversized_line_is_skipped_and_counted, a_flood_of_allowed_wildcard_hosts_keeps_256_entries, each_refusal_is_journaled_once}`; core `output.rs::tests::{allowed_hosts_are_bounded, a_replay_ranks_refusals_the_same}`; host `process.rs::line_tests::a_long_line_is_skipped_to_its_newline`. The fake engine's `logs --follow` replays sidecar lines per role (`follow`, `follow_cut_first`). Gate 3's saved stream was not kept as a file; the cases build their lines from the gate entries' shapes.
- AC 5: `workshop_follow.rs::{a_token_in_a_refused_host_is_replaced_before_the_journal, a_token_in_a_wildcard_allowed_host_is_replaced_before_the_view, a_token_in_a_mid_run_report_tail_is_replaced_before_the_view, without_a_credential_hosts_wait_for_collection}`; host `follow.rs::tests::{a_login_run_holds_its_texts_until_collection, a_refusal_is_replaced_and_journaled_once}` (a `login` run cannot launch before 6.26); core `output.rs::tests::texts_are_replaced`. `WorkshopView` lands in 6.18: "the view" here is `Followed::texts`, which it will read, and the stored `RunTexts`.
- AC 6: `workshop_vectors.rs::every_finish_cause_has_a_case`; `workshop_follow.rs::{the_deadline_stops_the_worker, the_output_cap_stops_the_worker, a_stream_stop_stops_the_worker, a_limit_sets_no_flag}`; `lifecycle.rs::tests::environment_failures_follow_d217`; core `output.rs::tests::the_output_cap_stops`.
- AC 7: `workshop_vectors.rs::reconcile_cases_match`; `workshop_follow.rs::{a_window_restart_finds_and_follows_a_live_run, an_engine_down_at_start_changes_no_row, an_engine_back_reconciles, a_lost_container_is_unknown_never_finished, an_orphan_is_listed_not_adopted, job_and_login_containers_are_never_orphans}`; `journal.rs::tests::workshop_finished_round_trips` (D-249).
- AC 8: README "Following a run and its limits", its numbers held by `readme.rs::readme_states_the_run_limits`; CHANGELOG.

## File List

- `crates/officina-core/src/workshop/{lifecycle,stream,output,run,mod}.rs`, `crates/officina-core/src/relay/{journal,view}.rs`, `crates/officina-core/src/untrusted.rs`
- `crates/officina-core/tests/{workshop_vectors,stream_vectors}.rs`, `tests/vectors/{workshop,stream}.json`
- `crates/officina-host/src/workshop/{follow,engine,launch,mod}.rs`, `crates/officina-host/src/workshop/launch/window.rs`, `crates/officina-host/src/{process,collector,watch}.rs`, `crates/officina-host/src/registry/{window,workshop}.rs`
- `crates/officina/src/bin/officina-fake-docker.rs`, `crates/officina/tests/{workshop_follow,workshop_launch,readme}.rs`, `crates/officina/tests/common/workshop.rs`
- `app/src-tauri/src/main.rs`
- `README.md`, `CHANGELOG.md`, `docs/research-notes.md` (E101), `docs/stories/epic-6-decisions.md` (D-1160 to D-1166), `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: every untrusted text replaced with the run's needles before journal, store or emit, held when no needles exist (F2); no clock folded from replayed lines (F21); the first-init check's limit after rotation stated (F55); gate 3's window half moved to 6.19 (F5)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
- 2026-10-29 the owner's answers: `auth_failed` also reads the login entry's failure shapes; a `login` run always holds its texts until collection (D-607, D-609, D-610)
- 2026-10-29 adversarial pass: only run roles can be orphans; job containers and the owner's login container never are (D-605)
- 2026-11-02 implemented: followers, limits and reconciliation on the host; E101 recorded; D-1160 to D-1166
