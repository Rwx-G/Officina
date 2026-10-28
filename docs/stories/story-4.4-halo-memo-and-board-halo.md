# Story 4.4: The halo memo, and the board halo on opacity

**Status:** Review

## Story

As the author who keeps the board open all day,
I want the attention halo of a newly waiting session to breathe for two minutes, in one phase shared by every halo, animating opacity only, then stay steady,
so that a wait catches my eye once, without nagging me for hours and without costing CPU.

## Acceptance Criteria

1. Timing (AD-65, D-108): `app/src/lib/halo.ts` exports the cycle constants (`HALO_HALF_MS`, the cycle being two halves) and `HALO_BREATHING_MS`, a pure `haloTiming(enteredAtMs, firstSeenMs)` returning `{ startMs, iterations }`, a `HaloMemo` class with an injected clock (`() => number`, default `Date.now`) backed by a plain `Map` (not reactive state) keyed by `(key, enteredAt)`, which reads its clock once per new pair and keeps the timing while the key waits, and `export const haloMemo = new HaloMemo()`, the one instance the action, the scene and `BoardStore` use; `enteredAt` is `last_event_at_ms`, else `started_at_ms`, else the first-sight time; `retain(keys)` drops every other key. Proof: Vitest `app/src/lib/halo.test.ts > start is a multiple of the cycle, so every halo shares the wall-clock phase`, `> iterations are odd and cover the breathing time`, `> a wait older than the breathing time at first sight gets no iteration`, `> a second request returns the same timing without reading the clock`, `> a new enteredAt for the same key starts a new timing`, `> a card without times breathes from first sight`, `> retain drops keys that no longer wait`; every case builds its own `HaloMemo` with explicit times and uses the module's constants.
2. The action (D-108): `breathe(node, { key, enteredAt })` reads `mountMs` from the memo's clock and sets `--halo-delay` (`startMs - mountMs`), `--halo-count` and `--halo-half` when it mounts, ignores an update carrying the same pair and re-applies on a new pair; a remount later keeps the phase and the end. Any element may use it (board ring, scene halo of story 4.7, the alert room's beacon glow of story 4.9). Proof: `halo.test.ts > the action writes the style once and ignores an update with the same pair`, `> the action re-applies on a new pair`, `> a remount later keeps the phase and the end of the breathing`.
3. The board ring (AD-65 board fix, FR-9, D-180): `SessionCard.svelte` drops `animation: halo-breath` and uses `breathe` on Needs you cards; `app.css` drops `@keyframes halo-breath`, gains `--halo-rest` (its only copy) and `@keyframes halo-breathe { from { opacity: 0.35 } to { opacity: var(--halo-rest) } }`, and its reduced-motion rule selects `*, *::before, *::after`; `.card.needs_you::after` carries the ring (`box-shadow: 0 0 0 3px var(--color-attention), 0 0 0 7px color-mix(in srgb, var(--color-attention) 35%, transparent)`), `opacity: var(--halo-rest)`, `will-change: opacity`, `pointer-events: none`, `animation: halo-breathe var(--halo-half) ease-in-out var(--halo-delay) var(--halo-count) alternate`. Proof: Vitest `app/src/components/components.test.ts > a Needs you card gets its halo timing once`; Playwright `app/e2e/halo.spec.ts > the board halo animates opacity only`, on `scene-c` with D-101's rewrite of `last_event_at_ms`: the running `halo-breathe` animations on `::after` number exactly the Needs you cards, and each one's keyframes hold `opacity` alone.
4. Compositing guard on the board (AD-70): with five Needs you cards breathing (`scene-c`, rewritten as in AC 3), once `document.getAnimations()` holds one running `halo-breathe` per waiting card and 100 ms of real time have passed, `RecalcStyleCount` from CDP `Performance.getMetrics` does not grow over 3 s. Proof: Playwright `halo.spec.ts > board halos breathe without style recalculation` (Chromium only).
5. Steady when it must be (FR-3, D-180): under `emulateMedia({ reducedMotion: "reduce" })`, on `scene-c` rewritten as in AC 3, no animation runs and each Needs you card's `::after` opacity equals the resolved `--halo-rest`; a wait already older than the breathing time is steady. Proof: `halo.spec.ts > under reduced motion the board ring is steady`, `> a wait older than two minutes at first sight is steady` (the `busy` fixture as generated).
6. Pruning: `BoardStore` calls `haloMemo.retain` with the keys still waiting in each board it receives (Needs you card keys, and the beacon's key of story 4.9 once it exists). Proof: Vitest `app/src/lib/board.test.ts > a board drops the halo timings of sessions no longer waiting`.
7. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` under Changed: the Needs you halo breathes two minutes in a shared phase, on opacity only, then stays steady, and stops under reduced motion. Proof: review.

## Tasks

- [x] `app/src/lib/halo.ts`: constants, `haloTiming`, `HaloMemo`, `haloMemo`, `breathe` (AC 1, 2)
- [x] `SessionCard.svelte` and `app.css`: the `::after` ring, `--halo-rest`, `halo-breathe`, removal of `halo-breath`, the reduced-motion selector (AC 3, 5)
- [x] `lib/board.svelte.ts`: `haloMemo.retain` on each board (AC 6)
- [x] `app/e2e/halo.spec.ts` with a CDP session helper and the settle point; Vitest cases (AC 1-6)
- [x] Update `CHANGELOG.md` (AC 7)

## Dev Notes

- Requirements: FR-3 (two minutes, shared phase, steady under reduced motion), FR-9 (the board halo follows the same rules). Decisions: AD-65 (as amended), AD-70. Epic decisions: D-101, D-108, D-121, D-180, D-474.
- Unchanged by the scene v2 rework except the action's users: the scene halo (story 4.7) and the beacon glow (story 4.9) reuse the memo and the action, keyed on a card key and its `enteredAt` or `waiting_since_ms`.
- Measured (architecture section 1): the `box-shadow` halo costs a style recalculation per frame, 60 to 160 per second; the same ring on `::after` animating `opacity` costs none.
- An odd iteration count ends on `to`, the resting `--halo-rest`: the end is not a jump and no `animation-fill-mode` is needed.
- `lib/halo.ts` is outside the scene paths the lint covers (D-470): it may read the clock for the halo.
- Playwright's installed clock moves `Date.now`, not CSS animations: a breathing case sets the cards' `last_event_at_ms` to the installed time (D-101).
- v0.2's approval cards keep their amber border and get no halo.
- Dependencies: story 4.2's fixtures only; can run beside 4.3.

## Testing

- Vitest: `app/src/lib/{halo,board}.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/halo.spec.ts` (Chromium only for the CDP case).

## Dev Agent Record

### Debug Log

- Values the story leaves to AD-65 (authoring copy): `HALO_HALF_MS` 2 s, the cycle two halves, `HALO_BREATHING_MS` 2 minutes, `--halo-rest` 0.8. `HALO_ANIMATION` names the keyframes the action anchors; the e2e cases find the animations by it, so a renamed keyframe turns them red.
- A CSS `animation-delay` counts from the moment the browser starts the animation, not from when the action wrote it, and changing it does not restart a running animation. Three cases broke D-108's "written once at mount", each measured in Chromium before the fix (about 1.5 s off phase): a keyed move in Needs you (Chromium restarts the CSS animations of a moved node), reduced motion turned off while the window runs (the animation is created then), and a new pair on a mounted card (the old start stays, so a wait that comes back after its first breathing ended would not breathe at all). The action therefore also rewrites the delay on its own element's `animationstart` for `halo-breathe`, and on a new pair seeks the halo animation to 0 after writing the new values. The mount and same-pair rules of AC 2 are unchanged; the rewrite happens when an animation starts, before the settle point AC 4 measures from.
- The memo keeps every `(key, enteredAt)` pair of a key until `retain` drops the key, rather than replacing the older pair: the board ring and story 4.9's beacon may ask with one key and two starts, and replacing would restart each on the other's request.
- A start later than first sight (a clock ahead of the window's) counts from first sight, so no halo breathes longer than `HALO_BREATHING_MS` after it is first seen. A wait exactly `HALO_BREATHING_MS` old at first sight is steady.
- `breathe` takes `null` for an element without a halo and removes its properties, so `SessionCard` uses it on every card and passes a pair for Needs you only. The ring's box is the card's border box through two local properties (`--edge`, `--stripe`) that the card's borders use too.
- `BoardStore` retains on the first board from `get_board` as well as on each pushed board.
- Light theme contrast, computed from `app.css`: the Needs you border (`--color-attention-border`) is 3.73:1 on the card surface and 3.46:1 on the background; against the ring at rest it is 2.33:1, against v0.1's full ring 2.11:1, so the boundary reads no worse than before. The ring is decor (D-117) and never the only cue (FR-3): the column, the border and the number carry the wait.
- Not measured: a window hidden when a halo mounts. Playwright cannot hide the page the way a minimised WebView2 does; the `animationstart` rewrite should cover it, since the browser starts the animation only once it renders again, but that is unverified.
- README: the Needs you row says how the ring breathes and that reduced motion keeps it steady. `docs/assets/board.png` still shows v0.1's ring; it is regenerated with the README assets, not here.

### Completion Notes

- AC 1: `app/src/lib/halo.test.ts > haloTiming > start is a multiple of the cycle, so every halo shares the wall-clock phase`, `> iterations are odd and cover the breathing time`, `> a wait older than the breathing time at first sight gets no iteration`; `> HaloMemo > a second request returns the same timing without reading the clock`, `> a new enteredAt for the same key starts a new timing`, `> a card without times breathes from first sight`, `> retain drops keys that no longer wait`. Added: `> haloTiming > a start later than first sight breathes no longer than from first sight`.
- AC 2: `halo.test.ts > breathe > the action writes the style once and ignores an update with the same pair`, `> the action re-applies on a new pair`, `> a remount later keeps the phase and the end of the breathing`. Added: `> an animation the browser starts anew counts its delay from that moment`, `> a card that does not wait gets no halo, and loses it when it stops waiting`.
- AC 3: `app/src/components/components.test.ts > SessionCard > halo > a Needs you card gets its halo timing once`; `app/e2e/halo.spec.ts > board halo > the board halo animates opacity only` (scene-c rewritten as D-101; also asserts the approval card gets none). Added: `components.test.ts > SessionCard > halo > cards outside Needs you get no halo timing`, `> a waiting card without times breathes from first sight`; `halo.spec.ts > every halo breathes in the wall-clock phase for two minutes from its wait`.
- AC 4: `halo.spec.ts > board halos breathe without style recalculation` (Chromium only).
- AC 5: `halo.spec.ts > under reduced motion the board ring is steady`, `> a wait older than two minutes at first sight is steady` (busy as generated). Added: `> reduced motion turned off later resumes the shared phase, not a fresh breathing`; `components.test.ts > SessionCard > halo > a waiting card older than the breathing time is steady`.
- AC 6: `app/src/lib/board.test.ts > BoardStore halo timings > a board drops the halo timings of sessions no longer waiting`. Added: `> the first board keeps the timings of what it shows waiting only`.
- AC 7: `CHANGELOG.md` `[Unreleased]` Changed; README's Needs you row.
- Adversarial pass, added as e2e cases: `halo.spec.ts > a session that waits again breathes again, in the shared phase`, `> a session that leaves Needs you and comes back breathes again`, `> cards that change places keep their phase and their end`, `> a remount keeps the phase and the end of the breathing`. With the `animationstart` rewrite removed, the reorder and reduced-motion cases fail; with the seek removed, the waits-again case fails (both checked).
- Gates: `pnpm check`, `pnpm lint`, `pnpm test` green; `pnpm e2e --repeat-each=3` 396 passed.

## File List

- `app/src/lib/halo.ts`, `app/src/lib/halo.test.ts` (new)
- `app/e2e/halo.spec.ts` (new)
- `app/src/components/SessionCard.svelte`, `app/src/components/components.test.ts`
- `app/src/lib/board.svelte.ts`, `app/src/lib/board.test.ts`
- `app/src/app.css`
- `CHANGELOG.md`, `README.md`

## Change Log

- 2026-10-28 created as story 4.2
- 2026-10-28 reworked after review (F1, F18, F19, F23)
- 2026-10-28 renumbered 4.4 for the scene v2 (D-460); the action's users widened to the scene halo and the beacon
- 2026-10-28 story numbers per D-498 (plan challenge)
- 2026-10-28 implemented: the halo memo and the `breathe` action, the board ring on `::after` animating opacity, `--halo-rest` and `halo-breathe`, the reduced-motion rule on pseudo-elements, pruning in `BoardStore`; adversarial pass anchored the delay on the animation's actual start (keyed moves, reduced motion turned off, a new pair). Status review
