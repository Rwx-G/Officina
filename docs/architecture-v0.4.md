---
title: "Architecture addendum: Officina v0.4 - The Scene"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions); reworked 2026-10-28 after the owner approved the interactive scene prototype v2 (section 7)
inputs:
  - prd/prd-v0.4.md
  - (requirement challenge, not published)
  - (framing, not published; section 8)
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - (Slice 0 mock, not published: _bmad-output/planning-artifacts/design/scene-mock.html, git-ignored)
  - (interactive scene prototype v2, not published: _bmad-output/planning-artifacts/design/prototype/interactive/, git-ignored)
  - https://code.claude.com/docs/en/hooks
  - https://v2.tauri.app/reference/javascript/api/namespacewindow/
  - https://docs.rs/tauri/2.12.5/tauri/webview/struct.WebviewWindow.html
---

# Architecture addendum: Officina v0.4

AD-1 to AD-58 stay in force except where section 3 amends them; this addendum adds AD-59 to AD-72 and, after the scene rework of 2026-10-28, AD-167 to AD-178. **Measured** marks a number taken on the workstation for this addendum (section 1). **Gate** marks a fact to check on the workstation before the dependent story closes. **Verified** marks a fact read in the documentation or the code on 2026-10-28, with its source.

The scene is a view. It places the board's sessions and decides nothing a board card does not already say: no classification and no new source of truth. Its one pure rule beyond the board (which zone a live session belongs in, alert included) lives in core next to the classification, with a vector set (AD-167). Every rule below serves one property the PRD makes testable: once every employee has arrived and while the payloads are unchanged, the scene element does not change.

**Visual reference.** The Slice 0 mock (`design/scene-mock.html`) fixed the first visual language; the interactive prototype v2 (`design/prototype/interactive/`, sources `scene.js`, `scene.css`, `build.mjs`, renders `scene-interactive.png` and `scene-interactive-dark-target.png`; v1 kept as `scene-interactive-v1.html`) replaces it since the owner approved it on 2026-10-28. Both are git-ignored sketches to port from, never imported and absent from CI. The prototype simulates signals with an accelerated clock (1 real second is 30 workshop seconds) and has a "target" mode for v0.5 and later; only its "v0.4" mode is in scope here.

## 1. Measured prototype

**What was measured.** One scene with 10 employees, 5 of them waiting (density (c) of the mock with two working sessions removed), rendered six ways from the same prototype output. Pages in `design/prototype/perf-pages.mjs`, runner in `perf-run.mjs`.

**How.** Playwright 1.64.1 (the app's) driving Chromium 156.0.8078.4, viewport 1100 x 720 at device scale 1.5. Per variant: a fresh browser, 3 s warm-up, then a 20 s window. CPU is the sum of `cpuTime` deltas of every browser process (browser, renderer, GPU, utility) from CDP `SystemInfo.getProcessInfo`, divided by wall time: **% of one logical core**. Main-thread busy time is the `TaskDuration` delta from CDP `Performance.getMetrics`; style recalculations per second from `RecalcStyleCount`. Workstation: Intel Core Ultra 9 285K, 24 logical cores, NVIDIA RTX 5090. Two compositing paths:

- **Software**: Playwright's default headless shell; SwiftShader rasterization and compositing; frames at 60 per second.
- **GPU**: full Chromium in new headless mode with `--use-angle=d3d11 --enable-gpu --ignore-gpu-blocklist --enable-gpu-rasterization` (`gpu_compositing: enabled`); frames at the display rate, about 170 per second here. WebView2 normally composites on the GPU the same way; not verified on this machine.

WebView2 itself could not be driven: the runtime (155.0.4283.62) refuses to start without a host, Microsoft Edge is not installed, and launching `officina-app.exe` would start the collector against the real home and fight the single-instance lock. Chromium 156 is the same engine one version later; the workstation measurement of AD-71 closes the gap.

| Variant | Software: % core | main busy % | recalcs/s | GPU: % core | main busy % | recalcs/s |
|---|---|---|---|---|---|---|
| static SVG scene, nothing animated | 0.06 | 0.00 | 0 | 2.17 (start-up overhead; 0.25 over 10 minutes) | 0.00 | 0 |
| SVG scene, 5 halos animating `opacity` inside the SVG | 3.94 | 0.05 | 0 | 12.26 | 0.08 | 0 |
| SVG scene, 5 halos on an HTML layer, `opacity`, `will-change` (proposed) | 4.05 | 0.05 | 0 | 15.27 | 0.13 | 0 |
| board today: 5 cards animating `box-shadow` | 15.44 | 1.36 | 60 | 22.57 | 2.79 | 160.6 |
| board fixed: same ring on `::after`, `opacity` only | 5.41 | 0.05 | 0 | 12.30 | 0.09 | 0 |
| Canvas: scene bitmap and 5 halos redrawn per `requestAnimationFrame` | 36.72 | 26.19 | 0 | 80.83 | 7.12 | 0 |

The 20 s GPU figures include about 2 points of browser start-up work (compare the 10-minute static run below); the comparison between rows holds. Divide by 24 for Task Manager's "% of the machine": the proposed variant is 0.17 % (software) and 0.64 % (GPU) of this machine while halos breathe.

**Ten minutes, GPU path** (FR-9's window), same method with a 600 s window:

| Variant | % of one core | % of the machine | main busy % | recalcs/s |
|---|---|---|---|---|
| static SVG scene | 0.25 | 0.010 | 0.00 | 0 |
| HTML halos breathing the whole 10 minutes (worst case) | 12.10 | 0.504 | 0.07 | 0 |
| HTML halos breathing 2 minutes, then steady (FR-3) | 2.61 | 0.109 | 0.02 | 0 |

The two-minute run costs the static scene plus a fifth of the worst case (0.25 + 11.85 x 120 / 600 = 2.62), so the breathing cost is linear in breathing time and nothing lingers after it stops.

**DOM size**, counted on the mock's prototype output: 1,069 SVG elements and 17 targets for density (c); 1,380 SVG elements and 37 targets at full occupancy. The measured page (10 employees, static variant) holds 1,041 SVG elements and 1,049 DOM elements in all.

**What the numbers say.**

1. A static SVG scene costs nothing between payloads. All cost is animation.
2. In Chromium 156 an `opacity` animation on an SVG element is composited: no style recalculation, no main-thread work, same cost as the HTML layer. It is not a documented guarantee, and v1 adds WebKitGTK.
3. The board's `box-shadow` halo is the expensive one: a style recalculation per frame, 60 to 160 per second, 1.8 to 2.9 times the cost of the same ring animated by `opacity`.
4. Canvas needs a frame loop for any animation and costs 5 to 9 times the SVG scene while it runs.
5. Any running animation costs per displayed frame. The two-minute breathing limit (FR-3), not the technique, is what keeps the scene's average cost near zero.

**Not measured: walking.** The v2 prototype drives the walk cycle from a JavaScript frame loop that runs while someone walks. Its cost is gate 3 of AD-178 (Slice 0), and the workstation measurement of AD-71 as amended includes it.

## 2. Decisions

### AD-59: SVG with HTML layers, not Canvas

**Amended 2026-10-28 (scene v2).** Walking figures cross rooms, so they cannot live in a layer separate from the furniture they pass behind. The stack becomes five layers sharing one logical coordinate box:

| # | Layer | Content | Owner | Pointer |
|---|---|---|---|---|
| 1 | `floor` (SVG) | ground, slab, walls, wall windows and frames, the door, room floors and seams, rugs, spot marks, the alert room's hazard outline | Svelte, from the layout | none |
| 2 | `halos` (HTML) | one element per waiting employee standing at its place, `opacity` animation only, `will-change: opacity` | Svelte, from the layout | none |
| 3 | `objects` (SVG) | partitions, desks, chairs, sofas, plants and every other static object, and every figure, in one depth-sorted list | the scene's object module (AD-171), not Svelte templates | none |
| 4 | `overlay` (SVG with `foreignObject`) | plaques, the register, the "high priority" pills | Svelte, from the layout | none |
| 5 | `targets` (HTML) | one transparent `<button>` per actionable element | Svelte, from the layout | all |

Layers 1 to 4 are `aria-hidden="true"`; layer 5 carries every name, role and focus. The halo stays on HTML for AD-59's original reason (composited `opacity` on every engine Officina targets). Session text never enters layer 3: its markup is built from closed types only (AD-171), and names reach the page through Svelte's `{text}` in layers 4 and 5.

Rejected, unchanged: **Canvas**, which needs a frame loop for the halo and gives up the DOM that the accessibility rules, the mutation e2e and the screenshot tests rely on.

### AD-60: The layout is a pure TypeScript function

`app/src/lib/scene/layout.ts` exports `layoutScene(input: SceneInput, memory: SceneMemory): SceneLayout`. No DOM, no clock, no storage, no `Math.random`.

**Amended 2026-10-28 (scene v2).** `SceneInput` is `{ board: Board }`: the lamp, the dials, the shelf and the cabinet left the scene (AD-177), so the relay and registry payloads are no longer inputs; v0.5 adds a nullable `orchestrator` (prd-v0.5.md section 9). The layout reads each live card's `scene` state (AD-167) and never maps columns to zones itself. `SceneLayout` lists: the floor shape (office count, AD-169), rooms with their plaques, employees (card key, zone, spot id, pose, marks: ticket number, approval ticket, desk badge, question mark), desks (free, occupied, kept with a number), groups beyond capacity, targets with their rectangles and tab order, the text alternative, and the next `SceneMemory`. It names places, never motion: getting a figure from its old place to its new one is AD-172's.

Why TypeScript and not the Rust core: it is geometry and presentation (logical pixels, spots, plaque caps, the tab order of HTML buttons); none of it feeds a decision, a CLI or a file, and slot memory is per-machine view state (AD-62). Geometry constants live in `lib/scene/geometry.ts` and are imported, never retyped, by the tests.

### AD-61: Composition, geometry and scaling

**Superseded 2026-10-28 by AD-169** for the composition, the grid, the room sizes and the canvas. Kept: figure targets of 48 x 48 logical px; plaques capped at 140 logical px with CSS ellipsis; the scale rule `s = clamp(min(areaWidth / CANVAS.w, areaHeight / CANVAS.h), MIN_SCALE, MAX_SCALE)` applied as one `transform: scale(s)` on the layer stack, with `MIN_SCALE = 44 / 48` so every target is 44 CSS px at the minimum; below the minimum area the scene stays at `MIN_SCALE` and the sessions area scrolls, while Board and Focus fit any size.

### AD-62: Slot and desk assignment

**Amended 2026-10-28 (scene v2):** "workshop" reads "office" and the antechamber is replaced by the decision office's queue.

**Offices (FR-1).** Order: floor projects in file order (from `Board.floor_projects`, AD-68); then projects remembered from earlier runs that have a live card on the first layout of the run; then projects in order of first appearance since start (ties within one payload: earliest `started_at_ms`, `null` last, then name, then card key). A project keeps its office until restart, dimmed when it has no live session. Nothing reorders by activity. Offices 1 to 4 are the base floor's; offices 5 to 8 come with the floor's growth (AD-169). With more than eight projects, the first seven keep their offices and office 8 becomes **More offices**, whose plaque counts the extra projects and which is one group target listing their employees (epic decision D-103). "other" and "unknown" (v0.1 FR-9) are ordinary projects.

**Desks.** Each office has four desks. A live session takes the lowest free desk of its project the first time it is laid out outside reception, and keeps it for as long as it is live, wherever it stands; a session without a free desk joins its office's "+n" chip list and keeps that place until its own zone changes (D-102). An empty chair at a kept desk shows the session's board number while it waits, or its desk number while it is in the lounge.

**Places in a zone.** The decision office's queue and the alert room follow board order every time: Needs you cards in `columns.needs_you` order, then the sessions waiting only on a relay approval, by the approval's `requested_at_ms`. When the first one leaves, the others move up one place: the one move that is not the mover's own state change (FR-2 wins). Reception and the lounge give the first free spot in their spot order and keep it while the session stays (sticky). Beyond a zone's spots, the rest join the zone's group (D-484).

**Memory.** `{ v: 2, floor: string | null, offices: string[], desks: Record<string, (string | null)[]> }` in `localStorage["officina.scene.v2"]`, read once at start, written only when the assignment changes; `officina.scene.v1` is ignored. Pruned at start: projects with no live card on the first layout and desks whose card key is not live are dropped; a change of `Board.floor` discards the memory. The last place of each live key (for AD-167's Unknown) and sticky spots live in the in-memory `SceneMemory` only (D-184). Lost storage costs only stability.

### AD-63: Z-order, hit testing and keyboard

**Amended 2026-10-28 (scene v2).** Z-order is AD-171's depth sort; the rest holds:

- **Hit testing** is the browser's: layer 5 holds absolutely positioned `<button>`s at the layout's target rectangles, in logical px under the shared transform; layers 1 to 4 have `pointer-events: none`. Targets never overlap (AD-169's property). A figure's target sits at the place the layout gives it, not on the walking figure: when its state changes, the target moves at once to the new place, and the figure walks there (D-471).
- A hovered or focused target shows a name tag pre-rendered inside the button through CSS (`:hover`, `:focus-visible`), so hovering mutates nothing.
- **Focus ring**: two-tone, a 3 px `--color-working` outline with a 2 px offset plus `box-shadow: 0 0 0 2px var(--color-surface)` (D-188).
- **Tab order** (FR-6) is DOM order: reception by spot, the decision office's queue in board order, the register's group, the alert room in board order, offices by slot with desks front to back and each chip after its desks, the lounge by spot, More offices; the line under the scene (AD-177) follows the scene element.
- **Arrows** move to the nearest target within 45 degrees in the same group (D-187); **Enter** or a click opens the file with **Bring to front** focused; **digits 1 to 9** act on the nth Needs you session; **Escape** closes the file and returns focus to the target that now represents the card.
- **Text alternative**: a visually hidden summary built by the layout, e.g. "5 need you, 1 in alert. Oldest: billing-api, project billing-api, permission prompt. 1 at reception. 2 your turn. 5 working in 4 offices." Each target's `aria-label` states name, project, zone, state, waiting reason and number.

### AD-64: Motion only on state change

**Amended 2026-10-28 (scene v2), replacing "moves under 400 ms on a state change only" as the default.** A changed place is a walk along the corridors (AD-170, AD-172); walking off gives a glide under 400 ms; reduced motion places at once. What holds:

- Every move is triggered by a changed layout entry; no idle motion, no timer, no motion of a figure that has arrived.
- The scene never reads `BoardStore.nowMs` and never draws a relative time.
- The only frame loop is the mover's (`lib/scene/mover.ts`), started by a changed layout and stopped when every figure has arrived and every fade has ended. ESLint's `no-restricted-globals` (`setInterval`, `setTimeout`, `requestAnimationFrame`) and `no-restricted-properties` (`Date.now`, `performance.now`, `Math.random`) cover `src/lib/scene/**` and `src/components/scene/**` except `src/lib/scene/mover.ts`, the one allowed module (D-470).
- An identical payload produces an identical layout; once settled, the scene element receives no write. The e2e of AD-70 asserts exactly that.

### AD-65: The halo, in the scene and on the board

Unchanged in its rules (element, `halo-breathe` keyframes, shared phase, two minutes, memo of D-108, steady under reduced motion, no halo in Focus, the board's `::after` fix). **Amended 2026-10-28 (scene v2):** a scene halo exists for an employee in the decision office or the alert room only once it stands at its place (none while walking), at the place's logical point (the prototype's ellipse: 27 by 38 logical px standing, 27 by 33 seated, centred 36 or 30 px above the floor point); in the alert room it uses `--color-alert` in its gradient. The alert room's beacon glow breathes under the same memo and rules (AD-175).

### AD-66: Modes of the sessions area, and view persistence

Unchanged (Sessions, Subjects, Relay; Board, Scene, Focus; `m`, `f`, digits in every mode; `lib/view.svelte.ts`; Board the default). **Amended 2026-10-28 (scene v2):** `ViewState` gains `walking: boolean`, read field by field with default `true` (D-113's rule); the header shows a "Walking" toggle (`aria-pressed`) in Scene mode only. Turning walking off while figures walk sends each to its place at once. Reduced motion is the system's (`prefers-reduced-motion`), read live, never stored (D-469).

### AD-67: Dark theme and switching

Unchanged (same roles, recomputed values, `app.css` the only copy, the contrast test, the switching rules, the live-update gate). **Amended 2026-10-28 (scene v2):** the prototype's tokens join the set: `--color-project-plum` (with `ProjectColor::Plum` in core, AD-68), `--color-alert` and `--color-on-alert`, `--scene-alert-floor`, the sofa, rug, board, lamp, pot, ceramic, soil and third-leaf materials, `--figure-legs`, `--figure-shoe`, `--figure-eye`, and the people palette (`--skin-0` to `--skin-4`, `--hair-0` to `--hair-2`, the same in both themes). The contrast pairs gain the prototype's (`build.mjs`): text on the alert colour 4.5, the alert colour on the background 3, trousers and tunic on each new floor and rug 3; the alert colour on amber is decor. The pairs of skin and hair kept as looks are those at 3:1 or more, computed by a test from the tokens, never listed (D-478). An alert never changes the theme (FR-8).

### AD-68: Payloads, additions and graceful absence

**Amended 2026-10-28 (scene v2).**

- **`Board.floor_projects: Vec<ProjectView>`** (core): the floor's projects in file order, empty without a floor (unchanged).
- **`ProjectView.priority: Priority`** (`normal`, `high`), from the floor (AD-168); **`ProjectColor::Plum`** joins the palette.
- **`Card.last_message_tail`** (D-182, unchanged).
- **`Card.scene: Option<SceneState>`**, **`Board.alert_count`**, **`Board.alert_after_ms`** and **`Board.next_escalation_at_ms`** (AD-167, AD-168).
- **`Card.subagents: Vec<SubagentView>`**, only if gate 1 of AD-178 passes (AD-176).
- Dropped: `RegistryView.archived_count` (D-100) and `QuotaView.five_hour_reset_passed` and `seven_day_reset_passed` (D-183): their consumers, the cabinet and the dials, left the scene (AD-177).
- `BOARD_SCHEMA` goes up by one for all of v0.4, relative to `main` at that merge (P-22 as amended).
- **Absence** is ordinary input: without `floor_projects`, first appearance only; without hook events, no reception and no sub-agents; without a known wait start, no escalation by time.

### AD-69: The file panel

Unchanged (a side panel, 420 px, outside the scene element, actions with the board's outcomes, text as text, Escape). **Amended 2026-10-28 (scene v2):** it adds the zone, "waiting since" (or "since unknown"), the alert's cause, the pending approval with a link to Board mode where it is approved (pointer only, D-28), and the sub-agents beside the session with their `agent_type` as text (AD-176). A sub-agent's own target opens its lead's file.

### AD-70: Test strategy

**Amended 2026-10-28 (scene v2).** Rows replaced or added; the halo memo, tokens, components, fixtures, keyboard and modes, layout at size, themes and readability rows hold.

| Layer | What | How |
|---|---|---|
| core scene state | every rule of AD-167: zones from column and badge, reception on evidence, approvals, alert by priority and by time, unknown wait start, stale board, next escalation; a coverage test over `Zone` and `AlertCause` | `tests/vectors/scene.json`, replayed through `build_board` (AD-14's pattern) |
| layout unit | office order (floor, memory, first appearance, pruning, floor change, growth, More offices); desks (kept while away, freed on end, chip list); queue and alert room in board order with approvals after; sticky reception and lounge spots; groups beyond capacity; unknown keeps its last place, first sight at its desk; identical input gives a deep-equal layout; fixed point of its own memory | Vitest on `layoutScene`, boards from the fixture index (D-181) |
| geometry property | at full occupancy for 4, 6 and 8 offices (every spot, desk, chip, register overflow, More offices): no two targets intersect; no target meets a plaque it does not own; every target is 44 CSS px or more at `MIN_SCALE`; every target lies inside the canvas | Vitest, constants imported, never retyped |
| paths | every spot reachable from every spot and from the entrance without staff nodes; routes stay on graph edges; smoothing keeps the end points; deterministic | Vitest on `lib/scene/paths.ts` |
| mover | with an injected clock: positions continuous, speed within `WALK.vmax`, arrival exact; glide ends in `GLIDE_MS`; reduced motion places at once; the loop stops when all arrived; facing hysteresis | Vitest on `lib/scene/mover.ts` |
| markup | layer 3's builders take closed types only and output no text node from a payload | Vitest: built for every pose, look and project colour, scanned for text nodes |
| truthful motion | Scene open, figures settled (`data-moving="0"` on the scene element, written by the mover), `page.clock.install`, a `MutationObserver` on `[data-testid=scene]`; advance 150 s, push the same board: zero records; push a changed board: records | Playwright |
| stable figures | a state change that makes a figure walk, stepped with `page.clock.runFor(16)` for 72 frames: one node from A to B, exactly one pose group and one view visible per frame, opacity 1, largest step under 3 logical px per frame, facing never flips back within 8 frames, no CSS animation inside any figure (`document.getAnimations()` filtered by `[data-figure]` ancestors), and the loop stopped after arrival | Playwright, the prototype's flicker check ported |
| alert | band text and presence, alert room placement, beacon lit, theme tokens unchanged | Playwright |
| taskbar | the overlay image for counts 1 to 9 and above nine, amber and alert; its colours equal `app.css`'s light tokens; a new alerted key requests attention once, a known one never | Rust unit tests in the app crate (AD-174) |
| screenshots | the densities in both themes with the scene settled, `toHaveScreenshot` with the clock installed and animations disabled; baselines from CI (D-119), compared once by the owner with the prototype's renders | Playwright, `app/e2e/scene.spec.ts` |

Not automatable, recorded in research notes: AD-178's gates; AD-71's CPU measurement; AD-67's live theme gate; the scene on the workstation's WebView2 at 100 % and 150 % scaling.

### AD-71: Measuring FR-9 on the workstation

The officina-app process and its `msedgewebview2.exe` children, 10 minutes, a fixture home of 10 employees and 5 waiting (D-120), Task Manager's CPU column or `Get-Counter "\Process(*)\% Processor Time"` summed over those processes, recorded with the logical core count and the display refresh rate in % of the machine and % of one core. Runs: halos breathing throughout, the real two-minute rule, halos steady.

**Amended 2026-10-28 (scene v2):** two more runs. **Walking**: the same home with a replay script that rewrites the fixture's event files and agents sidecar so one state change happens every 10 s for 10 minutes (60 walks, each crossing at least one corridor), walking on. **Glide**: the same replay with walking off. FR-9's threshold applies to the machine figure of the walking run with the two-minute halo rule; if it exceeds it, walking ships off by default (D-480) and halos keep their rule; if the halo run alone exceeds it, halos are steady from the start.

### AD-72: Frontend structure and README assets

**Amended 2026-10-28 (scene v2).** Files:

- `lib/scene/geometry.ts` (AD-169), `places.ts` (spots and the navigation graph, AD-170), `paths.ts` (routes and smoothing), `layout.ts` (AD-60), `memory.ts` (AD-62), `mover.ts` (AD-172, the one module with a frame loop), `objects.ts` (layer 3's object module and depth sort, AD-171), `rig.ts` (figure markup, poses), `furniture.ts` (seeded statics, AD-173), `text.ts` (text alternative and names);
- `lib/halo.ts` (AD-65), `lib/view.svelte.ts` (AD-66, AD-67);
- `components/scene/`: `Scene.svelte` (layer stack, scale, the mover's host), `SceneFloor.svelte`, `SceneHalos.svelte`, `SceneObjects.svelte` (mounts `objects.ts` with an action), `SceneOverlay.svelte`, `SceneTargets.svelte`, `Plaque.svelte`, `Register.svelte`, `SceneStrip.svelte` (AD-177);
- `components/AlertBand.svelte` (AD-175), `SessionFile.svelte`, `FocusList.svelte`, `ModeSwitch.svelte`, `ThemeSwitch.svelte`, `WalkingSwitch.svelte`.

README assets: `docs/assets/scene.png` and `scene-dark.png` from a settled scene, and `scene.gif`: a session at reception gets its first prompt and walks to its desk, another walks to the decision office and its halo breathes, the author answers and it walks back. The board assets stay.

### AD-167: The scene state in core

`officina-core::scene` is pure and runs inside `build_board`, after classification. For each live card (Needs you, Your turn, Working) it sets `Card.scene: Some(SceneState { zone, waiting_since_ms, alert, approval })`; To resume and Done cards get `None`, since ended sessions are not drawn.

`Zone` (`closed_enum!`): `reception`, `office`, `decision`, `alert`, `lounge`, `unknown`. `AlertCause`: `priority`, `waited`. Rules, first match wins:

1. `board.stale` is set (the agents snapshot failed): `unknown`. The view keeps the employee where it was, with a question mark.
2. The card's badge is `unknown`: `unknown`.
3. **Waiting**: the column is Needs you, or a pending approval names the card's session. `waiting_since_ms` is, for Needs you, the latest blocking `Notification` (`BLOCKING_NOTIFICATIONS`) after the session's last prompt and last stop, else `None`; for an approval only, the earliest pending approval's `requested_at_ms`, and `approval = true`. `alert` is `priority` when the project's priority is high, else `waited` when `waiting_since_ms` is known and `now_ms - waiting_since_ms >= alert_after_ms`, else `None`. Zone `alert` with an alert, else `decision`.
4. Your turn, with a `SessionStart` among the session's events and no `UserPromptSubmit` since: `reception`.
5. Your turn otherwise: `lounge`.
6. Working: `office`.

`Board.alert_count` counts cards with an alert; `Board.next_escalation_at_ms` is the earliest `waiting_since_ms + alert_after_ms` among waiting cards without an alert whose start is known, else `None`. Board order is unchanged, so digits keep acting on `columns.needs_you`; an alerted session keeps its number.

Reception rests on positive evidence: without hook events no session is ever at reception. An `idle_prompt` notification is not blocking and moves nobody (D-462). `PreToolUse` is not an input (D-463).

The vector set `tests/vectors/scene.json` (hand-written, redacted, exempt from every formatter) holds raw agents objects, event lines, approvals, floor priorities, the threshold and `now_ms`, and the expected `scene` per card key; a coverage test iterates `Zone::ALL` and `AlertCause::ALL` and fails when a value appears in no case. The frontend never re-implements these rules (AD-13).

### AD-168: Floor keys and escalation timing

- **`[[project]] priority`**: `"normal"` (default) or `"high"`; core enum `Priority`. Read into `ProjectRule` and `ProjectView` through the one constructor of D-99, so a card's priority always equals its office's.
- **`[alert] after_minutes`**: an integer from 1 to 1440, default 10; `Board.alert_after_ms` carries it in ms, so the threshold is never typed in TypeScript (AD-5).
- Both keys are view keys: they authorise nothing, so they are outside the relay's canonical catalogue and its approval (AD-24), and a change applies at the next floor load. Doctor reports an unknown value or an out-of-range threshold with its line, as for other floor keys; an invalid key leaves the default in force.
- **Timing**: an escalation by time is a state change caused by the clock, so the collector watches it. At each tick (AD-4's 250 ms stat loop) it compares `now_ms` with the last board's `next_escalation_at_ms`; when reached, it recomputes the board from its cached inputs, with no `claude` spawn (as a quota change does). The board's bytes change through `Card.scene.alert`, so AD-5's comparison emits it. The alert reaches the window within one tick.

### AD-169: Geometry of the floor

The prototype's projection and constants, in `geometry.ts`, imported by every test:

- **Projection**: 2:1 dimetric, `P(gx, gy, z) = [OX + (gx - gy) * 36, OY + (gx + gy) * 18 - z * 30]`: tile 72 x 36 logical px, z unit 30.
- **Grid**: rooms are square cells of `R = 4.4` tiles, separated by corridors of `G = 1.1` tiles, inside a margin `M = 0.5`. Two rows. Columns 0 and 1 hold the shared zones: reception (0, 0), decision office (1, 0), lounge (0, 1), alert room (1, 1). Columns from 2 hold offices, back row then front row: offices 1 to 4 in (2, 0), (3, 0), (2, 1), (3, 1); offices 5 and 6 in (4, 0), (4, 1); offices 7 and 8 in (5, 0), (5, 1).
- **Growth (FR-1)**: the floor has 4 columns until a fifth project needs an office, then 5, then 6 from the seventh. Columns are added on the right, `OX` and `OY` depend on the row count only, so no existing room moves. The floor never shrinks before restart.
- **Floor size**: `FX = 2M + C x R + (C - 1) x G` for C columns, `FY = 2M + 2R + G = 10.9`; walls 3.4 z units high; slab 0.45; partitions 0.42 high and 0.14 thick with door gaps; the corridor centre line runs at `gy = M + R + G / 2 = 5.45`.
- **Canvas**: padding top 30, left 14, right 14, bottom 14; `w = (FX + FY) x 36 + 28`, `h = 30 + 3.4 x 30 + (FX + FY) x 18 + 0.45 x 30 + 14`: **1209 x 750** logical px with 4 columns, 1407 x 849 with 5, 1605 x 948 with 6, computed, never typed. The minimum sessions area is the canvas times `MIN_SCALE` (about 1108 x 688 CSS px with 4 columns), below which the scene scrolls.
- **Wall furniture**: the entrance door on the left wall at `gy` 1.5 to 2.7; windows and seeded frames on both walls; a whiteboard on the back wall above the decision office (static in v0.4); a wall clock drawn at a fixed time, never ticking (AD-64).
- **Targets**: 48 x 48 logical px for figures, centred on the place's floor point and rising 56 px above it; group targets (register overflow, chips, More offices, zone overflows) as epic decision D-104 and D-484 fix them. The prototype's spots are starting values: where two spots are too close for disjoint targets (the queue's 1.1-tile pitch, the lounge's 0.7-tile seats), spots are spread or removed until AD-70's property holds; the property never changes (D-471).

### AD-170: Places and the navigation graph

`places.ts` builds, from `geometry.ts`, a graph of nodes in floor coordinates and its spots, as the prototype's navigation graph:

- **Reception**: the exit node outside the door, an entry node, a hub, a door node to the corridor; four visitor spots around the counter. A new session appears on the exit node and walks in; an ended one walks out and disappears there.
- **Decision office**: the decider's desk with two screens and a chair (empty in v0.4: the author is the decider); a door to the corridor; the queue's spots, three inside along the rug, then four in the corridor leading back to the door; a consult spot in front of the desk and staff nodes from the reception counter, unused in v0.4 and reserved for v0.5's orchestrator (prd-v0.5.md section 9).
- **Offices**: a door on the corridor side (front side for the back row, back side for the front row), an aisle, four chairs at local (1.25, 1.9), (3.25, 1.9), (1.25, 3.7), (3.25, 3.7), two standing spots for the chip list.
- **Lounge**: a three-seat sofa, a two-seat sofa, two armchairs and two standing places by the coffee corner, each seat with its slide-in offset; a door to the corridor.
- **Alert room**: five standing spots around the beacon; a door to the corridor.
- **Corridor**: one node per door on the centre line, chained in `gx` order.

Routes are shortest paths (Dijkstra on Euclidean edge lengths) that never pass staff nodes; interior corners are rounded by a quadratic curve of radius 0.45 tile (at most half of each segment); a route starting mid-walk starts from the nearer of the walker's previous and next nodes. All of it is pure and deterministic.

### AD-171: Figures, the object layer, depth and snapping

The prototype's v2 rendering rules, which removed its flicker, are requirements:

- **One persistent node per figure**, created when its card key first appears and kept until it leaves; a pose change toggles the visibility of one of three pose groups built once (standing, with a front and a back view and a horizontal flip; seated facing the author; seated working, back turned with the chair back covering the body). A pose change never rebuilds markup.
- **No CSS animation inside a figure.** The walk cycle, fades and seat slide are written by the mover (AD-172), so moving a node in the DOM restarts nothing.
- **The object layer is owned by `objects.ts`**, not by Svelte templates, because the depth sort moves figure nodes among static ones and Svelte's keyed blocks would fight it. `SceneObjects.svelte` mounts it with an action; the module builds statics from `furniture.ts`, sorted by depth `gx + gy`, and inserts figures among them.
- **Depth with hysteresis**: a figure's depth is `gx + gy` (its seat's when seated); its index among statics changes only when its depth leaves a window of 0.05 tile around the index's statics, and its quantised depth only when it moves by more than 0.06; ties by quantised depth, then creation order. Only figure nodes are moved, with `insertBefore`, and only when their neighbour changed.
- **Snapped positions**: a figure's translate is rounded to whole device pixels (`snapK` = the scene's screen matrix scale times `devicePixelRatio`, recomputed on resize and scale change).
- **Writes only on change**: every attribute the mover writes goes through a per-figure cache, so a settled figure receives no write.
- **Markup from closed types**: rig markup takes a look (skin index, hair index, hair style), a project colour token, the cap flag and the scale, never text; a figure carries `data-figure` (its card key, an attribute, not markup) and `aria-hidden`.
- **Marks**: the ticket (board number on amber, on `--color-alert` in the alert room), the approval ticket (amber, no digit), the desk badge (desk number on surface, ringed in the project colour), the question mark; the Your turn ring and the selection ring under the figure. Rebuilt only when their key changes.
- **Lead and sub-agent**: every session's figure wears a cap in its project colour; a sub-agent figure is scaled 0.68, without a cap (AD-176).

### AD-172: The mover

`mover.ts` takes each new layout and the current figure positions, and moves figures to their places:

- **Walk** (default): along AD-170's route at up to 1.9 tiles per second (about 1.3 m/s with a 0.7 m tile), accelerating at 4.5 and braking at 3.0 tiles/s^2 so it stops on its spot; legs swing 24 degrees and arms 20 degrees in opposition, with a 1.4 px bob, one half-cycle per 0.5 tile; facing follows the screen velocity smoothed over 0.12 s, changing only past 30 % (horizontal) and 22 % (vertical) of the speed; on a chair or seat it pauses 0.35 s, then sits, sliding into the seat over 0.3 s.
- **Glide** (walking off): a straight ease-in-out move to the spot in `GLIDE_MS = 320`.
- **Reduced motion** (system setting, read at each move): placed at once, no fade.
- **Appear and leave**: a new key fades in over 320 ms on the exit node and walks in; a key gone from the layout walks to the exit and fades out over 320 ms (at once under reduced motion). On the first layout of a run every figure is placed at its place at once, with no walk and no fade.
- **The loop**: one `requestAnimationFrame` loop, started when a layout changes a place or a fade begins, with frame time capped at 0.1 s, stopped when nothing moves; it writes `data-moving` (`1` or `0`) on the scene element when that changes, which the e2e waits on. A new layout during a walk re-routes from the walker's position.
- **Durations** are constants of `mover.ts`, exported for the tests.

### AD-173: Furnishing and looks

- **Furniture**: every room is furnished as the prototype: reception (counter with a screen, coat rack, plants), decision office (wide desk with two screens, the queue rail, a plant), offices (four desks with pedestals, keyboards, one or two monitors, and props: mug, papers, lamp, succulent, books, or none), lounge (sofas, armchairs, coffee table, coffee corner, bookshelf, floor lamp, plants), alert room (beacon, status board, plant).
- **Seeds**: each office's details come from `mulberry32(fnv1a(slotId) ^ constant)`, so an office looks the same on every start and different from its neighbours. Seeds come from the slot, not from session data. The functions live in `furniture.ts`, pure, with no `Math.random`.
- **Looks**: an employee's skin, hair colour and hair style are picked from the allowed pairs (D-478) by the FNV-1a hash of its card key, so a session keeps its look for its life; the look carries no meaning.

### AD-174: The taskbar overlay badge and attention

**Verified on 2026-10-28:**

- `WebviewWindow::set_overlay_icon(Option<Image>)` is **Windows only**; `None` removes the overlay. The app calls it from Rust since v0.1 (`app/src-tauri/src/main.rs`, `waiting_dot`, Tauri 2.12.5 in `Cargo.lock`). The JavaScript form needs `core:window:allow-set-overlay-icon`, not in `core:window:default` ([window namespace](https://v2.tauri.app/reference/javascript/api/namespacewindow/)).
- `request_user_attention(Option<UserAttentionType>)`: "no effect if the application is already focused". On Windows, `Informational` "flashes the taskbar button until the application is in focus"; `Critical` "flashes both the window and the taskbar button until the application is in focus". The JavaScript form needs `core:window:allow-request-user-attention` ([window namespace](https://v2.tauri.app/reference/javascript/api/namespacewindow/); Rust signature on [docs.rs, tauri 2.12.5](https://docs.rs/tauri/2.12.5/tauri/webview/struct.WebviewWindow.html)).
- `set_badge_count` is **unsupported on Windows** ("use `set_overlay_icon` instead"); `setBadgeLabel` is macOS only. Neither is used.

**Design.** `WindowEvents::board` in the app's `main.rs` already sets the title and the overlay on each emitted board; it now:

- draws the overlay in code (no asset): a disc with the waiting count (`waiting_count`, 1 to 9, "9+" above) in a small bitmap digit font, amber with dark digits, or `--color-alert` with light digits while `alert_count > 0`, inside the dark rim of today's dot; no overlay when nothing waits. The pixel size is the one gate 2 of AD-178 finds legible at 100 % and 150 % display scaling (16, 24 or 32 px);
- sets the title to `Officina (N waiting)`, or `Officina (N waiting, M in alert)`;
- keeps the set of alerted card keys of the last emitted board and calls `request_user_attention(Some(UserAttentionType::Informational))` once when a key appears that the set did not hold. Focusing the window ends the flash (documented). The flash is not repeated for a known key.

The calls are Rust, so no capability is added. The overlay's colours are constants of the app crate, and a Rust test reads `app/src/app.css` and asserts they equal the light `--color-attention`, `--color-attention-text` and `--color-alert` values (`derived-not-transcribed`). The new-alert decision is a pure function (`alerts_to_announce(previous, board) -> bool`) with unit tests. No new dependency.

### AD-175: The alert band and the beacon

- **Band**: `AlertBand.svelte`, under the header and above the other banners, in every view and mode while `board.alert_count > 0`; `role="status"`; amber background (`--color-attention`, `--color-attention-text`) with a 3 px `--color-alert` bottom border and an "Alert" pill in `--color-alert` and `--color-on-alert`. Text: "{name} ({project}) is blocked: {waiting reason}, {cause}, and N more." where cause is "high-priority project" or "waiting N min" (worded from `waiting_since_ms` on the board store's 30 s tick, outside the scene). The first alerted session is the first in board order. No dismiss: the band leaves when nobody is in alert. Session text renders as text.
- **The theme never changes** (FR-8).
- **Beacon**: the alert room's lamp is lit (`--color-alert`) while the room holds someone; its glow animates `opacity` only, under AD-65's memo keyed on the first alerted card's `(key, waiting_since_ms)`, two minutes in the shared phase, then steady; steady under reduced motion. The prototype's rotating beam is drawn static (D-474). The status board's "!" turns `--color-alert`, and the room's floor and hazard outline take `--scene-alert-floor` and `--color-alert`.
- **Plaques**: the alert room's plaque is `--color-alert` with its count while occupied; an office of a high-priority project carries a small "high priority" pill beside its plaque.

### AD-176: Sub-agents, only on confirmed signals

**Verified on 2026-10-28** ([hooks reference](https://code.claude.com/docs/en/hooks)):

- `SubagentStart` "runs when Claude spawns a subagent with the Agent tool, when Claude resumes a subagent, and each time an in-process agent team teammate handles a new message"; its input example carries `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `agent_id` ("the unique identifier for the subagent") and `agent_type`. It cannot block.
- `SubagentStop` "runs when a Claude Code subagent has finished responding"; its input carries `session_id`, `permission_mode`, `stop_hook_active`, `agent_id`, `agent_type`, `agent_transcript_path` (the sub-agent's own transcript, in a nested `subagents/` folder), `transcript_path` ("the main session's transcript"), `last_assistant_message`, `background_tasks` and `session_crons` ("scoped to the parent session").
- "Not every SubagentStop event comes from a subagent Claude spawned": prompt suggestions and `/btw` side questions trigger it too, with `agent_type` the session's own agent name or empty.
- Tool events fired inside a sub-agent carry `agent_id` and `agent_type`.
- **Not stated**: whether `session_id` is the parent's (the examples show the main transcript), and behaviour in `--bg` and `-p` sessions and for nested sub-agents. Gate 1 of AD-178 settles it.

**Design, only if gate 1 passes:**

- Doctor's `BUFFERED_EVENTS` gains `SubagentStart` and `SubagentStop`, so the printed hooks block registers them (synchronous, `timeout: 5`, like the others); the doctor check "hooks registered" covers them. The author pastes the block; Officina writes no Claude configuration.
- The event line gains `agent_id` and `agent_type`, each cut to 64 characters on a char boundary; `last_assistant_message` and the transcript paths are never recorded. `core::contract`'s hook fields gain both events' fields for the canary.
- Core folds, per session id: a sub-agent is running from its first `SubagentStart` to the first later `SubagentStop` with the same `agent_id`; a repeated start is the same sub-agent; a stop without a start is ignored (the internal agents above); `SessionEnd` and a superseding `/clear` end them all. `Card.subagents: Vec<SubagentView { agent_id, agent_type }>` lists them in start order, at most 16.
- The scene draws at most three beside the lead's desk (offsets of the prototype), even while the lead stands elsewhere, since the sub-agent keeps working in the office; beyond three, the lead's file lists the rest. A lead without a desk shows its sub-agents in its file only. A sub-agent figure is static once placed: the prototype's blinking dots are dropped (FR-3).
- **If gate 1 fails**, none of this ships: no event registered, no field, no figure; nothing is inferred from tool events or transcripts.

### AD-177: Instruments in the header, the line under the scene

The prototype dropped the scene's instruments to keep the eye on movement, and the header already holds them in every mode: the quota gauges (v0.1) and the relay chip and approvals pill (v0.2). The scene therefore draws no lamp, no dials, no shelf and no cabinet. `SceneStrip.svelte`, outside the scene element, under it, shows two buttons: "N to resume" and "N done in 24 h" (worded from `done_window_ms`), each opening Board mode. Gives up: a scene-only glance at quota on a second screen, which the header's gauges already give. Retired: D-115, D-116, D-183, D-100, D-112 (D-472).

### AD-178: Slice 0 checks, exact procedures

Each runs on the owner's workstation, records a research-notes entry (version, date, what was seen; numbered when written) and leaves no session behind.

| # | Procedure | Pass | On failure |
|---|---|---|---|
| 1 | Sub-agent signals. In a trusted scratch folder, the owner adds to that folder's `.claude/settings.local.json` a logging hook on `SubagentStart`, `SubagentStop` and `UserPromptSubmit` that appends the raw payload's `session_id`, `agent_id`, `agent_type`, `hook_event_name` and `cwd` to a file. Cases: an interactive session asked to run one Explore sub-agent; two in parallel; one launched in the background that outlives the lead's turn; a `claude --bg` session spawning one; a nested sub-agent if the version allows it; a `/btw` side question. Record the payloads, redacted | `SubagentStart.session_id` equals the lead's `UserPromptSubmit.session_id`; each started `agent_id` has one later stop; the `/btw` stop has no start | FR-12 ships nothing (AD-176); story 4.9 stays approved and unimplemented |
| 2 | Taskbar. `cargo run -p officina-app --example taskbar_probe` (an example of the app crate, no change to the app): sets overlays for 1, 9 and "9+", amber then alert colour, at 16, 24 and 32 px, five seconds each, then, after a 5 s delay for the owner to focus another window, calls `request_user_attention(Informational)`; then calls it again with the probe focused. Observed at 100 % and 150 % display scaling, with a light and a dark taskbar | the badge is legible at one size; the button flashes until the probe is focused; nothing happens when focused | an illegible count: the overlay keeps today's dot, recoloured on alert, and the count stays in the title; no flash: the band and the red badge carry the alert alone |
| 3 | Walking CPU on the prototype. `perf-run.mjs` gains two pages: the v2 prototype in v0.4 mode running `busy` at 1x for 10 minutes with walking on, and the same with walking off; same method as section 1 (GPU path, 600 s, % of one core, % of the machine, main busy, recalcs) | the walking run under 1 % of the machine | walking ships off by default (D-480) and AD-71's measurement decides again on the real window |

## 3. Amendments to earlier decisions

- **AD-4**: the collector also recomputes, with no spawn, when `now_ms` reaches the last board's `next_escalation_at_ms` (AD-168).
- **AD-13**: `Board` gains `floor_projects`, `alert_count`, `alert_after_ms`, `next_escalation_at_ms`; `Card` gains `last_message_tail`, `scene` and, if gate 1 passes, `subagents`; `ProjectView` gains `priority`.
- **AD-16**: if gate 1 passes, the hooks block registers seven events (AD-176).
- **AD-19**: the overlay carries the waiting count and turns to the alert colour; `request_user_attention` on a new alert; the title counts alerts (AD-174).
- **AD-20**: superseded for the frontend layout by AD-72.
- **AD-35, AD-55**: the "Board" view is renamed "Sessions"; digits 1 to 9 work in every mode of the Sessions view.
- **AD-61**: superseded by AD-169 for the composition.
- **AD-64**: replaced as the default by walking (AD-172); the lint allows a frame loop in `mover.ts` only.
- **`app.css`**: the `halo-breath` keyframes go (AD-65); the global `prefers-reduced-motion` rule stays and covers pseudo-elements (D-180).
- **Floor schema** (`officina.toml`): `[[project]] priority` and `[alert] after_minutes` (AD-168); `ProjectColor` gains `plum`.

No dependency is added. Rejected: `axe-core`, a FLIP or animation library (the mover is about 300 lines of plain TypeScript), any canvas or WebGL library, a tray or notification plugin (FR-11 uses the window API).

## 4. Touch points with v0.2, v0.3 and later versions

- Slice A uses `Board` only; relay approvals reach the scene through `board.approvals` and `Card.scene`.
- `core/board.rs` (`floor_projects`, `scene`, alert fields, schema bump), `core/scene.rs` (new), `core/floor.rs` (`priority`, `[alert]`, `Plum`), `core/contract.rs` (sub-agent fields, if gate 1 passes); `host/watch.rs` (escalation recompute); `host/doctor.rs` (buffered events, if gate 1 passes; floor key checks); the app's `main.rs` (overlay, title, attention).
- `ipc.ts` (none new), `e2e-mock.ts` (the scene reads `get_board`), `Header.svelte` (view, mode, walking and theme controls), `App.svelte` (alert band), `SessionCard.svelte` and `app.css` (halo, tokens), `BoardColumn.svelte` (tokens).
- `e2e-fixtures.mjs` gains floor files with priorities and the scene scenarios; `board.spec.ts` keeps passing unchanged in Board mode.
- **v0.5** places its orchestrator in the decision office and briefs arrivals at reception (prd-v0.5.md section 9, architecture-v0.5.md section 7); the staff nodes and the consult spot of AD-170 are reserved for it.
- **v0.6**'s scene workers (architecture-v0.6.md AD-122 Slice B) map onto these zones: a live worker at a desk of its project's office; a package awaiting review in the decision office's queue, like a pending approval. v0.6 confirms this when its scene story is written.
- **v0.7**'s helpers walk in through the decision office (prd-v0.7.md section 9, architecture-v0.7.md section 7).

## 5. PRD amendments

The first version's amendments 1 to 11 were applied to the PRD text on 2026-10-28. The scene rework edited the PRD directly; its Change Log (section 8) lists what changed, and epic-4-decisions.md D-460 to D-485 record why.

## 6. Risks

| # | Risk | Mitigation |
|---|---|---|
| R25 | WebView2 155 composites differently from Chromium 156 | HTML halo layer (AD-59); compositing guard e2e; AD-71 measurement; steady halos as the fallback FR-9 already names |
| R26 | A high-refresh display multiplies animation cost | two-minute breathing limit; the frame loop stops when nothing moves; refresh rate recorded with the measurement |
| R27 | The drawing reads as clutter at low occupancy | empty desks quiet; owner review of the prototype (done, 2026-10-28) and of the baselines before they are committed |
| R28 | Font metrics differ between the prototype and the app | 140 px plaque cap with CSS ellipsis; geometry checked at the cap |
| R29 | Slot memory keeps a stale order after projects are renamed | pruned at start; reset when the floor name changes; harmless when lost |
| R30 | v0.2 or v0.3 not merged when Slice A starts | Slice A reads the board only |
| R31 | A walk, a hover or a re-render mutates the scene with no payload change once settled | per-figure write cache, the mover's stop, CSS-only name tags, the mutation e2e after `data-moving="0"` |
| R32 | WebKitGTK at v1 composites SVG and HTML layers differently | AD-59 keeps the portable layer; re-measure at v1 |
| R98 | Walking costs more CPU than FR-9 allows on the real window | gate 3 on the prototype first; AD-71's walking run; walking off by default as the fallback (D-480); the loop runs only while someone moves |
| R99 | `SubagentStart` does not carry the lead's session id, or stops arrive without starts | gate 1 before any sub-agent code; fold by `agent_id`, stops without a start ignored; on failure nothing is drawn (FR-12) |
| R100 | The prototype's spots are too dense for disjoint 44 px targets | AD-70's property decides; spots spread or removed; zone groups take the overflow (D-484) |
| R101 | A wait's start is unknown, so a long wait never escalates | stated on the file ("since unknown"); priority still escalates; 4.12 records how often it happens |
| R102 | The overlay count is illegible at the taskbar's size or scaling | gate 2 picks the size; fallback to today's dot recoloured, the count in the title |
| R103 | Depth order flickers in WebView2 although it does not in Chromium and Firefox | AD-171's hysteresis and snapping; the stable-figures e2e; the WebView2 look in 4.12 |
| R104 | The frame loop never stops after an interrupted walk and burns CPU | `data-moving` and the mover's unit cases; the truthful-motion e2e waits for it and fails on a timeout |
| R105 | More than four projects make the canvas larger than the default window | growth only with the fifth project; scroll below the minimum area (unchanged rule); the owner's real project count read in 4.12 |
| R106 | A missed `SubagentStop` leaves a sub-agent drawn forever | cleared at `SessionEnd` and on `/clear`; the file shows its start time; recorded in 4.12 if seen |

## 7. Change Log

- 2026-10-28 created and approved.
- 2026-10-28 reworked after the owner approved the interactive scene prototype v2: AD-59, AD-60, AD-62 to AD-72 amended in place, each amendment marked; AD-61 superseded by AD-169; AD-167 to AD-178 added; sections 3, 4 and 6 updated; R98 to R106 added. Decisions: epic-4-decisions.md D-460 to D-485.
