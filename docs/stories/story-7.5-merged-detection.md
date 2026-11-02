# Story 7.5: Merged detection

**Status:** Approved

## Story

As the author who imports workshop packages as `officina/*` branches and merges them in my own repository,
I want Officina to read, without fetching or writing anything, whether each imported branch reached its base, and to let me correct a squash merge with one click,
so that each template's acceptance can be read beside what I actually merged, and the "Gate on merges" switch rests on evidence (FR-1, J3).

## Acceptance Criteria

1. **Rules in core** (AD-130, FR-1 as amended by amendment 1). `governor::merge::classify(&MergeFacts, imported_at_ms, now_ms) -> MergeOutcome`, first match wins: `merged` (T an ancestor of a B reference); `merged_changed` (the branch's current tip past T, T its ancestor, and that tip an ancestor of a B reference); `merged_equivalent` (at least one `cherry` line against one B reference, and every line of it `-`); `not_merged` (the branch gone, or `MERGE_READ_DAYS`, 14 days, since the import); else `pending`. Proof: the `merge` section of `tests/vectors/governor.json`, a case per row and per boundary (13 days 23:59 pending, 14 days not merged), through `crates/officina-core/tests/governor_vectors.rs::{merge_cases_match, every_merge_outcome_has_a_case}`.
2. **The facts, on their own thread** (AD-127, AD-130, D-312). Thread `governor-git` (`window-authority`) runs one job at a time; the collector never runs git and never waits. For each run read (imported, `commits_past_base` above 0, merge row absent or `pending`; B, C, T and the branch from the run's row), with host `git.rs` and `workshop::git::rule_args` (the addendum's `GitRule::Workshop`, D-1202) and `GIT_OPTIONAL_LOCKS=0`, never `fetch`: the branch's tip (`rev-parse --verify --quiet refs/heads/officina/<name>^{commit}`); `git remote`, each name passing core's ref-component check before it becomes `refs/remotes/<remote>/<B>` (others skipped and counted); `merge-base --is-ancestor` of T and of the current tip against each B reference; `git cherry <B ref> <T> <C>` against each. Outcomes return as an `AuthorRequest` without deadline, never dropped (D-135's outcome pattern). Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{the_board_keeps_its_pace_while_a_merge_pass_hangs, a_merge_outcome_arriving_late_is_still_recorded}`; host unit `crates/officina-host/src/governor/git.rs::tests::a_remote_with_an_invalid_name_is_skipped_and_counted`.
3. **Real repositories** (AD-148 merges row, D-23, D-339). With real git on fixture repositories: merged locally (fast-forward and merge commit), merged through a remote-tracking branch, retouched then merged, cherry-picked, squashed (`pending`, then the author's correction), branch deleted without a merge, 14 days without a merge, a project folder missing (the row stays `pending` and is counted). Proof: host integration `crates/officina-host/tests/governor_merge.rs::{merged_locally_by_fast_forward, merged_locally_by_merge_commit, merged_through_a_remote_tracking_branch, retouched_then_merged, cherry_picked_is_equivalent, squashed_stays_pending_until_corrected, deleted_branch_is_not_merged, fourteen_days_is_not_merged, a_missing_project_stays_pending}`.
4. **No program runs, nothing is written** (FR-1 as amended, AD-130, D-312, D-154's pattern, D-163). One test per neutralised configuration (an external diff driver, a textconv driver, an fsmonitor hook, a filter driver set in repository config and attributes) points it at a marker program that must not run during a pass; a pass leaves the project's references, index and work tree unchanged after a tracked file's modification time was touched (so plain git would refresh the index), and a control running the same call without `GIT_OPTIONAL_LOCKS=0` sees the index change. Proof: `governor_merge.rs::{no_external_diff_runs, no_textconv_runs, no_fsmonitor_runs, no_filter_driver_runs, a_pass_leaves_refs_index_and_work_tree_unchanged, the_control_without_optional_locks_changes_the_index}`.
5. **When, and once** (AD-130, D-312). The pass runs hourly and on `AuthorRequest::MergePass`, which story 7.12's `get_governor` sends when the last pass is older than 5 minutes; only rows absent or `pending` are read again; an outcome other than `pending` is written once with `settled_at_ms`, and `merge_settled` (run, outcome, source) is recorded once per final outcome through `relay::journal::record` (D-1207). Proof: `governor_merge.rs::{only_pending_rows_are_read_again, merge_settled_is_journaled_once}`; core `crates/officina-core/src/relay/journal.rs::tests::merge_settled_round_trips` (`JournalKind` gains it).
6. **Corrections** (AD-130, D-28, D-312). `correct_merge(run_id, merged | not_merged)`, pointer only: accepted on a `pending` or `not_merged` row of source `rules`, written with source `author` and final, journaled `merge_settled` with source `author`; anything else is refused `correction_refused`. The command is declared in `build.rs`, allowed in `capabilities/default.json`, a thin wrapper over `AuthorRequest` with a deadline (D-92), wrapped in `ipc.ts` and answered by `e2e-mock.ts`. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::{a_correction_on_a_pending_row_is_final, a_correction_on_a_merged_row_is_refused, a_second_correction_is_refused}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`; the control's pointer-only case is story 7.12's.
7. **Zero write** (D-80). A merge pass on a fixture repository joins the fingerprint test of the Claude config dir and `<home>/.claude.json`. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
8. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: merged detection, read-only, never fetching. README "The governor (v0.7, in progress)": what each outcome means, that a merge on a forge counts once the author's repository has fetched it, that squash merges need the one-click correction, that no program the repository configures runs. Proof: review.
9. **Worktrees of work orders** (D-1214). Merged detection also covers the git worktrees Claude creates for v0.5 work orders (D-1002), through story 7.21, which reuses this story's `merge::classify`, thread and fixture repositories. `classify` takes `MergeFacts` and the import or first-read time only, no workshop type, so both callers share it, and the `merge` vector cases carry no workshop field. Proof: `governor_vectors.rs::merge_cases_match`; story 7.21's tests.
10. **The window's corrections as built** (D-1210). The two corrections show in story 7.12's `TemplateEvidence.svelte` as pointer-only buttons whose labels keep their box; their Playwright cases run in both themes. Proof: story 7.12's `governor.spec.ts` cases; a `stable-size.spec.ts` case for the correction buttons.

## Tasks

- [ ] `crates/officina-core/src/governor/merge.rs`, `merge` section (AC 1)
- [ ] `crates/officina-host/src/governor/git.rs`: thread `governor-git`, the merge job, `AuthorRequest` variants (AC 2, 5)
- [ ] Fixture repositories and `governor_merge.rs` (AC 3, 4)
- [ ] `JournalKind::MergeSettled` (AC 5)
- [ ] `correct_merge` writer, command, capability, `ipc.ts`, `e2e-mock.ts` (AC 6)
- [ ] Zero-write case; `CHANGELOG.md`, README (AC 7, 8)

## Dev Notes

- Reworked 2026-11-02 (D-1202, D-1203, D-1207, D-1214): the addendum's `GitRule::Workshop` is `workshop::git::rule_args` on `main`; corrections are written with `append` before the row (D-1207), outcomes with `record`. The story needs no gate, so it may land as soon as story 7.4 and v0.6's Import have merged. Worktrees of work orders are story 7.21's, on the same rules.
- Requirements: FR-1 as amended (amendment 1), FR-17's per-template outcomes, PRD section 7's Merge detection row, section 8 question 10. Decisions: AD-3 as amended (thread `governor-git`), AD-111 (`GitRule::Workshop`), AD-127, AD-130, AD-142. Risks: R42, R78. Epic decisions: D-22, D-23, D-28 (Epic 2); D-92 (Epic 3); D-135, D-154, D-163 (Epic 5); D-219 (Epic 6); D-300, D-305, D-312, D-339, D-340.
- Seams (D-300, D-1202): host `git.rs` and `workshop/git.rs` (6.3, as built), the `workshop_run` rows (6.4, 6.16 for the verdict and branch), the hourly pass (3.2), the collector's `AuthorRequest` (3.5), D-154's marker tests (5.10).
- Waits for: story 7.4 and v0.6's Import (story 6.16); no gate (D-1203). If story 7.21 merged first, `merge::classify`, the `merge` section and the `governor-git` thread are already there and this story adds the import job, the corrections and its real-git cases.
- T, C and B come from the registry as host-validated values (D-219's rule); a remote name comes from repository configuration any session can write, so it is checked before it joins a reference name. Nothing is passed to git from a worker file.
- Only the author's references decide: the worker never wrote the project repository (v0.6 FR-12), and this pass writes nothing either.
- The pass is useful in Slice A on clicked runs: v0.6 imports exist before any autonomous run, and section 7's Merge detection row is a Slice A row.

## Testing

- Core unit and integration: `crates/officina-core/src/{governor/merge,relay/journal}.rs`, `crates/officina-core/tests/governor_vectors.rs`.
- Host unit: `crates/officina-host/src/governor/{git,writers}.rs`; host integration: `crates/officina-host/tests/governor_merge.rs` (real git).
- CLI integration: `crates/officina/tests/{watch,quota_doctor}.rs`. App crate: `app/src-tauri/src/main.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-11-02 reworked against v0.4 to v0.6 as built: git calls as built (D-1202); journal writes per D-760 (D-1207); worktrees of work orders covered through story 7.21 on the same rules (D-1214, AC 9); corrections' UI constraints (D-1210, AC 10); no gate (D-1203)
