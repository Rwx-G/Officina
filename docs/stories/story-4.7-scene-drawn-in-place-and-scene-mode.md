# Story 4.7: The scene drawn in place, and Scene mode

**Status:** In progress

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

- [x] `lib/view.svelte.ts` (`base`), `ModeSwitch.svelte`, `Header.svelte` (fixed gauge width), `App.svelte` (keys, sessions area) (AC 1)
- [x] `furniture.ts`, `rig.ts`, `objects.ts` and their tests (AC 3)
- [x] `components/scene/{Scene,SceneFloor,SceneHalos,SceneObjects,SceneOverlay,Plaque,Register}.svelte`; `memory.load` before the first layout, `memory.commit` from an effect (D-184); `data-scale`, `data-moving` (AC 2, 4-6)
- [x] `eslint.config.js` blocks and `lint.test.ts` (AC 7)
- [x] Light tokens in `app.css`; contrast and looks cases in `tokens.test.ts` (AC 3, 8)
- [x] `app/vitest.config.ts` `setupFiles`: jsdom stubs for `matchMedia` (reduced motion on by default), `ResizeObserver`, `Element.animate`
- [x] `.github/workflows/ci.yml`: the `workflow_dispatch` snapshot input and artifact upload (D-119)
- [ ] `app/e2e/fixtures.ts`: `openScene`, `SCREENSHOT_MAX_DIFF_RATIO`; `scene.spec.ts`, `modes.spec.ts`; the alert cases extended; baselines from CI after the owner's comparison, recorded (AC 1-10). Done but the baselines: they wait for the CI snapshot run and the owner's comparison (D-508)
- [x] Update `CHANGELOG.md` and the README (AC 11)

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

- Header (D-517, D-525). Scene mode keeps the Board header's top padding, order and alignment (brand, view switch, mode switch, gauges, then the buttons at the right end), so choosing a mode moves neither radio, nor the gauges' corner, nor a button; only its bottom padding shrinks, the gauges take two lines (label, track and value, then the explanation with the freshness) and the status chips sit before the buttons, in a slot whose own width counts for nothing (`contain: inline-size`), so a chip wraps inside it instead of pushing the buttons to another line. The one-line layout follows the stored mode, not the view shown, so switching views never relays the header. The mode switch keeps its room outside the Sessions view, hidden and inert. Header height at 1920 px: 108 px in Board mode (unchanged), 72.7 px in Scene mode; with every chip at once the slot wraps and the header grows (131 px), moving nothing beside it.
- Gauges (D-185): one width in both modes, `--gauge-width` (290 px) in `app.css`; a line longer than it ends in an ellipsis with its whole text in the tooltip. On one header line the freshness comes first while it is stale.
- The view id is `sessions` (`view-sessions`), so `relay-view.spec.ts`, `subjects.spec.ts`, `halo.spec.ts` and `alert.spec.ts` follow the new id; `board.spec.ts` is unchanged.
- Paint: the floor shapes, statics and rigs paint with `var(--token)` in SVG presentation attributes, which Chromium resolves (checked in the e2e build). Inline `style` attributes in parsed markup would break the window's CSP (`style-src 'self'`). `furniture.ts`'s `PAINTS` lists the tokens drawn with and `tokens.test.ts` holds each to its `app.css` declaration. WebKitGTK (v1, Linux) must be checked for `var()` in presentation attributes when it lands.
- Layer 1 is a Svelte template over typed shapes (`floorShapes`); layer 3's statics and figures are markup built from closed types and owned by `objects.ts` (AD-59, AD-171). The only text nodes in layer 3 are digits from integers (a ticket, a desk badge); the alert room's "!" and the question mark are drawn.
- The alert room is drawn as an ordinary room: its beacon and status board unlit, no hazard outline, no alert floor, no glow (story 4.9). Plaques show the state the layout gives, the alert state included, and a ticket in the alert room is on `--color-alert` (AD-171's marks); the halo is amber everywhere (its alert colour is story 4.9's).
- From the prototype's look, kept: the decision office's partitions and outline turn amber while someone waits there; free desks are faded, a working desk's screen is lit, a kept desk's chair shows its number. Not ported (D-463, D-476, AD-169, v0.5): per-tool flicker, blinking dots, the ticking clock (the wall clock shows 10:10, `WALL_CLOCK`), the orchestrator's screens, the bench, the rotating beam, the legend card, the panels, the event log, the taskbar mock.
- The register lists the employees of the decision office and the alert room in board order, `REGISTER.rows` rows, then "+N more". The decision office's plaque carries no "+n": its group's target is the register's last line (D-524); the other rooms carry theirs at the plaque's left end.
- Offices 5 to 8 are furnished as the base four, seeded by slot; an office slot the layout has not given yet is furnished in the neutral colour with every desk free; More offices is a room of shelves without desks. The back wall gains a window and a seeded frame per added column.
- Figures: the cap, collar and working chair take the card's project colour; a colour change (a floor reload) rebuilds that figure's node. A figure's depth is its spot's `gx + gy` plus 0.001, so it stands in front of a static at its very depth; ties by depth, then creation order.
- Scale: `fitScale` floors the fitted ratio to 1e-4 before clamping, so the scene never overflows its area by a rounding error; the scene's box is floored to whole pixels and centred with a whole-pixel margin, so snapped figures stay on device pixels. The sessions area keeps the scrollbar's room. `data-columns` on the scene element lets `openScene` compute the expected scale from `geometry.ts` and the area's client size.
- `memory.load` runs inside the derived layout (it reads storage once, then returns the committed memory), `memory.commit` in an effect: a commit never re-runs the layout (D-184).
- Halo memo: `BoardStore` also keeps the timings of sessions waiting on a relay approval alone (Working, zone decision or alert), whose scene halo would otherwise restart its breathing on each mode switch (AD-65).
- Vitest's setup file also points `localStorage` at jsdom's: Node's own global shadowed it, warned and kept nothing.
- `stable-size.spec.ts` counts radios as toggles, gains the mode switch case (choosing the other mode releases the chosen one) and a Scene mode scene, and runs D-517's chip case in both modes.
- The baseline case is `test.fixme` while any of its three files is missing and the run is not writing snapshots (D-508); the CI input runs Playwright once with `--update-snapshots=all`. A local snapshot run wrote, then compared, the three files three times over without a difference; they were moved out of the repository, not committed.
- Adversarial pass: stale and unknown employees (question mark, kept place and pose), a card without a scene state, a board with no live card (`empty`), more than eight projects (`scene-overflow`), the window at 1000 x 760 and 1400 x 600 (the area scrolls at `MIN_SCALE`, the window does not) and 1400 x 900 (fits), reduced motion, a storage that throws, markup in session and project names, the new tokens' contrast, `m` held, with a modifier, in a text field and outside Sessions. Misses found and fixed, each with a test: tall banners (a pending catalogue with a stopped relay) squeezed the sessions area to nothing, now at least half the window, the window scrolling (`modes.spec.ts > tall banners scroll the window and leave the scene half of it`); an approval-only wait's halo timing was dropped on each board (`board.test.ts > a session waiting on a relay approval alone keeps its halo timing`); the register's heading wrapped and the decision plaque's "+n" cut its name (seen on the renders). A first draft of the unknown case compared transforms across the stale banner's rescale; it now compares places at the new scale.
- Playwright: the whole suite with `--repeat-each=3` passed twice in a row (546 runs each). Before that, twice in five Playwright runs, a `page.goto` met `ERR_CONNECTION_REFUSED` from the local `vite preview` server, a machine shared with another checkout's suite (once on a scene case, once on `approvals.spec.ts`); the same cases passed on every other run. Not reproduced since; the cause is not established.

### Completion Notes

- AC 1: Playwright `modes.spec.ts > the header names the Sessions view`, `> the mode switch offers Board and Scene, Board by default`, `> m switches between Board and Scene and ignores a held key`, `> the mode survives a reload`, `> an unreadable stored view falls back to Board`, `> banners stay above the scene`; also `> the mode switch belongs to the Sessions view`, `> digits act on Needs you in both modes`, `> a stored view with an unknown mode falls back to Board`, `> a storage that throws keeps Board and still switches`, `> tall banners scroll the window and leave the scene half of it`; `stable-size.spec.ts > mode switch keeps its box and moves nothing after it`, `> header status chips move neither the gauges nor the buttons (board mode)` and `(scene mode)`, `> every toggle of the sceneMode scene has a case`; `scene.spec.ts > the quota gauges' tick never rescales the scene`. Vitest `view.test.ts > stored values are read field by field with their defaults`, `> a storage that throws keeps the defaults`, `> Board is the default mode and Sessions the view at start`, `> a mode change is stored and read back by the next window`, `> an unchanged mode writes nothing`; `components.test.ts > Gauge > on one header line it says the same, stale first, the whole line in its tooltip`.
- AC 2: Vitest `components/scene/scene.test.ts > layers 1 to 4 are hidden from assistive technology and ignore the pointer`, `> one halo per employee in the decision office or the alert room` (every fixture board), `> plaque and register names render as text`; Playwright `scene.spec.ts > the scene draws every live session of each fixture` (one case per board of the fixture index).
- AC 3: Vitest `rig.test.ts > a rig has three pose groups and two views`, `> a pose change toggles visibility and rebuilds nothing`, `> markup holds no text node for every look, pose and colour`, `> a look comes from the card key, among the allowed pairs only`, `> marks carry numbers only, as digits`; `objects.test.ts > a placed figure sits among statics by its depth` (every fixture board), `> a settled figure receives no write`, `> a new key's node appears at its place and a departed key's node is removed`, `> figures on one static keep their creation order`, `> positions are snapped to whole device pixels and follow a new scale`, `> a kept desk shows its session's number and a free desk is faded`; `furniture.test.ts > an office looks the same for the same slot and differs between slots`, `> the furniture builders use no Math.random`, `> statics are sorted by depth, and a floor's statics are the same on every build`, `> every office slot has a desk and a chair per desk spot, marked for objects.ts`; `tokens.test.ts > the allowed looks are the skin and hair pairs at 3:1 or more`, `> every paint the scene draws with is a token of app.css`.
- AC 4: Playwright `scene.spec.ts > a changed board places the figure at its new place at once`, `> no CSS animation runs inside a figure`; also `> missing data leaves every employee at its place with a question mark`, `> under reduced motion nothing in the scene animates`.
- AC 5: Playwright `scene.spec.ts > nothing in the scene changes while the payloads are unchanged`.
- AC 6: Playwright `scene.spec.ts > the scene's scale follows the sessions area`, `> below the minimum area the scene keeps the minimum scale and the area scrolls`, `> a fifth project widens the canvas and rescales once`, `> eight projects fit a maximised Full HD window`, `> eight projects fit a maximised Full HD window with the alert band`; Vitest `geometry.test.ts > scale > fits the canvas to the area, within MIN_SCALE and MAX_SCALE, never over the area`. No scene viewing scale: `MIN_SCALE` is the lower bound (D-520).
- AC 7: `pnpm lint`; Vitest `lint.test.ts > each restricted name is reported under both scene globs and not outside them` (a file named `src/lib/scene/mover.ts` included: no override yet).
- AC 8: Vitest `tokens.test.ts > every role pair meets its contrast target in the light theme` (the prototype's role pairs and the figure pairs added), `> every project colour meets 3:1 on the surface`. **Open**: Playwright `scene.spec.ts > the three densities match their light baselines` is `test.fixme` until the baselines are committed. To close it: run the CI workflow by hand with `update_snapshots` ticked, download the `playwright-snapshots` artifact, compare `scene-a-light`, `scene-b-light` and `scene-c-light` with the prototype's `scene-interactive.png` (the baselines show the scene settled, without the prototype's panels, legend, taskbar mock or event log), write the date and verdict here, then commit the files under `app/e2e/scene.spec.ts-snapshots/`.
- AC 9: Playwright `alert.spec.ts > the band shows in every view and mode` (Scene mode added, the band above the scene), `> an alert changes no theme token` (in Scene mode, over the `--color-*`, `--scene-*`, `--figure-*`, `--skin-*` and `--hair-*` tokens).
- AC 10: `board.spec.ts` unchanged, passing.
- AC 11: `CHANGELOG.md` `[Unreleased]` (Added: Scene mode and `m`; Changed: the view named Sessions, the gauges' fixed width); README "The scene (v0.4, in progress)" gains "Board and Scene" (the modes, `m`, Board as default, the zones), and the Relay and Subjects sections name the Sessions view.
- 1920 x 1080 (D-520, D-525), measured in the e2e build on `scene-eight` (six columns, the minimum sessions area 1471 x 869 CSS px): at 1920 x 1001 the Scene mode header is 72.7 px and the sessions area 1890 x 928, scale 0.9789, no scroll; with the alert band (41 px and its 12 px margin) 1890 x 875, scale 0.9229, no scroll, 6 px to spare. At 1920 x 961: without the band 1890 x 888, scale 0.9367, no scroll; with the band 1890 x 835, which scrolls 34 px at `MIN_SCALE`. `PAD.top` is not trimmed: the 1001 cases pass without it, and even `PAD.top` at 0 would give back 27.5 CSS px, still 6.5 short at 961 with the band, so that case cannot fit without smaller targets, which D-520 refuses.

## File List

- `.github/workflows/ci.yml` (modified)
- `CHANGELOG.md` (modified)
- `README.md` (modified)
- `app/e2e/alert.spec.ts` (modified)
- `app/e2e/fixtures.ts` (modified)
- `app/e2e/halo.spec.ts` (modified)
- `app/e2e/modes.spec.ts` (new)
- `app/e2e/relay-view.spec.ts` (modified)
- `app/e2e/scene.spec.ts` (new)
- `app/e2e/stable-size.spec.ts` (modified)
- `app/e2e/subjects.spec.ts` (modified)
- `app/eslint.config.js` (modified)
- `app/src/App.svelte` (modified)
- `app/src/app.css` (modified)
- `app/src/components/Gauge.svelte` (modified)
- `app/src/components/Header.svelte` (modified)
- `app/src/components/ModeSwitch.svelte` (new)
- `app/src/components/components.test.ts` (modified)
- `app/src/components/scene/Plaque.svelte` (new)
- `app/src/components/scene/Register.svelte` (new)
- `app/src/components/scene/Scene.svelte` (new)
- `app/src/components/scene/SceneFloor.svelte` (new)
- `app/src/components/scene/SceneHalos.svelte` (new)
- `app/src/components/scene/SceneObjects.svelte` (new)
- `app/src/components/scene/SceneOverlay.svelte` (new)
- `app/src/components/scene/scene.test.ts` (new)
- `app/src/lib/board.svelte.ts` (modified)
- `app/src/lib/board.test.ts` (modified)
- `app/src/lib/relay.svelte.ts` (modified)
- `app/src/lib/scene/furniture.ts` (new)
- `app/src/lib/scene/furniture.test.ts` (new)
- `app/src/lib/scene/geometry.ts` (modified)
- `app/src/lib/scene/geometry.test.ts` (modified)
- `app/src/lib/scene/lint.test.ts` (new)
- `app/src/lib/scene/objects.ts` (new)
- `app/src/lib/scene/objects.test.ts` (new)
- `app/src/lib/scene/rig.ts` (new)
- `app/src/lib/scene/rig.test.ts` (new)
- `app/src/lib/scene/text.ts` (modified)
- `app/src/lib/view.svelte.ts` (new)
- `app/src/lib/view.test.ts` (new)
- `app/src/test-setup.ts` (new)
- `app/src/tokens.test.ts` (modified)
- `app/vitest.config.ts` (modified)
- `docs/stories/story-4.7-scene-drawn-in-place-and-scene-mode.md` (modified)
- `docs/stories/sprint-status.yaml` (modified)

## Change Log

- 2026-10-28 created as story 4.4
- 2026-10-28 reworked after review (F3, F5 to F9, F20 to F22, F25, F27, F37, F38)
- 2026-10-28 renumbered 4.6 and rewritten for the scene v2 (D-460): five layers, the imperative object layer, persistent rigs, depth hysteresis and snapping, the mover with walking, glide and reduced motion, the stable-figures e2e; `seatMove` withdrawn (D-461)
- 2026-10-28 renumbered 4.7 and split after the plan challenge (D-497, D-506, D-508): this story draws the scene placed at once; walking, the mover, fades, the depth hysteresis, the walking toggle, the lint override and the stable-figures e2e move to story 4.10; the alert room's look to story 4.9; the baseline comparison is recorded
- 2026-10-28 implemented: the Sessions view with Board and Scene modes and `m`, the one-line header in Scene mode, the furnished floor, figures placed at once, halos, plaques and register, the scale, the scene's lint, the light tokens and the CI snapshot input; eight projects fit 1920 x 1001 with and without the alert band. Status In progress until the owner compares the CI baselines (D-508)
