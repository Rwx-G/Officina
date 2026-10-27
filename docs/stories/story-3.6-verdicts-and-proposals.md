# Story 3.6: What needs a verdict, proposals, subjects to file under

**Status:** Approved

## Story

As the author who wants to close finished work without reading every session,
I want Officina to list only the subjects whose sessions have all ended, each with one proposal saying where it came from and why, and, for an unfiled session, the subject it should go under,
so that I give one verdict per finished subject, never one per session, and every proposal is checkable.

## Acceptance Criteria

1. Rows (FR-7 as amended by amendment 7, AD-47, AD-49): core `registry::verdict` lists an `active` subject whose sessions have all ended, whose chain-head latest session saw a prompt and has no unreopened confirmation, and an ended, unfiled, interactive session that saw a prompt; superseded sessions, non-interactive runs and sessions without a prompt give no row. Proof: the `triage` section of `tests/vectors/triage.json` (state in, rows out), replayed by `crates/officina-core/tests/triage_vectors.rs::every_case_matches`; core `crates/officina-core/src/registry/verdict.rs::tests::{one_row_per_subject_not_per_session, a_live_session_keeps_the_subject_off_the_list, a_reopened_confirmation_does_not_count}`.
2. Proposals (FR-8, AD-49, D-59): core `registry::proposal`, first match wins: `state` (a background `done` proposes `finished`, `failed` and `stopped` propose `to_resume`); `rules` (cut mid-turn); `model` when the input says the model is enabled and answered, `unsure` allowed; default `to_resume`, source `rules`. A `SessionEnd` with `prompt_input_exit` after a `Stop` proposes nothing by itself. Each proposal carries a `ReasonCode`, never prose, and is recomputed on every view build, never stored. Proof: the `triage` cases; `crates/officina-core/src/registry/proposal.rs::tests::{first_match_wins, prompt_input_exit_after_stop_falls_to_the_default, model_answer_replaces_only_the_default, proposals_are_recomputed_not_stored}`.
3. Subject for an unfiled row (AD-49): proposed when the session's project has exactly one `active` or `to_resume` subject; the row lists the project's open subjects and accepts a new name. Proof: the `triage` cases.
4. `RegistryView` gains the rows (kind, subject or session, proposal with source and reason code, the excerpts, the proposed subject and the choices) and their count, which the window's "N to sort" chip will show (story 3.7); `officina subjects --json` prints them. Proof: core `crates/officina-core/src/registry/view.rs::tests::rows_and_their_count_follow_the_registry`; CLI `crates/officina/tests/subjects.rs::rows_carry_proposal_source_and_reason`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
5. Attention metric (PRD section 7): a seeded subject with several ended sessions, including a cleared and a resumed one, yields exactly one row, on its chain-head latest session. Proof: a named `triage` case and CLI `subjects.rs::multi_session_subject_needs_one_verdict`.
6. Coverage (AD-57, D-45): `triage_vectors.rs::every_value_has_a_case` now also iterates `RowKind::ALL`, `ReasonCode::ALL`, `ProposalSource::ALL`, `ModelVerdict::ALL` and `Verdict::ALL` (the model's slot is pure here, so its cases exist before the runner of story 3.9). Proof: that test.
7. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": which subjects need a verdict and where proposals come from. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/registry/verdict.rs`: rows (AC 1)
- [ ] `crates/officina-core/src/registry/proposal.rs`: sources in order, reason codes, model slot as input (AC 2)
- [ ] Subject proposal and choices for unfiled rows (AC 3)
- [ ] `RegistryView` rows and count; regenerate TypeScript (AC 4)
- [ ] `triage` section of `tests/vectors/triage.json`; coverage additions (AC 1-3, 5, 6)
- [ ] CLI tests in `subjects.rs` (AC 4, 5)
- [ ] Update `CHANGELOG.md` and the README (AC 7)

## Dev Notes

- Requirements: FR-7 as amended (amendment 7), FR-8, PRD section 7 Attention. Decisions: AD-47 (ended, superseded, non-interactive), AD-49 (rows, proposals, subject for an unfiled row), AD-57. Facts: E1 (a background `done` session is alive and idle). Epic decisions: D-45, D-59.
- "Cut mid-turn" reads the last `UserPromptSubmit` and the last `Stop` times recorded as facts by story 3.3, never the transcript (PRD non-goals).
- Proposals from `state` and `rules` are recomputed on every view build and their reason is a `ReasonCode` (AD-40); nothing here writes. Confirmation is story 3.7.
- The model slot is a pure input (`enabled`, `answer`): the vectors cover it now, the runner and the queue come with stories 3.9 and 3.11.
- Reopening sets `to_resume` (AD-48), which keeps a reopened subject off the list until a new session of it ends.

## Testing

- Core unit: `crates/officina-core/src/registry/{verdict,proposal,view}.rs`.
- Core integration: `crates/officina-core/tests/{triage_vectors,typescript_bindings}.rs`.
- CLI integration: `crates/officina/tests/subjects.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
