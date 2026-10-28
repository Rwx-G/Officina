# Story 2.6: Class after-check, tree fingerprint, `officina relay --json`

**Status:** Review

## Story

As the author delegating "check, then apply" subjects,
I want an `after-check` action to run only after its dry run succeeded with the same parameters on the same code, and a read-only view of the relay's state,
so that a session cannot apply what it did not check, and the window and the e2e suite read one relay contract.

## Acceptance Criteria

1. Core `classes::check_satisfied(lines, project, check_action, params, catalogue_hash, tree, now_ms)` is true only when a `run_started` paired with `run_finished(exited, 0)` matches all of them within 30 minutes (AD-33, P-3). Proof: unit tests `crates/officina-core/src/relay/classes.rs::tests::{matching_successful_check_satisfies, failed_check_does_not, other_params_do_not, other_project_does_not, other_catalogue_hash_does_not, other_tree_does_not, check_older_than_30_min_does_not, started_without_finished_does_not}`.
2. The journal of the previous UTC day counts: a check at 23:50 UTC satisfies an apply at 00:10 (AD-33). Proof: `classes.rs::tests::check_before_midnight_counts_after_midnight` and host test `crates/officina-host/tests/relay_journal.rs::reader_spans_two_days_and_counts_corrupt` (story 2.4).
3. Tree fingerprint (AD-33, D-22): for check actions and `after-check` actions, `run` records SHA-256 over `git rev-parse HEAD`, `git diff HEAD --binary` and the untracked files (`--exclude-standard`), with `git` run as `git --no-ext-diff --no-textconv -c core.fsmonitor=false` in the project folder; untracked files are hashed by content up to 1 MiB each and by size beyond, and the record says so; a work tree with no commit uses the "no HEAD" marker; outside a work tree, or when `git` cannot run, the field is absent. Proof: host tests `crates/officina-host/tests/relay_tree.rs::{same_tree_same_fingerprint, tracked_edit_changes_it, same_size_untracked_edit_changes_it, untracked_file_over_1_mib_is_bound_by_size, ignored_files_do_not_count, repository_without_commit_is_bound, external_diff_and_textconv_are_not_run, outside_git_it_is_absent}` (temp repositories built with `git init`; the external-diff test configures a `diff.external` that would write a marker file and asserts it is never written).
4. Delegation flow (PRD journey 2, AD-37 flows): `check` then `apply` with the same parameters runs both; `apply` is refused with "run `<check>` first" without a check, with other parameters, after a tracked edit, after a same-size untracked edit, and when the check is older than 30 minutes (seeded journal). Proof: CLI tests `crates/officina/tests/run.rs::{check_then_apply_runs, apply_without_check_is_refused, apply_with_other_params_is_refused, apply_after_a_tree_change_is_refused, apply_after_an_untracked_edit_is_refused, stale_check_is_refused}`.
5. Git in tests (D-23): the harness resolves git's directory and puts it in the child's `PATH`; with no `git` on the machine, the tests of AC 3 and AC 4 that need it skip with a printed reason. Proof: `crates/officina/tests/common/mod.rs::git_dir` used by the tests above; the skip message appears in the test output.
6. `class_not_available` no longer applies to `after-check` (D-20). Proof: `run.rs::after_check_and_human_are_not_available_yet` is reduced to the `human` case, and `run.rs::list_marks_unavailable_classes` now marks `human` only.
7. `officina relay --json` prints a `RelayView` (AD-35: `stopped_since_ms`, `approved_hash`, `catalogue`, `pending` with hash and diff, `issues` with lines, `recent` with the last 50 entries and runs paired, `journal_corrupt_lines`, `gate_errors_24h`), built by core `relay::view` from host reads. `catalogue` is the approved snapshot and the current floor's differences are `pending.diff` (D-24). A `run_started` without `run_finished` past its timeout plus 60 s reads "abandoned" (AD-32). Proof: CLI tests `crates/officina/tests/relay.rs::{prints_the_relay_view, catalogue_is_the_approved_snapshot, floor_edit_shows_a_pending_diff, issues_carry_their_lines, recent_pairs_runs_and_marks_abandoned}` and core unit tests `crates/officina-core/src/relay/view.rs::tests::*`.
8. Exit codes of `relay --json` (D-24): 0, 64 on usage, 1 when the home cannot be read; never 2. Proof: `relay.rs::{usage_error_exits_64, unreadable_home_exits_1}`.
9. `RelayView` and its parts derive `ts_rs::TS` and join the AD-13 drift check. Proof: `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types` with `RelayView` exported.
10. Zero write: `relay --json` joins the fingerprint test; it writes nothing under the Officina home either. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` and `relay.rs::relay_json_writes_nothing`.

## Tasks

- [x] Core `relay::classes::check_satisfied` and the `after-check` branch of the class decision (AC 1, 2, 6)
- [x] Host `relay::tree`: `git` with the D-22 flags and `CREATE_NO_WINDOW`, untracked content hashing with the cap, "no HEAD" marker, absence rules (AC 3)
- [x] `run` records `tree` on check and `after-check` actions and passes it to the class decision (AC 3, 4)
- [x] Harness `git_dir` helper and skip message (AC 5)
- [x] Core `relay::view`: `RelayView`, pairing, abandoned state; host read of approved, floor, journal, stop flag (AC 7)
- [x] `officina relay --json` in `crates/officina/src/main.rs`; update `USAGE` (AC 7, 8)
- [x] Export `RelayView` in `typescript_bindings.rs`; regenerate `app/src/lib/generated/` (AC 9)
- [x] CLI tests `run.rs`, `relay.rs`; extend the zero-write test (AC 4, 6-8, 10)
- [x] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (`after-check`, `relay --json`) (D-34)

## Dev Notes

- Requirements: FR-8 `after-check` as amended (amendment 8), FR-10 (data side), FR-13. Decisions: AD-32 (abandoned), AD-33 first part, AD-35 (`RelayView` fields). PRD journey 2. Epic decisions: P-3, D-20, D-22 to D-24.
- D-22: `git diff` honours repository-local `diff.external`, textconv drivers and `core.fsmonitor`; without the flags the fingerprint could vary between runs, or run a program configured in the repository inside the relay's job. Ignored files (vault files, local inventories) are not bound: stated in the README and by doctor.
- `git` is resolved through `PATH`, which the session controls: acceptable for a guardrail (PRD section 1), noted for v0.6.
- `relay --json` also feeds the Playwright fixtures (stories 2.7 to 2.9); keep its output deterministic for a given home. It takes no `now_ms`, so fixtures that need an abandoned run seed the journal (D-25).

## Testing

- Core unit: `crates/officina-core/src/relay/{classes,view}.rs`.
- Host integration: `crates/officina-host/tests/relay_tree.rs`.
- CLI integration: `crates/officina/tests/{run,relay,quota_doctor}.rs`.
- Bindings: `crates/officina-core/tests/typescript_bindings.rs`.

## Dev Agent Record

### Debug Log

- **Decisions where the documents are silent**, each the reading that fails closed or matches the architecture:
  - The 30 minutes of AD-33 run from the check's `run_started`, the moment its tree and parameters were bound; a check dated in the future does not count. The session that ran the check is not bound (AD-33 lists project, action, parameters, hash and tree).
  - A failed `after-check` is refused with a new code, `check_required` (`RefusalReason`, after `class_not_available`); the stderr line starts `run `<check>` first` (PRD FR-8). `run.rs::every_refusal_code_exits_126_with_one_journal_line` covers it through the exhaustive scenario.
  - "Check actions" are the actions any `after-check` action of the catalogue names as its `check`; their runs and the `after-check` runs record the tree. Other runs spawn no `git`.
  - The tree is a `TreeFingerprint { sha256, untracked_by_size }` in the journal rather than a bare string, so "the record says so" (D-22) is a field. `run_started` also records `timeout_s`, which the abandoned state needs (AD-32 does not list it); lines written before it existed wait the longest timeout (3600 s).
  - The whole work tree binds, whichever folder of it the project is: `ls-files --full-name -- :/`, read through `rev-parse --show-cdup` (relative, so a `git` printing `/c/...` paths still reads the files). One `rev-parse` call tells outside, no HEAD and HEAD apart. `--no-ext-diff --no-textconv` are options of `git diff`, so they go on the diff; `--no-pager`, `--no-optional-locks` (no index write-back in the project), `-c core.fsmonitor=false` and `-c core.untrackedCache=false` go on every call.
  - Without a commit, every file `git` lists (staged or not) is bound by content after the "no HEAD" marker, since there is nothing to diff against; staging alone does not change the fingerprint.
  - `RelayView` carries, beyond AD-35's list: `schema` (the contract's version, as the board has; Epic 5's P-26 bumps it), `stopped` (a flag that cannot be read stops the relay without a time, AD-34), `floor` (the relay floor's path, D-8), `floor_error`, `snapshot_error` and `approved_at_ms`, so the window can say why nothing runs.
  - `pending` exists when the floor's hash differs from the approved one. With nothing approved, only when the floor declares an action or a guarded project (a v0.1 floor has nothing to approve), diffed against an empty catalogue; with a broken snapshot, always, so the author can approve again.
  - `recent` holds the last 50 entries sorted by time, newest first, a run once at its start; an end line whose start is older than the two days read is dropped. The run variant is boxed in `RecentEvent` (clippy `large_enum_variant`).
  - `relay --json` exits 1 when no home can be determined, or the home exists and is not a readable folder; a home that does not exist yet is an empty relay (exit 0), and nothing is created.
  - ts-rs warns that it ignores `deny_unknown_fields` on `CanonicalParam` (an enum); the attribute stays for serde and the warning does not fail `-D warnings`. `FieldChange`'s JSON values are typed `unknown` through `#[ts(type)]`, so ts-rs needs no `serde-json-impl` feature.
- **CI fix on the way (coordinator request).** The core-linux job had failed `gate.rs::reason_lists_approved_actions` since story 2.4. It was not a key mismatch in the gate: off Windows the fake tool's path is a POSIX path, no `native` program (AD-22), so the test floor's actions were dropped and the approved snapshot gave the project no action. Reproduced on Windows by forcing a POSIX path (same line, same message), fixed in the test harness (`common::fake_tool_path`) with a guard test that fails on Linux without it: commit `test(relay): name a Windows program in test floors off Windows`. No Rust toolchain in WSL, so the Linux leg is reasoned and reproduced, not run.
- **Adversarial pass.** Tried: a check then a tracked edit, a same-size untracked edit, an untracked file over 1 MiB, ignored files, a repository without a commit, staging, a project in a subfolder of the work tree, a repository configuring `diff.external` and a textconv driver (trap with a positive control), `GIT_DIR` and `GIT_WORK_TREE` set by the caller, `assume-unchanged` and `skip-worktree` set after the check, a check still running, failed, timed out, under another hash, in another project or with other parameters, a check just before midnight UTC, a stale check (seeded journal, with a recent one as positive control), `approved.json` deleted after an approval, an empty stop flag, and a home that is a file. Three real misses became tests: `git diff HEAD` hides edits of files flagged `assume-unchanged` or `skip-worktree`, so a session could check, flag, edit and apply (`relay_tree.rs::flagged_tracked_edits_change_it`, mutation-checked); the caller's `GIT_*` variables could point the fingerprint at another repository (`relay_tree.rs::git_variables_do_not_redirect_it`); with `approved.json` gone after an approval the gate guards every project (D-15) while the view offered nothing to approve for a floor without actions (`relay.rs::a_snapshot_gone_after_an_approval_can_be_approved_again`). `officina run --list` now says which check an `after-check` action waits for.
- **Known limits, recorded in `relay/tree.rs`:** clean and smudge filters a repository configures still run when `git` compares a modified file; a tracked edit whose size and modification time are put back escapes `git`'s stat check; `git` comes from the session's `PATH` (noted for v0.6). An `after-check` action whose check its project does not list can never run; doctor (story 2.10) is the place to warn.

### Completion Notes

- AC 1: core `relay/classes.rs::tests::{matching_successful_check_satisfies, failed_check_does_not, other_params_do_not, other_project_does_not, other_catalogue_hash_does_not, other_tree_does_not, check_older_than_30_min_does_not, started_without_finished_does_not}`; core `relay/run.rs::tests::after_check_needs_its_check_and_binds_the_tree`.
- AC 2: `classes.rs::tests::check_before_midnight_counts_after_midnight`; host `crates/officina-host/tests/relay_journal.rs::reader_spans_two_days_and_counts_corrupt` (story 2.4).
- AC 3: `crates/officina-host/tests/relay_tree.rs::{same_tree_same_fingerprint, tracked_edit_changes_it, same_size_untracked_edit_changes_it, untracked_file_over_1_mib_is_bound_by_size, ignored_files_do_not_count, repository_without_commit_is_bound, external_diff_and_textconv_are_not_run, outside_git_it_is_absent}`, plus `flagged_tracked_edits_change_it` and `git_variables_do_not_redirect_it`.
- AC 4: `crates/officina/tests/run.rs::{check_then_apply_runs, apply_without_check_is_refused, apply_with_other_params_is_refused, apply_after_a_tree_change_is_refused, apply_after_an_untracked_edit_is_refused, stale_check_is_refused}`.
- AC 5: `crates/officina/tests/common/mod.rs::{git_dir, git_dir_or_skip}`; run with no `git` on `PATH`, the run and tree tests print `skipped <test>: no git on this machine's PATH`.
- AC 6: `run.rs::after_check_and_human_are_not_available_yet` (human only); `run.rs::list_marks_unavailable_classes` (human marked, `apply` and `probe` not).
- AC 7: `crates/officina/tests/relay.rs::{prints_the_relay_view, catalogue_is_the_approved_snapshot, floor_edit_shows_a_pending_diff, issues_carry_their_lines, recent_pairs_runs_and_marks_abandoned}`; core `relay/view.rs::tests::*`.
- AC 8: `relay.rs::{usage_error_exits_64, unreadable_home_exits_1}`.
- AC 9: `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types` exports `RelayView`; `app/src/lib/generated/` regenerated.
- AC 10: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` runs `relay --json`; `relay.rs::relay_json_writes_nothing` fingerprints the whole temp root, and a fresh home is not created.
- Gates at the end of the story: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings` and with CI's feature sets (`--features officina-core/ts`, the workspace and the three Linux crates); `cargo test --workspace --all-features` 367 passed, 0 failed, 5 ignored; TS drift test green; `pnpm -C app check` clean. The after-check commit was built and tested alone in a scratch worktree (clippy and the three crates' tests green).

## File List

- `crates/officina-core/src/relay/{mod,classes,journal,run,view,catalogue,gate}.rs`, `crates/officina-core/src/floor.rs`
- `crates/officina-core/tests/typescript_bindings.rs`, `app/src/lib/generated/*.ts` (relay types added)
- `crates/officina-host/src/relay/{mod,run,tree,view}.rs`
- `crates/officina/src/main.rs`
- `crates/officina-host/tests/relay_tree.rs`
- `crates/officina/tests/{common/mod,run,relay,quota_doctor,gate}.rs`
- `README.md`, `CHANGELOG.md`

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: D-22 (git flags, untracked content, no HEAD), D-23 (git in test `PATH`, skip with reason), D-24 (exit codes, approved snapshot as catalogue), exit 1 test
- 2026-10-28 implemented: `after-check` with the journal and the tree fingerprint, `officina relay --json` and the `RelayView` bindings; adversarial pass (flagged files, `GIT_*` variables, a snapshot gone after an approval); status Review
