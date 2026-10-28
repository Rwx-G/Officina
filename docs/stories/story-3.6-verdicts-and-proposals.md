# Story 3.6: What needs a verdict, proposals, subjects to file under

**Status:** review

## Story

As the author who wants to close finished work without reading every session,
I want Officina to list only the subjects whose sessions have all ended, each with one proposal saying where it came from and why, empty subjects offered for archiving, and, for an unfiled session, the subject it should go under,
so that I give one verdict per finished subject, never one per session, no subject stays open forever, and every proposal is checkable.

## Acceptance Criteria

1. Rows (FR-7 as amended by amendment 7, AD-47, AD-49, D-75, D-76): core `registry::verdict` lists an `active` subject whose sessions have all ended and that has no unreopened confirmation, anchored on its latest session that saw a prompt, superseded or not; and an ended, unfiled, interactive session that saw a prompt and is not superseded. Superseded unfiled sessions, non-interactive runs and sessions without a prompt give no row of their own. Proof: the `triage` section of `tests/vectors/triage.json` (state in, rows out), replayed by `crates/officina-core/tests/triage_vectors.rs::every_case_matches`, with cases for a latest session that saw no prompt, `/clear` then exit, and a `clear` successor filed elsewhere or left unfiled; core `crates/officina-core/src/registry/verdict.rs::tests::{one_row_per_subject_not_per_session, anchor_is_the_latest_session_that_saw_a_prompt, a_live_session_keeps_the_subject_off_the_list, a_reopened_confirmation_does_not_count}`.
2. Empty subjects (D-54, D-76): an `active` subject none of whose sessions saw a prompt, including one left with no session, gives a row of its own kind, "empty subject: archive?", proposing `finished` with source `rules` and its own `ReasonCode`. Proof: the `triage` cases "subject whose only session never prompted", "subject left with no session"; `verdict.rs::tests::empty_subject_gets_its_own_row`.
3. Proposals (FR-8, AD-49, D-59): core `registry::proposal`, first match wins: `state` (a background `done` proposes `finished`, `failed` and `stopped` propose `to_resume`); `rules` (cut mid-turn); `model` when the input says the model is enabled and answered, `unsure` allowed; default `to_resume`, source `rules`. A `SessionEnd` with `prompt_input_exit` after a `Stop` proposes nothing by itself. Each proposal carries a `ReasonCode`, never prose, and is recomputed on every view build, never stored. Proof: the `triage` cases; `crates/officina-core/src/registry/proposal.rs::tests::{first_match_wins, prompt_input_exit_after_stop_falls_to_the_default, model_answer_replaces_only_the_default, proposals_are_recomputed_not_stored}`.
4. Subject for an unfiled row (AD-49): proposed when the session's project has exactly one `active` or `to_resume` subject; the row lists the project's open subjects and accepts a new name. Proof: the `triage` cases.
5. `RegistryView` gains the rows (kind, subject or session, proposal with source and reason code, the excerpts, the proposed subject and the choices) and their count, which the window's "N to sort" chip will show (story 3.7); `officina subjects --json` prints them. Proof: core `crates/officina-core/src/registry/view.rs::tests::rows_and_their_count_follow_the_registry`; CLI `crates/officina/tests/subjects.rs::rows_carry_proposal_source_and_reason`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
6. Attention metric (PRD section 7): a seeded subject with several ended sessions, including a cleared one, yields exactly one row, on its latest session that saw a prompt. Proof: a named `triage` case and CLI `subjects.rs::multi_session_subject_needs_one_verdict`.
7. Coverage (AD-57, D-45): `triage_vectors.rs::every_value_has_a_case` now also iterates `RowKind::ALL`, `ReasonCode::ALL`, `ProposalSource::ALL`, `ModelVerdict::ALL` and `Verdict::ALL` (the model's slot is pure here, so its cases exist before the runner of story 3.9). Proof: that test.
8. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": which subjects need a verdict, empty subjects, and where proposals come from. Proof: review.

## Tasks

- [x] `crates/officina-core/src/registry/verdict.rs`: rows, anchor, empty subjects (AC 1, 2)
- [x] `crates/officina-core/src/registry/proposal.rs`: sources in order, reason codes, model slot as input (AC 3)
- [x] Subject proposal and choices for unfiled rows (AC 4)
- [x] `RegistryView` rows and count; regenerate TypeScript (AC 5)
- [x] `triage` section of `tests/vectors/triage.json`; coverage additions (AC 1-4, 6, 7)
- [x] CLI tests in `subjects.rs` (AC 5, 6)
- [x] Update `CHANGELOG.md` and the README (AC 8)

## Dev Notes

- Requirements: FR-7 as amended (amendment 7), FR-8, PRD section 7 Attention. Decisions: AD-47 (ended, superseded, non-interactive), AD-49 (rows, proposals, subject for an unfiled row), AD-57. Facts: E1 (a background `done` session is alive and idle). Epic decisions: P-17, D-45, D-54, D-59, D-75, D-76.
- D-76 departs from AD-49's anchor (P-17): the row is the subject's, so a subject whose latest session never saw a prompt, or ended with `/clear` then exit, still reaches triage on the last session that did; a subject with no prompted session at all is offered for archiving instead of staying `active` forever.
- Supersession only removes a session's own row (D-75): inside a subject, the superseded session can be the anchor.
- "Cut mid-turn" reads the last `UserPromptSubmit` and the last `Stop` times recorded as facts by story 3.3, never the transcript (PRD non-goals).
- The empty-subject rule is the one `rules` proposal of `finished`: its false-`finished` rate counts from now on (story 3.10). Auto-archive never takes it (D-70).
- Proposals from `state` and `rules` are recomputed on every view build and their reason is a `ReasonCode` (AD-40); nothing here writes. Confirmation is story 3.7.
- The model slot is a pure input (`enabled`, `answer`): the vectors cover it now, the runner and the queue come with stories 3.9 and 3.11.
- Reopening sets `to_resume` (AD-48), which keeps a reopened subject off the list until a session of it comes alive again (D-77) and ends.

## Testing

- Core unit: `crates/officina-core/src/registry/{verdict,proposal,view}.rs`.
- Core integration: `crates/officina-core/tests/{triage_vectors,typescript_bindings}.rs`.
- CLI integration: `crates/officina/tests/subjects.rs`.

## Dev Agent Record

### Debug Log

- **Decisions where the story is silent:**
  - AC 1, "unreopened confirmation": a confirmation answers a row while it is not reopened, is on the row's session (none for an empty subject, any subject for an unfiled row), and was made at or after the end the row rests on (the anchor's end, or the empty subject's last status change). Without the last condition, a subject confirmed `to_resume` and woken by a session alive again (D-77, which stamps a reopen only for an archived subject) would stay `active` with a stale confirmation forever, against the story's "no subject stays open forever". Confirmations are read from `confirmation` (subject, session, times); story 3.7 writes them.
  - AC 1, "interactive": an unfiled row excludes non-interactive runs (AD-47), not background sessions: the AC's list of what gives no row names superseded sessions, non-interactive runs and sessions without a prompt, and the `state` rule needs background anchors. A subject's anchor is chosen among every session counting under it, a run the author filed included (D-56); runs filed by rule do not count (story 3.4).
  - AC 1: the anchor is the session with the latest prompt time, then latest seen, then id. Rows are listed latest first: by the anchor's end, an empty subject by its last status change, then by key (`subject:<id>`, `session:<id>`), which story 3.7's confirmation items can carry.
  - AC 3: `registry::proposal::ModelInput { enabled, answers }` (answers by session id) is the model's pure slot; the view passes it disabled until stories 3.9 and 3.11. "Cut mid-turn" is a prompt later than the last `Stop`, or a prompt no `Stop` followed. `Proposal::definite()` is the verdict confirming as shown applies, none for `unsure` (D-59), which the row carries as `verdict` and which the coverage test reads for `Verdict::ALL`.
  - AC 5: `RegistryView.triage: Vec<TriageRow>` (key, kind, project, subject as `SubjectRef`, session id and name, proposal, verdict, the anchor's excerpts and end, proposed subject, choices as `SubjectRef`) and `to_sort`. `REGISTRY_VIEW_SCHEMA` stays 1 (P-13).
- **Adversarial pass** (a live session beside ended ones, a clear successor filed elsewhere or left unfiled, `/resume` within the process, a run filed by the author, a subject emptied by unfiling, a verdict then a new end, a reopened verdict, an `unsure` answer, the model disabled with an answer, unfiled rows in a project with one, several or no open subject): one real miss. The first `triage` case for a reopened confirmation also brought its row back through a later end, so dropping the reopen check left the set green (mutation check); the case now reaches a reopened confirmation with no later end through a merge into the reopened subject, and the same mutation turns it red. Mutation check: dropping the supersession check of unfiled rows turns "superseded unfiled sessions ... give no row" red.

### Completion Notes

- AC 1: the `triage` section of `tests/vectors/triage.json`, replayed by `crates/officina-core/tests/triage_vectors.rs::every_case_matches`, with the cases "a latest session that saw no prompt leaves the anchor on the one before", "clear then exit anchors the row on the cleared session", "a clear successor filed elsewhere gives each subject its row", "a clear successor left unfiled gives its own row and the subject keeps its row", "superseded unfiled sessions, non-interactive runs, sessions without a prompt and live ones give no row"; core `crates/officina-core/src/registry/verdict.rs::tests::{one_row_per_subject_not_per_session, anchor_is_the_latest_session_that_saw_a_prompt, a_live_session_keeps_the_subject_off_the_list, a_reopened_confirmation_does_not_count, unfiled_rows_need_an_end_a_prompt_and_no_successor}`.
- AC 2: the cases "subject whose only session never prompted", "subject left with no session"; `verdict.rs::tests::empty_subject_gets_its_own_row`.
- AC 3: the cases "a background state proposes from the state", "a turn cut mid-way proposes to_resume by rule", "prompt_input_exit after a Stop proposes nothing by itself", "the model answers where the default would, unsure included", "a disabled model leaves the default"; `crates/officina-core/src/registry/proposal.rs::tests::{first_match_wins, prompt_input_exit_after_stop_falls_to_the_default, model_answer_replaces_only_the_default, proposals_are_recomputed_not_stored}`.
- AC 4: the case "an unfiled session is proposed its project's only open subject, offered several, or none".
- AC 5: `crates/officina-core/src/registry/view.rs::tests::rows_and_their_count_follow_the_registry`; `crates/officina/tests/subjects.rs::rows_carry_proposal_source_and_reason`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
- AC 6: the case "a subject with several ended sessions, a cleared one among them, needs one verdict"; `subjects.rs::multi_session_subject_needs_one_verdict`.
- AC 7: `triage_vectors.rs::every_value_has_a_case` iterates `RowKind::ALL`, `ReasonCode::ALL`, `ProposalSource::ALL`, `ModelVerdict::ALL` and `Verdict::ALL` over the `triage` section.
- AC 8: `CHANGELOG.md` `[Unreleased]` Added; README "What needs a verdict".

### Full gate at the end of story 3.6

Every job of `.github/workflows/ci.yml`, run locally with `--locked`, covering stories 3.5 and 3.6: `cargo fmt --check`; frontend install (frozen lockfile) and build; clippy `--workspace --all-targets --features officina-core/ts` clean, in the `core-linux` feature set, and on the host without `window-authority`; `cargo test --workspace --features officina-core/ts` 835 passed, 0 failed, 10 ignored (TypeScript drift green); `perf.rs` alone in release 5 passed (hook own cost 8.1 ms, budget 150 ms; gate own cost 2.7 ms, budget 15 ms; in-process p95 3.4 ms); the lexer differential 2 passed; `approvals_stress` passed; the crash step 2 passed (helper built first); "officina holds no authority" verbatim: both markers absent from `officina.exe`, present in `officina-app.exe`; "Registry dependencies" ok; "No bundled roots" ok; `pnpm check`, `lint`, `test` 45 passed; fixtures rebuilt from the real CLI (17 scenarios, the four registry ones through `registry_fixture`), Playwright 68 passed, and the board and subjects specs 114 passed with `--repeat-each=3`; `cargo deny --locked check` ok; `pnpm audit` 1 low, ignored with its reason (unchanged). The `core-linux` job replayed in a `rust:1-bookworm` container on the committed tree (ce18242) with Rust 1.99.0: fmt, clippy clean, 724 passed, 0 failed, 8 ignored. Node 25.4 locally against CI's 22: no difference showed.

## File List

- `crates/officina-core/src/registry/{mod,types,proposal,verdict,view}.rs`, `crates/officina-core/tests/triage_vectors.rs`, `tests/vectors/triage.json`
- `crates/officina-host/src/registry/read.rs`
- `crates/officina/tests/subjects.rs`
- `app/src/lib/generated/{RegistryView,TriageRow,Proposal,ProposalSource,ReasonCode,RowKind,ModelVerdict,Verdict}.ts`
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.6-verdicts-and-proposals.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: rows anchored on the latest session that saw a prompt, superseded sessions give no row of their own (D-75, D-76); "empty subject: archive?" rows (D-76); vector cases for a latest session without a prompt, `/clear` then exit, and a successor filed elsewhere
- 2026-10-28 implemented: rows per finished subject, empty subjects and unfiled sessions, proposals from state, rules and the model slot, the subject for an unfiled row, the rows and their count in `RegistryView` and `subjects --json`, the `triage` vector section and its coverage; one vector case made able to fail. Full gate green. Status review
