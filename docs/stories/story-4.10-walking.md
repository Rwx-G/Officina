# Story 4.10: Walking

**Status:** Approved

## Story

As the author who approved a company whose employees walk,
I want an employee whose state changes to walk along the corridors to its new place with a natural walk cycle, to arrive and leave through the entrance, and then to stay still, with walking off and reduced motion as quieter alternatives,
so that I enjoy the movement and follow who went where, while nothing moves once everyone has arrived and walking never costs more than FR-9 allows.

## Acceptance Criteria

1. Walking setting (AD-66 as amended, D-113, D-469, D-500): `ViewState` gains `walking`, read field by field with default `DEFAULT_WALKING` (`true`); in Scene mode the header shows `WalkingSwitch.svelte`, a toggle button (`aria-pressed`) labelled "Walking", hidden in Board mode; the setting survives a reload. Proof: Playwright `app/e2e/modes.spec.ts > walking is on by default`, `> the walking toggle shows in Scene mode only`, `> the walking setting survives a reload`; Vitest `app/src/lib/view.test.ts > a stored view without walking reads as walking on`.
2. The navigation graph (AD-170 as amended, D-503): `places.ts` gains, from `geometry.ts`, the nodes and links joining story 4.6's spots: reception's exit node outside the door, entry, hub and door to the corridor; each room's door on the corridor side (front side for the back row, back side for the front row) and each office's aisle; one corridor node per door on the centre line, chained in `gx` order; every spot linked to its room's door. Proof: Vitest `app/src/lib/scene/places.test.ts > every spot is reachable from the exit` (for every column count `geometry.ts` defines), `> each door links its room to the corridor`, `> the corridor nodes are chained in gx order`.
3. Paths (AD-170, D-512): `paths.ts` exports `route(graph, from, toSpot)` (Dijkstra on Euclidean lengths, from the nearer of a walker's previous and next node when mid-walk) and `smooth(points)` (quadratic corners of radius 0.45 tile, at most half of each segment), deterministic. Proof: Vitest `app/src/lib/scene/paths.test.ts > a route follows graph edges`, `> a route from mid-walk starts at the nearer node`, `> smoothing keeps both ends and stays within the corner radius`, `> the same input gives the same route`.
4. The mover (AD-172 as amended, D-469, D-470, D-483, D-502): `mover.ts` sits between the layout and `objects.ts`; it walks figures along `paths.ts` routes with AD-172's walk constants (speed, acceleration, braking, swing, bob, facing hysteresis, the seat pause and slide), glides in `GLIDE_MS` when walking is off, places at once under reduced motion (read at each move), fades a new key in on the exit node and walks it in, walks a departed key to the exit and fades it out, places everyone at once on the first layout of a run, re-routes a walker from where it is when a new layout arrives, places every walker at once and ends every fade on `visibilitychange` to hidden or after a frame gap above 0.25 s, runs one `requestAnimationFrame` loop only while something moves, and writes `data-moving` on the scene element. Proof: Vitest `app/src/lib/scene/mover.test.ts` (injected clock, frame source and visibility) `> a walk follows the route with continuous positions and stops on the spot`, `> speed never exceeds WALK.vmax`, `> facing changes only past the velocity thresholds`, `> a glide ends in GLIDE_MS`, `> reduced motion places at once and fades nothing`, `> the first layout places everyone at once`, `> a new key walks in from the exit and a departed key walks out`, `> a new layout mid-walk re-routes from the current position`, `> a hidden page places every walker at once`, `> a frame gap above 0.25 s places every walker at once`, `> the loop stops when nothing moves and data-moving follows`.
5. Depth with hysteresis (AD-171): a moving figure's index among statics changes only when its depth leaves the 0.05-tile window around its statics, and its quantised depth only past 0.06; ties by quantised depth, then creation order; only figure nodes move, with `insertBefore`, and only when their neighbour changed. Proof: Vitest `app/src/lib/scene/objects.test.ts > a moving figure's index among statics changes only past the hysteresis`, `> only figure nodes move, and only when their neighbour changed`.
6. Halos and walks (AD-65 as amended): a walking employee has no halo; it gets its halo when it arrives at a decision office or alert room place, keyed by `(key, enteredAt)` so the memo keeps its phase across a move between the two. Proof: Vitest `app/src/components/scene/scene.test.ts > a walking employee has no halo until it arrives`.
7. Stable figures (AD-70 as amended, the prototype's flicker check): on `scene-b`, a pushed board that moves one Working session to Needs you, stepped with `page.clock.runFor(16)` for 72 frames: the figure keeps one node from A to B, exactly one pose group and one view are visible in every frame, its opacity is 1, its largest step is under 3 logical px per frame, its facing never flips back within 8 frames, `document.getAnimations()` holds no animation whose target is inside a `[data-figure]`, and `data-moving` returns to `0` after arrival. Proof: Playwright `app/e2e/walk.spec.ts > a walking figure stays one stable node`, `> no CSS animation runs inside a walking figure`, `> the frame loop stops after arrival`.
8. Arrival and exit (D-483): a session new to the board appears at the entrance, fades in and walks to its place; a session that leaves the live set walks out and fades; a reload places everyone at once. Proof: Playwright `walk.spec.ts > a new session walks in from the entrance and an ended one walks out`, `> a reload places everyone at once`.
9. Walking off and reduced motion (FR-3, D-469): with walking off, a state change moves the figure in under 400 ms with no route; under `emulateMedia({ reducedMotion: "reduce" })` the figure is at its new place in the next frame and no fade runs; turning walking off mid-walk places walkers at once. Proof: Playwright `walk.spec.ts > walking off glides in under 400 ms`, `> reduced motion places at once`, `> turning walking off mid-walk places the walkers`.
10. Targets during a walk (D-471): when a state change makes a figure walk, its target is at the destination from the first frame and the walking figure ignores the pointer. Proof: Playwright `scene.spec.ts > a walking figure's target is at its destination from the first frame`.
11. Truthful motion with walking (PRD section 7): story 4.7's truthful-motion case passes unchanged with walking on, `openScene` waiting for `data-moving="0"`. Proof: Playwright `scene.spec.ts > nothing in the scene changes while the payloads are unchanged`.
12. The mover's clock (AD-64 as amended, D-470): `eslint.config.js` gains one override for `src/lib/scene/mover.ts` that allows `requestAnimationFrame`, `cancelAnimationFrame` and `performance.now` only. Proof: `pnpm lint`; Vitest `app/src/lib/scene/lint.test.ts > the mover alone may use the frame loop and performance.now`.
13. Default (D-480, D-500, D-501): `DEFAULT_WALKING` is `true`; story 4.14's AD-71 walking run, in both units, sets it to `false` in the same change if it fails. Proof: `modes.spec.ts > walking is on by default`, or its `off` twin after story 4.14's fallback.
14. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: walking and the walking toggle); README "The scene": walking, walking off, reduced motion, arrivals and departures through the entrance. Proof: review.

## Tasks

- [ ] `lib/view.svelte.ts` (`walking`), `WalkingSwitch.svelte`, `Header.svelte` (AC 1)
- [ ] The navigation graph in `places.ts`; `paths.ts`; their tests (AC 2, 3)
- [ ] `mover.ts` and its tests; `Scene.svelte` hosts it between the layout and `objects.ts` (AC 4, 9)
- [ ] `objects.ts`: hysteresis for moving figures; halos only on arrival in `SceneHalos.svelte` (AC 5, 6)
- [ ] `eslint.config.js` override and its `lint.test.ts` case (AC 12)
- [ ] `walk.spec.ts`; `scene.spec.ts` and `modes.spec.ts` cases (AC 1, 7-11, 13)
- [ ] Update `CHANGELOG.md` and the README (AC 14)

## Dev Notes

- Requirements: FR-3, FR-7, FR-9; PRD section 7 (truthful motion, stable figures). Decisions: AD-64 to AD-66, AD-70, AD-71, AD-170 to AD-172 as amended. Epic decisions: D-469, D-470, D-471, D-480, D-483, D-497, D-500 to D-503, D-512.
- Dependencies: story 4.7 (the scene drawn in place, `objects.ts`, `data-moving`) and story 4.6 (spots); stories 4.8 and 4.9 come first in the order (D-497) so the scene is usable and alerted before it walks, but this story needs neither.
- Port from the prototype v2's `scene.js`: its navigation graph (`addNode`, `link`, the rooms' door blocks, the corridor chaining), `shortestPath`, `smooth`, and "movement" (`setGoal`, `advance`, `stepGlide`, `arrive`) and "rendering" (`reorder`, the hysteresis window). Its v2 header comment lists the four rules that removed the flicker; AC 5 and AC 7 hold the port to the two that concern moving figures.
- Gate 3 is withdrawn (D-500): no measurement precedes this story. Walking ships on, and story 4.14's AD-71 walking run on the real window decides, against 1 % of the machine and 8 % of one core (D-501). A 30 frames per second cap was rejected (D-501): it would break AC 7's step bound.
- `page.clock` fakes `requestAnimationFrame`, so `runFor(16)` steps the mover deterministically; `openScene` waits with `expect.poll`, never with `requestAnimationFrame`.
- The first layout of a run places everyone at once (D-469): a restart of the window never makes the whole company walk in. The hidden-window rule (D-502) is the same idea for a window that was behind the editor: Chromium throttles `requestAnimationFrame` there, and replaying walks on return would show figures still on their way.
- v0.5's staff nodes are not built here (D-503).

## Testing

- Vitest: `app/src/lib/{view,scene/places,scene/paths,scene/mover,scene/objects,scene/lint}.test.ts`, `app/src/components/scene/scene.test.ts`.
- Playwright: `app/e2e/{walk,scene,modes}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created after the plan challenge (D-497), from the walking parts of the former stories 4.5 (graph and paths) and 4.6 (mover, walk cycle, hysteresis, fades, walking off, reduced motion, the stable-figures e2e, the lint override), with the hidden-window rule (D-502) and no prototype gate (D-500)
