# Story 4.1: Floor projects on the board, the archived count, and the scene fixtures

**Status:** Approved

## Story

As the author whose workshops must follow my floor file,
I want the board to carry the floor's projects in file order, each answer's tail and whether each quota window has reset, the registry view to carry the archived count, and the e2e suite to hold real scene fixtures with floor files,
so that the scene places workshops, fills the file and the cabinet, and mutes its dials from the payloads alone, and every scene story is tested on boards the real CLI produced.

## Acceptance Criteria

1. `Board.floor_projects: Vec<ProjectView>` (AD-68, AD-13 as amended, D-99): the loaded floor's projects in file order, each built by the one constructor `project_for` also uses for a matching `cwd` (declared colour, else `derived_color(name)`); empty when no floor is loaded or the floor was ignored; a repeated name is passed through. Proof: core unit tests `crates/officina-core/src/board.rs::tests::{floor_projects_keep_file_order, floor_project_equals_the_card_project_of_a_cwd_inside, floor_projects_are_empty_without_projects}`; CLI tests `crates/officina/tests/board.rs::{floor_projects_are_printed_in_file_order, an_ignored_floor_prints_no_floor_projects}`.
2. `Card.last_message_tail: Option<String>` (D-182): a `Stop` line carrying `last_message` sets the tail to that line's `last_message_tail`, absent included; `no_excerpts` withholds it like the head. Proof: core `board.rs::tests::{cards_carry_the_answer_tail_and_withhold_it_without_excerpts, a_short_answer_clears_an_older_tail}`; CLI `crates/officina/tests/board.rs::cards_print_the_answer_tail` (a long `Stop` payload through `officina hook`).
3. `QuotaView.five_hour_reset_passed` and `seven_day_reset_passed: bool` (D-183): true when that window's `resets_at` is at or before `now_ms` in `build_board`, false without a window. Proof: core `board.rs::tests::{reset_passed_follows_now_ms, a_passed_reset_changes_the_comparable_board}` (the second serializes two boards built on either side of a reset with `generated_at_ms` zeroed, as `watch::publish` compares them, and asserts the bytes differ).
4. `BOARD_SCHEMA` goes up by one relative to `main` at merge, once for AC 1 to 3 (P-22); the TypeScript bindings are regenerated; `crates/officina/tests/board.rs::board_classifies_the_fake_sessions_and_joins_hook_events` compares `schema` with `officina_core::board::BOARD_SCHEMA`, not a literal. Proof: `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; that CLI test.
5. `RegistryView.archived_count: u32` (AD-68, D-100) and `REGISTRY_VIEW_SCHEMA` up by one (P-22): the host counts with one `COUNT` whose filter is the SQL fragment `list_archived` uses, and passes it to the core builder; `officina subjects --json` prints it; `crates/officina/tests/subjects.rs` and story 3.4's view tests compare `schema` with `REGISTRY_VIEW_SCHEMA`, never a literal (review F24). Proof: host `crates/officina-host/tests/registry_archived.rs::{archived_count_equals_the_archived_list, merged_subjects_are_not_counted}` (`required-features = ["window-authority"]`, subjects archived and merged through the writer at explicit `now_ms`); core `crates/officina-core/src/registry/view.rs::tests::archived_count_reaches_the_view`; CLI `crates/officina/tests/subjects.rs::prints_the_archived_count`; `typescript_bindings.rs::committed_bindings_match_the_rust_types`.
6. Scene fixtures (AD-70 fixtures row, D-101, D-181): a scenario of `app/scripts/e2e-fixtures.mjs` may carry `floor` (project name, optional colour), written as `floor.toml` (schema 1, placeholder paths) in the temporary Officina home and passed through `OFFICINA_FLOOR`; the script gains `scene-a`, `scene-b`, `scene-c` and `scene-overflow`, every wait in them at least 3 minutes old at generation; each scenario helper carries the column it is written to produce, and after each scene scenario the script compares the board's floor projects, cards per column and waiting count with those intents and exits non-zero on a difference; the script writes `app/e2e/.fixtures/index.json` (each output file and its kind: `board`, `relay`, `registry`) from its scenario table; `app/e2e/fixture-index.ts` (directory as an argument, `.fixtures` by default) loads files by kind through the index and fails, naming `pnpm e2e:fixtures`, when the index is missing or lists no board; `app/e2e/fixtures.ts`'s `loadBoard` goes through it. Proof: CI step "e2e fixtures from the real CLI"; Vitest `app/e2e/fixture-index.test.ts > a missing index fails and names pnpm e2e:fixtures`, `> an index without a board fails`, `> files are returned by kind` (temporary directories, so the cases need no generated fixture; the Vitest include gains `e2e/*.test.ts`).
7. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` under Changed: `board --json` lists the floor's projects in file order, carries each answer's tail and whether each quota window has reset (board schema bump), and `subjects --json` prints the archived count (registry view schema bump). Proof: review.

## Tasks

- [ ] `crates/officina-core/src/board.rs`: one `ProjectView` constructor shared by `project_for` and `floor_projects`; `Board.floor_projects`; `last_message_tail` folded in `group_events` and set in `new_card`; the two reset flags in `build_board`; `BOARD_SCHEMA` (AC 1-4)
- [ ] Regenerate `app/src/lib/generated/` with the command of `.claude/rules/officina-rust.md` (AC 4, 5)
- [ ] `RegistryView.archived_count` in core; the shared `archived` filter and the `COUNT` in the host's registry read; `REGISTRY_VIEW_SCHEMA`; schema literals in tests replaced by the constant (AC 5)
- [ ] `app/scripts/e2e-fixtures.mjs`: `floor` support, the four scenarios with their intents, the self-checks, `index.json`; `app/e2e/fixture-index.ts` and its test; `app/e2e/fixtures.ts`; `app/vitest.config.ts` include (AC 6)
- [ ] Tests: `board.rs` (core and CLI), `registry_archived.rs`, `view.rs`, `subjects.rs` (AC 1-5)
- [ ] Update `CHANGELOG.md` `[Unreleased]` (AC 7)

## Dev Notes

- Requirements: FR-1 as amended (floor-file order, the cabinet's count), FR-4 (dials that mute after a reset), FR-5 (the answer's tail), PRD section 7 (fixtures for the readability check). Decisions: AD-13 and AD-54 as amended by AD-68, AD-68, AD-70 (fixtures row). Epic decisions: P-13, D-55, D-72 (Epic 3); P-20, P-22, D-98 to D-101, D-181 to D-183.
- v0.2 and v0.3 seams (D-98): story 2.8 makes `BOARD_SCHEMA` 2 and story 3.2 bumps it again; this story bumps once more, relative to `main`, for all three board fields. `EventLine.last_message_tail` comes from story 3.2, `RegistryView` from 3.4 and `list_archived` from 3.5; if one has not merged, the AC reading it waits and the rest ships. The fixture script has grown with v0.2's `approve_fixture` step (D-25), v0.2's `relay --json` outputs and v0.3's `registry_fixture` step (D-72) with its `subjects --json` outputs: the index of D-181 lists those under their kinds, from the same scenario table, so no reader parses a relay view as a board.
- Fold the tail with the head: `group_events` already takes `last_message` from `Stop` lines when present; take `last_message_tail` from the same line at the same moment, so the pair always comes from one answer. The classification vector set compares a subset of card fields, so it needs no change.
- The reset flags are computed in core at `now_ms` (core stays pure); `watch::publish` compares the board without `generated_at_ms`, so the flag's change is what emits a new board after a reset, at the next recompute (every agents poll recomputes).
- `BoardInput.projects` already holds the floor's `ProjectRule`s in file order (`collector::load_floor` maps them in order); an ignored floor leaves it empty. `floor.rs` does not reject a repeated name: pass it through; story 4.3 keeps the first (D-99).
- Port the scenarios from the prototype's `fixtures.mjs` (git-ignored, see `epic-4-decisions.md`): its helpers (`live`, `waiting`, `yourTurn`, `working`, `ended`, `done`) and its densities (a), (b), (c), not the file. Give each helper the column it means to produce (that is the self-check's oracle, never a JavaScript classifier, AD-13), and move every wait to at least 3 minutes before generation (the prototype's `scene-c` has one at 1 minute, whose halo would breathe in the motion cases).
- `scene-overflow` (D-101): ten floor projects with generic placeholder names, eleven Needs you sessions spread over them, one of the first seven projects with five live sessions, live sessions in the eighth to tenth projects. It exists for the overflow groups of D-103; stories 4.4 and 4.5 read it.
- `scene-measure` and `--home` (D-120) belong to story 4.9.
- Keep the existing scenarios unchanged so `board.spec.ts` keeps passing.
- No employer content: placeholder project names and paths only (officina-project rule).

## Testing

- Core unit: `crates/officina-core/src/board.rs`, `src/registry/view.rs`.
- Host integration: `crates/officina-host/tests/registry_archived.rs`.
- CLI integration: `crates/officina/tests/{board,subjects}.rs`.
- Vitest: `app/e2e/fixture-index.test.ts`.
- CI: the TypeScript bindings drift check, "e2e fixtures from the real CLI".

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `Card.last_message_tail` (F4, D-182) and the quota reset flags (F11, D-183) under the same bump (P-22); fixture index and shared loader (F2, D-181); helper intents as the self-check's oracle and registry schema compared with its constant (F24); scene waits at least 3 minutes old (F5); one SQL filter for the archived list and count
