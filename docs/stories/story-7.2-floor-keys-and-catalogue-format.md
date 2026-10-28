# Story 7.2: Governor keys, standing orders, the chord and scope grammars, and the catalogue format

**Status:** Approved

## Story

As the author who will let routine work run without my click,
I want the governor's settings and my standing orders to be floor keys that only my catalogue approval makes usable, validated by the same value check a click uses,
so that a session editing my floor to add an order or loosen a threshold gets nothing until I approve the diff, and nothing a model writes can become a standing order (FR-10, principle 8).

## Acceptance Criteria

1. **Raw tables** (AD-128 phase 1, D-306). The floor gains a raw `governor: Option<toml::Spanned<toml::Value>>` and a raw `standing_order: Vec<toml::Spanned<toml::Value>>`; `RELAY_KEYS` gains both; nothing in them can invalidate the floor the board uses. Proof: core `crates/officina-core/src/floor.rs::tests::{governor_tables_are_relay_keys, a_broken_governor_table_leaves_the_board_floor_valid}` and D-7's existing relay-keys test.
2. **Phase 2 rules** (FR-10 as AD-128 reads it, D-306). Each rule of FR-10 and `MaxUsdRequired` is an `IssueKind` variant listed with its table (`governor` or `standing_order`), with a failing fixture: `window` as two different `HH:MM` within 00:00 to 23:59, stored as minute numbers; `days` from `mon` to `sun`, case-sensitive, one to seven, distinct; every number within its range, `five_hour_max_pct` at most 85; `usd_per_week` at most two decimals, stored as cents, required with `[workshop] auth = "api-key"` and refused otherwise; `stop_shortcut` by AC 4's grammar; an order's `name` passing `is_valid_action_name`, unique, at most 16 orders; `template` an approved workshop template (a host template is an issue); `project` one of the template's with a `[project.workshop]` table; `values` through AC 3's check; `base` in the template's base list; `every_hours` within 12 to 720, default 72; `paths` by AC 5's grammar. Without a valid `[governor]`, the governor is unavailable and every standing order is listed with that issue. Proof: core `crates/officina-core/src/relay/catalogue.rs::tests::every_issue_kind_has_a_case` (iterates `IssueKind::ALL`), and `governor/floor.rs::tests::{an_invalid_governor_lists_every_order_as_unavailable, a_host_template_is_an_issue, usd_per_week_is_refused_on_an_oauth_floor}`.
3. **One value check** (AD-128, AD-77, D-306). `template::check_values(&Template, &ProjectName, &values)` is extracted from `WorkOrder::validate` (story 5.5), which now calls it, and phase 2 calls it for each order: each parameter present once, each value in its list. The `values` section of `tests/vectors/governor.json` is replayed through both callers, which must refuse the same inputs with the same cause. Proof: core `crates/officina-core/tests/governor_vectors.rs::values_cases_refuse_alike`; v0.5's `orders_vectors.rs` cases still pass.
4. **The chord grammar** (AD-141, D-306). `governor::chord::parse`: tokens joined by `+`, case-insensitive; at least two distinct modifiers among `Ctrl`, `Alt`, `Shift` and `Win`, then exactly one key among `A` to `Z`, `F1` to `F24` and `Pause`; the normal form orders the modifiers `Ctrl`, `Alt`, `Shift`, `Win` and spells the key in uppercase. `ChordError` is a `closed_enum!`. Proof: the `chord` section through `governor_vectors.rs::{chord_cases_match, every_chord_error_has_a_case}`.
5. **The scope grammar and matcher** (D-307). `governor::scope`: at most 32 patterns, AD-113's glob syntax, no `.` or `..` segment, compiled at validation; matching anchored at the project root, `*` within a segment, `**` across segments, case-sensitive; a package path is in scope when it matches some pattern. Cases include `src/**` refusing `vendor/src/x.rs`, `Cargo.toml` refusing `sub/Cargo.toml`, a case difference refused, and both sides of a rename checked. Proof: the `scope` section through `governor_vectors.rs::scope_cases_match`; `governor/scope.rs::tests::a_rename_needs_both_sides_in_scope`.
6. **Canonical form and the format bump** (AD-128, AD-76, D-131, D-306, P-63). `CanonicalGovernor` holds every key with its default explicit, `window` as two minute numbers, `days` in week order, `usd_per_week` in cents or absent, the chord in its normal form; an absent `[governor]` is canonical as absent. `standing_orders: BTreeMap<String, CanonicalStandingOrder>` holds template, project, values sorted by parameter, base, `every_hours`, and paths sorted and deduplicated. New fields take `#[serde(default)]`. `CANONICAL_FORMAT` goes up by one relative to `main` at this story's merge, once for v0.7, `stop_shortcut` included. Proof: core `catalogue.rs::tests::{an_absent_governor_is_canonical_as_absent, defaults_are_explicit_in_the_canonical_form, standing_orders_sort_values_and_paths}`; `crates/officina-core/tests/catalogue_hash.rs` golden hash re-pinned, and its previous-format fixture test loading an `approved.json` written in the previous format and asserting AD-76's four facts again.
7. **Approved standing orders** (AD-128, AD-77's pattern, D-306). `ApprovedStandingOrder::new(&ApprovedCatalogue, &RelayFloor, name)` is the only constructor: it requires the current floor's hash to equal the approved one and reads the order and its template from the approved snapshot. `governor::floor::approved_config` returns the approved `GovernorConfig` and orders, or why there are none (no valid table, no order). Proof: core `governor/floor.rs::tests::{a_pending_change_gives_no_standing_order, orders_come_from_the_approved_snapshot_only, no_order_reads_unavailable}`; a `compile_fail` doctest on an `ApprovedStandingOrder` struct literal.
8. **Diff and banner** (AD-128). `catalogue::diff` gains a governor section and a standing orders section; the catalogue banner shows them. Proof: core `catalogue.rs::tests::diff_lists_governor_and_standing_order_changes`; Playwright `app/e2e/relay.spec.ts > the catalogue banner lists a standing order change` on a fixture from `approve_fixture.rs` (D-25).
9. **Principle 8 by source** (AD-148, D-305 as amended by review F17). `crates/officina-core/tests/governor_sources.rs` reads every file under `src/governor/` and fails on the tokens `Untrusted`, `DisplayText` and `untrusted::`, and on any `crate::` path outside the allow-list D-305 names (`governor`, `dates`, `closed_enum`, the catalogue's approved types, `workshop::LaunchRefusal` and the readiness facts' module), each allowed module's file scanned for the same tokens; the scan applied to `src/untrusted.rs` finds the tokens, and a fixture naming a module outside the list fails it. Proof: `governor_sources.rs::{no_governor_source_names_untrusted_text, no_governor_source_reaches_outside_the_allow_list, the_scan_finds_the_token_where_it_lives, a_path_outside_the_allow_list_fails_the_scan}`.
10. **Doctor** (AD-147 Slice A, D-341). `[governor]` and standing order issues, each with its table. Proof: host `crates/officina-host/src/governor/doctor.rs::tests::governor_and_standing_order_issues_are_reported` (the module that holds every governor doctor item, D-341).
11. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: the `[governor]` table and `[[standing_order]]`, approved with the catalogue; under Changed: the catalogue format goes up and voids the approval once. README section "The governor (v0.7, in progress)" created: the keys, that an order is usable only once approved, the exposure sentence of FR-12 beside `[[standing_order]]` ("Do not write a standing order for a repository you would not paste into a model"), the upgrade note. Proof: review.

## Tasks

- [ ] Raw tables and `RELAY_KEYS` (AC 1)
- [ ] `crates/officina-core/src/governor/{mod,floor}.rs`: types, phase 2, `IssueKind` variants and fixtures, `approved_config`, `ApprovedStandingOrder` (AC 2, 7)
- [ ] Extract `template::check_values`; `values` section (AC 3)
- [ ] `governor/chord.rs`, `governor/scope.rs` and their sections; `tests/vectors/governor.json` created and added to every formatter's ignore list (AC 4, 5)
- [ ] Canonical types, format bump, golden hash, previous-format fixture (AC 6)
- [ ] Diff sections and the banner (AC 8)
- [ ] `crates/officina-core/tests/governor_sources.rs` (AC 9)
- [ ] Doctor; `CHANGELOG.md`, README (AC 10, 11)

## Dev Notes

- Requirements: FR-10, FR-12's Exposure paragraph, FR-16's chord rule, PRD section 5's first non-goal. Decisions: AD-23, AD-24, AD-76, AD-77, AD-113 (syntax only), AD-127, AD-128, AD-141 (grammar). Risks: R74, R82. Epic decisions: D-7 (Epic 2); D-131, D-132 (Epic 5); P-53 (Epic 6); P-63, P-65, D-300, D-305, D-306, D-307, D-340, D-341.
- Seams (D-300): `floor::RELAY_KEYS` and the raw tables (2.1, 6.2), `relay::catalogue` phase 2, canonical form and diff (2.1, 2.2, 2.11, 5.4, 6.2), the banner (2.7, 2.9), `WorkOrder::validate` (5.5), the workshop keys v0.6 added (`[template.workshop]` with its base list, `[project.workshop]`, `[workshop] auth`; 6.2).
- Waits for: E60 to E63 (story 7.1), since FR-0 holds every Slice A story behind them.
- Every v0.7 canonical key lands here (P-63), so the golden hash is re-pinned once; `stop_shortcut` is validated now and registered from story 7.17, as P-53 did for `memory_name`.
- An order's state is keyed on its name (AD-128, R82): nothing here stores state; editing an order can never lift a suspension (story 7.6).
- `scope` lands here because `paths` must compile at validation; story 7.15 uses the matcher at collection.
- `governor_sources.rs` must find the token in `src/untrusted.rs` and fail on a path outside the allow-list, otherwise a scan that reads nothing would pass (Epic 5 review F22). It proves the absence of untrusted text types, not of untrusted numbers, which P-64 covers (README, D-340).
- No employer content in fixtures: placeholder templates, projects and packages.

## Testing

- Core unit: `crates/officina-core/src/{floor,relay/catalogue,governor/floor,governor/chord,governor/scope}.rs`; core integration: `crates/officina-core/tests/{governor_vectors,governor_sources,catalogue_hash,orders_vectors}.rs`; doctests.
- Host unit: `crates/officina-host/src/governor/doctor.rs`.
- Playwright: `app/e2e/relay.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the source scan also refuses paths outside an allow-list (F17); governor doctor items in `governor/doctor.rs` (F30)
