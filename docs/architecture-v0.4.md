---
title: "Architecture addendum: Officina v0.4 - The Scene"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions; Slice 0 mock to be shown to the owner, R27)
inputs:
  - prd/prd-v0.4.md
  - (requirement challenge, not published)
  - (framing, not published; section 8)
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - (Slice 0 mock, not published: _bmad-output/planning-artifacts/design/scene-mock.html, git-ignored)
---

# Architecture addendum: Officina v0.4

AD-1 to AD-58 stay in force except where section 3 amends them; this addendum adds AD-59 onward. **Measured** marks a number taken on the workstation for this addendum (section 1). **Gate** marks a fact to check on the workstation before the dependent story closes.

The scene is a view. It reads the board, relay and registry payloads and decides nothing: no classification, no new source of truth, no clock. Every rule below serves one property the PRD makes testable: while the payloads are unchanged, the scene element does not change.

The Slice 0 mock (FR-0) is `_bmad-output/planning-artifacts/design/scene-mock.html`, rendered to `scene-mock.png` beside it. It was produced by a prototype of AD-60 (`design/prototype/`, git-ignored like the mock) from real `officina board --json` output: the CLI built on 2026-10-28 run against the fake claude with hand-written, redacted scenarios and floor files. The mock fixes the visual language; this addendum fixes how it is built.

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

**DOM size**, counted on the prototype output: 1,069 SVG elements and 17 targets for density (c); 1,380 SVG elements and 37 targets at full occupancy (nine waiting, every desk taken). The measured page (10 employees, static variant) holds 1,041 SVG elements and 1,049 DOM elements in all.

**What the numbers say.**

1. A static SVG scene costs nothing between payloads. All cost is animation.
2. In Chromium 156 an `opacity` animation on an SVG element is composited: no style recalculation, no main-thread work, same cost as the HTML layer. The challenge's belief that SVG children are main-thread painted does not hold for `opacity` in this engine. It is not a documented guarantee, and v1 adds WebKitGTK.
3. The board's `box-shadow` halo is the expensive one: a style recalculation per frame, 60 to 160 per second, 1.8 to 2.9 times the cost of the same ring animated by `opacity`.
4. Canvas needs a frame loop for any animation, which FR-9 forbids, and costs 5 to 9 times the SVG scene while it runs.
5. Any running animation costs per displayed frame. The GPU path draws about 170 frames per second here against 60 on the software path and costs about three times as much CPU (not a controlled comparison: the paths also composite differently). The two-minute breathing limit (FR-3), not the technique, is what keeps the scene's average cost near zero.

## 2. Decisions

### AD-59: SVG with HTML layers, not Canvas

The scene is SVG rendered by Svelte, inside a stack of four layers sharing one logical coordinate box:

| # | Layer | Content | Pointer |
|---|---|---|---|
| 1 | `scene` (SVG) | floor, walls, wall fixtures, rooms, desks, workshop figures | none |
| 2 | `halos` (HTML) | one element per waiting figure, `opacity` animation only, `will-change: opacity` | none |
| 3 | `fore` (SVG) | antechamber occupants and tickets, plaques, register | none |
| 4 | `targets` (HTML) | one transparent `<button>` per actionable element | all |

Layers 1 and 3 are `aria-hidden="true"`; layer 4 carries every name, role and focus. The halo sits on HTML rather than inside the SVG although Chromium 156 composites both equally (section 1): HTML `opacity` with `will-change` is composited by every engine Officina targets, including WebKitGTK at v1, and it costs one more `<svg>` element.

Rejected: **Canvas**, which needs a frame loop for the halo (FR-9), costs 26 % main-thread time on the software path, and gives up the DOM that the accessibility rules, the mutation e2e and the screenshot tests rely on. **One SVG with SVG halos**, kept as the fallback if layer splitting proves awkward: the measured cost is the same on Chromium.

### AD-60: The layout is a pure TypeScript function

`app/src/lib/scene/layout.ts` exports `layoutScene(input: SceneInput, memory: SceneMemory): SceneLayout`. No DOM, no clock, no storage, no `Math.random`. `SceneInput` is `{ board: Board; relay: RelayView | null; registry: RegistryView | null }`; `SceneMemory` is AD-62's remembered order; `SceneLayout` lists rooms, placed employees (place, pose, logical coordinates, ticket number), instruments, plaques with their maximum-width rectangles, targets with their rectangles and tab order, the register rows, the text alternative, and the next `SceneMemory`.

Why TypeScript and not the Rust core with ts-rs:

- It is geometry and presentation: logical pixels, a plaque width cap tied to the embedded font, the tab order of HTML buttons. None of it feeds a decision, a CLI or a file.
- It joins three payloads that arrive on three events (`board://updated`, `relay://updated`, `registry://updated`); only the window holds all three.
- Slot memory is per-machine view state (AD-62), which lives in the webview.

AD-13's rule holds: the frontend does not re-implement classification. The layout reads `card.column` and `card.badge` and maps them to a place and a pose; there is no second implementation to keep in step, so no vector set. Geometry constants live in `lib/scene/geometry.ts` and are imported, never retyped, by the tests.

Trade-off: the geometry is not exercised by the Rust suite on Linux CI. It is exercised by Vitest on every CI run, which is where it can break.

### AD-61: Composition, geometry and scaling

Fixed by the mock; the numbers live in `geometry.ts`.

- **One composition.** A 3 x 3 grid of square cells on a 2:1 dimetric floor with two back walls. The antechamber holds the back cell, at the top of the window where reading starts, beside the door and the register. Eight workshop slots fill the other cells in screen reading order (top to bottom, then left to right); that order is the slot order and the tab order. A new project fills a slot; nothing ever resizes or reflows the floor.
- **Tile** 72 x 36 logical px; z unit 30 px; cell 4 tiles; corridor 0.8 tile; logical canvas **1061 x 672 px**.
- **Figure**: 24 px wide, 58 px seated (64 with the raised hand), built from tokens: tunic (`--figure-tunic`, neutral), head (`--figure-head`, wood), hair (`--figure-hair`, ink), and a collar in the project colour, the only project-coloured part. Identity is carried by the room plaque and the accessible name, never by the collar alone (FR-6).
- **Poses** (FR-2), verified in greyscale in the mock: **Working** has its back turned (the head reads as hair, the chair back covers the torso) and its screen lit in `--color-working`. **Your turn** faces the author with a raised hand, screen dark, inside a 2.6 px `--color-attention` ring on the floor, the board's outline. **Unknown** faces the author with hands on the lap and a question mark. **Needs you** sits in the antechamber with its board number on the chest, on a halo. **Away**: a waiting employee's desk keeps an empty chair carrying the same number.
- **Antechamber**: nine seats in rows of three, pitch 1.38 tiles, row 0 the front row, filled in board order; the sign "Needs you N" hangs on the wall corner above. The **register**, in the free triangle above the left wall, names the waiting in board order with their numbers (the "door sign" of FR-2). Relative times are never drawn in the scene (AD-64).
- **Plaques**: filled pills (ink, or amber for the antechamber, or quiet surface for an empty workshop, the shelf and the archive) on each room's back-left partition, 0.8 tile from the back corner; 12 px bold; **capped at 140 logical px** with ellipsis, full name in the target's accessible name.
- **Walls**: left wall: door, relay lamp, the two quota dials ("5 h" left of "week", as in the header), a window. Right wall: a window, the to-resume shelf (folders with a project-coloured tab), the archive cabinet.
- **Hit areas**: figures 48 x 48 logical px, from 56 px above the chair point; instruments 44 to 60 px. A check over the full-occupancy layout (nine waiting, 32 desks, every plaque at 140 px with its "+n" chip target, lamp, dials, shelf, archive) finds no target overlapping another target or a plaque; the parameters above were chosen by searching that check (`design/prototype/search.mjs`).
- **Scale**: `s = clamp(min(areaWidth / 1061, areaHeight / 672), 44 / 48, 1.6)`, applied as one `transform: scale(s)` on the layer stack. The lower bound keeps every figure target at 44 x 44 CSS px. **Minimum sessions area 973 x 616 CSS px**, about a 1000 x 760 window with the header; below it the scene stays at the minimum scale and the area scrolls, while Board and Focus fit any size. The window's own limits (`minWidth` 720, `minHeight` 480, default 1320 x 780) do not change; at the default size `s` is about 1.0.

### AD-62: Slot and desk assignment

**Workshops (FR-1).** Order: floor projects in file order (from the new `Board.floor_projects`, AD-68); then projects remembered from earlier runs that have a card on the first board after start; then projects in order of first appearance since start (ties within one payload: earliest `started_at_ms`, then name). A project keeps its slot until restart, dimmed when it has no live session. Nothing reorders by activity. With more than eight projects, the first seven keep their cells and slot 8 becomes **More workshops**, whose plaque counts the extra projects and whose room lists their employees by name, one target each. "other" and "unknown" (v0.1 FR-9) are ordinary projects here.

**Desks.** Each workshop has four desks. A live session (Needs you, Your turn, Working) keeps its desk for as long as it is live, including while it waits in the antechamber; a new session takes the lowest free desk, in `started_at_ms` order; from the fifth live session in one project on, the plaque gains a "+n" chip at its right end, whose 44 x 44 target (right-aligned with the plaque, bottom on its bottom) lists those sessions by name.

**Antechamber.** Seats follow board order every time: when the first waiting session leaves, the others move up one seat. This is the one move that is not the mover's own state change; FR-2 (keys 1 to 9 match the board) wins, and the PRD amendment says so.

**Memory.** `{ v: 1, floor: string | null, slots: string[], desks: Record<string, (string | null)[]> }` in `localStorage["officina.scene.v1"]`, read once at start, written only when the assignment changes. Pruned at start: projects with no card on the first board and desks whose card key is not live are dropped; a change of `Board.floor` discards the memory. Card keys of live sessions survive an Officina restart (AD-11), so desks do too. Lost or cleared storage costs nothing but stability: the order falls back to floor, then first appearance.

### AD-63: Z-order, hit testing and keyboard

- **Z-order.** Inside layer 1, rooms are painted back to front by cell depth, and inside a room desks and figures by floor depth (`gx + gy`): a desk stands behind its chair, so nothing occludes a workshop figure. Layer 3 paints the antechamber occupants, plaques and register over everything; AD-61's check guarantees they cover no target.
- **Hit testing** is the browser's: layer 4 holds absolutely positioned `<button>`s at the layout's target rectangles, in logical px under the shared transform. Layers 1 to 3 have `pointer-events: none`. Targets never overlap (AD-61), so the topmost-element rule never has to arbitrate. A hovered or focused target shows a name tag through CSS (`:hover`, `:focus-visible`); the tag is pre-rendered inside the button, so hovering mutates nothing.
- **Focus ring**: 3 px `--color-working` outline with a 2 px offset, rounded to the target, visible on every scene token in both themes.
- **Tab order** (FR-6) is DOM order: antechamber in board order, then the register's rows beyond nine, then workshops by slot with desks front to back, then More workshops, shelf, archive, relay lamp, dials ("5 h", then "week").
- **Arrows** move focus to the nearest target in that screen direction within the same room (target centres, ties by tab order). **Enter** or a click opens the file with **Bring to front** focused, so Enter twice does what one Enter does on a board card. **Digits 1 to 9** act directly on the nth Needs you session, as on the board. **Escape** closes the file and returns focus to the figure.
- **Text alternative**: the scene element carries a visually hidden summary built by the layout, e.g. "5 need you. Oldest: billing-api, project billing-api, permission prompt. 2 your turn. 5 working in 8 workshops." Each target's `aria-label` states name, project, state, waiting reason and number.

### AD-64: Motion only on state change

- The scene component never reads `BoardStore.nowMs` and never draws a relative time. Board ages tick every 30 s (AD-5); the scene has none to tick. ESLint gains `no-restricted-globals` and `no-restricted-properties` for `setInterval`, `setTimeout`, `requestAnimationFrame` and `Date.now` in `src/lib/scene/**` and `src/components/scene/**`; the one clock read the halo needs lives in `lib/halo.ts`, outside those paths, and is called once per newly waiting key (AD-65).
- Figures are keyed by card key. **Within a room** a moved figure translates in 300 ms, ease-out. **Between rooms** (layers 1 and 3 are separate SVGs, so no element travels between them) the figure fades out in 120 ms at the old place and in over 200 ms at the new one. A **pose** change cross-fades in 200 ms. Every transition is triggered by a changed layout entry and stays under 400 ms (FR-3); none runs under `prefers-reduced-motion`. Durations live in `lib/scene/motion.ts`.
- An identical payload, including the same board re-emitted, produces an identical layout; Svelte then writes nothing to the DOM. The e2e of AD-70 asserts exactly that.

### AD-65: The halo, in the scene and on the board

- **Element**: in layer 2, one absolutely positioned element per waiting figure, `border-radius: 50%`, background `radial-gradient(closest-side, var(--color-attention) 45%, transparent)`, `will-change: opacity`, `pointer-events: none`, `aria-hidden`.
- **Breathing**: `@keyframes halo-breathe { from { opacity: 0.35 } to { opacity: 0.8 } }`, 2 s per direction, `alternate`, ease-in-out. Resting opacity 0.8, so the end of breathing is not a jump.
- **One shared phase**: `animation-delay = -(firstSeenMs mod 4000) ms`, so every halo is in phase with the wall clock whenever it mounts.
- **Two minutes**: `enteredAt` is the card's `last_event_at_ms` (the event that made it wait), else `started_at_ms`. `animation-iteration-count` is the smallest odd number of 2 s halves covering `enteredAt + 120 000 - firstSeenMs` plus the delay already elapsed, or 0 when the wait is older than two minutes at first sight (after a restart, for example). An odd count ends on `to`, the resting value.
- **Memo**: `lib/halo.ts` computes `{ delayMs, iterations }` once per `(card key, enteredAt)` and keeps it while the card waits, so a re-render never rewrites the style and never restarts the animation. The layout stays pure: it emits `enteredAt`; the component asks the memo.
- **Off**: no breathing under `prefers-reduced-motion` (the halo is steady from the start); no halo in Focus mode, which draws no scene.
- **Board fix (FR-9)**: `SessionCard.svelte` drops `animation: halo-breath` and `app.css` drops the `box-shadow` keyframes. A `::after` on `.card.needs_you` carries the ring (`box-shadow: 0 0 0 3px var(--color-attention), 0 0 0 7px` at 35 % amber) and animates only its `opacity`, through `--halo-delay` and `--halo-count` set from the same memo. The board follows the same two-minute rule. Measured: 15.4 to 5.4 % of a core on the software path, 22.6 to 12.3 % on the GPU path, style recalculations from 60 to 160 per second down to 0.

### AD-66: Modes of the sessions area, and view persistence

- The header's view switch becomes **Sessions, Subjects, Relay** (AD-35 and AD-55 named the first one "Board"). On Sessions, a segmented control (`radiogroup`) offers **Board, Scene, Focus**.
- Keys, active only in the Sessions view: **`m`** switches between Board and Scene; **`f`** turns Focus on, and off back to the previous mode; digits 1 to 9 work in all three modes (AD-55 limited them to the Board view). Triage keeps its own `f` in the Subjects view (AD-55); keys are view-scoped, so they never meet.
- **Focus** lists Needs you (numbered, amber border) and Your turn (amber outline) as board-style rows of at least 44 px, with Working as a count; no scene, no halo, no animation.
- `lib/view.svelte.ts` holds `{ mode, theme }`, persisted in `localStorage["officina.view.v1"]` (per machine, per FR-7). Default mode **Board**; it changes in code only once the adoption metric (PRD section 7) is met, by the owner's decision.

### AD-67: Dark theme and switching

- **Same roles, recomputed values.** Light values stay the framing's. Dark values were derived in the mock (`design/prototype/tokens.mjs`, shown with swatches and computed contrasts in `scene-mock.html`) and move once into `app.css`, which stays the only committed copy of the tokens. This addendum lists roles, not values, so it cannot drift from `app.css`.
- **Derivation rules**: backgrounds are warm near-blacks of the light hues; text roles invert; amber stays `#F5B731` with the same text on it; `--color-attention-border` brightens to keep at least 3:1 on the dark background; `--color-working` takes the dark logo's slate blue (`docs/assets/logo-dark.svg`); project colours lighten to at least 3:1 on the dark surface; scene materials keep their order (top lighter than the left face, lighter than the right face) and figures gain a light rim (`--figure-rim`) so the ink hair still reads on dark floors.
- **Contrast targets**, computed in the mock for both themes and all met: ink on page and surface 7:1; secondary text 4.5:1; text on amber 7:1; antechamber border 3:1; working colour and project colours 3:1 on surface; figure tunic on room floor and hair on head (the pose cue) 3:1. A Vitest test parses `app.css`, computes WCAG contrast for each role pair in both themes and fails under target; the pairs are a constant in the test, the values are read, never retyped.
- **New tokens** replace colours hard-coded today, so dark mode covers the whole window: the error banner (`App.svelte`: `#f6e3dc`, `#c0603c`), the broken-contract text (`Header.svelte`: `#8a2b14`), the column washes (`BoardColumn.svelte`: `rgb(255 253 248 / 0.55)`, `rgb(245 183 49 / 0.12)`). Names: `--color-danger`, `--color-danger-surface`, `--color-column`, `--color-attention-wash`. Scene material tokens (`--scene-*`, `--figure-*`) join `app.css` in the same block.
- **Switching**: dark values apply under `:root[data-theme="dark"]` and under `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) }`. The header's theme control cycles **Auto, Light, Dark**; `main.ts` sets `data-theme` from `officina.view.v1` before mounting, so there is no light flash. The header logo follows the theme (`logo-dark.svg`). **Gate:** WebView2 updates `prefers-color-scheme` live when Windows switches theme with Tauri's window `theme` left unset; checked by hand once, recorded in research notes.

### AD-68: Payloads, additions and graceful absence

- **`Board.floor_projects: Vec<ProjectView>`** (new, core): the floor's projects in file order, empty without a floor. Needed for FR-1's floor-file order, which no current field carries. `BOARD_SCHEMA` goes up by one after v0.2's and v0.3's bumps (AD-54's rule: whoever merges last bumps last).
- **`RegistryView.archived_count: u32`** (new, core, one `COUNT`): FR-1's archive count; v0.3 only lists archived subjects on demand (`list_archived`).
- **Approvals** (v0.2 AD-35): the lamp's count is `board.approvals.length`; activating the lamp opens the Relay view, where Approve and Reject stay pointer-only. No key approves (AD-35).
- **Absence** is ordinary input, not an error:

| Payload | Present | Absent (not built, not received yet, or unavailable) |
|---|---|---|
| `RelayView` | lamp drawn: unlit when stopped, `--color-working` when running with nothing pending, amber with the count when an approval or catalogue change waits | no lamp, no lamp target |
| `RegistryView` | shelf folders are the subjects with status `to_resume`; the cabinet counts `archived_count` | shelf folders are the board's To resume cards; the cabinet counts the board's Done column, labelled "Done, 24 h" |
| `Card.subject` | the file shows the subject | the file omits the line |
| `Board.floor_projects` | floor order first | first appearance only |

- `SceneInput` types `relay` and `registry` as nullable from the start, so Slice A needs only the board and can merge before or after v0.2 and v0.3.

### AD-69: The file panel

A side panel (`SessionFile.svelte`, 420 px wide, the board card's radius and tokens) opened from a scene target: project, number when waiting, name, state with the waiting reason as an amber pill, subject (v0.3), last prompt, last answer head and tail (v0.3 AD-45), and the card's actions with the same outcomes (`store.focus`, copy attach command, wrong window). All session text renders as text (`svelte/no-at-html-tags`). It lives outside the scene element, so opening it never mutates the scene. Escape closes it and returns focus to the figure.

### AD-70: Test strategy

| Layer | What | How |
|---|---|---|
| layout unit | slot order (floor, memory, first appearance, pruning at start, floor change, More workshops); desks (kept while waiting, freed on end, overflow); antechamber board order and numbers; pose from `(column, badge)`; absent relay and registry; text alternative; identical input gives a deep-equal layout | Vitest on `layoutScene`, boards built from the e2e fixtures |
| geometry property | at full occupancy (nine waiting, every desk, every "+n" chip, More workshops, every instrument): no two targets intersect; no target meets a plaque at its 140 px cap; every figure target is 44 x 44 CSS px or more at the minimum scale | Vitest, geometry constants imported, never retyped |
| halo memo | shared phase; odd count; 0 for a wait older than two minutes; same output on a second call | Vitest with explicit times |
| tokens | WCAG contrast of each role pair in both themes, values parsed from `app.css` | Vitest |
| components | one button per target with its accessible name; DOM order equals AD-63's tab order; arrows, Enter, Escape; names with markup render as text; the file shows the card's actions | Vitest with @testing-library/svelte |
| fixtures | three scene scenarios (1 project and 2 sessions; 4 projects and 10 sessions; 8 projects, 12 sessions, 5 waiting) with floor files, run through the real `officina board --json` | `e2e-fixtures.mjs` gains `OFFICINA_FLOOR` and the scenarios of `design/prototype/fixtures.mjs` |
| truthful motion | Scene open, `page.clock.install`, a `MutationObserver` on `[data-testid=scene]` (subtree, attributes, child list, character data); advance the clock 150 s (five 30 s board ticks; CSS breathing runs on real time and never touches the DOM), push the same board again: zero records. Then push a changed board: records appear, so the observer was live | Playwright |
| compositing guard | with five halos breathing, `RecalcStyleCount` from CDP `Performance.getMetrics` does not grow over 3 s, in the scene and on the board | Playwright, Chromium only |
| keyboard and modes | `m`, `f`, digits in each mode, tab order, Enter then Enter brings to front, Escape; mode and theme survive a reload | Playwright |
| layout at size | at 1000 x 760 and 1400 x 900: every target's box is 44 x 44 or more and `elementFromPoint` at its centre is that target | Playwright |
| themes | `emulateMedia({ colorScheme: "dark" })` gives dark tokens; a manual override wins and persists | Playwright |
| readability | the text alternative names the Needs you count and the oldest waiting name, for every e2e fixture with someone waiting (the PRD's 2 s reading check stays manual) | Playwright |
| screenshots | the three densities in both themes, `toHaveScreenshot` with the clock installed and animations disabled; baselines generated on Windows, compared once by the owner against `scene-mock.png`, then committed | Playwright, `app/e2e/scene.spec.ts` |

Not automatable, recorded in research notes: AD-71's CPU measurement; AD-67's live theme gate; a look at the scene on the workstation's WebView2 at 100 % and 150 % scaling.

### AD-71: Measuring FR-9 on the workstation

The officina-app process and its `msedgewebview2.exe` children, 10 minutes, a fixture board of 10 employees and 5 waiting kept on screen through the e2e mock build, Task Manager's CPU column (or `Get-Counter "\Process(*)\% Processor Time"` summed over those processes). Recorded with the logical core count and the display refresh rate, in both units (% of the machine, as AD-17 measures idle CPU, and % of one core, which travels between machines). Three runs: halos breathing throughout (worst case), the real two-minute rule, halos steady. FR-9's threshold applies to the machine figure; if the breathing run exceeds it, halos are steady from the start.

### AD-72: Frontend structure and README assets

Amends AD-20. New files:

- `lib/scene/geometry.ts` (constants, projection), `layout.ts` (AD-60), `memory.ts` (AD-62 storage), `motion.ts` (AD-64 durations), `text.ts` (text alternative and accessible names);
- `lib/halo.ts` (AD-65 memo, shared by the scene and `SessionCard`), `lib/view.svelte.ts` (AD-66, AD-67);
- `components/scene/`: `Scene.svelte` (layer stack, scale), `SceneFloor.svelte` (layer 1), `SceneHalos.svelte` (layer 2), `SceneFore.svelte` (layer 3), `SceneTargets.svelte` (layer 4), `Figure.svelte`, `Desk.svelte`, `Plaque.svelte`, `Instruments.svelte`, `Register.svelte`;
- `components/SessionFile.svelte`, `FocusList.svelte`, `ModeSwitch.svelte`, `ThemeSwitch.svelte`.

README assets (`readme-assets.spec.ts`, AD-17): `docs/assets/scene.png` and `scene-dark.png` from density (c), and `scene.gif`: a working employee is called to the antechamber, its halo breathes, the author answers, it goes back to its desk. The board assets stay.

## 3. Amendments to earlier decisions

- **AD-13**: `Board` gains `floor_projects` (AD-68).
- **AD-20**: superseded for the frontend layout by AD-72.
- **AD-35, AD-55**: the "Board" view is renamed "Sessions"; digits 1 to 9 work in every mode of the Sessions view.
- **AD-54**: `RegistryView` gains `archived_count`.
- **`app.css`**: the `halo-breath` keyframes go (AD-65); the global `prefers-reduced-motion` rule stays and also covers the scene's transitions.

No dependency is added. Rejected: `axe-core` (the DOM checks of AD-70 cover the 44 px, overlap and naming rules for this view), a FLIP or animation library (Svelte's `animate` and `transition` suffice), and any canvas or WebGL library.

## 4. Touch points with v0.2 and v0.3

- Slice A uses `Board` only, plus `floor_projects`; it can merge before or after v0.2 and v0.3.
- `core/board.rs` (`floor_projects`, schema bump), `core` registry view builder (`archived_count`), `ipc.ts` (none new), `e2e-mock.ts` (the scene reads the existing `get_board`, `get_relay`, `get_registry`), `Header.svelte` (view and mode switches, theme), `SessionCard.svelte` and `app.css` (halo, tokens), `BoardColumn.svelte` and `App.svelte` (tokens).
- `e2e-fixtures.mjs` gains floor files and three scenarios; `board.spec.ts` keeps passing unchanged in Board mode.

## 5. PRD amendments

1. **FR-0.** Append: "Approved mock: tile 72 x 36 logical px, logical canvas 1061 x 672, figure 24 x 58 with a 48 x 48 hit area, plaques capped at 140 px, scale clamped to [44/48, 1.6], minimum sessions area 973 x 616 CSS px, below which the scene scrolls."
2. **FR-1.** Replace the first three sentences (up to "lists its employees by name.") with: "Workshops take slots in floor-file order, then in order of first appearance; the order is remembered on the machine, so a project with a session at the next start keeps its slot. An empty workshop keeps its slot, dimmed, until restart. Nothing reorders by activity. Beyond 8 projects, the eighth slot becomes 'More workshops', which lists the remaining projects' employees by name." Append: "Until the registry ships, the shelf shows the board's To resume column and the cabinet counts the board's Done sessions of the last 24 hours."
3. **FR-2.** Replace with: "Every Needs you session sits in the antechamber in board order, oldest first, wearing its number, so keys 1 to 9 match the board: rows of three, up to nine figures. A register beside the antechamber names them in the same order; beyond nine, it names the others, each focusable and actionable. A waiting session keeps its desk, which shows its number. Working employees sit at their desk with their back turned and their screen lit; Your turn employees face the author with a raised hand inside an amber ring (outlined, as on the board); an unknown state shows a question mark."
4. **FR-3.** Replace the first sentence with: "An employee changes place or pose only when its own state changes, or when a seat ahead of it in the antechamber frees up, so board order holds." Replace "no halo under `prefers-reduced-motion` or in focus mode" with "under `prefers-reduced-motion` halos are steady from the start; focus mode has no halo."
5. **FR-4.** Replace the parenthesis with: "(unlit: relay stopped; blue: running, nothing pending; amber with the count: an approval or a catalogue change waits; drawn only when the relay exists)".
6. **FR-6.** Replace "Enter acts as on the board" with "Enter opens the file with Bring to front focused, so Enter twice does what Enter does on a board card; keys 1 to 9 act as on the board."
7. **FR-7.** Replace "switched by one key and one header control" with "switched by `m` (Board and Scene), `f` (Focus on, then back to the previous mode) and a header control; these keys act in the sessions area only."
8. **FR-9.** Replace the last two sentences with: "Measured on the workstation over 10 minutes with 10 employees and 5 waiting, in % of the machine as v0.1 measures idle CPU, with the core count and display refresh rate recorded: under 1 %; above it, halos are steady from the start. The board halo follows the same rules, two-minute limit included."
9. **Section 3, Room.** Replace "the archive" with "the archive; beyond 8 projects, 'More workshops'" (the board already has an "other" project, so the overflow room is not called "other workshops").
10. **Section 7, Truthful motion.** Replace the "How measured" cell with: "e2e: clock installed and advanced 150 s, the same board pushed again, a mutation observer on the scene element: zero mutations; a changed board then produces some".
11. **Section 7, Idle CPU.** Replace the "How measured" cell with: "workstation, AD-71 protocol, both units recorded".

## 6. Risks

| # | Risk | Mitigation |
|---|---|---|
| R25 | WebView2 155 composites differently from Chromium 156 | HTML halo layer (AD-59); compositing guard e2e; AD-71 measurement; steady halos as the fallback FR-9 already names |
| R26 | A high-refresh display multiplies animation cost (the 170 Hz GPU path cost about 3 times the 60 Hz software path) | two-minute breathing limit; refresh rate recorded with the measurement |
| R27 | 32 desks read as clutter at low occupancy | empty desks at 42 % opacity (mock); owner review of the mock before Slice A |
| R28 | Font metrics differ between the mock and the app | 140 px plaque cap with CSS ellipsis; geometry checked at the cap, not at the measured width |
| R29 | Slot memory keeps a stale order after projects are renamed | pruned at start; reset when the floor name changes; harmless when lost |
| R30 | v0.2 or v0.3 not merged when Slice A starts | Slice A reads the board only; relay and registry are nullable inputs (AD-68) |
| R31 | A transition, a hover or a re-render mutates the scene with no payload change | keyed blocks, the halo memo, CSS-only name tags, the mutation e2e |
| R32 | WebKitGTK at v1 composites SVG and HTML layers differently | AD-59 already picks the portable layer; re-measure at v1 |
