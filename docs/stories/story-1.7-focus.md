# Story 1.7: Focus

**Status:** Approved

## Story

As the author,
I want one click on a card to bring that session's window to the foreground, or open an attach for a background session,
so that I stop cycling through editor windows and terminal tabs.

## Acceptance Criteria

1. Candidate names come from `cwd`: last segment, then each parent segment up to but excluding the user home and the drive root; titles match case-insensitively on a whole-word boundary; the first name matching exactly one window wins; a single window owned by the ancestor wins outright; otherwise candidates are listed (AD-15 steps 2-3, FR-10). Proof: core unit tests `focus::tests::{last_segment_matches_one_window, subfolder_session_matches_workspace_window, partial_word_does_not_match, ambiguous_name_falls_through_to_parent, single_window_wins, no_match_lists_candidates, stops_before_home_and_drive_root}`.
2. The ancestor walk stops at 16 hops or a cycle and drops ancestors created after the child (AD-15 step 1). The walk logic is pure over a process map. Proof: core unit tests `focus::tests::{walk_stops_at_16_hops, walk_stops_on_cycle, walk_drops_younger_parent}`.
3. The attach argv is built from the session id only, validated against the FR-10 pattern; `cwd` must be an existing absolute directory, passed as the working directory (AD-15). Proof: core unit tests `attach::tests::{valid_id_gives_attach_argv, rejects_separators_quotes_spaces_and_unicode, rejects_empty_and_65_chars}`, host test `crates/officina-host/tests/attach.rs::relative_or_missing_cwd_fails`.
4. A background card click spawns `claude attach <id>` with `CREATE_NEW_CONSOLE`, the session `cwd`, the floor `CLAUDE_CONFIG_DIR`, unwaited, and returns `attach_opened`. Proof: host test `attach.rs::spawns_fake_claude_with_argv_cwd_and_config_dir` (fake claude extended to log its working directory).
5. On Windows, the platform module enumerates the process tree and top-level windows and focuses with the calls AD-15 lists; refusal flashes the window and returns `refused`; non-Windows builds return `Unsupported` (AD-1). Proof: host test `crates/officina-host/tests/process_tree.rs::walks_the_test_process_ancestors` (Windows); `core-linux` CI compiles `platform::other`. Real focus is manual (AC 10).
6. Tauri commands `focus_card(key)` and `report_wrong_window(key)` are `async` and use `spawn_blocking` (AD-3, AD-19). Proof: Rust unit test `commands.rs::tests::focus_card_unknown_key_returns_failed`.
7. The card shows the outcome for 10 s (focused, attach opened, candidates with their titles, no window, refused, failed with reason), offers "wrong window", and background cards offer "copy attach command" through `navigator.clipboard`. Proof: Playwright `app/e2e/focus.spec.ts > shows each outcome returned by focus_card` (iterating the generated outcome type), `> wrong window calls report_wrong_window`, `> copy attach command writes the attach line to the clipboard`.
8. Each click and each "wrong window" appends a line to `state/focus.jsonl` with the AD-8 shape, carrying a hashed card key and no path or text. Proof: host test `crates/officina-host/tests/focus_log.rs::lines_carry_no_path_or_title`.
9. The focus success rate is a pure function over outcome lines: first clicks with `focused` or `attach_opened` not followed within 10 s by "wrong window" or a second click on the same card (FR-10, AD-15). Proof: core unit tests `focus::metric::tests::{success_counts, wrong_window_within_10s_fails, second_click_within_10s_fails, events_after_10s_do_not_count}`.
10. The AD-15 manual checklist passes and each click result is recorded in `docs/research-notes.md`. Proof: manual, per AD-17 (no automation can judge "right window").

## Tasks

- [ ] `officina-core::focus` (candidate names, title match, ranking, ancestor walk over a map, metric) and `officina-core::attach` (argv) (AC 1, 2, 3, 9)
- [ ] `officina-host/src/platform/windows/` with module-level `#![allow(unsafe_code)]` and `// SAFETY:` per block; `platform/other.rs` (AC 5)
- [ ] `officina_host::focus::focus_session` dispatch: interactive vs background, outcome enum (AC 4, 5)
- [ ] Extend the fake claude to log its working directory (AC 4)
- [ ] `state/focus.jsonl` appender (AC 8)
- [ ] Tauri commands, capability entries, ts-rs export of the outcome type (AC 6, 7)
- [ ] Card outcome display, wrong-window and copy buttons; store keeps outcomes per card for 10 s (AC 7)
- [ ] Tests listed in the ACs (AC 1-9)
- [ ] Run the manual checklist, record results (AC 10)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Requirements: FR-10 (as amended by spine section 5, items 1 and 2). Decisions: AD-1 (unsafe exception), AD-3, AD-8 (focus log), AD-15 (everything), AD-19. Facts: E8 (process chain, `MainWindowTitle` is wrong in the multi-window case).
- Risks: R5 (custom titles, subfolders, cloaked windows), R6 (`SetForegroundWindow` refused).
- No text from prompts or messages may reach a process argument: the argv builder takes a validated id and nothing else.
- The child is not tracked (Officina owns no session process).
- `CREATE_NEW_CONSOLE` on a CI runner: if the runner refuses a console, gate the host test on an env var set by the `windows` job and note it.

Manual checklist (AD-15):
- [ ] Two VS Code windows on two repositories, one session each: each card focuses its own window.
- [ ] One session started in a subfolder of an opened workspace.
- [ ] One minimized window: restored and focused.
- [ ] One window on another virtual desktop: outcome recorded.
- [ ] Windows Terminal standalone session.
- [ ] One background session: attach opens in the default terminal with the right config dir.

## Testing

- Core unit: `crates/officina-core/src/{focus,attach}.rs`.
- Host integration: `crates/officina-host/tests/{attach,process_tree,focus_log}.rs`.
- Rust app unit: `app/src-tauri/src/commands.rs`.
- Playwright: `app/e2e/focus.spec.ts`.
- Manual: checklist above, research notes.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
