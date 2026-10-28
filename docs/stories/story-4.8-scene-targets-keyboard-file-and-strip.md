# Story 4.8: Acting from the scene: targets, keyboard, the session file and the line under the scene

**Status:** Review

## Story

As the author who sees someone queued at the decision office,
I want to click or tab to any employee, read their file and act on it as on a board card, with the keyboard alone if I like, and reach the sessions to resume and done from under the scene,
so that the scene is a place I work from, not only a picture, and every session is reachable however many there are.

## Acceptance Criteria

1. Layer 5 (AD-59, AD-63 as amended, D-104, D-188, D-471): `components/scene/SceneTargets.svelte` renders one transparent, absolutely positioned `<button>` per layout target at its rectangle under the shared transform, in the layout's tab order, with `data-target` and, for a figure, `data-key`; `aria-label` from `text.ts`; a name tag inside the button with `pointer-events: none`, shown by `:hover` and `:focus-visible` only; the two-tone focus ring; focus and hover write no state; a figure's target is at its place, so it moves when the state changes, never with a walk (story 4.10). Proof: Vitest `app/src/components/scene/scene.test.ts > one button per target with its accessible name`, `> DOM order equals the layout's tab order`, `> the name tag is in the button before any hover and ignores the pointer`; Playwright `app/e2e/scene.spec.ts > hovering and focusing targets changes nothing in the scene`, `> Tab walks reception, the queue, the register, the alert room, the offices, the lounge and More offices in that order`, `> a focused target shows the two-tone ring`, `> a figure's target is at its new place as soon as the board changes`.
2. The file (FR-5, AD-69 as amended, D-111, D-182, D-464, D-465): a click, Enter or Space on a figure opens `components/SessionFile.svelte` (420 px, non-modal `role="dialog"` labelled by the session name) with project and its priority when high, number when one of the first nine waiting, name, zone, state with the waiting reason as an amber pill, "waiting since" or "since unknown", the alert's cause, the pending approval with a "Open in Board" button (pointer only) when there is one, subject when set, last prompt, the answer's head and tail, and the card's actions with the board's outcomes from `lib/outcome.ts`; Bring to front has focus when the file opens; Escape closes it and returns focus to the target that now represents the card; when the card leaves the board the file closes. Proof: Playwright `scene.spec.ts > clicking a figure opens its file with Bring to front focused`, `> Enter then Enter brings the session to front as a board card does`, `> Escape closes the file and returns focus to the figure`, `> the file offers the card's actions with their outcomes`, `> the file of an approval-only wait opens Board mode on its approval`, `> the file of an alerted session states its cause`, `> the file of an unknown session that keeps an alert says last known`, `> opening and closing the file changes nothing in the scene`, `> the file closes when its session leaves the board`; Vitest `app/src/components/components.test.ts > SessionFile renders session text as text`, `> SessionFile is a dialog labelled by the session name`, `> SessionFile omits the subject line without a subject`, `> SessionFile shows "since unknown" without a wait start`, `> SessionCard and SessionFile word outcomes alike`.
3. Groups (FR-1, FR-2, D-103, D-484): on `scene-overflow`, the register, each zone's group, an office's chip and More offices each open the file panel's list of their sessions in board order, each row a button of at least 44 px that opens that session's file; Escape steps back from a listed file to the list, then to the group's target. Proof: Playwright `scene.spec.ts > the register lists the waiting beyond the queue, each opening its file`, `> a chip lists the office's sessions without a desk`, `> More offices lists the extra projects' sessions`, `> a full lounge lists the rest`, `> Escape steps back from a listed file to the list, then to the group`, `> list rows are 44 px or more`.
4. Keys (FR-6, AD-63, AD-66, D-187): arrows move focus with `nearestTarget` inside the focused target's group, stay put at the group's edge and call `preventDefault`; digits 1 to 9 bring the nth Needs you session to front directly, alerted ones included, as on the board. Proof: Playwright `scene.spec.ts > arrows move within a zone, stop at its edge and do not scroll the area`, `> keys 1 to 9 bring the nth waiting session to front from the scene, in the alert room too`.
5. Size and topmost (FR-6, AD-70): at 1000 x 760 and 1400 x 900, on `scene-c` and `scene-overflow`, settled, every target after `scrollIntoViewIfNeeded()` has a box of 44 x 44 CSS px or more (compared with `>= 44 - 0.01`) and `document.elementFromPoint` at its centre is that target. Proof: Playwright `scene.spec.ts > every target is 44 px or more and topmost at its centre`.
6. Readability (PRD section 7, AD-70): for every board of the fixture index with a Needs you session, the scene's text alternative names the count and the oldest waiting session's name. Proof: Playwright `scene.spec.ts > the text alternative names the waiting count and the oldest waiting session`.
7. The line under the scene (FR-4, AD-177, D-472): `components/scene/SceneStrip.svelte`, outside the scene element, shows "N to resume" and "N done in 24 h" (worded from `done_window_ms`), each a button of at least 44 px opening Board mode; it changes only when those counts change. Proof: Playwright `scene.spec.ts > the line under the scene counts To resume and Done and opens Board mode`; Vitest `components.test.ts > SceneStrip words the Done window from the board`.
8. Untrusted text (frontend rule, AD-59, AD-69): a name, a prompt or a project name carrying markup renders as text in targets, name tags, the register, plaques, lists, the strip and the file, and never reaches layer 3. Proof: Playwright `scene.spec.ts > session text renders as text in the scene and the file` (an `<img onerror>` name); the `svelte/no-at-html-tags` lint; story 4.7's markup test.
9. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: the session file, keyboard in the scene, the line under the scene); README "The scene": the file and its actions, Enter twice, Escape, arrows, digits, the lists behind the groups, where the gauges and relay state are. Proof: review.

## Tasks

- [x] `SceneTargets.svelte` with name tags and the two-tone ring (AC 1)
- [x] `lib/outcome.ts` from `SessionCard.svelte`; `SessionFile.svelte` with its file and list forms; open, close and focus return in `Scene.svelte` (AC 2, 3)
- [x] Arrow and digit handling in the Scene (AC 4)
- [x] `SceneStrip.svelte` (AC 7)
- [x] Vitest cases; `scene.spec.ts` cases (AC 1-8)
- [x] Update `CHANGELOG.md` and the README (AC 9)

## Dev Notes

- Requirements: FR-1, FR-2, FR-4, FR-5, FR-6; PRD section 7 (readability). Decisions: AD-59, AD-63, AD-66, AD-69, AD-70, AD-177 as amended. Epic decisions: D-28 (Epic 2); D-103, D-104, D-107, D-111, D-121, D-181, D-182, D-187, D-188, D-464, D-465, D-471, D-472, D-484, D-494, D-496, D-497.
- Dependencies: story 4.7 (the scene drawn in place); it does not wait for walking (D-497), and can run beside story 4.9. `Card.subject` (v0.3) feeds the subject line.
- Targets never overlap (story 4.6's property); AC 5 is its runtime check. At 1000 x 760 the sessions area scrolls (D-105), hence the scroll before `elementFromPoint`.
- A walker is not clickable mid-walk (D-471): its target is already at its new place, which is also what a keyboard user reaches; story 4.10 adds the walking case. The AC 5 and AC 1 cases wait for `data-moving="0"` (always `0` until story 4.10) where they compare figures and targets, so they hold once walking exists.
- The file's alert line reads `card.scene`: an approval-only wait shows its request time as "waiting since" and alerts only for a high-priority project (D-496); an unknown employee that keeps an alert shows the cause with "last known" (D-494).
- The file panel lives outside the scene element (D-111), so neither its opening nor its text touches the scene. The approval itself is approved on its board card (D-28); the file only sends the author there.
- The strip replaces the old instruments story (D-472): quota gauges and relay state stay in the header, which every mode shows.
- Escape's focus return looks the card up by key in the current layout: its figure if it has one, else the group holding it, else the scene element.

## Testing

- Vitest: `app/src/components/scene/scene.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/scene.spec.ts` on `scene-a`, `scene-b`, `scene-c`, `scene-overflow` and every fixture board for readability.

## Dev Agent Record

### Debug Log

- The sessions area's scroller moves into `Scene.svelte` (`[data-testid=scene-area]`, the scene element's parent, which `fit` and the tests already measure); `App.svelte`'s `main.sessions-area` becomes its positioned, non-scrolling host. So the file panel, a sibling of the scroller, overlays the visible area's right edge and stays there while the area scrolls (D-111), and the line under the scene, inside the scroller, scrolls with the scene and so never sits over a target the area has scrolled under it.
- The line under the scene (AD-177, D-525): `geometry.ts` gains `STRIP` and `stripRect(columns)`, the room it may take in the canvas's bottom-left corner (360 x `TARGET_MIN` logical px, bottom on `PAD.bottom`). `geometry.test.ts > the line under the scene lies below the floor's front-left edge and meets no target or plaque` holds it for every column count, More offices included; the e2e checks the drawn buttons stay inside that room at the scene's scale and meet no drawn target. `Scene.svelte` positions it with the scale and the centring offset; it takes no height: on `scene-eight` at 1920 x 1001 with the alert band, the area is 875 px high and the scene 874 at scale 0.9229, the strip at 818 to 862 inside it.
- "Open in Board (pointer only)" read with D-28: the pointer-only rule is the approval's, which stays on its board card; the file's button only switches modes, so it is a normal button the keyboard reaches too.
- A file follows its card into any column and closes only when the card leaves the board (D-111's "in whatever column"): a card moved to To resume keeps its file, without the zone line, and Escape then focuses the scene element, since no target represents it.
- When the card leaves the board, or a list's group empties, the panel closes and focus goes to the scene element only when focus was in the panel (it is then on the body); a focus the author put elsewhere is not taken.
- The panel has its own button ("Close", or "Back to the list" for a listed file), doing what Escape does, so a pointer user can close a non-modal panel. A list's heading is its room's plaque text; a listed file's Escape refocuses the row it came from.
- A figure target is labelled by `aria-label` and keeps its name tag `aria-hidden`, so the name is not read twice. Arrows with a modifier pass through. The focus ring scales with the scene (2.75 CSS px at `MIN_SCALE`, 4.5 at `MAX_SCALE`), as every other part of it does.
- Digits: `App.svelte`'s handler already acts in every mode of the Sessions view on `store.needsYou`, alerted ones included; the story adds its cases, no code.
- `scene-overflow` had no chip group: sessions started later took no desk, so its busiest office filled four desks and its chip spot exactly. Two more sessions at work there (`scripts/scene-fixtures.mjs`, D-101's intent "a +n chip") leave one in the chip's list.
- `lib/outcome.ts` holds `OUTCOME_TEXT`, an exhaustive record by outcome, `outcomeText` and `visibleOutcome`; `SessionCard.svelte` and `SessionFile.svelte` read it (story 4.11's Focus rows will).
- Layout targets: none found wrong; nothing in `layout.ts` changed. More offices lists only its projects' sessions at work; their waiting and resting sessions are in the queue and the lounge, as D-484 places them.
- Tooling: the port given for Playwright, 4182, was held by another process serving a non-e2e build (every page failed on Tauri's missing internals), so the runs used `OFFICINA_E2E_PORT=4192`.
- Adversarial pass: stale and unknown employees (the file's zone and "last known"), a card leaving to To resume and leaving the board, a key that leaves and returns (its file stays closed), a board arriving while a list is open and its group emptying, an empty board, more than eight projects, keyboard only (Tab, arrows at every edge of the queue and an office, Escape chains, focus return), reduced motion (no animation added), no new token, 1000 x 760, 1400 x 900 and 1920 x 1001. One miss found and fixed with a test: `scene-overflow` had no chip list (above). Two cases added: `> a list whose group empties closes, focus on the scene`, `> an empty board has no target and the line under the scene says zero`.

### Completion Notes

Playwright cases are in `app/e2e/scene.spec.ts` (describe "acting from the scene" unless named otherwise); Vitest in `app/src/components/scene/scene.test.ts` and `app/src/components/components.test.ts`.

- AC 1: Vitest `scene.test.ts > one button per target with its accessible name`, `> DOM order equals the layout's tab order`, `> the name tag is in the button before any hover and ignores the pointer`, `> layers 1 to 4 are hidden from assistive technology and ignore the pointer` (layer 5 named); Playwright `> hovering and focusing targets changes nothing in the scene`, `> Tab walks reception, the queue, the register, the alert room, the offices, the lounge and More offices in that order`, `> a focused target shows the two-tone ring`, `> a figure's target is at its new place as soon as the board changes`.
- AC 2: Playwright `> clicking a figure opens its file with Bring to front focused`, `> Enter then Enter brings the session to front as a board card does` (Space too), `> Escape closes the file and returns focus to the figure` (also after the card moves to its desk, and to the scene element once it is in To resume), `> the file offers the card's actions with their outcomes`, `> the file of an approval-only wait opens Board mode on its approval`, `> the file of an alerted session states its cause`, `> the file of an unknown session that keeps an alert says last known`, `> opening and closing the file changes nothing in the scene`, `> the file closes when its session leaves the board`; Vitest `components.test.ts > SessionFile renders session text as text`, `> SessionFile is a dialog labelled by the session name`, `> SessionFile omits the subject line without a subject`, `> SessionFile shows "since unknown" without a wait start`, `> SessionCard and SessionFile word outcomes alike`, plus `> SessionFile shows its project's priority, its number and the zone`, `> SessionFile states an alert's cause, last known on unknown data`, `> SessionFile shows a pending approval and opens Board on it`, `> the list form lists its sessions, each a button that opens its file`.
- AC 3: Playwright `> the register lists the waiting beyond the queue, each opening its file`, `> a chip lists the office's sessions without a desk`, `> More offices lists the extra projects' sessions`, `> a full lounge lists the rest`, `> Escape steps back from a listed file to the list, then to the group`, `> list rows are 44 px or more`, `> a list whose group empties closes, focus on the scene`.
- AC 4: Playwright `> arrows move within a zone, stop at its edge and do not scroll the area` (at 1000 x 760, every arrow from every target of the queue and of an office, expected targets from `nearestTarget`, `defaultPrevented` on each), `> keys 1 to 9 bring the nth waiting session to front from the scene, in the alert room too`.
- AC 5: Playwright `> every target is 44 px or more and topmost at its centre` (`scene-c` and `scene-overflow`, 1000 x 760 and 1400 x 900, each target after `scrollIntoViewIfNeeded`, and the strip in its corner over no target); describe "scene" `> eight projects fit a maximised Full HD window` and `> ... with the alert band` now also check the strip and every target's size at 1920 x 1001.
- AC 6: Playwright describe "the text alternative names the waiting count and the oldest waiting session", one case per fixture board with a Needs you session.
- AC 7: Playwright `> the line under the scene counts To resume and Done and opens Board mode` (worded from `done_window_ms`, no write while its counts hold, both buttons open Board mode); Vitest `components.test.ts > SceneStrip words the Done window from the board` (every fixture board, and a 12 h window), `scene.test.ts > the line under the scene is outside the scene element, right after it`; `geometry.test.ts > the line under the scene lies below the floor's front-left edge and meets no target or plaque`.
- AC 8: Playwright `> session text renders as text in the scene and the file` (an `<img onerror>` name in a queue figure's label and tag, the register, a plaque, a group's label and list, the file); Vitest `scene.test.ts > plaque and register names render as text` (targets and tags added), `components.test.ts > SessionFile renders session text as text`; the `svelte/no-at-html-tags` lint; story 4.7's markup test.
- AC 9: `CHANGELOG.md` `[Unreleased]` Added "Acting from the scene"; README "The scene (v0.4, in progress)" gains "Acting from the scene" (the file and its actions, Enter twice, Escape, Tab and arrows, digits, the lists behind the groups, the line under the scene, the gauges and relay state in the header). Proof: review.
- Gates run: `pnpm check`, `pnpm lint`, `pnpm test` (207 cases), `pnpm build` then `cargo test -p officina-app`, `cargo test -p officina --test readme`, the whole Playwright suite with `--repeat-each=3` (654 passed, 3 skipped).
- Open: nothing in this story. Story 4.10 adds the walking case of AC 1 and AC 5 (the cases already wait for `data-moving="0"`).

## File List

- `app/src/lib/outcome.ts` (new)
- `app/src/components/SessionCard.svelte` (modified)
- `app/src/components/SessionFile.svelte` (new)
- `app/src/components/scene/SceneTargets.svelte` (new)
- `app/src/components/scene/SceneStrip.svelte` (new)
- `app/src/components/scene/Scene.svelte` (modified)
- `app/src/components/scene/scene.test.ts` (modified)
- `app/src/components/components.test.ts` (modified)
- `app/src/App.svelte` (modified)
- `app/src/lib/scene/geometry.ts` (modified)
- `app/src/lib/scene/geometry.test.ts` (modified)
- `app/scripts/scene-fixtures.mjs` (modified)
- `app/e2e/fixtures.ts` (modified)
- `app/e2e/scene.spec.ts` (modified)
- `README.md` (modified)
- `CHANGELOG.md` (modified)
- `docs/stories/story-4.8-scene-targets-keyboard-file-and-strip.md` (modified)
- `docs/stories/sprint-status.yaml` (modified)

## Change Log

- 2026-10-28 created as story 4.5
- 2026-10-28 reworked after review (F2, F4, F14, F16, F26, F27)
- 2026-10-28 renumbered 4.7 and reworked for the scene v2 (D-460): targets at places, the file gains zone, wait start, alert cause and approval, groups per zone, the line under the scene replaces the shelf and cabinet
- 2026-10-28 renumbered 4.8 after the plan challenge (D-497, D-498): it needs the placed scene only; the file words a last known alert; story numbers per D-498
- 2026-10-28 implemented: targets, keys, the file and its lists, the line under the scene in the canvas's corner; moved to review
