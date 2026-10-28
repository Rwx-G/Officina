# Story 4.7: Acting from the scene: targets, keyboard, the session file and the line under the scene

**Status:** Approved

## Story

As the author who sees someone queued at the decision office,
I want to click or tab to any employee, read their file and act on it as on a board card, with the keyboard alone if I like, and reach the sessions to resume and done from under the scene,
so that the scene is a place I work from, not only a picture, and every session is reachable however many there are.

## Acceptance Criteria

1. Layer 5 (AD-59, AD-63 as amended, D-104, D-188, D-471): `components/scene/SceneTargets.svelte` renders one transparent, absolutely positioned `<button>` per layout target at its rectangle under the shared transform, in the layout's tab order, with `data-target` and, for a figure, `data-key`; `aria-label` from `text.ts`; a name tag inside the button with `pointer-events: none`, shown by `:hover` and `:focus-visible` only; the two-tone focus ring; focus and hover write no state; a figure's target is at its place, so it moves when the state changes, not with the walk. Proof: Vitest `app/src/components/scene/scene.test.ts > one button per target with its accessible name`, `> DOM order equals the layout's tab order`, `> the name tag is in the button before any hover and ignores the pointer`; Playwright `app/e2e/scene.spec.ts > hovering and focusing targets changes nothing in the scene`, `> Tab walks reception, the queue, the register, the alert room, the offices, the lounge and More offices in that order`, `> a focused target shows the two-tone ring`, `> a figure's target is at its new place as soon as the board changes`.
2. The file (FR-5, AD-69 as amended, D-111, D-182, D-464, D-465): a click, Enter or Space on a figure opens `components/SessionFile.svelte` (420 px, non-modal `role="dialog"` labelled by the session name) with project and its priority when high, number when one of the first nine waiting, name, zone, state with the waiting reason as an amber pill, "waiting since" or "since unknown", the alert's cause, the pending approval with a "Open in Board" button (pointer only) when there is one, subject when set, last prompt, the answer's head and tail, and the card's actions with the board's outcomes from `lib/outcome.ts`; Bring to front has focus when the file opens; Escape closes it and returns focus to the target that now represents the card; when the card leaves the board the file closes. Proof: Playwright `scene.spec.ts > clicking a figure opens its file with Bring to front focused`, `> Enter then Enter brings the session to front as a board card does`, `> Escape closes the file and returns focus to the figure`, `> the file offers the card's actions with their outcomes`, `> the file of an approval-only wait opens Board mode on its approval`, `> the file of an alerted session states its cause`, `> opening and closing the file changes nothing in the scene`, `> the file closes when its session leaves the board`; Vitest `app/src/components/components.test.ts > SessionFile renders session text as text`, `> SessionFile is a dialog labelled by the session name`, `> SessionFile omits the subject line without a subject`, `> SessionFile shows "since unknown" without a wait start`, `> SessionCard and SessionFile word outcomes alike`.
3. Groups (FR-1, FR-2, D-103, D-484): on `scene-overflow`, the register, each zone's group, an office's chip and More offices each open the file panel's list of their sessions in board order, each row a button of at least 44 px that opens that session's file; Escape steps back from a listed file to the list, then to the group's target. Proof: Playwright `scene.spec.ts > the register lists the waiting beyond the queue, each opening its file`, `> a chip lists the office's sessions without a desk`, `> More offices lists the extra projects' sessions`, `> a full lounge lists the rest`, `> Escape steps back from a listed file to the list, then to the group`, `> list rows are 44 px or more`.
4. Keys (FR-6, AD-63, AD-66, D-187): arrows move focus with `nearestTarget` inside the focused target's group, stay put at the group's edge and call `preventDefault`; digits 1 to 9 bring the nth Needs you session to front directly, alerted ones included, as on the board. Proof: Playwright `scene.spec.ts > arrows move within a zone, stop at its edge and do not scroll the area`, `> keys 1 to 9 bring the nth waiting session to front from the scene, in the alert room too`.
5. Size and topmost (FR-6, AD-70): at 1000 x 760 and 1400 x 900, on `scene-c` and `scene-overflow`, settled, every target after `scrollIntoViewIfNeeded()` has a box of 44 x 44 CSS px or more (compared with `>= 44 - 0.01`) and `document.elementFromPoint` at its centre is that target. Proof: Playwright `scene.spec.ts > every target is 44 px or more and topmost at its centre`.
6. Readability (PRD section 7, AD-70): for every board of the fixture index with a Needs you session, the scene's text alternative names the count and the oldest waiting session's name. Proof: Playwright `scene.spec.ts > the text alternative names the waiting count and the oldest waiting session`.
7. The line under the scene (FR-4, AD-177, D-472): `components/scene/SceneStrip.svelte`, outside the scene element, shows "N to resume" and "N done in 24 h" (worded from `done_window_ms`), each a button of at least 44 px opening Board mode; it changes only when those counts change. Proof: Playwright `scene.spec.ts > the line under the scene counts To resume and Done and opens Board mode`; Vitest `components.test.ts > SceneStrip words the Done window from the board`.
8. Untrusted text (frontend rule, AD-59, AD-69): a name, a prompt or a project name carrying markup renders as text in targets, name tags, the register, plaques, lists, the strip and the file, and never reaches layer 3. Proof: Playwright `scene.spec.ts > session text renders as text in the scene and the file` (an `<img onerror>` name); the `svelte/no-at-html-tags` lint; story 4.6's markup test.
9. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: the session file, keyboard in the scene, the line under the scene); README "The scene": the file and its actions, Enter twice, Escape, arrows, digits, the lists behind the groups, where the gauges and relay state are. Proof: review.

## Tasks

- [ ] `SceneTargets.svelte` with name tags and the two-tone ring (AC 1)
- [ ] `lib/outcome.ts` from `SessionCard.svelte`; `SessionFile.svelte` with its file and list forms; open, close and focus return in `Scene.svelte` (AC 2, 3)
- [ ] Arrow and digit handling in the Scene (AC 4)
- [ ] `SceneStrip.svelte` (AC 7)
- [ ] Vitest cases; `scene.spec.ts` cases (AC 1-8)
- [ ] Update `CHANGELOG.md` and the README (AC 9)

## Dev Notes

- Requirements: FR-1, FR-2, FR-4, FR-5, FR-6; PRD section 7 (readability). Decisions: AD-59, AD-63, AD-66, AD-69, AD-70, AD-177 as amended. Epic decisions: D-28 (Epic 2); D-103, D-104, D-107, D-111, D-121, D-181, D-182, D-187, D-188, D-464, D-465, D-471, D-472, D-484.
- Dependencies: story 4.6 (the drawn scene). `Card.subject` (v0.3) feeds the subject line.
- Targets never overlap (story 4.5's property); AC 5 is its runtime check. At 1000 x 760 the sessions area scrolls (D-105), hence the scroll before `elementFromPoint`.
- A walker is not clickable mid-walk (D-471): its target is already at its new place, which is also what a keyboard user reaches. The AC 5 and AC 1 cases wait for `data-moving="0"` only where they compare figures and targets.
- The file panel lives outside the scene element (D-111), so neither its opening nor its text touches the scene. The approval itself is approved on its board card (D-28); the file only sends the author there.
- The strip replaces the old instruments story (D-472): quota gauges and relay state stay in the header, which every mode shows.
- Escape's focus return looks the card up by key in the current layout: its figure if it has one, else the group holding it, else the scene element.

## Testing

- Vitest: `app/src/components/scene/scene.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/scene.spec.ts` on `scene-a`, `scene-b`, `scene-c`, `scene-overflow` and every fixture board for readability.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created as story 4.5
- 2026-10-28 reworked after review (F2, F4, F14, F16, F26, F27)
- 2026-10-28 renumbered 4.7 and reworked for the scene v2 (D-460): targets at places, the file gains zone, wait start, alert cause and approval, groups per zone, the line under the scene replaces the shelf and cabinet
