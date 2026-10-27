# Story 1.10: Floor file

**Status:** Approved

## Story

As the author working across several repositories and possibly a second Claude account,
I want an optional `officina.toml` that names my projects, gives them colours and selects the config dir,
so that cards carry the right project and the board reads the right account.

## Acceptance Criteria

1. The floor file is found through `--floor`, then `OFFICINA_FLOOR`, then `<officina home>/floor.toml` (FR-9, AD-7), for `board --json`, `doctor` and `officina-app`. Proof: CLI tests `crates/officina/tests/floor.rs::{flag_beats_env, env_beats_default, default_location_is_used, no_floor_derives_projects_from_cwd}`.
2. A session whose `cwd` lies under a declared project path (relative to the floor file, normalized, case-insensitive on Windows, longest prefix wins) gets that project; a session outside every project goes to "other" and is never hidden (FR-9). Proof: core unit tests `floor::tests::{maps_cwd_under_project, nested_projects_longest_prefix_wins, case_and_separator_insensitive, outside_goes_to_other}`.
3. Each card carries its project colour token; the UI maps every `ProjectColor` to a CSS variable. Proof: Playwright `app/e2e/floor.spec.ts > every project colour token renders its chip colour`, iterating the generated `ProjectColor` union and reading the expected value from `:root`.
4. The floor `config_dir` reaches every place AD-6, AD-7 and AD-15 name: the `claude agents` child, the attach child, the watched sessions directory and the quota account key. Proof: CLI test `floor.rs::config_dir_reaches_agents_child` (fake claude log), host tests `attach.rs::floor_config_dir_reaches_attach_child`, `collector.rs::watches_sessions_dir_and_quota_of_floor_config_dir`.
5. A relative `config_dir` is invalid (E7). Proof: CLI test `doctor.rs::relative_config_dir_is_red`.
6. Doctor validates the floor file: unknown keys and unknown colour tokens reported with line numbers, wrong schema reported; floor validity is a required check when a floor file is found. Proof: CLI tests `doctor.rs::{valid_floor_is_green, unknown_key_reports_line, unknown_color_reports_line, wrong_schema_is_red}`.
7. With an invalid floor file, `board --json` still prints a board using cwd-derived projects and reports the floor error in `diagnostics`; its exit code is unchanged. Proof: CLI test `floor.rs::invalid_floor_degrades_to_cwd_projects_with_diagnostic`.
8. The header shows the floor name when a floor is loaded. Proof: Playwright `floor.spec.ts > header shows floor name`.

## Tasks

- [ ] `officina_host::floor::load` (resolution from story 1.1 `paths`, read, parse with core) (AC 1, 7)
- [ ] `--floor` flag on `board`, `doctor` and the app (AC 1)
- [ ] Project mapping in core `build_board` (AC 2)
- [ ] Propagate `config_dir` into the collector, attach and quota key (AC 4)
- [ ] Colour map and header floor name; fixtures with a floor for `e2e:fixtures` (AC 3, 8)
- [ ] Doctor floor checks (AC 5, 6)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Requirements: FR-9, glossary "Floor". Decisions: AD-6, AD-7, AD-8 (floor format, `config_dir` is a floor key), AD-15 (attach env). Facts: E7.
- No employer content: example floors in tests and README use placeholder project names and `C:/path/to/...` paths (officina-project rule).
- AC 7 is a decision this story takes because neither the PRD nor the spine says what `board --json` does with an invalid floor; it is listed as an open point for the owner.
- `officina-app` with single instance (story 1.12): a second launch with a different `--floor` focuses the existing window and does not switch floors; say so in the README.

## Testing

- Core unit: `crates/officina-core/src/floor.rs`.
- CLI integration: `crates/officina/tests/{floor,doctor}.rs`.
- Host integration: `crates/officina-host/tests/{attach,collector}.rs`.
- Playwright: `app/e2e/floor.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
