# Story 4.5: Acting from the scene: targets, keyboard and the session file

**Status:** Approved

## Story

As the author who sees someone waiting in the antechamber,
I want to click or tab to any employee, read their file and act on it as on a board card, with the keyboard alone if I like,
so that the scene is a place I work from, not only a picture, and every waiting session is reachable however many there are.

## Acceptance Criteria

1. Layer 4 (AD-59, AD-63, D-104, D-188): `components/scene/SceneTargets.svelte` renders one transparent, absolutely positioned `<button>` per layout target at its rectangle under the shared transform, in the layout's tab order, with `data-target` (its id) and, for a figure, `data-key`; `aria-label` from `text.ts`; a name tag inside the button, with `pointer-events: none`, shown by `:hover` and `:focus-visible` only; the focus ring is two-tone: a 3 px `--color-working` outline with a 2 px offset and `box-shadow: 0 0 0 2px var(--color-surface)` filling the gap; focus and hover write no state (no roving `tabindex`). Proof: Vitest `app/src/components/scene/scene.test.ts > one button per target with its accessible name`, `> DOM order equals the layout's tab order`, `> the name tag is in the button before any hover and ignores the pointer`; Playwright `app/e2e/scene.spec.ts > hovering and focusing targets changes nothing in the scene`, `> Tab walks the antechamber, the register, the workshops, More workshops, the shelf and the cabinet in that order`, `> a focused target shows the two-tone ring` (computed `outline` and `box-shadow`) (review F14, F27).
2. The file (FR-5, AD-69, D-111, D-182): a click, Enter or Space on a figure opens `components/SessionFile.svelte` (420 px, the card's radius and tokens; a non-modal `role="dialog"` labelled by the session name) with project, number when one of the first nine waiting, name, state with the waiting reason as an amber pill, subject when `card.subject` is set, last prompt, the answer's head (`card.last_message`) and tail (`card.last_message_tail`), and the card's actions with the board's outcomes (`store.focus`, Copy attach command when `attach_available`, Wrong window? after a focused outcome), the outcome wording from `lib/outcome.ts`; Bring to front has focus when the file opens, so Enter twice does what Enter does on a board card; Escape closes it and returns focus to the target that now represents the card; when the card leaves the board the file closes. Proof: Playwright `scene.spec.ts > clicking a figure opens its file with Bring to front focused`, `> Enter then Enter brings the session to front as a board card does`, `> Escape closes the file and returns focus to the figure`, `> the file offers the card's actions with their outcomes`, `> opening and closing the file changes nothing in the scene`, `> the file closes when its session leaves the board`; Vitest `app/src/components/components.test.ts > SessionFile renders session text as text`, `> SessionFile is a dialog labelled by the session name`, `> SessionFile omits the subject line without a subject`, `> SessionFile shows the answer's head and tail from the card`, `> SessionCard and SessionFile word outcomes alike` (review F4, F27).
3. Groups (FR-2 and FR-1 as amended, D-103): on `scene-overflow`, the register's overflow target, a workshop's chip and More workshops each open the file panel's list of their sessions in board order, each row a button of at least 44 px that opens that session's file; Escape from a listed file returns to the list, from the list to the group's target. Proof: Playwright `scene.spec.ts > the register lists the waiting beyond nine, each opening its file`, `> a chip lists the workshop's sessions without a desk`, `> More workshops lists the extra projects' sessions`, `> Escape steps back from a listed file to the list, then to the group`, `> list rows are 44 px or more` (review F26).
4. Keys (FR-6 as amended, AD-63, AD-66, D-187): arrows move focus with `nearestTarget` from `document.activeElement`'s target inside its group, stay put at the group's edge, and call `preventDefault` so the sessions area does not scroll; digits 1 to 9 bring the nth Needs you session to front directly, as on the board, without opening the file. Proof: Playwright `scene.spec.ts > arrows move within a room, stop at its edge and do not scroll the area`, `> keys 1 to 9 bring the nth waiting session to front from the scene` (review F27).
5. Size and topmost (FR-6, AD-70): at 1000 x 760 and 1400 x 900, on `scene-c` and `scene-overflow`, every target, after `scrollIntoViewIfNeeded()`, has a box of 44 x 44 CSS px or more (compared with `>= 44 - 0.01`) and `document.elementFromPoint` at its centre is that target. Proof: Playwright `scene.spec.ts > every target is 44 px or more and topmost at its centre` (review F16).
6. Readability (PRD section 7, AD-70): for every board of the fixture index with a Needs you session, the scene's text alternative names the count and the oldest waiting session's name. Proof: Playwright `scene.spec.ts > the text alternative names the waiting count and the oldest waiting session` (boards through `app/e2e/fixture-index.ts`, never a list of names) (review F2).
7. Shelf and cabinet (D-112): without a registry both open Board mode; with one, the Subjects view. Proof: Playwright `scene.spec.ts > the shelf and the cabinet open Board mode without a registry`, `> the shelf and the cabinet open the Subjects view with a registry` (the second through story 3.5's `__officinaPushRegistry`).
8. Untrusted text (frontend rule, AD-69): a name or prompt carrying markup renders as text in targets, name tags, the register, plaques, lists and the file. Proof: Playwright `scene.spec.ts > session text renders as text in the scene and the file` (an `<img onerror>` name); the `svelte/no-at-html-tags` lint.
9. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: the session file, keyboard in the scene); README "The scene": the file and its actions, Enter twice, Escape, arrows, digits, the lists behind the register, the chip and More workshops. Proof: review.

## Tasks

- [ ] `SceneTargets.svelte` with name tags and the two-tone ring (AC 1)
- [ ] `lib/outcome.ts` from `SessionCard.svelte`; `SessionFile.svelte` with its file and list forms; open, close and focus return in `Scene.svelte` (AC 2, 3)
- [ ] Arrow and digit handling in the Scene (AC 4)
- [ ] Shelf and cabinet activation through `lib/view.svelte.ts` (AC 7)
- [ ] Vitest cases; `scene.spec.ts` cases (AC 1-8)
- [ ] Update `CHANGELOG.md` and the README (AC 9)

## Dev Notes

- Requirements: FR-1 (More workshops' list), FR-2 (the register's list), FR-5, FR-6 as amended; PRD section 7 (readability). Decisions: AD-59, AD-63, AD-66, AD-69, AD-70. Epic decisions: D-28 (Epic 2); D-98, D-103, D-104, D-107, D-111, D-112, D-121, D-181, D-182, D-187, D-188.
- Seams (D-98): `Card.subject` (3.4) feeds the subject line; the answer's tail is on the card (D-182, from story 3.2's event field through 4.1), so the file needs no registry for it. The registry mock global of story 3.5 (`window.__officinaPushRegistry`) feeds AC 7's second case. If 3.4 or 3.5 has not merged, those parts wait.
- Targets never overlap (AD-61, D-104), so the browser's hit testing never arbitrates; AC 5 is the runtime check of story 4.3's geometry property. At 1000 x 760 the header wraps with v0.2's and v0.3's controls and the sessions area scrolls (D-105), hence the scroll before `elementFromPoint`; at `MIN_SCALE` a 48 px target is exactly 44 CSS px, hence the tolerance.
- The name tag is rendered inside the button from the start and shown by CSS: hovering writes nothing (AD-63), and `pointer-events: none` keeps a shown tag from covering a neighbour. The file panel overlays the sessions area (D-111) and lives outside the scene element, so neither its opening nor its outcome text touches the scene.
- The two-tone ring (D-188): `--color-working` alone is 1.23:1 on the light figure tunic and 1.13:1 on the dark one; the `--color-surface` inner ring keeps the outline visible wherever it crosses.
- Bring to front calls `store.focus(card)`, whose repeat guard (`FOCUS_REPEAT_GUARD_MS`) already stops a held Enter from opening one attach console per repeat.
- Buttons answer Enter and Space natively; no key approves anything here (D-28 concerns approval cards, which the scene does not draw).
- Escape's focus return looks the card up by key in the current layout: its figure if it has one, else the group target holding it, else the scene element.

## Testing

- Vitest: `app/src/components/scene/scene.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/scene.spec.ts` on `scene-a`, `scene-b`, `scene-c`, `scene-overflow` and every fixture board for readability.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: readability over the fixture index (F2); the tail read from the card (F4, D-182); two-tone focus ring asserted (F14, D-188); size case scrolls and allows float rounding (F16); list rows checked at 44 px (F26); file as a non-modal dialog, no roving `tabindex`, `preventDefault` on arrows, name tags ignoring the pointer (F27)
