# Story 3.4: Subjects and filing, `officina subjects --json`, subjects on the board

**Status:** Approved

## Story

As the author who works by subject across several sessions and days,
I want new sessions filed under a subject only when the choice is obvious, a subject woken up when one of its sessions comes alive again, every subject action of mine recorded, and the result readable from the command line and on board cards,
so that my sessions group themselves without guesswork and I can see a session's subject wherever it appears.

## Acceptance Criteria

1. Filing at creation (FR-5, AD-48, D-53, D-75, D-79): every new session id, whatever its `SessionStart.source`, joins the single `active` subject of its project; zero or several leaves it unfiled; the two never-filed project names never file; filing a live session into a `to_resume` subject makes it `active`; filing runs in the transaction that creates the session, whichever path creates it (an event line or an agents record). A resumed conversation keeps its id and its subject. No subject is ever created by a rule. Proof: the `filing` section of `tests/vectors/triage.json`, replayed by `crates/officina-core/tests/triage_vectors.rs::every_case_matches`, including a `clear` successor and a `resume` of a known id; host `crates/officina-host/tests/registry_ingest.rs::filing_runs_in_the_creating_transaction`; CLI-crate collector test `crates/officina/tests/watch.rs::a_session_first_seen_in_agents_is_filed`.
2. Alive again (D-77, subject side): when a session of a `to_resume` or `archived` subject comes alive again (story 3.3 AC 5), the subject becomes `active` with source `rules`; an archived subject reopened this way gets `reopened_at_ms` on its latest confirmation (D-58). Proof: `confirm` cases "to_resume subject, session listed live", "archived subject, newer prompt", "auto-archived subject, background `done` then `working`"; host `crates/officina-host/tests/registry_write.rs::alive_again_reactivates_and_stamps_the_reopen`.
3. Author actions (AD-48, D-54 to D-58, D-95): writer methods taking `now_ms` create (with a project, D-95), rename, merge, file and unfile (one or several sessions), archive with `routine`, and reopen; each transition is a case of the `confirm` section of the vector set; each refusal is a variant of a fieldless refusal enum with `ALL`, distinct from the error type that carries the marker (D-57). Proof: the `confirm` cases; host `registry_write.rs::{author_actions_round_trip, create_takes_the_project_of_the_selected_sessions, filing_into_an_archived_subject_is_refused, merge_moves_sessions_and_marks_the_source, name_rules_are_enforced, every_author_refusal_has_a_case}` (the last iterates the refusal enum's `ALL` through an exhaustive scenario builder).
4. Author writes are `FULL` transactions and carry source `author` with their time in the source columns they touch (`session.subject_src`, `subject.status_src`) (AD-39, AD-40). Proof: `registry_write.rs::author_writes_carry_source_author_and_time`.
5. Latest excerpts per subject (FR-4, P-9, D-50): a filed session loses its excerpt row once a later session of its subject has one, including when it is filed or merged into such a subject. Proof: `registry_write.rs::older_session_loses_its_excerpts_once_a_later_one_has_them` and the `filing` cases.
6. Archive clears the excerpts of the subject's sessions and runs `wal_checkpoint(TRUNCATE)`, so the text is gone from the file and its WAL (FR-4, AD-44). Proof: `registry_write.rs::archive_clears_excerpts_from_the_file_and_the_wal` (byte search for a marker).
7. `RegistryView` (AD-54, P-13: `REGISTRY_VIEW_SCHEMA = 1`) is built in core: registry state, ingestion (with the rewritten-file count of D-84), active and `to_resume` subjects first with their sessions joined to the live board cards by session id, each subject's latest excerpts, unfiled sessions per project with non-interactive runs counted and the 10 most recent listed (D-96), the synced-folder note and the restore notice of D-93, thresholds; archived subjects are not in it (story 3.5 lists them on demand). Proof: core `crates/officina-core/src/registry/view.rs::tests::{active_and_to_resume_come_first, sessions_join_live_cards_by_session_id, unfiled_are_grouped_by_project, non_interactive_runs_are_counted_and_bounded, restore_notice_shows_for_7_days, archived_are_left_out}`; `RegistryView` exported in `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
8. `officina subjects --json` (FR-6 as amended by amendment 6, AD-53, D-47, D-82, D-97): prints `RegistryView` from the read-only registry and a fresh board, with `ingestion.pending_bytes` from `stat` against the stored offsets, and works with the window closed; exit 0 when the registry is readable, a stale board inside the view notwithstanding; 3 with the reason in the view whenever nothing could be read; 64 on usage; never 2. Proof: CLI `crates/officina/tests/subjects.rs::{prints_the_registry_view_with_the_window_closed, pending_bytes_show_the_lag_behind_a_closed_window, a_stale_board_still_exits_0, every_unreadable_state_exits_3_with_its_reason, usage_error_exits_64, subjects_json_changes_no_row}` (the fourth iterates the reader-state enum of story 3.1); `USAGE` lists the command.
9. Board cards show their subject (FR-6 last sentence, AD-54, P-13): `Card.subject: Option<SubjectRef>` from `BoardInput.subjects`; `officina board --json` fills it from the read-only registry, otherwise every subject is null and `diagnostics.registry` says why; no second schema bump. Proof: core `crates/officina-core/src/board.rs::tests::cards_carry_their_subject_by_session_id`; CLI `crates/officina/tests/board.rs::{cards_carry_their_subject, unreadable_registry_leaves_subjects_null_with_the_reason}`; the bindings test.
10. Zero write: `subjects --json` joins the fingerprint test (D-80). Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
11. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": what a subject is, the filing rule, what wakes a subject up, `officina subjects --json` in the command table. Proof: review; `readme.rs::readme_mentions_every_subcommand` covers `subjects` when it exists.

## Tasks

- [ ] `crates/officina-core/src/registry/filing.rs` and the author and alive-again transitions in `confirm.rs` (AC 1-3)
- [ ] Constants for the two never-filed project names in `board.rs`, used by `project_for` and filing (AC 1, D-53)
- [ ] Filing inside the creation function of story 3.2, for both paths (AC 1)
- [ ] `crates/officina-host/src/registry/write.rs`: author actions, refusal enum with `ALL`, error type, excerpt clearing, checkpoint (AC 3-6)
- [ ] `crates/officina-core/src/registry/view.rs`: `RegistryView`, builder from registry records and a `Board` (AC 7)
- [ ] `officina subjects --json` in `crates/officina/src/main.rs`; host read path; `USAGE` (AC 8)
- [ ] `Card.subject`, `BoardInput.subjects`, read-only fill in `board --json` and the collector; regenerate TypeScript (AC 9)
- [ ] `filing` and `confirm` cases in `tests/vectors/triage.json` (AC 1-3, 5)
- [ ] Tests: `registry_write.rs`, `subjects.rs`, `board.rs`, `watch.rs`, extend the zero-write test (AC 1-10)
- [ ] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-4, FR-5 (its amendment 5 read through D-75), FR-6 (data side, amendment 6). Decisions: AD-43 (filing in the creating transaction), AD-48, AD-53, AD-54. Epic decisions: P-9, P-13, P-17, D-47, D-50, D-53 to D-58, D-75, D-77, D-79, D-82, D-84, D-93, D-95 to D-97.
- D-75 departs from AD-48's resume clause (P-17): `--resume <id>` keeps the session id (documented), so the conversation simply continues its own record and subject, and a `resume` successor is a new id like any other. Supersession never files.
- D-77 is how a confirmed or archived subject comes back: a background `done` session re-instructed through `claude attach`, or a conversation resumed later. Without it the next end would never be triaged; with it, a wrong auto-archive becomes a counted flip (story 3.10).
- `subject.project` decides which new sessions may join (AD-48); the author can file sessions of any project under any non-archived subject.
- The filing rule reads the registry at ingestion time (AD-48), which lags only while the window is closed: `subjects --json` shows that lag in `ingestion.pending_bytes`.
- Author actions are tested here through the writer directly; the window's `AuthorRequest` path and the Tauri commands are story 3.5.
- `subjects --json` takes no `now_ms` argument and needs `claude agents` for the live join; tests run it against the fake claude like `board --json`. With the window closed the reader may create `-wal` and `-shm` (D-82); "changes no row" is what the test checks.
- `Card.subject` joins by the card's `session_id`, the current one from `claude agents` for a live card (E14), so a cleared session's card shows its successor's subject.

## Testing

- Core unit: `crates/officina-core/src/registry/{filing,confirm,view}.rs`, `src/board.rs`.
- Core integration: `crates/officina-core/tests/{triage_vectors,typescript_bindings}.rs`.
- Host integration: `crates/officina-host/tests/{registry_write,registry_ingest}.rs`.
- CLI integration and collector: `crates/officina/tests/{subjects,board,watch,quota_doctor}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: every new id follows the normal rule, resume keeps its id and subject, no filing through supersession (D-75); filing on both creation paths (D-79); alive again wakes the subject and stamps a reopen (D-77); `create_subject` takes a project (D-95); refusal enum split from the error type; non-interactive runs bounded (D-96); restore notice (D-93); `subjects --json` with the window closed and exit 0 on a stale board (D-82, D-97)
