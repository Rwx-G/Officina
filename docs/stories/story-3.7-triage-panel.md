# Story 3.7: Triage panel and batch confirmation

**Status:** review

## Story

As the author sorting four finished subjects on a Monday morning,
I want a Triage panel that shows each proposal with its source, reason and excerpts, lets me confirm or flip one row from the keyboard, file an unfiled session in the same gesture, and confirm every shown row with one click,
so that closing finished work takes seconds and every confirmation records what it answered.

## Acceptance Criteria

1. Confirmation in core (AD-49, P-11, D-60, D-62, D-63, D-76, D-85, D-88): `registry::confirm` takes items carrying the row key and the proposal shown; an item whose recomputed proposal differs is refused with a typed reason; an unfiled row is filed first (creating a named subject when asked), then the verdict applies per D-60; `finished` archives (with the routine flag of D-62) and clears the subject's excerpts, `to_resume` sets the status; an empty-subject row is confirmed the same way, with no session and no input; the confirmation copies the proposal's verdict, source, `ReasonCode` and call id, never prose; `confirmation_input` copies the excerpts and the facts the decision rested on and expires with the excerpts it copies, inside the 30-day bound. Proof: the triage cases of the `confirm` section of `tests/vectors/triage.json`; core `crates/officina-core/src/registry/confirm.rs::tests::{changed_proposal_is_refused, unfiled_row_is_filed_then_decided, verdict_not_needed_is_answered_with_its_cause, empty_subject_row_archives_without_input, confirmation_input_expires_with_its_excerpts}`.
2. Writer (AD-49, AD-39, FR-4, D-85): `confirm_triage` runs one `FULL` transaction and answers per item; on `finished` the session's `session_excerpt` row is gone and the only copy of its text is the `confirmation_input` row with its expiry; once that row expires and the pass runs `wal_checkpoint(TRUNCATE)`, a byte search finds the text neither in the database nor in its WAL. Proof: host `crates/officina-host/tests/registry_write.rs::{batch_is_one_transaction_answered_per_item, finished_leaves_one_expiring_copy_then_none, confirmation_copies_the_proposal_it_answered}`, at explicit `now_ms`.
3. Crash, confirmation part (AD-58 crash row, D-89): the crash helper gains mode `confirm` (ingests and confirms in a loop, logging each commit it saw acknowledged, with a stall inside a confirmation transaction at a chosen item); after kills at that point and at 20 random points, every acknowledged confirmation is present once with its input, no row is confirmed twice, and `quick_check` passes. Proof: `#[ignore]` `crates/officina-host/tests/registry_crash.rs::killed_while_confirming_keeps_every_acknowledged_confirmation_once`, in the CI release step of story 3.2.
4. Restart (journey 5, PRD section 7 Memory): with the window stopped, three days of event files are written in which sessions end; at the next start every session that ended meanwhile is in the registry, and every subject and confirmation made before the stop is still there. Proof: CLI-crate collector test `crates/officina/tests/watch.rs::a_closed_weekend_loses_no_session_subject_or_verdict`.
5. Triage panel (FR-9, AD-55, D-61): the panel heads the Subjects view; each row shows the proposal, its source, its reason through an exhaustive `Record<ReasonCode, string>`, the excerpts, and for an unfiled row the proposed subject with the choices and a new-name field; an empty-subject row says "empty subject: archive?". Proof: Playwright `app/e2e/triage.spec.ts > each row shows its proposal, source, reason and excerpts`, `> an unfiled row proposes the obvious subject and lets me choose or name one`, `> an empty subject is offered for archiving`; Vitest `app/src/components/components.test.ts > TriageRow renders the reason wording of every reason code` (iterating the generated `ReasonCode` values); `pnpm check` fails on a missing `Record` entry.
6. Keys (AD-55): arrows move focus; `f` and `t` set the focused row's verdict; `Enter` confirms that row only; `event.repeat` is ignored; after a confirm, `Enter` waits for the view that removes the row and focus moves to the row taking its place; an `unsure` row has no verdict until `f` or `t` (D-59). Proof: `triage.spec.ts > arrows move, f and t set the verdict, Enter confirms the focused row only`, `> a held Enter confirms once`, `> focus goes to the next row once the view drops the confirmed one`, `> an unsure row needs f or t before Enter`.
7. Confirm all shown (AD-55, D-28): pointer only (`event.detail > 0`); its label carries the count; `unsure` rows, rows without a subject and rows in flight are left out and counted apart. Proof: `triage.spec.ts > confirm all shown confirms the shown rows with one click`, `> its label counts the rows and the ones left out`; Vitest `components.test.ts > Enter and Space on Confirm all do nothing`.
8. The "N to sort" chip (AD-55, D-61): neutral colour, never amber; rows to sort leave `waiting_count`, hence the window title and the taskbar overlay, unchanged; a click opens the Subjects view on the panel. Proof: `triage.spec.ts > the to-sort chip is neutral and opens the panel`; core `crates/officina-core/src/board.rs::tests::rows_to_sort_leave_waiting_count_unchanged` (a board built with and without pending rows in the registry input).
9. Confirming `finished` offers the routine box (FR-13, D-62); a refused item shows its reason on its row and keeps it. Proof: `triage.spec.ts > confirming finished offers the routine box`, `> a refused item keeps its row with the reason`.
10. Session and subject text renders as text (AD-55). Proof: `triage.spec.ts > excerpts and names render as text, never as HTML`; the `svelte/no-at-html-tags` lint.
11. Command and fixtures: `confirm_triage` is declared, allowed, wrapped in `ipc.ts` and mocked (AD-54); a `triage` scenario from `registry_fixture` (D-72) holds a multi-session subject, a state-proposed row, a cut-mid-turn row, an empty subject, an unfiled row with one obvious subject and one with none. Proof: `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`; CI fixture step and the Playwright cases.
12. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": the panel, its keys, Confirm all, and that a confirmation keeps the excerpts it was decided on under the same bound. Proof: review.

## Tasks

- [x] `crates/officina-core/src/registry/confirm.rs`: triage confirmation, empty-subject rows, item outcomes (AC 1)
- [x] Host writer `confirm_triage`, `confirmation_input` with its expiry, purge and checkpoint (AC 2)
- [x] Crash helper mode `confirm` with its acknowledgement log and stall point; `registry_crash.rs` case (AC 3)
- [x] Closed-weekend collector test (AC 4)
- [x] Components `TriagePanel`, `TriageRow`; `SubjectPicker` reuse; key handling; Confirm all; chip in `Header.svelte` (AC 5-10)
- [x] `confirm_triage` command, capability, `ipc.ts`, mock; `triage` scenario (AC 11)
- [x] `confirm` cases in the vector set; `app/e2e/triage.spec.ts`; Vitest cases (AC 1, 5-10)
- [x] Update `CHANGELOG.md` and the README (AC 12)

## Dev Notes

- Requirements: FR-4 (amendment 4: each confirmation keeps the excerpts it was decided on), FR-9, FR-13 (the box on a `finished` confirmation), journeys 2, 3 and 5, PRD section 7 Memory and Attention. Decisions: AD-39, AD-40, AD-49, AD-54, AD-55, AD-58 (crash row). Epic decisions: D-28 (pointer-only rule, Epic 2), P-11, D-59 to D-63, D-72, D-76, D-85, D-88, D-89.
- The refusal of a changed proposal mirrors `approve_catalogue` refusing a hash that moved (AD-24): the author decides on what was shown, never on what the writer computed later.
- `confirmation_input` is the evaluation set of the dry run (story 3.9); it expires with the excerpts it copies (D-85), so AC 2's byte search can only pass after that expiry: until then the copy is expected.
- An empty-subject row has no session and no excerpt: its confirmation has a null `session_id` (D-88) and no input.
- Excluding rows in flight from Confirm all prevents one row being confirmed twice when a click lands during a keyboard confirm.
- The chip is neutral on purpose: triage blocks no session, and amber marks only what waits on the author (frontend rule, AD-55). The title count comes from `waiting_count`, which is why AC 8 asserts it in core rather than on `window_title`.
- Board digits work only in the Board view (story 3.5); `f`, `t` and `Enter` act only in the Triage panel.

## Testing

- Core unit: `crates/officina-core/src/registry/confirm.rs`, `src/board.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host integration: `crates/officina-host/tests/{registry_write,registry_crash}.rs`.
- CLI-crate collector: `crates/officina/tests/watch.rs`.
- App crate unit: `app/src-tauri/src/main.rs`.
- Vitest: `app/src/components/components.test.ts`. Playwright: `app/e2e/triage.spec.ts`.

## Dev Agent Record

### Debug Log

- **Decisions where the story is silent:**
  - AC 1: `registry::confirm::{ConfirmItem, FileUnder, ItemOutcome, ItemAnswer, confirm_item, ConfirmPlan}`. An item carries `key`, `shown` (the whole `Proposal`), the author's `verdict`, `routine` (read on `finished` only) and `file_under` (`{subject: {id}}` or `{new: {name}}`, read on an unfiled row only). The core finds the row again among the rows computed now: none with the key is `ConfirmRefusal::RowGone` (confirmed, alive again, filed elsewhere, or the same key twice in a batch), a different proposal is `ProposalChanged`, an unfiled row without `file_under` is `NoSubject`, and a filing a subject rule refuses is `FilingRefused` with its `AuthorRefusal`. The filing reuses the author `Plan` of `File` or `Create {origin: sessions}`, so D-54 and D-57 hold unchanged.
  - AC 1, D-60: the verdict of a filed session applies when it is the latest prompted session among the target's sessions (the filed one included) and none of them is live; otherwise `VerdictNotNeeded` with `NotNeeded::{NotLatest, LiveSession}`. "Neither archived" needs no cause: filing into an archived subject is already refused (D-54).
  - AC 1, D-63, D-88: `Proposal` gains `call_id` (the call of a model answer, `None` for every other source), and `ModelInput.answers` holds `{verdict, call_id}`; the two model cases of the vector set say so. The comparison covers the call id, so another call's answer is a changed proposal.
  - AC 1, D-85: `confirmation_input` copies the row session's own excerpt row (prompt, answer head and tail) with `end_reason` (the `SessionEnd` reason, `interrupted`, or a background session's state) and `cut_mid_turn`; it expires at the earliest of the copied excerpt's times plus `EXCERPT_TTL_MS`, the instant the hourly pass would have cleared the earliest of them. A row whose session holds no excerpt (cleared, `no_excerpts`, or the excerpts kept by a later answer of another session of the subject, D-50) gets no input row, like an empty subject: there is nothing to evaluate a model on.
  - AC 1: `finished` clears the excerpts of every session counting under the subject (the filed one included), as archive does; `to_resume` sets the status only. Status changes carry source `author` and the confirmation's time.
  - AC 2: `Writer::confirm_triage` reads the registry again for each item inside the one `IMMEDIATE` transaction, so an item sees the items before it; `confirm_triage_observed` calls back after each item, before the commit (the crash helper's stall point). The WAL is truncated after a batch that cleared excerpts. `Writer::view` builds the view on the writer's connection, for the helper and tests. `read::model_input()` is the one place the view and the writer take the model's part from (disabled until 3.9 and 3.11).
  - AC 3: helper mode `confirm` confirms in batches of 3 (one unfiled row per item, filed under a subject of its own, `finished` and `to_resume` alternating) and prints `ack <key>` after each commit; a run killed between a commit and its acknowledgement leaves rows confirmed and unacknowledged, so the test asserts every acknowledged key present once with its input, no session confirmed twice, every row confirmed after the last run, and no key acknowledged twice. The CI crash step runs it with the ingestion cases; its comment names both.
  - AC 4: `Reader::confirmations()` lets the CLI-crate test read the confirmations without a new dev-dependency.
  - AC 5 to 10: per-row choices live in `RegistryStore` (`verdicts`, `filings`, `routines`, `confirming`, `rowErrors`) and are dropped with the row on each view. A confirmed or filed row stays "in flight" until a view drops it; a refused one is released at once with its reason. `SubjectPicker` takes `{id, name, project}` and a `selected` id, so the unfiled row reuses it. The rows are focusable list items (roving `tabindex`), the WAI feed pattern; `Enter` acts only when the row itself has the focus, so a focused button keeps its own `Enter`. Every triage key ignores `event.repeat`. Confirm all's label is "Confirm all N shown; left out: ..." with the three counts, and "unsure" reads "without a verdict", since an `unsure` row the author answered with `f` or `t` is taken. The chip is a button in the header that sets `openPanel` and switches to Subjects; the panel takes the flag, scrolls to itself and focuses its first row.
  - AC 5: the Vitest case iterates the values of the generated `ReasonCode.ts`, read as text (`?raw`), and checks them against the `Record`'s keys.
  - AC 11: the `triage` scenario adds a background session reported `done` by `claude agents`, so a `state` proposal comes from the real fold; the fixture script checks every row kind, a `state` proposal, a cut turn, the two unfiled rows and a subject row of several sessions, and each scenario now carries its own checks.
- **Adversarial pass** (a key given twice in a batch, a proposal moved by a call id, a filing refused by a name rule, a session filed under a subject with a later or a live session, an empty subject, no excerpt to copy, a commit not acknowledged before a kill, the view arriving before the answer, a held `Enter` on a row refused at once, `f` typed in the new-name field, Enter on a focused button inside a row, an unavailable registry): two real misses, both now tested: a confirmation sent while the registry is unavailable had no test (`watch.rs::unavailable_registry_keeps_the_board_and_deletes_nothing` asserts its `registry_unavailable` answer), and the note a filed-but-not-decided row leaves had no Playwright case (`triage.spec.ts > a session filed without its verdict says its subject's row decides`). Mutation checks: removing the `event.repeat` guard turns `> a held Enter confirms once` red (the case uses a refusal so the in-flight guard cannot hide it); committing per item would turn the crash test's "the stalled batch committed nothing" and `batch_is_one_transaction_answered_per_item` red.
- **Per-commit gates:** fmt; clippy on the workspace with `officina-core/ts` and on the host with and without `window-authority`; the tests of the crates touched; the crash file in release; `pnpm check`, `lint`, `test`; Playwright on the triage spec, then the whole suite (82 passed). The full gate runs at the end of story 3.8.

### Completion Notes

- AC 1: the triage cases of the `confirm` section of `tests/vectors/triage.json`, replayed by `crates/officina-core/tests/triage_vectors.rs::every_case_matches` and through the writer by `crates/officina-host/tests/registry_write.rs::subject_vectors_hold_through_the_writer`, with `every_value_has_a_case` now iterating `ConfirmRefusal::ALL`, `NotNeeded::ALL` and the confirmed `Verdict::ALL`; core `crates/officina-core/src/registry/confirm.rs::tests::{changed_proposal_is_refused, unfiled_row_is_filed_then_decided, verdict_not_needed_is_answered_with_its_cause, empty_subject_row_archives_without_input, confirmation_input_expires_with_its_excerpts}`.
- AC 2: `crates/officina-host/tests/registry_write.rs::{batch_is_one_transaction_answered_per_item, finished_leaves_one_expiring_copy_then_none, confirmation_copies_the_proposal_it_answered}`, at explicit `now_ms`.
- AC 3: `crates/officina-host/tests/registry_crash.rs::killed_while_confirming_keeps_every_acknowledged_confirmation_once` (`#[ignore]`, CI release crash step).
- AC 4: `crates/officina/tests/watch.rs::a_closed_weekend_loses_no_session_subject_or_verdict`.
- AC 5: `app/e2e/triage.spec.ts > each row shows its proposal, source, reason and excerpts`, `> an unfiled row proposes the obvious subject and lets me choose or name one`, `> an empty subject is offered for archiving`; Vitest `app/src/components/components.test.ts > TriageRow renders the reason wording of every reason code`; `REASON_TEXT` is a `Record<ReasonCode, string>`, so `pnpm check` fails on a missing entry.
- AC 6: `triage.spec.ts > arrows move, f and t set the verdict, Enter confirms the focused row only`, `> a held Enter confirms once`, `> focus goes to the next row once the view drops the confirmed one`, `> an unsure row needs f or t before Enter`.
- AC 7: `triage.spec.ts > confirm all shown confirms the shown rows with one click`, `> its label counts the rows and the ones left out`; Vitest `components.test.ts > Enter and Space on Confirm all do nothing`.
- AC 8: `triage.spec.ts > the to-sort chip is neutral and opens the panel`; core `crates/officina-core/src/board.rs::tests::rows_to_sort_leave_waiting_count_unchanged`.
- AC 9: `triage.spec.ts > confirming finished offers the routine box`, `> a refused item keeps its row with the reason`.
- AC 10: `triage.spec.ts > excerpts and names render as text, never as HTML`; the `svelte/no-at-html-tags` lint.
- AC 11: `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` (now naming `confirm_triage`); the `triage` scenario of `app/scripts/e2e-fixtures.mjs`, built through `registry_fixture` and printed by the real `subjects --json` and `board --json`.
- AC 12: `CHANGELOG.md` `[Unreleased]` Added; README "The Triage panel".

## File List

- `crates/officina-core/src/registry/{types,proposal,verdict,confirm}.rs`, `crates/officina-core/src/board.rs`, `crates/officina-core/tests/{triage_vectors,typescript_bindings}.rs`, `tests/vectors/triage.json`
- `crates/officina-host/src/registry/{read,write,window}.rs`, `crates/officina-host/src/watch.rs`, `crates/officina-host/examples/registry_crash_helper.rs`, `crates/officina-host/tests/{registry_write,registry_crash}.rs`
- `crates/officina/tests/watch.rs`
- `app/src-tauri/src/main.rs`, `app/src-tauri/build.rs`, `app/src-tauri/capabilities/default.json`
- `app/src/{App.svelte,e2e-mock.ts,e2e-globals.d.ts}`, `app/src/lib/{ipc.ts,registry.svelte.ts,registry-text.ts}`, `app/src/lib/generated/{Proposal,ConfirmItem,ConfirmRefusal,FileUnder,ItemAnswer,ItemOutcome,NotNeeded}.ts`, `app/src/components/{TriagePanel,TriageRow,SubjectPicker,SubjectsView,Header}.svelte`, `app/src/components/components.test.ts`
- `app/scripts/e2e-fixtures.mjs`, `app/e2e/{fixtures.ts,triage.spec.ts}`
- `.github/workflows/ci.yml` (crash step comment)
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.7-triage-panel.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: AC 2 proves one expiring copy, then none (D-85); `confirmation_input` under the 30-day bound; empty-subject rows confirmed like any row (D-76); call id stored (D-88); crash test on acknowledged commits with a stall point (D-89); the chip's title rule asserted on `waiting_count` in core
- 2026-10-28 implemented: triage confirmation in core with its vector cases, the writer's one-transaction batch with the input it keeps, the crash helper's `confirm` mode, the collector's `Confirm` request, the `confirm_triage` command, the Triage panel with its keys, Confirm all and the to-sort chip, the `triage` fixture scenario; two adversarial tests added. Status review
