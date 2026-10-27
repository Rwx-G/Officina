# Story 3.11: Model proposals: enable gate, queue, self-disable

**Status:** Approved

## Story

As the author whose dry run showed the model beating the default on my own confirmations,
I want to turn the model on for the rows no rule settled, see "model pending" then its proposal with its reason as plain text, and have it turn itself off if it starts doing worse than the default,
so that the model saves me flips only while it measurably does, and its text never gains any power.

## Acceptance Criteria

1. Enable gate (FR-10 as amended by amendment 8, AD-50, D-69): `set_triage_model` turns the model on only while the model enable gate of story 3.10 holds and the model is available (no `.cmd` shim, no `no_excerpts`, not paused); otherwise it is refused with the gate's reason, and the window shows the toggle disabled with that reason. `set_triage_model` is declared, allowed, wrapped and mocked. Proof: host `crates/officina-host/tests/registry_write.rs::{enabling_the_model_requires_a_recent_winning_dry_run, enabling_is_refused_while_unavailable_or_paused}`; Playwright `app/e2e/evidence.spec.ts > the model toggle is offered only after a winning dry run under 7 days`, `> a disabled toggle says why`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
2. Queue (AD-52, FR-8 last bullet): while enabled, a row falling to the default is queued, one case per call, within the budget shared with the dry run; the row shows "model pending"; the answer goes to `model_answer` and serves while the row is unconfirmed; rows settled by `state` or `rules` are never sent; a failed call leaves the default with its outcome shown. Proof: CLI-crate tests `crates/officina/tests/triage.rs::{enabled_model_answers_default_rows_only, one_case_per_call, failed_call_keeps_the_default_and_says_why}`; Playwright `app/e2e/triage.spec.ts > a queued row says model pending, then shows the model's proposal`.
3. Untrusted text (AD-40, AD-55, P-11, D-68): the model's reason reaches the view only as `model_reason`, rendered as text under a "model" label; a confirmation stores the model `ReasonCode` and the call id, never the text; `model_answer` rows are deleted on confirmation, archive and excerpt expiry. Proof: host `registry_write.rs::{model_text_never_leaves_model_answer, confirmation_archive_and_ttl_delete_model_answers}` (the first scans every table for the fake's marker reason after a confirmation); Playwright `triage.spec.ts > model text renders as text under a model label` (an `<img onerror>` reason).
4. A confirmation item answering a model proposal carries the call id; a newer answer for the same row makes it refused as changed (D-63). Proof: `crates/officina-core/src/registry/confirm.rs::tests::model_item_is_refused_when_the_answer_changed`.
5. Self-disable and pause (FR-10 as amended, AD-50, D-66, D-69): when the self-disable test of story 3.10 fails, the model turns off with source `rules` and its reason; three failed calls in a row pause it until re-enabled or until the binary's stamp changes; with `no_excerpts` it is unavailable. Proof: host `registry_write.rs::{model_disables_itself_when_worse_than_the_default, three_failures_pause_the_model_until_the_binary_changes, no_excerpts_makes_the_model_unavailable}`; CLI `triage.rs::self_disable_through_the_fake_claude` (fake verdicts flipped by seeded confirmations).
6. Auto-archive never takes a model proposal (R18, D-70), checked again with live model rows. Proof: `triage.rs::model_finished_is_never_auto_archived`.
7. Coverage complete (AD-57, D-45): every provenance value, the model source included, now has a vector case; `triage_vectors.rs::every_value_has_a_case` iterates `Source::ALL` and every enum AD-57 names. Proof: that test.
8. Fixtures (D-72): a `model` scenario from `registry_fixture` with a winning dry run, a pending row and an answered row. Proof: CI fixture step and the Playwright cases.
9. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": turning the model on, what it is sent, when it turns itself off. Proof: review.

## Tasks

- [ ] `set_triage_model` with the gate and availability checks; `setting` provenance (AC 1)
- [ ] Queue in the collector feeding the `triage` thread; `model_answer`; proposals with `model` source through story 3.6's slot (AC 2)
- [ ] `model_reason` in the view; model label rendering; `model_answer` deletion rules (AC 3)
- [ ] Call id on confirmation items (AC 4)
- [ ] Self-disable after each confirmation of a model proposal; pause lifting by binary stamp (AC 5)
- [ ] Vector cases completing provenance coverage (AC 7)
- [ ] `model` scenario; Playwright and CLI tests (AC 1-3, 5, 6, 8)
- [ ] Update `CHANGELOG.md` and the README (AC 9)

## Dev Notes

- Requirements: FR-8 (`model` source), FR-10 as amended (amendment 8), journey 4. Decisions: AD-40 (untrusted model text), AD-49, AD-50, AD-52, AD-55, AD-57. Facts: E16 (only the schema-constrained field may be read). Risks: R18, R19, R23. Epic decisions: P-11, D-45, D-59, D-63, D-66, D-68 to D-70.
- The model never settles a row a rule settled (FR-8): only the default's place is offered to it, and `unsure` stays a proposal the author must turn into `f` or `t` (D-59).
- Rules never read `model_answer.reason` (AD-40): the proposal function takes the model's verdict only; the text goes to the view untouched, as data.
- Self-disable is checked after each confirmation of a model proposal, against the counts story 3.10 shows, so the panel and the switch never disagree.
- After this story the PRD's features are all built; story 3.12 closes the epic.

## Testing

- Core unit: `crates/officina-core/src/registry/confirm.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host integration: `crates/officina-host/tests/registry_write.rs`.
- CLI integration: `crates/officina/tests/triage.rs` with `officina-fake-claude`.
- App crate unit: `app/src-tauri/src/main.rs`. Playwright: `app/e2e/{evidence,triage}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
