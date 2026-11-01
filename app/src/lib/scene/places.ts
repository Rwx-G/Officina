// The spots of every room (AD-170 as amended, D-503), built from
// `geometry.ts`. Room-local coordinates are the prototype v2's starting
// values, spread or removed where two figure targets would meet (D-471):
// the geometry property test arbitrates every change here. The navigation
// graph that joins them (story 4.10) is the prototype v2's, linked to these
// spots: doors, halls and aisles inside each room, one corridor node per
// door on the centre line.
import {
  COLUMN_COUNTS,
  CORR_Y,
  DECIDER,
  HH,
  HW,
  MAX_COLUMNS,
  OFFICE_CELLS,
  ROWS,
  SHARED_CELLS,
  canvas,
  chipTarget,
  deciderScreens,
  entranceSignRect,
  figureTarget,
  intersects,
  moreOfficesTarget,
  officeCells,
  officesIn,
  plaqueRect,
  priorityPillRect,
  projector,
  roomOrigin,
  whiteboardPlane,
  zoneGroupTarget,
  type Rect,
  type Cell,
  type RoomKind,
  type SharedZone,
} from "./geometry";

export type Posture = "standing" | "seated";
/** Which way a figure at rest faces: toward the author, or away. */
export type Facing = "front" | "back";

export interface Spot {
  /** Unique on the floor, stable across runs. */
  id: string;
  /** The room the spot belongs to. */
  room: RoomKind;
  /** The office slot, for an office's spots; `null` for a shared room. */
  slot: number | null;
  /** Floor point on the grid, in tiles. */
  gx: number;
  gy: number;
  posture: Posture;
  facing: Facing;
  /** Where a seated figure rests relative to its floor point, in tiles. */
  sit: { dx: number; dy: number } | null;
  /** A queue place outside its room, on the corridor's centre line. */
  corridor: boolean;
}

type Local = readonly [lx: number, ly: number];

interface LocalSpot {
  at: Local;
  posture: Posture;
  facing: Facing;
  sit?: Local;
}

function spot(id: string, room: RoomKind, slot: number | null, cell: Cell, s: LocalSpot): Spot {
  const origin = roomOrigin(cell);
  return {
    id,
    room,
    slot,
    gx: origin.gx + s.at[0],
    gy: origin.gy + s.at[1],
    posture: s.posture,
    facing: s.facing,
    sit: s.sit ? { dx: s.sit[0], dy: s.sit[1] } : null,
    corridor: Math.abs(origin.gy + s.at[1] - CORR_Y) < 1e-9,
  };
}

const standFront = (at: Local): LocalSpot => ({ at, posture: "standing", facing: "front" });
const standBack = (at: Local): LocalSpot => ({ at, posture: "standing", facing: "back" });
const seatBack = (at: Local): LocalSpot => ({ at, posture: "seated", facing: "back" });

// Two figure targets are disjoint when their floor points are 48 px apart
// on screen in x or in y: 1.34 tiles along one grid axis, or 2.67 along
// the sum of both. The prototype's pitches (0.7 to 1.1 tiles) were not.

/** Reception: visitors stand in front of the counter and at its ends,
 *  backs turned to the author. The left end's spot keeps clear of the
 *  queue's corridor places, which rise over the reception's front. */
const RECEPTION: readonly LocalSpot[] = [
  standBack([3.2, 2.35]),
  standBack([2.55, 3.05]),
  standBack([3.9, 1.7]),
  standBack([1.15, 1.75]),
];

/** The decision office's queue, first place first: inside in front of the
 *  rug, then in the corridor leading back toward the entrance, its first
 *  corridor place clear of the office's plaque. The prototype's fourth
 *  corridor place met the lounge's back sofa. */
const QUEUE_CORRIDOR_LY = CORR_Y - roomOrigin(SHARED_CELLS.decision).gy;
const QUEUE: readonly LocalSpot[] = [
  standFront([3.7, 4.0]),
  standFront([2.3, 4.0]),
  standFront([0.9, 4.0]),
  standFront([-0.4, QUEUE_CORRIDOR_LY]),
  standFront([-1.8, QUEUE_CORRIDOR_LY]),
  standFront([-3.2, QUEUE_CORRIDOR_LY]),
];

/** The lounge: the back sofa's end seat and one armchair, each with the
 *  offset a figure slides in by, and one standing place by the coffee
 *  corner. The side sofa sits under the lounge's plaque, so its seats go;
 *  the back sofa's other seats met the queue's last corridor place or
 *  this seat, and the second armchair and standing place met the kept
 *  ones. The queue's place was preferred: a waiting session is the one
 *  the author acts on. */
const LOUNGE: readonly LocalSpot[] = [
  { at: [0.85, 1.2], posture: "seated", facing: "front", sit: [0, -0.6] },
  { at: [3.55, 2.95], posture: "seated", facing: "front", sit: [0, -0.48] },
  standFront([2.1, 4.05]),
];

/** The alert room: a row in front of the beacon, and one place before it,
 *  every target below the room's plaque and left of the next office's chip. */
const ALERT: readonly LocalSpot[] = [
  standFront([1.7, 2.5]),
  standFront([2.4, 1.8]),
  standFront([1.0, 3.2]),
  standFront([3.1, 1.1]),
  standFront([3.45, 3.45]),
];

/** An office's chairs, desk order; a seated worker turns its back. */
const CHAIRS: readonly LocalSpot[] = [
  seatBack([1.25, 1.9]),
  seatBack([3.25, 1.9]),
  seatBack([1.25, 3.7]),
  seatBack([3.25, 3.7]),
];

/** Where an office's chip list stands: its left corner, clear of the desks.
 *  The right corner's place met the chip of the office to the right. */
const CHIP_SPOTS: readonly LocalSpot[] = [standFront([0.3, 4.1])];

const SHARED_SPOTS: Record<SharedZone, readonly LocalSpot[]> = {
  reception: RECEPTION,
  decision: QUEUE,
  lounge: LOUNGE,
  alert: ALERT,
};

/** A shared zone's spots in their order: the queue's and the alert room's
 *  follow board order, reception's and the lounge's the first free one. */
export function sharedSpots(zone: SharedZone): Spot[] {
  return SHARED_SPOTS[zone].map((s, i) => spot(`${zone}:${i}`, zone, null, SHARED_CELLS[zone], s));
}

/** The office cell of `slot`. */
export function officeCell(slot: number): Cell {
  const cell = OFFICE_CELLS[slot];
  if (!cell) throw new RangeError(`no office slot ${slot}`);
  return cell;
}

/** An office's desks (its chairs, in desk order) and chip spots. */
export function officeSpots(slot: number): { desks: Spot[]; chips: Spot[] } {
  const cell = officeCell(slot);
  return {
    desks: CHAIRS.map((s, d) => spot(`office${slot}:desk${d}`, "office", slot, cell, s)),
    chips: CHIP_SPOTS.map((s, i) => spot(`office${slot}:chip${i}`, "office", slot, cell, s)),
  };
}

/** How many sessions each shared zone places on spots, and per office. */
export const CAPACITY = {
  reception: RECEPTION.length,
  decision: QUEUE.length,
  lounge: LOUNGE.length,
  alert: ALERT.length,
  desks: CHAIRS.length,
  chips: CHIP_SPOTS.length,
} as const;

/** Every spot of a floor of `columns` columns. */
export function allSpots(columns: number): Spot[] {
  const out: Spot[] = [];
  for (const zone of Object.keys(SHARED_CELLS) as SharedZone[]) out.push(...sharedSpots(zone));
  for (let slot = 0; slot < officesIn(columns); slot++) {
    const { desks, chips } = officeSpots(slot);
    out.push(...desks, ...chips);
  }
  return out;
}

/** Where a figure at `s` rests: its floor point, plus the slide-in offset
 *  of a seat. Its target and its drawing sit here. */
export function restPoint(s: Spot): { gx: number; gy: number } {
  return s.sit ? { gx: s.gx + s.sit.dx, gy: s.gy + s.sit.dy } : { gx: s.gx, gy: s.gy };
}

// ------------------------------------------------------------ waiting places

/** The zones where figures wait their turn: their places are not drawn on
 *  the floor, and each figure stands a little off its spot, so a queue
 *  reads as people waiting rather than a grid (story 4.20, D-1046). */
export const WAITING_ZONES: readonly SharedZone[] = ["decision", "alert"];

/** How far off its spot a waiting figure may stand, in screen px: the
 *  candidates each spot keeps those of that are clear of every other
 *  place's target (`waitingOffsets`). */
const WAIT_STEPS = { x: [-6, -3, 0, 3, 6], y: [-10, -6, -3, 0, 3, 6, 10] } as const;
const WAIT_CANDIDATES: readonly { x: number; y: number }[] = WAIT_STEPS.y.flatMap((y) => WAIT_STEPS.x.map((x) => ({ x, y })));

/** The part of a standing figure above its target's top, in px, with a
 *  margin: `rig.ts` draws the head at 52.5 px, its cap 11.4 px above that
 *  and its brim 13.4 px to the side, against the target's 56 px rise. */
const HEAD_ABOVE_TARGET = { h: 12, halfWidth: 15 } as const;

export interface Offset {
  dgx: number;
  dgy: number;
}

/** A screen offset in tiles: the projection's inverse, without height. */
function toTiles(p: { x: number; y: number }): Offset {
  return { dgx: p.x / (2 * HW) + p.y / (2 * HH), dgy: p.y / (2 * HH) - p.x / (2 * HW) };
}

let offsetsBySpot: Map<string, Offset[]> | null = null;

/** What stands fixed on the floor, which no figure target may meet: the
 *  plaques and signs, the group targets, the decider's screens and the
 *  whiteboard (as their bounding boxes). Rooms never move when columns are
 *  added, so the largest floor holds every one. */
function fixedRects(): Rect[] {
  const columns = MAX_COLUMNS;
  const cells = officeCells(columns);
  const boxOf = (corners: readonly { x: number; y: number }[]): Rect => {
    const xs = corners.map((c) => c.x);
    const ys = corners.map((c) => c.y);
    return { x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) };
  };
  const shared = Object.keys(SHARED_CELLS) as SharedZone[];
  return [
    ...shared.map((zone) => plaqueRect(columns, SHARED_CELLS[zone], zone)),
    ...shared.map((zone) => zoneGroupTarget(columns, zone)),
    ...cells.flatMap((cell) => [plaqueRect(columns, cell, "office"), priorityPillRect(columns, cell), chipTarget(columns, cell), moreOfficesTarget(columns, cell)]),
    entranceSignRect(columns),
    ...deciderScreens(columns).map((plane) => boxOf(plane.corners)),
    boxOf(whiteboardPlane(columns).corners),
  ];
}

/** The offsets each waiting spot may take, the spot itself first: built
 *  once from every place of the largest floor, so that no figure target
 *  meets another, or anything fixed, or leaves the smallest canvas,
 *  whichever offsets two figures take. First each candidate must clear
 *  the fixed things and every other place's spot; then, while two
 *  candidates of two places meet, the farther from its spot is dropped.
 *  The spot itself always stays, as the places meet nothing
 *  (geometry.test.ts). */
function waitingOffsetTable(): Map<string, Offset[]> {
  if (offsetsBySpot) return offsetsBySpot;
  const p = projector(MAX_COLUMNS);
  const { chair, counter } = staffSpots();
  const places = [...allSpots(MAX_COLUMNS), chair, counter];
  const zero = { x: 0, y: 0 };
  const isZero = (o: { x: number; y: number }) => o.x === 0 && o.y === 0;
  const rect = (s: Spot, o: { x: number; y: number }) => {
    const rest = restPoint(s);
    const at = p(rest.gx, rest.gy);
    return figureTarget({ x: at.x + o.x, y: at.y + o.y });
  };
  const fixed = fixedRects();
  const room = canvas(COLUMN_COUNTS[0]!);
  const clear = (s: Spot, o: { x: number; y: number }) => {
    const r = rect(s, o);
    if (r.x < 0 || r.y < 0 || r.x + r.w > room.w || r.y + r.h > room.h) return false;
    // The head and its cap rise above the target: a plaque never covers
    // them (readability.spec.ts), so the fixed things are kept off both.
    const head = { x: r.x + r.w / 2 - HEAD_ABOVE_TARGET.halfWidth, y: r.y - HEAD_ABOVE_TARGET.h, w: 2 * HEAD_ABOVE_TARGET.halfWidth, h: HEAD_ABOVE_TARGET.h };
    return !fixed.some((f) => intersects(r, f) || intersects(head, f)) && places.every((other) => other === s || !intersects(r, rect(other, zero)));
  };
  // The waiting zones' employee spots; the orchestrator's chair, in the
  // decision office, is furniture and stays where it is.
  const waitingIds = new Set(WAITING_ZONES.flatMap((zone) => sharedSpots(zone).map((s) => s.id)));
  const candidates = new Map(
    places.map((s) => {
      const waiting = waitingIds.has(s.id);
      return [s.id, waiting ? WAIT_CANDIDATES.filter((o) => isZero(o) || clear(s, o)) : [zero]];
    }),
  );
  const size = (o: { x: number; y: number }) => Math.hypot(o.x, o.y);
  const moving = places.filter((s) => candidates.get(s.id)!.length > 1);
  for (let changed = true; changed; ) {
    changed = false;
    for (const a of moving) {
      for (const b of moving) {
        if (a.id >= b.id) continue;
        const ca = candidates.get(a.id)!;
        const cb = candidates.get(b.id)!;
        for (const oa of ca.filter((o) => !isZero(o))) {
          const ob = cb.find((o) => !isZero(o) && intersects(rect(a, oa), rect(b, o)));
          if (!ob) continue;
          const [list, drop] = size(oa) >= size(ob) ? [ca, oa] : [cb, ob];
          list.splice(list.indexOf(drop), 1);
          changed = true;
          break;
        }
      }
    }
  }
  offsetsBySpot = new Map(
    [...candidates].map(([id, list]) => [id, [...list].sort((m, n) => size(m) - size(n)).map(toTiles)]),
  );
  return offsetsBySpot;
}

/** The offsets spot `id` may take, in tiles, the spot itself first. */
export function waitingOffsets(id: string): readonly Offset[] {
  return waitingOffsetTable().get(id) ?? [{ dgx: 0, dgy: 0 }];
}

/** FNV-1a 32-bit: a stable index from a session key, nothing secret. */
function hashOf(text: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/** Where the figure of session `key` stands off spot `s`: chosen from the
 *  key, so it is the same on every board and never jumps; none outside the
 *  waiting zones. */
export function waitingOffset(s: Pick<Spot, "id">, key: string): Offset {
  const offsets = waitingOffsets(s.id);
  return offsets[hashOf(key) % offsets.length]!;
}

/** Where the figure of session `key` rests at `s`: its rest point, plus
 *  its offset in a waiting zone. Its target, its drawing and its walk's
 *  end sit here. */
export function restPointFor(s: Spot, key: string): { gx: number; gy: number } {
  const rest = restPoint(s);
  const off = waitingOffset(s, key);
  return { gx: rest.gx + off.dgx, gy: rest.gy + off.dgy };
}

/** The decider's desk and chair: the orchestrator's while it is live
 *  (AD-179), never an employee's spot (D-503). */
export { DECIDER };

/** Reception's counter, room-local tiles: its back corner, its width
 *  along gx and its depth along gy. */
export const COUNTER = { lx: 1.5, ly: 1.25, w: 2.0, d: 0.55 } as const;

// ------------------------------------------------------------ staff

/** The orchestrator's two places (D-568): the decider's chair, where it
 *  sits, and the counter's staff side, where it briefs. Neither is ever
 *  an employee's spot. */
export const STAFF_CHAIR = "staff:chair";
export const STAFF_COUNTER = "staff:counter";

/** The staff chain, the prototype v2's (D-832): from the counter's staff
 *  side, out past reception's open side into the corridor between the
 *  two rooms, through the decision office's side door, along its back to
 *  the decider's chair. Each room-local to the room named. */
const STAFF_CHAIN: readonly { id: string; room: SharedZone; at: Local }[] = [
  { id: STAFF_COUNTER, room: "reception", at: [2.5, 0.8] },
  { id: "staff:reception-side", room: "reception", at: [4.95, 0.8] },
  { id: "staff:decision-door", room: "decision", at: [0.4, 1.0] },
  { id: "staff:decision-back", room: "decision", at: [0.8, 1.9] },
  { id: STAFF_CHAIR, room: "decision", at: [DECIDER.chair.lx, DECIDER.chair.ly] },
];

/** The orchestrator's chair, a seat facing its desk, and its place at the
 *  counter, facing the visitors. */
export function staffSpots(): { chair: Spot; counter: Spot } {
  const at = (id: string) => STAFF_CHAIN.find((n) => n.id === id)!;
  const chair = at(STAFF_CHAIR);
  const counter = at(STAFF_COUNTER);
  return {
    chair: spot(chair.id, chair.room, null, SHARED_CELLS[chair.room], seatBack(chair.at)),
    counter: spot(counter.id, counter.room, null, SHARED_CELLS[counter.room], standFront(counter.at)),
  };
}

/** Reception's visitor spot nearest the counter's centre: where a briefed
 *  employee stands (D-568). */
export function briefingSpot(): Spot {
  const { gx, gy } = roomOrigin(SHARED_CELLS.reception);
  const cx = gx + COUNTER.lx + COUNTER.w / 2;
  const cy = gy + COUNTER.ly + COUNTER.d / 2;
  const d = (s: Spot) => Math.hypot(s.gx - cx, s.gy - cy);
  return sharedSpots("reception").reduce((a, b) => (d(b) < d(a) ? b : a));
}

// ------------------------------------------------------------ navigation graph

/** A node of the navigation graph: a spot (its id is the spot's), or a
 *  door, hall, aisle, corridor or exit node. */
export interface NavNode {
  id: string;
  gx: number;
  gy: number;
  /** The room it belongs to (`reception`, `office3`, ...), `corridor` for
   *  a corridor node of its own, `outside` for the exit. */
  room: string;
  adj: string[];
  /** On the orchestrator's chain (D-568), which no employee walks. */
  staff: boolean;
}

export interface NavGraph {
  nodes: ReadonlyMap<string, NavNode>;
  /** Reception's exit node, outside the entrance door: a new session
   *  appears there and an ended one leaves there (D-483). */
  exit: string;
  /** Each room's door node on its corridor side, by room id. */
  doors: ReadonlyMap<string, string>;
}

/** The exit node's id. */
export const EXIT = "reception:exit";

/** A spot's room id: its shared zone, or `office<slot>`. */
export function roomOf(s: Pick<Spot, "room" | "slot">): string {
  return s.slot === null ? s.room : `office${s.slot}`;
}

/** A shared room's nodes besides its spots, room-local tiles: its door on
 *  the corridor side, then its halls, chained from the door. */
const SHARED_WAYS: Record<SharedZone, { door: Local; halls: readonly Local[] }> = {
  reception: { door: [2.5, 4.4], halls: [] },
  decision: { door: [2.6, 4.4], halls: [] },
  lounge: { door: [3.5, 0], halls: [[3.5, 0.7], [2.95, 1.3], [2.95, 2.95], [2.95, 4.05]] },
  alert: { door: [2.2, 0], halls: [[2.2, 0.75], [2.2, 2.4]] },
};
/** The entrance: the exit outside the door, the entry inside it, and the
 *  hub between the counter and the coat rack. */
const RECEPTION_EXIT: Local = [-0.95, 1.6];
const RECEPTION_ENTRY: Local = [0.35, 1.6];
const RECEPTION_HUB: Local = [1.0, 2.75];
/** An office's aisle between its two columns of desks, and its stops: by
 *  the back desks' chairs, then by the front ones'. */
const AISLE_LX = 2.25;
const AISLE_LY = [2.3, 4.05] as const;
/** A front-row office's door, on its back side, and the stop inside it. */
const FRONT_DOOR_LY = 0;
const FRONT_ENTRY_LY = 0.45;
/** A back-row office's door, on its front side. */
const BACK_DOOR_LY = 4.4;
/** Two corridor nodes closer than this along gx are one. */
const SAME_GX = 0.05;

/** The navigation graph of a floor of `columns` columns (AD-170 as
 *  amended, D-503, D-568: no consult spot; the staff chain joins nothing
 *  else, so no employee's route can take it), ported from the prototype
 *  v2 and linked to this module's spots. Pure: the same column count gives
 *  the same graph. */
export function navGraph(columns: number): NavGraph {
  const nodes = new Map<string, NavNode>();
  const doors = new Map<string, string>();
  const add = (id: string, room: string, gx: number, gy: number, staff = false): string => {
    nodes.set(id, { id, gx, gy, room, adj: [], staff });
    return id;
  };
  const local = (id: string, room: string, cell: Cell, [lx, ly]: Local): string => {
    const o = roomOrigin(cell);
    return add(id, room, o.gx + lx, o.gy + ly);
  };
  const link = (a: string, b: string): void => {
    nodes.get(a)!.adj.push(b);
    nodes.get(b)!.adj.push(a);
  };
  const chain = (ids: readonly string[]): void => ids.slice(1).forEach((id, i) => link(ids[i]!, id));
  const spotNode = (s: Spot): string => add(s.id, roomOf(s), s.gx, s.gy);
  const distance = (a: string, b: string): number => {
    const u = nodes.get(a)!;
    const v = nodes.get(b)!;
    return Math.hypot(u.gx - v.gx, u.gy - v.gy);
  };
  /** Links a spot to the nearest of its room's ways, the first on a tie. */
  const linkNearest = (spot: string, ways: readonly string[]): void => {
    link(spot, ways.reduce((a, b) => (distance(spot, b) < distance(spot, a) ? b : a)));
  };

  for (const zone of Object.keys(SHARED_CELLS) as SharedZone[]) {
    const cell = SHARED_CELLS[zone];
    const door = local(`${zone}:door`, zone, cell, SHARED_WAYS[zone].door);
    doors.set(zone, door);
    const halls = SHARED_WAYS[zone].halls.map((at, i) => local(`${zone}:hall${i}`, zone, cell, at));
    chain([door, ...halls]);
    const spots = sharedSpots(zone).filter((s) => !s.corridor).map(spotNode);
    if (zone === "reception") {
      const hub = local("reception:hub", zone, cell, RECEPTION_HUB);
      chain([local(EXIT, "outside", cell, RECEPTION_EXIT), local("reception:entry", zone, cell, RECEPTION_ENTRY), hub, door]);
      // Visitors reach the counter from the hub and from the door.
      for (const s of spots) {
        link(s, hub);
        link(s, door);
      }
    } else if (halls.length === 0) {
      for (const s of spots) link(s, door);
    } else {
      for (const s of spots) linkNearest(s, halls);
    }
  }

  for (let slot = 0; slot < officesIn(columns); slot++) {
    const cell = officeCell(slot);
    const room = `office${slot}`;
    const aisle = AISLE_LY.map((ly, i) => local(`${room}:aisle${i}`, room, cell, [AISLE_LX, ly]));
    let door: string;
    if (cell[1] === ROWS - 1) {
      door = local(`${room}:door`, room, cell, [AISLE_LX, FRONT_DOOR_LY]);
      chain([door, local(`${room}:entry`, room, cell, [AISLE_LX, FRONT_ENTRY_LY]), ...aisle]);
    } else {
      door = local(`${room}:door`, room, cell, [AISLE_LX, BACK_DOOR_LY]);
      chain([door, ...[...aisle].reverse()]);
    }
    doors.set(room, door);
    const { desks, chips } = officeSpots(slot);
    for (const s of [...desks, ...chips]) linkNearest(spotNode(s), aisle);
  }

  // The corridor: the queue's places on the centre line, and one node per
  // door that none of them stands in front of, chained in gx order.
  const corridor = sharedSpots("decision").filter((s) => s.corridor).map(spotNode);
  for (const [room, door] of doors) {
    const d = nodes.get(door)!;
    let c = corridor.find((id) => Math.abs(nodes.get(id)!.gx - d.gx) < SAME_GX);
    if (c === undefined) {
      c = add(`corridor:${room}`, "corridor", d.gx, CORR_Y);
      corridor.push(c);
    }
    link(door, c);
  }
  corridor.sort((a, b) => nodes.get(a)!.gx - nodes.get(b)!.gx);
  chain(corridor);

  chain(
    STAFF_CHAIN.map(({ id, room, at: [lx, ly] }) => {
      const o = roomOrigin(SHARED_CELLS[room]);
      return add(id, room, o.gx + lx, o.gy + ly, true);
    }),
  );

  return { nodes, exit: EXIT, doors };
}
