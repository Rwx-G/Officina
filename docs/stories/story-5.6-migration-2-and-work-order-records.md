# Story 5.6: Registry migration 2, work order records, binding and filing

**Status:** Review

## Story

As the author whose launched work must land in the right subject and stay on record,
I want the registry to gain, in one migration, every table v0.5 needs, and work orders to be recorded, bound to the session they started by its unique name, and filed under the subject I chose,
so that a launch is never lost or misfiled, and my registry never needs a second v0.5 migration because I used a Slice A build.

## Acceptance Criteria

1. **Migration 2, whole** (AD-82, D-133). `schema::MIGRATIONS[1]` creates, as STRICT tables, `work_order`, `proposal`, `orchestrator_start` and `auto_review` with AD-82's columns, and rebuilds `setting` (create, copy, drop, rename, in the migration's transaction) so its key `CHECK` admits `auto_approval`, `auto_mode` and `briefing_read_at_ms`; the runner of story 3.1 runs `VACUUM INTO registry/backup/pre-v2.db` first; its SHA-256 is pinned. Proof: host `crates/officina-host/tests/registry_schema.rs::{every_table_is_strict, migration_hashes_are_pinned, every_past_version_has_a_fixture, migrating_a_database_with_data_copies_it_first, the_setting_rebuild_keeps_every_row, version_1_with_data_migrates_to_2}` (a new fixture `tests/fixtures/registry/v1.sql` holds version 1 with rows).
2. **Enums and `CHECK` lists** (AD-82, D-133). `registry::types` declares with `closed_enum!`: `WorkOrderSource` (`composer`, `proposal`), `LaunchOutcome` (`launching`, `started`, `untrusted_workspace`, `failed`, `unknown`), `ProposalKind` (`recap`, `work_order`, `answer`; the last is used from story 5.21, declared here so migration 2's `CHECK` list holds it, D-575), `ProposalStatus` (`pending`, `invalid`, `accepted`, `dismissed`, `expired`, `superseded`), `OrchestratorStartOutcome` (`spawned`, `refused`, `failed`), `ReviewMode` (`shadow`, `live`, `rule`), `ReviewOutcome` (`no_call`, `answered`, `invalid`, `failed`, `timeout`), `AuthorDecision` (`approved`, `rejected`), `AuthorReason` (`should_not_run`, `not_now`); `SettingKey` gains the three keys; `relay::auto` is created with `ReviewDecision` (`approve`, `reject`, `escalate`) alone. `CHECK` lists sit on exactly the columns D-133 names, read from the enums' `ALL`. Proof: `registry_schema.rs::{check_lists_match_the_core_enums, columns_without_check_are_the_ones_d_88_names}` (the second extended with D-133's columns); core `crates/officina-core/src/registry/types.rs::tests::{every_variant_round_trips_its_text, unknown_text_is_refused}` iterating the new enums' `ALL`.
3. **Shape** (D-133; review F2, F3). No foreign key points at `session` or `subject`. Per table:
   - `work_order`: key `id` (text, the card id); `NOT NULL` `seq`, `template`, `project`, `cwd` (the project folder at launch), `values_json`, `argv_json`, `prompt_sha256`, `catalogue_hash`, `name`, `source`, `launched_at_ms`, `outcome`; the other columns nullable; `UNIQUE(name)`, `UNIQUE(template, seq)`; index on `launched_at_ms`;
   - `proposal`: key `id` (text, the file id); `NOT NULL` `status`, `received_at_ms`; `kind` nullable, for a file that does not parse; the other columns nullable; index on `(status, received_at_ms)`;
   - `orchestrator_start`: key `id` (integer); `NOT NULL` `started_at_ms`, `outcome`; `pid`, `pid_created_at_ms` and `error` nullable;
   - `auto_review`: key `id` (integer); `NOT NULL` `approval_id`, `request_key`, `project`, `action`, `requested_at_ms`, `mode`, `outcome`, `rule_baseline`; the other columns nullable; indexes on `(project, action, requested_at_ms)` and `requested_at_ms`.

   Proof: `registry_schema.rs::{work_order_names_are_unique, work_order_template_and_seq_are_unique, a_proposal_without_kind_is_accepted, orchestrator_start_keeps_the_pid_creation_time, not_null_columns_are_the_ones_the_story_names, no_foreign_key_points_at_session_or_subject, the_counting_indexes_exist}`.
4. **Recording a launch** (AD-81 steps 1 and 3, D-135, D-136; review F2, F14). Methods under `window-authority`: `highest_work_order_seq(template)`, which the card store reads when it reserves a card's `seq` at issue (story 5.7); `insert_work_order(order, card_id, seq, source, subject choice, proposal_id, now_ms)` with outcome `launching`, the card's `seq`, the order's project folder as `cwd`, the argument vector and values as JSON, the prompt SHA-256 and catalogue hash, `name` = `<template>-<seq>`, a violation of either `UNIQUE` constraint returning `name_taken`; `record_launch_outcome(id, outcome, exit_code, error, printed, now_ms)` storing `printed` (at most 4 KiB) and `error` (at most 500 characters) as untrusted text; `expire_launching(now_ms)` turning `launching` rows older than 2 minutes into `unknown`, called by the start sequence before the first view and on the collector's own tick. Proof: host `crates/officina-host/tests/registry_orders.rs::{highest_seq_reads_the_template_rows, a_taken_name_is_refused_as_name_taken, a_launch_is_recorded_launching_then_its_outcome, printed_and_error_are_bounded, stale_launching_rows_become_unknown_at_start}`, at explicit `now_ms`; CLI-crate collector test `crates/officina/tests/watch.rs::a_stale_launching_row_becomes_unknown_on_the_tick`.
5. **Binding** (AD-81 step 4, D-133). On each agents refresh, a `started` order younger than 10 minutes binds to the first background entry whose `name` equals the order's name, whose `cwd` lies in the order's stored `cwd` (normalised as `quota::normalise_path`), so a floor edited since the launch moves nothing, and whose `startedAt` is at most 5 s before `launched_at_ms`; binding sets `bg_id`, `session_id` and `bound_at_ms`; an order is bound once. Proof: core `crates/officina-core/src/orders/work_order.rs::tests::{binds_by_name_folder_and_start_time, a_session_started_too_early_does_not_bind, an_entry_outside_the_folder_does_not_bind, an_order_older_than_10_minutes_does_not_bind}`; CLI-crate collector test `crates/officina/tests/watch.rs::a_started_order_binds_to_its_background_session` (fake claude `bg` mode, D-129).
6. **Filing** (AD-81 step 5, D-79, D-158). The session creation function of D-79 consults bound orders first and files the session under the order's subject with source `author`; a `New` subject is created when the outcome is `started`, never at the click, with D-57's rules and the order's project (a non-archived subject of that name in the project, ignoring case, is used instead, D-158); when the session was created first and filed by rule elsewhere, binding re-files it (`author` beats `rules`). Proof: core `crates/officina-core/src/registry/filing.rs::tests::{a_bound_order_files_its_session_first, author_filing_beats_rule_filing}`; host `registry_orders.rs::{a_new_subject_is_created_at_started_not_at_the_click, a_new_name_already_taken_files_into_that_subject, a_session_seen_before_binding_is_refiled}`.
7. **Reading back** (AD-82, D-130). `registry::read` returns the last 50 work orders with their binding and the bound session's record; `printed` and `error` come back as `Untrusted<String>`. Proof: host `registry_orders.rs::work_orders_read_back_newest_first_with_untrusted_texts`.
8. **Retention** (AD-82, D-134). The hourly pass deletes `work_order` rows 90 days after `launched_at_ms` and clears `printed` and `error` `EXCERPT_TTL_MS` after it. Proof: host `crates/officina-host/tests/registry_retention.rs::{work_order_rows_are_deleted_at_90_days, work_order_texts_are_cleared_with_the_excerpt_ttl}`; core `crates/officina-core/src/registry/retention.rs::tests::ttl_copy_lifetime_and_pass_interval_stay_within_30_days` unchanged.
9. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Changed: registry schema version 2 (copied to `registry/backup/pre-v2.db` before migrating). Proof: review.

## Tasks

- [x] Enums in `registry::types`; `relay/auto.rs` with `ReviewDecision`; `SettingKey` (AC 2)
- [x] `crates/officina-host/src/registry/schema.rs`: migration 2, setting rebuild, pinned hash; `tests/fixtures/registry/v1.sql` (AC 1-3)
- [x] Writer and reader methods for `work_order`; expiry at start and on the tick (AC 4, 7)
- [x] Binding in `orders::work_order` and its call on agents refresh; filing rule in `registry::filing` and the D-79 function (AC 5, 6)
- [x] Retention rules (AC 8)
- [x] Tests: `registry_schema.rs`, `registry_orders.rs`, `registry_retention.rs`, `watch.rs` (AC 1-8)
- [x] Update `CHANGELOG.md` (AC 9)

## Dev Notes

- Requirements: FR-4's recording, binding and filing (as amended by amendment 2), FR-14's "proposals, starts, reviews live in the registry". Decisions: AD-40 and AD-41 as amended, AD-81, AD-82. Epic decisions: P-14 (Epic 3, moved to migration 2 then superseded by D-133), D-45, D-57, D-79, D-82, D-85, D-88, D-89; D-124, D-130, D-133 to D-136, D-158.
- Seams (D-124, read through `main`, D-563): `crates/officina-host/src/registry/schema.rs` (`MIGRATIONS` holds `V1` alone today) and `officina-core::registry::types` (`SettingKey`: `restore_notice`, `triage_model`, `triage_model_pause`, `auto_archive`), the creation function of D-79 (3.2 to 3.4), `registry::filing` (3.4), the hourly pass (`registry::retention`, 3.2).
- Type names (D-576): v0.3's `registry::proposal` exports the TypeScript types `Proposal` and `ProposalSource` (verdict proposals); this story's `ProposalKind`, `ProposalStatus` and `WorkOrderSource` are other names and keep them.
- D-133 is the reason the `proposal`, `orchestrator_start` and `auto_review` tables are created here, empty, although their writers come in 5.13, 5.16 and 5.17: the owner's registry reaches version 2 with a Slice A build. If a later story finds a hole, it adds migration 3 and its fixture; it never edits migration 2.
- `auto_review.cause` and `proposal.invalid_reason` carry no `CHECK` (D-133): their enums (`EscalationCause`, `ProposalInvalidReason`) do not exist yet and grow with their stories.
- Writer methods take `now_ms`; tests that write declare `required-features = ["window-authority"]` as `registry_write.rs` does.
- The launch itself (cards, the spawn, the journal line) is story 5.7; this story records what it is given.
- No employer content in the fixture.

## Testing

- Core unit: `crates/officina-core/src/{registry/types,registry/filing,registry/retention,orders/work_order}.rs`.
- Host integration: `crates/officina-host/tests/{registry_schema,registry_orders,registry_retention}.rs`.
- CLI integration: `crates/officina/tests/watch.rs`.

## Dev Agent Record

### Debug Log

- The schema test's column parser orders rows by `rowid` in its first draft; `session_fact` is `WITHOUT ROWID`, so the comparison orders by every column instead.
- The tests that simulated "a version past this build" appended their extra migration to `MIGRATIONS[0]`, which now reads as version 1, older than the build: they append it to `MIGRATIONS` (D-659). `registry_store.rs`'s crash-helper cases read `Older` until the helper example was rebuilt, as CI's step does.
- `ALTER TABLE ... RENAME` writes the rebuilt `setting` back as `CREATE TABLE "setting"`, one column per line, so the `CHECK` parser of `registry_schema.rs` reads it unchanged.
- The first Windows gate run, alongside the Linux container, failed `approvals.rs::wait_times_out_with_the_retry_message` at 9.04 s against its 6 s ceiling, a timing test outside this story; it was run again alone.

### Completion Notes

Every AC is implemented and proven by the tests named; decisions D-658 to D-663 (epic-5-decisions.md, "Implementation (2026-10-29)"), none to confirm. No new dependency or feature.

- AC 1: host `registry_schema.rs::{every_table_is_strict, migration_hashes_are_pinned, every_past_version_has_a_fixture, migrating_a_database_with_data_copies_it_first, the_setting_rebuild_keeps_every_row, version_1_with_data_migrates_to_2}`; the fixture `crates/officina-host/tests/fixtures/registry/v1.sql` holds rows only, schema 1 built by `MIGRATIONS[..1]` (D-659). The copy is `pre-v2-<UTC date>.db`, the name every migration's copy has (D-659).
- AC 2: `registry_schema.rs::{check_lists_match_the_core_enums, columns_without_check_are_the_ones_d_88_names}` (D-133's columns added, `auto_review.cause` and `proposal.invalid_reason` without `CHECK`); core `registry/types.rs::tests::{every_variant_round_trips_its_text, unknown_text_is_refused}` over the new enums' `ALL`, their count tied to the file's `closed_enum!` blocks; `relay::tests::code_equals_the_serde_name_for_every_variant` covers `relay::auto::ReviewDecision`.
- AC 3: `registry_schema.rs::{work_order_names_are_unique, work_order_template_and_seq_are_unique, a_proposal_without_kind_is_accepted, orchestrator_start_keeps_the_pid_creation_time, not_null_columns_are_the_ones_the_story_names, no_foreign_key_points_at_session_or_subject, the_counting_indexes_exist}`. The text keys are also `NOT NULL` (D-658).
- AC 4: host `registry_orders.rs::{highest_seq_reads_the_template_rows, a_taken_name_is_refused_as_name_taken, a_launch_is_recorded_launching_then_its_outcome, printed_and_error_are_bounded, stale_launching_rows_become_unknown_at_start}` at explicit `now_ms`; CLI `watch.rs::a_stale_launching_row_becomes_unknown_on_the_tick` (a row expired before the first board, another written while the window runs expired by the tick); core `orders/work_order.rs::tests::printed_and_error_are_cut_on_character_boundaries`. API as built: D-660.
- AC 5: core `orders/work_order.rs::tests::{binds_by_name_folder_and_start_time, a_session_started_too_early_does_not_bind, an_entry_outside_the_folder_does_not_bind, an_order_older_than_10_minutes_does_not_bind}`; CLI `watch.rs::a_started_order_binds_to_its_background_session` (the fake's `bg` mode starts and lists the session; the id `--bg` printed is the bound entry's). Rules as built: D-662.
- AC 6: core `registry/filing.rs::tests::{a_bound_order_files_its_session_first, author_filing_beats_rule_filing, a_new_subject_name_follows_d_57_and_d_158}`; host `registry_orders.rs::{a_new_subject_is_created_at_started_not_at_the_click, a_new_name_already_taken_files_into_that_subject, a_session_seen_before_binding_is_refiled, a_session_first_seen_at_binding_is_filed_under_its_order}`. D-661.
- AC 7: host `registry_orders.rs::work_orders_read_back_newest_first_with_untrusted_texts` (through a command line's reader: 50 rows, newest first, the bound session's record, `printed` as `Untrusted<String>` whose `Debug` hides it).
- AC 8: host `registry_retention.rs::{work_order_rows_are_deleted_at_90_days, work_order_texts_are_cleared_with_the_excerpt_ttl}` (the cleared texts gone from the database and its WAL); core `retention.rs::tests::ttl_copy_lifetime_and_pass_interval_stay_within_30_days` unchanged. D-663.
- AC 9: `CHANGELOG.md` `[Unreleased]` Changed (schema version 2, the copy) and Added (the records); README "Work orders (not offered yet)" and "Upgrading from v0.4".
- Not built here, by design: the card store, the launch command and the journal line are story 5.7's; it maps `NameTaken` to `LaunchRefusal::NameTaken` and passes the reserved `seq` (D-660). The view of the history (`unbound`, "printed id differs") is 5.8's, computed from the stored columns (D-136).
- Gate (2026-10-29), on `7859032`: the full local replay of `ci.yml`. Windows green: Rust 1260 passed, 0 failed, 11 ignored; the launch half without the authority, perf, lexer differential, approvals stress, registry crash, authority markers, registry dependencies, bundled roots and e2e fixtures green (the fixtures unchanged); Vitest 279; Playwright 978 (x3); `cargo deny` ok; `pnpm audit` 1 low, ignored as before. Linux (`rust:1-bookworm`): 1122 passed, 0 failed, 9 ignored, and the spawn step alone.

## File List

- `CHANGELOG.md`, `README.md`
- `crates/officina-core/src/spawn.rs`, `crates/officina-core/src/orders/work_order.rs`
- `crates/officina-core/src/registry/{filing,retention,types}.rs`
- `crates/officina-core/src/relay/mod.rs`, `crates/officina-core/src/relay/auto.rs` (new)
- `crates/officina-host/Cargo.toml`, `crates/officina-host/src/{doctor,watch}.rs`
- `crates/officina-host/src/registry/{agents,ingest,mod,read,retention,schema,window}.rs`, `crates/officina-host/src/registry/orders.rs` (new)
- `crates/officina-host/tests/{registry_retention,registry_schema,registry_store}.rs`, `crates/officina-host/tests/registry_orders.rs` (new), `crates/officina-host/tests/fixtures/registry/v1.sql` (new)
- `crates/officina/tests/watch.rs`
- `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this file

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: migration 2's shape stated per table (`NOT NULL`, keys, `UNIQUE(name)`, `UNIQUE(template, seq)`, `cwd`, `pid_created_at_ms`, nullable `proposal.kind`, no foreign key, counting indexes) (F2, F3); `seq` reserved by the card store, `name_taken` (F2); binding compares the stored `cwd` (F3); stale `launching` rows expire on the tick too (F14); the taken-name rule recorded as D-158 (F33)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: `ProposalKind` holds `answer` from migration 2 for story 5.21 (D-575); the names v0.3 exports are kept apart (D-576); seams cited by file (D-563)
- 2026-10-29 implemented; decisions D-658 to D-663; status Review
