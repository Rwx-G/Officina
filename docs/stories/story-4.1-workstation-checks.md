# Story 4.1: Slice 0: sub-agent signals, the taskbar on Windows, and the cost of walking

**Status:** Approved

## Story

As the author who approved a walking scene with a taskbar badge and sub-agents beside their lead,
I want the three facts it rests on checked on my workstation before the code that depends on them,
so that the scene draws sub-agents only from signals that exist, the badge is legible on my taskbar, and walking ships on only if it is cheap.

## Acceptance Criteria

1. Gate 1, sub-agent signals (FR-12, AD-176, AD-178 row 1, D-476, D-479): in a trusted scratch folder, with a logging hook the owner adds to that folder's `.claude/settings.local.json` on `SubagentStart`, `SubagentStop` and `UserPromptSubmit`, the six cases of AD-178 are run (one Explore sub-agent; two in parallel; one in the background outliving the lead's turn; one from a `claude --bg` session; a nested one if the version allows; a `/btw` side question), and the entry records, redacted, for each case: the Claude Code version, each payload's `hook_event_name`, `session_id`, `agent_id`, `agent_type`, and whether `SubagentStart.session_id` equals the lead's `UserPromptSubmit.session_id`. The entry ends with **pass** (every start carries the lead's session id and pairs with one later stop by `agent_id`) or **fail**, and names story 4.9's fate (D-476). Proof: the research-notes entry.
2. Gate 2, the taskbar (FR-11, AD-174, AD-178 row 2): `app/src-tauri/examples/taskbar_probe.rs` opens one window and, with no collector and no Officina home, sets overlays for 1, 9 and "9+", amber then alert colour, at 16, 24 and 32 px, five seconds each, then waits 5 s for the owner to focus another window and calls `request_user_attention(Some(UserAttentionType::Informational))`, then calls it again while focused. The overlay images come from the function story 4.8 ships (`overlay_badge(count, alert, size)`), introduced here in `app/src-tauri/src/taskbar.rs` with its unit tests. The entry records, at 100 % and 150 % display scaling with a light and a dark taskbar, which size is legible, whether the flash runs until focus, and that nothing happens when focused, with the Windows build and the Tauri version. Proof: the research-notes entry; `cargo test -p officina-app taskbar` (`overlay_badge_draws_each_count`, `overlay_badge_uses_the_alert_colour`, `nine_plus_above_nine`).
3. Gate 3, walking CPU (FR-9, AD-178 row 3, D-480): `_bmad-output/planning-artifacts/design/prototype/perf-run.mjs` (git-ignored, not committed) gains two pages, the prototype v2 in v0.4 mode running `busy` at 1x for 600 s with walking on and with walking off, measured as section 1 of the architecture (GPU path, % of one core, % of the machine, main busy, recalcs, the core count and refresh rate). The entry records both runs and the static baseline, and ends with **pass** (walking under 1 % of the machine) or **fail**, naming the default story 4.6 ships (D-480). Proof: the research-notes entry.
4. Order (D-479): gate 1 is recorded before story 4.9 starts; gate 3 before story 4.6 merges; gate 2 before story 4.8 closes. Proof: the dates of the entries against the stories' merge commits, checked in 4.12's report.
5. No session or configuration is left behind: `claude agents --json` before and after each gate shows no added session; the owner removes the scratch folder's hook after gate 1; the probe writes nothing under the Officina home or the Claude config dir. Proof: the entries; for the probe, its source (no path but the window).

## Tasks

- [ ] Gate 1: the owner's scratch folder and logging hook; the six cases; the entry (AC 1, 5)
- [ ] `app/src-tauri/src/taskbar.rs` with `overlay_badge` and its unit tests; `examples/taskbar_probe.rs`; run it; the entry (AC 2, 5)
- [ ] Gate 3: the two pages in the prototype's harness; the two 10-minute runs; the entry (AC 3)
- [ ] Record the outcomes in this story's Completion Notes and in 4.12's checklist (AC 4)

## Dev Notes

- Requirements: FR-0 (checks first), FR-9, FR-11, FR-12. Decisions: AD-174, AD-176, AD-178. Epic decisions: D-460, D-476, D-479, D-480.
- Gate 1 is the owner's to run: Officina never writes Claude configuration, so the logging hook is pasted by the owner into the scratch folder only, and removed after. Use a tiny script that appends stdin to a file in the scratch folder; redact paths before recording. The docs (verified 2026-10-28) show `session_id` and the main `transcript_path` on `SubagentStop`, and say a `SubagentStop` also fires for prompt suggestions and `/btw` without a start: the `/btw` case checks the second fact on this version.
- Gate 2's `overlay_badge` is the production function, not a probe copy: it draws a disc with a bitmap digit font in code (no asset), amber with dark digits or the alert colour with light digits, inside the dark rim of today's `waiting_dot`. Its colour constants are checked against `app.css` in story 4.8 (`derived-not-transcribed`). `set_badge_count` is unsupported on Windows (verified, AD-174), so it is not probed.
- The probe is an example of the app crate, run by hand, never in CI: it opens a window without the collector, the single-instance plugin or a floor, so it cannot touch the real home.
- Gate 3 measures the prototype, not the app: it decides the default before the mover is written. AD-71's walking run in 4.12 measures the real window and can still flip the default (D-480).
- The prototype and its harness are git-ignored (`_bmad-output/`): nothing of gate 3's tooling is committed; the entry states the command run.
- No employer content in the entries: placeholder paths and names only.

## Testing

- Rust unit: `app/src-tauri/src/taskbar.rs` tests.
- Manual: three research-notes entries.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created with the scene v2 rework (D-479)
