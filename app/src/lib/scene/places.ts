// The spots of every room (AD-170 as amended, D-503), built from
// `geometry.ts`. Room-local coordinates are the prototype v2's starting
// values, spread or removed where two figure targets would meet (D-471):
// the geometry property test arbitrates every change here. The navigation
// graph that joins the spots is story 4.10's.
import {
  CORR_Y,
  OFFICE_CELLS,
  SHARED_CELLS,
  officesIn,
  roomOrigin,
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

/** The decider's desk and chair, drawn empty in v0.4 (the author is the
 *  decider): furniture, never a spot (D-503). Room-local tiles. */
export const DECIDER = {
  chair: { lx: 2.4, ly: 1.9 },
  desk: { lx: 1.4, ly: 0.82, w: 2.0, d: 0.58 },
} as const;
