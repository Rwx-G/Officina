# Story 4.17: Scene readability

**Status:** Review (code done; story 4.14 AC 9, the owner's two-second test on the screenshots this story rendered again, passed with a caveat: research notes E87, D-701)

## Story

As the author who reads the scene at a glance,
I want the Needs you count and the oldest waiting name readable in one place, every queue number visible, and no desk number taken for a queue number,
so that Scene mode answers "who needs me, and who first" in under two seconds, as the board does.

Preparing the owner's readability test (story 4.14 AC 9: "on five fixtures with someone waiting, the Needs you count and the oldest waiting name are read in under 2 s in Scene mode, as on the board"), the screenshots at 1920 x 1032 of `scene-subagents`, `scene-b`, `scene-c`, `scene-alert` and `scene-eight` showed four defects:

1. In `scene-c` and `scene-eight` the queue spills into the corridor, and waiting figure 4 stands behind the alert room's beacon, which covers its number: only an amber dot shows.
2. The register, "Needs you, board order", shows five rows then "+1 more" in `scene-c`; the sixth is an approval-only wait (`rotate-certs`) with no digit. The count of what needs you is in no one place.
3. At eight projects the register's text shrinks to about 11 px against about 15 px elsewhere, since the whole scene scales down.
4. Your turn sessions carry white-ringed desk numbers ("1", "2") in the lounge and over their chairs, which compete with the amber queue badge 1.

And two minor ones: in `scene-c` figure 3's head touches the bottom of the "Decisions: you 6" plaque; in `scene-alert` the "high priority" pill sits on the billing-api plaque.

## Acceptance Criteria

1. No queue badge is ever covered by scenery (D-554): a waiting figure's badge and a kept desk's board number are drawn in a layer above every object of the floor; a walk writes nothing in that layer. Proof: Vitest `objects.test.ts > queue badges are drawn in the badge layer, above every object, at their figure`, `> a kept desk shows its session's number and a free desk is faded`; Playwright `readability.spec.ts > no queue badge is covered: <fixture>` for every fixture board with someone waiting, at 1920 x 1032 and 3440 x 1392, which hit-tests points inside every badge's disc or diamond (it fails when the badge layer is put under the objects); `walk.spec.ts > a walk writes only the walker, in one depth band at a time`, unchanged.
2. The count of what needs you in one place (D-556): the register's heading reads "Needs you: N", N everyone waiting in the scene, numbered or not; it shows a row per board key (`MAX_NUMBER`), then "+n more"; an approval-only wait has an amber diamond, on its row and on its figure, and the word "approval" on its row. Proof: Vitest `scene.test.ts > the register's heading counts everyone waiting; an approval-only wait has a diamond and the word`, `> the register has a row per board key`, `rig.test.ts > a digit on a disc is a board number, a desk a letter on a square, an approval a diamond`; Playwright `readability.spec.ts > the register counts everyone waiting at a fixed size and covers nothing: <fixture>` (rows, count, marks, from `layoutScene`).
3. The register is overlay UI at a fixed size (D-555): in CSS px, never scaled with the floor, over the scene's top-right corner, where nothing of the floor is drawn; it covers no target, plaque or drawn object at 1920 x 1032, at the minimum window (1140 x 820) and at 3440 x 1392; at eight projects on the owner's screen its text is at least 14 px and the same size as with four projects. Proof: Playwright `readability.spec.ts > the register counts everyone waiting at a fixed size and covers nothing: <fixture>` (width, height bound, font sizes, no target meets it, nothing drawn under it), `> at eight projects on the owner's screen the register reads as large as with four`; Vitest `scene.test.ts > layers 1 to 4, the badges and the register are hidden from assistive technology and ignore the pointer`.
4. The decision office's group (D-557): the waiting beyond its queue's places are a "+n" at the right end of its plaque, as every other room's group sits on its plaque, since the register no longer has a place on the floor. Proof: Vitest `geometry.test.ts` (no two targets intersect, no target meets a plaque it does not own, at full occupancy for every column count), `layout.test.ts > arrows follow the vectors of a full office and a full queue`; Playwright `scene.spec.ts > the decision office's group lists the waiting beyond the queue, each opening its file`, `> Tab walks reception, the queue and its group, ...` (the group's target, unchanged in tab order), both renamed from "the register".
5. Desk marks are not queue badges (D-558): a desk is named by its letter, A for an office's first, on a square tag edged in its project's colour, on a lounge figure's chest and over its kept chair; a digit on a disc is always a board number, the key 1 to 9 that opens a Needs you session. Proof: Vitest `rig.test.ts > marks carry numbers and desk letters only`, `> a digit on a disc is a board number, a desk a letter on a square, an approval a diamond`, `objects.test.ts > a kept desk whose session rests in the lounge shows the desk's letter on its chair`, `scene.test.ts > plaque and register names render as text` (layer 3 and the badges hold board numbers and desk letters only).
6. The minor defects (D-559): the decision office's plaque hangs at the office's back, clear of the queue's heads and of reception's figure by the counter's end; the "high priority" pill sits 9 px above its office's plaque. Proof: Playwright `readability.spec.ts > no plaque covers a waiting figure's head: <fixture>` (fails with the plaque where it was); Vitest `geometry.test.ts` (the plaque and the pill against every target).
7. Nothing else moves: D-516 and D-517 (the header's boxes), story 4.15's radius and transition rules (D-539, D-545), the dark theme (no new colour: the register and the marks use existing tokens), FR-9 (nothing loops; the badge layer is written only when a badge changes), and eight projects still fit a maximised Full HD window. Proof: the gate's Vitest (`tokens.test.ts`) and Playwright `--repeat-each=3` runs (`stable-size.spec.ts`, `header.spec.ts`, `motion.spec.ts`, `themes.spec.ts`, `scene.spec.ts > eight projects fit a maximised Full HD window`, `halo.spec.ts`).
8. Documentation: this story, D-554 to D-559, `CHANGELOG.md` `[Unreleased]`, the README's scene section and its screenshots, `sprint-status.yaml`, the Epic 4 report; the readability screenshots rendered again from the pushed code for story 4.14 AC 9. Proof: review.

## Tasks

- [x] The badge layer in `objects.ts` and `SceneObjects.svelte`; the hit-test case (AC 1)
- [x] The register as overlay UI in `Scene.svelte` and `Register.svelte`; its heading count, rows and approval mark (AC 2, 3)
- [x] The decision office's group on its plaque (AC 4)
- [x] Desk letters on square tags (AC 5)
- [x] The decision plaque and the priority pill (AC 6)
- [x] The gate (AC 7)
- [x] Documentation and the readability screenshots (AC 8)

## Dev Notes

- Requirements: FR-1, FR-2, FR-6, FR-9; story 4.14 AC 9. Decisions: D-103, D-104, D-107, D-464, D-484, D-516, D-517, D-520, D-524, D-531, D-539, D-543, D-545, D-546, D-550; D-554 to D-559.
- The hit tests read what is painted on top with `elementFromPoint`: the page's CSP refuses an injected style sheet, so the test turns pointer events on through the CSSOM for every SVG shape and every HTML box that paints a background, never a layer's own box, and hides the targets.
- The register's place: its top-left corner, where it was, holds at most seven rows at the minimum scale before it meets the lounge's plaque and its back sofa's figure; the top-right corner lies above the back wall's top, where the floor draws nothing (D-555).

## Testing

- Vitest: `objects.test.ts`, `rig.test.ts`, `layout.test.ts`, `geometry.test.ts`, `scene.test.ts`.
- Playwright: `readability.spec.ts` (new), `alert.spec.ts` (the badge's place), every scene spec unchanged.

## Dev Agent Record

### Debug Log

- Figure 4's spot is the queue's first corridor place, room-local (-0.4, corridor) of the decision office; the beacon's post stands at the alert room's (0.5, 0.5), 5 px to its right on screen and 1.95 tiles deeper, so the lamp, 60 px up, sits on the figure's chest. Moving the spot would hold for that beacon only; the badge layer holds for any object (D-554).
- Desk numbers and queue numbers: the kept chair of a waiting session already showed its board number, the same as its figure's, which stays a queue badge (the badge layer); only the lounge's desk number changes (D-558).
- The decision office's plaque between 0.9 and 1.8 tiles along its partition meets reception's spot by the counter's end (geometry test); from 2.0 to 2.7 it covers the third waiting head or reception's shoes. At 0.6 it clears both (D-559).

### Completion Notes

- AC 1 to 6: the cases above, green in the gate (the Epic 4 report's figures).
- AC 8: the readability screenshots and `answers.json` were rendered again from the pushed commit for story 4.14 AC 9.

## File List

- `app/src/lib/scene/objects.ts`, `rig.ts`, `geometry.ts`, `layout.ts`, `text.ts`
- `app/src/components/scene/SceneObjects.svelte`, `Scene.svelte`, `Register.svelte`, `SceneOverlay.svelte`, `Plaque.svelte`
- `app/src/lib/scene/objects.test.ts`, `rig.test.ts`, `layout.test.ts`, `app/src/components/scene/scene.test.ts`
- `app/e2e/readability.spec.ts` (new), `app/e2e/alert.spec.ts`, `app/e2e/scene.spec.ts` (two titles)
- `README.md`, `CHANGELOG.md`, `docs/assets/scene.png`, `scene-dark.png`, `scene.gif`

## Change Log

- 2026-10-29 created from the readability screenshots of story 4.14 AC 9; implemented (D-554 to D-559)
- 2026-10-29 story 4.14 AC 9 run on its screenshots: pass with a caveat (E87, D-701)
