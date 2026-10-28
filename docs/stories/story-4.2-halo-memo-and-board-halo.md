# Story 4.2: The halo memo, and the board halo on opacity

**Status:** Approved

## Story

As the author who keeps the board open all day,
I want the attention halo of a newly waiting session to breathe for two minutes, in one phase shared by every halo, animating opacity only, then stay steady,
so that a wait catches my eye once, without nagging me for hours and without costing CPU.

## Acceptance Criteria

1. Timing (AD-65, D-108): `app/src/lib/halo.ts` exports the cycle constants (`HALO_HALF_MS`, the cycle being two halves) and `HALO_BREATHING_MS`, a pure `haloTiming(enteredAtMs, firstSeenMs)` returning `{ startMs, iterations }`, a `HaloMemo` class with an injected clock (`() => number`, default `Date.now`) backed by a plain `Map` (not reactive state) keyed by `(card key, enteredAt)`, which reads its clock once per new pair and keeps the timing while the key waits, and `export const haloMemo = new HaloMemo()`, the one instance the action and `BoardStore` use (review F19); `enteredAt` is `last_event_at_ms`, else `started_at_ms`, else the first-sight time; `retain(keys)` drops every other key. Proof: Vitest `app/src/lib/halo.test.ts > start is a multiple of the cycle, so every halo shares the wall-clock phase`, `> iterations are odd and cover the breathing time`, `> a wait older than the breathing time at first sight gets no iteration`, `> a second request returns the same timing without reading the clock`, `> a new enteredAt for the same key starts a new timing`, `> a card without times breathes from first sight`, `> retain drops keys that no longer wait`; every case builds its own `HaloMemo` with explicit times and uses the module's constants, never retyped values.
2. The action (D-108): `breathe(node, { key, enteredAt })` reads `mountMs` from the memo's clock and sets the custom properties `--halo-delay` (`startMs - mountMs`), `--halo-count` and `--halo-half` (from `HALO_HALF_MS`) when it mounts, ignores an update carrying the same pair (Svelte calls `update` on every new parameter object) and re-applies on a new pair; a remount later keeps the phase and the end. Proof: `halo.test.ts > the action writes the style once and ignores an update with the same pair`, `> the action re-applies on a new pair`, `> a remount later keeps the phase and the end of the breathing`, each with a memo on an explicit clock.
3. The board ring (AD-65 board fix, FR-9 as amended, D-180): `SessionCard.svelte` drops `animation: halo-breath` and uses `breathe` on Needs you cards; `app.css` drops `@keyframes halo-breath`, gains `--halo-rest` (the resting opacity, its only copy) and `@keyframes halo-breathe { from { opacity: 0.35 } to { opacity: var(--halo-rest) } }`, and its reduced-motion rule selects `*, *::before, *::after`; `.card.needs_you::after` carries the ring (`box-shadow: 0 0 0 3px var(--color-attention), 0 0 0 7px color-mix(in srgb, var(--color-attention) 35%, transparent)`), `opacity: var(--halo-rest)`, `will-change: opacity`, `pointer-events: none`, `animation: halo-breathe var(--halo-half) ease-in-out var(--halo-delay) var(--halo-count) alternate`. Proof: Vitest `app/src/components/components.test.ts > a Needs you card gets its halo timing once` (re-rendered with the same card, the properties are unchanged); Playwright `app/e2e/halo.spec.ts > the board halo animates opacity only`, on `scene-c` with D-101's rewrite of `last_event_at_ms`: it first asserts that the running `halo-breathe` animations on `::after` number exactly the Needs you cards, then that each one's keyframes hold `opacity` alone (review F1).
4. Compositing guard on the board (AD-70): with five Needs you cards breathing (`scene-c`, rewritten as in AC 3), once `document.getAnimations()` holds one running `halo-breathe` per waiting card and 100 ms of real time (`page.waitForTimeout`) have passed, `RecalcStyleCount` from CDP `Performance.getMetrics` does not grow over 3 s. Proof: Playwright `halo.spec.ts > board halos breathe without style recalculation` (Chromium only) (review F18).
5. Steady when it must be (FR-3 as amended, D-180): under `emulateMedia({ reducedMotion: "reduce" })`, on `scene-c` rewritten as in AC 3, no animation runs and each Needs you card's `getComputedStyle(card, "::after").opacity` equals the resolved `--halo-rest`; a wait already older than the breathing time is steady. Proof: `halo.spec.ts > under reduced motion the board ring is steady` (review F1), `> a wait older than two minutes at first sight is steady` (the `busy` fixture as generated).
6. Pruning: `BoardStore` calls `haloMemo.retain` with the Needs you keys of each board it receives. Proof: Vitest `app/src/lib/board.test.ts > a board drops the halo timings of sessions no longer waiting`.
7. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` under Changed: the Needs you halo breathes two minutes in a shared phase, on opacity only, then stays steady, and stops under reduced motion; the README's board description says the same if it describes the halo. Proof: review.

## Tasks

- [ ] `app/src/lib/halo.ts`: constants, `haloTiming`, `HaloMemo` with an injected clock and a plain `Map`, `haloMemo`, `breathe` action (AC 1, 2)
- [ ] `SessionCard.svelte` and `app.css`: the `::after` ring, `--halo-rest`, `halo-breathe`, removal of `halo-breath`, the reduced-motion selector (AC 3, 5)
- [ ] `lib/board.svelte.ts`: `haloMemo.retain` on each board (AC 6)
- [ ] `app/e2e/halo.spec.ts` with a CDP session helper and the settle point (AC 3-5); Vitest cases (AC 1-3, 6)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README if needed (AC 7)

## Dev Notes

- Requirements: FR-3 as amended (two minutes, shared phase, steady under reduced motion), FR-9 as amended (the board halo follows the same rules). Decisions: AD-65, AD-70 (halo memo, compositing guard). Epic decisions: D-101, D-108, D-121, D-180.
- Measured (addendum section 1): the `box-shadow` halo costs a style recalculation per frame, 60 to 160 per second; the same ring on `::after` animating `opacity` costs none. This story is that fix; the scene's halos reuse the memo and the action in story 4.4.
- D-108 refines AD-65's delay: AD-65 fixes `-(firstSeenMs mod 4000)` at first sight, which is right only for the first mount. With `startMs` stored and the delay taken at mount, a card that remounts (a mode change in 4.4 and 4.6) is still in phase and still ends at the same time. At any instant the animation's phase is the wall clock modulo the cycle, so every halo shares it.
- An odd iteration count ends on `to`, the resting `--halo-rest` of the base style: the end of breathing is not a jump, and no `animation-fill-mode` is needed. With 0 iterations the element simply rests.
- `lib/halo.ts` is outside the scene paths AD-64's lint covers (story 4.4): it is the one place allowed to read the clock for the halo.
- The universal selector matches no pseudo-element (D-180): today's `* { animation: none !important; ... }` would leave the `::after` ring breathing under reduced motion. The widened selector also covers the scene's HTML halos of story 4.4.
- `color-mix()` keeps the outer ring derived from `--color-attention`, so the dark theme (4.8) needs no second value.
- Playwright's installed clock moves `Date.now`, not CSS animations, which run on real time (AD-70): a breathing case sets the cards' `last_event_at_ms` to the installed time (D-101). The `busy` fixture's waits are minutes old at its `generated_at_ms`, and the scene fixtures' at least 3 minutes, hence steady as generated: a case on them without the rewrite would pass with no animation at all.
- Seams: none beyond the board. v0.2's approval cards (2.8) keep their amber border and get no halo: AD-65 concerns `.card.needs_you`.

## Testing

- Vitest: `app/src/lib/{halo,board}.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/halo.spec.ts` (Chromium only for the CDP case).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: reduced-motion rule widened to pseudo-elements and the ring cases run on breathing halos with a count check (F1, D-180); compositing guard settles first (F18); one `haloMemo` on a plain `Map`, the action reading the memo's clock (F19); `--halo-rest` as the one copy of the resting opacity (F23)
