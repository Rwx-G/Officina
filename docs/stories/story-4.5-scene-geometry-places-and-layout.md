# Story 4.5: Scene geometry, places, paths and layout

**Status:** Approved

## Story

As the author who learns where each project's office is,
I want one pure function that turns the board into places, poses, targets and words on the approved floor, with offices remembered on this machine and routes along the corridors computed without a browser,
so that the scene is the same every time for the same payloads, every employee has a place to walk to, and every rule is tested in Vitest.

## Acceptance Criteria

1. Geometry (AD-169, D-468): `app/src/lib/scene/geometry.ts` holds the prototype v2's projection and constants (tile, z unit, room size, corridor, margin, walls, slab, partitions, padding, corridor line), the cell of each shared zone, the office cells for 4, 5 and 6 columns in slot order, `canvas(columns)` computed from them, `columnsFor(officeCount)`, `TARGET_CSS_MIN` (44), `MIN_SCALE` as `TARGET_CSS_MIN / FIGURE_TARGET.w`, `MAX_SCALE`, `TARGET_MIN`, and one function per target rectangle (figure at a place, desk chip, each zone's group, More offices). Tests import them, never retype them. Proof: Vitest `app/src/lib/scene/geometry.test.ts > the canvas follows the columns` (4, 5 and 6 columns: computed sizes grow and no room's projected corners move), `app/src/tokens.test.ts > TARGET_CSS_MIN equals app.css's --target-min`.
2. Places and graph (AD-170): `places.ts` builds from `geometry.ts` the nodes and spots of reception (exit, entry, hub, door, four visitor spots), the decision office (door, queue spots, the decider's desk and chair, consult spot and staff nodes marked staff), each office (door on the corridor side, aisle, four chairs, two chip spots), the lounge (seats with slide-in offsets, standing places, door) and the alert room (door, five spots), and the corridor nodes chained in `gx` order. Proof: Vitest `app/src/lib/scene/places.test.ts > every spot is reachable from the exit without staff nodes` (4, 5 and 6 columns), `> each door links its room to the corridor`, `> staff nodes are never on a route between two non-staff spots`.
3. Paths (AD-170): `paths.ts` exports `route(graph, from, toSpot)` (Dijkstra on Euclidean lengths, never through staff nodes, from the nearer of a walker's previous and next node when mid-walk) and `smooth(points)` (quadratic corners of radius 0.45 tile, at most half of each segment), deterministic. Proof: Vitest `app/src/lib/scene/paths.test.ts > a route follows graph edges`, `> a route from mid-walk starts at the nearer node`, `> smoothing keeps both ends and stays within the corner radius`, `> the same input gives the same route`.
4. Geometry property (AD-70, D-104, D-471, D-484): built from `geometry.ts` and `places.ts` alone, at full occupancy for 4, 5 and 6 columns (every spot of every zone, every desk, every chip, every zone's group, More offices at 6 columns): no two targets intersect; no target meets a plaque at its 140 px cap other than the one it owns; every target side is `TARGET_MIN` or more; every target lies inside the canvas. Spots that fail are spread or removed in `places.ts`, never the property. Proof: Vitest `geometry.test.ts > no two targets intersect at full occupancy`, `> no target meets a plaque it does not own`, `> every target is 44 CSS px or more at the minimum scale`, `> every target lies inside the canvas`.
5. The function (AD-60 as amended): `layout.ts` exports `layoutScene(input: SceneInput, memory: SceneMemory): SceneLayout` with `SceneInput = { board: Board }`, and no DOM, clock, storage, flag or `Math.random`. `SceneLayout` holds the column count, rooms with plaques (text, count, state: quiet, ink, attention, alert, dimmed, "high priority" pill), employees (card key, zone, spot id, pose, marks), desks (free, occupied, kept with a number), groups with their key lists, targets in tab order (id, group, kind, rectangle, `aria-label`, name tag, keys), the text alternative and the next `SceneMemory`. Proof: Vitest `app/src/lib/scene/layout.test.ts > identical input gives a deep-equal layout`, `> the layout is a fixed point of its own memory`, each over every board of the fixture index (D-181).
6. Offices (FR-1, AD-62 as amended, D-99, D-102, D-468): floor projects first in `floor_projects` order (a repeated name keeps its first place only), then remembered projects with a live card on the first layout, then first appearance with D-102's ties; a slot is kept until restart and dimmed without a live session; the fifth project adds a column, the seventh another; a ninth project turns office 8 into More offices; the column count never decreases within a run. Proof: `layout.test.ts > floor projects take the first offices in file order`, `> a repeated floor project name takes no second office`, `> remembered projects with a live card come next`, `> new projects follow in order of first appearance`, `> ties go to the earliest start, then the name`, `> an office keeps its slot and dims when its sessions end`, `> a fifth project adds a column and the floor never shrinks`, `> a ninth project turns office 8 into More offices`, `> other and unknown are ordinary projects`.
7. Zones to places (AD-62 as amended, AD-167, D-462 to D-464, D-482, D-484): each live card's place follows `card.scene.zone`, never its column; `office` at its desk; `decision` on the queue's spots in board order (Needs you in `columns.needs_you` order, then approval-only waits by `requested_at_ms`); `alert` on the alert room's spots in the same order; `reception` and `lounge` on the first free spot, kept while there; `unknown` at its last place from the memory, or at its desk on first sight; beyond a zone's spots, the zone's group. Proof: `layout.test.ts > each zone places its employees`, `> the queue follows board order with approvals after the numbered`, `> the alert room follows board order`, `> reception and lounge spots are kept while the employee stays`, `> an unknown employee keeps its last place`, `> an unknown employee first seen sits at its desk`, `> beyond its spots a zone fills its group`, `> only the changed card and the queue members whose order changed get a new place` (for every live card of every fixture board).
8. Desks (AD-62 as amended, D-102, D-107): four per office; a live session takes the lowest free desk the first time it is laid out outside reception and keeps it while live, whatever its zone; a session without a free desk joins the office's chip list until its own zone changes; a kept desk shows the board number (queue, alert room) or the desk number (lounge). Proof: `layout.test.ts > a session at reception has no desk yet`, `> a session keeps its desk while it waits or rests`, `> a kept desk shows the number up to nine or the desk number in the lounge`, `> an ended session frees its desk`, `> a fifth live session goes to the chip list and stays there until its zone changes`.
9. Poses and marks (FR-2, AD-171): `office` seated working (back turned, screen lit); `decision` and `alert` standing facing, ticket with the board number up to nine, or the approval ticket; `lounge` seated facing, raised hand, Your turn ring, desk badge; `reception` standing, back turned toward the counter; `unknown` keeps its last pose with a question mark. Proof: `layout.test.ts > the pose and marks follow the zone` (iterating `Zone` values from the generated types).
10. Targets and arrows (AD-63 as amended, D-103, D-187, D-471): tab order as AD-63; groups are reception, the decision office (its queue and register), the alert room, each office (desks then chip), the lounge, More offices; a figure's target is at its place; `nearestTarget(targets, fromId, direction)` as D-187. Proof: `layout.test.ts > targets follow the tab order`, `> a figure's target sits at its place`, `> arrows follow the vectors of a full office and a full queue`, `> an arrow with no target within 45 degrees goes nowhere`.
11. Words (AD-63, D-107): `text.ts` builds the text alternative and every `aria-label` and name tag from the layout: counts and names, zone words and state words from exhaustive records over `Zone`, `Column` and `Badge`, never a relative time. Proof: Vitest `app/src/lib/scene/text.test.ts > the summary names the waiting count, the alert count and the oldest waiting session`, `> nobody waiting reads as such`, `> every figure's name states session, project, zone, state and its number up to nine`, `> an unknown employee reads as unknown`, `> group targets name their count`, `> every zone, column and badge has a word`.
12. Memory (AD-62 as amended, D-184): `memory.ts` keeps `SceneMemory` in a plain module variable; `load(firstBoard)` reads `localStorage["officina.scene.v2"]` once, ignores `officina.scene.v1`, discards a memory of another floor and prunes it against the first board; `commit(next)` writes storage only when the persisted part changed; last places and sticky spots stay in memory only. Proof: Vitest `app/src/lib/scene/memory.test.ts > load prunes offices and desks without a live card on the first board`, `> load discards a memory of another floor`, `> the v1 key is ignored`, `> commit writes only when the persisted assignment changes`, `> an unreadable or foreign value starts empty`, `> a storage that throws keeps the layout working`.
13. Fixtures before Vitest (D-101, D-181): the CI `windows` job runs "e2e fixtures from the real CLI" before `pnpm test`. Proof: the CI `windows` job.

## Tasks

- [ ] `geometry.ts` from the prototype v2's constants; `geometry.test.ts`; `tokens.test.ts` first case (AC 1, 4)
- [ ] `places.ts`, `paths.ts` and their tests (AC 2, 3)
- [ ] `layout.ts`: types, `layoutScene`, `nearestTarget` (AC 5-10)
- [ ] `text.ts` (AC 11); `memory.ts` (AC 12)
- [ ] `layout.test.ts` with the arrow vectors, `text.test.ts`, `memory.test.ts` (AC 5-12)
- [ ] `.github/workflows/ci.yml`: fixture step before `pnpm test` (AC 13)

## Dev Notes

- Requirements: FR-1, FR-2, FR-3 (only state changes move), FR-6. Decisions: AD-60, AD-62, AD-63 as amended, AD-167, AD-169, AD-170. Epic decisions: D-99, D-101 to D-104, D-107, D-181, D-184, D-187, D-462 to D-464, D-468, D-471, D-482, D-484.
- Dependencies: stories 4.2 (`floor_projects`, fixtures) and 4.3 (`Card.scene`).
- Port from the prototype v2's `scene.js` (`epic-4-decisions.md` says where): its geometry section, its navigation graph (`addNode`, `link`, the rooms' blocks, the corridor chaining), `shortestPath` and `smooth`. Its room list is fixed at four offices; generalise the office cells to 5 and 6 columns as AD-169 states. Its spot pitches fail the target property (D-471): expect to spread the queue (about 1.4 tiles) and to drop lounge seats; record the final capacities in Completion Notes, since D-484's group texts depend on them.
- The layout names places, never motion: getting a figure there is the mover's (story 4.6). A changed place is the only thing that makes a figure move, so AC 7's last case is the "only state changes move" guarantee.
- AD-13 holds: the layout reads `card.scene`, never `card.column`, for the zone; it reads `columns.needs_you` for board order and numbers.
- Staff nodes and the consult spot exist for v0.5 (prd-v0.5.md section 9) and are never on a v0.4 route.
- Relative times are never drawn: `text.ts` imports nothing from `lib/time.ts`.

## Testing

- Vitest: `app/src/lib/scene/{geometry,places,paths,layout,text,memory}.test.ts`, `app/src/tokens.test.ts`, on the fixture boards and synthetic full occupancy.
- CI: the `windows` job's step order.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created as story 4.3
- 2026-10-28 reworked after review (F2, F6, F10, F13, F17, F23, F25, F33, F36)
- 2026-10-28 renumbered 4.5 and rewritten for the scene v2 (D-460): the prototype v2's floor, offices growing by column, places and the navigation graph, zones from core, approvals in the queue, unknown in place; shelf, cabinet, lamp and dials withdrawn
