# Story 5.6: Registry migration 2, work order records, binding and filing

**Status:** Approved

## Story

As the author whose launched work must land in the right subject and stay on record,
I want the registry to gain, in one migration, every table v0.5 needs, and work orders to be recorded, bound to the session they started by its unique name, and filed under the subject I chose,
so that a launch is never lost or misfiled, and my registry never needs a second v0.5 migration because I used a Slice A build.

## Acceptance Criteria

1. **Migration 2, whole** (AD-82, D-133). `schema::MIGRATIONS[1]` creates, as STRICT tables, `work_order`, `proposal`, `orchestrator_start` and `auto_review` with AD-82's columns, and rebuilds `setting` (create, copy, drop, rename, in the migration's transaction) so its key `CHECK` admits `auto_approval`, `auto_mode` and `briefing_read_at_ms`; the runner of story 3.1 runs `VACUUM INTO registry/backup/pre-v2.db` first; its SHA-256 is pinned. Proof: host `crates/officina-host/tests/registry_schema.rs::{every_table_is_strict, migration_hashes_are_pinned, every_past_version_has_a_fixture, migrating_a_database_with_data_copies_it_first, the_setting_rebuild_keeps_every_row, version_1_with_data_migrates_to_2}` (a new fixture `tests/fixtures/registry/v1.sql` holds version 1 with rows).
2. **Enums and `CHECK` lists** (AD-82, D-133). `registry::types` declares with `closed_enum!`: `WorkOrderSource` (`composer`, `proposal`), `LaunchOutcome` (`launching`, `started`, `untrusted_workspace`, `failed`, `unknown`), `ProposalKind` (`recap`, `work_order`), `ProposalStatus` (`pending`, `invalid`, `accepted`, `dismissed`, `expired`, `superseded`), `OrchestratorStartOutcome` (`spawned`, `refused`, `failed`), `ReviewMode` (`shadow`, `live`, `rule`), `ReviewOutcome` (`no_call`, `answered`, `invalid`, `failed`, `timeout`), `AuthorDecision` (`approved`, `rejected`), `AuthorReason` (`should_not_run`, `not_now`); `SettingKey` gains the three keys; `relay::auto` is created with `ReviewDecision` (`approve`, `reject`, `escalate`) alone. `CHECK` lists sit on exactly the columns D-133 names, read from the enums' `ALL`. Proof: `registry_schema.rs::{check_lists_match_the_core_enums, columns_without_check_are_the_ones_d_88_names}` (the second extended with D-133's columns); core `crates/officina-core/src/registry/types.rs::tests::{every_variant_round_trips_its_text, unknown_text_is_refused}` iterating the new enums' `ALL`.
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

- [ ] Enums in `registry::types`; `relay/auto.rs` with `ReviewDecision`; `SettingKey` (AC 2)
- [ ] `crates/officina-host/src/registry/schema.rs`: migration 2, setting rebuild, pinned hash; `tests/fixtures/registry/v1.sql` (AC 1-3)
- [ ] Writer and reader methods for `work_order`; expiry at start and on the tick (AC 4, 7)
- [ ] Binding in `orders::work_order` and its call on agents refresh; filing rule in `registry::filing` and the D-79 function (AC 5, 6)
- [ ] Retention rules (AC 8)
- [ ] Tests: `registry_schema.rs`, `registry_orders.rs`, `registry_retention.rs`, `watch.rs` (AC 1-8)
- [ ] Update `CHANGELOG.md` (AC 9)

## Dev Notes

- Requirements: FR-4's recording, binding and filing (as amended by amendment 2), FR-14's "proposals, starts, reviews live in the registry". Decisions: AD-40 and AD-41 as amended, AD-81, AD-82. Epic decisions: P-14 (Epic 3, moved to migration 2 then superseded by D-133), D-45, D-57, D-79, D-82, D-85, D-88, D-89; D-124, D-130, D-133 to D-136, D-158.
- Seams (D-124): `registry::schema` and `SettingKey` (3.1), the creation function of D-79 (3.2 to 3.4), `registry::filing` (3.4), the hourly pass (3.2).
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

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: migration 2's shape stated per table (`NOT NULL`, keys, `UNIQUE(name)`, `UNIQUE(template, seq)`, `cwd`, `pid_created_at_ms`, nullable `proposal.kind`, no foreign key, counting indexes) (F2, F3); `seq` reserved by the card store, `name_taken` (F2); binding compares the stored `cwd` (F3); stale `launching` rows expire on the tick too (F14); the taken-name rule recorded as D-158 (F33)
