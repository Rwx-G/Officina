# Story 1.12: Window polish

**Status:** Approved

## Story

As the author with the board behind a maximized editor,
I want a single window that signals waiting sessions from the taskbar, answers to the keyboard and stays readable with many sessions,
so that "who needs me" costs a glance or a keystroke.

## Acceptance Criteria

1. A second launch of `officina-app` focuses the existing window and starts no second collector (AD-19). Proof: manual checklist item 1 (no Tauri runtime test layer exists in AD-17); the single-instance plugin wiring is reviewed in the PR.
2. The window title is `Officina` with the waiting count appended when it is above zero, and the taskbar shows the amber overlay when it is above zero. The exact Tauri overlay API is checked first; if absent, only the title changes (AD-19, R13). Proof: Rust unit test `app/src-tauri/src/window.rs::tests::title_reflects_waiting_count`; overlay by manual checklist item 2; the API finding is recorded in Completion Notes.
3. Keyboard: `1`-`9` focus the nth Needs-you card, arrows move the selection, `Enter` focuses the selected card, `w` reports wrong window, `r` refreshes (spine section 3 item 2). Proof: Playwright `app/e2e/keyboard.spec.ts > {digit focuses nth needs-you card, arrows move selection across columns, enter focuses selection, w reports wrong window, r calls refresh}` asserting the mocked IPC calls.
4. An always-on-top toggle in the header uses the core permission `core:window:allow-set-always-on-top` (AD-19, spine section 3 item 9). Proof: Playwright `app/e2e/header.spec.ts > always-on-top toggle invokes set_always_on_top`; capability test from story 1.6 still green.
5. Done collapses beyond 5 cards with a count; Needs you never collapses (spine section 3 item 5). Proof: Playwright `app/e2e/collapse.spec.ts > {done collapses beyond 5 with count, expanding shows all, needs you with 12 cards never collapses}` on fixtures generated with many sessions.
6. The stale banner shows how long the board has been stale and disappears on the next fresh board. Proof: Playwright `app/e2e/board.spec.ts > stale banner shows its age and clears on a fresh push` with `page.clock`.
7. The app runs the retention pass at startup and hourly: event files per AD-9, focus log lines older than 30 days per AD-8. Proof: host tests `crates/officina-host/tests/retention.rs::{prunes_event_files_before_yesterday, drops_focus_lines_older_than_30_days}` and planner unit test `collector::plan::tests::retention_runs_at_start_and_hourly`.
8. Idle CPU stays under 1 % with 8 real sessions (PRD section 7, R1). Proof: manual, Task Manager over 10 minutes, recorded in research notes (AD-17).

## Tasks

- [ ] Add `tauri-plugin-single-instance` and wire focus-on-second-launch (AC 1)
- [ ] Check the overlay API in the pinned Tauri version; implement title and overlay or title only (AC 2)
- [ ] Keyboard handler in the board store; selection state (AC 3)
- [ ] Always-on-top toggle and capability entry (AC 4)
- [ ] Done collapse in `Column` (AC 5)
- [ ] Stale age in `StaleBanner` (AC 6)
- [ ] Retention pass in host and its schedule in the collector planner (AC 7)
- [ ] `e2e:fixtures` scenarios for many sessions and keyboard (AC 3, 5)
- [ ] Manual checklist and CPU measurement (AC 1, 2, 8)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Decisions: AD-8 (focus log retention), AD-9 (event retention), AD-19 (single instance, title, overlay, always-on-top), spine section 3 items 1, 2, 3, 5, 9. Risks: R1, R13.
- The stale banner itself exists since story 1.6; this story adds its age and the recovery test.
- Keyboard shortcuts must not fire while focus is in a text field or the candidates list of a card.
- Rejected for v0.1 (spine section 3): tray icon, notifications, global hotkey, position memory, autostart. Do not add them here.

Manual checklist:
- [ ] Launch twice: one window, one collector (one `claude agents` spawn cadence observed).
- [ ] Waiting session present: taskbar overlay and title count visible with the window behind the editor; both clear when it is answered.
- [ ] Idle CPU with 8 sessions over 10 minutes.

## Testing

- Rust app unit: `app/src-tauri/src/window.rs`.
- Host integration: `crates/officina-host/tests/retention.rs`; planner unit tests.
- Playwright: `app/e2e/{keyboard,header,collapse,board}.spec.ts`.
- Manual: checklist above.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
