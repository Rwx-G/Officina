# Story 3.9: Contained triage call, runner and dry run (model disabled)

**Status:** Approved

## Story

As the author who will not let a model touch anything until it has proved itself on my own decisions,
I want a contained `claude -p` call that can only answer a verdict, run one case at a time within a budget, and evaluated on my past confirmations without ever producing a live proposal,
so that I can see how a model would have scored before deciding anything, and pay for nothing I did not ask for.

## Acceptance Criteria

1. Call parts in core (AD-51, FR-10 as amended by amendment 8): `registry::triage_call` builds the argument vector, the JSON schema from `ModelVerdict::ALL`, the stdin prompt (constant instruction, nonce markers, one escaped JSON line) and the environment changes (`TRIAGE_ENV_REMOVED` plus the `OFFICINA_` prefix, the floor's `CLAUDE_CONFIG_DIR`, `NoDefaultCurrentDirectoryInExePath`); input containing the nonce is refused. Proof: core `crates/officina-core/src/registry/triage_call.rs::tests::{argv_is_pinned, schema_is_pinned, prompt_is_pinned_for_a_fixed_nonce, input_holding_the_nonce_is_refused, environment_changes_are_derived_from_the_constant}`; the `prompt` section of `tests/vectors/triage.json` (nonce collision, escaping of markers, quotes, newlines and bidi characters).
2. Output validation (AD-51): only `structured_output` is read, with `deny_unknown_fields`; a reason of 1 to 200 characters without control, bidi or invisible formatting characters; anything else is an outcome (`invalid`, `failed`, `timeout`, `refused`), never a proposal; `usage` and `total_cost_usd` are read permissively. Proof: the `model_output` section (raw stdout in, verdict or error out); `triage_vectors.rs::every_value_has_a_case` now iterates `ModelOutputError::ALL`.
3. Refusals before spawning (AD-51, FR-10): `claude` resolving to `.cmd` or `.bat`, `no_excerpts` (environment or the window's floor), or a non-empty `<officina home>/triage/empty` refuse with zero launches. Proof: CLI-crate tests `crates/officina/tests/triage.rs::{cmd_or_bat_shim_is_refused_with_zero_launches, no_excerpts_is_refused_with_zero_launches, non_empty_workdir_is_refused_with_zero_launches}` (the fake claude's log stays absent).
4. Containment (AD-51, PRD section 7 Containment, R18): the child gets the golden argument vector, the workdir as cwd, `CREATE_NO_WINDOW`, the excerpts on stdin only (never in argv or the environment), none of the removed variables, the floor's `CLAUDE_CONFIG_DIR`; it runs in its own kill-on-close job assigned after the spawn (D-64), is killed at 60 s and its stdout is capped at 64 KiB. Proof: `triage.rs::{argv_matches_the_golden_vector, excerpts_reach_the_child_on_stdin_only, removed_variables_do_not_reach_the_child, floor_config_dir_reaches_the_child, timeout_ends_the_child_and_its_tree, oversized_stdout_is_invalid, each_invalid_output_mode_is_an_outcome}`; the `#[cfg(windows)]` tree test uses a fake claude mode that starts a grandchild writing a heartbeat.
5. Fake claude (AD-58 triage row): `officina-fake-claude` gains mode `triage` (logs argv, environment variable names and stdin; answers the verdict named by a `FAKE-VERDICT:` token in stdin) and invalid-output modes. Proof: `triage.rs::fake_triage_mode_answers_the_named_verdict` and the tests of AC 3, 4.
6. Worker (AD-52, spine amendment to AD-3): a `std::thread` named `triage` with no registry connection takes one job at a time from the collector and returns the outcome on a channel the collector drains with `try_recv` each tick; the board never waits on a call. Proof: CLI-crate collector test `crates/officina/tests/watch.rs::board_keeps_its_pace_while_a_triage_call_hangs`.
7. Budget, pause and counts (AD-50 last bullet, AD-52, D-65, D-66): one call per 5 s, 60 per hour, 300 per UTC day, held by the collector from `triage_call` rows so it survives restarts; refusals spend nothing; three failed calls in a row pause the model until the binary's stamp changes or it is re-enabled; calls, tokens and cost today and over 30 days reach the view. Proof: host `crates/officina-host/tests/registry_triage.rs::{budget_holds_across_a_restart, refusals_spend_no_budget, three_failures_pause_until_the_binary_changes, counts_cover_today_and_30_days}`, all at explicit `now_ms`.
8. Dry run (AD-52, D-59, D-67): `evaluate_triage_model` takes the confirmations of the last 28 days with inputs whose proposal was the default, newest first, at most 50, one case per call; results go to `model_evaluation` only (cases, model correct, default correct, model false `finished`, unanswered) and never become proposals. Proof: host `registry_triage.rs::{dry_run_selects_default_confirmations_of_28_days_newest_first, dry_run_records_one_evaluation, dry_run_stops_when_the_budget_is_spent, second_dry_run_is_refused_while_one_runs}`; CLI `triage.rs::{dry_run_scores_model_and_default_through_the_fake_claude, dry_run_yields_no_proposal}`.
9. Evidence in the window (FR-10, AD-54, AD-55, journey 4): the Triage panel's `TriageEvidence` part shows the latest dry run (cases, correct counts for model and default, model false `finished`, unanswered, cost), the call counts and an Evaluate button; the model stays disabled (no toggle before story 3.11). `evaluate_triage_model` is declared, allowed, wrapped and mocked. Proof: Playwright `app/e2e/evidence.spec.ts > a dry run shows its cases, correct counts, false finished, unanswered and cost`, `> evaluate sends evaluate_triage_model`, `> call counts for today and 30 days are shown`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
10. Retention: `triage_call` rows are kept 90 days (AD-44). Proof: `crates/officina-host/tests/registry_retention.rs::triage_call_rows_are_kept_90_days`.
11. Doctor (AD-53): the model's state (disabled, paused with cause), call counts and cost, and the `.cmd` refusal when `claude` resolves to a shim. Proof: host `crates/officina-host/src/doctor.rs::tests::{cmd_shim_is_reported_as_refusing_triage, model_state_and_counts_are_reported}`.
12. Zero write (AD-58): a triage call through the fake claude joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
13. **Gate (AD-51, AD-58 manual):** `crates/officina-host/examples/triage_probe.rs` runs one real call with the argument vector as Rust quotes it, fingerprints the config dir before and after (unchanged), and compares input tokens with E16; the result, with the Claude Code version, is recorded in `docs/research-notes.md` before this story closes. Proof: manual, research notes entry.
14. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": the model is off by default, what the call can and cannot do, the dry run and its cost. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/registry/triage_call.rs`: argv, schema, prompt, nonce check, environment, validation, `ModelOutputError` (AC 1, 2)
- [ ] `prompt` and `model_output` sections of the vector set (AC 1, 2)
- [ ] Job Object helpers in `platform/windows.rs` per D-64, child assignment (AC 4)
- [ ] `crates/officina-host/src/registry/triage.rs`: refusals, spawn, deadline, cap, worker thread, collector channel (AC 3, 4, 6)
- [ ] Budget, pause with binary stamp, counts from `triage_call` rows (AC 7)
- [ ] Dry run selection and `model_evaluation`; `evaluate_triage_model` (AC 8, 9)
- [ ] Fake claude `triage` and invalid modes (AC 5)
- [ ] `TriageEvidence` component, view fields, `ipc.ts`, mock, `evidence` scenario from `registry_fixture` (AC 9)
- [ ] `triage_call` retention; doctor lines; zero-write extension (AC 10-12)
- [ ] `examples/triage_probe.rs`; run it; research notes (AC 13)
- [ ] Update `CHANGELOG.md` and the README (AC 14)

## Dev Notes

- Requirements: FR-10 as amended (amendment 8), journey 4, PRD section 7 Containment. Decisions: AD-3 as amended, AD-6, AD-15 (`NoDefaultCurrentDirectoryInExePath`), AD-50 (pause), AD-51, AD-52, AD-53, AD-57, AD-58. Facts: E5, E16. Risks: R18, R19, R23. Epic decisions: P-15, D-59, D-64 to D-67, D-72.
- Every argument is passed as a separate `Command` argument, never through a shell; the empty values of `--tools` and `--setting-sources` are the case the gate of AC 13 checks, since E16 used a shell.
- The prompt never reaches the command line (E16): only stdin carries excerpts, so no excerpt shows in a process list.
- D-64: never assign the window itself to a job (P-15): attach consoles started by focus must outlive the window.
- One case per call (AD-52 as amended): an excerpt cannot steer another subject's verdict, and the dry run measures what production runs.
- The fake claude never needs network or credentials; the probe of AC 13 is the only real call, run by hand.
- The backlog's Low item on killing a timed-out `claude agents` tree is not this story's: the job here wraps the triage child only.

## Testing

- Core unit: `crates/officina-core/src/registry/triage_call.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host unit and integration: `crates/officina-host/src/doctor.rs`, `crates/officina-host/tests/{registry_triage,registry_retention}.rs`.
- CLI integration: `crates/officina/tests/{triage,watch,quota_doctor}.rs` with `officina-fake-claude`.
- App crate unit: `app/src-tauri/src/main.rs`. Playwright: `app/e2e/evidence.spec.ts`.
- Manual: `examples/triage_probe.rs`, research notes (AC 13).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
