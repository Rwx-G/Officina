# Story 3.1: Registry store: schema 1, migrations, integrity, copies and location

**Status:** review

## Story

As the author whose sessions, subjects and verdicts must outlive the board's 24 hours,
I want one local SQLite registry that only the window writes, that checks itself at open, keeps daily copies and refuses a network folder,
so that nothing I decide is lost to a restart, a crash or a sync client, and every later Epic 3 story stores into one settled schema.

## Acceptance Criteria

1. `rusqlite` and the `windows-sys` feature of addendum section 3 are added to `officina-host` only, with the version and features given there; `cargo deny check` passes; a CI step fails when `cargo tree -p officina-host -e normal` lists a crate the addendum rules out (D-91). Proof: CI `audit` job and the new CI `windows` step "registry dependencies".
2. `closed_enum!` (exported with `#[macro_export]`) and `sha256_hex` live at the core crate root, re-exported from `relay` (D-42). Proof: the unchanged tests of `crates/officina-core/src/relay/` and the `sha256_hex` doc test at its new path.
3. `officina-core::registry::types` declares the enums of D-45 with `closed_enum!`, a `snake_case` text form, the records and `Sourced<T>` (AD-38, AD-40). Proof: `crates/officina-core/src/registry/types.rs::tests::{every_variant_round_trips_its_text, unknown_text_is_refused}`, each iterating every enum's `ALL`.
4. Schema version 1 is AD-40 with the additions of D-88: STRICT tables, keys and columns, `session_fact` WITHOUT ROWID, the indexes, and the `CHECK` constraints D-88 lists, on exactly those columns (P-14). Proof: `crates/officina-host/tests/registry_schema.rs::{every_table_is_strict, check_lists_match_the_core_enums, columns_without_check_are_the_ones_d_88_names}`; the second reads `sqlite_master` and compares each list with its enum's `ALL`, so a new variant fails until a migration widens the check.
5. Migrations (AD-41): entry `i` of `schema::MIGRATIONS` moves version `i` to `i + 1` in one transaction that also sets `user_version`; each shipped entry's SHA-256 is pinned; before migrating a database that holds data, the writer runs `VACUUM INTO registry/backup/pre-v<n>.db`; every past version has a fixture under `tests/fixtures/registry/`. Proof: `registry_schema.rs::{migration_hashes_are_pinned, failed_migration_leaves_the_version_unchanged, migrating_a_database_with_data_copies_it_first, every_past_version_has_a_fixture}` (the runner takes the migration list as an argument, so the second and third use a two-entry list; the fourth holds trivially at version 1 and fails as soon as a second migration lands without the fixture of the version it leaves).
6. Location (AD-39): the registry is `registry/registry.db` under the Officina home; the host classifies its folder as network, synced or local from injected probes; a network registry is never opened; a synced one opens and reports the folder to exclude; `SYNC_ROOT_MARKERS` is the only list of markers. Proof: host unit tests `crates/officina-host/src/registry/store.rs::tests::{unc_path_is_network, remote_drive_type_is_network, each_sync_marker_in_an_ancestor_is_synced, onedrive_environment_path_is_synced, cloud_files_attribute_is_synced, local_folder_is_local}` (the third iterates the constant), the `#[cfg(windows)]` test `store.rs::tests::temp_dir_drive_is_not_remote` through `GetDriveTypeW`, and integration tests `crates/officina-host/tests/registry_store.rs::{planted_marker_opens_and_names_the_folder, network_location_is_not_opened}`.
7. Connections (AD-38, AD-39): the writer sets every pragma of AD-39 at each open; a reader opens with `SQLITE_OPEN_READ_ONLY`, `query_only = ON` and `busy_timeout = 2000`, reads beside the writer and refuses writes; the reader type has no writing method. Proof: `registry_store.rs::{writer_sets_every_pragma, reader_is_read_only_and_query_only, reader_reads_beside_the_writer}` and a `compile_fail` doc test on the reader type calling a writer method.
8. Readers with the window closed (D-82): after a clean close and after the writer's process was killed, a reader reads every row, changes none, never takes the write lock and never blocks a writer opening meanwhile; it may create `-wal` and `-shm`. Proof: `registry_store.rs::{reader_reads_a_cleanly_closed_registry, reader_reads_after_the_writer_was_killed, reader_leaves_the_database_file_unchanged, reader_never_blocks_the_writer}`; the killed writer is `crates/officina-host/examples/registry_crash_helper.rs` in its `hold` mode (created here, extended by stories 3.2 and 3.7).
9. Version rule (AD-41 table, D-47): version 0 is created at version 1 by the writer only, never by a reader; an older version is copied, then migrated; an equal version reads and writes; a newer version opens read-only for the writer (no ingestion, no deletion); a reader reads only an equal version and otherwise reports which state it found. Proof: `registry_store.rs::{absent_registry_is_created_by_the_writer_only, older_version_is_copied_then_migrated, newer_version_opens_read_only_for_the_writer, reader_reports_each_state}`; the last iterates the reader-state enum's `ALL` through a scenario builder whose `match` is exhaustive.
10. Integrity at open (AD-42, D-48, D-93, D-97): the writer runs `quick_check` at open; on `SQLITE_CORRUPT`, `SQLITE_NOTADB` or a failed check, the files move to `registry/quarantine/<now_ms>/`, the newest copy passing `quick_check` is restored and migrated, and the restore notice (copy name and time) is written to `setting`; with no valid copy a new database starts and the outcome says so; a quarantine move that fails leaves the registry unavailable until the next start, nothing deleted. Proof: `registry_store.rs::{damaged_page_is_quarantined_and_the_newest_valid_copy_restored, damaged_newest_copy_falls_back_to_the_next, not_a_database_is_quarantined, no_valid_copy_starts_empty_and_says_so, restore_writes_the_notice, held_file_leaves_the_registry_unavailable}` (the last holds the database open with a share mode that refuses the move).
11. Copies (AD-42, D-94): `copy_if_due(now_ms)` writes `registry/backup/registry-<UTC date>.db` through a temporary name renamed when complete, when the newest daily copy's date is over a day old; the two newest valid copies are kept; a text deleted under `secure_delete` and checkpointed is found neither in the database, its WAL nor the copy. Proof: `registry_store.rs::{daily_copy_is_due_by_the_date_in_its_name, interrupted_copy_leaves_no_dated_file, a_damaged_copy_never_pushes_out_a_valid_one, deleted_text_reaches_neither_the_file_nor_the_copy}` (byte search for a marker string), all at explicit `now_ms`. The hourly scheduling is story 3.2.
12. Only the window writes (AD-38, P-5, D-43, D-83): `registry::{write, ingest}` and the writing half of `store` compile under `window-authority`; the writer's error type carries the registry marker; the CI step "officina holds no authority" gains a second extraction and fails when the registry marker is in the fresh `officina.exe` (the positive control on `officina-app.exe` comes with story 3.2). Proof: that CI step; `cargo build -p officina` alone compiles without the feature.
13. Doctor (FR-1 as amended, amendment 1; AD-53 doctor line, its store part; D-91): location (synced warns and names the folder to exclude, network fails), schema version (newer warns, older says the window will migrate it), `quick_check`, daily copies (count and age of the newest), quarantine (present warns), restore notice. Doctor opens read-only and never creates the registry. Proof: host unit tests `crates/officina-host/src/doctor.rs::tests::{registry_in_a_synced_folder_warns_and_names_it, newer_registry_warns, failed_quick_check_fails, copies_quarantine_and_restore_are_reported}`; CLI tests `crates/officina/tests/doctor_registry.rs::{unc_officina_home_fails_the_registry_location, absent_registry_is_reported_not_created, doctor_leaves_the_database_file_unchanged}` (the first sets `OFFICINA_HOME` to a UNC path; the last fingerprints `registry/`, ignoring `-wal` and `-shm` per D-82).
14. Documentation (D-73): `CHANGELOG.md` `[Unreleased]` gains the registry entry, and the README gains a section "Subjects and triage (v0.3, in progress)" saying where the registry lives and that `registry/` must be excluded from any sync client. Proof: review; `crates/officina/tests/readme.rs` stays green when it exists.

## Tasks

- [x] Add `rusqlite` and the `windows-sys` feature to `crates/officina-host/Cargo.toml`; `cargo deny check`; CI step "registry dependencies" (AC 1)
- [x] Hoist `closed_enum!` (with `#[macro_export]`) and `sha256_hex` to the core crate root with re-exports (AC 2)
- [x] `crates/officina-core/src/registry/{mod,types}.rs`: enums of D-45, records, `Sourced<T>` (AC 3)
- [x] `crates/officina-host/src/registry/{mod,schema,store,read}.rs`: migration 1 with D-88, runner, pinned hashes, pragmas, reader type, version states, location probes, integrity, quarantine, restore notice, copies (AC 4-11)
- [x] `GetDriveTypeW` in `crates/officina-host/src/platform/windows.rs` with a `// SAFETY:` comment (AC 6)
- [x] `window-authority` gating of `write`, `ingest` and the writing half of `store`; the registry marker; second extraction and negative check in the CI step (AC 12)
- [x] `examples/registry_crash_helper.rs` with mode `hold`, `required-features = ["window-authority"]` (AC 8)
- [x] Doctor registry checks (AC 13)
- [x] Tests: `registry_schema.rs`, `registry_store.rs`, `doctor_registry.rs`, unit tests (AC 3-13)
- [x] Update `CHANGELOG.md` `[Unreleased]` and the README (AC 14)

## Dev Notes

- Requirements: FR-1 as amended (addendum section 4, amendment 1). Decisions: AD-38, AD-39, AD-40, AD-41, AD-42, addendum section 3 (dependencies). Facts: the addendum's verified list (bundled SQLite build, `user_version` rolling back with its transaction, a reader beside a WAL writer, `secure_delete` with a checkpoint, `quick_check` on a damaged page, the Synology marker). Epic decisions: P-5, D-9 (Epic 2); P-14, P-17, D-42, D-43, D-45, D-47, D-48, D-82, D-83, D-88, D-91, D-93, D-94, D-97 (`docs/stories/epic-3-decisions.md`).
- v0.2 stories 2.1 and 2.2 have merged: `closed_enum!`, `sha256_hex`, `window-authority` and the CI step exist; this story moves the first two and extends the step.
- Schema 1 is complete here, D-88's additions and every enum its `CHECK` constraints name included (P-14), so later stories store into it without a migration. `session_fact.fact` has no `CHECK` on purpose (AD-40: a new fact needs no migration).
- D-82 replaces the earlier gate: a reader with the window closed may create `-wal` and `-shm`. What matters is that it changes no row and never blocks the writer; the doctor fingerprint compares the database file and ignores those two names.
- The marker's positive control waits for story 3.2 (D-83): under `lto` and `strip`, a marker survives only in a binary that calls the code carrying it (D-9), and the window first opens the writer in 3.2.
- `user_version` and DDL roll back together (verified), which is what makes one transaction per migration safe.
- `VACUUM INTO` writes no free pages (AD-42), but the WAL can still hold an old page: copy and purge paths run `wal_checkpoint(TRUNCATE)` first (AD-39).
- `GetDriveTypeW` takes the root of the path (`C:\`); a UNC path is network without any call, which is what lets the doctor CLI test use one. Use a UNC path on `localhost` with a share that does not exist, so nothing waits on a remote host; doctor's other checks may fail on it, the test asserts the registry line only. `MetadataExt::file_attributes` is std, no unsafe. The host reads the OneDrive environment variables and passes them to the classification as arguments, so tests set nothing process-wide.
- Copy dates come from the file name (D-94), so every copy test runs at an explicit `now_ms`; modification time only orders restore candidates.
- Integration tests that write declare `required-features = ["window-authority"]`, like `authority.rs` of story 2.2. `cargo test` builds examples; the killed-writer test finds the helper next to its own executable, under `../examples/`.
- A damaged page for AC 10 is made by overwriting bytes inside a page past the header, which the addendum verified `quick_check` reports.
- No employer content: temp homes, placeholder paths only (officina-project rule).

## Testing

- Core unit: `crates/officina-core/src/registry/types.rs`.
- Host unit: `crates/officina-host/src/{registry/store,doctor}.rs`.
- Host integration: `crates/officina-host/tests/{registry_schema,registry_store}.rs`.
- CLI integration: `crates/officina/tests/doctor_registry.rs`.
- CI: `audit` job, "registry dependencies" step, "officina holds no authority" step.

## Dev Agent Record

### Debug Log

- **Probed before designing** (rusqlite 0.41.0 bundled, on the workstation): a `VACUUM INTO` copy is written in rollback mode (header bytes 18 and 19 at 1), so a read-only open of a copy creates no file beside it; a read-only open of a cleanly closed WAL registry creates `-wal` and `-shm` and leaves them (D-82); on-disk damage under a running connection stays invisible while its page cache holds the pages, and shows as `SQLITE_CORRUPT` once another connection commits (story 3.2's runtime test relies on this).
- **Decisions where the story is silent:**
  - AC 2: the macro also generates `from_code()`, the inverse of `code()`, which the registry needs to read its text columns; `wire` is `#[doc(hidden)] pub` so the exported macro expands in other crates.
  - AC 3: `ReasonCode` is `state_done`, `state_failed`, `state_stopped`, `cut_mid_turn`, `default_to_resume`, `empty_subject`, `model`; `SettingKey` is `restore_notice`, `triage_model`, `triage_model_pause`, `auto_archive`; `TriagePurpose` `proposal`, `dry_run`; `TriageOutcome` `answer`, `refused`, `failed`, `timeout`, `invalid`. Stories 3.6 to 3.11 may change them under P-14. Records: `Sourced<T>`, `EventFileRecord`, `RestoreNotice`.
  - AC 4: schema 1 adds `event_file.rewrites` (D-84, story 3.2) so migration 1 is written once; `call_id` columns carry no foreign key, since `triage_call` rows expire after 90 days (AD-44) and a confirmation keeps its call id as history.
  - AC 5: the pre-migration copy is `backup/pre-v<target version>-<UTC date>.db`: the date lets story 3.2's retention read its age from the name (D-94). The runner (`schema::migrate`), `open_writer_with` and `open_reader_expecting` take the migration list or the expected version as an argument, so the older-version cases are tested with two entries.
  - AC 6: the Cloud Files attributes checked are `PINNED`, `UNPINNED`, `RECALL_ON_OPEN` and `RECALL_ON_DATA_ACCESS`, on the folder and each ancestor; `onedrive_roots_from_env()` reads `OneDrive`, `OneDriveCommercial` and `OneDriveConsumer`; `DRIVE_REMOTE` is declared next to `GetDriveTypeW`, since windows-sys puts it under a feature the addendum does not list.
  - AC 7: `Writer::pragma` and `Reader::pragma` read a setting back by name; `read_only_connection` is the one place readers open, and the test proves it refuses writes.
  - AC 9: the reader-state enum is the host's `ReaderState` (with `ready`), declared with the exported macro (D-42); the window's newer-version outcome is `WriterOpen::ReadOnly` holding a `Reader`, so nothing can ingest or delete by construction.
  - AC 10: restore candidates are every `.db` under `backup/` (daily and pre-migration copies), newest modification first; a copy is placed through `registry.db.restore`, renamed once complete. A quarantine that fails removes the folder it created when empty, so doctor reports no empty quarantine.
  - AC 11: pruning validates each daily copy (`quick_check`, read-only) and deletes every daily copy beyond the two newest valid ones, damaged ones included.
  - AC 13: doctor levels: absent ok, older and newer warn, unreadable fail; no copy warns; a quarantine warns; a restore warns for the 7 days the view shows it (D-93), then is reported ok.
- **Adversarial pass** (concurrent readers during writes, a newer version, a corrupt file, a synced home, a kill mid-transaction): two real misses, each now a test and fixed in its own commit. A kill between the quarantine's moves left `registry.db-wal` without its database, which SQLite would replay into a new database (`a_log_left_without_its_database_is_set_aside`); a kill after the quarantine and before the copy took its place made the next start begin empty (`an_interrupted_restore_is_finished_at_the_next_open`). With no database file, the window now sets aside any `-wal` and `-shm`, and finishes the restore when a quarantine exists; a first start restores nothing (`a_first_start_restores_nothing`). A kill inside a migration needs no helper: its transaction never commits, which `failed_migration_leaves_the_version_unchanged` proves for a failure.
- **Mutation check:** `deleted_text_reaches_neither_the_file_nor_the_copy` stayed green with `secure_delete` off in its first form (the replaced cell was the page's only one, which SQLite reuses); rewritten with rows after the marker and a longer replacement, it fails with `secure_delete` off and passes with it on.
- **Per-commit gates:** fmt; clippy on core (`ts`) and on the host with and without `window-authority`; the tests of the crates touched. The full CI replay runs at the end of story 3.2.

### Completion Notes

- AC 1: `crates/officina-host/Cargo.toml` (`rusqlite` 0.41.0 `bundled`, `cache`; windows-sys `Win32_Storage_FileSystem`); `cargo deny check` ok; CI `windows` step "Registry dependencies" (rusqlite as positive control).
- AC 2: `crates/officina-core/src/lib.rs` (`#[macro_export] closed_enum!`, `sha256_hex`), re-exported by `relay`; the relay tests unchanged and green; doc tests `lib.rs - sha256_hex` and `lib.rs - closed_enum`.
- AC 3: `crates/officina-core/src/registry/types.rs::tests::{every_variant_round_trips_its_text, unknown_text_is_refused}`, iterating every enum of the file (a count of the file's `closed_enum!` blocks fails on an unlisted one).
- AC 4: `crates/officina-host/tests/registry_schema.rs::{every_table_is_strict, check_lists_match_the_core_enums, columns_without_check_are_the_ones_d_88_names}`.
- AC 5: `registry_schema.rs::{migration_hashes_are_pinned, failed_migration_leaves_the_version_unchanged, migrating_a_database_with_data_copies_it_first, every_past_version_has_a_fixture}`.
- AC 6: `crates/officina-host/src/registry/store.rs::tests::{unc_path_is_network, remote_drive_type_is_network, each_sync_marker_in_an_ancestor_is_synced, onedrive_environment_path_is_synced, cloud_files_attribute_is_synced, local_folder_is_local, temp_dir_drive_is_not_remote}`; `crates/officina-host/tests/registry_store.rs::{planted_marker_opens_and_names_the_folder, network_location_is_not_opened}`.
- AC 7: `registry_store.rs::{writer_sets_every_pragma, reader_is_read_only_and_query_only, reader_reads_beside_the_writer}`; the `compile_fail` doc test on `registry::read::Reader`, with its compiling twin.
- AC 8: `registry_store.rs::{reader_reads_a_cleanly_closed_registry, reader_reads_after_the_writer_was_killed, reader_leaves_the_database_file_unchanged, reader_never_blocks_the_writer}`; `crates/officina-host/examples/registry_crash_helper.rs` mode `hold`.
- AC 9: `registry_store.rs::{absent_registry_is_created_by_the_writer_only, older_version_is_copied_then_migrated, newer_version_opens_read_only_for_the_writer, reader_reports_each_state}`.
- AC 10: `registry_store.rs::{damaged_page_is_quarantined_and_the_newest_valid_copy_restored, damaged_newest_copy_falls_back_to_the_next, not_a_database_is_quarantined, no_valid_copy_starts_empty_and_says_so, restore_writes_the_notice, held_file_leaves_the_registry_unavailable}`.
- AC 11: `registry_store.rs::{daily_copy_is_due_by_the_date_in_its_name, interrupted_copy_leaves_no_dated_file, a_damaged_copy_never_pushes_out_a_valid_one, deleted_text_reaches_neither_the_file_nor_the_copy}`.
- AC 12: `registry::write` and the writing half of `store` under `window-authority`; `REGISTRY_MARKER` in `RegistryError`; the CI step "officina holds no authority" extracts it and fails when `officina.exe` holds it; `cargo build -p officina` alone compiles, its feature graph holds neither feature, and its exe holds no marker.
- AC 13: `crates/officina-host/src/doctor.rs::tests::{registry_in_a_synced_folder_warns_and_names_it, newer_registry_warns, failed_quick_check_fails, copies_quarantine_and_restore_are_reported}`; `crates/officina/tests/doctor_registry.rs::{unc_officina_home_fails_the_registry_location, absent_registry_is_reported_not_created, doctor_leaves_the_database_file_unchanged}`.
- AC 14: `CHANGELOG.md` `[Unreleased]` Added; README "Subjects and triage (v0.3, in progress)"; `readme.rs` green.

## File List

- `crates/officina-core/src/{lib,wire,dates}.rs`, `crates/officina-core/src/relay/mod.rs`, `crates/officina-core/src/registry/{mod,types,retention}.rs`
- `crates/officina-host/Cargo.toml`, `Cargo.lock`, `crates/officina-host/src/{lib,doctor}.rs`, `crates/officina-host/src/platform/windows.rs`, `crates/officina-host/src/registry/{mod,schema,store,read,write}.rs`, `crates/officina-host/examples/registry_crash_helper.rs`, `crates/officina-host/tests/common/{mod,registry}.rs`, `crates/officina-host/tests/{registry_schema,registry_store}.rs`
- `crates/officina/tests/doctor_registry.rs`
- `.github/workflows/ci.yml`, `README.md`, `CHANGELOG.md`, `docs/stories/story-3.1-registry-store.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: D-82 replaces the reader gate (readers may create `-wal`/`-shm`, tested with the window closed); schema 1 additions and the listed `CHECK` columns (D-88); marker negative check only, positive control moved to 3.2 (D-83); copies by UTC date, temp-then-rename, two newest valid kept (D-94); restore notice (D-93); quarantine move failure (D-97); ruled-out crates step and UNC doctor test automated (D-91); `#[macro_export]`; `compile_fail` proof for the reader type
- 2026-10-28 implemented: the registry store (schema 1, migrations, location, readers, the window's writer with its integrity check, quarantine, restore and daily copies), doctor's registry lines, the CI checks, the README section; two adversarial fixes (a log left without its database, an interrupted restore). Status review
- 2026-10-28 story 3.2 moved the reader-state enum to core as `RegistryState` (the board's diagnostics need it); `reader_reports_each_state` iterates it
