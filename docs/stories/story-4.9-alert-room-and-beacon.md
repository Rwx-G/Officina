# Story 4.9: The alert room and its beacon

**Status:** Approved

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

- [ ] Room state and the high-priority pill in `layout.ts`; their Vitest cases (AC 1, 4)
- [ ] The alert room's drawing in `SceneFloor.svelte`, `objects.ts` and `furniture.ts` (floor, hazard outline, status board, static beam); plaque state in `Plaque.svelte` (AC 1, 4)
- [ ] The beacon glow and the alert halo colour in `SceneHalos.svelte` (AC 2, 3)
- [ ] `lib/board.svelte.ts`: the beacon's key in `retain` (AC 6)
- [ ] Tokens and contrast pairs (AC 7)
- [ ] `alert.spec.ts` and `scene.spec.ts` cases (AC 1, 2, 4, 5, 8)
- [ ] Update `CHANGELOG.md` and the README (AC 9)

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

### Completion Notes

## File List

## Change Log

- 2026-10-28 created after the plan challenge (D-497), from the alert room and beacon parts of the former story 4.8
