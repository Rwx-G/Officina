# Story 5.4: Templates, the new floor tables and the canonical catalogue format

**Status:** Approved

## Story

As the author who will delegate routine subjects from templates I wrote,
I want `[[template]]`, `[orchestrator]`, `[auto_review]` and the `auto` class read from my floor file, validated rule by rule and covered by the catalogue I approve,
so that a template can run only once I approved exactly its text and its closed lists, and nothing about them can break the board.

## Acceptance Criteria

1. **Phase 1** (AD-75, AD-23 as amended). `template`, `orchestrator` and `auto_review` are kept as raw spanned values, as `action` is; `floor::RELAY_KEYS` gains the three, so a schema 1 floor carrying one is an error naming `schema = 2`; a malformed template or table never invalidates the floor the board uses. Proof: D-7's constant test in `crates/officina-core/src/floor.rs::tests` passes with the new keys; `floor.rs::tests::a_schema_1_floor_with_a_template_names_schema_2`; `crates/officina-core/src/relay/catalogue.rs::tests::a_malformed_template_never_invalidates_the_floor`.
2. **Template types** (AD-77 types, AD-75). `officina-core::relay::template` declares `TemplateName` (`is_valid_action_name`), `ParamName` and `ProjectName` (the catalogue's name rules), `ModelAlias` (`closed_enum!`, D-132), `render(prompt, values)` and `worst_case_chars(prompt, params)`. Proof: core `crates/officina-core/src/relay/template.rs::tests::{names_follow_the_catalogue_rules, render_replaces_each_placeholder_once, worst_case_uses_each_longest_value, model_alias_round_trips_its_text}` (the last iterates `ModelAlias::ALL`).
3. **Template rules** (AD-75 phase 2, FR-2 as amended by amendment 1, D-132). Each template converts alone (`deny_unknown_fields`); each rule below is an `IssueKind` variant with a failing fixture, and a failing template is dropped with its issue, reported with its table: the name rule and uniqueness among templates (separate from actions); `description` 1 to 200 characters without control or format character; `projects` non-empty, an unknown project an issue that leaves the template, a template left with none dropped; braces delimit placeholders only (D-2); every placeholder names a declared parameter and every parameter is used; no control character but newline and tab, no character of `FORBIDDEN_FORMAT_RANGES`; the first character after leading whitespace is author text that is not `/`, `!` or `-` (D-159), and a prompt holding only whitespace is an issue; the worst-case rendering is at most 8,000 characters; a parameter has `values` only, at least one, `pattern` being the issue `TemplatePattern`; each value passes `template_value` (AC 4); `model` parses as `ModelAlias`. Proof: `catalogue.rs::tests::every_issue_kind_has_a_case` (iterates `IssueKind::ALL`), plus `catalogue.rs::tests::{template_names_are_separate_from_action_names, an_unknown_project_leaves_the_template, a_template_left_with_no_project_is_dropped, a_prompt_starting_with_a_placeholder_is_refused, a_leading_slash_bang_or_dash_after_whitespace_is_refused, the_longest_rendering_is_checked, pattern_is_refused_for_templates}`.
4. **Value rules split** (AD-75, AD-30, D-5). `relay::params` splits `check_value` into AD-30's runner-independent rules and the native runner's refusal of a leading `/` or `@`; `template_value` applies FR-2's length (at most 64) and character set (`[A-Za-z0-9._:/@+-]`, no space), then the runner-independent rules, so `@scope/pkg` is a valid template value while an action's native argument still refuses it. Proof: `crates/officina-core/src/relay/params.rs::tests::{template_value_accepts_a_scoped_package, template_value_refuses_a_space_and_65_characters, template_value_refuses_a_leading_dash_and_a_dot_segment, native_argument_values_still_refuse_a_leading_slash_or_at}`; `crates/officina-core/tests/argv_vectors.rs` unchanged.
5. **Actions and tables** (AD-75, AD-23 as amended, D-132). `class = "auto"` is valid (`AutoReserved` goes, with its fixture); `check` is allowed on `auto` with `after-check`'s rule (a valid `free` action with the same parameters); `auto_max_per_hour` is 1 to 60, default 6, and an issue on any other class; `class = "auto"` on a `kind = "http"` action is the issue `AutoOnHttp`, so HTTP actions stay on story 2.11's `human` path (D-156); a failing `[orchestrator]` or `[auto_review]` table is an issue. `classes::is_available(Auto)` stays false: `officina run` refuses an `auto` action with `class_not_available` and `run --list` marks it unavailable until story 5.12. Proof: `catalogue.rs::tests::{auto_is_a_valid_class, auto_check_names_a_valid_free_action, auto_max_per_hour_range_and_class, auto_on_an_http_action_is_an_issue, a_failing_orchestrator_table_is_an_issue, a_failing_auto_review_table_is_an_issue}`; the `classes.rs` doctest unchanged; CLI `crates/officina/tests/run.rs::auto_is_refused_as_not_available`.
6. **Canonical form** (AD-76, D-131). `CANONICAL_FORMAT` goes up by one relative to `main` at merge; `CanonicalCatalogue` gains `templates` (description, sorted projects, prompt as written, parameters with sorted values, model) and `auto_review { model }` with `haiku` explicit when absent; `CanonicalAction` gains `auto_max_per_hour` (`Some` with the default for `auto`, `None` otherwise); `[orchestrator]` stays out. The golden hash is re-pinned in this commit only. Proof: `crates/officina-core/tests/catalogue_hash.rs::{golden_fixture_hash_is_pinned, every_canonical_field_changes_the_hash}` on a golden floor that now declares a template, an `auto` action and `[auto_review]`; `catalogue.rs::tests::orchestrator_table_is_not_canonical`.
7. **The previous format** (AD-76's four facts, D-131). A test loads an `approved.json` written in the previous format: it parses (new fields `#[serde(default)]`); its stored hash is kept, never recomputed; `officina run` refuses with "catalogue changed, approval pending"; the gate keeps applying that snapshot's guarded projects; the pending diff shows the format change and the templates added. Proof: host `crates/officina-host/tests/authority.rs::an_approval_in_the_previous_format_keeps_guarding_and_waits_for_one_approval`; CLI `run.rs::previous_format_approval_refuses_as_pending`; `catalogue.rs::tests::diff_reports_the_format_change`.
8. **Diff** (AD-76). `catalogue::diff` gains a templates section (added, removed, changed per field) and the `auto_review` field. Proof: `catalogue.rs::tests::{diff_reports_templates_added_removed_and_changed, diff_reports_the_review_model}`; `diff_covers_every_canonical_field` extended to the new fields.
9. **Relay view** (AD-76, AD-35). The catalogue table gains a templates table (name, description, projects, parameters with their values, model) and the review model; the banner's diff lists templates and the format change. All text renders as text. Proof: Playwright `app/e2e/relay-view.spec.ts > the catalogue lists templates with their projects, values and model`, `> the pending diff shows the format change and the templates added`; `typescript_bindings.rs::committed_bindings_match_the_rust_types`.
10. **Doctor** (AD-94 part A, FR-15 as amended). Doctor lists template issues with their lines and the templates awaiting approval (in the pending diff). Proof: host `crates/officina-host/src/relay/doctor.rs::tests::{template_issues_are_listed_with_their_lines, templates_awaiting_approval_are_listed}`.
11. **Documentation** (D-149, R39). `CHANGELOG.md` `[Unreleased]`: Added `[[template]]`, `[orchestrator]`, `[auto_review]`, `auto_max_per_hour`; Changed: the catalogue format, with the note that the first start of v0.5 asks for one catalogue approval and keeps guarding meanwhile. README "The orchestrator (v0.5, in progress)": templates in the floor file with an example using placeholder project names, closed lists only, and the upgrade note. Proof: review; `crates/officina/tests/readme.rs` stays green.

## Tasks

- [ ] `floor.rs`: `RELAY_KEYS`; phase 1 raw tables (AC 1)
- [ ] `crates/officina-core/src/relay/template.rs` (AC 2)
- [ ] `relay/catalogue.rs`: template conversion and rules, `IssueKind` variants and fixtures, issue table, `auto` and `auto_max_per_hour`, the two tables (AC 3, 5)
- [ ] `relay/params.rs`: split and `template_value` (AC 4)
- [ ] Canonical form, format, golden floor and hash; previous-format fixture; diff (AC 6-8)
- [ ] Relay view templates table and diff; bindings (AC 9)
- [ ] Doctor lines (AC 10)
- [ ] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-2 as amended (amendment 1), FR-8's `[auto_review]` clause (amendment 5's malformed-table sentence, consumed by 5.13). Decisions: AD-23, AD-24 and AD-30 as amended, AD-75, AD-76, AD-77 (types only). Risks: R39. Epic decisions: D-2, D-5, D-7, D-17, D-20, D-39 (Epic 2); P-24, P-26, D-124, D-131, D-132, D-149, D-156, D-159.
- Seams (D-124): `relay::catalogue` (2.1, 2.2, 2.11) and HTTP actions (2.11, kept off `auto` by D-156), `relay::params` (2.1), `floor.rs` (2.1), `relay::classes` (2.5, 2.8), the Relay view (2.9), `relay/doctor.rs` (2.10), the gate's snapshot rule (D-14, D-15: unchanged, tested here for the previous format).
- D-131: the previous format is whatever `main` holds when this merges, normally the one story 2.11 introduced for credentials. Build the previous-format fixture by serializing the current canonical form before the change, not by hand.
- No floor schema bump (AD-75): templates change no default. A v0.4 binary reading a v0.5 floor reports an unknown key.
- `ApprovedTemplate` and `WorkOrder` are story 5.5; this story only parses, validates and canonicalises.
- If `RelayView` carries a schema number, it goes up by one here (P-26).
- The orchestrator's model is parsed here but used by story 5.16; the review model by 5.13.
- No employer content in fixtures, the golden floor or the README example: `backend`, `serde_json`, placeholder paths.

## Testing

- Core unit: `crates/officina-core/src/{floor,relay/template,relay/catalogue,relay/params}.rs`.
- Core integration: `crates/officina-core/tests/{catalogue_hash,argv_vectors,typescript_bindings}.rs`.
- Host: `crates/officina-host/tests/authority.rs`, `src/relay/doctor.rs`. CLI: `crates/officina/tests/run.rs`.
- Playwright: `app/e2e/relay-view.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the prompt's first-character rule applies after leading whitespace (F34); `auto` on an HTTP action is an issue, 2.11 named as a seam (F24)
