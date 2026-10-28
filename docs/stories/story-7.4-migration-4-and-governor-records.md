# Story 7.4: Migration 4 and the governor's records

**Status:** Approved

## Story

As the author whose registry already holds v0.5's work orders and v0.6's workshop runs,
I want the governor's records (merge checks, order states, routine links, held packages, the governor's source on a work order) stored by one migration written whole, with every enum-valued column checked and every row bounded in time,
so that no later story reshapes a table my registry already has, and a suspended order cannot come back through a floor edit (R81, R82).

## Acceptance Criteria

1. **Enums declared whole** (D-310 as amended by review F8). `closed_enum!` declarations in the modules that own them: `WorkOrderSource` gains `governor`; `OrderState` (`on`, `off`, `suspended`, `paused`); `MergeOutcome` (`pending`, `merged`, `merged_changed`, `merged_equivalent`, `not_merged`); `SettingKey` gains `governor_mode`, `governor_self_stop` and `gate_on_merges`; `GovernorMode` (`off`, `shadow`, `on`) and `SelfStopCause` (`reserve_reached`, `held_expired`, `credential_found`), whose values `setting.value` holds without a `CHECK`, so stories 7.9 and 7.10 can use them before 7.11 and 7.15 give them behaviour. Later stories add behaviour, never a variant. Proof: core `crates/officina-core/src/registry/types.rs::tests::{governor_is_a_work_order_source, governor_mode_and_self_stop_round_trip_through_setting_values}`; the pinned hash of AC 3.
2. **Migration 4** (AD-142 as D-309 reads it). `MIGRATIONS[3]` runs `VACUUM INTO registry/backup/pre-v4.db` first, then, in its transaction: `work_order` rebuilt with every column, default, `NOT NULL` and `CHECK` of migrations 2 and 3, `UNIQUE(name)`, `UNIQUE(template, seq)` and `work_order(launched_at_ms)` recreated, `source`'s `CHECK` from `WorkOrderSource::ALL`, `standing_order TEXT` with `CHECK ((source = 'governor') = (standing_order IS NOT NULL))` and the index `work_order(standing_order, launched_at_ms)`; `setting` rebuilt, its key `CHECK` from `SettingKey::ALL`; `workshop_run.held_at_ms` and `released_at_ms` added, with the partial index on held packages not yet released; `standing_order_state` (with `identity_sha256`, D-350, and its index), `merge_check` and `standing_order_subject` created with D-309's `NOT NULL` lists, `CHECK` on `state`, `outcome` and both `src` columns, `CHECK ((outcome = 'pending') = (settled_at_ms IS NULL))` on `merge_check` (review F36), none on `cause`, the index `merge_check(outcome)`; no foreign key. Every `CHECK` list is read from its enum's `ALL`. Proof: host integration `crates/officina-host/tests/registry_schema.rs::{migration_4_backs_up_first, a_version_3_registry_migrates_with_every_row, rebuilt_tables_keep_their_unique_constraints_and_indexes, every_enum_column_refuses_an_unknown_value, a_governor_work_order_needs_its_standing_order, a_composer_work_order_refuses_a_standing_order, a_pending_merge_row_refuses_a_settled_time, a_final_merge_row_needs_one, cause_columns_accept_any_text}` (the migration test starts from a version 3 fixture with rows in `work_order` and `setting` and compares every row; the enum test iterates each enum's `ALL` to build a valid row, then one invalid value per column).
3. **Pinned** (D-133, D-309). Migration 4's SQL text is hashed; the test pins it. Proof: `registry_schema.rs::migration_4_is_pinned`.
4. **Records** (AD-142). `registry::governor` gains typed writers and readers, each named here for the story that uses it: an order's state with source, cause, numbers and `identity_sha256` (5.6's pattern: a cause no variant parses reads `unknown`, never dropped) and its `absent_since_ms`; the suspended and paused rows by identity, present or absent for at most 90 days (7.6, D-350); `merge_check` rows inserted `pending`, made final once with `settled_at_ms` (a final row is never rewritten), corrected with source `author`; routine links added and removed; `held_at_ms` and `released_at_ms` set once each; queries: imported runs whose merge row is absent or `pending` (story 7.5), a template's runs with their verdict, quarantine, outcome and merge row in settlement order (7.6), an order's autonomous runs by finish time (7.6), an order's latest real launch (7.6), held packages oldest first (7.15). Each typed writer inserts its row with every `Option` field `None`, which proves the schema's nullability from the row types rather than from a list retyped beside the pinned SQL (review F33). Proof: host unit `crates/officina-host/src/registry/governor.rs::tests::{a_final_merge_row_is_never_rewritten, a_correction_writes_source_author, an_unknown_cause_reads_unknown, held_packages_list_oldest_first, settlement_rows_join_work_order_workshop_run_and_merge_check, every_writer_accepts_its_row_with_every_option_none, suspended_identities_include_absent_rows_of_90_days}`.
5. **Retention** (D-311, D-134, D-212). In the hourly pass: `merge_check` rows go with their `workshop_run` in D-212's 90-day transaction; the pass sets `absent_since_ms` on a state row whose order is missing from the approved snapshot and clears it when the order returns; a state row goes 90 days after its `absent_since_ms`; a link goes with its order's state row, or, for an order with no state row, when the order is absent at a pass and the link is older than 90 days. The last rule departs from AD-142's "absent for 90 days" (D-311, review F24). Proof: host `crates/officina-host/tests/registry_retention.rs::{merge_checks_go_with_their_run, absent_orders_lose_their_state_after_90_days, a_returning_order_keeps_its_suspension, a_renamed_suspended_order_keeps_its_identity_row, links_of_stateless_absent_orders_go_after_90_days}`.
6. **No untrusted text** (AD-142). No new column holds text from a session, a model, a worker or the probe: `cause` holds an enum name, `numbers_json` typed numbers, `tip_seen` an object id, `identity_sha256` a hash. Proof: the typed writers of AC 4 take only enums, numbers, ids and author names from the approved catalogue (a row type cannot hold `Untrusted`, checked by `crates/officina-host/tests/registry_schema.rs::governor_row_types_hold_no_untrusted_field`, which scans `registry/governor.rs` for the token as story 7.2's scan does); review.
7. **The schema number of a generated payload** (P-63, review F20). A payload whose generated type changes with `WorkOrderSource::Governor` (`OrchestratorView`'s history rows, if they carry the source) moves its schema number once, relative to `main`, and the TypeScript bindings are regenerated (`OFFICINA_WRITE_TS=1 cargo test -p officina-core --features ts --test typescript_bindings`). Proof: `crates/officina-core/tests/typescript_bindings.rs` passes with the committed bindings; the schema case of the payload's own test.
8. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: registry version 4 (governor tables), with the backup file named; README privacy: what the governor stores and for how long. Proof: review.

## Tasks

- [ ] Enums in their modules (AC 1)
- [ ] `MIGRATIONS[3]`, the two rebuilds, the new tables and indexes, the pinned hash (AC 2, 3)
- [ ] Version 3 fixture with rows in both rebuilt tables (AC 2)
- [ ] `crates/officina-host/src/registry/governor.rs` writers, readers, queries (AC 4)
- [ ] Retention rules and `absent_since_ms` in the hourly pass (AC 5)
- [ ] Row-type scan; schema bump and bindings (AC 6, 7)
- [ ] `CHANGELOG.md`, README (AC 8)

## Dev Notes

- Requirements: FR-1, FR-3, FR-8, FR-12 step 5, FR-17 (routine links). Decisions: AD-40, AD-41, AD-44, AD-82, AD-119, AD-142. Risks: R81, R82. Epic decisions: D-133, D-134 (Epic 5); D-212, D-213, P-53 (Epic 6); P-63, P-65, D-300, D-309, D-310, D-311, D-340, D-350.
- Seams (D-300): `registry::schema::MIGRATIONS` and `SettingKey` (3.1, 5.6, 6.4), the hourly pass and D-212's workshop retention (3.2, 6.4).
- Waits for: story 7.1 (FR-0).
- Why the whole migration here: the owner runs Slice A builds for weeks of shadow before Slice B (PRD section 6.1.1), so version 4 reaches a real registry long before the launch columns are used; a hole found after this merges is migration 5 (D-133).
- The two rebuilds follow migration 2's `setting` rebuild (create, copy, drop, rename, indexes and constraints recreated); widening a `CHECK` in SQLite needs one. The migration test compares every row and reads back every `UNIQUE`, index and `CHECK` from `sqlite_master`, since a rebuild that forgets one passes a row-count test (R81).
- `absent_since_ms` is the one column D-309 adds to AD-142's table: without it, "absent for 90 days" cannot be computed, and a state row older than 90 days would be deleted the day its order disappears, lifting a suspension when the order returns.
- Probe counts, ratio samples, bookings and hold history stay in the journal (AD-142): each is a fold over lines the journal keeps 90 days.

## Testing

- Core unit: `crates/officina-core/src/registry/types.rs`.
- Host unit: `crates/officina-host/src/registry/governor.rs`; host integration: `crates/officina-host/tests/{registry_schema,registry_retention}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `GovernorMode` and `SelfStopCause` declared here (F8); `identity_sha256` against renames (F18); `merge_check`'s pending and settled `CHECK` (F36); P-63's schema bump and bindings (F20); nullability proved from row types, no retyped list (F33); the links' retention listed as a departure (F24)
