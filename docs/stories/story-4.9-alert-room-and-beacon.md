# Story 4.9: The alert room and its beacon

**Status:** Review

## Story

As the author glancing at the scene on a second screen,
I want the alert room to light up while someone is in it, its beacon glowing for two minutes and then steady, and the offices of important projects marked,
so that an alert reads in the scene as clearly as in the band, without anything moving for as long as the alert lasts.

## Acceptance Criteria

1. The room's state (AD-175, D-474): `layoutScene` states the alert room as `alert` while it holds someone and `quiet` otherwise; while `alert`, the room's floor and hazard outline take `--scene-alert-floor` and `--color-alert`, the status board's mark turns `--color-alert`, and the plaque takes `--color-alert` and `--color-on-alert` with the room's count. Proof: Vitest `app/src/lib/scene/layout.test.ts > the alert room's state follows its occupants`; Playwright `app/e2e/alert.spec.ts > the alert room lights up while occupied and goes quiet when it empties`.
2. The beacon (AD-175, D-474): while the room is `alert`, the beacon is lit; its glow is an HTML element of layer 2 (`data-beacon`) that animates `opacity` only through story 4.4's `breathe`, keyed on the first alerted card's `(key, waiting_since_ms)` in board order (its `enteredAt` when the start is unknown): two minutes in the shared phase, then steady, and steady under reduced motion; the prototype's rotating beam is drawn static. Proof: Playwright `alert.spec.ts > the beacon glow animates opacity only and stops after two minutes` (clock and `last_event_at_ms` rewritten as D-101), `> under reduced motion the beacon is steady`; Vitest `app/src/components/scene/scene.test.ts > the beacon is keyed on the first alerted card`.
3. Halos in the alert room (AD-65 as amended): an employee standing at an alert room place has a halo whose gradient uses `--color-alert`; in the decision office it keeps the attention colour. Proof: Vitest `scene.test.ts > a halo in the alert room uses the alert colour`.
4. High priority (AD-175, FR-10): an office whose project's priority is high carries a "high priority" pill beside its plaque, in layer 4, as text. Proof: Vitest `layout.test.ts > a high-priority office carries its pill`; Playwright `alert.spec.ts > a high-priority office carries its pill`.
5. Time escalation in the scene (FR-10, AD-168, D-467): on `scene-alert`, two boards built by the real CLI at times either side of the threshold are pushed; with the second, the employee whose wait crossed the threshold is in the alert room and the room is lit, from the board alone, once `data-moving` is `0`; the approval-only wait of the scenario's normal project stays in the decision office's queue. Proof: Playwright `alert.spec.ts > a wait that reaches the threshold moves to the alert room`, `> an approval-only wait on a normal project stays in the queue`.
6. Pruning (AD-65, story 4.4 AC 6): `BoardStore` keeps the beacon's key in `haloMemo.retain` while its card is in alert, and drops it after. Proof: Vitest `app/src/lib/board.test.ts > the beacon's halo timing is kept while its card is in alert`.
7. Tokens (AD-67 as amended, D-117): the prototype v2's light `--scene-alert-floor` and the beacon, status board and hazard materials join `app.css`; the contrast pairs gain trousers and tunic on the alert floor (3). Proof: Vitest `app/src/tokens.test.ts > every role pair meets its contrast target in the light theme`.
8. Nothing moves while the alert lasts (FR-3, FR-9): on `scene-alert` settled, with the beacon's two minutes passed, the truthful-motion case records zero mutations and `document.getAnimations()` holds no running animation. Proof: Playwright `scene.spec.ts > nothing in the scene changes while the payloads are unchanged` run on `scene-alert` as well.
9. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: the alert room lights up and its beacon glows for two minutes; the high-priority pill); README "The scene": the alert room and the beacon. Proof: review.

## Tasks

- [x] Room state and the high-priority pill in `layout.ts`; their Vitest cases (AC 1, 4)
- [x] The alert room's drawing in `SceneFloor.svelte`, `objects.ts` and `furniture.ts` (floor, hazard outline, status board, static beam); plaque state in `Plaque.svelte` (AC 1, 4)
- [x] The beacon glow and the alert halo colour in `SceneHalos.svelte` (AC 2, 3)
- [x] `lib/board.svelte.ts`: the beacon's key in `retain` (AC 6)
- [x] Tokens and contrast pairs (AC 7)
- [x] `alert.spec.ts` and `scene.spec.ts` cases (AC 1, 2, 4, 5, 8)
- [x] Update `CHANGELOG.md` and the README (AC 9)

## Dev Notes

- Requirements: FR-3, FR-8, FR-9, FR-10. Decisions: AD-65, AD-67, AD-168, AD-175. Epic decisions: D-101, D-117, D-465, D-467, D-474, D-496, D-497.
- Dependencies: story 4.7 (the scene drawn in place), story 4.4 (`breathe`), story 4.5 (the alert tokens). It can run beside story 4.8. Split from the former alert story so that the band, the title and the taskbar did not wait for the drawing (D-497).
- Port from the prototype v2's `scene.js`: the alert room's block of the static drawing and its lit state. The beam's rotation and the status board's blinking are not ported (D-474): FR-9 forbids an animation that lasts as long as the alert.
- AC 5 holds whether walking exists or not: it waits for `data-moving="0"`, which story 4.7 writes as `0` and story 4.10's mover returns to `0` after arrival.
- An unknown employee keeping its alert (D-494) stays at its last place with its question mark, so it may stand in the alert room with the room lit; the room's state counts it.

## Testing

- Vitest: `app/src/lib/scene/layout.test.ts`, `app/src/components/scene/scene.test.ts`, `app/src/lib/board.test.ts`, `app/src/tokens.test.ts`.
- Playwright: `app/e2e/{alert,scene}.spec.ts`.

## Dev Agent Record

### Debug Log

- The room's state (AC 1) is the alert room's plaque state, which story 4.6 already gave as `alert` or `quiet`; `layout.ts` adds `alertLit(layout)`, read by layers 1 to 3, rather than a second field. What changed is who counts: the alert room counts whoever stands in it, its spots or its group, so an unknown employee kept there (D-494, Dev Notes) keeps the room lit and counts. The other shared rooms still count by zone: no decision says an unknown keeps its waiting amber.
- Departure, flagged for the owner (D-474 read literally). Keyed on `waiting_since_ms`, the beacon of a wait escalated by time never breathes: by the time it alerts, the wait is at least `[alert] after_minutes` old (1 minute at the least, 10 by default), past the two minutes. It breathes for a high-priority project's fresh wait, and for a threshold under two minutes for what is left of them. Implemented as written; keying the beacon on the alert's own start (the wait's start plus the floor's threshold) would need the threshold on the board.
- The beacon's key is the card's own key, not a namespaced one: its pair `(key, waiting_since_ms)` shares the memo entry of that card's halo when both start at the same moment, which keeps them in one phase. `beaconPair` lives in `lib/alert.ts` beside `alertedCards`, so the beacon's card is the band's first card; `haloEnteredAt` in `lib/halo.ts` is the halo's start, shared by the scene halo and the beacon's fallback.
- AC 6: `BoardStore` adds the beacon's key to the waiting keys it retains. The case that needs it is an alerted card whose zone went unknown and whose column is not Needs you: the waiting rule alone dropped its timing.
- Lit state ported from the prototype v2 beyond AC 1's list: the alert room's partition tops take `--color-alert` (`.alert-on .pt-alert`). The status board: its panel takes `--color-alert` and its "!" `--color-on-alert`, as the prototype does; AC 1's "mark" is read as the board's sign, since an "!" in `--color-alert` on an alert panel would vanish.
- The beam: drawn in layer 3 at the prototype's 0.22 opacity, shown while lit, never rotated (D-474). The glow: the prototype's ellipse (34 by 26 logical px on the lamp), a radial gradient of `--color-alert` at 90 % to transparent, resting at the prototype's lit opacity 0.55, set as the element's own `--halo-rest` so `halo-breathe` ends there and reduced motion holds it there. Its point is `geometry.ts`'s `BEACON`, read by the post in layer 3 and the glow in layer 2.
- AC 3: the halo's gradient reads `--halo-colour`, set per halo from `paint()`, so Vitest reads the colour from the element.
- AC 7: no material token is new. The prototype's beacon, status board and hazard paint with `--scene-screen-off`, `--scene-metal`, `--scene-wall-top`, `--color-alert` and `--color-on-alert`, all in `app.css`; `--scene-alert-floor` (light `#fbe6dd`) was already there from story 4.7 and joins `PAINTS`. The contrast pairs gain trousers on the alert floor; tunic on it was already held.
- AC 4: story 4.7 already drew the pill in layer 4 at `PRIORITY_PILL`, above the plaque (D-524 over the AC's "beside"); this story adds its proofs.
- AC 8 is a second case beside the `scene-c` one, sharing its body: the beacon's wait is dated to end its breathing a few seconds after first sight (D-101), the case sees it run, waits in real time for it to stop (the installed clock does not drive CSS animations), then watches. About 8 s.
- Adversarial pass: an unknown employee in the alert room kept an amber ticket (story 4.7 coloured the ticket by zone); `objects.ts` now colours it by the employee's alert, which D-494 keeps (`alert.spec.ts > missing data keeps the alert room lit`). The lit parts' markers are `data-beacon-lamp`, `data-beacon-beam`, `data-status-panel` and `data-status-mark`: a first `data-mark` collided with the figures' marks.

### Completion Notes

AC to test:

- AC 1: Vitest `app/src/lib/scene/layout.test.ts > the alert room's state follows its occupants` (occupied, an unknown kept there, emptied, an unknown elsewhere, the group); Playwright `app/e2e/alert.spec.ts > the alert room lights up while occupied and goes quiet when it empties`, and `> missing data keeps the alert room lit`.
- AC 2: Playwright `alert.spec.ts > the beacon glow animates opacity only and stops after two minutes`, `> under reduced motion the beacon is steady`; Vitest `app/src/components/scene/scene.test.ts > the beacon is keyed on the first alerted card`.
- AC 3: Vitest `scene.test.ts > a halo in the alert room uses the alert colour`; also asserted on the real boards in `alert.spec.ts`'s threshold and approval-only cases.
- AC 4: Vitest `layout.test.ts > a high-priority office carries its pill` (offices, shared rooms, More offices); Playwright `alert.spec.ts > a high-priority office carries its pill`.
- AC 5: Playwright `alert.spec.ts > a wait that reaches the threshold moves to the alert room`, `> an approval-only wait on a normal project stays in the queue`.
- AC 6: Vitest `app/src/lib/board.test.ts > the beacon's halo timing is kept while its card is in alert` (fails with the retain change removed).
- AC 7: Vitest `app/src/tokens.test.ts > every role pair meets its contrast target in the light theme`.
- AC 8: Playwright `app/e2e/scene.spec.ts > nothing in the scene changes while the payloads are unchanged, the alert room lit`.
- AC 9: `CHANGELOG.md` `[Unreleased]` Added, README "The scene (v0.4, in progress)": the alert room and high priority.

Gates run: `pnpm check`, `pnpm lint`, `pnpm test` (194), `pnpm build`, `cargo test -p officina-app`, the whole Playwright suite with `--repeat-each=3` (570 passed, 3 skipped: the baseline case of story 4.7, D-508).

Open: the beacon of a time-escalated wait is steady from the start (Debug Log); the owner decides whether D-474 keys on the wait or on the alert. The dark values of the new parts are story 4.13's.

## File List

- `CHANGELOG.md` (modified)
- `README.md` (modified)
- `app/e2e/alert.spec.ts` (modified)
- `app/e2e/scene.spec.ts` (modified)
- `app/src/components/scene/Scene.svelte` (modified)
- `app/src/components/scene/SceneFloor.svelte` (modified)
- `app/src/components/scene/SceneHalos.svelte` (modified)
- `app/src/components/scene/scene.test.ts` (modified)
- `app/src/lib/alert.ts` (modified)
- `app/src/lib/board.svelte.ts` (modified)
- `app/src/lib/board.test.ts` (modified)
- `app/src/lib/halo.ts` (modified)
- `app/src/lib/scene/furniture.ts` (modified)
- `app/src/lib/scene/geometry.ts` (modified)
- `app/src/lib/scene/layout.test.ts` (modified)
- `app/src/lib/scene/layout.ts` (modified)
- `app/src/lib/scene/objects.ts` (modified)
- `app/src/tokens.test.ts` (modified)
- `docs/stories/sprint-status.yaml` (modified)
- `docs/stories/story-4.9-alert-room-and-beacon.md` (modified)

## Change Log

- 2026-10-28 created after the plan challenge (D-497), from the alert room and beacon parts of the former story 4.8
- 2026-10-28 implemented: the alert room lit while occupied, the beacon's glow, the alert halo, the pill's proofs; moved to Review
- 2026-10-28 the beacon is keyed on when the alert began, not the wait (D-527), which answers the owner question the implementation raised
