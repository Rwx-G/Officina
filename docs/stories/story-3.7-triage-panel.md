# Story 3.7: Triage panel and batch confirmation

**Status:** Approved

## Story

As the author sorting four finished subjects on a Monday morning,
I want a Triage panel that shows each proposal with its source, reason and excerpts, lets me confirm or flip one row from the keyboard, file an unfiled session in the same gesture, and confirm every shown row with one click,
so that closing finished work takes seconds and every confirmation records what it answered.

## Acceptance Criteria

1. Confirmation in core (AD-49, P-11, D-60, D-62, D-63): `registry::confirm` takes items carrying the row key and the proposal shown; an item whose recomputed proposal differs is refused with a typed reason; an unfiled row is filed first (creating a named subject when asked), then the verdict applies per D-60; `finished` archives (with the routine flag of D-62) and clears the subject's excerpts, `to_resume` sets the status; the confirmation copies the proposal's verdict, source and `ReasonCode`, never prose; `confirmation_input` copies the excerpts and the facts the decision rested on, expiring when those excerpts were due to. Proof: the triage cases of the `confirm` section of `tests/vectors/triage.json`; core `crates/officina-core/src/registry/confirm.rs::tests::{changed_proposal_is_refused, unfiled_row_is_filed_then_decided, verdict_not_needed_is_answered_with_its_cause, confirmation_input_expires_with_its_excerpts}`.
2. Writer (AD-49, AD-39): `confirm_triage` runs one `FULL` transaction and answers per item; the excerpt purge runs `wal_checkpoint(TRUNCATE)`. Proof: host `crates/officina-host/tests/registry_write.rs::{batch_is_one_transaction_answered_per_item, finished_clears_excerpts_from_the_file_and_the_wal, confirmation_copies_the_proposal_it_answered}`.
3. Crash, confirmation part (AD-58 crash row): the crash helper gains mode `confirm` (ingests and confirms in a loop); after 20 kills at random points every committed confirmation is present once, with its input, and `quick_check` passes. Proof: `#[ignore]` `crates/officina-host/tests/registry_crash.rs::killed_twenty_times_while_confirming_keeps_every_committed_confirmation`, in the CI release step of story 3.2.
4. Restart (journey 5, PRD section 7 Memory): with the window stopped, three days of event files are written in which sessions end; at the next start every session that ended meanwhile is in the registry, and every subject and confirmation made before the stop is still there. Proof: CLI-crate collector test `crates/officina/tests/watch.rs::a_closed_weekend_loses_no_session_subject_or_verdict`.
5. Triage panel (FR-9, AD-55, D-61): the panel heads the Subjects view; each row shows the proposal, its source, its reason through an exhaustive `Record<ReasonCode, string>`, the excerpts, and for an unfiled row the proposed subject with the choices and a new-name field. Proof: Playwright `app/e2e/triage.spec.ts > each row shows its proposal, source, reason and excerpts`, `> an unfiled row proposes the obvious subject and lets me choose or name one`; Vitest `app/src/components/components.test.ts > TriageRow renders the reason wording of every reason code` (iterating the generated `ReasonCode` values); `pnpm check` fails on a missing `Record` entry.
6. Keys (AD-55): arrows move focus; `f` and `t` set the focused row's verdict; `Enter` confirms that row only; `event.repeat` is ignored; after a confirm, `Enter` waits for the view that removes the row and focus moves to the row taking its place; an `unsure` row has no verdict until `f` or `t` (D-59). Proof: `triage.spec.ts > arrows move, f and t set the verdict, Enter confirms the focused row only`, `> a held Enter confirms once`, `> focus goes to the next row once the view drops the confirmed one`, `> an unsure row needs f or t before Enter`.
7. Confirm all shown (AD-55, D-28): pointer only (`event.detail > 0`); its label carries the count; `unsure` rows, rows without a subject and rows in flight are left out and counted apart. Proof: `triage.spec.ts > confirm all shown confirms the shown rows with one click`, `> its label counts the rows and the ones left out`; Vitest `components.test.ts > Enter and Space on Confirm all do nothing`.
8. The "N to sort" chip (AD-55, D-61): neutral colour, never amber, absent from the window title and the taskbar overlay; a click opens the Subjects view on the panel. Proof: `triage.spec.ts > the to-sort chip is neutral and opens the panel`; app-crate `app/src-tauri/src/main.rs::tests::window_title_ignores_rows_to_sort`.
9. Confirming `finished` offers the routine box (FR-13, D-62); a refused item shows its reason on its row and keeps it. Proof: `triage.spec.ts > confirming finished offers the routine box`, `> a refused item keeps its row with the reason`.
10. Session and subject text renders as text (AD-55). Proof: `triage.spec.ts > excerpts and names render as text, never as HTML`; the `svelte/no-at-html-tags` lint.
11. Command and fixtures: `confirm_triage` is declared, allowed, wrapped in `ipc.ts` and mocked (AD-54); a `triage` scenario from `registry_fixture` (D-72) holds a multi-session subject, a state-proposed row, a cut-mid-turn row, an unfiled row with one obvious subject and one with none. Proof: `main.rs::tests::every_declared_command_is_allowed`; CI fixture step and the Playwright cases.
12. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": the panel, its keys and Confirm all. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/registry/confirm.rs`: triage confirmation, item outcomes (AC 1)
- [ ] Host writer `confirm_triage`, `confirmation_input`, purge and checkpoint (AC 2)
- [ ] Crash helper mode `confirm`; `registry_crash.rs` case (AC 3)
- [ ] Closed-weekend collector test (AC 4)
- [ ] Components `TriagePanel`, `TriageRow`; `SubjectPicker` reuse; key handling; Confirm all; chip in `Header.svelte` (AC 5-10)
- [ ] `confirm_triage` command, capability, `ipc.ts`, mock; `triage` scenario (AC 11)
- [ ] `confirm` cases in the vector set; `app/e2e/triage.spec.ts`; Vitest cases (AC 1, 5-10)
- [ ] Update `CHANGELOG.md` and the README (AC 12)

## Dev Notes

- Requirements: FR-9, FR-13 (the box on a `finished` confirmation), journeys 2, 3 and 5, PRD section 7 Memory and Attention. Decisions: AD-39, AD-40, AD-49, AD-54, AD-55, AD-58 (crash row). Epic decisions: D-28 (pointer-only rule, Epic 2), P-11, D-59 to D-63, D-72.
- The refusal of a changed proposal mirrors `approve_catalogue` refusing a hash that moved (AD-24): the author decides on what was shown, never on what the writer computed later.
- `confirmation_input` is the evaluation set of the dry run (AD-52, story 3.9); its expiry follows the excerpts it copies, so FR-4's 30-day bound holds (AD-44).
- Excluding rows in flight from Confirm all prevents one row being confirmed twice when a click lands during a keyboard confirm.
- The chip is neutral on purpose: triage blocks no session, and amber marks only what waits on the author (frontend rule, AD-55).
- Board digits work only in the Board view (story 3.5); `f`, `t` and `Enter` act only in the Triage panel.

## Testing

- Core unit: `crates/officina-core/src/registry/confirm.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host integration: `crates/officina-host/tests/{registry_write,registry_crash}.rs`.
- CLI-crate collector: `crates/officina/tests/watch.rs`.
- App crate unit: `app/src-tauri/src/main.rs`.
- Vitest: `app/src/components/components.test.ts`. Playwright: `app/e2e/triage.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
