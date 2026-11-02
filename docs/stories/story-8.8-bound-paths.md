# Story 8.8: Bound paths in the tree fingerprint (FR-4, only if gate 1 finds ignored roles or collections)

**Status:** Approved

## Story

As the author whose Ansible projects run roles and collections that git ignores,
I want the folders I name hashed by content into the fingerprint my dry run and my approval are bound to, links bound by their target and missing paths by their absence,
so that a role changed between the dry run and the run voids the approval as a tracked file does, and the card says which files it covers.

## Acceptance Criteria

1. **Entry condition** (PRD section 6.1.1, FR-0, P-70, P-72, D-403). The gate 1 entry records ignored roles or collections under a guarded project's tree; 8.3 landed the `bind` key; story 2.8 is on `main`; 8.2's Decision time row does not hold it. Slice A's trigger is not a condition, nor any v0.7 story (D-1301). Otherwise this story stays approved and unimplemented. Proof: the gate 1 entry and `epic-8-decisions.md`.
2. **The walk** (AD-155, FR-4 as amended by amendment 5, D-22, D-413). Host `relay::tree::fingerprint_with(program, dir, bind)`: after the git part, for each bound path in canonical order, a walk with `symlink_metadata` that never follows a link or a junction; entries by relative path, sorted; a file by content up to D-22's 1 MiB and by size beyond; a link by its target text; a missing path by a marker, so absence binds too. The bound section is appended after a line `bound 1` only when `bind` is not empty, so a project without it keeps its `officina-tree 1` value. `TreeFingerprint` gains `bound: Option<BoundSummary { files, bytes, too_large }>`. Proof: host `crates/officina-host/tests/relay_tree.rs::{without_bind_the_fingerprint_is_unchanged, a_changed_ignored_file_under_a_bound_path_changes_it, a_bound_symlink_is_not_followed, a_bound_junction_is_not_followed, a_missing_bound_path_binds_its_absence, a_large_file_binds_by_size}` (the junction case creates one with `cmd /c mklink /J`, D-413); core `crates/officina-core/src/relay/journal.rs::tests::tree_fingerprint_with_bound_round_trips`.
3. **Limits** (AD-155, D-413). Past `BIND_MAX_FILES` (10,000 entries) or `BIND_MAX_BYTES` (256 MiB read) the walk stops and the fingerprint is `too_large`. A check on it still runs, being a read; an after-check run or a `human` request that needs that check is refused `bind_too_large` (`RefusalReason` gains it, its line recorded after the fact with `record`, D-760, D-1307; the TypeScript bindings are regenerated, P-73). Proof: host `relay_tree.rs::the_walk_stops_at_its_limits`; CLI `crates/officina/tests/approvals.rs::{a_check_runs_on_a_too_large_fingerprint, a_request_needing_it_is_refused_bind_too_large}`; `crates/officina/tests/run.rs::every_refusal_code_exits_126_with_one_journal_line`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types` (review F29).
4. **Every caller binds** (AD-155, D-413, review F14). `officina run` (the check's record, the after-check rule, story 2.8's request and consumption recheck), the window's `auto` baseline fingerprint (D-151) and the collector's `board_approvals` closure (host `collector.rs`, which recomputes the fingerprint every `TREE_RECHECK`, 10 s as built, for the card's "files unchanged") pass the project's `bind`. In the collector the bound part is cached by the summary of its `(path, size, mtime)` entries and its contents read again only when that summary changes, so a 256 MiB bound tree is not rehashed every 10 s. Proof: CLI `approvals.rs::a_bound_file_changed_after_the_dry_run_refuses_check_not_satisfied` (PRD section 7's Bound paths row), `crates/officina/tests/run.rs::an_after_check_run_sees_a_bound_file_change` and `crates/officina/tests/auto.rs::an_auto_baseline_sees_a_bound_file_change` (the rule baseline turns false); core `crates/officina-core/src/relay/approvals.rs::tests::a_bound_project_reads_files_unchanged` (where `DryRunView` is built, D-1306); host `crates/officina-host/tests/relay_tree.rs::{the_bound_cache_rehashes_only_on_a_changed_summary, a_changed_mtime_rehashes}`.
5. **The card's line** (AD-155, AD-162, FR-9, P-73). `DryRunView.bound` turns the card's line into "tracked, unignored and bound files unchanged since the dry run". `BOARD_SCHEMA` goes up by one relative to `main` at this merge unless 8.7 already moved it for v0.8 with no release in between (D-1301). The line is one stable label, so its longer wording moves nothing after it (D-1309). Proof: core `crates/officina-core/src/relay/approvals.rs::tests::the_dry_run_line_names_bound_files`; Vitest `app/src/components/components.test.ts > ApprovalCard names bound files when the project binds paths`; Playwright `app/e2e/approvals.spec.ts > a bound project's card names bound files`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
6. **Doctor** (AD-164, FR-10, D-413, D-421, D-1314, review F32). Each line is a `ScopeCheck` variant (8.4) with its level. Per project with `bind`: each bound path with its file count and size from one walk, `bind_too_large` named as a warning; a bound path that is itself a link or junction, which binds only its target text, as a warning that nothing under it is covered; bound entries carrying the Cloud Files attribute (reading a placeholder hydrates it), named with a count. Proof: CLI `crates/officina/tests/doctor_relay.rs::{bound_paths_show_count_and_size, bind_too_large_warns, a_bound_link_root_warns}`; host `crates/officina-host/src/relay/doctor.rs::tests::bound_cloud_placeholders_are_named` (an injected attribute probe, as story 3.1's location check).
7. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: bound paths. README "Relay scope and workers (v0.8, in progress)": `bind` with a placeholder example, the limits, links and absence, and what stays unbound (PRD section 4.1). Proof: review.

## Tasks

- [ ] `relay::tree`: the bound walk, `BoundSummary`, limits (AC 2, 3)
- [ ] Callers: `officina run`, after-check, story 2.8's request and recheck, the baseline (AC 4)
- [ ] `RefusalReason::BindTooLarge` (AC 3)
- [ ] `DryRunView.bound`, the card's line, `BOARD_SCHEMA` (AC 5)
- [ ] Doctor (AC 6); `CHANGELOG.md`, README (AC 7)

## Dev Notes

- Requirements: FR-4 as amended (amendment 5), section 7's Bound paths row. Decisions: AD-33, AD-155, AD-162, AD-164. Risks: R91. Epic decisions: D-22, D-151, D-170, D-760; P-70, P-72, P-73, D-400, D-403, D-404, D-413, D-421, D-423, D-1301, D-1306, D-1307, D-1309, D-1314.
- Seams (D-400, read through `main` as built, D-1306): host `relay::tree::fingerprint_with` (`crates/officina-host/src/relay/tree.rs`) and D-22's content rule (2.6, 5.2), the after-check rule (2.6), story 2.8's request and recheck (host `relay/run/request.rs`, `consume.rs`; 2.8 AC 19, AC 21), the review's baseline fingerprint (5.13, D-151), the collector's `board_approvals` closure and `collector::TREE_RECHECK` (2.8 AC 20), `DryRunView` in core `relay/approvals.rs` (2.8 AC 20).
- Waits for: the entry condition (AC 1).
- The walk reads files with `std::fs` only (no crate, AD's dependency note); `read_dir` with `symlink_metadata` replaces `walkdir`.
- A missing bound path binding its absence means creating it later voids the approval, which is the intent: an ignored folder appearing between the dry run and the run is a change.
- No employer content in fixtures: a scratch repository with ignored `roles/` and `collections/` folders of placeholder files.

## Testing

- Core unit: `crates/officina-core/src/relay/{journal,approvals}.rs`; core integration: `crates/officina-core/tests/typescript_bindings.rs`.
- Host integration: `crates/officina-host/tests/relay_tree.rs` (as built, extended).
- CLI integration: `crates/officina/tests/{approvals,run,auto,doctor_relay}.rs`.
- Vitest: `app/src/components/components.test.ts`. Playwright: `app/e2e/approvals.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the collector's fingerprint passes `bind`, with the bound part cached by its entries' summary (F14, D-413); doctor warns on a bound link root and names Cloud Files placeholders (F32); bindings named (F29)
- 2026-11-02 reworked against v0.4 to v0.6 as built: no v0.7 condition (D-1301); `DryRunView` tests in core `relay/approvals.rs`, host files named as on `main` (D-1306); `bind_too_large` recorded after the fact (D-760, D-1307); the card's line a stable label with a Playwright case (D-1309); doctor lines as `ScopeCheck` variants (D-1314)
