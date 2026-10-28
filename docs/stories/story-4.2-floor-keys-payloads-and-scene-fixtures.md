# Story 4.2: Floor keys, board payloads, and the scene fixtures

**Status:** Review

## Story

As the author whose offices must follow my floor file and whose important projects must raise alerts,
I want the floor file to carry each project's priority and the escalation threshold, the board to carry the floor's projects in file order and each answer's tail, and the e2e suite to hold real scene fixtures with floor files,
so that the scene places offices, fills the file and knows what is important from the payloads alone, and every scene story is tested on boards the real CLI produced.

## Acceptance Criteria

1. Priority (FR-10, AD-168, D-466): `[[project]] priority` accepts `"normal"` and `"high"`, default normal; core enum `Priority`; `ProjectRule` and `ProjectView` carry it through the one constructor `project_for` and `floor_projects` share (D-99), so a card's priority equals its office's; an unknown value is a floor issue with its line and the project stays normal. Proof: core `crates/officina-core/src/floor.rs::tests::{priority_defaults_to_normal, priority_high_is_read, an_unknown_priority_is_an_issue_and_stays_normal}`; `board.rs::tests::a_card_carries_its_project_priority`.
2. Threshold (FR-10, AD-168, D-466): `[alert] after_minutes`, an integer from 1 to 1440, default 10; out of range or not an integer is a floor issue with its line and the default stays; `Board.alert_after_ms` carries the value in ms. Proof: `floor.rs::tests::{alert_threshold_defaults_to_ten_minutes, alert_threshold_out_of_range_is_an_issue}`; `board.rs::tests::the_board_carries_the_alert_threshold`.
3. Both keys are view keys (D-466): they are not part of the relay's canonical catalogue, so changing them voids no approval. Proof: core `crates/officina-core/src/relay/catalogue.rs::tests::priority_and_alert_keys_do_not_change_the_canonical_hash`.
4. Doctor (AD-168): an invalid `priority` or `after_minutes` prints a floor line naming the key and the line number, as other floor issues do. Proof: CLI `crates/officina/tests/doctor.rs::doctor_reports_a_bad_priority_and_threshold`.
5. Plum (AD-67 as amended, D-466): `ProjectColor` gains `Plum`, in `ALL`, with `--color-project-plum` in `app.css` (light value from the prototype v2) and the TypeScript `Record<ProjectColor, string>` updated, so a missing mapping fails `pnpm check`. Proof: `floor.rs::tests::plum_is_a_project_colour`; `pnpm check`; Vitest `app/src/tokens.test.ts > every project colour has a token`.
6. `Board.floor_projects: Vec<ProjectView>` (AD-68, D-99): the loaded floor's projects in file order, priority included, empty when no floor is loaded or the floor was ignored; a repeated name is passed through. Proof: core `board.rs::tests::{floor_projects_keep_file_order, floor_project_equals_the_card_project_of_a_cwd_inside, floor_projects_are_empty_without_projects}`; CLI `crates/officina/tests/board.rs::{floor_projects_are_printed_in_file_order, an_ignored_floor_prints_no_floor_projects}`.
7. `Card.last_message_tail: Option<String>` (D-182): a `Stop` line carrying `last_message` sets the tail to that line's `last_message_tail`, absent included; `no_excerpts` withholds it like the head. Proof: core `board.rs::tests::{cards_carry_the_answer_tail_and_withhold_it_without_excerpts, a_short_answer_clears_an_older_tail}`; CLI `crates/officina/tests/board.rs::cards_print_the_answer_tail`.
8. `BOARD_SCHEMA` goes up by one relative to `main` at merge, once for this story and stories 4.3 and 4.12 (P-22 as amended by D-461): this story bumps it, and 4.3 and 4.12 add fields under the same number if they merge before v0.4 ships; TypeScript bindings regenerated; `crates/officina/tests/board.rs::board_classifies_the_fake_sessions_and_joins_hook_events` compares `schema` with the constant. Proof: `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; that CLI test.
9. Scene fixtures (AD-70, D-101 as amended, D-181): a scenario of `app/scripts/e2e-fixtures.mjs` may carry `floor` (projects with name, optional colour and priority, optional `alert_after_minutes`), written as `floor.toml` (schema 1, placeholder paths) in the temporary Officina home and passed through `OFFICINA_FLOOR`; the script gains `scene-a` (1 project, 2 sessions), `scene-b` (4 projects, 10 sessions, one at reception after a `startup` start, one in the lounge after a `compact` start and no prompt since), `scene-c` (4 projects, 12 sessions, 5 Needs you, one pending relay approval), `scene-alert` (a high-priority project with one Needs you, one Needs you whose blocking notification is older than the scenario's threshold, and one pending relay approval of a Working session on a normal project, which queues and never escalates by time), `scene-overflow` (10 floor projects, more waiting than the queue holds, one office with five live sessions, live sessions in the eighth to tenth projects); every scenario helper carries the column and, from story 4.3 on, the zone it is written to produce, and the script compares the board with those intents and exits non-zero on a difference; waits are at least 3 minutes old at generation except where a scenario tests time escalation; the script writes `app/e2e/.fixtures/index.json` (each output file and its kind) and `app/e2e/fixture-index.ts` loads files by kind through it, failing and naming `pnpm e2e:fixtures` when it is missing or lists no board. Proof: CI step "e2e fixtures from the real CLI"; Vitest `app/e2e/fixture-index.test.ts > a missing index fails and names pnpm e2e:fixtures`, `> an index without a board fails`, `> files are returned by kind`.
10. Documentation (D-121): `CHANGELOG.md` `[Unreleased]`: Added, the floor keys `priority` and `[alert] after_minutes` and the colour `plum`; Changed, `board --json` lists the floor's projects in file order and carries each answer's tail and the alert threshold (board schema bump). The README's floor-file section documents both keys. Proof: review; `crates/officina/tests/readme.rs` stays green.

## Tasks

- [x] `crates/officina-core/src/floor.rs`: `Priority`, `priority`, `[alert] after_minutes`, `ProjectColor::Plum`, issues with lines (AC 1, 2, 5)
- [x] `relay/catalogue.rs`: confirm the canonical form ignores both keys, with its test (AC 3)
- [x] `crates/officina-host/src/doctor.rs`: floor lines for both keys (AC 4)
- [x] `crates/officina-core/src/board.rs`: one `ProjectView` constructor, `floor_projects`, `alert_after_ms`, `last_message_tail`, `BOARD_SCHEMA` (AC 6-8)
- [x] Regenerate `app/src/lib/generated/` with the command of `.claude/rules/officina-rust.md`; `app.css` plum token; colour record (AC 5, 8)
- [x] `app/scripts/e2e-fixtures.mjs`: `floor` support, the five scenarios with their intents, the self-checks, `index.json`; `app/e2e/fixture-index.ts` and its test; `app/vitest.config.ts` include (AC 9)
- [x] Update `CHANGELOG.md` and the README floor section (AC 10)

## Dev Notes

- Requirements: FR-1 (floor-file order), FR-5 (the answer's tail), FR-10 (priority, threshold). Decisions: AD-13 and AD-54 as amended by AD-68, AD-67 (plum), AD-68, AD-168, AD-70 (fixtures row). Epic decisions: P-22 as amended, D-99, D-101, D-181, D-182, D-461, D-466.
- What left this story with the rework (D-461): `RegistryView.archived_count` and the quota reset flags had only the scene's cabinet and dials as consumers, which left the scene (D-472); they are not built.
- `BoardInput.projects` already holds the floor's `ProjectRule`s in file order; an ignored floor leaves it empty; a repeated name is passed through and story 4.6 keeps the first.
- Fold the tail with the head in `group_events`, from the same `Stop` line, so the pair always comes from one answer.
- The threshold travels in the board (AD-5): no TypeScript constant repeats 10 minutes. The time escalation itself is story 4.3's.
- Port the scenarios from the prototype's `fixtures.mjs` and the prototype v2's scenarios (`busy`, `alert`), as shapes, not files; each helper carries its intent, which is the self-check's oracle, never a JavaScript classifier (AD-13). `scene-c`'s pending approval goes through the v0.2 approval step the script already has (D-25). Story 4.3 adds the zone to each intent once `Card.scene` exists.
- `scene-measure` and `--home` (D-120) belong to story 4.14.
- Dependencies: none among Epic 4 stories; story 4.1's gates do not hold this story. Story 4.3 adds the zone intents; stories 4.5 to 4.10 read these scenarios.
- Keep the existing scenarios unchanged so `board.spec.ts` keeps passing.
- No employer content: placeholder project names and paths only.

## Testing

- Core unit: `floor.rs`, `board.rs`, `relay/catalogue.rs`.
- CLI integration: `crates/officina/tests/{board,doctor}.rs`.
- Vitest: `app/e2e/fixture-index.test.ts`, `app/src/tokens.test.ts`.
- CI: the TypeScript bindings drift check, "e2e fixtures from the real CLI".

## Dev Agent Record

### Debug Log

- `[[project]] priority` and `[alert] after_minutes` are kept as spanned raw values (`toml::Spanned<toml::Value>`) in `ProjectEntry` and `AlertTable`, so a bad value is a view issue with its line instead of a refused floor; `FloorFile::priority_of`, `alert_after_minutes` and `view_issues` read them. `VIEW_KEYS` names them beside `RELAY_KEYS`, and the type test now asserts every view key is a field and the relay keys are exactly the rest of the schema 2 additions. `[alert]` refuses an unknown key, as the floor's other tables do.
- `ProjectRule::view` is D-99's one constructor: `project_for` and `Board::floor_projects` both call it, so colour and priority always agree.
- `ProjectColor::ALL` gains `Plum`, so `derived_color` now spreads over four colours: a project without a declared colour may change colour once. Recorded in the changelog.
- `BOARD_SCHEMA` goes from 3 to 4; stories 4.3 and 4.12 add fields under it.
- The fixture script's scene scenarios are `scene-a`, `scene-b`, `scene-c`, `scene-alert`, `scene-alert-early` (the same at one minute short of the threshold, for story 4.5's two boards either side of it), `scene-eight` (D-519's eight projects, for story 4.7's 1920 x 1080 case) and `scene-overflow`. A project whose session makes a relay request uses a real temporary folder, since `officina run` starts there; the request goes through the real `officina run --wait 1` after `approve_fixture`, as the `approvals` scenario does. The self-check compares each session's column with its helper's intent, the live card count, the floor's project order and the pending requests.
- Vitest replaces imported CSS, `?raw` included, by an empty module, and jsdom's module URLs are not file URLs, so `tokens.test.ts` reads `app.css` from the app folder with `node:fs`. Playwright now matches `*.spec.ts` only, so the Vitest `*.test.ts` files under `e2e/` are not run as specs.
- Adversarial pass: a doctor floor line could hide a view issue behind the `no_excerpts` warning, which returned first; both now share the line (`doctor.rs::tests::a_no_excerpts_floor_the_hook_cannot_see_warns`). Covered by the tests above: schema 1 and 2, every `Priority`, wrong types (`3`, `true`, `"HIGH"`), float, negative, string and out-of-range thresholds, an empty `[alert]`, an unknown key in it, a repeated project name, an ignored floor, a short answer after a long one, a `Stop` without an answer.

### Completion Notes

- AC 1: `floor.rs::tests::{priority_defaults_to_normal, priority_high_is_read, an_unknown_priority_is_an_issue_and_stays_normal}`, `board.rs::tests::a_card_carries_its_project_priority`.
- AC 2: `floor.rs::tests::{alert_threshold_defaults_to_ten_minutes, alert_threshold_out_of_range_is_an_issue}`, `board.rs::tests::the_board_carries_the_alert_threshold`.
- AC 3: `relay/catalogue.rs::tests::priority_and_alert_keys_do_not_change_the_canonical_hash`.
- AC 4: `crates/officina/tests/doctor.rs::doctor_reports_a_bad_priority_and_threshold`.
- AC 5: `floor.rs::tests::plum_is_a_project_colour`; `pnpm check` (the exhaustive `PROJECT_COLORS`); `app/src/tokens.test.ts > every project colour has a token`.
- AC 6: `board.rs::tests::{floor_projects_keep_file_order, floor_project_equals_the_card_project_of_a_cwd_inside, floor_projects_are_empty_without_projects}`; `crates/officina/tests/board.rs::{floor_projects_are_printed_in_file_order, an_ignored_floor_prints_no_floor_projects}`.
- AC 7: `board.rs::tests::{cards_carry_the_answer_tail_and_withhold_it_without_excerpts, a_short_answer_clears_an_older_tail}`; `crates/officina/tests/board.rs::cards_print_the_answer_tail`.
- AC 8: `typescript_bindings.rs::committed_bindings_match_the_rust_types`; `board.rs::board_classifies_the_fake_sessions_and_joins_hook_events` compares `schema` with the constant.
- AC 9: CI step "e2e fixtures from the real CLI" (the intents' self-check, now before the frontend checks); `app/e2e/fixture-index.test.ts > {a missing index fails and names pnpm e2e:fixtures, an index without a board fails, files are returned by kind}`. Zone intents arrive with story 4.3.
- AC 10: `CHANGELOG.md` Added and Changed; README floor section; `crates/officina/tests/readme.rs` green.

## File List

- `crates/officina-core/src/floor.rs`, `board.rs`, `relay/catalogue.rs`, `relay/mod.rs`
- `crates/officina-host/src/collector.rs`, `doctor.rs`, `focus.rs`, `tests/registry_ingest.rs`, `tests/registry_write.rs`
- `crates/officina/tests/board.rs`, `crates/officina/tests/doctor.rs`
- `app/src/lib/generated/` (Board, Card, ProjectColor, ProjectView, Priority), `app/src/lib/colors.ts`, `app/src/app.css`, `app/src/tokens.test.ts`, `app/src/lib/board.test.ts`, `app/src/components/components.test.ts`
- `app/scripts/e2e-fixtures.mjs`, `app/e2e/fixture-index.ts`, `app/e2e/fixture-index.test.ts`, `app/e2e/fixtures.ts`, `app/vitest.config.ts`, `app/playwright.config.ts`
- `.github/workflows/ci.yml`, `README.md`, `CHANGELOG.md`

## Change Log

- 2026-10-28 created as story 4.1
- 2026-10-28 reworked after review (F2, F4, F5, F11, F24)
- 2026-10-28 renumbered 4.2 and reworked for the scene v2 (D-460, D-461): floor keys `priority` and `[alert] after_minutes`, `plum`, `alert_after_ms`; scenarios `scene-alert` and approvals; archived count and reset flags withdrawn
- 2026-10-28 after the plan challenge (D-495, D-496, D-498): `scene-b`'s reception and lounge sessions carry their `SessionStart` source; `scene-alert` gains an approval-only wait; story numbers per D-498
- 2026-10-28 implemented: floor keys, plum, `floor_projects`, `alert_after_ms`, `last_message_tail`, board schema 4, the scene fixtures with their intents and the fixture index; adversarial pass fixed the doctor line. Status review
