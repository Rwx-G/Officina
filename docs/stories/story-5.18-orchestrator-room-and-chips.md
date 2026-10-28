# Story 5.18: The orchestrator's room in the scene and the header chips

**Status:** Approved

## Story

As the author who looks at the workshop all day,
I want the foreman drawn in a room of its own, with an empty chair and Start when it is absent, a plaque counting its proposals, and two quiet chips in the header,
so that I see at a glance whether the orchestrator is there and what it suggested, without anything it does ever looking like something waiting on me (FR-13).

## Acceptance Criteria

1. **The mock first** (AD-93, R44, D-146, D-119). Before any scene code, the prototype (`_bmad-output/planning-artifacts/design/prototype/`, git-ignored) gains the room in the eighth cell and More workshops in slot 7, and `scene-mock.html` is regenerated at D-146's full occupancy in both themes; the owner sees it before the screenshot baselines with the room are committed. Proof: manual, the owner's approval recorded in this story's Completion Notes.
2. **Seven workshop slots** (AD-93, AD-61 and AD-62 as amended, FR-13 as amended by amendment 8, D-146). Slots 1 to 7 are workshops; an eighth project turns slot 7 into More workshops at once and slot 7's project joins it; the eighth cell is the orchestrator's room. Proof: Vitest `app/src/lib/scene/layout.test.ts > an eighth project turns slot 7 into More workshops`, `> the eighth cell is the orchestrator's room`, and the existing slot cases updated to seven (`> floor projects take the first slots in file order` and the others keep their names).
3. **The room** (AD-93, FR-13). `SceneInput.orchestrator` is nullable (AD-68's rule). Present and live, the room draws the orchestrator with the employees' poses: busy with its back turned, any other status facing the author, neutral, never ringed, never with a raised hand. Absent, it draws an empty chair with a Start target. A neutral plaque counts pending proposals. The room's target and Start are keyboard-reachable (Start launches Officina's own session, not a work order); its accessible name states presence, status and the proposal count. Proof: Vitest `layout.test.ts > a live orchestrator takes the pose of its status`, `> an absent orchestrator leaves an empty chair with Start`, `> a waiting orchestrator is never ringed and never raises a hand`, `> the plaque counts pending proposals`, `> no orchestrator input draws the empty room`; `app/src/lib/scene/text.test.ts > the room's name states presence, status and proposals`.
4. **Geometry** (AD-70 as amended, D-104, D-146). The geometry property runs on D-146's full occupancy: no two targets intersect, no target meets a plaque it does not own, every target is at least 44 CSS px at the minimum scale, every target lies inside the canvas. Proof: Vitest `app/src/lib/scene/geometry.test.ts > no two targets intersect at full occupancy`, `> no target meets a plaque it does not own`, `> every target is 44 CSS px or more at the minimum scale`, `> every target lies inside the canvas`, each now with the room.
5. **Tab order and truthful motion** (AD-63, AD-64, AD-70). The room comes after More workshops and before the shelf; nothing in the scene changes while the payloads are unchanged; a presence or proposal change changes the room once. Proof: Vitest `layout.test.ts > targets follow AD-63's tab order`; Playwright `app/e2e/scene.spec.ts > Tab walks the antechamber, the register, the workshops, More workshops, the room, the shelf and the cabinet in that order`, `> nothing in the scene changes while the payloads are unchanged`, `> Start in the room sends start_orchestrator`.
6. **Screenshots** (D-119, D-146; review F31). The light and dark baselines of `scene-a`, `scene-b` and `scene-c` are regenerated with the room after the owner saw the mock (AC 1), through D-119's path only: a `workflow_dispatch` run of the CI `windows` job with the input that adds `--update-snapshots`, its uploaded `*-snapshots` folders compared by the owner, then committed; `scene-overflow` shows More workshops in slot 7. Proof: Playwright `scene.spec.ts > the three densities match their light baselines`, `> the three densities match their dark baselines`.
7. **Header chips** (AD-93, AD-55, AD-66 as amended, FR-13). Two neutral chips on the quiet surface: "orchestrator live" or "orchestrator absent" (presence per D-166: the entry named `officina-orchestrator` in the workdir), and "N proposals", which opens the Orchestrator view; never the attention colour, not in the window title, not in the taskbar overlay. This story owns the assertion that proposals and the orchestrator stay out of the title, the overlay and the attention colour (review F40). Proof: Playwright `app/e2e/orchestrator.spec.ts > the chips show presence and the proposal count`, `> the proposals chip opens the Orchestrator view`, `> proposals leave the title and the overlay unchanged`, `> the chips, the proposal list and the room never take the attention colour` (computed colours compared, in the real browser, with `--color-attention` resolved on the document root; jsdom resolves no `var()`, D-163, review F22).
8. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Changed: the scene's eighth cell is the orchestrator's room, workshops keep seven slots; README "The orchestrator (v0.5, in progress)" and the scene section: the room, the chips. Proof: review.

## Tasks

- [ ] Prototype and mock with the room; owner review (AC 1)
- [ ] `lib/scene/layout.ts`, `geometry.ts`, `text.ts`: seven slots, the room, poses, Start target, plaque (AC 2-5)
- [ ] `components/scene/*`: the room drawn on layers 1 and 3, its targets on layer 4 (AC 3)
- [ ] Baselines regenerated after the owner's review (AC 6)
- [ ] `Header.svelte` chips (AC 7)
- [ ] Update `CHANGELOG.md` and the README (AC 8)

## Dev Notes

- Requirements: FR-13 as amended (amendment 8). Decisions: AD-55, AD-61 to AD-64 and AD-66 as amended, AD-68, AD-70, AD-93. Risks: R44. Epic decisions: D-102 to D-104, D-119 (Epic 4, as D-146 reads them); P-24, D-124, D-146, D-163, D-166.
- Seams (D-124): the scene's layout, geometry, text and memory (4.3), the drawn layers (4.4), targets and keyboard (4.5), the scene fixtures (4.1), `Header.svelte` (4.4).
- `SceneInput.orchestrator` comes from `Board.orchestrator` (5.16) and the proposal count from `OrchestratorView` (5.17); the layout stays a pure function of its input (AD-60).
- Slot memory (AD-62) keeps working: a remembered project in slot 8 from a v0.4 run is pruned at start like any slot past the new count.
- A waiting orchestrator is never amber (FR-13): it waits on nobody; its proposals are neutral like "N to sort".
- D-119: a baseline is regenerated only with a visual change the owner has seen, never to turn a red run green.

## Testing

- Vitest: `app/src/lib/scene/{layout,geometry,text}.test.ts`, `components.test.ts`.
- Playwright: `app/e2e/{scene,orchestrator}.spec.ts`.
- Manual: the owner's review of the mock (AC 1).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the attention-colour check is a Playwright computed-style case (F22); presence by name and folder (F27); baselines through D-119's `workflow_dispatch` path (F31); sole owner of the title and overlay assertion (F40)
