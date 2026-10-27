# Story 2.6: Class after-check, tree fingerprint, `officina relay --json`

**Status:** Approved

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

- [ ] Core `relay::classes::check_satisfied` and the `after-check` branch of the class decision (AC 1, 2, 6)
- [ ] Host `relay::tree`: `git` with the D-22 flags and `CREATE_NO_WINDOW`, untracked content hashing with the cap, "no HEAD" marker, absence rules (AC 3)
- [ ] `run` records `tree` on check and `after-check` actions and passes it to the class decision (AC 3, 4)
- [ ] Harness `git_dir` helper and skip message (AC 5)
- [ ] Core `relay::view`: `RelayView`, pairing, abandoned state; host read of approved, floor, journal, stop flag (AC 7)
- [ ] `officina relay --json` in `crates/officina/src/main.rs`; update `USAGE` (AC 7, 8)
- [ ] Export `RelayView` in `typescript_bindings.rs`; regenerate `app/src/lib/generated/` (AC 9)
- [ ] CLI tests `run.rs`, `relay.rs`; extend the zero-write test (AC 4, 6-8, 10)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (`after-check`, `relay --json`) (D-34)

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

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: D-22 (git flags, untracked content, no HEAD), D-23 (git in test `PATH`, skip with reason), D-24 (exit codes, approved snapshot as catalogue), exit 1 test
