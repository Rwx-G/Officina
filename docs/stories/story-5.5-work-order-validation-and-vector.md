# Story 5.5: `ApprovedTemplate`, `WorkOrder` and the work order vector

**Status:** Approved

## Story

As the author whose principle is that no model text launches anything,
I want a work order to exist only as the output of one validating constructor that accepts values from my approved template's closed lists, and its argument vector to come only from that order,
so that whatever reaches `claude --bg` is my template's text and my lists' values, provably, before any button exists.

## Acceptance Criteria

1. **`ApprovedTemplate`** (AD-77). `ApprovedTemplate::new(approved, current, name) -> Result<ApprovedTemplate, TemplateUnavailable>` is the only constructor: it requires the current floor's hash to equal the approved one (else `catalogue_changed`), reads the template from the approved snapshot (absent: `not_approved`), and takes each allowed project's folder from the current floor in its declared spelling resolved against the floor (D-6), never the lowercased comparison form; fields are private. `TemplateUnavailable` is a `closed_enum!`. Proof: core `crates/officina-core/src/relay/template.rs::tests::{a_floor_edited_since_approval_is_catalogue_changed, a_template_absent_from_the_snapshot_is_not_approved, project_folders_keep_their_declared_spelling, every_unavailable_reason_has_a_case}` (the last iterates `TemplateUnavailable::ALL`).
2. **`WorkOrder::validate`** (AD-77, FR-3). `officina-core::orders::work_order::WorkOrder::validate(&ApprovedTemplate, &ProjectName, &[(ParamName, &str)]) -> Result<WorkOrder, WorkOrderRefusal>` is the only constructor. In order: the project is one of the template's; every declared parameter is present once and none is extra; each value equals an entry of its list byte for byte, and the order stores the template's own string; the rendered prompt is at most 8,000 characters and does not start with `/`, `!` or `-`. `WorkOrder` holds, private with getters: template, project, project folder, values, rendered prompt, its SHA-256, model, catalogue hash. `WorkOrderRefusal` is a `closed_enum!`. Proof: core `crates/officina-core/src/orders/work_order.rs::tests::{the_project_must_be_one_of_the_template, a_missing_duplicate_or_extra_parameter_is_refused, a_value_outside_its_list_is_refused, the_stored_value_is_the_template_string, the_prompt_sha256_is_of_the_rendered_prompt}`; the `validate` section of `tests/vectors/orders.json` replayed by `crates/officina-core/tests/orders_vectors.rs::{validate_cases_match, every_refusal_has_a_case}` (iterates `WorkOrderRefusal::ALL`).
3. **Type proofs** (AD-74, FR-3). `compile_fail` doctests on `WorkOrder::validate`: passing an `Untrusted<String>`, its `display()`, `&*untrusted`, `untrusted.as_ref()`, `format!("{}", untrusted.display())` or `String::from(untrusted)` as a value does not compile; neither does a struct literal of `WorkOrder` or `ApprovedTemplate` outside its module. A positive doctest shows the author's path from an approved snapshot. Proof: `cargo test -p officina-core --doc`.
4. **The `WorkOrder` vector** (AD-79, AD-80, FR-3 as amended by amendment 2, E20, D-127, D-128). `VectorKind` gains `WorkOrder`; `spawn::work_order(&WorkOrder, seq: u32)` is its only builder: arguments `--bg --name <template>-<seq> [--model <alias>]`, then `--` when E20's `--bg` cases passed, then the rendered prompt as exactly one argument; working directory the order's project folder; no console; stdin null; stdout and stderr captured at 64 KiB each; deadline 60 s, which kills the direct child only, in no job (D-128); only an `.exe` accepted; D-128's session-kind environment: `SESSION_ENV_REMOVED` and `CLAUDE_CODE_DISABLE_AGENT_VIEW` removed, the author's `OFFICINA_*` names kept, so the session's hooks find his home, floor and excerpt switch as in a session he starts (review F1); no `--settings` and no permission mode. Proof: the `WorkOrder` case of `tests/vectors/spawn.json` through `spawn_vectors.rs::{every_case_matches_its_builder, every_kind_has_a_case}`; core `crates/officina-core/src/spawn.rs::tests::{the_prompt_is_one_argument_after_the_separator, a_work_order_vector_carries_no_settings_and_no_permission_mode}`; CLI `crates/officina/tests/spawn.rs::{session_kinds_keep_the_authors_officina_settings, seeded_session_variables_never_reach_the_child}` (story 5.2) now cover `WorkOrder` through their iteration.
5. **Through the fake claude** (AD-95 spawn and fake claude rows, D-129). Fake claude mode `bg` prints a background id and appends to its agents fixture a background entry with that id, the `--name` value, its working directory as `cwd` and the current time as `startedAt`. Proof: CLI `crates/officina/tests/spawn.rs::every_kind_reaches_the_fake_claude_as_its_golden_vector` now covers `WorkOrder`, comparing the logged `argv` array element by element (D-129), so a prompt holding newlines, quotes or a leading dash shows as one argument; `spawn.rs::bg_mode_lists_the_new_session_in_agents` (a `claude agents --json` through the fake after the spawn shows the entry).
6. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added (internal until story 5.7 offers it): the validated work order and its vector. Proof: review.

## Tasks

- [ ] `ApprovedTemplate::new`, `TemplateUnavailable` in `relay/template.rs` (AC 1)
- [ ] `crates/officina-core/src/orders/{mod,work_order}.rs`: `WorkOrder`, `validate`, `WorkOrderRefusal` (AC 2)
- [ ] `tests/vectors/orders.json` (`validate` section) and `crates/officina-core/tests/orders_vectors.rs` (AC 2)
- [ ] Doctests (AC 3)
- [ ] `VectorKind::WorkOrder`, `spawn::work_order`, golden case; the launch half of host `spawn` accepts it under `window-authority` (AC 4)
- [ ] Fake claude `bg` mode; `spawn.rs` cases (AC 5)
- [ ] Update `CHANGELOG.md` (AC 6)

## Dev Notes

- Requirements: FR-3 as amended (amendment 2), FR-2's bounds. Decisions: AD-74 (doctests), AD-77, AD-79, AD-80, AD-95. Facts: E20 (gate 2), addendum section 1 items 4 and 5 (`--bg` before `--` is load-bearing: without it a spawn with a prompt and no TTY is a model turn). Epic decisions: D-6 (Epic 2); D-124, D-127 to D-129, D-135.
- Seams (D-124): the approved snapshot type and relay floor resolution of story 2.2 (D-8); `relay::template` of 5.4; `spawn` of 5.2.
- E20 decides the separator (D-127). With `--bg` cases failed, the vector has no `--`, and the prompt cannot start with `-` anyway (AD-77 refuses it): record which branch shipped in the Completion Notes.
- `seq` is reserved by the card store when a card is issued, from the registry's highest and the cards in the store (D-135, story 5.7); this story takes it as an argument, and the card shows the vector built with it, so what runs is what the author clicked (review F2).
- Core stays pure: the prompt's SHA-256 uses `sha256_hex`; the project folder is a string resolved by the floor rules, not a filesystem check (the folder's existence is a launch check of 5.7).
- No employer content in `orders.json` or `spawn.json`.

## Testing

- Core unit and doctests: `crates/officina-core/src/{relay/template,orders/work_order,spawn}.rs`.
- Core integration: `crates/officina-core/tests/{orders_vectors,spawn_vectors}.rs`.
- CLI integration: `crates/officina/tests/spawn.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `seq` comes from the card's reservation (F2); the argument vector is compared as a JSON array (F12); session-kind environment keeps `OFFICINA_*`, no job (F1, F10)
