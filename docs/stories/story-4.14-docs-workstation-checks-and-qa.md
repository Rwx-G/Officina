# Story 4.14: Documentation, README assets, workstation checks and QA closure

**Status:** In progress (the owner's workstation runs, AC 6 and 8 to 10, and his review of the README assets are open; AC 5 measured with its fallback, AC 7 passed; see Completion Notes)

## Story

As the author installing v0.4 on a workstation, and as a future reader of the repository,
I want the README and changelog to explain the scene in one piece with its pictures, the facts no automated test can measure measured on the workstation, walking included, and the epic closed with evidence,
so that v0.4 is understood without guessing, its CPU promise is a number, and nothing of the PRD is left deferred.

## Acceptance Criteria

1. README (D-121): the sections the stories added become one "The scene" section: the three modes and the header controls (mode, walking, theme), `m`, `f` and digits, the zones and what puts an employee in each, walking, walking off and reduced motion, numbers, the register and the groups, the file and its actions, the line under the scene and where the gauges and relay state are, the halo's two-minute rule, the alert (the two floor keys, the band, the beacon, the taskbar badge and flash, the last known alert kept on missing data, approval waits that alert by priority only, no toast), sub-agents (or that they are not shown, if gate 1 failed), the theme control, the offices remembered on this machine, and Board as the default until the adoption condition holds; the scene screenshot sits beside the board's; the roadmap's v0.4 row is current. It transcribes no list owned by code. Proof: review; `crates/officina/tests/readme.rs` stays green.
2. README assets (AD-72 as amended, AD-17): `app/e2e/readme-assets.spec.ts` gains `docs/assets/scene.png` and `scene-dark.png` from a settled `scene-c`, and `scene.gif`: a session at reception gets its first prompt and walks to its desk, another walks to the decision office and its halo breathes, the author opens its file and brings it to front, and it walks back. Proof: `pnpm readme-assets` run on the workstation, the assets committed and reviewed.
3. `CHANGELOG.md` `[Unreleased]` holds one coherent v0.4 set under Added and Changed. Proof: review.
4. Measurement home and replay (D-120, AD-71 as amended): `app/scripts/e2e-fixtures.mjs --home <dir> <scenario>` writes that scenario's Officina home and user profile under `<dir>`, prints the environment for `officina-app.exe`, and refuses a directory that exists and is not empty; `scene-measure` is `scene-c` without two Working sessions, its waits dated at the run; `app/scripts/scene-replay.mjs --home <dir>` rewrites that home's event files and agents sidecar so one state change happens every 10 s for 10 minutes, each crossing at least one corridor, and prints what it did. Proof: the CI step "e2e fixtures from the real CLI" (without the options, unchanged); a Vitest case `app/scripts/scene-replay.test.ts > the replay schedules one crossing change every 10 s` on a temporary home.
5. Idle and walking CPU (FR-9, AD-71 as amended, D-120, D-480): on the workstation, the real `officina-app.exe` started with the printed environment, Scene mode, 10 minutes per run: halos breathing throughout, the real two-minute rule, halos steady, the replay with walking on, the replay with walking off; the `officina-app` process and its `msedgewebview2.exe` children summed, in % of the machine and % of one logical core, with the machine, the core count and the refresh rate; recorded in a research-notes entry. FR-9's threshold is both units: under 1 % of the machine and under 8 % of one core (D-501). If the two-minute halo run reaches either, `HALO_BREATHING_MS` becomes 0 in the same change; if the walking run does, `DEFAULT_WALKING` becomes `false` in the same change (D-480); tests and the README follow. This run is the only measurement of walking's cost (gate 3 withdrawn, D-500). Proof: the entry; in a fallback case, `halo.test.ts` or `modes.spec.ts` with the new constant.
6. Taskbar on the real app (FR-11, AD-174): with the real window behind another one, a waiting session shows the badge with its count; a high-priority permission prompt turns it to the alert colour and flashes the button until the window is focused; at 100 % and 150 % scaling. Proof: the research-notes entry.
7. Live theme (AD-67 gate, D-118): with the theme on Auto, switching Windows between light and dark updates the running window without a restart; recorded with the WebView2 runtime version; on failure D-118's fallback ships with `core:window:allow-theme` and its Playwright case. Proof: the entry; in the fallback case, `themes.spec.ts > a theme-changed event from the window re-themes Auto`.
8. WebView2 look (AD-70 manual list, R103): `scene-measure`'s scene in the real window at 100 % and 150 % scaling, light and dark, settled and during the replay's walks, compared by eye with the baselines and the prototype v2: no flicker of a walking figure against furniture, no blurred edges after a walk. Proof: the entry.
9. Readability (PRD section 7): on five fixtures with someone waiting, the Needs you count and the oldest waiting name are read in under 2 s in Scene mode, as on the board. Proof: the entry.
10. Adoption (PRD section 7, D-123, D-481): the day the owner starts choosing Scene is recorded; the default stays Board. Proof: the entry.
11. Slice 0 order (D-479 as amended by D-499, D-500 and D-506): the report checks that gate 4 preceded story 4.3's start, gate 5 and the owner's answer preceded story 4.6's merge, gate 2 preceded story 4.5's close and gate 1 preceded story 4.12's start, and that gate 3 is recorded as withdrawn. What a real wait emits, formerly checked here, is gate 4 (D-499). Proof: the report's evidence table.
12. Stability: CI runs Playwright with `--repeat-each=3` (D-91), the walk and screenshot cases included. Proof: the CI `windows` job.
13. QA closure (D-122): `docs/qa/epic-4-report.md` with the gate decision, the evidence table (AC 5 to 10 numbers included, and the four Slice 0 entries with the owner's answers), the audits (accessibility against FR-6, untrusted text in the scene, the band and the file, performance against FR-9 with walking), the traceability table from FR-0 to FR-13 and from AD-59 to AD-72 and AD-167 to AD-178 to the ACs that prove them, every departure of P-20 and D-461 with its decision, those from the PRD marked as such, the plan challenge's outcomes (D-494 to D-514, the floor's outcome of D-506 named), FR-12's outcome (shipped or not shipped by its own rule), and open findings moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC or a recorded rule that withholds it.

## Tasks

- [x] README "The scene" made coherent; roadmap (AC 1)
- [x] `readme-assets.spec.ts` cases; `pnpm readme-assets`; commit the assets (AC 2). The owner's review is open
- [x] CHANGELOG consolidation (AC 3)
- [x] `e2e-fixtures.mjs` `--home`, `scene-measure`; `scene-replay.mjs` and its test; the Vitest include gains `scripts/*.test.ts` (AC 4): built in story 4.2
- [x] AC 5 CPU runs (E82) and D-480's walking fallback
- [ ] Workstation runs and research-notes entries (AC 6, 8-10): the owner's
- [ ] Audits, `docs/qa/epic-4-report.md`, backlog (AC 11, 13): the report is written, open until the owner's entries; the five audits run at closure

## Dev Notes

- Requirements: FR-0 (via the baselines and Slice 0), FR-8 (live switching), FR-9, FR-11, PRD section 7. Decisions: AD-17, AD-67 (gate), AD-70 (manual list), AD-71 as amended, AD-72, AD-174, AD-178. Epic decisions: P-20, P-21, D-101, D-108, D-118, D-120 to D-123, D-461, D-479 to D-481, D-494 to D-514.
- Dependencies: stories 4.1 to 4.13 (4.12 only if gate 1 passed). The workstation checks run on a build of `main` with all of them merged.
- The replay rewrites only the temporary home the script created; it never touches the real Officina home or the Claude config dir. It writes event lines the way `officina hook` does and the fake claude's sidecar the way the fixture script does.
- Quit a running Officina first (single instance). Start the window within a minute of the script, so the two-minute rule is what the halo run measures.
- FR-9's threshold applies to both figures (D-501): the machine figure as v0.1 measures idle CPU, the per-core figure so that a pass here also holds on an 8-core machine; the refresh rate is recorded because animation cost scales with it (R26). If the scene is used daily on another machine, the owner may run AC 5 there too and record it beside.
- Cutting `[0.4.0]` and bumping the workspace version belong to the v0.4 release; not in this story.
- No employer content in the README, the research notes or the report.

## Testing

- Playwright: `app/e2e/readme-assets.spec.ts` (by hand), the full suite with `--repeat-each=3` in CI (AC 12).
- Vitest: `app/scripts/scene-replay.test.ts`.
- CLI integration: `crates/officina/tests/readme.rs`.
- Manual: research-notes entries (AC 5 to 10), the QA report (AC 13).

## Dev Agent Record

### Debug Log

- README (AC 1): the scene section gained the three modes and `f` (story 4.11), the scene's animated picture, the offices remembered on this machine (the README said "kept until the window restarts", which `memory.ts` contradicts: projects with a live session at start keep their office, and a session its desk), the halo's two-minute rule from the wait's start, and a "Sub-agents" paragraph saying the scene does not draw them while gate 1 has not passed; the scene screenshot sits beside the board's; the roadmap's v0.4 row and the design documents' links name v0.4. No list owned by code is transcribed.
- Assets (AC 2): `scene.png` and `scene-dark.png` from a settled `scene-c`, and `scene.gif`: the reception session's first prompt and its walk to its desk, a Working session's walk to the decision office and its halo, its file and Bring to front, then its walk back. Encoded with one palette and only the changed pixels per frame (`changesOnlyGif`), the GIF is 450 KB instead of 8.3 MB. Only the scene cases were run (`-g scene`), so `board.png` and `demo.gif` are unchanged.
- CHANGELOG (AC 3): the v0.4 entries already form one set under Added (Scene mode, walking, Focus mode, acting from the scene, the alert, the alert room, floor keys, dark theme) and Changed; nothing to move.
- AC 4 was built with story 4.2's fixtures: `--home`, `scene-measure`, `scene-replay.mjs` and `scene-replay.test.ts > the replay schedules one crossing change every 10 s`, in the Vitest include.

### Completion Notes

- AC 1: review; `crates/officina/tests/readme.rs` green in the gate.
- AC 2: `pnpm exec playwright test --config playwright.readme.config.ts -g scene` wrote the three assets. **Open**: the owner's review of `docs/assets/scene.png`, `scene-dark.png` and `scene.gif`.
- AC 3: review.
- AC 4: Vitest `app/scripts/scene-replay.test.ts > the replay schedules one crossing change every 10 s`; the CI step "e2e fixtures from the real CLI" unchanged.
- AC 7: **pass** (owner, 2026-10-28): with the installed build open on Auto, he switched the Windows theme and the window followed by itself, no restart; D-118's fallback is not needed. The WebView2 runtime version was not noted.
- AC 5: **walking run failed, fallback shipped** (2026-10-29, research notes E82): mean CPU in % of the machine (gate under 1 %, D-515) and of one core (information): halos breathing throughout 0.49 % (11.8 %), the real two-minute rule 0.12 % (2.9 %), halos steady 0.066 % (1.6 %), replay with walking on **1.95 %** (46.9 %), replay with walking off 0.58 % (14.0 %). `DEFAULT_WALKING` is `false` in the same change (D-480): `view.test.ts > a stored view without walking reads as walking off`, `modes.spec.ts > walking is off by default`; the cases about walking open the scene with walking on (`openWalkingScene`); README and CHANGELOG follow. `HALO_BREATHING_MS` stays. Measured with the session locked (the owner asleep): the WebView stayed visible but drew about 170 frames per second instead of the screen's 240, so the passes are under that condition (E82 gives the figures scaled by 1.4); the failure holds either way. After story 4.16 cut a walk's cost, runs 4 and 5 were run again (research notes E86, same condition): replay with walking on 0.9985 % and, run again, 1.007 % of the machine (about 24 % of one core), walking off 0.51 % (12.1 %); run 4 is not under 1 %, so `DEFAULT_WALKING` stayed `false` (D-553). The owner then accepted walking's cost at the gate and turned it on by default (D-700, 2026-10-29): `view.test.ts > a stored view without walking reads as walking on`, `modes.spec.ts > walking is on by default`.
- AC 6, 8 to 10: **open (owner)**. Build the release window from `main`, quit any running Officina, then `node app/scripts/e2e-fixtures.mjs --home <new folder> scene-measure` and start `officina-app.exe` with the environment it prints, plus `WEBVIEW2_USER_DATA_FOLDER=<that folder>webview` so the run does not rewrite the installed window's stored view (E82); `node app/scripts/scene-replay.mjs --home <that folder>` drives the walks. Then the taskbar at 100 % and 150 % (AC 6), the look at 100 % and 150 %, light and dark, settled and walking (AC 8), the 2 s readability on five fixtures (AC 9), and the day Scene becomes the chosen mode (AC 10), each a research-notes entry.
- AC 11: `docs/qa/epic-4-report.md` "Slice 0 order": gate 2 before 4.5's close, gate 3 withdrawn, gate 1 not run and 4.12 not started; gate 4 did not precede 4.3 (recorded in story 4.3); gate 5's answers preceded 4.6, its counts open.
- AC 12: the `windows` job's Playwright step runs `--repeat-each=3`; the local replay of it passed (783 passed, 6 skipped).
- AC 13: `docs/qa/epic-4-report.md`, gate decision open; the per-AD traceability, the audits and the closure wait for the owner's entries.

## File List

- `README.md`
- `app/e2e/readme-assets.spec.ts`
- `app/e2e/gifenc.d.ts`
- `docs/assets/scene.png` (new)
- `docs/assets/scene-dark.png` (new)
- `docs/assets/scene.gif` (new)
- `docs/qa/epic-4-report.md` (new)
- `docs/research-notes.md` (E82, E86)
- `app/src/lib/view.svelte.ts`, `app/src/lib/view.test.ts`, `app/src/components/Header.svelte`, `app/src/components/scene/scene.test.ts` (AC 5 fallback)
- `app/e2e/fixtures.ts`, `app/e2e/modes.spec.ts`, `app/e2e/walk.spec.ts`, `app/e2e/scene.spec.ts`, `app/e2e/stable-size.spec.ts`, `app/e2e/readme-assets.spec.ts` (AC 5 fallback)
- `CHANGELOG.md`
- `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created as story 4.9
- 2026-10-28 reworked after review (F29 to F32, F35)
- 2026-10-28 renumbered 4.12 and reworked for the scene v2 (D-460): the walking runs and the replay, the taskbar on the real app, the WebView2 look during walks, Slice 0's order, FR-10 to FR-13 and AD-167 to AD-178 in the traceability
- 2026-10-28 renumbered 4.14 after the plan challenge (D-497 to D-501, D-506): the former AC 11 (what a real wait emits) moved to story 4.1's gate 4; the CPU threshold in both units and the only walking measurement; Slice 0's order for gates 1, 2, 4 and 5; the challenge's outcomes in the report
- 2026-10-28 started: README pass, the scene's README assets, the QA report written open; the workstation runs and the assets' review are the owner's
- 2026-10-28 AC 7 passed: the window followed a Windows theme switch on Auto (owner)
- 2026-10-29 AC 5 measured (E82): the walking run over 1 % of the machine; `DEFAULT_WALKING` is `false` (D-480)
- 2026-10-29 AC 5's walking runs measured again after story 4.16 (E86): run 4 at 1.00 % of the machine, still not under 1 %; walking stays off by default
- 2026-10-29 walking on by default, the owner accepting its cost at the gate (D-700)
