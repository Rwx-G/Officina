# Story 8.15: Doctor for workers and the check-mode scan (Slice B)

**Status:** Approved

## Story

As the author who lists dry runs a worker may request,
I want doctor to name every file in those projects where a task can run for real under `--check` or reach beyond the limit, to say whose identity worker reads use, and to check that each workshop image can carry a relay request,
so that I know where a "read" can write before a worker meets it, and the setup behind worker reads is visible without guessing (FR-10).

## Acceptance Criteria

1. **Entry condition** (P-72, D-1313). 8.12 has merged; v0.6's hook program inventory (6.9, AD-118) is on `main`. Proof: the sprint status.
2. **The constant** (AD-164, FR-7, FR-10 as amended by amendment 11, D-421). Core `relay::ansible_scan::ANSIBLE_CHECK_ESCAPES`, seeded once from FR-7's four forms (a `check_mode` set to false or no, `delegate_to`, `local_action`, a `run_once` set to true or yes); from then on the constant is the list. Each entry holds its key and the values that count; `scan_line(&str) -> Option<EscapeKind>` matches a YAML key at the start of a line after spaces and an optional `- `, values compared ignoring ASCII case with or without quotes, a line whose first non-space character is `#` skipped. Proof: core `crates/officina-core/src/relay/ansible_scan.rs::tests::{every_entry_matches_its_shaped_lines, every_entry_ignores_comments_and_other_values, a_key_inside_a_value_does_not_match}` (each iterates `ANSIBLE_CHECK_ESCAPES` and derives its case, quote, list-item and comment variants programmatically).
3. **The scan** (AD-164, D-421, D-427, D-1314, review F2). Its findings are `ScopeCheck` variants (8.4) with their level. For each project of a template whose `relay_actions` hold a dry run for workers in D-427's sense (the check of any write of the catalogue, or an unscoped check-mode read; `dry_run_for_workers`, 8.4), doctor reads every file under the project folder, ignored folders included, `.git` excluded, symbolic links and junctions not followed, at most 4,000 files read up to 256 KiB each; it names each file with a non-zero count, with its counts per kind, as a warning beside the dry-run actions listed for workers, and says when a limit stopped it. It resolves no playbook and parses no YAML. Proof: host `crates/officina-host/src/relay/doctor.rs::tests::{the_scan_names_files_with_counts, the_scan_reads_ignored_folders_and_skips_git, the_scan_stops_at_its_limits_and_says_so, the_scan_does_not_follow_links, an_unscoped_check_mode_read_brings_its_project_into_the_scan}`.
4. **Workers** (AD-164, FR-10, D-421, D-624, D-1311, D-1314). Per workshop template: its relay actions; per image: the `officina-relay` link, which AD-118's inventory now resolves as a fixed entry (`found`, `missing`), a `WorkshopCheck` variant (D-1113's pattern) whose `missing` on an image a template with relay actions uses fails doctor; per relay action listed for workers, an identity line read from the approved action's `args` by shape: "your own SSH identity", or "a key file named in its arguments" when an `-i` item is present (a service account, D-624), never the file, the key or the account; with the gate 3 entry's outcome as the README records it. Proof: CLI `crates/officina/tests/doctor_workshop.rs::{relay_actions_are_listed_per_template, a_missing_relay_link_exits_1, the_identity_line_is_per_action, a_key_file_in_args_reads_as_a_key_file}` (as built for the workshop section, D-1306); host `crates/officina-host/src/workshop/inventory.rs::tests::the_relay_link_is_a_fixed_entry` (6.9's module).
5. **Scope lines for worker dry runs** (AD-164, FR-3, D-403, D-433, D-1314, review F21). Each is a `ScopeCheck` variant. For projects with dry runs listed for workers, whether or not Slice A is built: a write with a target and no targets action, "scope judged on names only" (every write when D-403 left the resolver unbuilt); a write without a target, "a write without a target: its dry runs are always out of scope". Each is a warning naming the project and action. Proof: CLI `crates/officina/tests/doctor_relay.rs::{names_only_is_named_for_worker_dry_runs, a_write_without_target_is_named_for_worker_dry_runs}`.
6. **Doctor runs nothing** (AD-164). Doctor runs no action, no targets resolution and no container. Proof: CLI `doctor_workshop.rs::doctor_launches_nothing_for_workers` (the fake tool's launch count and the fake engine's `create` and `start` counts stay zero).
7. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: doctor's worker checks and the check-mode scan. README: the scan described, `ANSIBLE_CHECK_ESCAPES` named and never listed; what to do with a named file (keep its dry run off `relay_actions`, or remove the task's escape); the identity line and the service-account recommendation (D-1311). Proof: review.

## Tasks

- [ ] `crates/officina-core/src/relay/ansible_scan.rs` (AC 2)
- [ ] Doctor's scan (AC 3)
- [ ] Inventory fixed entry as a `WorkshopCheck` variant; doctor's worker items and the per-action identity line (AC 4); the two scope lines as `ScopeCheck` variants (AC 5)
- [ ] `doctor_workshop.rs` and `doctor_relay.rs` cases (AC 4 to 6); `CHANGELOG.md`, README (AC 7)

## Dev Notes

- Requirements: FR-10 as amended (amendment 11), FR-7 ("Why reads and in-scope dry runs only"), PRD question 7. Decisions: AD-118, AD-164. Gates: the gate 1 entry (counts for the README), the gate 3 entry (identity). Risks: R84, R85, R88. Epic decisions: D-211, D-624, D-1113; P-72, D-400, D-403, D-421, D-423, D-427, D-433, D-1306, D-1311, D-1313, D-1314.
- Seams (D-400, read through `main` as built, D-1306): doctor's Workshop section, core `workshop::doctor::WorkshopCheck` and `crates/officina/tests/doctor_workshop.rs` (6.3 as built, D-1113; 6.24 groups it), AD-118's inventory (6.9, D-211), relay doctor (`crates/officina-host/src/relay/doctor.rs`, 2.10), `ScopeCheck` (8.4), `RelayView`'s `dry_run_for_workers` (8.4).
- Waits for: the entry condition (AC 1).
- The scan names files, never playbooks: deciding which playbook reaches which role is Ansible's job, and a line-based scan cannot do it honestly (amendment 11). A named file is a reason to look, not a verdict.
- The constant is the only list of forms; the README and doctor's text name it and describe the families (D-423, derived, not transcribed).
- No employer content in fixtures: a scratch project with placeholder playbooks.

## Testing

- Core unit: `crates/officina-core/src/relay/ansible_scan.rs`.
- Host unit: `crates/officina-host/src/{relay/doctor,workshop/inventory}.rs`.
- CLI integration: `crates/officina/tests/{doctor_workshop,doctor_relay}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the scan covers projects with any dry run for workers in D-427's sense, unscoped check-mode reads included (F2); this story owns the two scope lines for worker dry runs (F21, D-433); `a_missing_relay_link_exits_1` stays here only (F36, D-421)
- 2026-11-02 reworked against v0.4 to v0.6 as built: entry names v0.6's 6.9 (D-1313); the link a `WorkshopCheck` variant and the scope lines `ScopeCheck` variants (D-1314); the identity line per relay action, a key file in `args` read as a service account (D-624, D-1311); CLI cases in the existing `doctor_workshop.rs` and `doctor_relay.rs` (D-1306)
