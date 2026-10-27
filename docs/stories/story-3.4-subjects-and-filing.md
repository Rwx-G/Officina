# Story 3.4: Subjects and filing, `officina subjects --json`, subjects on the board

**Status:** Approved

## Story

As the author who works by subject across several sessions and days,
I want new sessions filed under a subject only when the choice is obvious, every subject action of mine recorded, and the result readable from the command line and on board cards,
so that my sessions group themselves without guesswork and I can see a session's subject wherever it appears.

## Acceptance Criteria

1. Filing at first sight (FR-5 as amended by amendment 5, AD-48, D-52, D-53): a `resume` successor joins its predecessor's subject; otherwise a session joins the single `active` subject of its project; zero or several leaves it unfiled; the two never-filed project names never file; filing a live session into a `to_resume` subject makes it `active`; filing runs in the transaction that creates the session (AD-43). No subject is ever created by a rule. Proof: the `filing` section of `tests/vectors/triage.json`, replayed by `crates/officina-core/tests/triage_vectors.rs::every_case_matches`; host `crates/officina-host/tests/registry_ingest.rs::filing_runs_in_the_creating_transaction`.
2. Author actions (AD-48, D-54 to D-58): writer methods taking `now_ms` create, rename, merge, file and unfile (one or several sessions), archive with `routine`, and reopen; each transition is a case of the `confirm` section of the vector set; each refusal is a variant of the writer's error enum. Proof: the `confirm` cases; host `crates/officina-host/tests/registry_write.rs::{author_actions_round_trip, filing_into_an_archived_subject_is_refused, merge_moves_sessions_and_marks_the_source, name_rules_are_enforced, every_author_error_has_a_case}` (the last iterates the error enum's `ALL` through an exhaustive scenario builder).
3. Author writes are `FULL` transactions and carry source `author` with their time in the source columns they touch (`session.subject_src`, `subject.status_src`) (AD-39, AD-40). Proof: `registry_write.rs::author_writes_carry_source_author_and_time`.
4. Latest excerpts per subject (FR-4, P-9, D-50): a filed session loses its excerpt row once a later session of its subject has one, including when it is filed or merged into such a subject. Proof: `registry_write.rs::older_session_loses_its_excerpts_once_a_later_one_has_them` and the `filing` cases.
5. Archive clears the excerpts of the subject's sessions and runs `wal_checkpoint(TRUNCATE)`, so the text is gone from the file and its WAL (FR-4, AD-44). Proof: `registry_write.rs::archive_clears_excerpts_from_the_file_and_the_wal` (byte search for a marker).
6. `RegistryView` (AD-54, P-13: `REGISTRY_VIEW_SCHEMA = 1`) is built in core: registry state, ingestion, active and `to_resume` subjects first with their sessions joined to the live board cards by session id, each subject's latest excerpts, unfiled sessions per project with non-interactive ones collapsed, the synced-folder note and the restored copy name, thresholds; archived subjects are not in it (story 3.5 lists them on demand). Proof: core `crates/officina-core/src/registry/view.rs::tests::{active_and_to_resume_come_first, sessions_join_live_cards_by_session_id, unfiled_are_grouped_by_project, non_interactive_are_listed_collapsed, archived_are_left_out}`; `RegistryView` exported in `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
7. `officina subjects --json` (FR-6 as amended by amendment 6, AD-53, D-47): prints `RegistryView` from the read-only registry and a fresh board, with `ingestion.pending_bytes` from `stat` against the stored offsets; exit 0; 3 with the reason in the view whenever nothing could be read; 64 on usage; never 2. Proof: CLI `crates/officina/tests/subjects.rs::{prints_the_registry_view, pending_bytes_show_the_lag_behind_a_closed_window, every_unreadable_state_exits_3_with_its_reason, usage_error_exits_64, subjects_json_writes_nothing}` (the third iterates the reader-state enum of story 3.1); `USAGE` lists the command.
8. Board cards show their subject (FR-6 last sentence, AD-54, P-13): `Card.subject: Option<SubjectRef>` from `BoardInput.subjects`; `officina board --json` fills it from the read-only registry, otherwise every subject is null and `diagnostics.registry` says why; no second schema bump. Proof: core `crates/officina-core/src/board.rs::tests::cards_carry_their_subject_by_session_id`; CLI `crates/officina/tests/board.rs::{cards_carry_their_subject, unreadable_registry_leaves_subjects_null_with_the_reason}`; the bindings test.
9. Zero write: `subjects --json` joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
10. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": what a subject is, the filing rule, `officina subjects --json` in the command table. Proof: review; `readme.rs::readme_mentions_every_subcommand` covers `subjects` when it exists.

## Tasks

- [ ] `crates/officina-core/src/registry/filing.rs` and the author transitions in `confirm.rs` (AC 1, 2)
- [ ] Constants for the two never-filed project names in `board.rs`, used by `project_for` and filing (AC 1, D-53)
- [ ] `crates/officina-host/src/registry/write.rs`: author actions, error enum with `ALL`, excerpt clearing, checkpoint (AC 2-5)
- [ ] `crates/officina-core/src/registry/view.rs`: `RegistryView`, builder from registry records and a `Board` (AC 6)
- [ ] `officina subjects --json` in `crates/officina/src/main.rs`; host read path; `USAGE` (AC 7)
- [ ] `Card.subject`, `BoardInput.subjects`, read-only fill in `board --json` and the collector; regenerate TypeScript (AC 8)
- [ ] `filing` and `confirm` cases in `tests/vectors/triage.json` (AC 1, 2, 4)
- [ ] Tests: `registry_write.rs`, `subjects.rs`, `board.rs`, extend the zero-write test (AC 2-9)
- [ ] Update `CHANGELOG.md` and the README (AC 10)

## Dev Notes

- Requirements: FR-4, FR-5 as amended (amendment 5), FR-6 (data side, amendment 6). Decisions: AD-43 (filing in the creating transaction), AD-48, AD-53, AD-54. Epic decisions: P-9, P-13, D-47, D-50, D-52 to D-58.
- `subject.project` decides which new sessions may join (AD-48); the author can file sessions of any project under any non-archived subject.
- The filing rule reads the registry at ingestion time (AD-48), which lags only while the window is closed: `subjects --json` shows that lag in `ingestion.pending_bytes`.
- Author actions are tested here through the writer directly; the window's `AuthorRequest` path and the Tauri commands are story 3.5.
- `subjects --json` takes no `now_ms` argument and needs `claude agents` for the live join; tests run it against the fake claude like `board --json`.
- The view's `state` is the reader or writer state of story 3.1 plus the location note and the restored copy name (AD-39, AD-42); the window shows them in story 3.5.
- `Card.subject` joins by the card's `session_id`, the current one from `claude agents` for a live card (E14), so a cleared session's card shows its successor's subject.

## Testing

- Core unit: `crates/officina-core/src/registry/{filing,confirm,view}.rs`, `src/board.rs`.
- Core integration: `crates/officina-core/tests/{triage_vectors,typescript_bindings}.rs`.
- Host integration: `crates/officina-host/tests/{registry_write,registry_ingest}.rs`.
- CLI integration: `crates/officina/tests/{subjects,board,quota_doctor}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
