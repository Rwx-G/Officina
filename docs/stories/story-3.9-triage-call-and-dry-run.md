# Story 3.9: Contained triage call, runner and dry run (model disabled)

**Status:** Approved

## Story

As the author who will not let a model touch anything until it has proved itself on my own decisions,
I want a contained `claude -p` call that can only answer a verdict, run one case at a time within a budget, and evaluated on my past confirmations without ever producing a live proposal,
so that I can see how a model would have scored before deciding anything, and pay for nothing I did not ask for.

## Acceptance Criteria

1. Call parts in core (AD-51, FR-10 as amended by amendment 8): `registry::triage_call` builds the argument vector, the JSON schema from `ModelVerdict::ALL`, the stdin prompt (constant instruction, nonce markers, one escaped JSON line) and the environment changes (`TRIAGE_ENV_REMOVED` plus the `OFFICINA_` prefix, the floor's `CLAUDE_CONFIG_DIR`, `NoDefaultCurrentDirectoryInExePath`) applied to a base environment it receives; input containing the nonce is refused. Proof: core `crates/officina-core/src/registry/triage_call.rs::tests::{argv_is_pinned, schema_is_pinned, prompt_is_pinned_for_a_fixed_nonce, input_holding_the_nonce_is_refused, environment_changes_are_derived_from_the_constant}`; the `prompt` section of `tests/vectors/triage.json` (nonce collision, escaping of markers, quotes, newlines and bidi characters).
2. Output validation (AD-51): only `structured_output` is read, with `deny_unknown_fields`; a reason of 1 to 200 characters without control, bidi or invisible formatting characters; anything else is an outcome (`invalid`, `failed`, `timeout`, `refused`), never a proposal; `usage` and `total_cost_usd` are read permissively. Proof: the `model_output` section (raw stdout in, verdict or error out); `triage_vectors.rs::every_value_has_a_case` now iterates `ModelOutputError::ALL`.
3. Refusals before spawning (AD-51, FR-10, D-87): `claude` resolving to `.cmd` or `.bat`, `no_excerpts` (environment or the window's floor), or a non-empty `<officina home>/triage/empty` refuse with zero launches; each refusal is one `triage_call` row, counted and shown with its cause, spends no budget, and its row is not queued again until the session's inputs change. Proof: CLI-crate tests `crates/officina/tests/triage.rs::{cmd_or_bat_shim_is_refused_with_zero_launches, no_excerpts_is_refused_with_zero_launches, non_empty_workdir_is_refused_with_zero_launches}` (the fake claude's log stays absent); host `crates/officina-host/tests/registry_triage.rs::{a_refused_row_is_not_queued_again_until_its_inputs_change, refusals_spend_no_budget_and_are_counted}`.
4. Containment (AD-51, PRD section 7 Containment, R18, D-64, D-86, D-97): the runner takes its base environment as an argument (the window passes its own); the child gets the golden argument vector, the workdir as cwd, `CREATE_NO_WINDOW`, the excerpts on stdin only (never in argv or the environment), none of the removed variables, the floor's `CLAUDE_CONFIG_DIR`; it runs in its own kill-on-close job assigned after the spawn, is killed at 60 s, and its stdout is capped at 64 KiB while both streams keep draining, so an oversized answer is `invalid`. Proof: `triage.rs::{argv_matches_the_golden_vector, excerpts_reach_the_child_on_stdin_only, removed_variables_do_not_reach_the_child, floor_config_dir_reaches_the_child, timeout_ends_the_child_and_its_tree, oversized_stdout_is_invalid_not_a_timeout, each_invalid_output_mode_is_an_outcome}` (the third passes a base environment holding every removed name; the `#[cfg(windows)]` tree test uses a fake claude mode that starts a grandchild writing a heartbeat).
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

- [ ] `crates/officina-core/src/registry/triage_call.rs`: argv, schema, prompt, nonce check, environment on a given base, validation, `ModelOutputError` (AC 1, 2)
- [ ] `prompt` and `model_output` sections of the vector set (AC 1, 2)
- [ ] Job Object helpers in `platform/windows.rs` per D-64, child assignment; the `Win32_System_JobObjects` feature if story 2.5 has not added it (D-97) (AC 4)
- [ ] `crates/officina-host/src/registry/triage.rs`: refusals and their non-requeue, spawn on a given base environment, deadline, cap with draining, worker thread, collector channel (AC 3, 4, 6)
- [ ] Budget and counts from `triage_call` rows (AC 7)
- [ ] Dry run selection, progress, `model_evaluation` with cost; `evaluate_triage_model` answering once queued (AC 8, 9)
- [ ] Fake claude `triage` and invalid modes, through the sidecar (AC 5)
- [ ] `TriageEvidence` component, view fields, `ipc.ts`, mock, `evidence` scenario from `registry_fixture` (AC 9)
- [ ] `triage_call` retention; doctor lines; zero-write extension to `<home>/.claude.json` (AC 10-12)
- [ ] `examples/triage_probe.rs`; run it; research notes (AC 13)
- [ ] Update `CHANGELOG.md` and the README (AC 14)

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

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: zero write is Officina's own, probe checks the transcript and the answer with a pass criterion (D-80, D-91); pause moved to story 3.11, dry runs run while disabled or paused (D-81); refusals not re-queued until inputs change (D-87); dry-run cost columns (D-88); evaluation set held under 30 days (D-85); base environment as an argument and fake claude sidecar (D-86); draining past the cap, job feature (D-97); answer once queued with progress (D-92)
