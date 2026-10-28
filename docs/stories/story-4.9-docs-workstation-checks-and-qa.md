# Story 4.9: Documentation, README assets, workstation checks and QA closure

**Status:** Approved

## Story

As the author installing v0.4 on a workstation, and as a future reader of the repository,
I want the README and changelog to explain the scene in one piece with its pictures, the facts no automated test can measure measured on the workstation, and the epic closed with evidence,
so that v0.4 is understood without guessing, its CPU promise is a number, and nothing of the PRD is left deferred.

## Acceptance Criteria

1. README (D-121): the sections the stories added become one "The scene" section: the three modes and the header control, `m`, `f` and digits in every mode of the Sessions view, what the scene draws (rooms, places and poses, numbers, the register, the chip, More workshops, the shelf and the cabinet with and without the registry, the instruments), the file and its actions, the lists behind the groups, the halo's two-minute rule and reduced motion, the theme control, the slot order remembered on this machine, and Board as the default until the adoption condition holds (D-123); the scene screenshot sits beside the board's; the roadmap's v0.4 row and the design documents line are current. It transcribes no list owned by code. Proof: review; `crates/officina/tests/readme.rs` stays green.
2. README assets (AD-72, AD-17): `app/e2e/readme-assets.spec.ts` gains `docs/assets/scene.png` and `scene-dark.png` from `scene-c`, and `scene.gif`: a working employee is called to the antechamber (its `last_event_at_ms` set to the installed time, D-101), its halo breathes over several frames, the author opens the file and brings it to front, and it goes back to its desk; the board's `board.png` and `demo.gif` stay. Proof: `pnpm readme-assets` run on the workstation, the new scene assets committed and reviewed.
3. `CHANGELOG.md` `[Unreleased]` holds one coherent v0.4 set under Added and Changed, built from the entries each story added. Proof: review.
4. Idle CPU (FR-9 as amended, AD-71, D-120): on the workstation, with `scene-measure` written by `node scripts/e2e-fixtures.mjs --home <dir> scene-measure` and the real `officina-app.exe` started with the environment it prints, Scene mode shown, 10 minutes per run: halos breathing throughout (a local build with `HALO_BREATHING_MS` at the window's length), the real two-minute rule, halos steady (a local build at 0); the `officina-app` process and its `msedgewebview2.exe` children summed, in % of the machine and % of one core, with the logical core count and the display refresh rate; recorded in a new `docs/research-notes.md` entry. If the two-minute run reaches 1 % of the machine, `HALO_BREATHING_MS` becomes 0 in the same change, the halo tests and the README follow, and the PRD's fallback is recorded as applied. Proof: the research notes entry; in the fallback case, `app/src/lib/halo.test.ts` with the new constant.
5. Live theme (AD-67 gate, D-118): with the theme on Auto, switching Windows between light and dark updates the running window without a restart; recorded with the WebView2 runtime version. If it does not, D-118's fallback ships, with `core:window:allow-theme` added to `app/src-tauri/capabilities/default.json` and a Playwright case for the event path, and the check is run again. Proof: the research notes entry; in the fallback case, `app/e2e/themes.spec.ts > a theme-changed event from the window re-themes Auto`.
6. WebView2 look (AD-70 manual list): `scene-measure`'s scene in the real window at 100 % and 150 % display scaling, light and dark, compared by eye with the Playwright baselines; differences recorded. Proof: the research notes entry.
7. Readability (PRD section 7): on five fixtures with someone waiting, the Needs you count and the oldest waiting name are read in under 2 s in Scene mode, as on the board; recorded with the fixtures used. Proof: the research notes entry.
8. Adoption (PRD section 7, D-123): the day the owner starts choosing Scene is recorded; the default stays Board. Proof: the research notes entry.
9. What a real wait emits (D-108): during a real permission prompt and a real question in an interactive session, which hook events arrive after the first `Notification` while the session still waits (an `idle_prompt` notification included or not), with the Claude Code version; each such line moves `last_event_at_ms`, hence `enteredAt`, and restarts the halo's two minutes. If they do, the finding goes to `docs/backlog.md` with the observed events; no change in this story (review F35). Proof: the research notes entry.
10. Measurement home (D-120): `app/scripts/e2e-fixtures.mjs --home <dir> <scenario>` writes that scenario's Officina home and user profile under `<dir>` instead of a fixture, prints the environment for `officina-app.exe`, and refuses a directory that exists and is not empty; `scene-measure` is `scene-c` without two Working sessions, its waits dated at the run. Proof: the CI step "e2e fixtures from the real CLI", which runs the script without the option and is unchanged; the workstation run of AC 4.
11. The full automated suite is stable: CI runs Playwright with `--repeat-each=3` (D-91), the scene's screenshot cases included. Proof: the CI `windows` job.
12. QA closure (D-122): `docs/qa/epic-4-report.md` with the gate decision, the evidence table (AC 4 to 9 numbers included), the audits (accessibility against FR-6, untrusted text in the scene and the file, performance against FR-9), the traceability table from FR-0 to FR-9 and from AD-59 to AD-72 to the ACs that prove them, every departure of P-20 with its decision, those from the PRD marked as such, and open findings moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC.

## Tasks

- [ ] README "The scene" made coherent; roadmap and design documents line (AC 1)
- [ ] `readme-assets.spec.ts` cases; run `pnpm readme-assets`; commit the assets (AC 2)
- [ ] CHANGELOG consolidation (AC 3)
- [ ] `e2e-fixtures.mjs`: `--home`, `scene-measure` (AC 10)
- [ ] Workstation runs and research notes entries (AC 4-9); the fallbacks if a check fails (AC 4, 5)
- [ ] Audits, `docs/qa/epic-4-report.md`, backlog (AC 12)

## Dev Notes

- Requirements: FR-0 (via the baselines, D-122), FR-8 (live switching), FR-9 as amended, PRD section 7. Decisions: AD-17, AD-67 (gate), AD-70 (manual list), AD-71, AD-72. Epic decisions: D-44, P-16 (Epic 3), P-7 (Epic 2); P-20, P-21, D-98, D-101, D-108, D-118, D-120 to D-123.
- Dependencies: stories 4.1 to 4.8. The workstation checks run on a build of `main` with all of them merged.
- Seam (D-98): `crates/officina/tests/readme.rs` comes from story 2.10, or from 3.12 if 2.10 had not merged (D-44).
- D-120 replaces AD-71's "through the e2e mock build": the measurement uses the real window and collector against a temporary home with the fake claude, which needs no board injection and counts the collector, as v0.1's idle CPU does. Quit a running Officina first (single instance). Start the window within a minute of running the script, so the waits are fresh and the two-minute rule is what the second run measures.
- FR-9's threshold applies to the machine figure (AD-71); the per-core figure travels between machines. R26: the refresh rate is recorded because animation cost scales with it.
- The README's sessions are fictitious, as the board's (`scene-c` is a redacted, hand-written scenario).
- Cutting `[0.4.0]` and bumping the workspace version belong to the v0.4 release, after `[0.3.0]`; not in this story.
- No employer content in the README, the research notes or the report: placeholder names and paths only (officina-project rule).

## Testing

- Playwright: `app/e2e/readme-assets.spec.ts` (by hand, not a CI gate), the full suite with `--repeat-each=3` in CI.
- CLI integration: `crates/officina/tests/readme.rs`.
- Manual: research notes entries (AC 4 to 9), the QA report (AC 12).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: theme fallback names `core:window:allow-theme` (F29); no file count in AC 2 (F30); `readme.rs` seam stated (F31); the report marks the PRD departures of P-20 (F32); what a real wait emits recorded on the workstation, AC 9 (F35)
