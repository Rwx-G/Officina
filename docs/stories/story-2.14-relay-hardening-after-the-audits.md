# Story 2.14: Relay hardening after the Epic 2 audits

**Status:** Approved

## Story

As the author relying on the relay's dry runs and approvals,
I want the checks that bind what runs to what I saw to hold against a session that controls its own environment and files,
so that a hijacked session cannot make an approved or checked action run different code, and cannot forge the window's decisions unnoticed.

## Acceptance Criteria

1. `git` is never resolved through the caller's `PATH` (D-173): the host resolves it from Git for Windows' install location in the registry (`HKLM`, then `HKCU`, `SOFTWARE\GitForWindows`, `InstallPath`, then `cmd\git.exe` under it), `.exe` only, and records the resolved path in the journal line of each fingerprint. When it is not found, an action that needs a fingerprint refuses `git_unavailable` and doctor fails with the reason. Proof: host `crates/officina-host/src/relay/tree.rs::tests::{git_is_resolved_from_the_registry_not_path, a_shim_first_on_path_is_not_used}` (a fake `git.exe` placed first on `PATH` must not run: its marker file stays absent), CLI `crates/officina/tests/run.rs::git_unavailable_refuses_actions_that_need_a_fingerprint`.
2. The fingerprint reads content, not git's stat cache (D-173): it lists tracked and unignored files with `git ls-files -z --cached --others --exclude-standard` (the resolved git, `GIT_*` removed, `-c core.fsmonitor=false`, no filters run), then hashes each file's bytes itself; a file over 64 MiB or a tree over 20,000 files or 512 MiB refuses `tree_too_large`. A content change with size and modification time restored, and a change hidden by a repository clean filter, both change the fingerprint. Proof: `tree.rs::tests::{restored_size_and_mtime_still_change_the_fingerprint, a_clean_filter_does_not_hide_a_change, limits_refuse_tree_too_large}`.
3. Actions run with an environment the caller does not control for program lookup (D-173): `officina run` builds the child's `PATH` from the machine and user `Path` values in the registry (expanded), not from its own environment, and drops the caller's `PATHEXT`, `WSLENV` and `ComSpec` (system values are used); the catalogue's `env` keys still apply. Proof: CLI `run.rs::{a_caller_path_shim_is_not_found_by_the_action, wslenv_from_the_caller_does_not_reach_wsl}` (fake tool logs its `PATH` and environment keys).
4. The gate denies, in a guarded project, an assignment of `PATH`, `PATHEXT`, `WSLENV`, `ComSpec` or any `GIT_` name in the same command as an `officina` command word, with deny kind `officina_environment` (belt and braces to AC 3). Proof: `tests/vectors/gate.json` cases, one per name, derived from the core constant by the coverage test.
5. Every `human` action in a project that is a git tree binds the tree (D-173): the request records the fingerprint (with or without a `check`), and `run` recomputes it before consuming the approval; a mismatch refuses `tree_changed` and leaves the approval unconsumed. The card says "tracked and unignored files unchanged since the request" (or "since the dry run" when a check is named). Outside a git tree the card says "no file binding: not a git tree". Proof: CLI `crates/officina/tests/approvals.rs::{a_human_action_without_check_is_bound_to_its_tree, an_edit_while_waiting_refuses_tree_changed}`, Vitest `components.test.ts > ApprovalCard states its binding`.
6. File tools cannot write the relay's state (D-173): once a project is guarded, the gate denies `Write`, `Edit`, `MultiEdit` and `NotebookEdit` calls whose target path (canonicalised as AC 8) lies under the Officina home, whatever the project, with deny kind `relay_state_write`. The `PreToolUse` matcher doctor prints and requires is built from the shell and file tool constants together. Proof: `crates/officina/tests/gate.rs::{write_into_the_relay_dir_is_denied, edit_of_the_journal_is_denied, write_elsewhere_gets_no_decision}`, `doctor.rs::tests::block_matcher_names_every_gated_tool`.
7. Forged decisions are surfaced (D-173): the window records the ids it decided; a `run_started` carrying an `approval_id` the window did not decide, an `approved.json` the window did not write, or a deleted `stopped` flag the window did not clear, is shown as an anomaly in the Relay view and fails doctor until acknowledged. Proof: host `crates/officina-host/tests/authority.rs::{a_foreign_approval_file_is_an_anomaly, a_run_without_a_window_decision_is_an_anomaly}`, Playwright `app/e2e/relay.spec.ts > anomalies are listed`.
8. Path aliases resolve before the gate (D-173): the hook canonicalises the payload `cwd`, `CLAUDE_PROJECT_DIR` and file tool targets with the final path of an opened handle (8.3 short names, junctions, symbolic links, `subst` drives) before the pure gate sees them; a path that cannot be opened keeps its normalised string. The gate budget (D-19) still holds. Proof: host `crates/officina-host/src/relay/gate.rs::tests::{short_name_resolves_to_the_guarded_project, junction_into_a_guarded_project_is_guarded}` (`#[cfg(windows)]`, junction and 8.3 name created in a temp folder, skipped with a message where 8.3 names are disabled), `perf.rs` green.
9. Forwarded values need an explicit opt-in (D-173, extends D-172): an action with a `RemoteCommandValue` or `TemplatedValue` notice whose parameter declares a `pattern` that admits whitespace, a shell character or a template character (probed with strings derived from one core constant) becomes an issue `ForwardedPattern` and is dropped, unless the action sets `forwarded_values = "accepted"`, a canonical key (format bump, approvals void once). A `values` list is always accepted. Proof: core `catalogue.rs::tests::{a_permissive_pattern_on_a_forwarded_value_is_an_issue, values_on_a_forwarded_value_is_fine, forwarded_values_accepted_keeps_the_action}`, probe strings iterated from the constant.
10. The webview CSP drops `'unsafe-inline'` from `style-src` when the built app needs none; if a style attribute or element still needs it, the story records which and keeps it. Proof: Playwright suite green under the new CSP; a test asserting the CSP string from `tauri.conf.json` matches the decision.
11. Documentation: the README relay section states that the session id on a card is reported by the session (advisory, not proof of origin), that the pointer-only rule is a guard against slips and not against code in the webview, and describes the bindings of AC 2 and AC 5 and their limits (files git ignores, programs outside the tree). Proof: `crates/officina/tests/readme.rs` checks the binding sentence names the refusal codes' constant.

## Tasks

- [ ] Host: git resolution from the registry; content-hashing fingerprint with limits (AC 1, 2)
- [ ] Host: action environment from the registry `Path`; dropped caller variables (AC 3)
- [ ] Core: gate deny kinds `officina_environment` and `relay_state_write`; file tools constant; vector cases (AC 4, 6)
- [ ] Host and core: tree binding for every `human` action; `tree_changed` refusal; card wording (AC 5)
- [ ] Window: decided-ids record, anomalies in the relay view and doctor (AC 7)
- [ ] Host: handle-based canonicalisation before the gate (AC 8)
- [ ] Core: `ForwardedPattern` issue, `forwarded_values` key, format bump (AC 9)
- [ ] App: CSP (AC 10); README and readme test (AC 11)
- [ ] Update `CHANGELOG.md` `[Unreleased]` (Security)

## Dev Notes

- Sources: `_bmad-output/planning-artifacts/qa/epic-2-penetration.md` (High: PATH-shimmed git, human action without content binding, forwarded values; Medium: forged decision files, path aliasing, client-side pointer rule) and `qa/epic-2-security.md` (Medium: git through `PATH`, clean/smudge filters and restored stat; Low: CSP, advisory session id). Both reports are git-ignored; this story carries what they found.
- The relay stays a guardrail, not a boundary (PRD v0.2 section 1): a session running as the author can still do anything the author's identity allows outside the relay. These fixes close the gaps on the relay's own controls.
- `Monitor`'s command field is `command` (its tool schema), so story 2.12's rule reads the right field.
- Decisions: D-173 (this story), D-170, D-171, D-172, D-22, D-27, D-28, D-29, D-19.

## Testing

- Core unit: `crates/officina-core/src/relay/{gate,catalogue}.rs`; vector set `tests/vectors/gate.json`.
- Host: `crates/officina-host/src/relay/{tree,gate}.rs`, `crates/officina-host/tests/authority.rs`.
- CLI: `crates/officina/tests/{run,approvals,gate,readme,perf}.rs`.
- Vitest and Playwright: `app/src/components/components.test.ts`, `app/e2e/relay.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created from the Epic 2 security and penetration audits (D-173)
