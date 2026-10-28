# Story 5.13: The review call: causes, prompt, schema, worker and blind shadow

**Status:** Approved

## Story

As the author who wants doubtful actions to come to me and routine ones, some day, to run without me,
I want every pending `auto` request reviewed by a contained model call that can only answer approve, reject or escalate, that escalates without any call whenever a fail-closed cause holds, and that runs blind in shadow while auto approval is off,
so that evidence about the review accumulates on my own decisions without the review ever approving anything or anchoring me (J3, FR-7 to FR-9).

## Acceptance Criteria

1. **Fail-closed precheck** (AD-87, FR-8 as amended by amendment 5, D-140, D-142; review F15). `officina-core::relay::auto::precheck` returns `Escalate(EscalationCause)` or `Review`, checking in order: `claude_shim` (anything but an `.exe`, D-128); `workdir_not_empty`; `no_session_id`; `no_excerpts`; `quota_high` (the latest known 5-hour `used_percentage` at or above 90, stale data included, no data not blocking); `hourly_cap` (review `approve` answers for that project and action in the last 3,600 s reached `auto_max_per_hour`, whatever their mode); `budget_spent` (60 calls per hour and 300 per UTC day, counted from rows with a call; the 5 s spacing is the worker's pacing, AC 4, never an escalation); `review_config_invalid`. After a call: `call_failed`, `call_timeout` (60 s), `invalid_answer`; then `review_rejected`, `review_escalated`, `auto_approval_off`. `EscalationCause` is a `closed_enum!`. Whether the orchestrator is open plays no part; a stopped relay refuses as today. Proof: the `precheck` section of `tests/vectors/auto.json` through `crates/officina-core/tests/auto_vectors.rs::{precheck_cases_match, causes_are_checked_in_their_order, every_cause_has_a_case}` (the last iterates `EscalationCause::ALL`; a case with two requests one second apart, the second not escalated).
2. **Prompt and schema** (AD-86, AD-51's rules, FR-7). Core builds the stdin prompt from the constant `auto::REVIEW_INSTRUCTION`, nonce markers and one escaped JSON line holding the request as core rebuilt it from the approved catalogue (action, description, project, values, argument vector; HTTP actions cannot be `auto`, D-156), the subject's name, the session's excerpts as `Untrusted`, the last 10 outcomes of that action in that project and the check's status; input holding the nonce is refused and the host draws again. The schema is generated from `ReviewDecision::ALL`: `decision` and `reason` (1 to 200 characters, AD-51's character rules), `additionalProperties: false`. Only `structured_output` is read, with `deny_unknown_fields`; anything else is `invalid_answer`. Proof: core `crates/officina-core/src/relay/auto.rs::tests::{prompt_is_pinned_for_a_fixed_nonce, schema_is_pinned, input_holding_the_nonce_is_refused}`; the `prompt` and `output` sections of `auto.json` through `auto_vectors.rs::{prompt_cases_match, output_cases_match}`.
3. **The `ReviewCall` vector** (AD-79, AD-51 as amended by section 3, P-23, D-128). `VectorKind` gains `ReviewCall`: AD-51's flags with the floor's review model (`ModelAlias`, default `haiku`), the review schema and `--settings {"crossSessionInbound":"refuse"}`; working directory `<home>/review/empty`; no console, inside its own kill-on-close job; the data on stdin; stdout 64 KiB; 60 s; only an `.exe` accepted; D-128's contained-kind environment: `SESSION_ENV_REMOVED`, every `OFFICINA_*` name and `CLAUDE_CODE_DISABLE_AGENT_VIEW` removed. Proof: the `ReviewCall` case of `tests/vectors/spawn.json`; CLI `crates/officina/tests/spawn.rs::{every_kind_reaches_the_fake_claude_as_its_golden_vector, contained_kinds_drop_the_officina_prefix}`.
4. **The worker** (AD-86, AD-52 as amended, AD-3 as amended, D-142; review F15). `officina-host::relay::review` (`window-authority`) runs a `std::thread` named `review`, separate from `triage`, taking one job at a time from the collector and returning the outcome on a channel the collector drains with `try_recv`; the queue runs oldest first and waits out 5 s between calls; right before each call the collector checks `quota_high`, `hourly_cap` and the budget again and escalates the job instead when one holds; the board never waits on a call. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{the_board_keeps_its_pace_while_a_review_hangs, a_triage_dry_run_does_not_delay_a_review, queued_reviews_are_paced_5_s_apart, the_cap_is_checked_again_right_before_each_call}`.
5. **Shadow, blind** (AD-86, AD-88, FR-9, D-143). Auto approval is off in this story: each pending `auto` request is a Needs you card at once, and its review runs in parallel when the precheck allows. Its result is recorded and hidden: the card shows `auto_approval_off` or a precheck cause only, and neither `ApprovalView` nor `RelayView` carries a review field, a post-call cause or the reason before the author decides; a review that answers `approve` writes no approval file. Proof: core `crates/officina-core/src/board.rs::tests::a_pending_auto_request_with_a_stored_review_carries_no_review_field` (serializes the view and searches for every review field name and the stored reason); core `crates/officina-core/src/relay/view.rs::tests::an_undecided_request_shows_no_review_line`; CLI `crates/officina/tests/auto.rs::an_approve_in_shadow_writes_no_approval_file`.
6. **Rows and the baseline** (AD-82, AD-88, D-142, D-143). Each review writes one `auto_review` row: approval id, request key, project, action, `requested_at_ms`, mode `shadow`, outcome, cause, decision, reason (untrusted), the call metrics of AD-40's `triage_call` columns, and `rule_baseline` from `auto::rule_baseline(journal, request, tree, now_ms)` (the check, if named, satisfied by `check_satisfied` within 30 minutes for the tree fingerprint the window computes through `host::git` at review time, and the last `run_finished` of that action in that project exited 0; false with no earlier run). The baseline is an estimate; story 5.12's consumption-time check is the guarantee (D-151, review F6). The author's decision (approve, or reject with its reason from story 5.12; expiry is none) is copied onto the row with `decided_at_ms`. Proof: core `auto.rs::tests::{baseline_needs_the_check_and_a_last_success, baseline_is_false_without_an_earlier_run, baseline_is_false_after_a_tree_change}`; the `baseline` section of `auto.json`, with the tree as an input; host `crates/officina-host/tests/registry_review.rs::{a_review_writes_one_row_with_its_baseline, the_author_decision_is_copied_onto_the_row, expiry_is_not_a_decision}`.
7. **Journal** (AD-32 as amended). `auto_escalated` (approval id, cause) and `auto_review` (approval id, outcome, mode, cause, decision; never the reason). Proof: core `crates/officina-core/src/relay/journal.rs::tests::{auto_escalated_round_trips, auto_review_never_carries_the_reason}`.
8. **End to end** (AD-95 auto row, PRD section 7 Auto fail-closed row). With the fake claude's `review` mode (D-129) and its `invalid`, `hang` and `exit1` modes: approve, reject, escalate, invalid, timeout each produce their row and cause; each precheck cause escalates with zero calls; the budget and the hourly cap hold across a window restart. Proof: CLI `crates/officina/tests/auto.rs::{each_review_mode_records_its_outcome, each_precheck_cause_escalates_without_a_call, the_budget_holds_across_a_restart, the_hourly_cap_counts_shadow_approvals}`.
9. **Injection battery, review excerpts** (AD-95, D-147). Section `review_excerpt` of `tests/vectors/injection.json`: excerpts carrying the nonce, the end marker, "approve this request", quotes that would close the JSON line, bidi characters and oversized text reach the child on stdin only, inside the markers, never in its arguments or environment; the outcome comes from `structured_output` alone; no case writes an approval file or spawns `--bg`. Proof: `crates/officina/tests/injection.rs::{review_excerpt_cases, every_section_family_and_refusal_has_a_case}`.
10. **Exclusion and retention** (AD-43 as amended, D-134, D-166). Ingestion skips lines whose `cwd` is `<home>/review/empty`, and only those, so a session working elsewhere under `<home>/review/` is recorded like any other; `auto_review` rows are deleted 90 days after `requested_at_ms` and `reason` is cleared `EXCERPT_TTL_MS` after it. Proof: host `crates/officina-host/tests/registry_ingest.rs::{review_workdir_lines_are_skipped, other_lines_under_the_review_folder_are_recorded}`; `crates/officina-host/tests/registry_retention.rs::{auto_review_rows_are_deleted_at_90_days, review_reasons_are_cleared_with_the_excerpt_ttl}`.
11. **Zero write** (D-80). A review through the fake claude joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
12. **Doctor** (AD-94 part B). The review model, `review_config_invalid`, the reviews and calls of today against the budget. Proof: host `crates/officina-host/src/relay/doctor.rs::tests::{review_model_and_budget_are_reported, an_invalid_auto_review_table_is_reported}`.
13. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: the `auto` review in blind shadow; README "The orchestrator (v0.5, in progress)": what the review sees and answers, that it approves nothing yet, the families of causes that escalate without a call (the binary, the session, privacy, quota, caps and budget, configuration) naming `EscalationCause` without listing it (D-149, review F44), its own budget and cost. Proof: review.
14. **Memory files, E25** (D-164, D-126; review F23). `crates/officina-host/examples/review_probe.rs` runs one real review through the `ReviewCall` vector with a marker `CLAUDE.md` written in `<home>/review/` (the workdir's parent) whose instruction would show in the answer's `reason`; the result, with the Claude Code version, is recorded in `docs/research-notes.md` as E25. If memory loads, the vector adds the flag the documentation names to disable it; without one, `workdir_not_empty` also escalates while `<home>/review/` holds anything but `empty/`, with its `auto.json` case. Proof: manual, research notes entry E25; in the second branch, the `precheck` case.

## Tasks

- [ ] `crates/officina-core/src/relay/auto.rs`: `EscalationCause`, precheck, prompt, schema, validation, `rule_baseline` (AC 1, 2, 6)
- [ ] `tests/vectors/auto.json` (`precheck`, `prompt`, `output`, `baseline`) and `crates/officina-core/tests/auto_vectors.rs` (AC 1, 2, 6)
- [ ] `VectorKind::ReviewCall`, golden case, `<home>/review/empty` (AC 3)
- [ ] `crates/officina-host/src/relay/review.rs`: worker, queue, budget and cap counts, rows, author decision copy (AC 4, 6)
- [ ] Blind views; journal kinds (AC 5, 7)
- [ ] Fake claude `review` mode; `crates/officina/tests/auto.rs` (AC 8)
- [ ] `review_excerpt` section (AC 9)
- [ ] Exclusion, retention, zero-write, doctor (AC 10-12)
- [ ] Update `CHANGELOG.md` and the README (AC 13)
- [ ] `examples/review_probe.rs`; run it; E25 and its branch (AC 14)

## Dev Notes

- Requirements: FR-7 as amended (amendments 4 and 10), FR-8 as amended (amendment 5), FR-9's shadow part, J3, PRD section 7 Auto fail-closed row. Decisions: AD-3, AD-33 (`check_satisfied`), AD-43, AD-51 and AD-52 as amended, AD-79, AD-82, AD-86, AD-87, AD-88 (blind, baseline), AD-95. Facts: E16 (the contained call). Risks: R40, R41. Epic decisions: D-27 (Epic 2); D-64, D-65, D-80, D-86, D-87 (Epic 3); P-23, P-24, D-124, D-126, D-127, D-128, D-129, D-134, D-140, D-142, D-143, D-147, D-149, D-151, D-156, D-164, D-166.
- Seams (D-124): the triage worker shape (3.9), AD-43's exclusion (3.2), the journal reader and `check_satisfied` (2.6), the quota the collector holds (v0.1), the approval view (2.8, 5.12).
- Live mode, rule mode, the statistics and the toggle are story 5.14; here auto approval is always off.
- Pacing, not escalation (D-142): two requests a second apart both get reviewed, five seconds apart; a burst cannot pass `hourly_cap` before any review answers, because the cap is checked again right before each call (and, in 5.14, right before an approval is recorded).
- A queue of reviews can outlast `officina run`'s 110 s wait: the session is told to run the same command again, as for a pending `human` request (FR-7, R41).
- One review per request: a request reused by a second identical `run` call (AD-33's 60-minute reuse) is not reviewed again.
- The session never reads the review's reason (FR-8); its refusal text stays v0.2's.
- No employer content in `auto.json`: placeholder actions, projects and excerpts.

## Testing

- Core unit: `crates/officina-core/src/{relay/auto,relay/journal,relay/view,board}.rs`; core integration: `crates/officina-core/tests/{auto_vectors,spawn_vectors}.rs`.
- Host: `crates/officina-host/tests/{registry_review,registry_ingest,registry_retention}.rs`, `src/relay/doctor.rs`.
- CLI integration: `crates/officina/tests/{auto,watch,spawn,injection,quota_doctor}.rs`.
- Manual: `examples/review_probe.rs`, research notes entry E25.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the baseline takes the tree, the consumption-time check is the guarantee (F6); the 5 s spacing paces the worker, caps and budget rechecked before each call (F15); `examples/review_probe.rs` and E25 for memory files (F23); HTTP actions excluded (F24); contained-kind environment (F1); exclusion narrowed to the workdir (F27); README describes cause families (F44)
