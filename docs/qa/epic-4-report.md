---
title: Epic 4 QA report (v0.4, The Scene)
author: Rwx-G
date: 2026-10-28
---

# Epic 4 QA report

## Executive summary

Gate decision: **open**. The scene is built and covered by automated tests:
stories 4.2 to 4.13 are implemented, the full local gate replaying
every job of `.github/workflows/ci.yml` is green on the last commit, and
every rule the stories added has its case. Epic 4 cannot close on code
alone: its proof of FR-9 (CPU), of the real taskbar, of the live theme and
of the WebView2 look is a set of workstation runs that only the owner can
make (story 4.14 AC 6, 8 and 10; AC 7 passed; AC 5 measured, E82; AC 9 passed with a caveat, E87), and one Slice 0
gate still waits for the owner's real-use data (gate 5; gate 1 passed,
E83; gate 4 is recorded, E81). Story 4.12 (sub-agents) shipped once gate 1
passed: FR-12 is met.

What closes the epic, in order:

1. The owner's gate 5 entry (story 4.1 Completion Notes give the
   command). Gate 1 passed (E83, 2026-10-29) and story 4.12 shipped.
2. The CI snapshot run, then the owner's comparison of the light and dark
   baselines with the prototype v2's renders (D-508, stories 4.7 AC 8 and
   4.13 AC 5). Owner decision (2026-10-28): it waits until his visual
   fixes are done (story 4.15 is the first set).
3. The workstation runs of story 4.14 AC 6, 8 and 10, and AC 9's re-run of
   its first two floors (E87, D-701). AC 5 is measured
   (E82): the walking run failed and `DEFAULT_WALKING` is `false` (D-480);
   its passes were taken with the session locked, at about 170 frames per
   second, and a run on the unlocked screen would confirm them. Story 4.16
   then cut a walk's cost at its cause (D-550 to D-552); E82's runs 4 and 5
   run again on its release (E86, same locked session) put the walking run
   at 1.00 % of the machine (0.9985 %, and 1.007 % run again), walking's
   own cost from 1.4 % to 0.5 %: run 4 is not under 1 %, so D-553 left
   walking off. The owner then accepted walking's cost at the gate and
   turned it on by default (D-700, 2026-10-29).
4. The owner's review of the README assets (story 4.14 AC 2), waiting on
   the same visual fixes (owner decision); story 4.15 rendered them again.

Findings of the implementation, all decided:

1. The one-line header ran out of room: with the Walking toggle beside the
   theme control, eight projects with the alert band scrolled by 13 px on
   the owner's screen. The compact gauges are narrower (D-533).
2. The "relay stopped" chip's label read 4.15:1 in the light theme: it takes
   the danger colour (D-529, the owner to confirm the look).
3. Doctor's paste blocks were fragments to merge by hand; they are now the
   whole key, merged, never another key of the file (D-530, owner's
   decision).
4. Copying doctor's blocks from the Claude Code terminal cut their long
   lines (owner finding): each block is also written to a file under the
   Officina home that its line names (D-534).
5. Story 4.10 was finished by a sub-agent of the run that crashed and never
   reached main; it was recovered and replayed after the dark theme
   (D-531 records its choices).

## Evidence

Figures from the full local gate after story 4.17, on its code
(`995ca78` and the renamed `scene.spec.ts` cases), Rust 1.99.1, Windows 11
host; `core-linux` in a `rust:1-bookworm` container. The gate after stories
4.12 and 4.16 (`0effd0d`) read 1,114 Rust tests, 271 Vitest and 855
Playwright runs; after story 4.15 (`1e7183f`), 1,084, 257 and 840.

| Gate | Result |
|---|---|
| `cargo fmt --check`; clippy `-D warnings`, workspace with `officina-core/ts` | clean |
| Rust unit, integration, CLI end to end, doc tests, vector sets, TypeScript bindings drift | 1,114 passed, 0 failed, 11 ignored (the release-only perf, stress, crash and lexer tests below) |
| Perf, release (hook own cost, gate own cost, gate in process, appends) | 5 passed; hook own cost 12.5 ms against an empty process at 4.4 ms p95 (bound 150 ms with CI's budget); gate own cost 2.9 ms (bound 15 ms); gate in process p95 3.3 ms (a workstation check, not asserted) |
| Lexer against Git Bash; approvals stress; crash (release) | pass |
| `officina.exe` built alone: no authority or registry writer marker; both in `officina-app.exe` | pass |
| Registry dependencies, no bundled roots | pass |
| Frontend: svelte-check, ESLint, Vitest | 0 errors, clean, 276 passed in 25 files |
| e2e fixtures from the real CLI, Playwright `--repeat-each=3` | 960 passed, 6 skipped (the light and dark baseline cases, `test.fixme` until D-508), three times each |
| `cargo deny` (0.19.4), `pnpm audit` (11.5.3) | `cargo deny` ok (advisories, bans, licenses, sources); `pnpm audit` 1 low, ignored with its dated reason |
| `core-linux` in `rust:1-bookworm` | fmt and clippy clean, 982 passed, 0 failed, 9 ignored |
| Zero write under the Claude config dir | `quota_doctor.rs::no_command_writes_under_the_claude_config_dir` |

Stability: after story 4.12, `themes.spec.ts > the header logo follows
the theme` failed once in a three-times run under full load: its last
check read the logo's `complete` right after the source switched; it now
polls (`a5bc797`), and 150 runs of it passed. Before that, one failure across the Playwright
runs of the epic's closing work, `scene.spec.ts > a changed board places the figure at its
new place at once`, not reproduced in 1,057 further runs (story 4.10 Debug
Log); a single `ERR_CONNECTION_REFUSED` from the preview server, under a
Linux build running in parallel, not seen again; and one perf run where
the gate's own cost read 23.7 ms against its 15 ms bound while the empty
process it is compared with measured 29.6 ms against 12.3 ms for the
binary without the gate, process-start noise: the next run read 4.7 ms.

## Story status

| Story | Status | What is open |
|---|---|---|
| 4.1 Workstation checks | in progress | gates 1, 5 (owner); gate 4 recorded (E81) |
| 4.2 Floor keys, payloads, fixtures | review | |
| 4.3 Scene state in core | review | |
| 4.4 Halo | review | |
| 4.5 Alert band, title, taskbar | review | |
| 4.6 Geometry, spots, layout | review | |
| 4.7 Scene drawn in place | in progress | light baselines (CI run, owner comparison) |
| 4.8 Targets, keys, file, strip | review | |
| 4.9 Alert room and beacon | review | |
| 4.10 Walking | review | the walking CPU run failed (E82), then sat at 1.00 % after story 4.16 (E86); on by default, the owner accepting its cost at the gate (D-700) |
| 4.11 Focus mode and keys | review | |
| 4.12 Sub-agents | review | gate 1 passed (E83); a teammate, `/btw`, `--bg` and nested sub-agents not observed by name |
| 4.13 Dark theme | in progress | dark baselines (CI run, owner comparison) |
| 4.14 Docs, checks, QA | in progress | AC 2 review, AC 6, 8 and 10 (owner), AC 9's re-run of two floors, this report's closure; AC 5 measured (E82, E86), AC 7 passed, AC 9 passed with a caveat (E87) |
| 4.15 First-use feedback | review | the owner's next use of the real window |
| 4.16 Walking's cost | review | AC 6 measured (E86): the walking run at 1.00 % of the machine, not under 1 %; walking on by default by the owner's decision (D-700) |
| 4.17 Scene readability | review | the owner's two-second test on its screenshots passed with a caveat (story 4.14 AC 9, E87) |

## Slice 0 order (story 4.14 AC 11)

| Gate | Rule (D-479 as amended) | Recorded |
|---|---|---|
| 1 Sub-agent signals | before 4.12 starts | met: E83 (2026-10-29), pass, recorded before 4.12's first code commit |
| 2 Taskbar | before 4.5 closes | E80, passed (light taskbar, 100 %); 4.5 in review |
| 3 Walking cost | withdrawn (D-500) | AD-71's walking run in 4.14 AC 5: failed, E82 |
| 4 What a real wait emits | before 4.3 starts | **not met**, recorded late: E81 (2026-10-28) after 4.3 was built for both question shapes; a question emits `permission_prompt`, a known start, now a vector case; rule 3 unchanged |
| 5 Floor and window | before 4.6 merges | the owner's answers before 4.6 (D-519, D-520); the real-use counts open |

## Workstation runs (story 4.14 AC 5 to 10)

AC 5 measured (E82, E86), AC 7 passed, AC 9 passed with a caveat (E87); the others open, the owner's. The measurement home is ready:
`node app/scripts/e2e-fixtures.mjs --home <new folder> scene-measure`
prints the environment for `officina-app.exe`; `node
app/scripts/scene-replay.mjs --home <that folder>` drives one crossing
change every 10 s for 10 minutes. Start the window with
`WEBVIEW2_USER_DATA_FOLDER` set to a folder of that home as well, or the
run rewrites the installed window's stored view (E82).

| AC | Run | Result |
|---|---|---|
| 5 | Idle and walking CPU, five 10-minute runs, both units (1 % of the machine, D-515) | E82, 2026-10-29, session locked (about 170 frames per second): mean 0.49 % of the machine (11.8 % of one core) halos breathing throughout, 0.12 % (2.9 %) the two-minute rule, 0.066 % (1.6 %) halos steady, 0.58 % (14.0 %) replay walking off: pass; **1.95 %** (46.9 %) replay walking on: **fail**, D-480's fallback shipped (`DEFAULT_WALKING` false). After story 4.16, E86, same condition: replay walking on 0.9985 % (24.0 %) and, run again, **1.007 %** (24.2 %): not under 1 %; walking off 0.51 % (12.1 %); `DEFAULT_WALKING` stayed false (D-553), then the owner turned it on, accepting the cost at the gate (D-700) |
| 6 | Taskbar on the real app, 100 % and 150 % | open (gate 2 passed on the probe) |
| 7 | Live theme switch in WebView2 | **pass** (owner, 2026-10-28): the window followed a Windows theme switch on Auto, no restart |
| 8 | WebView2 look, 100 % and 150 %, light and dark, settled and walking | open |
| 9 | Readability, five fixtures, under 2 s | **pass, with a caveat** (owner, 2026-10-29, E87, D-701): 1920 x 1032, Scene, light, from `2a2fb94`, 2 s per floor in a random order. Count 5 of 5; oldest name 3 of 5, the two misses (`scene-subagents`, `scene-b`) the first two floors shown, before the owner knew what to look at, then 3 of 3. Re-run of those two floors in a random order pending; no scene change follows |
| 10 | Adoption day | open |

## Traceability

| Requirement | Stories | Tests |
|---|---|---|
| FR-0 Visual language and checks first | 4.1, 4.2 | `scene-fixtures.mjs` intents and index, the probes of 4.1 |
| FR-1 Zones with stable places | 4.3, 4.6, 4.7 | `tests/vectors/scene.json`, `scene_vectors.rs`, `layout.test.ts`, `places.test.ts`, `memory.test.ts`, `scene.spec.ts` |
| FR-2 Every waiting session is visible | 4.6, 4.8 | `layout.test.ts` (groups), `scene.spec.ts > the scene draws every live session of each fixture` |
| FR-3 Only state changes move | 4.4, 4.7, 4.10 | `halo.test.ts`, `halo.spec.ts`, `scene.spec.ts > nothing in the scene changes while the payloads are unchanged`, `walk.spec.ts`, `mover.test.ts` |
| FR-4 Instruments in the header | 4.7, 4.8 | `stable-size.spec.ts`, `scene.spec.ts` (strip, gauges) |
| FR-5 Files and actions | 4.8 | `scene.spec.ts` (acting from the scene), `components.test.ts > SessionFile` |
| FR-6 Keyboard and accessibility | 4.8, 4.11, 4.13 | `scene.spec.ts` (Tab, arrows, 44 px targets), `modes.spec.ts`, `tokens.test.ts` |
| FR-7 Modes | 4.7, 4.11 | `modes.spec.ts`, `view.test.ts` |
| FR-8 Dark theme | 4.13 | `themes.spec.ts`, `tokens.test.ts`, `alert.spec.ts > an alert changes no theme token` |
| FR-9 Performance | 4.10, 4.14, 4.16 | AC 5, E82: the halo rule and walking off within 1 % of the machine; walking on over it, so off by default. Story 4.16 AC 6, E86: walking on at 1.00 % of the machine after the cut (0.9985 %, 1.007 % run again), not under 1 %; walking off 0.51 %. **The owner accepted walking's cost at the gate** (D-700, 2026-10-29): the replay is deliberately harsh (one crossing walk every 10 s, 240 Hz ultrawide), so walking is on by default and the toggle turns it off (`view.test.ts`, `modes.spec.ts > walking is on by default`); the 1 % gate itself is unchanged (D-515) |
| FR-10 Alert | 4.3, 4.5, 4.9 | the alert sections of `scene.json`, `components.test.ts` (the band's words), `alert.spec.ts` |
| FR-11 Taskbar, no toast | 4.5 | `taskbar.rs` tests, E80; real app open (AC 6) |
| FR-12 Sub-agents | 4.1 (tooling), 4.12 | shipped: gate 1 passed (E83); folding by `agent_id` (D-547), the 1 s hold (D-548), the targets (D-549); vectors, Vitest and Playwright cases in story 4.12 |
| FR-13 What the scene shows, and from where | 4.2, 4.3 | `board.rs` tests, `typescript_bindings.rs`, `scene_vectors.rs` |

The AD-59 to AD-72 and AD-167 to AD-178 rows map to the stories' ACs as
each story's Dev Notes name them; the per-AD table is written at closure,
with the owner's entries.

## Story 4.15 (the owner's first-use feedback)

After using the installed build of `44d0f92` on his real sessions, the
owner gave nine items; story 4.15 answers each, with the decisions D-535
to D-546:

1. Controls: one radius family, 4 px on controls, 6 px on surfaces, pills
   for count badges only (D-539).
2. Wide screens: cards side by side in a board column with room, reading
   views capped and centred, the session file docked beside the scene
   when there is room (D-543).
3. Moving an ended card by drag, menu or keyboard, to Done, back to To
   resume or to a Discard target, with Undo and a Restore in the archived
   list; the author's verdict is a registry fact covering only what came
   before it (D-536 to D-538, D-542).
4. To resume and Done counted in the header (D-541).
5. A minimum window size of 1140 x 820, derived from the layouts (D-546).
6. The shadow under the scene removed (D-544).
7. The bug: archiving a subject left its ended session in To resume,
   since the board never read the subject's status; fixed, with a Rust test
   written first and a real-CLI fixture (D-535).
8. Transitions, 160 ms, opacity and transform only, none under reduced
   motion (D-545).
9. One header in every view and mode, gauges included (D-540, amending
   D-533).

Not verified on the real window, which this run did not open: the drag in
WebView2, the minimum size as Windows applies it, and the transitions'
feel; each is covered in Playwright's Chromium.

## Story 4.17 (the scene's readability)

Preparing the owner's two-second test (story 4.14 AC 9), the screenshots
of five fixtures at 1920 x 1032 showed four defects; story 4.17 fixes
each, with the decisions D-554 to D-559:

1. A queue number hidden behind the alert room's beacon: every queue
   badge is drawn in a layer above the floor's objects (D-554).
2. The count of what needs you in no one place: the register's heading
   reads "Needs you: N", it shows a row per number key, and an
   approval-only wait has a diamond and the word "approval" (D-556).
3. The register's text at about 11 px at eight projects: it is overlay UI
   at a fixed size over the scene's top-right corner (D-555); the
   decision office's extra waits move to a "+n" on its plaque (D-557).
4. Desk numbers read as queue numbers: a desk is a letter on a square
   tag, so a digit on a disc is always a key (D-558).

The minor findings, a plaque on a waiting head and the "high priority"
pill on its plaque, are fixed too (D-559). `readability.spec.ts` holds
each fix with a hit test on what is painted. The owner's AC 9 run on
these screenshots passed with a caveat (E87, D-701): every count read
right, the oldest name read right on every floor once he knew the
questions; the first two floors are to be run again in a random order.

## Decisions taken under delegation, for the owner to confirm

- D-529: the "relay stopped" chip in the danger colour.
- D-530: doctor's merged blocks, and that their keys print in alphabetical
  order (no `preserve_order`).
- D-531: walking's open cases (fades where a session stands with walking
  off, marks hidden while walking, the turn-back rule).
- D-532: Focus as a flag over the base.
- D-533: the compact gauges at 190 px.
- D-535 to D-546 (story 4.15): above all D-536 (discard as a state of its
  own, restored from the archived list, not an archive with a reason),
  D-540 (the Walking toggle in every view, "Pin" for "Pin on top", the
  gauges at 150 px), D-542 (a card done by its archived subject goes back
  by reopening the subject, without Undo) and D-546 (1140 x 820, which
  rules out half of a Full HD screen).
- D-554 to D-559 (story 4.17): above all D-555 (the register in the
  top-right corner, which the session file covers on a Full HD screen
  while it is open) and D-558 (desks named by letters instead of numbers).
