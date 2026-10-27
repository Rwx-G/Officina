# Story 3.11: Model proposals: enable gate, queue, pause, self-disable

**Status:** Approved

## Story

As the author whose dry run showed the model beating the default on my own confirmations,
I want to turn the model on for the rows no rule settled, see "model pending" then its proposal with its reason as plain text, and have it pause after repeated failures and turn itself off if it starts doing worse than the default,
so that the model saves me flips only while it measurably does, and its text never gains any power.

## Acceptance Criteria

1. Enable gate (FR-10 as amended by amendment 8, AD-50, D-69, D-81): `set_triage_model` turns the model on only while the model enable gate of story 3.10 holds and the model is available (no `.cmd` or `.bat` shim, no `no_excerpts`); turning it on while paused is allowed and lifts the pause; otherwise it is refused with the gate's reason, and the window shows the toggle disabled with that reason. `set_triage_model` is declared, allowed, wrapped and mocked. Proof: host `crates/officina-host/tests/registry_write.rs::{enabling_the_model_requires_a_recent_winning_dry_run, enabling_is_refused_while_unavailable, re_enabling_lifts_the_pause}`; Playwright `app/e2e/evidence.spec.ts > the model toggle is offered only after a winning dry run under 7 days`, `> a disabled toggle says why`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
2. Queue (AD-52, FR-8 last bullet, D-87): while enabled and not paused, a row falling to the default is queued, one case per call, within the budget shared with the dry run; the row shows "model pending"; the answer goes to `model_answer` and serves while the row is unconfirmed; rows settled by `state` or `rules` are never sent; a failed call leaves the default with its outcome shown; a refused row is not queued again until its inputs change. Proof: CLI-crate tests `crates/officina/tests/triage.rs::{enabled_model_answers_default_rows_only, one_case_per_call, failed_call_keeps_the_default_and_says_why, refused_row_waits_for_new_inputs}`; Playwright `app/e2e/triage.spec.ts > a queued row says model pending, then shows the model's proposal`.
3. Untrusted text (AD-40, AD-55, P-11, D-68, D-88): the model's reason reaches the view only as `model_reason`, rendered as text under a "model" label; a confirmation stores the model `ReasonCode` and `call_id`, never the text; `model_answer` rows are deleted on confirmation, archive, excerpt expiry and `no_excerpts`. Proof: host `registry_write.rs::{model_text_never_leaves_model_answer, confirmation_archive_ttl_and_no_excerpts_delete_model_answers}` (the first scans every table for the fake's marker reason after a confirmation); Playwright `triage.spec.ts > model text renders as text under a model label` (an `<img onerror>` reason).
4. A confirmation item answering a model proposal carries the call id; a newer answer for the same row makes it refused as changed (D-63). Proof: `crates/officina-core/src/registry/confirm.rs::tests::model_item_is_refused_when_the_answer_changed`.
5. Pause (AD-50, D-66, D-81; this story owns the rule): three failed proposal calls in a row (`failed`, `timeout`, `invalid`) pause the model; re-enabling or a change of the binary's stamp lifts the pause; refusals and dry-run failures do not count toward it; while paused nothing is queued and the view says why. Proof: host `registry_write.rs::{three_failed_proposal_calls_pause_the_model, a_binary_change_lifts_the_pause, refusals_and_dry_run_failures_do_not_count_toward_the_pause}`; Playwright `evidence.spec.ts > a paused model says why and offers re-enabling`.
6. Self-disable (FR-10 as amended, AD-50, D-69): when the self-disable test of story 3.10 fails after a confirmation of a model proposal, the model turns off with source `rules` and its reason; turning it on again needs the enable gate; with `no_excerpts` the model is unavailable. Proof: host `registry_write.rs::{model_disables_itself_when_worse_than_the_default, no_excerpts_makes_the_model_unavailable}`; CLI `triage.rs::self_disable_through_the_fake_claude` (fake verdicts flipped by seeded confirmations).
7. Auto-archive never takes a model proposal (R18, D-70), checked again with live model rows. Proof: `triage.rs::model_finished_is_never_auto_archived`.
8. Coverage complete (AD-57, D-45): every provenance value, the model source included, now has a vector case; `triage_vectors.rs::every_value_has_a_case` iterates `Source::ALL` and every enum AD-57 names. Proof: that test.
9. Fixtures (D-72): a `model` scenario from `registry_fixture` with a winning dry run, a pending row, an answered row and a paused state. Proof: CI fixture step and the Playwright cases.
10. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": turning the model on, what it is sent, when it pauses and how to lift a pause, when it turns itself off. Proof: review.

## Tasks

- [ ] `set_triage_model` with the gate, availability and pause lifting; `setting` provenance (AC 1, 5)
- [ ] Queue in the collector feeding the `triage` thread; `model_answer`; proposals with `model` source through story 3.6's slot; refused rows keyed by their inputs (AC 2)
- [ ] `model_reason` in the view; model label rendering; `model_answer` deletion rules (AC 3)
- [ ] `confirmation.call_id` on confirmation items (AC 4)
- [ ] Pause rule with the binary stamp; self-disable after each confirmation of a model proposal (AC 5, 6)
- [ ] Vector cases completing provenance coverage (AC 8)
- [ ] `model` scenario; Playwright and CLI tests (AC 1-3, 5-7, 9)
- [ ] Update `CHANGELOG.md` and the README (AC 10)

## Dev Notes

- Requirements: FR-8 (`model` source), FR-10 as amended (amendment 8), journey 4. Decisions: AD-40 (untrusted model text), AD-49, AD-50, AD-52, AD-55, AD-57. Facts: E16 (only the schema-constrained field may be read). Risks: R18, R19, R23. Epic decisions: P-11, D-45, D-59, D-63, D-65, D-66, D-68 to D-70, D-81, D-87, D-88.
- This story owns the pause (D-81); story 3.9 only feeds it with proposal calls. Re-enabling lifts the pause so a blip never locks the feature until Claude Code updates itself, and dry runs stay possible while paused.
- The model never settles a row a rule settled (FR-8): only the default's place is offered to it, and `unsure` stays a proposal the author must turn into `f` or `t` (D-59).
- Rules never read `model_answer.reason` (AD-40): the proposal function takes the model's verdict only; the text goes to the view untouched, as data.
- Self-disable is checked after each confirmation of a model proposal, against the counts story 3.10 shows, so the panel and the switch never disagree.
- After this story the PRD's features are all built; story 3.12 closes the epic.

## Testing

- Core unit: `crates/officina-core/src/registry/confirm.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host integration: `crates/officina-host/tests/registry_write.rs`.
- CLI integration: `crates/officina/tests/triage.rs` with a copied `officina-fake-claude`.
- App crate unit: `app/src-tauri/src/main.rs`. Playwright: `app/e2e/{evidence,triage}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: this story owns the pause, re-enabling lifts it, refusals and dry-run failures do not count (D-81); refused rows not re-queued until their inputs change (D-87); `confirmation.call_id` (D-88); `model_answer` deleted on `no_excerpts` too
