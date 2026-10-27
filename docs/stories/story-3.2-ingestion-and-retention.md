# Story 3.2: Ingestion before retention, the answer's tail in the hook

**Status:** Approved

## Story

As the author who closes the window on Friday and reopens it on Monday,
I want every event line ingested into the registry before anything is deleted, with the end of each answer kept beside its start,
so that a closed weekend loses nothing and the Subjects view can show how the last answer ended.

## Acceptance Criteria

1. Event line (AD-45 first bullet, spine amendment to AD-8, FR-4 as amended): `last_message_tail` is an optional field of version 1, no version bump; it holds the last 280 characters of `last_assistant_message` on a character boundary, led by an ellipsis when cut, and is absent when the head holds the whole message; over `MAX_LINE_BYTES` the tail is dropped first, then the other excerpts; `no_excerpts` (environment or the floor the hook resolves) drops it with the head. Proof: core `crates/officina-core/src/events.rs::tests::{tail_is_the_last_280_characters_on_a_boundary, tail_is_absent_when_the_head_holds_the_message, oversized_line_drops_the_tail_first, line_with_the_new_field_parses_as_version_1}`; CLI `crates/officina/tests/hook.rs::{stop_records_the_answer_head_and_tail, no_excerpts_drops_the_tail_too}`; `crates/officina/tests/perf.rs::hook_latency_p95` stays green.
2. Order and offsets (AD-43, D-49): valid file names ascending, lines in file order; the offset in `event_file.ingested_bytes` advances to the last `\n` only; a partial tail stays unread, except in a file dated before yesterday, where it is one corrupt line; a file dated before today is complete once fully read and past the grace of D-49; corrupt lines are counted per file. Proof: host `crates/officina-host/tests/registry_ingest.rs::{files_are_ingested_oldest_first, offset_stops_at_the_last_newline, partial_tail_stays_unread_until_it_is_completed, partial_tail_before_yesterday_is_one_corrupt_line, file_before_today_is_complete_only_after_the_grace, corrupt_lines_are_counted_per_file}`.
3. Chunks (AD-43): one transaction per chunk of at most 2000 lines or 1 MiB (apply, count corrupt lines, advance the offset, commit); replaying a chunk changes nothing; a fact or excerpt is replaced only by an observation with a greater `ts`, whatever the order. Proof: `registry_ingest.rs::{chunk_boundary_commits_each_chunk, replayed_chunk_changes_nothing, later_observation_wins_whatever_the_order}`.
4. Session rows and excerpts (FR-3 first part, FR-4, AD-40, D-50, D-53): the first line of a session creates its `session` row (project per D-53, first and last seen); `session_excerpt` keeps the latest prompt and the head and tail of the latest answer with their event times; `no_excerpts` (environment or the window's floor) drops excerpts at ingestion. Facts and provenance are story 3.3. Proof: `registry_ingest.rs::{first_line_creates_the_session_with_its_project, project_is_not_rewritten_by_a_floor_change, excerpts_keep_the_prompt_and_the_answer_head_and_tail, no_excerpts_drops_excerpts_at_ingestion}`.
5. Lines whose `cwd` lies under `<officina home>/triage/` are skipped, in either slash form (AD-43). Proof: `registry_ingest.rs::triage_workdir_lines_are_skipped`.
6. Retention per surface (AD-44, FR-2 as amended by amendment 2, spine amendment to AD-9, D-48, D-50): event files per D-48, never while the registry is unavailable or read-only; excerpts cleared `EXCERPT_TTL_MS` after their event time, then `wal_checkpoint(TRUNCATE)`; daily copies, quarantine and pre-migration copies kept two and for 2 days. Core holds the rules and bounds. Proof: core `crates/officina-core/src/registry/retention.rs::tests::{excerpt_ttl_plus_copy_lifetime_stays_within_30_days, event_file_is_deletable_only_when_every_kept_copy_holds_it_complete}`; host `crates/officina-host/tests/registry_retention.rs::{four_utc_days_are_ingested_before_any_deletion, deletion_waits_for_both_copies, a_damaged_copy_does_not_count, unavailable_registry_deletes_nothing, read_only_registry_deletes_nothing, excerpts_expire_from_their_event_time, expired_text_leaves_the_wal, copies_quarantine_and_pre_migration_copies_kept_two_for_2_days}`, every function called with an explicit `now_ms`.
7. Collector (AD-43 "When", AD-44, AD-3, AD-4, D-44): at start the collector opens the writer (story 3.1), ingests every file not yet ingested, then runs the first hourly pass, then builds the first board; when today's file grows it ingests before the recompute; the hourly pass takes the daily copy when due, then retains; v0.2's journal and approval cleanups run in the same pass, ungated. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{start_ingests_before_retention_and_the_first_board, today_growth_is_ingested_before_the_recompute}` and host `registry_retention.rs::hourly_pass_takes_the_copy_before_retaining`.
8. Registry unavailable or read-only (FR-1, AD-41, AD-42, D-47, P-13): the board keeps working from live sources, nothing is deleted, and `Diagnostics.registry` says why; the Board view shows a banner; `BOARD_SCHEMA` goes up by one. Proof: `watch.rs::unavailable_registry_keeps_the_board_and_deletes_nothing`; core `crates/officina-core/src/board.rs::tests::registry_state_reaches_the_diagnostics`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; Playwright `app/e2e/board.spec.ts > an unavailable registry is reported and the board still shows sessions` on a `registry-unavailable` fixture whose `registry.db` holds bytes that are not a database, written by `scripts/e2e-fixtures.mjs`.
9. No CLI deletes anything (FR-2, AD-44, AD-53): `officina board --json` runs no retention and reads the registry read-only to report its state; no ungated deletion of event files is left in the code. Proof: `crates/officina/tests/board.rs::{board_json_deletes_no_event_file, board_json_reports_the_registry_state}`; the v0.1 test `crates/officina-host/src/files.rs::tests::retention_removes_only_expired_event_files` is replaced by the gated tests of AC 6.
10. Crash, ingestion part (AD-58 crash row, PRD section 7 Memory): the crash helper gains mode `ingest`; a test kills it 20 times at random points, then `quick_check` passes, every offset sits at a line end and no session or excerpt is duplicated. Proof: `#[ignore]` test `crates/officina-host/tests/registry_crash.rs::killed_twenty_times_while_ingesting_loses_nothing`, run by a new CI step `cargo test --release -p officina-host --features window-authority --test registry_crash -- --ignored`.
11. Doctor warns when more than 7 event files wait for ingestion or deletion (AD-44, R21). Proof: `crates/officina-host/src/doctor.rs::tests::more_than_seven_waiting_event_files_warn`.
12. Zero write: the collector run of the fingerprint test now ingests. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
13. Documentation (D-73): `CHANGELOG.md` `[Unreleased]` (Changed: event files are deleted only once ingested and copied; `board --json` deletes nothing) and the README's privacy section and "Subjects and triage" section (the answer's tail, what is kept and for how long). Proof: review; `readme.rs` stays green when it exists.

## Tasks

- [ ] `events.rs`: `last_message_tail`, bounded line order (AC 1)
- [ ] `crates/officina-host/src/registry/ingest.rs`: order, offsets, grace, chunks, sessions and excerpts, exclusion, `no_excerpts` (AC 2-5)
- [ ] `crates/officina-core/src/registry/retention.rs` and the host hourly pass: copies, event files per D-48, excerpts, copy lifetimes (AC 6, 7)
- [ ] `watch.rs` and `collector.rs`: writer open at start, ingestion before the first pass and the first board, ingestion on growth, one hourly pass (AC 7)
- [ ] `Diagnostics.registry`, `BOARD_SCHEMA` bump, regenerate TypeScript, Board banner (AC 8)
- [ ] `main.rs`: remove the retention call from `board --json`; read-only registry state (AC 9)
- [ ] Crash helper mode `ingest`, `registry_crash.rs`, CI release step (AC 10)
- [ ] Doctor waiting-files warning (AC 11)
- [ ] Fixture `registry-unavailable` in `scripts/e2e-fixtures.mjs`; Playwright case (AC 8)
- [ ] Update `CHANGELOG.md` and the README (AC 13)

## Dev Notes

- Requirements: FR-1 (unavailable registry reported on the board), FR-2 as amended (amendment 2), FR-3 first part, FR-4 as amended (amendment 4). Decisions: AD-3, AD-4, AD-41, AD-42, AD-43, AD-44, AD-45 (first bullet; `absent` is story 3.8), AD-53 (`board --json` part), spine amendments to AD-8 and AD-9. Epic decisions: P-9, P-13, D-44, D-47 to D-50, D-53.
- v0.2 seams (D-44): only `events::{EventLine, reduce_payload}` change in the hook, never the dispatch of AD-26. The hourly pass becomes one host function; v0.2 stories 2.8 and 2.9 add the approval and journal cleanups to it, ungated, in whichever order the branches merge. `files::retention_pass` disappears with its last caller.
- `BOARD_SCHEMA` goes up by one relative to `main` at merge time (P-13): stories 3.4 and 3.8 extend `Board` under the same number. Every board fixture regenerates.
- The board still reads today's and yesterday's event files itself (AD-9); ingestion does not change classification, so the classification vectors are untouched.
- Excerpt times are event times (D-50): a weekend with the window closed does not extend them.
- The tail goes first when a line is too long because the head is what the board shows; the head, prompt and message follow the v0.1 rule.
- The crash test needs the release build to finish in reasonable time; kill points are random offsets of a timer, seeded and printed so a failure can be replayed.

## Testing

- Core unit: `crates/officina-core/src/{events,board,registry/retention}.rs`.
- Host integration: `crates/officina-host/tests/{registry_ingest,registry_retention,registry_crash}.rs`.
- CLI integration: `crates/officina/tests/{hook,board,watch,quota_doctor}.rs`.
- Bindings: `crates/officina-core/tests/typescript_bindings.rs`.
- Playwright: `app/e2e/board.spec.ts`.
- Ignored release tests: `perf.rs`, `registry_crash.rs`, CI `windows` job.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
