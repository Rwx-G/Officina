# Story 6.16: Import, Discard, the per-template table, and the import battery

**Status:** Review

## Story

As the author who decides each package in one gesture,
I want Import to add exactly one new local branch and touch nothing else, Discard to record why, and the per-template results to come from the journal, with a battery and two mutants proving that no keystroke, stale hash or extra reference gets a package in,
so that my repository changes only when I click Import on a verified, approved package, and I can tell which templates earn their runs.

## Acceptance Criteria

1. **Import** (AD-114, FR-12 as amended by amendment 10, D-28). `import_package(run_id, tip)` (pointer only; this story owns its `build.rs` entry, capabilities, `every_declared_command_is_allowed`, `ipc.ts` wrapper and `e2e-mock.ts` answer, review F33), on thread `workshop`, under `GitRule::Workshop`: the run is in `review`, every group approved, `tip` equals the recorded tip (`state_changed`); `rev-parse --verify --quiet refs/heads/officina/<name>` in the project refuses `branch_exists`, and an existing `refs/heads/officina` (a branch named `officina`, which would make `update-ref` fail with a directory and file conflict) refuses `branch_conflict` (D-248, review F47); `git fetch --no-tags --no-write-fetch-head <scratch repository> refs/heads/officina/<name>` (objects only); `cat-file -e <tip>^{commit}` (`package_changed`); `update-ref refs/heads/officina/<name> <tip> <zero object id>`; state `imported`, `review_imported` (branch, tip). Each refusal is a variant of the `closed_enum!` `ImportRefusal`. Proof: CLI `crates/officina/tests/workshop_review.rs::{import_adds_exactly_one_reference, import_refuses_an_existing_branch, import_refuses_a_branch_named_officina_as_a_conflict, import_refuses_a_stale_tip, import_refuses_an_unapproved_group, import_refuses_a_quarantined_package, a_branch_created_between_check_and_update_is_not_moved}` (the last creates the branch right before `update-ref` through a test hook and expects the zero-old-value update to fail); core `crates/officina-core/src/relay/journal.rs::tests::review_imported_round_trips`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
2. **Host untouched** (PRD section 7 Host untouched, AD-125 zero-write row, D-80, D-163). Around a launch, a collection, a quarantine, a discard and an import through the fake engine: the Claude config dir and the project repository's references, index and work tree are unchanged, except that Import adds `refs/heads/officina/<name>` and objects. Proof: `crates/officina/tests/quota_doctor.rs::{a_quarantine_and_a_discard_leave_the_project_unchanged, import_changes_only_its_reference_and_objects}` (a tracked file's modification time touched first, so a plain `git status` would rewrite the index).
3. **Discard** (AD-114, FR-12). `discard_package(run_id, reason)` (keyboard-reachable: it only reduces; this story owns its plumbing as for Import, review F33): `DiscardReason` holds the author's three reasons and the system's `expired` and `not_collected`; the author's command accepts the three only; state `discarded`, `review_discarded` (reason). A quarantined package offers Discard only; a `no_change` run neither. Proof: CLI `workshop_review.rs::{discard_records_the_reason, the_author_cannot_send_a_system_reason, a_no_change_run_refuses_discard}`; core `journal.rs::tests::review_discarded_round_trips`.
4. **Expiry** (AD-124, amendment 10, D-213). The hourly pass discards a package still undecided at `RUN_FOLDER_TTL_MS`, with reason `expired`, in the same pass that deletes its run folder, so no Import is offered on a folder already gone (review F39). Proof: core `crates/officina-core/src/workshop/run.rs::tests::{an_undecided_package_expires_with_its_folder, no_undecided_package_outlives_its_folder}`; host `crates/officina-host/tests/registry_retention.rs::an_expired_package_is_discarded_with_its_reason`.
5. **The per-template table** (AD-114, FR-12 as amended, amendment 15, D-222). Core's `view::template_table(journal lines)` gives, per template: runs, `no_change` runs, failed runs, quarantined, imported, discarded by reason, imported over decided (decided being Import and the author's three reasons, D-222), and story 6.15's notice. Proof: core `crates/officina-core/src/workshop/view.rs::tests::{no_change_and_failed_are_not_decided, expired_and_not_collected_are_not_decided, imported_over_decided_counts_author_verdicts_only}`.
6. **Injection battery, workshop import** (AD-125, D-225). `injection.json` gains `workshop_import`: a keyboard Import (a case tagged `keyboard`, which the Rust harness skips by tag and story 6.19's Vitest replays against the Import button, expecting zero `import_package` calls on `detail === 0`), a tip mismatch, an unapproved group, a stale `hunks_sha256`, a quarantined package, an existing branch, a `no_change` run; each expects its refusal and an unchanged project repository. Proof: `crates/officina/tests/injection.rs::{workshop_import_cases, every_section_family_and_refusal_has_a_case}` (the coverage also iterates `ImportRefusal::ALL`).
7. **Mutants** (AD-125 mutation row, D-225, D-148). Core features `mutant-accept-extra-ref` (the single-reference check of `bundle::parse_header` skipped) and `mutant-skip-group-approval` (the import readiness check skipped), forwarded `officina` to `officina-host` to `officina-core`, `MUTANTS_ENABLED` covering them; the `mutants` job, per feature: `--no-run` compiles, then the run of the target the job names for that mutant fails with the named case: `cargo test -p officina --test workshop_collect` for `mutant-accept-extra-ref` (the crafted `extra_ref` package lives there), `--test injection` for `mutant-skip-group-approval` (the unapproved-group case of AC 6) (review F36); then the control passes. Proof: the CI job `mutants`.
8. **Documentation** (D-226). README: Import (a new local branch, never a merge, never a push), Discard and its reasons, the per-template table, Acceptance's denominator; `CHANGELOG.md`. Proof: review.

## Tasks

- [x] `crates/officina-host/src/workshop/import.rs`: Import, `ImportRefusal`, Discard; commands (AC 1, 3)
- [x] Zero-write cases (AC 2)
- [x] Expiry in the hourly pass (AC 4)
- [x] `view::template_table` (AC 5)
- [x] `workshop_import` section of `injection.json` (AC 6)
- [x] Two mutant features, forwarding, CI job (AC 7); README, `CHANGELOG.md` (AC 8)

## Dev Notes

- Requirements: FR-12 as amended (amendment 10), J2, journey 3, PRD section 7 Gesture, Host untouched, Acceptance. Decisions: AD-114, AD-120, AD-124, AD-125. Risks: R60. Epic decisions: D-28, D-80 (Epic 2, 3); D-248, D-249; D-147, D-148, D-163 (Epic 5); P-50, D-200, D-213, D-222, D-225, D-226.
- Seams (D-200): the injection battery and its harness (5.7), the `mutants` job (5.7), `quota_doctor.rs` (D-80), the hourly pass (3.2); `host::git` (D-124).
- Waits for: story 6.15.
- Import fetches objects first and then creates the reference with a zero old value, so it never moves an existing reference, even one created after the `rev-parse` check (amendment 10).
- Merged detection is v0.7's (PRD section 5); this table keeps imported over decided.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/{run,view}.rs`, `relay/journal.rs`; host `crates/officina-host/tests/registry_retention.rs`; CLI with the fake engine and real git `crates/officina/tests/{workshop_review,injection,quota_doctor}.rs`; CI `mutants`.

## Dev Agent Record

### Debug Log

- Every new test passed on its first run. The extra-ref mutant was run by hand against `workshop_collect` and the group-approval mutant against `injection` before the gate: each failed with its `(guarded by ...)` line, the second also on the stale-hash case.

### Completion Notes

- AC 1: CLI `workshop_review.rs::{import_adds_exactly_one_reference, import_refuses_an_existing_branch, import_refuses_a_branch_named_officina_as_a_conflict, import_refuses_a_stale_tip, import_refuses_an_unapproved_group, import_refuses_a_quarantined_package, a_branch_created_between_check_and_update_is_not_moved}` (the last through `import_package_staged`'s hook, the branch created at the base right before `update-ref`, which fails on the zero old value and leaves it there); core `review.rs::tests::import_checks_state_groups_then_tip`, `relay/journal.rs::tests::review_imported_round_trips`; host `workshop::import::tests::the_zero_id_follows_the_object_format`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` covers `import_package` and `discard_package` through `build.rs` and the capabilities; `watch.rs::the_collector_never_spawns_docker` sends both collector halves. Refusals and threads: D-1183, D-1184.
- AC 2: `quota_doctor.rs::{a_quarantine_and_a_discard_leave_the_project_unchanged, import_changes_only_its_reference_and_objects}`, each with a tracked file's modification time moved first; the first compares the whole project folder `.git` included and the Claude config dir, the second every file under `.git` but `objects/` (exactly the new loose reference and its folders added: no `FETCH_HEAD`, no reflog, the index unchanged), every earlier object kept byte for byte, at least one object added, the work tree and the Claude config dir unchanged.
- AC 3: CLI `workshop_review.rs::{discard_records_the_reason, the_author_cannot_send_a_system_reason, a_no_change_run_refuses_discard}` (and `import_refuses_a_quarantined_package`, which discards it); core `review.rs::tests::discard_takes_an_authors_reason_on_a_package`, `relay/journal.rs::tests::review_discarded_round_trips`. D-1185.
- AC 4: core `run.rs::tests::{an_undecided_package_expires_with_its_folder, no_undecided_package_outlives_its_folder}`; host `registry_retention.rs::an_expired_package_is_discarded_with_its_reason` (review and quarantined rows expire with their folders, a younger and a decided row stay, the next pass expires nothing) and `workshop::import::tests::each_expired_package_is_journaled_expired` (the window's `review_discarded` lines). D-1186.
- AC 5: core `view.rs::tests::{no_change_and_failed_are_not_decided, expired_and_not_collected_are_not_decided, imported_over_decided_counts_author_verdicts_only}`; the last also checks the notice from verdict lines (D-1181's fold). D-1188.
- AC 6: `injection.rs::{workshop_import_cases, every_section_family_and_refusal_has_a_case}`: `workshop_import` holds the keyboard case (skipped, expecting no call, D-1189), a tip mismatch, an unapproved group (guarding `mutant-skip-group-approval`), a stale `hunks_sha256` (the stored approval rewritten), a quarantined package, an existing branch, a `no_change` run, and a forged id, a branch named `officina`, the scratch repository gone and no `git` so every `ImportRefusal` has a case; each asserts its answer, the references, index and work tree unchanged (one reference more for the control) and no engine `create` or `start` after collection.
- AC 7: core features `mutant-accept-extra-ref` (`bundle::branch_reference`, D-1187) and `mutant-skip-group-approval` (`review::groups_approved`), forwarded by `officina-host` and `officina`, counted by `MUTANTS_ENABLED`; CI's `mutants` job runs `workshop_collect` for the first (`workshop_collect.rs::a_bundle_with_an_extra_reference_is_quarantined` names it) and `injection` for the second, then both targets as the control. The job itself is run locally by the gate script (CI is disabled).
- AC 8: README "Import and Discard" under the workshop's review (one local branch, never a merge or a push, Discard's reasons, `expired` and `not_collected`, the per-template table, Acceptance's denominator); `CHANGELOG.md`.

## File List

- `crates/officina-core/src/workshop/review.rs`, `crates/officina-core/src/workshop/view.rs`, `crates/officina-core/src/workshop/run.rs`, `crates/officina-core/src/workshop/bundle.rs`, `crates/officina-core/src/workshop/package.rs`, `crates/officina-core/src/relay/journal.rs`, `crates/officina-core/src/relay/view.rs`, `crates/officina-core/src/lib.rs`, `crates/officina-core/Cargo.toml`, `crates/officina-core/tests/typescript_bindings.rs`
- `crates/officina-host/src/workshop/import.rs` (new), `crates/officina-host/src/workshop/mod.rs`, `crates/officina-host/src/workshop/review.rs`, `crates/officina-host/src/watch.rs`, `crates/officina-host/src/registry/workshop.rs`, `crates/officina-host/src/registry/retention.rs`, `crates/officina-host/src/registry/window.rs`, `crates/officina-host/Cargo.toml`, `crates/officina-host/tests/registry_retention.rs`, `crates/officina-host/tests/registry_workshop.rs`
- `crates/officina/Cargo.toml`, `crates/officina/tests/workshop_review.rs`, `crates/officina/tests/workshop_collect.rs`, `crates/officina/tests/injection.rs`, `crates/officina/tests/quota_doctor.rs`, `crates/officina/tests/watch.rs`, `crates/officina/tests/common/package_run.rs`, `tests/vectors/injection.json`
- `app/src-tauri/build.rs`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/src/main.rs`, `app/src/lib/ipc.ts`, `app/src/e2e-mock.ts`, `app/src/e2e-globals.d.ts`, `app/src/lib/generated/` (`ImportAnswer`, `ImportRefusal`, `DiscardAnswer`, `DiscardRefusal`, `DiscardReason`)
- `.github/workflows/ci.yml`, `README.md`, `CHANGELOG.md`, `docs/stories/epic-6-decisions.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: Import and Discard plumbing owned here (F33); the mutants job names its test target per mutant (F36); expiry at `RUN_FOLDER_TTL_MS` with the folder (F39); `branch_conflict` (F47)
- 2026-11-03 implemented (D-1183 to D-1189); status review
