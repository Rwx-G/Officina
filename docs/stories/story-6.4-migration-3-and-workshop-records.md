# Story 6.4: Migration 3, workshop records, the home folder and retention

**Status:** Approved

## Story

As the author whose registry already holds v0.5's work orders,
I want the workshop's runs, approvals, images, inventories and batteries stored by one migration written whole, with every enum-valued column checked and every text bounded in time,
so that no later story needs to reshape a table my registry already has, and nothing a worker wrote outlives the 30-day bound.

## Acceptance Criteria

1. **Enums declared whole** (AD-119, D-212). `officina-core::workshop` declares with `closed_enum!`, each in the module that owns it: `Runner` (`host`, `workshop`), `RunState` (AD-109's states, `no_change` terminal), `WorkshopOutcome` (`package`, `no_change`, `failed`, `not_collected`, `unknown`), `Verdict` (`imported`, `discarded`), `DiscardReason` (the author's three, `expired`, `not_collected`), `ImageKind` (`workshop`, `browser`, `gateway`, `project`), `BuildOutcome`, `BatteryOutcome` (D-212); `CredentialKind` is story 6.2's. Later stories add behaviour, never a variant (a new variant changes migration 3's text, which the pinned hash refuses, D-133). Proof: core `crates/officina-core/src/workshop/run.rs::tests::no_change_is_terminal`; the hash test of AC 3.
2. **Migration 3** (AD-119 as D-212 reads it, P-53). The workshop migration is the next entry of `MIGRATIONS` relative to `main` at this merge ("migration 3" in these files); its index, the backup file `registry/backup/pre-v<n>.db` and the tests derive from `MIGRATIONS.len()`, never a literal (review F24). It runs `VACUUM INTO` the backup first, then: `work_order.runner TEXT NOT NULL DEFAULT 'host'` with its `CHECK`; `workshop_run` with AD-119's columns plus `battery_id`, its `NOT NULL` list per D-212, `CHECK` on `state`, `outcome`, `credential_kind`, `verdict`, `discard_reason`, none on `cause` and `quarantine_cause`; `workshop_approval` (`run_id`, `group_key`) WITHOUT ROWID; `workshop_image` (`name`) without inventory columns, with `programs_sha256` (D-233), `CHECK` on `kind` and `outcome`; `workshop_inventory` (`image`, `project`) WITHOUT ROWID; `workshop_battery` with `clock_skew_ms` (nullable, D-242), `vector_sha256` and `programs_sha256` (both `NOT NULL`, D-232, review F4), `CHECK` on `outcome`; indexes `workshop_run(state)`, `workshop_run(finished_at_ms)`, `workshop_run(decided_at_ms)`, `workshop_battery(image_id, finished_at_ms)`; no foreign key. Every `CHECK` list is read from its enum's `ALL`. Proof: host integration `crates/officina-host/tests/registry_schema.rs::{the_workshop_migration_backs_up_first, a_previous_version_registry_migrates_with_its_work_orders_as_host, every_enum_column_refuses_an_unknown_value, cause_columns_accept_any_text, a_battery_row_needs_its_vector_and_programs_digests}` (the backup name and version read from `MIGRATIONS.len()`; the third iterates the enums' `ALL` to build a valid row, then one invalid value per column).
3. **Pinned** (D-133, D-212). The workshop migration's SQL text is hashed; the test pins it. The pinned hash freezes the shape, so no test retypes D-212's nullable list (derived, not transcribed). Proof: `registry_schema.rs::the_workshop_migration_is_pinned`.
4. **Records** (AD-119). `registry::workshop` gains typed writers and readers: insert a run with its `work_order` row in one transaction; state transitions with `state_at_ms`; facts and texts (`facts_json` typed and versioned in core; `texts_json` read back as `Untrusted<String>` only, D-130); package fields; approvals; verdicts; images, inventories and batteries; and the queries later stories need, each named here: runs by state, the live count, packages awaiting a verdict (`review`, `quarantined`), imported runs of a template since a time, the newest passing battery for a key (story 6.10), the image row by name. A cause read back with no parsing variant is `unknown`, never dropped. Proof: host unit `crates/officina-host/src/registry/workshop.rs::tests::{a_run_and_its_work_order_insert_together, texts_read_back_untrusted, an_unknown_cause_reads_unknown, awaiting_counts_review_and_quarantined_only}`.
5. **The home** (AD-124, AD-25, D-13). `<home>/workshop/` with `runs/`, `build/`, `battery/` and `empty-hooks/` is created by the window on first use; `empty-hooks/` is refused unless empty (story 6.3's rule reads it). D-13's protected directories gain the absolute `<home>/workshop` (and the `.officina/workshop` tail for the default home), so a guarded session's command naming it is denied with the relay's limits. Proof: host `crates/officina-host/src/relay/home.rs::tests::workshop_is_created_on_first_use`; core `crates/officina-core/src/relay/gate.rs::tests::the_workshop_folder_is_protected` and a case in `tests/vectors/gate.json`.
6. **The home in a sync root** (D-228, review F44). Doctor warns when the Officina home lies in a Cloud Files sync root (AD-39's attribute test), since base bundles of up to 1 GiB and `verify.git` would sync. Proof: host `crates/officina-host/src/doctor.rs::tests::a_home_in_a_sync_root_warns` (the attribute reader as an argument).
7. **Retention** (AD-124, D-85, D-134, D-213). Core's `run::retention(rows, now_ms)` returns actions; the hourly pass applies them: `texts_json` and `workshop_image.build_tail` cleared `EXCERPT_TTL_MS` after the run's `started_at_ms` (or the build's `built_at_ms`); a run folder deleted `RUN_FOLDER_TTL_MS` after launch, that constant derived as 30 days minus the hourly interval; `workshop_run` with its `work_order` and its `workshop_approval` rows deleted at 90 days in one transaction; `workshop_battery` rows at 90 days. Packages expiring (story 6.16) and kept containers (story 6.17) join later as D-213 says. D-85's bound test gains both derivations. Proof: core `crates/officina-core/src/workshop/run.rs::tests::{retention_clears_texts_at_the_excerpt_ttl, run_folders_go_before_30_days}`; host `crates/officina-host/tests/registry_retention.rs::{workshop_rows_go_with_their_work_order, the_30_day_bound_holds_for_workshop_texts_and_folders}`.
8. **Documentation** (D-226). `CHANGELOG.md` `[Unreleased]` under Added: the registry's workshop tables, with the backup file named; README privacy: what the workshop stores and for how long. Proof: review.

## Tasks

- [ ] Enums in `crates/officina-core/src/workshop/{run,review,image,battery}.rs` (AC 1)
- [ ] The workshop migration at `MIGRATIONS.len()`, pinned hash (AC 2, 3)
- [ ] `crates/officina-host/src/registry/workshop.rs` writers, readers, queries (AC 4)
- [ ] Home folders, D-13 protected directory, gate vector case (AC 5); doctor's sync-root warning (AC 6)
- [ ] `run::retention`, the hourly pass, the bound test (AC 7)
- [ ] `CHANGELOG.md`, README (AC 8)

## Dev Notes

- Requirements: FR-10 (records), FR-12 (run folder 30 days), FR-14 (kept containers, later), FR-15 (battery validity), PRD section 7 (metrics read the journal, not these tables). Decisions: AD-25, AD-32, AD-39, AD-44, AD-117, AD-119, AD-124. Risks: R47, R48, R49. Epic decisions: D-13 (Epic 2); D-85 (Epic 3); D-130, D-133, D-134 (Epic 5); P-53, D-200, D-212, D-213, D-228, D-232, D-233, D-242.
- Settled before this story starts, since the migration freezes here (review F4, F24): the battery's `vector_sha256` and `programs_sha256`, the image's `programs_sha256` (so `not_from_base` covers replaced programs and reserved `ENV` with no new `BuildOutcome` variant, D-233), and the relative migration number.
- Seams (D-200): `registry::schema::MIGRATIONS` (3.1, 5.6), the hourly pass (3.2, D-134), `host/relay/home.rs` and D-13's protected directories (2.3).
- Waits for: E26, E27.
- Why the enums land here: migration 3's `CHECK` lists are read from their `ALL`, and the pinned hash freezes them; a story that later needs a variant writes migration 4 (D-133). Each enum's later story adds transitions, parsing and display, not values.
- `workshop_inventory` replaces AD-119's inventory columns on `workshop_image` because `officina-workshop:local` serves every project without a Dockerfile, and each project's hooks give that image a different inventory (D-212).
- Run folders are not in registry copies, hence their own derived TTL (AD-124).

## Testing

- CI, no Docker. Core unit: `crates/officina-core/src/workshop/run.rs`, `relay/gate.rs`; `tests/vectors/gate.json`.
- Host unit and integration: `crates/officina-host/src/{registry/workshop,relay/home}.rs`, `crates/officina-host/tests/{registry_schema,registry_retention}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: battery `vector_sha256` and `programs_sha256`, image `programs_sha256` (F4); migration number, backup name and tests from `MIGRATIONS.len()` (F24); the retyped nullable-list test dropped, the pinned hash holds the shape (derived, not transcribed); doctor warns on a home in a sync root (F44)
