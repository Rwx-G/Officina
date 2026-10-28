# Story 3.9: Contained triage call, runner and dry run (model disabled)

**Status:** review

## Story

As the author who will not let a model touch anything until it has proved itself on my own decisions,
I want a contained `claude -p` call that can only answer a verdict, run one case at a time within a budget, and evaluated on my past confirmations without ever producing a live proposal,
so that I can see how a model would have scored before deciding anything, and pay for nothing I did not ask for.

## Acceptance Criteria

1. Call parts in core (AD-51, FR-10 as amended by amendment 8): `registry::triage_call` builds the argument vector, the JSON schema from `ModelVerdict::ALL`, the stdin prompt (constant instruction, nonce markers, one escaped JSON line) and the environment changes (`TRIAGE_ENV_REMOVED` plus the `OFFICINA_` prefix, the floor's `CLAUDE_CONFIG_DIR`, `NoDefaultCurrentDirectoryInExePath`) applied to a base environment it receives; input containing the nonce is refused. Proof: core `crates/officina-core/src/registry/triage_call.rs::tests::{argv_is_pinned, schema_is_pinned, prompt_is_pinned_for_a_fixed_nonce, input_holding_the_nonce_is_refused, environment_is_an_allow_list_derived_from_the_constants}`; the `prompt` section of `tests/vectors/triage.json` (nonce collision, escaping of markers, quotes, newlines and bidi characters).
2. Output validation (AD-51): only `structured_output` is read, with `deny_unknown_fields`; a reason of 1 to 200 characters without control, bidi or invisible formatting characters; anything else is an outcome (`invalid`, `failed`, `timeout`, `refused`), never a proposal; `usage` and `total_cost_usd` are read permissively. Proof: the `model_output` section (raw stdout in, verdict or error out); `triage_vectors.rs::every_value_has_a_case` now iterates `ModelOutputError::ALL`.
3. Refusals before spawning (AD-51, FR-10, D-87): `claude` resolving to `.cmd` or `.bat`, `no_excerpts` (environment or the window's floor), or a non-empty `<officina home>/triage/empty` refuse with zero launches; each refusal is one `triage_call` row, counted and shown with its cause, spends no budget, and its row is not queued again until the session's inputs change. Proof: CLI-crate tests `crates/officina/tests/triage.rs::{cmd_or_bat_shim_is_refused_with_zero_launches, no_excerpts_is_refused_with_zero_launches, non_empty_workdir_is_refused_with_zero_launches}` (the fake claude's log stays absent); host `crates/officina-host/tests/registry_triage.rs::{a_refused_row_is_not_queued_again_until_its_inputs_change, refusals_spend_no_budget_and_are_counted}`.
4. Containment (AD-51, PRD section 7 Containment, R18, D-64, D-86, D-97): the runner takes its base environment as an argument (the window passes its own); the child gets the golden argument vector, the workdir as cwd, `CREATE_NO_WINDOW`, the excerpts on stdin only (never in argv or the environment), none of the removed variables, the floor's `CLAUDE_CONFIG_DIR`; it runs in its own kill-on-close job assigned after the spawn, is killed at 60 s, and its stdout is capped at 64 KiB while both streams keep draining, so an oversized answer is `invalid`. Proof: `triage.rs::{argv_matches_the_golden_vector, excerpts_reach_the_child_on_stdin_only, only_the_allowed_variables_reach_the_child, floor_config_dir_reaches_the_child, timeout_ends_the_child_and_its_tree, oversized_stdout_is_invalid_not_a_timeout, each_invalid_output_mode_is_an_outcome}` (the third passes a base environment holding every removed name; the `#[cfg(windows)]` tree test uses a fake claude mode that starts a grandchild writing a heartbeat).
5. Fake claude (AD-58 triage row, D-86): `officina-fake-claude` gains mode `triage` (logs argv, environment variable names and stdin; answers the verdict named by a `FAKE-VERDICT:` token in stdin) and invalid-output modes, configured through its sidecar in in-process tests. Proof: `triage.rs::fake_triage_mode_answers_the_named_verdict` and the tests of AC 3, 4.
6. Worker (AD-52, spine amendment to AD-3): a `std::thread` named `triage` with no registry connection takes one job at a time from the collector and returns the outcome on a channel the collector drains with `try_recv` each tick; the board never waits on a call. Proof: CLI-crate collector test `crates/officina/tests/watch.rs::board_keeps_its_pace_while_a_triage_call_hangs`.
7. Budget and counts (AD-52, D-65): one call per 5 s, 60 per hour, 300 per UTC day, held by the collector from `triage_call` rows so it survives restarts; calls, tokens and cost today and over 30 days reach the view. The pause rule is story 3.11's (D-81). Proof: host `registry_triage.rs::{budget_holds_across_a_restart, counts_cover_today_and_30_days}`, at explicit `now_ms`.
8. Dry run (AD-52, D-59, D-67, D-81, D-85, D-88, D-92): `evaluate_triage_model` answers once the run is queued; it runs while the model is disabled or paused, on the confirmations whose `confirmation_input` is still held (under 30 days by construction), whose proposal was the default, newest first, at most 50, one case per call, with progress in the view; results go to `model_evaluation` only (cases, model correct, default correct, model false `finished`, unanswered, tokens and cost) and never become proposals; its failed calls do not count toward the pause. Proof: host `registry_triage.rs::{dry_run_selects_held_default_confirmations_newest_first, dry_run_records_one_evaluation_with_its_cost, dry_run_runs_while_disabled_or_paused, dry_run_failures_do_not_count_toward_the_pause, dry_run_stops_when_the_budget_is_spent, second_dry_run_is_refused_while_one_runs}`; CLI `triage.rs::{dry_run_scores_model_and_default_through_the_fake_claude, dry_run_yields_no_proposal}`.
9. Evidence in the window (FR-10, AD-54, AD-55, journey 4): the Triage panel's `TriageEvidence` part shows the latest dry run (cases, correct counts for model and default, model false `finished`, unanswered, cost), its progress while it runs, the call counts and the refusals with their cause, and an Evaluate button; the model stays disabled (no toggle before story 3.11). `evaluate_triage_model` is declared, allowed, wrapped and mocked. Proof: Playwright `app/e2e/evidence.spec.ts > a dry run shows its cases, correct counts, false finished, unanswered and cost`, `> a running dry run shows its progress`, `> evaluate sends evaluate_triage_model`, `> call counts and refusals are shown`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
10. Retention: `triage_call` rows are kept 90 days (AD-44). Proof: `crates/officina-host/tests/registry_retention.rs::triage_call_rows_are_kept_90_days`.
11. Doctor (AD-53): the model's state (disabled, paused with cause per story 3.11), call counts and cost, and the `.cmd` refusal when `claude` resolves to a shim. Proof: host `crates/officina-host/src/doctor.rs::tests::{cmd_shim_is_reported_as_refusing_triage, model_state_and_counts_are_reported}`.
12. Zero write (AD-58, D-80): Officina itself never writes under the Claude config dir or into `<home>/.claude.json`; a triage call through the fake claude joins the fingerprint test, which now covers both. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
13. **Gate (AD-51, AD-58 manual, D-80, D-91):** `crates/officina-host/examples/triage_probe.rs` runs one real call with the argument vector as Rust quotes it. It passes when the answer is a schema-valid verdict, no transcript exists under the config dir for the session id the call reports (`--no-session-persistence`), and its input tokens are within 20 % of E16's figure; the result, with the Claude Code version, is recorded in `docs/research-notes.md` before this story closes. The `claude` process maintains its own state like any session (E17); the probe does not fingerprint it. Proof: manual, research notes entry.
14. Documentation (D-73, D-80): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": the model is off by default, what the call can and cannot do, that Officina itself writes nothing under the Claude config dir while the `claude` processes it starts keep their own state, the dry run and its cost. Proof: review.

## Tasks

- [x] `crates/officina-core/src/registry/triage_call.rs`: argv, schema, prompt, nonce check, environment on a given base, validation, `ModelOutputError` (AC 1, 2)
- [x] `prompt` and `model_output` sections of the vector set (AC 1, 2)
- [x] Job Object helpers in `platform/windows.rs` per D-64, child assignment; the `Win32_System_JobObjects` feature if story 2.5 has not added it (D-97) (AC 4): story 2.5 had landed both; the runner assigns its child to a kill-on-close job of its own
- [x] `crates/officina-host/src/registry/triage.rs`: refusals and their non-requeue, spawn on a given base environment, deadline, cap with draining, worker thread, collector channel (AC 3, 4, 6)
- [x] Budget and counts from `triage_call` rows (AC 7)
- [x] Dry run selection, progress, `model_evaluation` with cost; `evaluate_triage_model` answering once queued (AC 8, 9)
- [x] Fake claude `triage` and invalid modes, through the sidecar (AC 5)
- [x] `TriageEvidence` component, view fields, `ipc.ts`, mock, `evidence` scenario from `registry_fixture` (AC 9)
- [x] `triage_call` retention; doctor lines; zero-write extension to `<home>/.claude.json` (AC 10-12)
- [x] `examples/triage_probe.rs`; run it; research notes (AC 13): the example is written; the owner ran it on the workstation on 2026-10-28 and E16 records the result (a real call is never made by the developer)
- [x] Update `CHANGELOG.md` and the README (AC 14)

## Dev Notes

- Requirements: FR-10 as amended (amendment 8), journey 4, PRD section 7 Containment as read by D-80. Decisions: AD-3 as amended, AD-6, AD-15 (`NoDefaultCurrentDirectoryInExePath`), AD-51, AD-52, AD-53, AD-57, AD-58. Facts: E5, E16, E17. Risks: R18, R19, R23. Epic decisions: P-15, P-17, D-59, D-64, D-65, D-67, D-80, D-81, D-85 to D-88, D-91, D-92, D-97.
- The pause belongs to story 3.11 (D-81): this story's calls feed it, its dry runs ignore it.
- Every argument is passed as a separate `Command` argument, never through a shell; the empty values of `--tools` and `--setting-sources` are the case the gate of AC 13 checks, since E16 used a shell.
- The prompt never reaches the command line (E16): only stdin carries excerpts, so no excerpt shows in a process list.
- The base environment is an argument (D-86): tests pass one holding `CLAUDE_PID` and the other removed names without touching the process environment.
- `claude::run` stops reading at its cap today; the triage runner must keep draining both streams (D-97), or an oversized answer would block the pipe and end as a `timeout`.
- D-64: never assign the window itself to a job (P-15): attach consoles started by focus must outlive the window.
- One case per call (AD-52 as amended): an excerpt cannot steer another subject's verdict, and the dry run measures what production runs.
- D-80 settles what the probe can and cannot show: `~/.claude.json` changes while any `claude` runs (E17), so it is not the probe's business; Officina's own writes are what the automated fingerprint proves.
- The backlog's Low item on killing a timed-out `claude agents` tree is not this story's: the job here wraps the triage child only.

## Testing

- Core unit: `crates/officina-core/src/registry/triage_call.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host unit and integration: `crates/officina-host/src/doctor.rs`, `crates/officina-host/tests/{registry_triage,registry_retention}.rs`.
- CLI integration: `crates/officina/tests/{triage,watch,quota_doctor}.rs` with a copied `officina-fake-claude`.
- App crate unit: `app/src-tauri/src/main.rs`. Playwright: `app/e2e/evidence.spec.ts`.
- Manual: `examples/triage_probe.rs`, research notes (AC 13).

## Dev Agent Record

### Debug Log

- **Decisions where the story is silent:**
  - AC 1: core `registry::triage_call` also holds the budget, the counts, the D-87 rule (`queue_again`) and the failed proposal streak the pause of story 3.11 reads (`failed_proposal_streak`), since they read the same `triage_call` rows. The instruction names its fields and verdicts from `TriageCase` and `ModelVerdict::ALL` (a test holds the field list to the struct). The data line is `serde_json`'s, then every `<`, `>`, control, line separator and character of `relay::params::FORBIDDEN_FORMAT_RANGES` as a `\u` escape: no excerpt can write a marker, break the line or hide text, and the line still reads back as the case. The nonce check runs on that escaped line, ignoring case, so a nonce formed across an escape is refused too; the host draws again up to eight times. The schema adds `minLength: 1` to AD-51's, matching the validation.
  - AC 1, environment: `triage_env(base, config_dir)` drops `TRIAGE_ENV_REMOVED`, every `OFFICINA_` name (both ignoring case, as Windows does), and the base's own `CLAUDE_CONFIG_DIR` and `NoDefaultCurrentDirectoryInExePath`, then sets the floor's config dir only when the floor names one: an inherited config dir never reaches the call. The host clears the child's environment and sets exactly that.
  - AC 2: the input token count sums `input_tokens` and both cache counts (E16's 4.5k includes the cached system prompt); the cost is stored in millionths of a dollar. `ModelOutputError::ReportedError` (`is_error: true`) maps to `failed`, every other error to `invalid`; a non-zero exit is `failed`.
  - AC 3, D-87: schema 1 (P-14: no released database holds it) gains `triage_call.case_key`, `input_hash` and `refusal` (with its `CHECK` list from `TriageRefusal::ALL`), so a refused case is not queued again across a restart and each refusal shows its cause; the pinned hash moved with it. A refusal ends a dry run, since its cause holds for every case; an explicit dry run never consults the non-requeue rule, which is the proposal queue's (story 3.11): pressing Evaluate after fixing a shim must run. `TriageRefusal` gains `claude_not_found` for a missing binary: nothing starts, so it is a refusal that spends no budget, not a failed call.
  - AC 4: the job comes from story 2.5's `Job` (kill on close), assigned right after the spawn; the window never joins one. Stdout keeps 64 KiB and drains the rest; one byte past the cap is `invalid` (`too_large`) without reading the answer. After the child exits, its job is ended before the output is read, so a process it left cannot hold the pipes.
  - AC 6: the collector creates the worker at start (no thread, no call: the board does not depend on it) and pumps it once per tick: results are recorded, then the next case starts when the thread is idle and the budget allows; a view is emitted when the dry run moved.
  - AC 8: a dry run with no case is refused with a message, like one while another runs (`IpcErrorCode::Refused`); a dry run that ran no case records no evaluation, so the latest one is never hidden by an empty row. How the last dry run ended (completed, budget spent, refused) lives in the collector's memory and reaches the view as `model.last_stop`.
  - AC 9: `TriageEvidence` closes the Triage panel; the progress replaces the Evaluate button while a run is out. The running and stop states are pushed views in Playwright, since a printed fixture has no running dry run.
  - AC 13: `examples/triage_probe.rs` reads the session id from the call's own stdout and looks for a file of that name anywhere under the config dir; it compares the input tokens with E16's 4,500 within 20 %. Not run here: the owner runs it and records the result with the Claude Code version.
- **Adversarial pass** (prompt injection in excerpts: forged markers, the nonce in capitals and across an escape, bidi and tag characters, line separators; oversized, malformed, schema-breaking and error answers; timeouts with a grandchild; budget exhaustion mid-run; a `.cmd` shim; a missing binary; `no_excerpts`; inputs expiring during a run): three real misses, each now tested. A `claude.cmd.` path escaped the shim check while Windows runs it as a batch file (`triage_call.rs::tests::every_spelling_of_a_shim_is_refused`, `tests/triage.rs::cmd_or_bat_shim_is_refused_with_zero_launches`); a path naming no file counted as a failed call that spent budget (`tests/triage.rs::missing_claude_is_refused_with_zero_launches`); a dry run sent a case whose copied input expired after it was queued (`registry_triage.rs::a_case_whose_input_expired_while_queued_is_not_sent`). A changed model answer between dry run and enable is story 3.11's: the enable gate reads the latest dry run, and a dry run's answers are never stored.

### Checks

Per commit: `cargo fmt`, clippy on the workspace with `--all-targets --features officina-core/ts` and on the host without `window-authority`, and the tests of the crates touched; `pnpm check`, `lint`, `test` and the evidence and triage specs for the frontend commits. The full gate runs once at the end of story 3.10 and is recorded there.

### Completion Notes

- AC 1: `crates/officina-core/src/registry/triage_call.rs::tests::{argv_is_pinned, schema_is_pinned, prompt_is_pinned_for_a_fixed_nonce, input_holding_the_nonce_is_refused, environment_is_an_allow_list_derived_from_the_constants}`; the `prompt` section of `tests/vectors/triage.json`.
- AC 2: the `model_output` section; `crates/officina-core/tests/triage_vectors.rs::every_value_has_a_case` iterates `ModelOutputError::ALL` and every answered `ModelVerdict`.
- AC 3: `crates/officina/tests/triage.rs::{cmd_or_bat_shim_is_refused_with_zero_launches, no_excerpts_is_refused_with_zero_launches, non_empty_workdir_is_refused_with_zero_launches, missing_claude_is_refused_with_zero_launches}`; `crates/officina-host/tests/registry_triage.rs::{a_refused_row_is_not_queued_again_until_its_inputs_change, refusals_spend_no_budget_and_are_counted}`.
- AC 4: `triage.rs::{argv_matches_the_golden_vector, excerpts_reach_the_child_on_stdin_only, only_the_allowed_variables_reach_the_child, floor_config_dir_reaches_the_child, timeout_ends_the_child_and_its_tree, oversized_stdout_is_invalid_not_a_timeout, each_invalid_output_mode_is_an_outcome}`.
- AC 5: `triage.rs::fake_triage_mode_answers_the_named_verdict` and the tests of AC 3 and 4.
- AC 6: `crates/officina/tests/watch.rs::board_keeps_its_pace_while_a_triage_call_hangs`.
- AC 7: `registry_triage.rs::{budget_holds_across_a_restart, counts_cover_today_and_30_days}`; core `triage_call.rs::tests::{budget_spaces_calls_and_caps_the_hour_and_the_day, counts_cover_today_the_window_and_refusals}`.
- AC 8: `registry_triage.rs::{dry_run_selects_held_default_confirmations_newest_first, dry_run_records_one_evaluation_with_its_cost, dry_run_runs_while_disabled_or_paused, dry_run_failures_do_not_count_toward_the_pause, dry_run_stops_when_the_budget_is_spent, second_dry_run_is_refused_while_one_runs}`; `triage.rs::{dry_run_scores_model_and_default_through_the_fake_claude, dry_run_yields_no_proposal}`; core `dry_run.rs::tests`.
- AC 9: `app/e2e/evidence.spec.ts > a dry run shows its cases, correct counts, false finished, unanswered and cost`, `> a running dry run shows its progress`, `> evaluate sends evaluate_triage_model`, `> call counts and refusals are shown`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
- AC 10: `crates/officina-host/tests/registry_retention.rs::triage_call_rows_are_kept_90_days`.
- AC 11: `crates/officina-host/src/doctor.rs::tests::{cmd_shim_is_reported_as_refusing_triage, model_state_and_counts_are_reported}`.
- AC 12: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`, a triage call through the fake pointed at the fingerprinted config dir.
- AC 13: proved by the owner's workstation run of `cargo run -p officina-host --features window-authority --example triage_probe` on 2026-10-28 with Claude Code 2.1.312: PASS (schema-valid answer, input tokens 1 % from E16's figure, no transcript for the call's session id); recorded in `docs/research-notes.md` E16.
- AC 14: `CHANGELOG.md` `[Unreleased]`; README "The triage model, and its dry run".

## File List

- `crates/officina-core/src/registry/{triage_call,dry_run,types,proposal,view,retention,mod}.rs`, `crates/officina-core/tests/triage_vectors.rs`, `tests/vectors/triage.json`
- `crates/officina-host/src/registry/{triage,read,write,window,schema,retention,mod}.rs`, `crates/officina-host/src/{watch,collector,doctor}.rs`, `crates/officina-host/Cargo.toml`, `crates/officina-host/examples/{triage_probe,registry_fixture}.rs`, `crates/officina-host/tests/{registry_triage,registry_schema,registry_retention}.rs`
- `crates/officina/src/bin/officina-fake-claude.rs`, `crates/officina/tests/{triage,watch,quota_doctor}.rs`
- `app/src-tauri/{build.rs,capabilities/default.json,src/main.rs}`, `app/src/lib/generated/` (regenerated), `app/src/lib/{ipc.ts,registry.svelte.ts,registry-text.ts}`, `app/src/{e2e-mock.ts,e2e-globals.d.ts}`, `app/src/components/{TriageEvidence,TriagePanel}.svelte`, `app/src/components/components.test.ts`, `app/scripts/e2e-fixtures.mjs`, `app/e2e/{fixtures.ts,evidence.spec.ts}`
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.9-triage-call-and-dry-run.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: zero write is Officina's own, probe checks the transcript and the answer with a pass criterion (D-80, D-91); pause moved to story 3.11, dry runs run while disabled or paused (D-81); refusals not re-queued until inputs change (D-87); dry-run cost columns (D-88); evaluation set held under 30 days (D-85); base environment as an argument and fake claude sidecar (D-86); draining past the cap, job feature (D-97); answer once queued with progress (D-92)
- 2026-10-28 implemented: the contained call in core and its vectors, the runner, the `triage` thread, the budget and counts, the dry run and its evidence in the panel, retention, doctor, zero write, the probe written; three adversarial tests added. Status review; AC 13 (the probe's real call) open for the owner
- 2026-10-28 AC 13 proved: the owner ran the probe on the workstation (Claude Code 2.1.312, PASS); E16 records it. Every acceptance criterion is now proved
- 2026-10-28 after the Epic 3 audits: the environment became an allow list (D-439), so AC 1 and AC 4 cite the tests renamed with it; the shim refusal now covers every `claude` call (D-436)
