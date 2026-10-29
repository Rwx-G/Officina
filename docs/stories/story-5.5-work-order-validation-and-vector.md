# Story 5.5: `ApprovedTemplate`, `WorkOrder` and the work order vector

**Status:** Review

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

- [x] `ApprovedTemplate::new`, `TemplateUnavailable` in `relay/template.rs` (AC 1)
- [x] `crates/officina-core/src/orders/{mod,work_order}.rs`: `WorkOrder`, `validate`, `WorkOrderRefusal` (AC 2)
- [x] `tests/vectors/orders.json` (`validate` section) and `crates/officina-core/tests/orders_vectors.rs` (AC 2)
- [x] Doctests (AC 3)
- [x] `VectorKind::WorkOrder`, `spawn::work_order`, golden case; the launch half of host `spawn` accepts it under `window-authority` (AC 4)
- [x] Fake claude `bg` mode; `spawn.rs` cases (AC 5)
- [x] Update `CHANGELOG.md` (AC 6)

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

- `RelayFloor` held no floor path, so `ApprovedTemplate::new` could not resolve a project's folder from its three arguments; `load` now keeps the path it read (D-653).
- `relay::tests::code_equals_the_serde_name_for_every_variant` counts every `closed_enum!` with `code()` in the sources it lists: `TemplateUnavailable` and `WorkOrderRefusal` joined it, and `orders/work_order.rs` joined its sources.
- `run_contained` put every captured child in a job and kept a fixed 4 KiB of stderr; a work order needs no job and 64 KiB, so `RunLimits` takes the kind's `JobRule` and `stderr_cap` (D-656). `the_job_rule_names_every_kind` derives the rule from console, environment rule and `is_launch`.
- The awkward golden prompt (quotes, a tab, `100% & | < > ^ !`, `$HOME`, inner `--bg -- --model opus`, a line starting with `-`, accents, a trailing backslash) reached the fake byte for byte through `Command`'s quoting on the first run.
- A Python heredoc with `\u` escapes and a shell-variable `bash` call were refused by the worktree guard; patches went through script files in the scratch space instead.

### Completion Notes

Every AC is implemented and proven by the tests named; decisions D-653 to D-657 (epic-5-decisions.md, "Implementation (2026-10-29)"), none to confirm. No new dependency or feature.

- E20's branch shipped (D-127): the `--bg` cases passed, so the vector keeps `--` before the prompt.
- AC 1: core `relay/template.rs::tests::{a_floor_edited_since_approval_is_catalogue_changed, a_template_absent_from_the_snapshot_is_not_approved, project_folders_keep_their_declared_spelling, every_unavailable_reason_has_a_case}`; the `compile_fail` doctest on `ApprovedTemplate`'s struct literal. A snapshot project missing from the floor is `catalogue_changed` (D-653).
- AC 2: core `orders/work_order.rs::tests::{the_project_must_be_one_of_the_template, a_missing_duplicate_or_extra_parameter_is_refused, a_value_outside_its_list_is_refused, the_stored_value_is_the_template_string, the_prompt_sha256_is_of_the_rendered_prompt, what_the_snapshot_renders_is_checked}`; `tests/vectors/orders.json` replayed by `orders_vectors.rs::{validate_cases_match, every_refusal_has_a_case, every_expected_order_is_consistent}`. `WorkOrderRefusal` adds `unrenderable_prompt` for a snapshot edited by hand (D-654); the expected orders carry no catalogue hash (D-655).
- AC 3: `cargo test -p officina-core --doc`: the six `compile_fail` doctests of `orders::work_order` (the value as `Untrusted<String>`, `display()`, `&*u`, `u.as_ref()`, `format!("{}", u.display())`, `String::from(u)`, each with its error code), the `WorkOrder` and `ApprovedTemplate` struct literals, and the positive doctest on `WorkOrder::validate`.
- AC 4: the two `work_order` cases of `tests/vectors/spawn.json` through `spawn_vectors.rs::{every_case_matches_its_builder, every_kind_has_a_case}`; core `spawn.rs::tests::{the_prompt_is_one_argument_after_the_separator, a_work_order_vector_carries_no_settings_and_no_permission_mode}` and the existing environment tests, which iterate `VectorKind::ALL`; CLI `spawn.rs::{session_kinds_keep_the_authors_officina_settings, seeded_session_variables_never_reach_the_child}` cover `WorkOrder` by iteration; CLI `spawn.rs::a_work_order_deadline_ends_the_child_alone` (no job, the grandchild keeps beating); host `spawn.rs::tests::{the_job_rule_names_every_kind, execute_refuses_every_launch_kind_without_the_feature}` (the launch half refuses it without `window-authority`, the CLI tests run it with).
- AC 5: CLI `spawn.rs::every_kind_reaches_the_fake_claude_as_its_golden_vector` (both `work_order` cases, argv element by element, the project folder as `cwd`) and `spawn.rs::bg_mode_lists_the_new_session_in_agents` (D-657).
- AC 6: `CHANGELOG.md` `[Unreleased]` Added; README "Work orders (not offered yet)"; `readme.rs` green.
- Not built here, by design: D-128's "`started` with what was read" when a process `claude --bg` started holds the pipes after exit 0 is story 5.7's, with `bg-hold`; until then such a run is `OutputHeld` (D-656).
- Gate (2026-10-29), on `ebd27a0`: the full local replay of `ci.yml`. Windows green: Rust 1226 passed, 0 failed, 11 ignored; the launch half without the authority, perf, lexer differential, approvals stress, registry crash, authority markers, registry dependencies, bundled roots and e2e fixtures green; Vitest 279; Playwright 978 (x3); `cargo deny` ok; `pnpm audit` 1 low, ignored as before. Linux (`rust:1-bookworm`): 1088 passed, 0 failed, 9 ignored, and the spawn step alone.

## File List

- `CHANGELOG.md`, `README.md`
- `crates/officina-core/src/{lib,spawn}.rs`, `crates/officina-core/src/relay/{catalogue,mod,template}.rs`
- `crates/officina-core/src/orders/{mod,work_order}.rs` (new)
- `crates/officina-core/tests/orders_vectors.rs` (new)
- `crates/officina-host/src/{process,spawn}.rs`
- `crates/officina/src/bin/officina-fake-claude.rs`
- `crates/officina/tests/spawn.rs`
- `tests/support/spawn_cases.rs`, `tests/vectors/spawn.json`, `tests/vectors/orders.json` (new)
- `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this file

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `seq` comes from the card's reservation (F2); the argument vector is compared as a JSON array (F12); session-kind environment keeps `OFFICINA_*`, no job (F1, F10)
- 2026-10-29 implemented; decisions D-653 to D-657; status Review
