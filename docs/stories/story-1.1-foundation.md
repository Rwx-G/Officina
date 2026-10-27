# Story 1.1: Foundation - host crate, test harness, gates and CI

**Status:** Approved

## Story

As the developer of Officina,
I want the host crate, the path overrides, a fake `claude`, shared test helpers and the CI gates in place before any feature lands,
so that every later story is tested in isolation from the real home and the real `claude`, on Linux and Windows, from its first commit.

## Acceptance Criteria

1. `crates/officina-host` exists as a workspace member with the dependencies AD-1 allows; `crates/officina` and `app/src-tauri` depend on it. Proof: CI gate `cargo build --workspace` on the `windows` job and `cargo test -p officina-host` on `core-linux`.
2. `crates/officina-core/clippy.toml` disallows the method families AD-1 names, and `cargo clippy -p officina-core --all-targets -- -D warnings` is clean. Proof: CI gate; plus a one-time negative check (a temporary `std::fs::read` call in core makes clippy fail) recorded in Completion Notes, then removed.
3. `officina_host::paths` resolves Officina home, Claude config dir, floor file and `claude` override in the order of AD-7, reading the environment through an injected lookup so no test mutates the process environment. Proof: host unit tests `paths::tests::officina_home_override_wins`, `paths::tests::config_dir_from_env_else_home`, `paths::tests::floor_flag_beats_env_beats_default`, `paths::tests::home_falls_back_to_userprofile_on_windows_and_home_elsewhere`.
4. `officina-fake-claude` is a second bin of `crates/officina` that implements every behaviour AD-17 lists for it (fixture file, the four modes, `--version`, argv and `CLAUDE_CONFIG_DIR` log). Proof: CLI integration tests `crates/officina/tests/fake_claude.rs::{prints_fixture_in_ok_mode, prints_garbage_mode, exits_1_in_exit1_mode, hang_mode_outlives_a_short_deadline, answers_version, logs_argv_and_config_dir}`.
5. A shared test harness exists: a `TempHome` helper that creates temp dirs for `HOME`, `USERPROFILE`, `OFFICINA_HOME`, `CLAUDE_CONFIG_DIR`, builds an `officina` command with a cleared environment plus those variables and `OFFICINA_CLAUDE_BIN` pointing at the fake, and a `fingerprint(dir)` helper (relative path, size, content bytes, per AD-17 zero-write row). Proof: CLI integration test `crates/officina/tests/harness.rs::{child_sees_only_temp_home, fingerprint_detects_added_changed_and_removed_files}`.
6. `deny.toml` matches AD-18 and `cargo deny check` passes. Proof: CI job `audit`.
7. `.github/workflows/ci.yml` runs the three jobs of AD-18 with every step that is runnable at the end of this story; steps that depend on later stories (ts-rs regeneration, `e2e:fixtures`, Playwright, `--ignored` latency and stress) are added by the story that makes them runnable, never stubbed. Proof: the workflow is green on the first push (CI run link in Completion Notes).
8. `.claude/rules/officina-rust.md` and `.claude/rules/officina-frontend.md` exist with `paths` frontmatter, state that the authoritative gate commands are the steps of `.github/workflows/ci.yml`, and copy none of them. Proof: manual review in the PR (a rule file cannot be executed); `grep -c "cargo clippy\|pnpm lint" .claude/rules/officina-*.md` returns 0.
9. `CHANGELOG.md` exists in Keep a Changelog format with an empty `[Unreleased]` section, so later stories record user-visible changes as they land. Proof: file present; checked by review.

## Tasks

- [ ] Create `crates/officina-host` (lib), add it to the workspace members and `[workspace.dependencies]`; add `thiserror` and `windows-sys` (cfg windows, features of AD-15) and `tempfile` as dev-dependency (AC 1)
- [ ] Wire `officina-host` into `crates/officina` and `app/src-tauri`; add `anyhow` to both binaries (AC 1)
- [ ] Write `crates/officina-core/clippy.toml` with the disallowed methods of AD-1; run the negative check once and record it (AC 2)
- [ ] Implement `officina_host::paths` with an `Env` lookup trait or closure; unit tests (AC 3)
- [ ] Add `[[bin]] officina-fake-claude` at `crates/officina/src/bin/officina-fake-claude.rs`; CLI tests for each behaviour (AC 4)
- [ ] Write `crates/officina/tests/common/mod.rs` (`TempHome`, `officina_cmd`, `fake_claude_path`, `fingerprint`) and `tests/harness.rs` (AC 5)
- [ ] Write `deny.toml` per AD-18 (AC 6)
- [ ] Write `.github/workflows/ci.yml` with `core-linux`, `windows` (Rust steps, `pnpm install --frozen-lockfile`, `pnpm check`, `pnpm lint`, `pnpm test`) and `audit` (AC 7)
- [ ] Write `.claude/rules/officina-rust.md` (paths `**/*.rs`, `**/Cargo.toml`, `deny.toml`) and `.claude/rules/officina-frontend.md` (paths `app/**`), pointing at `ci.yml` and at AD-1/AD-13/AD-17/AD-20 (AC 8)
- [ ] Create `CHANGELOG.md` with `[Unreleased]` (AC 9)

## Dev Notes

- Layout and dependency limits: AD-1. Paths and overrides: AD-7. Fake claude and harness layers: AD-17. CI and `deny.toml`: AD-18. Dependency approvals: spine section 4; nothing outside that list.
- Edition 2024 makes `std::env::set_var` unsafe and the workspace denies `unsafe_code`. Tests must never set the process environment: host code takes an injected env lookup, CLI tests set env on the child `Command` after `env_clear()`.
- `officina-fake-claude` ships in `target/` but must never reach a release artefact (story 1.13 asserts the zip contents).
- The `platform::windows` exception to `unsafe_code` (AD-1) arrives with story 1.7, not here.
- `rust-toolchain.toml` pins the toolchain; CI reads it, do not duplicate the version in `ci.yml`.
- R8: on the workstation set `CARGO_TARGET_DIR` outside the SynologyDrive tree before the first full build.
- The two rule files name where commands live; `derived-not-transcribed.md` forbids copying the command list.

## Testing

- Host unit tests in `crates/officina-host/src/paths.rs`.
- CLI integration tests in `crates/officina/tests/fake_claude.rs` and `tests/harness.rs`.
- CI gates: `ci.yml` jobs `core-linux`, `windows`, `audit`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
