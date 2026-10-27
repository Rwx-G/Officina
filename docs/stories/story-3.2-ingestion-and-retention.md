# Story 3.2: Ingestion before retention, the answer's tail in the hook

**Status:** Approved

## Story

As the author who closes the window on Friday and reopens it on Monday,
I want every event line ingested into the registry before anything is deleted, with the end of each answer kept beside its start,
so that a closed weekend loses nothing and the Subjects view can show how the last answer ended.

## Acceptance Criteria

1. Event line (AD-45 first bullet, spine amendment to AD-8, FR-4 as amended): `last_message_tail` is an optional field of version 1, no version bump; it holds the last 280 characters of `last_assistant_message` on a character boundary, led by an ellipsis when cut, and is absent when the head holds the whole message; over `MAX_LINE_BYTES` the tail is dropped first, then the other excerpts; `no_excerpts` (environment or the floor the hook resolves) drops it with the head. Proof: core `crates/officina-core/src/events.rs::tests::{tail_is_the_last_280_characters_on_a_boundary, tail_is_absent_when_the_head_holds_the_message, oversized_line_drops_the_tail_first, line_with_the_new_field_parses_as_version_1}`; CLI `crates/officina/tests/hook.rs::{stop_records_the_answer_head_and_tail, no_excerpts_drops_the_tail_too}`; `crates/officina/tests/perf.rs::hook_latency_p95` stays green.
2. Passes, order and offsets (AD-43, D-49, D-84): every ingestion pass covers every incomplete event file, valid names ascending, lines in file order; the offset in `event_file.ingested_bytes` advances to the last `\n` only; a partial tail stays unread, except in a file dated before yesterday, where it is one corrupt line; a file dated before today is complete once fully read and past the grace of D-49; corrupt lines are counted per file; a file shorter than its stored offset is ingested again from 0, its corrupt count restarts, and the rewrite is counted in the view's ingestion diagnostics and in doctor. Proof: host `crates/officina-host/tests/registry_ingest.rs::{files_are_ingested_oldest_first, every_pass_covers_every_incomplete_file, offset_stops_at_the_last_newline, partial_tail_stays_unread_until_it_is_completed, partial_tail_before_yesterday_is_one_corrupt_line, yesterday_completes_after_the_grace_in_a_growth_pass, corrupt_lines_are_counted_per_file, a_file_shorter_than_its_offset_is_reingested_and_reported}`, all at explicit `now_ms`.
3. Chunks (AD-43): one transaction per chunk of at most 2000 lines or 1 MiB (apply, count corrupt lines, advance the offset, commit); replaying a chunk changes nothing; a fact or excerpt is replaced only by an observation with a greater `ts`, whatever the order. Proof: `registry_ingest.rs::{chunk_boundary_commits_each_chunk, replayed_chunk_changes_nothing, later_observation_wins_whatever_the_order}`.
4. Session creation and excerpts (FR-3 first part, FR-4, AD-40, D-50, D-53, D-79): the first line of a session creates its record through the single creation function of D-79 (row, project per D-53, first and last seen; filing joins it in story 3.4); `session_excerpt` keeps the latest prompt and the head and tail of the latest answer with their event times; `no_excerpts` (environment or the window's floor) drops excerpts at ingestion. Facts and provenance are story 3.3. Proof: `registry_ingest.rs::{first_line_creates_the_session_with_its_project, project_is_not_rewritten_by_a_floor_change, excerpts_keep_the_prompt_and_the_answer_head_and_tail, no_excerpts_drops_excerpts_at_ingestion}`.
5. Lines whose `cwd` lies under `<officina home>/triage/`, compared with `quota::normalise_path` (case and slashes), are skipped (AD-43, D-97). Proof: `registry_ingest.rs::triage_workdir_lines_are_skipped`, with the backslash, forward-slash and mixed-case forms.
6. Retention and privacy bounds (AD-44, FR-2 as amended by amendment 2, FR-4, spine amendment to AD-9, D-48, D-50, D-85, D-94): event files per D-48, never while the registry is unavailable or read-only; excerpts cleared `EXCERPT_TTL_MS` after their event time, `EXCERPT_TTL_MS` derived per D-85; turning `no_excerpts` on clears stored excerpts at the next pass; daily copies, quarantine and pre-migration copies kept two and for 2 days, by the date in their name; every purge, and every hourly pass, runs `wal_checkpoint(TRUNCATE)` (AD-39, D-91). Proof: core `crates/officina-core/src/registry/retention.rs::tests::{ttl_copy_lifetime_and_pass_interval_stay_within_30_days, event_file_is_deletable_only_when_every_kept_copy_holds_it_complete}`; host `crates/officina-host/tests/registry_retention.rs::{four_utc_days_are_ingested_before_any_deletion, deletion_waits_for_both_copies, a_damaged_copy_does_not_count, unavailable_registry_deletes_nothing, read_only_registry_deletes_nothing, excerpts_expire_from_their_event_time, no_excerpts_turned_on_clears_stored_excerpts, expired_text_leaves_the_wal, hourly_pass_truncates_the_wal, copies_quarantine_and_pre_migration_copies_kept_two_for_2_days}`, every function called with an explicit `now_ms`.
7. Collector (AD-43 "When", AD-44, AD-3, AD-4, D-44, D-78, D-84, D-85): at start the collector opens the writer (story 3.1), ingests every incomplete file, runs the hourly pass (copy when due, retention, purge), and only then builds and emits the first board; each tick ingests before it applies the agents snapshot and before the recompute; the hourly pass ingests before it retains; v0.2's journal and approval cleanups run in the same pass, ungated. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{start_ingests_and_purges_before_the_first_board, each_tick_ingests_before_the_agents_snapshot, today_growth_is_ingested_before_the_recompute}`; host `registry_retention.rs::hourly_pass_ingests_then_copies_then_retains`.
8. Registry unavailable, read-only or corrupted while running (FR-1, AD-41, AD-42 last sentence, D-47, D-91, P-13): the board keeps working from live sources, nothing is deleted, offsets do not move, and `Diagnostics.registry` says why; corruption detected while the window runs makes the registry unavailable until the next start; the Board view shows a banner; `BOARD_SCHEMA` goes up by one. Proof: `watch.rs::{unavailable_registry_keeps_the_board_and_deletes_nothing, corruption_while_running_makes_the_registry_unavailable_until_restart}` (the second overwrites database pages while the collector runs, which Windows' sharing mode allows); core `crates/officina-core/src/board.rs::tests::registry_state_reaches_the_diagnostics`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; Playwright `app/e2e/board.spec.ts > an unavailable registry is reported and the board still shows sessions` on a `registry-unavailable` fixture whose `registry.db` holds bytes that are not a database, written by `app/scripts/e2e-fixtures.mjs`.
9. The window calls the writer, so the registry marker's positive control joins the CI step "officina holds no authority": the step fails when the marker is missing from `officina-app.exe` (D-83). Proof: that CI step.
10. No CLI deletes anything (FR-2, AD-44, AD-53): `officina board --json` runs no retention and reads the registry read-only to report its state, with the window closed too (D-82); no ungated deletion of event files is left in the code. Proof: `crates/officina/tests/board.rs::{board_json_deletes_no_event_file, board_json_reports_the_registry_state_with_the_window_closed}`; the v0.1 test `crates/officina-host/src/files.rs::tests::retention_removes_only_expired_event_files` is replaced by the gated tests of AC 6.
11. Crash, ingestion part (AD-58 crash row, PRD section 7 Memory, D-89): the crash helper gains mode `ingest`, with a stall inside a chunk's transaction at a chosen line; the test kills it there and at 20 random points; afterwards `quick_check` passes, the registry equals one built by an uninterrupted ingestion of the same files (sessions, facts, excerpts, offsets, corrupt counts), and every offset matches the rows it covers. Proof: `#[ignore]` test `crates/officina-host/tests/registry_crash.rs::killed_mid_transaction_and_at_random_loses_no_event`, run by a new CI step `cargo test --release -p officina-host --features window-authority --test registry_crash -- --ignored`.
12. Fake claude per test (D-86): `officina-fake-claude` reads its configuration from `FAKE_CLAUDE_*` variables or, when absent, from `fake-claude.json` next to its own executable; `crates/officina/tests/common/mod.rs` gains a helper that copies the fake into a temp dir with a sidecar; in-process collector tests use it, and no test calls `std::env::set_var`. Proof: `crates/officina/tests/fake_claude.rs::{sidecar_configures_a_copied_fake, environment_wins_over_the_sidecar}`; the collector tests of this story run against a copied fake with an agents fixture.
13. Doctor warns when more than 7 event files wait for ingestion or deletion, and reports rewritten event files (AD-44, R21, D-84). Proof: `crates/officina-host/src/doctor.rs::tests::{more_than_seven_waiting_event_files_warn, rewritten_event_files_are_reported}`.
14. Zero write: the collector run of the fingerprint test now ingests (D-80). Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
15. Documentation (D-73, D-85): `CHANGELOG.md` `[Unreleased]` (Changed: event files are deleted only once ingested and copied; `board --json` deletes nothing) and the README's privacy section and "Subjects and triage" section: the answer's tail, what is kept and for how long, that bounds are applied while the window runs and at its start, and that with the window closed or the registry unavailable nothing expires and event files are kept. Proof: review; `readme.rs` stays green when it exists.

## Tasks

- [ ] `events.rs`: `last_message_tail`, bounded line order (AC 1)
- [ ] `crates/officina-host/src/registry/ingest.rs`: passes over every incomplete file, offsets, grace, rewrite detection, chunks, session creation function, excerpts, exclusion, `no_excerpts` (AC 2-5)
- [ ] `crates/officina-core/src/registry/retention.rs` and the host hourly pass: ingestion first, copies, event files per D-48, excerpts, `no_excerpts` purge, unconditional checkpoint, copy lifetimes (AC 6, 7)
- [ ] `watch.rs` and `collector.rs`: writer open, start sequence, tick order, one hourly pass, runtime corruption (AC 7, 8)
- [ ] `Diagnostics.registry`, `BOARD_SCHEMA` bump, regenerate TypeScript, Board banner (AC 8)
- [ ] CI step: positive control of the registry marker (AC 9)
- [ ] `main.rs`: remove the retention call from `board --json`; read-only registry state (AC 10)
- [ ] Crash helper mode `ingest` with its stall point, `registry_crash.rs`, CI release step (AC 11)
- [ ] Fake claude sidecar, harness helper, `fake_claude.rs` (AC 12)
- [ ] Doctor waiting and rewritten files (AC 13)
- [ ] Fixture `registry-unavailable` in `app/scripts/e2e-fixtures.mjs`; Playwright case (AC 8)
- [ ] Update `CHANGELOG.md` and the README (AC 15)

## Dev Notes

- Requirements: FR-1 (unavailable registry reported on the board), FR-2 as amended (amendment 2), FR-3 first part, FR-4 as amended (amendment 4). Decisions: AD-3, AD-4, AD-39, AD-41, AD-42, AD-43, AD-44, AD-45 (first bullet; `absent` is story 3.8), AD-53 (`board --json` part), spine amendments to AD-8 and AD-9. Epic decisions: P-9, P-13, P-17, D-44, D-47 to D-50, D-53, D-78 to D-80, D-82 to D-86, D-89, D-91, D-94, D-97.
- v0.2 seams (D-44): only `events::{EventLine, reduce_payload}` change in the hook, never the dispatch of AD-26. The hourly pass becomes one host function; v0.2 stories 2.8 and 2.9 add the approval and journal cleanups to it, ungated, in whichever order the branches merge. `files::retention_pass` disappears with its last caller.
- Tick order (D-78): today `watch.rs::run` refreshes agents before the recompute; ingesting first is what keeps a `SessionEnd` and the vanishing of its session, seen in the same tick, from reading as `interrupted` (story 3.3 proves the outcome).
- Every pass covers every incomplete file (D-84): a window running across midnight completes yesterday's file after the grace, so it can be deleted once both copies hold it.
- A rewritten file (shorter than its offset) is re-read from 0: folding is idempotent (AD-43), so only the corrupt count restarts.
- The derived TTL (D-85) keeps "none survives 30 days" true with an hourly pass: an excerpt can sit in a copy for the copy lifetime plus one interval after its expiry.
- `BOARD_SCHEMA` goes up by one relative to `main` at merge time (P-13): stories 3.4 and 3.8 extend `Board` under the same number. Every board fixture regenerates.
- The board still reads today's and yesterday's event files itself (AD-9); ingestion does not change classification, so the classification vectors are untouched.
- The tail goes first when a line is too long because the head is what the board shows; the head, prompt and message follow the v0.1 rule.
- The fake's sidecar (D-86) exists because edition 2024 makes `std::env::set_var` unsafe and the workspace denies `unsafe_code`; `crates/officina/tests/watch.rs` notes the gap today.
- The crash test seeds its random kill points and prints the seed so a failure can be replayed; the release build keeps it fast.

## Testing

- Core unit: `crates/officina-core/src/{events,board,registry/retention}.rs`.
- Host integration: `crates/officina-host/tests/{registry_ingest,registry_retention,registry_crash}.rs`.
- CLI integration: `crates/officina/tests/{hook,board,watch,fake_claude,quota_doctor}.rs`.
- Bindings: `crates/officina-core/tests/typescript_bindings.rs`.
- Playwright: `app/e2e/board.spec.ts`.
- Ignored release tests: `perf.rs`, `registry_crash.rs`, CI `windows` job.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: every pass covers every incomplete file, rewritten files re-ingested and reported (D-84); tick order ingests first (D-78); single session creation function (D-79); purge at start, `no_excerpts` clears stored excerpts, derived TTL (D-85); hourly checkpoint and runtime corruption ACs (D-91); registry marker positive control (D-83); crash test that can fail (D-89); fake claude sidecar (D-86); normalised workdir exclusion (D-97); readers with the window closed (D-82); fixture script path
