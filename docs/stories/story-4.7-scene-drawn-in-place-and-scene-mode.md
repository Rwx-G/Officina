# Story 4.7: The scene drawn in place, and Scene mode

**Status:** Approved

## Story

As the author with the window on a second screen,
I want a Scene mode that draws the furnished company from the payloads, scaled to the window, with every employee at its place and moved there at once when its state changes,
so that I see at a glance who waits and who works where, and nothing moves while nothing happens, before any walking exists.

## Acceptance Criteria

1. View store and header (AD-66, D-113, D-185, D-481): `app/src/lib/view.svelte.ts` holds the current view and `ViewState` with `base`, read field by field with its default; the header's view switch reads Sessions, Subjects, Relay; on Sessions, `ModeSwitch.svelte` is a `radiogroup` of Board and Scene; `m` switches between them; Board is the default (`DEFAULT_BASE`); the mode survives a reload; the alert band and the banners stay above the sessions area; the gauges have a fixed width. Proof: Playwright `app/e2e/modes.spec.ts > the header names the Sessions view`, `> the mode switch offers Board and Scene, Board by default`, `> m switches between Board and Scene and ignores a held key`, `> the mode survives a reload`, `> an unreadable stored view falls back to Board`, `> banners stay above the scene`; Vitest `app/src/lib/view.test.ts > stored values are read field by field with their defaults`, `> a storage that throws keeps the defaults`.
2. Layers (AD-59 as amended, AD-72): `components/scene/Scene.svelte` is the scene element (`data-testid="scene"`, `tabindex="-1"`, no `role="img"`, `data-moving="0"`, which story 4.10's mover takes over) and stacks `SceneFloor.svelte` (layer 1), `SceneHalos.svelte` (layer 2: one element per employee at a decision office or alert room place, `data-halo` with its card key, keyed by key and `enteredAt`, using story 4.4's `breathe`), `SceneObjects.svelte` (layer 3, mounting `lib/scene/objects.ts` with an action), `SceneOverlay.svelte` (layer 4: `Plaque.svelte`, `Register.svelte`, HTML in `foreignObject`, D-106), and a slot for story 4.8's targets; layers 1 to 4 are `aria-hidden="true"` with `pointer-events: none`; the text alternative is a visually hidden element inside the scene element. Proof: Vitest `app/src/components/scene/scene.test.ts > layers 1 to 4 are hidden from assistive technology and ignore the pointer`, `> one halo per employee in the decision office or the alert room`, `> plaque and register names render as text`; Playwright `app/e2e/scene.spec.ts > the scene draws every live session of each fixture`: on every scene board of the fixture index, the `data-figure` keys equal the layout's employee keys, and every live card key appears exactly once among figures and group lists.
3. Objects and figures (AD-171 as amended, AD-173, D-477, D-478): `objects.ts` builds statics from `furniture.ts` (seeded per office slot) and one persistent rig per card key from `rig.ts` (three pose groups built once, front and back views, flip, the cap in the project colour, marks and under layers); a placed figure sits among statics by its depth `gx + gy` (its seat's when seated; ties by creation order); positions are snapped to whole device pixels; each attribute is written only when its cached value changes; a new key's node is created at its place and a departed key's node is removed; rig and furniture markup take closed types only. Proof: Vitest `app/src/lib/scene/rig.test.ts > a rig has three pose groups and two views`, `> a pose change toggles visibility and rebuilds nothing`, `> markup holds no text node for every look, pose and colour`; `app/src/lib/scene/objects.test.ts > a placed figure sits among statics by its depth`, `> a settled figure receives no write`, `> a new key's node appears at its place and a departed key's node is removed`; `app/src/lib/scene/furniture.test.ts > an office looks the same for the same slot and differs between slots`, `> the furniture builders use no Math.random`; `app/src/tokens.test.ts > the allowed looks are the skin and hair pairs at 3:1 or more`.
4. Placed at once (FR-3 under reduced motion, D-497): a changed board moves each figure whose place changed to its new place in the same update, with its new pose, and nothing else; no transition, no fade, no CSS animation inside a figure. Proof: Playwright `scene.spec.ts > a changed board places the figure at its new place at once`, `> no CSS animation runs inside a figure` (`document.getAnimations()` holds none whose target is inside a `[data-figure]`).
5. Truthful motion (PRD section 7, AD-64 as amended, AD-70): Scene open on `scene-c` with `openScene`, which waits for `data-scale` and `data-moving="0"`; a `MutationObserver` on `[data-testid=scene]`; the clock advanced 150 s; the same board pushed again: zero records; then a changed board: records. Proof: Playwright `scene.spec.ts > nothing in the scene changes while the payloads are unchanged`.
6. Scale (AD-61 as amended, D-105, D-185, D-506): `s = clamp(min(areaWidth / canvas.w, areaHeight / canvas.h), MIN_SCALE, MAX_SCALE)` for the layout's column count, or with story 4.6's scene viewing scale as the lower bound if the owner chose it, written with `data-scale` in the same write as the transform; below the minimum area the scene keeps the lower bound and the area scrolls; snapping recomputes on a scale change. Proof: Playwright `scene.spec.ts > the scene's scale follows the sessions area`, `> below the minimum area the scene keeps the minimum scale and the area scrolls`, and, with growth, `> a fifth project widens the canvas and rescales once`.
7. No clock in the scene (AD-64 as amended, D-110, D-470): `eslint.config.js` restricts in `src/lib/scene/**` and `src/components/scene/**` the globals `setInterval`, `setTimeout`, `requestAnimationFrame` and the properties `Date.now`, `performance.now`, `Math.random`, with no override; story 4.10 adds the mover's. Proof: `pnpm lint`; Vitest `app/src/lib/scene/lint.test.ts > each restricted name is reported under both scene globs and not outside them`.
8. Light tokens and baselines (AD-67 as amended, D-117, D-119, D-508): the prototype v2's light scene, figure, people and project tokens join `app.css` (story 4.5 added the alert ones); every light role pair meets its target; `scene-a`, `scene-b` and `scene-c` match their light baselines with `maxDiffPixelRatio: SCREENSHOT_MAX_DIFF_RATIO`; the baselines come from the CI snapshot run, and the owner's comparison with `scene-interactive.png` is written in Completion Notes with its date and verdict before they are committed. Proof: Vitest `tokens.test.ts > every role pair meets its contrast target in the light theme`; Playwright `scene.spec.ts > the three densities match their light baselines`; the Completion Notes entry.
9. The alert in Scene mode (FR-8, FR-10, D-473): story 4.5's band case covers Scene mode, and its theme case covers the `--scene-*` tokens. Proof: Playwright `alert.spec.ts > the band shows in every view and mode`, `> an alert changes no theme token`, extended here.
10. Board mode unchanged: `app/e2e/board.spec.ts` passes as it is. Proof: CI Playwright.
11. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: Scene mode and `m`; Changed: the Board view is named Sessions); README section "The scene (v0.4, in progress)": the modes, `m`, Board as default, the zones. Proof: review.

## Tasks

- [ ] `lib/view.svelte.ts` (`base`), `ModeSwitch.svelte`, `Header.svelte` (fixed gauge width), `App.svelte` (keys, sessions area) (AC 1)
- [ ] `furniture.ts`, `rig.ts`, `objects.ts` and their tests (AC 3)
- [ ] `components/scene/{Scene,SceneFloor,SceneHalos,SceneObjects,SceneOverlay,Plaque,Register}.svelte`; `memory.load` before the first layout, `memory.commit` from an effect (D-184); `data-scale`, `data-moving` (AC 2, 4-6)
- [ ] `eslint.config.js` blocks and `lint.test.ts` (AC 7)
- [ ] Light tokens in `app.css`; contrast and looks cases in `tokens.test.ts` (AC 3, 8)
- [ ] `app/vitest.config.ts` `setupFiles`: jsdom stubs for `matchMedia` (reduced motion on by default), `ResizeObserver`, `Element.animate`
- [ ] `.github/workflows/ci.yml`: the `workflow_dispatch` snapshot input and artifact upload (D-119)
- [ ] `app/e2e/fixtures.ts`: `openScene`, `SCREENSHOT_MAX_DIFF_RATIO`; `scene.spec.ts`, `modes.spec.ts`; the alert cases extended; baselines from CI after the owner's comparison, recorded (AC 1-10)
- [ ] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-1, FR-2, FR-3, FR-7, FR-9; PRD section 7 (truthful motion). Decisions: AD-59, AD-61, AD-64 to AD-67, AD-70, AD-72 as amended, AD-171, AD-173. Epic decisions: D-105, D-106, D-110, D-113, D-117, D-119, D-121, D-184, D-185, D-470, D-477, D-478, D-481, D-497, D-506, D-508.
- Dependencies: story 4.6 (layout, spots, the floor's outcome); story 4.4 (`breathe`); story 4.5 (the alert tokens and band).
- This story is a complete scene without walking (D-497): the first layout of every run and reduced motion place everyone at once in the walking version too, so nothing here is thrown away. Story 4.10 inserts the mover between the layout and `objects.ts`, adds the depth sort's hysteresis for moving figures, the fades and the lint override.
- Port from the prototype v2's `scene.js`: the "figure rig" section (`rigMarkup`, `headSVG`, `torsoSVG`, limbs), the "props and plants" and "static drawing" sections (`furniture.ts`), and from "rendering" `renderActor`, `staticIndexFor` and `updateSnap`. Its v2 header comment lists the four rules that removed the flicker; AC 3 holds the port to the ones that apply to placed figures (one node per figure, no CSS animation inside, writes only on change, snapping).
- The objects layer is imperative on purpose (AD-171): Svelte's keyed blocks would reorder figure nodes back once the depth sort moves them. Keep every other layer in Svelte templates, where session text renders with `{text}`.
- The prototype's per-tool screen flicker, the helpers' blinking dots, the ticking wall clock and the scrolling orchestrator screens are not ported (D-463, D-476, AD-169, v0.5).
- The alert room is drawn as an ordinary room here; its alert floor, beacon, status board mark and plaque state, and the halo's alert colour, are story 4.9's.
- The baseline comparison is the owner's gate for the drawing (R27, D-119, D-508): tell the owner the baselines show the scene settled, without the prototype's panels, taskbar mock or event log, and write his date and verdict in Completion Notes.

## Testing

- Vitest: `app/src/lib/{view,scene/rig,scene/objects,scene/furniture,scene/lint}.test.ts`, `app/src/tokens.test.ts`, `app/src/components/scene/scene.test.ts`.
- Playwright: `app/e2e/{scene,modes,alert,board}.spec.ts`.
- CI: the `workflow_dispatch` snapshot run.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created as story 4.4
- 2026-10-28 reworked after review (F3, F5 to F9, F20 to F22, F25, F27, F37, F38)
- 2026-10-28 renumbered 4.6 and rewritten for the scene v2 (D-460): five layers, the imperative object layer, persistent rigs, depth hysteresis and snapping, the mover with walking, glide and reduced motion, the stable-figures e2e; `seatMove` withdrawn (D-461)
- 2026-10-28 renumbered 4.7 and split after the plan challenge (D-497, D-506, D-508): this story draws the scene placed at once; walking, the mover, fades, the depth hysteresis, the walking toggle, the lint override and the stable-figures e2e move to story 4.10; the alert room's look to story 4.9; the baseline comparison is recorded
