# Story 4.1: Slice 0: sub-agent signals, the taskbar on Windows, what a real wait emits, and the floor and window to draw for

**Status:** In progress (tooling done; gates 1, 2, 4 and 5 are the owner's; see Completion Notes)

## Story

As the author who approved a scene with an alert, a taskbar badge and sub-agents beside their lead,
I want the facts it rests on checked on my workstation before the code that depends on them,
so that the scene draws sub-agents only from signals that exist, the badge is legible on my taskbar, the alert escalates on the waits I really have, and the floor and its scale fit my projects and my screen.

## Acceptance Criteria

1. Gate 1, sub-agent signals (FR-12, AD-176, AD-178 row 1, D-476, D-479, D-507): in a trusted scratch folder, with a logging hook the owner adds to that folder's `.claude/settings.local.json` on `SubagentStart`, `SubagentStop` and `UserPromptSubmit`, the seven cases of AD-178 are run (one Explore sub-agent; two in parallel; one in the background outliving the lead's turn; one from a `claude --bg` session; a nested one if the version allows; a `/btw` side question; an agent-team teammate handling two messages), and the entry records, redacted, for each case: the Claude Code version, each payload's `hook_event_name`, `session_id`, `agent_id`, `agent_type`, whether `SubagentStart.session_id` equals the lead's `UserPromptSubmit.session_id`, and, for the teammate, how many starts and stops one `agent_id` produced. The entry ends with **pass** (every start carries the lead's session id, pairs with one later stop by `agent_id`, and the `/btw` stop has no start) or **fail**, and names story 4.12's fate (D-476). Proof: the research-notes entry.
2. Gate 2, the taskbar (FR-11, AD-174, AD-178 row 2): `app/src-tauri/examples/taskbar_probe.rs` opens one window and, with no collector and no Officina home, sets overlays for 1, 9 and "9+", amber then alert colour, at 16, 24 and 32 px, five seconds each, then waits 5 s for the owner to focus another window and calls `request_user_attention(Some(UserAttentionType::Informational))`, then calls it again while focused. The overlay images come from the function story 4.5 ships (`overlay_badge(count, alert, size)`), introduced here in `app/src-tauri/src/taskbar.rs` with its unit tests. The entry records, at 100 % and 150 % display scaling with a light and a dark taskbar, which size is legible, whether the flash runs until focus, and that nothing happens when focused, with the Windows build and the Tauri version. Proof: the research-notes entry; `cargo test -p officina-app taskbar` (`overlay_badge_draws_each_count`, `overlay_badge_uses_the_alert_colour`, `nine_plus_above_nine`).
3. Gate 3 is withdrawn (D-500): no prototype measurement runs; walking's default is AD-71's to confirm in story 4.14. Proof: AD-178 row 3; this story's Completion Notes say so.
4. Gate 4, what a real wait emits (FR-10, AD-178 row 4, D-499, R101): in the trusted scratch folder, with gate 1's logging hook extended to `Notification`, `Stop` and `PreToolUse` lines with their `ts`, and the folder's settings in `default` permission mode (pasted by the owner), an interactive session is asked a question through `AskUserQuestion` and then a command that raises a permission prompt; the entry records, with the Claude Code version, every event between the dialog showing and the answer, whether a blocking `Notification` (`permission_prompt`, `elicitation_dialog`) arrives for each, and how long after the dialog it arrives. It also counts, over the days the hook buffer on disk holds, the `Notification` lines per `notification_type` per day, from a read-only pass over the Officina home's `events/` folder. The entry ends with what story 4.3's vectors must carry: a question with or without a known wait start. Proof: the research-notes entry, dated before story 4.3's first commit.
5. Gate 5, the floor and the window (FR-1, FR-6, AD-61 and AD-169 pending points, AD-178 row 5, D-506): a read-only query of `registry.db` (`sqlite3 -readonly`, the query written in the entry) gives, per local day since the owner started using the registry on 2026-10-28, the distinct projects with a session live that day (from `first_seen_ms` to `last_seen_ms`) and the most projects live at one time; then, on the screen that will show the scene, the entry records the display resolution, the Windows scaling, the Officina window as the owner keeps it (maximised or its size), and, from the dev build's devtools, `innerWidth`, `innerHeight`, `devicePixelRatio` and the sessions area's client size, compared with AD-169's minimum sessions area (about 1108 x 688 CSS px at 4 columns). The entry ends by asking the owner D-506's two questions; his answers are recorded beside it. Proof: the research-notes entry and the owner's answers, dated before story 4.6 merges.
6. Order (D-479 as amended): gate 4 is recorded before story 4.3 starts; gate 5 and the owner's answers before story 4.6 merges; gate 2 before story 4.5 closes; gate 1 before story 4.12 starts. Proof: the dates of the entries against the stories' first and merge commits, checked in story 4.14's report.
7. No session or configuration is left behind: `claude agents --json` before and after each gate shows no added session; the owner removes the scratch folder's hooks and settings after gates 1 and 4; the probe writes nothing under the Officina home or the Claude config dir; gates 4 and 5 open the event buffer and the registry read-only. Proof: the entries; for the probe, its source (no path but the window).

## Tasks

- [ ] Gate 4: a question and a permission prompt in a `default`-mode folder, read with `slice0_probe wait`; the buffer count with `slice0_probe notifications`; the entry (AC 4, 7) (owner)
- [x] Gate 1 tooling (D-518): doctor registers `SubagentStart` and `SubagentStop`; the hook records `agent_id` and `agent_type`; board and registry skip the lines; `slice0_probe subagents` (AC 1)
- [ ] Gate 1: the owner pastes the hooks block again; sub-agent sessions, with the cases of AD-178 row 1 he chooses; the entry (AC 1, 7) (owner)
- [x] `app/src-tauri/src/taskbar.rs` with `overlay_badge` and its unit tests; `examples/taskbar_probe.rs` (AC 2)
- [ ] Gate 2: run the probe at 100 % (150 % as information, D-520), light and dark taskbar; the entry (AC 2, 7) (owner)
- [x] Gate 5 tooling: `slice0_probe projects` (AC 5)
- [x] Gate 5's questions answered by the owner (D-519, D-520)
- [ ] Gate 5: the counts after a few working days, and the window's figures from the dev build's devtools; the entry (AC 5, 7) (owner)
- [x] Record gate 3's withdrawal and every outcome in this story's Completion Notes (AC 3, 6); story 4.14 checks the order (AC 6)

## Dev Notes

- Requirements: FR-0 (checks first), FR-1, FR-6, FR-10, FR-11, FR-12. Decisions: AD-61, AD-169, AD-174, AD-176, AD-178. Epic decisions: D-460, D-476, D-479, D-494 to D-500, D-506, D-507.
- Dependencies: none. Gate 4 comes first because its findings can change story 4.3's rule 3 vectors; gate 5 needs a few working days of registry data, so it runs last, before story 4.6 merges.
- Gates 1 and 4 are the owner's to run: Officina never writes Claude configuration, so the logging hook and the scratch folder's permission mode are pasted by the owner into that folder only, and removed after. Use a tiny script that appends stdin with a timestamp to a file in the scratch folder; redact paths before recording. The owner's own sessions run `bypassPermissions`, so a permission prompt is only seen in the scratch folder's `default` mode.
- The docs (verified 2026-10-28) show `session_id` and the main `transcript_path` on `SubagentStop`, say a `SubagentStop` also fires for prompt suggestions and `/btw` without a start, and say `SubagentStart` fires each time an in-process teammate handles a new message: the `/btw` and teammate cases check both on this version.
- Gate 4 replaces the closure story's former "what a real wait emits" (D-499): moved here so that the time escalation is built on the waits that exist. The buffer count is the cheapest evidence of which Needs you causes are common in the owner's setup (the challenge's assumption (c)); it reads `notification_type` only, never message text.
- Gate 2's `overlay_badge` is the production function, not a probe copy: it draws a disc with a bitmap digit font in code (no asset), amber with dark digits or the alert colour with light digits, inside the dark rim of today's `waiting_dot`. Its colour constants are checked against `app.css` in story 4.5 (`derived-not-transcribed`). `set_badge_count` is unsupported on Windows (verified, AD-174), so it is not probed.
- The probe is an example of the app crate, run by hand, never in CI: it opens a window without the collector, the single-instance plugin or a floor, so it cannot touch the real home.
- Gate 5's numbers feed D-506: growth by columns stays as designed if more than four projects are typically live, otherwise four fixed offices; and a sessions area below the minimum makes the owner choose between scrolling and a scene-only viewing scale. Story 4.6 carries both outcomes, so the gate settles which one ships without a replan. The registry holds `session.project`, `first_seen_ms` and `last_seen_ms` (v0.3 migration 1); the query groups by local day.
- Nothing of the checks' tooling is committed beyond the probe and `taskbar.rs`: the entries state the commands run.
- No employer content in the entries: placeholder paths, project names and hosts only; the project count is a number, never a list of names.

## Testing

- Rust unit: `app/src-tauri/src/taskbar.rs` tests.
- Manual: four research-notes entries (gates 1, 2, 4, 5) and the owner's answers.

## Dev Agent Record

### Debug Log

- Gate 1's procedure changed (D-518): the sub-agent events are registered in doctor's hooks block from this story, so the owner pastes the block once and real use fills the buffer; no scratch-folder logging hook. The hook keeps `agent_id` and `agent_type` (64 characters) on those two events only, never `last_assistant_message`, `prompt` or a transcript path, whatever the excerpt setting. `group_events` (board) and the registry's ingestion skip the lines, so a lead's activity time, Needs you's order and the registry's sessions are unchanged whatever `session_id` they carry (if gate 1 fails and the id is the sub-agent's own, no phantom session appears). `HOOK_FIELDS` gains both events from the hooks reference, so the canary reports a field a version leaves out.
- `slice0_probe` (officina-host example) prints counts, event names, notification types and delays only: no prompt, message, path or project name, so its output pastes into an entry as is. Local days come from SQLite's `localtime`, so no time-zone code or `unsafe` was needed. It was run read-only on the owner's home on 2026-10-28 to check it works: no sub-agent line yet (the block is not pasted again yet), no `Notification` line yet, and the registry's first day.
- `overlay_badge` draws a 3 x 5 bitmap font scaled by `(size - 4) / 10` (1 at 16 px, 2 at 24 and 32 px) inside a disc whose rim is an eighth of the size, as today's 16 px dot. The window uses it from this story at 16 px (today's size) until gate 2 picks one; `waiting_dot` is gone. The probe includes `taskbar.rs` by path (the app crate is a binary) and clears the configuration's window so the board is not loaded.
- The probe was built (clippy on all targets) but not run: it opens a window and flashes the taskbar on the owner's desktop, which is the gate itself.

### Completion Notes

- **Gate 3** is withdrawn (D-500): no prototype measurement runs; walking's default is AD-71's walking run in story 4.14, against 1 % of the machine (D-515).
- **Gate 1** (open, owner): run `officina doctor`, paste the hooks block it prints into the user settings (it now holds `SubagentStart` and `SubagentStop`), then use sessions that spawn sub-agents, with whichever of AD-178 row 1's cases the owner wants checked (two in parallel, one in the background, a `claude --bg` session, a nested one, a `/btw` side question, an agent-team teammate). Then `cargo run -p officina-host --example slice0_probe -- subagents` prints the figures for the entry; the entry adds the Claude Code version and ends with pass or fail (story 4.12's entry condition).
- **Gate 2** (run 1 recorded, research notes E80; run 2 open, owner): the flash passes; the badge as first drawn failed (blurred above 16 px, thin at 16 px), so D-523 redraws it at the small icon's size with larger digits and a `!` on the alert badge, and the probe now shows that design. To do: rerun `cargo run -p officina-app --example taskbar_probe` at 100 % scaling with a light and a dark taskbar (150 % as information, D-520). The probe's window says what the taskbar shows at each step and takes the owner's answers by click; at the end it prints them with the display scale, the Windows build and the Tauri version, and appends them to `taskbar-probe-report.md` beside its executable, which is the entry's content (the legible size, whether the flash runs until focus, that nothing happens when focused). The app crate's build script links the Common Controls v6 manifest into examples, without which the probe stopped at start with `STATUS_ENTRYPOINT_NOT_FOUND`.
- **Gate 4** (open, owner): in a scratch folder whose `.claude/settings.local.json` the owner sets to `default` permission mode (Officina writes no Claude configuration), an interactive session is asked a question through `AskUserQuestion`, then a command that raises a permission prompt; `slice0_probe wait <session id>` prints the session's events with their delays (the id is on the board card); `slice0_probe notifications` counts the `Notification` lines per local day and type over the buffer's days. Story 4.3 is built for both shapes of a question (with and without a blocking notification) and leaves its gate-4 vector case open until the entry exists.
- **Gate 5**: the owner's answers are recorded (D-519: four to six projects live, eight the design target, growth by columns kept; D-520: 1920 x 1080 at 100 %, maximised, no scrolling with eight projects). Open: `slice0_probe projects` after a few working days (first reading on 2026-10-28: 4 distinct projects, 3 at once, one day of data, not yet the gate's number), and the window's `innerWidth`, `innerHeight`, `devicePixelRatio` and the sessions area's size from the dev build's devtools.
- **AC 7** holds for the tooling: the probe reads the buffer and opens the registry with `SQLITE_OPEN_READ_ONLY`; the taskbar probe has no collector, no single-instance lock and no floor.
- AC to test: AC 1 tooling: `events.rs::tests::{subagent_fields_are_kept_and_capped, subagent_texts_and_paths_are_never_kept}`, `board.rs::tests::subagent_lines_leave_the_session_state_unchanged`, `doctor.rs::tests::{subagent_events_are_buffered, every_buffered_event_has_hook_fields}`, `registry_ingest.rs::subagent_lines_touch_no_session_record`, `hook.rs::a_subagent_start_and_stop_are_appended`; AC 2: `taskbar.rs::tests::{overlay_badge_draws_each_count, overlay_badge_uses_the_alert_colour, nine_plus_above_nine}`; AC 3: this note; AC 1, 2, 4, 5 entries: open (owner).

## File List

- `crates/officina-core/src/events.rs`, `board.rs`, `contract.rs`, `registry/fold.rs`
- `crates/officina-host/src/doctor.rs`, `src/registry/ingest.rs`, `tests/registry_ingest.rs`, `examples/slice0_probe.rs`, `Cargo.toml`
- `crates/officina/tests/hook.rs`
- `app/src-tauri/src/taskbar.rs`, `src/main.rs`, `examples/taskbar_probe.rs`
- `README.md`, `CHANGELOG.md`

## Change Log

- 2026-10-28 created with the scene v2 rework (D-479)
- 2026-10-28 reworked after the plan challenge (D-499, D-500, D-506, D-507): gate 3 withdrawn; gate 4 (what a real wait emits, moved from the closure story) and gate 5 (project count, window size and scaling) added; gate 1 aligned on AD-178 and given a teammate case; story numbers per D-498
- 2026-10-28 implemented the tooling: sub-agent events registered and recorded (D-518), `slice0_probe`, `overlay_badge` and `taskbar_probe`; the owner's answers to gate 5's questions recorded (D-519, D-520). Gates 1, 2, 4 and 5's entries stay open for the owner. Status in progress
- 2026-10-28 the taskbar probe asks the owner each step by click and writes the gate 2 report; the examples link the Common Controls v6 manifest (owner finding)
- 2026-10-28 gate 2 run 1 recorded (research notes E80) and its outcome decided (D-523): the badge redrawn, the probe follows; run 2 open
