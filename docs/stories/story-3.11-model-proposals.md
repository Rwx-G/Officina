# Story 3.11: Model proposals: enable gate, queue, pause, self-disable

**Status:** review

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

- [x] `set_triage_model` with the gate, availability and pause lifting; `setting` provenance (AC 1, 5)
- [x] Queue in the collector feeding the `triage` thread; `model_answer`; proposals with `model` source through story 3.6's slot; refused rows keyed by their inputs (AC 2)
- [x] `model_reason` in the view; model label rendering; `model_answer` deletion rules (AC 3)
- [x] `confirmation.call_id` on confirmation items (AC 4)
- [x] Pause rule with the binary stamp; self-disable after each confirmation of a model proposal (AC 5, 6)
- [x] Vector cases completing provenance coverage (AC 8)
- [x] `model` scenario; Playwright and CLI tests (AC 1-3, 5-7, 9): two scenarios, `model` and `model-paused`
- [x] Update `CHANGELOG.md` and the README (AC 10)

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

- **Decisions where the story is silent:**
  - AC 1: the model's setting is `{"on", "reason"}` like auto-archive's (D-69); its pause is `{"paused", "stamp"}`, written `paused: false` when lifted rather than deleted, so the lift's time stays: failed calls count toward the pause, and a failed case may be sent again, only from the latest enable or lift (`ModelFacts::since_ms`). The model is unavailable without a `claude` too (`claude_not_found`), beside the shim and `no_excerpts`; the window recomputes its availability and the binary's stamp each tick (`ModelEnv`), since the floor's `no_excerpts` and `claude` can change while it runs, and the writer reads them for the view, the confirmations and the queue. `subjects --json` computes the same availability from its own collector.
  - AC 1, D-69: "a newer winning dry run" after a self-disable is a gate state of its own, `needs_newer_dry_run`: the enable gate holds only on a dry run made after the model turned itself off. It has its rates vector cases and its wording.
  - AC 2: core `registry::model` decides everything the queue does (`row_model`, `queue`, `may_send`, `pause_due`, `answer_still_wanted`); the host reads the rows and writes. A proposal case is keyed `session:<id>` (the row's anchor, which `model_answer` is keyed by) and carries what a confirmation would keep of that session, so the dry run measures what production sends. A row without an excerpt is not sent: there is nothing to judge. A case is sent once per input hash: an answer is held, a refusal waits for new inputs (D-87), a failed call waits for the next enable or lift. A stored answer serves only while its call's input hash is the session's case now, so an answer to older excerpts never shows on newer ones. An answer is stored only when a row still offers the default's place for that session on the very case sent and the model still serves: one coming back after its row was confirmed, settled by a rule or changed, or after the model was turned off, is recorded as a call and dropped. A dry run the author asked for goes before the proposals; answers keep serving while the model is paused.
  - AC 3, D-68: two triggers of schema 1 delete a session's `model_answer` row whenever its `session_excerpt` row is deleted or its text changes, so archive, expiry, `no_excerpts`, the latest-excerpts pruning (D-50) and a newer excerpt all take the model's text with them whatever path clears them; confirmation deletes the row's answer explicitly, since `to_resume` clears no excerpt. Schema 1 and its pinned hash change in one commit (P-14: no released database holds version 1).
  - AC 5: the pause is checked in the transaction that records each proposal call, over the proposal calls since the latest enable or lift; the stamp is the binary's path, size and modification time, and a different stamp lifts the pause at the next queue read, source `rules`.
  - AC 6: self-disable runs inside the confirmation's transaction, after its items, whenever one confirmed item answered a model proposal, on the counts the panel shows (`model_self_disable_gate`).
  - AC 8: `every_value_has_a_case` collects the provenances the cases expect (the facts' sources, who filed each session and set each subject's status, and a stored model answer as a `model` row, D-45) and iterates `Source::ALL`. The `triage` section's model part moved from a raw `ModelInput` to the model's facts (on, paused, unavailable, answers and last calls on the current case or a stale one), so the vectors replay the queue's rules too: six cases added (pending, paused with a failed call, a refusal waiting for new inputs, a failed case sent again after an enable, a stale answer queued again, unavailable).
  - AC 9: one scenario cannot hold a pending row and a paused state at once (nothing is pending while paused), so there are two: `model` (winning dry run, the model on, the latest row answered with a markup reason, the others pending) and `model-paused` (the same, then three calls without an answer).
  - Core `triage_call::queue_again` (story 3.9) is replaced by `model::may_send`, which holds D-87 and the retry rule in one place; `Writer::may_queue` keeps its name and calls it.
- **Adversarial pass** (prompt injection in subject text and excerpts, malformed and oversized answers, timeouts, budget exhaustion, refused cases re-queued, a proposal acting without the author, races between the collector's writes and an answer out): one real miss, now fixed and tested: the collector read the whole queue (every subject, session and excerpt) on every 250 ms tick while the model was on, and after a refusal, which spends no budget, sent the next row at once (`registry_write.rs::model::the_queue_is_read_once_per_spacing`). The other cases held and are now tested: a subject's name never reaches the call (`triage.rs::proposals::subject_names_never_reach_the_call`), a spent budget keeps rows pending (`model::proposals_wait_while_the_budget_is_spent`), an answer to a case that changed or after the model was turned off is dropped (`model::an_answer_to_a_case_that_changed_meanwhile_is_dropped`, `model::an_answer_after_the_model_was_turned_off_is_dropped`), as is one for a row confirmed meanwhile (`model::an_answer_for_a_row_confirmed_meanwhile_is_dropped`). Malformed, oversized and timed-out answers are story 3.9's outcomes, which count toward the pause here (`model::three_failed_proposal_calls_pause_the_model`); nothing acts on a model proposal but the author's confirmation, and auto-archive never takes one (`triage.rs::proposals::model_finished_is_never_auto_archived`).

### Checks

Per commit: `cargo fmt`, clippy on the workspace with `--all-targets --features officina-core/ts` and on the host without `window-authority`, the tests of the crates touched; `pnpm check`, `lint`, `test`, and the evidence, triage and subjects specs with `--repeat-each=3` for the frontend commit. The full gate runs at the end of story 3.12's part and is recorded there.

### Completion Notes

- AC 1: `crates/officina-host/tests/registry_write.rs::model::{enabling_the_model_requires_a_recent_winning_dry_run, enabling_is_refused_while_unavailable, re_enabling_lifts_the_pause}`; `app/e2e/evidence.spec.ts > Model switch > the model toggle is offered only after a winning dry run under 7 days`, `> a disabled toggle says why`, `> a refused switch says why and stays as the registry holds it`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
- AC 2: `crates/officina/tests/triage.rs::proposals::{enabled_model_answers_default_rows_only, one_case_per_call, failed_call_keeps_the_default_and_says_why, refused_row_waits_for_new_inputs}`; `app/e2e/triage.spec.ts > Model proposals on rows > a queued row says model pending, then shows the model's proposal`, `> a call that gave no answer keeps the default and says why`; `registry_write.rs::model::a_default_row_is_pending_then_answered`; `crates/officina/tests/watch.rs::the_collector_sends_a_default_row_to_the_enabled_model`; core `crates/officina-core/src/registry/model.rs::tests` and the model cases of the `triage` vector section.
- AC 3: `registry_write.rs::model::{model_text_never_leaves_model_answer, confirmation_archive_ttl_and_no_excerpts_delete_model_answers}`; `triage.spec.ts > Model proposals on rows > model text renders as text under a model label`.
- AC 4: `crates/officina-core/src/registry/confirm.rs::tests::model_item_is_refused_when_the_answer_changed`.
- AC 5: `registry_write.rs::model::{three_failed_proposal_calls_pause_the_model, a_binary_change_lifts_the_pause, refusals_and_dry_run_failures_do_not_count_toward_the_pause}`; `evidence.spec.ts > Model switch > a paused model says why and offers re-enabling`; core `model.rs::tests::three_failed_calls_in_a_row_are_due_a_pause`; `crates/officina-host/src/doctor.rs::tests::model_state_and_counts_are_reported`.
- AC 6: `registry_write.rs::model::{model_disables_itself_when_worse_than_the_default, no_excerpts_makes_the_model_unavailable}`; `triage.rs::proposals::self_disable_through_the_fake_claude`; the `rates` cases of the newer dry run.
- AC 7: `triage.rs::proposals::model_finished_is_never_auto_archived`.
- AC 8: `crates/officina-core/tests/triage_vectors.rs::every_value_has_a_case`, which now iterates `Source::ALL` beside every enum AD-57 names.
- AC 9: the `model` and `model-paused` scenarios of `app/scripts/e2e-fixtures.mjs` with their checks; the Playwright cases above.
- AC 10: `CHANGELOG.md` `[Unreleased]` (Model proposals); README "Subjects and triage", "The triage model".

## File List

- `crates/officina-core/src/registry/{model,rates,triage_call,view,confirm,mod}.rs`, `crates/officina-core/tests/triage_vectors.rs`, `tests/vectors/triage.json`
- `crates/officina-host/src/registry/{triage,read,write,window,schema,store,commands}.rs`, `crates/officina-host/src/{watch,doctor}.rs`, `crates/officina-host/examples/registry_fixture.rs`, `crates/officina-host/tests/{registry_write,registry_schema,registry_read}.rs`
- `crates/officina/tests/{triage,watch}.rs`
- `app/src-tauri/{build.rs,capabilities/default.json,src/main.rs}`, `app/src/lib/generated/` (regenerated), `app/src/lib/{ipc.ts,registry.svelte.ts,registry-text.ts}`, `app/src/e2e-mock.ts`, `app/src/components/{TriageEvidence,TriageRow}.svelte`, `app/src/components/components.test.ts`, `app/scripts/e2e-fixtures.mjs`, `app/e2e/{fixtures.ts,evidence.spec.ts,triage.spec.ts}`
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.11-model-proposals.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: this story owns the pause, re-enabling lifts it, refusals and dry-run failures do not count (D-81); refused rows not re-queued until their inputs change (D-87); `confirmation.call_id` (D-88); `model_answer` deleted on `no_excerpts` too
- 2026-10-28 implemented: the enable gate and availability, the proposal queue and its answers, the pause with the binary's stamp, self-disable after each confirmation of a model proposal, the switch and the rows in the window, the vectors completing provenance coverage, the `model` and `model-paused` fixtures; schema 1 gains two triggers deleting a session's model answer with its excerpts, and its pinned hash moves (P-14); one adversarial fix (the queue read once per spacing). Status review
