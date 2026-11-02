# Story 7.21: Worktrees of work orders, merged and left behind

**Status:** Approved

## Story

As the author whose work order sessions sometimes make a git worktree of their own under the project and leave it there, locked, on a branch I must merge myself,
I want Officina to read, without writing anything, whether that branch reached my project's branch and whether the worktree is still there, and to show me the exact commands to clean it up,
so that a finished work order does not leave a forgotten branch and folder behind, and I never wonder where its changes went (`epic-5-decisions.md` D-1002, D-1214).

## Acceptance Criteria

1. **Rows read** (D-1214). A v0.5 host work order (`work_order.runner = 'host'`) is read when its bound session worked in another folder than the order's (`WorkOrderView.session_folder`, D-1002) and that session is no longer live (its card is gone or in Done). The folder is untrusted and used only as a key: the pass runs `git worktree list --porcelain` in the project's folder and reads a row only when one listed worktree's path equals the session folder under `quota::normalise_path`; otherwise the row is `pending` with the reason `not_a_worktree` in the view, and nothing more is read. Proof: host integration `crates/officina-host/tests/governor_worktree.rs::{a_finished_order_in_a_worktree_is_read, a_live_session_is_not_read, a_folder_git_does_not_list_is_not_read}` (real git, D-23).
2. **Facts, read-only** (D-1214, D-1110, D-312). On thread `governor-git` (story 7.5), with host `git.rs`, D-173's resolution of `git` (registry, never `PATH`, D-1110), `workshop::git::rule_args` and `GIT_OPTIONAL_LOCKS=0`: the worktree's path, HEAD, branch and lock state; at first read, the project's branch B (`symbolic-ref --quiet HEAD` in the project's folder), the worktree's tip T and C = `merge-base T B`, stored once in `worktree_check` (story 7.4); then, as for imports, B's local and remote-tracking references, `merge-base --is-ancestor` and `git cherry`. A branch or remote name passes core's ref-component check before it joins a reference; a detached worktree is read on its HEAD. The pass never runs `fetch`, `prune`, `worktree remove`, `worktree unlock` or `branch`. Proof: `governor_worktree.rs::{no_write_command_is_ever_run}` (the `officina-git-logger` fake records every argument vector, D-154's pattern), `{a_detached_worktree_is_read_on_its_head, a_branch_name_failing_the_ref_check_is_skipped}`.
3. **Outcomes** (D-1214, story 7.5's rules). Core `merge::classify` gives `merged`, `merged_changed`, `merged_equivalent`, `not_merged` (the branch gone, or 14 days since the first read) or `pending`, and `worktree::outcome` adds `no_commits` when T equals C; a final outcome is written once with `settled_at_ms` and recorded `worktree_settled` (work order, outcome, source) through `relay::journal::record` (D-1207). Proof: `merge` and `worktree` cases in `tests/vectors/governor.json` through `crates/officina-core/tests/governor_vectors.rs::{worktree_cases_match, every_worktree_outcome_has_a_case}` (iterating `WorktreeOutcome::ALL`); real-git cases `governor_worktree.rs::{merged_by_fast_forward, merged_through_a_remote_tracking_branch, squashed_stays_pending_until_corrected, nothing_committed_is_no_commits, branch_deleted_is_not_merged}`.
4. **Cleanup state** (D-1214). While the worktree is listed, the pass reads hourly, for at most 90 days, `present`, `locked` or `removed`, and whether the branch still exists; a removed worktree with its branch deleted is read no more. Proof: `governor_worktree.rs::{a_locked_worktree_reads_locked, a_removed_worktree_reads_removed, reading_stops_once_both_are_gone}`.
5. **The commands, never run** (D-1214, R130). While the worktree is listed and the outcome is `merged`, `merged_changed`, `merged_equivalent` or `no_commits`, core `worktree::cleanup_commands` returns the lines the author runs himself: `git -C <project> worktree unlock <folder>` when locked, `git -C <project> worktree remove <folder>`, and `git -C <project> branch -d <W>` when the branch exists; it returns none unless every name comes from `git worktree list` and passes core's strict character set (letters, digits, `-`, `_`, `.`, `/`, and `:` and `\` inside a path), so a crafted name never becomes shell text. Officina runs none of them. Proof: `worktree` vector cases, among them a branch named with `;`, `$(...)`, a backtick, a space and a quote, each giving no command; `crates/officina-core/src/governor/worktree.rs::tests::a_not_merged_branch_gets_no_command`.
6. **Corrections** (D-28, story 7.5's rule). `correct_worktree(work_order_id, merged | not_merged)`, pointer only, on a `pending` or `not_merged` row of source `rules`, written with source `author` and final, appended before the row is written (D-1207); anything else is refused `correction_refused`. The command is declared in `build.rs`, allowed in `capabilities/default.json`, a thin wrapper over `AuthorRequest` with a deadline (D-92), wrapped in `ipc.ts` and answered by `e2e-mock.ts`. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::{a_worktree_correction_on_a_pending_row_is_final, a_worktree_correction_on_a_merged_row_is_refused}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
7. **In the window** (D-1214, D-1210). `WorkOrderView` gains, for such an order, the outcome, B, W, the cleanup state and the commands; `ORCHESTRATOR_VIEW_SCHEMA` goes up by one relative to `main` and the TypeScript bindings are regenerated. `WorkOrderCard.svelte` and `WorkOrderHistory.svelte` extend D-1002's "Worked in <folder>" line with "branch <W>: merged into <B>" (or the outcome), the cleanup state and the commands as selectable text in a fixed-width block, plus the two corrections where they apply (pointer only); nothing here uses the attention colour, since none of it waits on the author; every element whose text or presence changes keeps its box. Proof: Vitest `app/src/components/components.test.ts > WorkOrderCard worktree line` (each outcome, commands present or absent); Playwright `app/e2e/orchestrator.spec.ts > a merged worktree shows its cleanup commands ({light,dark})`, `> a correction ignores a keyboard click`, `> a crafted branch name shows no command`; `stable-size.spec.ts` cases for the line in each state, on fixtures from `crates/officina-host/examples/orchestrator_fixture.rs`.
8. **When** (D-1214). The pass runs hourly and when the Orchestrator view asks for its view if the last pass is older than 5 minutes; it never runs on the collector thread and the board never waits. Proof: CLI-crate collector test `crates/officina/tests/watch.rs::the_board_keeps_its_pace_while_a_worktree_pass_hangs`.
9. **No program runs, nothing is written** (D-312's pattern, D-163). The neutralised-configuration cases of story 7.5 (external diff, textconv, fsmonitor, a filter driver) also run on a worktree pass; the project's references, index, work tree and its worktrees' administrative files stay unchanged. Proof: `governor_worktree.rs::{no_configured_program_runs, a_pass_leaves_the_repository_and_its_worktrees_unchanged}`; `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` with a worktree pass.
10. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: worktrees of work orders read, with their cleanup commands. README: the "Worktrees" paragraph of the work orders section replaces "noticing what was merged is v0.7's merged detection" with what the card now shows, that Officina never removes a worktree or a branch, and that the commands are shown only for names it can show safely. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/governor/worktree.rs`: outcome, cleanup commands, the character set; `worktree` section (AC 3, 5)
- [ ] The worktree job on `governor-git`; rows, first read, cleanup state (AC 1, 2, 4, 8)
- [ ] `correct_worktree` writer, command, capability, `ipc.ts`, `e2e-mock.ts` (AC 6)
- [ ] `WorkOrderView` fields, schema bump, bindings; the card and the history (AC 7)
- [ ] Fixture repositories with `git worktree add`, locked and detached cases (AC 1 to 4, 9)
- [ ] `CHANGELOG.md`, README (AC 10)

## Dev Notes

- Requirements: `epic-5-decisions.md` D-1002 (left to v0.7's merged detection); PRD v0.7 FR-1's rules applied to a second kind of branch; PRD section 5 (no write to the project repository beyond v0.6's Import). Decisions: AD-130, AD-142. Risks: R42, R78, R130. Epic decisions: D-22, D-23, D-28 (Epic 2); D-92 (Epic 3); D-134, D-154, D-163, D-173, D-1002 (Epic 5); D-1110 (Epic 6); D-312, D-1202, D-1203, D-1207, D-1210, D-1214.
- Seams (D-1202): `crates/officina-core/src/orders/view.rs` (`WorkOrderView.session_folder`, `ORCHESTRATOR_VIEW_SCHEMA`), `crates/officina-host/src/git.rs`, `crates/officina-host/src/relay/tree.rs` (D-173's `resolve_git`), `crates/officina-host/src/workshop/git.rs` (`rule_args`), `app/src/components/{WorkOrderCard,WorkOrderHistory}.svelte`, `crates/officina-host/examples/orchestrator_fixture.rs`.
- Waits for: story 7.4 (`worktree_check`). It shares story 7.5's `merge::classify`, its `merge` vector section and the `governor-git` thread: whichever of 7.5 and 7.21 merges first builds them, with 7.5's AC 1 cases, and the other reuses them. No gate (D-1203) and no v0.6 seam beyond migration 3, which is on `main`: it can land before v0.6 closes, while 7.5's imports wait for story 6.16.
- Why `git worktree list` and not the session's folder: the folder is what the session said, so it could name any path; only a worktree git itself lists is read, and every name in a shown command comes from git.
- Why commands and not a button: removing a worktree and deleting a branch write the project repository, which PRD section 5 leaves to the author; `git branch -d` refuses an unmerged branch, so the shown line cannot lose work.
- Claude Code's own cleanup of an interactive session's worktree is untouched: this story only reads what is left.
- This never feeds acceptance: host templates are never launched by the governor (FR-10).
- No employer content in fixtures: placeholder projects, branches and folders.

## Testing

- Core unit and integration: `crates/officina-core/src/governor/worktree.rs`, `crates/officina-core/tests/governor_vectors.rs`.
- Host unit: `crates/officina-host/src/governor/{git,writers}.rs`; host integration: `crates/officina-host/tests/governor_worktree.rs` (real git).
- CLI integration: `crates/officina/tests/{watch,quota_doctor}.rs`. App crate: `app/src-tauri/src/main.rs`.
- Vitest: `app/src/components/components.test.ts`; Playwright: `app/e2e/orchestrator.spec.ts`, `app/e2e/stable-size.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-11-02 created from D-1214 (merged detection extended to the worktrees of work orders, D-1002)
