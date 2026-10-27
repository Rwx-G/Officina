# Story 2.1: Relay core: floor schema 2, two-phase parse, values and argument vectors

**Status:** Approved

## Story

As the author declaring which actions a guarded repository may run,
I want the floor file to carry projects' autonomy and a catalogue of closed actions, parsed so that a broken action never breaks the board, and turned into argument vectors that no parameter value can bend,
so that every later relay story builds on one validated, pure model of what may run.

## Acceptance Criteria

1. A schema 2 floor with the shape of AD-22 (redacted fixture) parses: `guarded_programs`, `[[project]]` with `autonomy` and `actions`, `[[action]]` with `runner`, `distro`, `program`, `env`, `args`, `params`, `class`, `check`, `timeout_s`, `max_output_kib` (FR-1, FR-2, PRD amendments 2 and 3). A schema 2 project without `autonomy` is `guarded`; the defaults (`native`, 540 s, 64 KiB) are explicit after validation. Proof: core unit tests `crates/officina-core/src/floor.rs::tests::{schema_2_example_parses, schema_2_project_defaults_to_guarded}` and `crates/officina-core/src/relay/catalogue.rs::tests::defaults_are_explicit_after_validation`.
2. A schema 1 floor still loads with every project `free`; a schema 1 floor carrying any relay key is an error whose message names `schema = 2`; any other schema number is refused (AD-22). The relay keys are one constant asserted against the floor type (D-7). Proof: `floor.rs::tests::{schema_1_projects_are_free, schema_1_with_each_relay_key_names_schema_2, unsupported_schema_is_refused, relay_keys_constant_matches_the_type}`; the second iterates the constant.
3. Phase 1 parses projects strictly and actions as raw spanned values: a malformed project still invalidates the floor (v0.1 behaviour), a malformed action never does (AD-23, FR-4 last sentence). Proof: `catalogue.rs::tests::{malformed_project_invalidates_the_floor, malformed_action_never_invalidates_the_floor}` and CLI test `crates/officina/tests/board.rs::invalid_action_leaves_the_board_floor_valid` (`diagnostics.floor_error` stays null).
4. Phase 2 converts each action alone with `deny_unknown_fields` and validates every rule of AD-23, plus D-1 (`env` keys), D-2 (braces), D-3 (`guarded_programs` entries), D-4 (`distro`) and D-5 (`.bat` or `.cmd` native program); each failure is an `ActionIssue { name, line, message }` and the action is dropped. Each rule is a variant of one issue-kind enum. Proof: `catalogue.rs::tests::every_issue_kind_has_a_case`, which iterates the enum's `ALL` constant and fails when a variant has no failing fixture, plus one named test per variant.
5. **Gate (AD-23):** `toml::Spanned` inside an array of tables gives the action's line with toml 0.9. Proof: `catalogue.rs::tests::issue_line_points_at_the_action` (three actions, the second invalid, reports that action's line). If `Spanned` does not hold, the fallback of AD-23 (locate by `name`) ships instead and the result is recorded in `docs/research-notes.md`.
6. A project naming an unknown or dropped action gets an issue and keeps its other actions (AD-23). Proof: `catalogue.rs::tests::project_keeps_valid_actions_when_one_is_dropped`.
7. Value rules of AD-30 (length per P-2, leading `-`, control characters, the bidi and invisible formatting ranges, `.` and `..` segments on both separators), plus a leading `/` or `@` on the `native` runner (D-5), then `values` membership or the anchored `pattern`; every `values` entry passes the same rules at validation, for the runner of its action. Proof: `crates/officina-core/src/relay/params.rs::tests::*`, the refusal cases of AC 9, and `params.rs::tests::every_forbidden_range_bound_is_refused` (iterates the range constant, both bounds of each range and one code point outside it).
8. A `pattern` is at most 256 characters, compiled as `^(?:...)$` with a 1 MiB size limit; a pattern over either limit or failing to compile is an issue (AD-23). Proof: `params.rs::tests::{pattern_is_anchored, oversized_pattern_is_an_issue, invalid_pattern_is_an_issue}`.
9. `argv::build` turns a validated action and its parameters into a launch (program, arguments, working directory, environment changes) for both runners of AD-30: `native` keeps `program` and adds the declared `env`; `wsl` uses `<system dir>\wsl.exe` (the system directory is an argument, core stays pure) with `[-d <distro>] --cd <project folder> --exec /usr/bin/env K=V ... /usr/bin/timeout -k 10 <timeout_s + 5> <program> <args...>`, and removes `WSLENV` from the child's environment (D-6). An `args` item holds at most one placeholder with an optional literal prefix and suffix and always yields exactly one argument. Proof: vector set `tests/vectors/argv.json` (action, params, expected launch or refusal) replayed by `crates/officina-core/tests/argv_vectors.rs::every_case_matches`.
10. The vector set cannot age: `argv_vectors.rs::every_rule_and_runner_has_a_case` fails when a runner variant, a refusal kind of the value rules, or a character class of the E13 constant (the classes E13 verified through `--exec`) appears in no case.
11. `regex` and `sha2` are added to `officina-core` only, with licences accepted by `deny.toml` (addendum section 2); core stays pure. Proof: CI `audit` job (`cargo deny check`) and the `clippy` steps of both CI jobs with `crates/officina-core/clippy.toml`.
12. The board does not change for a floor rewritten as schema 2: projects, colours and `config_dir` map as before. Proof: `board.rs::schema_2_floor_maps_projects_like_schema_1` and the unchanged classification vector replay.

## Tasks

- [ ] Add `regex` and `sha2` to `crates/officina-core/Cargo.toml`; run `cargo deny check` (AC 11)
- [ ] `floor.rs`: accept schema 1 and 2, project `autonomy` and `actions`, top-level `guarded_programs`, `action` as `Vec<toml::Spanned<toml::Value>>`; relay keys constant and schema 1 error (AC 1, 2, 3)
- [ ] Default guarded program list as one constant in `relay::catalogue` (its content is settled by story 2.3, D-11) (AC 1)
- [ ] Module `crates/officina-core/src/relay/` with `mod.rs`, `catalogue.rs` (phase 2, issue kinds with `ALL`), `params.rs`, `argv.rs` (AC 3-10)
- [ ] Prove or replace `Spanned` line location; record the result in research notes if the fallback is used (AC 5)
- [ ] Write `tests/vectors/argv.json` by hand (redacted) and `crates/officina-core/tests/argv_vectors.rs` with the coverage test (AC 9, 10)
- [ ] CLI tests in `board.rs` (AC 3, 12)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README's floor example for schema 2 (D-34)

## Dev Notes

- Requirements: FR-1, FR-2, FR-3 as amended (addendum section 3, amendments 2 to 4; amendment 4's WSL charset is lifted by E13). Decisions: AD-21 (module layout), AD-22, AD-23, AD-30. Facts: E13. Epic decisions (`docs/stories/epic-2-decisions.md`): P-2, D-1 to D-7.
- `program` is absolute for its runner: a Windows absolute path for `native`, a path starting with `/` for `wsl` (AD-22). `floor::is_absolute` accepts both forms today, so the runner check needs its own rule.
- D-5 makes the value rules runner-aware: the same function takes the runner, and validation of `values` entries calls it with the action's runner. Never fork the function.
- D-6: `--cd` receives the resolved project path as declared in the floor, not the lowercased form used for comparison. `wsl --cd` with an accented or mixed-case path is part of the workstation run of story 2.10.
- D-7: serde exposes struct field names only through a capturing deserializer, so the relay keys are a constant, and the test derives the expected set from a fully populated schema 2 floor serialized to a value.
- Name the E13 character classes as one constant so the coverage test iterates it; `argv.json` carries one `wsl` case per class.
- `tests/vectors/` is exempt from every formatter (derived-not-transcribed rule). No formatter touches it today; keep it that way. Write control and bidi characters as JSON `\u` escapes.
- Phase 1 must stay cheap: the gate (story 2.4) runs it on every `Bash` call and never compiles a regex (AD-23, AD-28). Keep regex compilation in phase 2 only.
- No employer content: fixtures use placeholder projects, groups like `lab-web`, paths under `C:/path/to/` and `/home/user01/`.

## Testing

- Core unit: `crates/officina-core/src/floor.rs`, `src/relay/{catalogue,params,argv}.rs`.
- Core integration: `crates/officina-core/tests/argv_vectors.rs` replaying `tests/vectors/argv.json`.
- CLI integration: `crates/officina/tests/board.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: decisions D-1 to D-7 and P-2 (native `/` and `@`, `.bat`/`.cmd`, absolute WSL tools, inner timeout margin, `WSLENV`, relay keys constant)
