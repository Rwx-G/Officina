# Story 4.6: The scene drawn, walking, and switched on

**Status:** Approved

## Story

As the author with the window on a second screen,
I want a Scene mode that draws the furnished company from the payloads, scaled to the window, where employees walk along the corridors when their state changes and then stay still,
so that I see at a glance who waits and who works where, enjoy the movement, and nothing moves while nothing happens.

## Acceptance Criteria

1. View store and header (AD-66 as amended, D-113, D-469): `app/src/lib/view.svelte.ts` holds the current view and `ViewState` with `base` and `walking` (default `true` through `DEFAULT_WALKING`, read field by field); the header's view switch reads Sessions, Subjects, Relay; on Sessions, `ModeSwitch.svelte` is a `radiogroup` of Board and Scene; `m` switches between them; Board is the default (`DEFAULT_BASE`, D-481); in Scene mode `WalkingSwitch.svelte` is a toggle button (`aria-pressed`) labelled "Walking"; mode and walking survive a reload; the banners stay above the sessions area; the gauges have a fixed width (D-185). Proof: Playwright `app/e2e/modes.spec.ts > the header names the Sessions view`, `> the mode switch offers Board and Scene, Board by default`, `> m switches between Board and Scene and ignores a held key`, `> the mode and the walking setting survive a reload`, `> an unreadable stored view falls back to Board with walking on`, `> banners stay above the scene`; Vitest `app/src/lib/view.test.ts > stored values are read field by field with their defaults`, `> a storage that throws keeps the defaults`.
2. Layers (AD-59 as amended, AD-72): `components/scene/Scene.svelte` is the scene element (`data-testid="scene"`, `tabindex="-1"`, no `role="img"`) and stacks `SceneFloor.svelte` (layer 1), `SceneHalos.svelte` (layer 2: one element per employee standing at a decision office or alert room place, `data-halo` with its card key, keyed by key and `enteredAt`, using story 4.4's `breathe`, `--color-alert` in the alert room), `SceneObjects.svelte` (layer 3, mounting `lib/scene/objects.ts` with an action), `SceneOverlay.svelte` (layer 4: `Plaque.svelte`, `Register.svelte`, the "high priority" pills, HTML in `foreignObject`, D-106), and a slot for story 4.7's targets; layers 1 to 4 are `aria-hidden="true"` with `pointer-events: none`; the text alternative is a visually hidden element inside the scene element. Proof: Vitest `app/src/components/scene/scene.test.ts > layers 1 to 4 are hidden from assistive technology and ignore the pointer`, `> one halo per employee standing in the decision office or the alert room`, `> plaque and register names render as text`; Playwright `app/e2e/scene.spec.ts > the scene draws every live session of each fixture`: on every scene board of the fixture index, once settled, the `data-figure` keys equal the layout's employee keys, and every live card key appears exactly once among figures and group lists.
3. Objects and figures (AD-171, AD-173, D-477, D-478): `objects.ts` builds statics from `furniture.ts` (seeded per office slot) and one persistent rig per card key from `rig.ts` (three pose groups built once, front and back views, flip, the cap in the project colour, marks and under layers), inserts figures among statics by depth with AD-171's hysteresis, snaps positions to whole device pixels, and writes each attribute only when its cached value changes; rig and furniture markup take closed types only. Proof: Vitest `app/src/lib/scene/rig.test.ts > a rig has three pose groups and two views`, `> a pose change toggles visibility and rebuilds nothing`, `> markup holds no text node for every look, pose and colour`; `app/src/lib/scene/objects.test.ts > a figure's index among statics changes only past the hysteresis`, `> a settled figure receives no write`; `app/src/lib/scene/furniture.test.ts > an office looks the same for the same slot and differs between slots`, `> the furniture builders use no Math.random`; `app/src/tokens.test.ts > the allowed looks are the skin and hair pairs at 3:1 or more`.
4. The mover (AD-172, D-469, D-470, D-483): `mover.ts` walks figures along `paths.ts` routes with the walk constants of AD-172, glides in `GLIDE_MS` when walking is off, places at once under reduced motion (read at each move), fades a new key in on the exit node and walks it in, walks a departed key to the exit and fades it out, places everyone at once on the first layout of a run, re-routes a walker from where it is when a new layout arrives, runs one `requestAnimationFrame` loop only while something moves, and writes `data-moving` on the scene element. Proof: Vitest `app/src/lib/scene/mover.test.ts` (injected clock and frame source) `> a walk follows the route with continuous positions and stops on the spot`, `> speed never exceeds WALK.vmax`, `> facing changes only past the velocity thresholds`, `> a glide ends in GLIDE_MS`, `> reduced motion places at once and fades nothing`, `> the first layout places everyone at once`, `> a new key walks in from the exit and a departed key walks out`, `> a new layout mid-walk re-routes from the current position`, `> the loop stops when nothing moves and data-moving follows`.
5. Stable figures (AD-70 as amended, the prototype's flicker check): on `scene-b`, a pushed board that moves one Working session to Needs you, stepped with `page.clock.runFor(16)` for 72 frames: the figure keeps one node from A to B, exactly one pose group and one view are visible in every frame, its opacity is 1, its largest step is under 3 logical px per frame, its facing never flips back within 8 frames, `document.getAnimations()` holds no animation whose target is inside a `[data-figure]`, and `data-moving` returns to `0` after arrival. Proof: Playwright `app/e2e/walk.spec.ts > a walking figure stays one stable node`, `> no CSS animation runs inside a figure`, `> the frame loop stops after arrival`.
6. Truthful motion (PRD section 7, AD-64 as amended, AD-70): Scene open on `scene-c` with `openScene`, which waits for `data-scale` and `data-moving="0"`; a `MutationObserver` on `[data-testid=scene]`; the clock advanced 150 s; the same board pushed again: zero records; then a changed board: records. Proof: Playwright `scene.spec.ts > nothing in the scene changes while the payloads are unchanged`.
7. Walking off and reduced motion (FR-3, D-469): with walking off, a state change moves the figure in under 400 ms with no route; under `emulateMedia({ reducedMotion: "reduce" })` the figure is at its new place in the next frame and no fade runs; turning walking off mid-walk places walkers at once. Proof: Playwright `walk.spec.ts > walking off glides in under 400 ms`, `> reduced motion places at once`, `> turning walking off mid-walk places the walkers`.
8. Scale (AD-61 as amended, D-105, D-185): `s = clamp(min(areaWidth / canvas.w, areaHeight / canvas.h), MIN_SCALE, MAX_SCALE)` for the layout's column count, written with `data-scale` in the same write as the transform; below the minimum area the scene keeps `MIN_SCALE` and the area scrolls; snapping recomputes on a scale change. Proof: Playwright `scene.spec.ts > the scene's scale follows the sessions area`, `> below the minimum area the scene keeps the minimum scale and the area scrolls`, `> a fifth project widens the canvas and rescales once`.
9. No clock in the scene but the mover's (AD-64 as amended, D-470): `eslint.config.js` restricts in `src/lib/scene/**` and `src/components/scene/**` the globals `setInterval`, `setTimeout`, `requestAnimationFrame` and the properties `Date.now`, `performance.now`, `Math.random`, with one override for `src/lib/scene/mover.ts` that allows `requestAnimationFrame`, `cancelAnimationFrame` and `performance.now` only. Proof: `pnpm lint`; Vitest `app/src/lib/scene/lint.test.ts > each restricted name is reported under both scene globs and not outside them`, `> the mover alone may use the frame loop and performance.now`.
10. Light tokens and baselines (AD-67 as amended, D-117, D-119): the prototype v2's light scene, figure, people, alert and project tokens join `app.css`; every light role pair meets its target; `scene-a`, `scene-b` and `scene-c`, settled, match their light baselines with `maxDiffPixelRatio: SCREENSHOT_MAX_DIFF_RATIO`; the baselines come from the CI snapshot run and the owner compared them once with `scene-interactive.png` before they were committed. Proof: Vitest `tokens.test.ts > every role pair meets its contrast target in the light theme`; Playwright `scene.spec.ts > the three densities match their light baselines`.
11. Default from gate 3 (D-480): `DEFAULT_WALKING` is `true` if story 4.1's gate 3 passed, `false` otherwise, with the default-mode case following. Proof: `modes.spec.ts > walking is on by default` or its `off` twin, matching the research-notes entry.
12. Board mode unchanged: `app/e2e/board.spec.ts` passes as it is. Proof: CI Playwright.
13. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: Scene mode, `m`, walking and the walking toggle; Changed: the Board view is named Sessions); README section "The scene (v0.4, in progress)": the modes, `m`, Board as default, the zones, walking, walking off, reduced motion. Proof: review.

## Tasks

- [ ] `lib/view.svelte.ts` (`base`, `walking`), `ModeSwitch.svelte`, `WalkingSwitch.svelte`, `Header.svelte` (fixed gauge width), `App.svelte` (keys, sessions area) (AC 1)
- [ ] `furniture.ts`, `rig.ts`, `objects.ts` and their tests (AC 3)
- [ ] `mover.ts` and its tests (AC 4)
- [ ] `components/scene/{Scene,SceneFloor,SceneHalos,SceneObjects,SceneOverlay,Plaque,Register}.svelte`; `memory.load` before the first layout, `memory.commit` from an effect (D-184); `data-scale`, `data-moving` (AC 2, 6, 8)
- [ ] `eslint.config.js` blocks and `lint.test.ts` (AC 9)
- [ ] Light tokens in `app.css`; contrast and looks cases in `tokens.test.ts` (AC 3, 10)
- [ ] `app/vitest.config.ts` `setupFiles`: jsdom stubs for `matchMedia` (reduced motion on by default), `ResizeObserver`, `Element.animate`
- [ ] `.github/workflows/ci.yml`: the `workflow_dispatch` snapshot input and artifact upload (D-119)
- [ ] `app/e2e/fixtures.ts`: `openScene`, `SCREENSHOT_MAX_DIFF_RATIO`; `scene.spec.ts`, `walk.spec.ts`, `modes.spec.ts`; baselines from CI after the owner's comparison (AC 1-12)
- [ ] Update `CHANGELOG.md` and the README (AC 13)

## Dev Notes

- Requirements: FR-1, FR-2, FR-3, FR-7, FR-9; PRD section 7 (truthful motion, stable figures). Decisions: AD-59, AD-64 to AD-67, AD-70, AD-72 as amended, AD-171 to AD-173. Epic decisions: D-105, D-106, D-113, D-117, D-119, D-121, D-184, D-185, D-469, D-470, D-477, D-478, D-480, D-481, D-483.
- Dependencies: story 4.5 (layout, places, paths); story 4.4 (`breathe`); story 4.1's gate 3 before merge (AC 11).
- Port from the prototype v2's `scene.js`: the "figure rig" section (`rigMarkup`, `headSVG`, `torsoSVG`, limbs), the "props and plants" and "static drawing" sections (`furniture.ts`), "movement" (`setGoal`, `advance`, `stepGlide`, `arrive`) and "rendering" (`renderActor`, `reorder`, `staticIndexFor`, `updateSnap`). Its v2 header comment lists the four rules that removed the flicker; AC 3 and AC 5 hold the port to them.
- The objects layer is imperative on purpose (AD-171): Svelte's keyed blocks would reorder figure nodes back while the depth sort moves them. Keep every other layer in Svelte templates, where session text renders with `{text}`.
- The prototype's per-tool screen flicker, the helpers' blinking dots, the ticking wall clock and the scrolling orchestrator screens are not ported (D-463, D-476, AD-169, v0.5).
- `page.clock` fakes `requestAnimationFrame`, so `runFor(16)` steps the mover deterministically; `openScene` waits with `expect.poll`, never with `requestAnimationFrame`.
- The first layout of a run places everyone at once (D-469): a restart of the window never makes the whole company walk in.
- Halos appear when an employee stands at its place in the decision office or the alert room, keyed by `(key, enteredAt)` so the memo keeps its phase across moves between the two.
- The baseline comparison is the owner's gate for the drawing (R27, D-119): tell the owner the baselines show the scene settled, without the prototype's panels, taskbar mock or event log.

## Testing

- Vitest: `app/src/lib/{view,scene/rig,scene/objects,scene/furniture,scene/mover,scene/lint}.test.ts`, `app/src/tokens.test.ts`, `app/src/components/scene/scene.test.ts`.
- Playwright: `app/e2e/{scene,walk,modes,board}.spec.ts`.
- CI: the `workflow_dispatch` snapshot run.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created as story 4.4
- 2026-10-28 reworked after review (F3, F5 to F9, F20 to F22, F25, F27, F37, F38)
- 2026-10-28 renumbered 4.6 and rewritten for the scene v2 (D-460): five layers, the imperative object layer, persistent rigs, depth hysteresis and snapping, the mover with walking, glide and reduced motion, the stable-figures e2e; `seatMove` withdrawn (D-461)
