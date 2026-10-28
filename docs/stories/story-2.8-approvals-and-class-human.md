# Story 2.8: Approvals and class human

**Status:** Approved

## Story

As the author keeping the risky steps for myself,
I want a `human` action to wait in Needs you with its exact argument vector until I approve or reject it, and to run once as soon as I approve,
so that a shared system is touched only after one deliberate click, with the command in front of me.

## Acceptance Criteria

1. Request key = SHA-256 of canonical `(project, action, params, catalogue_hash)`; request id = 16 hex of the key, `-`, `now_ms` (AD-33). Proof: core unit tests `crates/officina-core/src/relay/approvals.rs::tests::{key_is_stable_across_param_order, key_changes_with_each_component, id_format}`.
2. `run` for a `human` action follows AD-33 steps 1 to 4: a rejection for the key younger than 15 minutes refuses "rejected in Officina; do not retry"; an approval decided under 15 minutes ago is consumed by renaming `<id>.approved.json` to `<id>.consumed.json` just before the spawn; otherwise a pending request younger than 60 minutes is reused or a new `<id>.request.json` is created with `create_new` (FR-8 as amended, FR-9). Proof: CLI tests `crates/officina/tests/approvals.rs::{first_call_creates_a_request, second_call_reuses_the_pending_request, rejection_blocks_retries_for_15_min, approval_older_than_15_min_is_not_used, approval_is_single_use}`.
3. The card and the decision never trust the request file (D-27): core rebuilds the argument vector from the approved catalogue and the request's `(project, action, params)` and recomputes the request key; when the stored `argv` or `request_key` disagrees, or the action is no longer in the approved catalogue, the request is invalid: listed as invalid with its reason, without Approve, and `decide_approval` refuses it. Proof: core `approvals.rs::tests::{view_argv_is_rebuilt_from_the_catalogue, mismatched_key_makes_the_request_invalid}`, host `crates/officina-host/tests/authority.rs::decide_refuses_a_forged_request`, CLI `approvals.rs::forged_request_argv_is_not_shown_or_approvable` (a hand-written request file with a harmless `argv` and another parameter set's key), Playwright `app/e2e/approvals.spec.ts > an invalid request has no Approve button`.
4. `--wait <s>` (default 110, 1 to 540, otherwise exit 64) waits for a decision, stat'ing every 250 ms and checking the stop flag each tick; approved: consume and run; rejected: refused; timeout: refused "approval still pending; run the same command again to keep waiting" (AD-33). Proof: `approvals.rs::{wait_out_of_range_is_usage, wait_times_out_with_the_retry_message, stop_during_a_wait_refuses}` (the last stops through `authority::set_relay_stopped`, story 2.7).
5. Approval round trip (PRD section 7): a waiting `run`, approved through `authority::decide_approval`, starts the action in under 2 s and runs it once; the next identical call creates a new request. Proof: `approvals.rs::approve_while_waiting_runs_once_under_2_s`.
6. Single use without a lock: two threads consuming one approval, exactly one wins (AD-33, AD-37 stress). Proof: `#[ignore]` host test `crates/officina-host/tests/approvals_stress.rs::two_consumers_one_winner`, run by a new CI release step `cargo test --release -p officina-host --test approvals_stress -- --ignored`.
7. A failure after the approval was consumed (spawn failure, journal lock timeout) burns it; the next call creates a new request (D-29). Proof: `approvals.rs::failure_after_consume_burns_the_approval`.
8. `authority::decide_approval(id, approve)` writes `<id>.approved.json` or `<id>.rejected.json` (`decided_at_ms`, `request_key`) and refuses unknown, already decided, expired and invalid (AC 3) requests and requests whose catalogue hash is not the approved one (AD-35, AD-25). Proof: host tests `authority.rs::{decide_approves_a_pending_request, decide_refuses_unknown_decided_expired_and_stale_hash}`.
9. Journal: `approval_requested` once per new request, `approval_given` or `approval_rejected` once per decision, each with `approval_id` and `request_key`; the run consuming an approval carries `approval_id` in `run_started` (AD-32, PRD amendment 11). Proof: `approvals.rs::every_request_and_decision_leaves_exactly_one_line`.
10. `class_not_available` is gone (D-20): the refusal enum no longer has it, and `run --list` marks no class unavailable. Proof: `crates/officina/tests/run.rs::every_refusal_code_exits_126_with_one_journal_line` passes with the reduced enum; `after_check_and_human_are_not_available_yet` and `list_marks_unavailable_classes` are deleted.
11. `Board` gains `approvals: Vec<ApprovalView>` (`id`, `project`, `action`, `description`, `argv` as rebuilt by AC 3, `valid` with its reason, `session_id`, `session_name` resolved from the cards, `requested_at_ms`, `expires_at_ms`) built in core; `waiting_count` counts valid ones; `BOARD_SCHEMA` becomes 2; pending requests older than 60 minutes and decided ones are not listed (AD-35, FR-9). Proof: core unit tests `crates/officina-core/src/board.rs::tests::{approvals_count_as_waiting, invalid_requests_do_not_count, session_name_comes_from_the_cards, expired_and_decided_requests_are_not_listed}`, the classification vector replay, and `typescript_bindings.rs::committed_bindings_match_the_rust_types`.
12. Needs you lists approval cards first, oldest first: amber border, project chip, action and description, argv as one monospace chip per argument, session name or "session unknown", age, expiry, Approve and Reject buttons (AD-35 layout). Proof: Vitest `app/src/components/components.test.ts > ApprovalCard renders one chip per argument`, `> bidi and markup in arguments stay inert text`, `> a pointer click calls decideApproval with the card id`; Playwright `approvals.spec.ts > approval cards come first in Needs you, oldest first`.
13. No key approves (D-28): Approve and Reject act only on a pointer click (`event.detail > 0`); Enter and Space on a focused card or on a focused Approve or Reject button do nothing; keys 1 to 9 still focus the nth session card and skip approval cards. Proof: Vitest `components.test.ts > Enter and Space on a focused Approve button do not decide`; Playwright `approvals.spec.ts > number keys never approve`, `> Enter and Space on a focused approval card do not approve`.
14. The title, taskbar overlay and waiting pill count approvals (AD-35). Proof: Playwright `approvals.spec.ts > waiting pill counts approvals` and app-crate unit test `app/src-tauri/src/main.rs::tests::window_title_counts_approvals`.
15. `decide_approval` is declared in `build.rs`, allowed in the capability file, wrapped in `ipc.ts` and mocked in `e2e-mock.ts`; it sends `CollectorMsg::RelayChanged` (AD-35). Proof: `main.rs::tests::every_declared_command_is_allowed` (story 2.7) and the Playwright cases.
16. The collector reads `relay/approvals/` for the board, and the window deletes approval files older than 24 hours on its hourly pass; `run` never deletes them (AD-33). Proof: host tests `crates/officina-host/tests/relay_home.rs::{approval_files_older_than_24_h_are_pruned, run_never_deletes_approval_files}`.
17. Fixtures (D-25): `scripts/e2e-fixtures.mjs` produces an `approvals` board scenario from the real CLI, with the catalogue approved through the `approve_fixture` example and a pending request created by `officina run --wait 1`, plus one forged request file for the invalid card. Proof: CI fixture step and the Playwright cases.
18. A `human` action may name a `check` (D-170): phase 2 accepts it, the canonical form already carries `check`, and the issue that limited `check` to `after-check` is narrowed to the classes that cannot use it. Proof: core `crates/officina-core/src/relay/catalogue.rs::tests::{human_action_may_name_a_check, free_action_naming_a_check_is_an_issue}`.
19. `run` of a `human` action naming a check, with no successful matching check (after-check rule: project, check action, mapped parameters, catalogue hash, tree fingerprint, 30 minutes), refuses `check_required` and creates no request; with one, the request records `check_run_id` and the tree fingerprint, and both are part of the request key's inputs, so a later check gives a new request (D-170). Proof: CLI `crates/officina/tests/approvals.rs::{human_with_check_needs_the_check_first, request_records_the_check_run_and_tree, a_new_check_gives_a_new_request}`.
20. Every check action `run` executes keeps its output (stdout then stderr, at most 256 KiB, a cut marker past it) at `relay/outputs/<run_id>.txt`, written with a temporary name then renamed; the window prunes files older than 7 days on its hourly pass. `ApprovalView` gains `dry_run` (the kept output of the request's `check_run_id`, or `missing` with a reason) and the card shows it under the argument chips as inert monospace text, with the line "tracked and unignored files unchanged since the dry run" while the fingerprint matches. Proof: CLI `approvals.rs::check_output_is_kept_and_capped`; host `relay_home.rs::kept_outputs_older_than_7_days_are_pruned`; core `board.rs::tests::approval_view_carries_the_dry_run_output`; Vitest `components.test.ts > ApprovalCard shows the dry run as inert text`; Playwright `approvals.spec.ts > an approval with a dry run shows its output`.
21. Just before consuming an approval of a `human` action naming a check, `run` recomputes the tree fingerprint; when it differs from the request's, `run` refuses `check_not_satisfied`, leaves the approval file unconsumed and journals the refusal (D-170, D-29's spirit). Proof: CLI `approvals.rs::an_approval_is_not_consumed_after_a_tree_change`.

## Tasks

- [ ] Core `relay::approvals`: request key, id, argv rebuild and validity, `ApprovalView` construction (AC 1, 3, 11)
- [ ] Host: request create and reuse, wait loop, consume by rename, `decide_approval` in `authority` (AC 2, 4-9)
- [ ] `run`: `human` branch, `--wait` parsing; remove `class_not_available` (AC 2, 4, 10)
- [ ] Board schema 2: `approvals`, `waiting_count`, `BOARD_SCHEMA`; collector reads approvals; regenerate TypeScript (AC 11, 16)
- [ ] Tauri `decide_approval`, capability, `ipc.ts`, mock; title and overlay counts (AC 14, 15)
- [ ] `ApprovalCard` component (pointer-only buttons), Needs you ordering, keyboard rule (AC 12, 13)
- [ ] Approval file retention in the window's hourly pass (AC 16)
- [ ] CI release step for `approvals_stress` (AC 6)
- [ ] Tests: `approvals.rs`, `approvals_stress.rs`, `authority.rs`, Vitest, `app/e2e/approvals.spec.ts`; fixture scenario (AC 1-17)
- [ ] `human` with a `check`: catalogue rule, request binding to the check run and tree, kept check outputs and their pruning, `dry_run` on `ApprovalView` and the card, fingerprint recheck before consumption (AC 18-21, D-170)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (approvals, the dry run on the card, the `--check` caveat of D-170) (D-34)

## Dev Notes

- Requirements: FR-8 `human` and FR-9 as amended (amendments 8 and 9: no command line command approves or rejects). Decisions: AD-21 (authority only in the window), AD-25 (files), AD-32, AD-33 second part, AD-35. Facts: E12 (an approval gate cannot rely on `ask`). PRD journey 3, section 7 approval round trip. Epic decisions: P-6, D-20, D-25, D-27 to D-29.
- D-27: any process running as the author can write a request file. Writing a fake `approved.json` bypasses the author (accepted, PRD section 1); a forged request deceives the author into approving something else, which the rebuild prevents. The request's own `argv` stays in the file for the journal and for diagnosis, never for display.
- D-28 gives up keyboard approval entirely; that is AD-35's intent ("no key approves"). Use a `click` handler that ignores `event.detail === 0` (keyboard-synthesised clicks) rather than `pointerup`, so a mouse click and a pen tap both work.
- The wait makes journey 3 one click: the call is still open when the author approves (AD-33).
- P-6: `--wait` 540 plus a `timeout_s` up to 540 exceeds the Bash tool's 10-minute cap; doctor warns (story 2.10).
- `argv` chips: render each argument with `{text}` in its own element so a crafted value with spaces or bidi characters cannot pass for two arguments (AD-35).
- Changing `BOARD_SCHEMA` regenerates every board fixture; existing Playwright cases must still pass on schema 2 boards.
- Keyboard: `store.needsYou` today is `columns.needs_you`; keep approvals out of that list so the 1-9 mapping is unchanged.

## Testing

- Core unit: `crates/officina-core/src/relay/approvals.rs`, `src/board.rs`.
- Host integration: `crates/officina-host/tests/{authority,relay_home,approvals_stress}.rs`.
- CLI integration: `crates/officina/tests/{approvals,run}.rs`.
- App crate unit: `app/src-tauri/src/main.rs`.
- Vitest: `app/src/components/components.test.ts`. Playwright: `app/e2e/approvals.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: D-27 (argv rebuilt, forged requests invalid), D-28 (pointer clicks only, Enter and Space tested), D-29 (burned approval), stress test CI step, stop through the authority of story 2.7, fixtures through the example (D-25)
- 2026-10-28 AC 18-21 added (D-170): a `human` action with a dry run, its kept output on the card, and the tree binding at consumption, brought forward from the v0.8 requirement challenge
