// The floor's geometry (AD-169): the prototype v2's projection and
// constants, the cells of the rooms, the canvas, the scale bounds and every
// target rectangle. This module is the only copy: places, the layout, the
// drawing and every test import these values, never retype them.
import type { Zone } from "../generated/Zone";

/** Half a tile's screen width, in logical px (tile 72 x 36, 2:1 dimetric). */
export const HW = 36;
/** Half a tile's screen height, in logical px. */
export const HH = 18;
/** Logical px per z unit. */
export const ZS = 30;
/** Side of a room cell, in tiles. */
export const R = 4.4;
/** Width of a corridor between cells, in tiles. */
export const G = 1.1;
/** Margin of the floor around the cells, in tiles. */
export const M = 0.5;
/** Wall height, in z units. */
export const WALL = 3.4;
/** Floor slab thickness, in z units. */
export const SLAB = 0.45;
/** Partition height, in z units. */
export const PART = 0.42;
/** Partition thickness, in tiles. */
export const PT = 0.14;
/** Canvas padding around the floor, in logical px. */
export const PAD = { top: 30, left: 14, right: 14, bottom: 14 } as const;
/** Rows of cells: the back row (0) and the front row (1). */
export const ROWS = 2;
/** The corridor's centre line, between the two rows. */
export const CORR_Y = M + R + G / 2;

export type Cell = readonly [column: number, row: number];
export interface Point {
  x: number;
  y: number;
}
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The zones that own one room each, in columns 0 and 1. */
export type SharedZone = Exclude<Zone, "office" | "unknown">;
/** What a room can be: a shared zone's room, or an office. */
export type RoomKind = SharedZone | "office";

export const SHARED_CELLS: Record<SharedZone, Cell> = {
  reception: [0, 0],
  decision: [1, 0],
  lounge: [0, 1],
  alert: [1, 1],
};
/** Columns 0 and 1 hold the shared zones; offices start after them. */
export const SHARED_COLUMNS = 2;
/** The base floor's column count, and the most the floor grows to (D-519). */
export const BASE_COLUMNS = 4;
export const MAX_COLUMNS = 6;

/** Every column count the floor can have, smallest first. */
export const COLUMN_COUNTS: readonly number[] = Array.from(
  { length: MAX_COLUMNS - BASE_COLUMNS + 1 },
  (_, i) => BASE_COLUMNS + i,
);

/** Office cells in slot order: the base floor's back row then front row,
 *  then each added column's back then front cell (AD-169). */
export const OFFICE_CELLS: readonly Cell[] = (() => {
  const cells: Cell[] = [];
  for (let row = 0; row < ROWS; row++) {
    for (let column = SHARED_COLUMNS; column < BASE_COLUMNS; column++) cells.push([column, row]);
  }
  for (let column = BASE_COLUMNS; column < MAX_COLUMNS; column++) {
    for (let row = 0; row < ROWS; row++) cells.push([column, row]);
  }
  return cells;
})();

/** How many offices a floor of `columns` columns holds. */
export function officesIn(columns: number): number {
  return (columns - SHARED_COLUMNS) * ROWS;
}

/** The most offices the floor holds; beyond, the last becomes More offices. */
export const MAX_OFFICES = officesIn(MAX_COLUMNS);

/** The office cells of a floor of `columns` columns, in slot order. */
export function officeCells(columns: number): readonly Cell[] {
  return OFFICE_CELLS.slice(0, officesIn(columns));
}

/** The smallest column count whose offices hold `officeCount` projects. */
export function columnsFor(officeCount: number): number {
  return COLUMN_COUNTS.find((columns) => officesIn(columns) >= officeCount) ?? MAX_COLUMNS;
}

/** Floor extent along gx for `columns` columns, in tiles. */
export function floorWidth(columns: number): number {
  return 2 * M + columns * R + (columns - 1) * G;
}
/** Floor extent along gy, in tiles: it depends on the rows only. */
export const FLOOR_DEPTH = 2 * M + ROWS * R + (ROWS - 1) * G;

/** The screen point of grid (0, 0, 0). It depends on the row count only,
 *  so adding a column moves no existing room. */
export function floorOrigin(_columns: number): Point {
  return { x: PAD.left + FLOOR_DEPTH * HW, y: PAD.top + WALL * ZS };
}

export type Projector = (gx: number, gy: number, z?: number) => Point;

/** The dimetric projection of a floor of `columns` columns. */
export function projector(columns: number): Projector {
  const origin = floorOrigin(columns);
  return (gx, gy, z = 0) => ({ x: origin.x + (gx - gy) * HW, y: origin.y + (gx + gy) * HH - z * ZS });
}

/** The logical size of the canvas for `columns` columns. */
export function canvas(columns: number): { w: number; h: number } {
  const span = floorWidth(columns) + FLOOR_DEPTH;
  return {
    w: Math.round(span * HW + PAD.left + PAD.right),
    h: Math.round(PAD.top + WALL * ZS + span * HH + SLAB * ZS + PAD.bottom),
  };
}

/** A room's origin on the grid: its back corner. */
export function roomOrigin(cell: Cell): { gx: number; gy: number } {
  return { gx: M + cell[0] * (R + G), gy: M + cell[1] * (R + G) };
}

/** A room's four floor corners on screen: back, right, front, left. */
export function roomCorners(columns: number, cell: Cell): Point[] {
  const p = projector(columns);
  const { gx, gy } = roomOrigin(cell);
  return [p(gx, gy), p(gx + R, gy), p(gx + R, gy + R), p(gx, gy + R)];
}

// ------------------------------------------------------------ scale

/** The smallest target side in CSS px, equal to app.css's `--target-min`. */
export const TARGET_CSS_MIN = 44;
/** A figure's target: 48 x 48 logical px, from `rise` px above its floor point. */
export const FIGURE_TARGET = { w: 48, h: 48, rise: 56 } as const;
/** The scale at which a figure's target is `TARGET_CSS_MIN` CSS px. */
export const MIN_SCALE = TARGET_CSS_MIN / FIGURE_TARGET.w;
/** How many depth bands layer 3 is split into, each its own `<svg>` and so
 *  its own paint layer (story 4.16, D-550): a walk repaints the band it walks in
 *  and reuses the others' recorded paint. Eight took a walk frame's paint
 *  from about 1.4 to 0.4 ms on the eight-project floor; 24 gained little
 *  more. */
export const DEPTH_BANDS = 8;
/** The largest scale the scene is drawn at, so a figure stays a figure. */
export const MAX_SCALE = 1.5;
/** The smallest target side in logical px (D-104). The epsilon absorbs the
 *  rounding of the division, which is exact in real numbers. */
export const TARGET_MIN = Math.ceil(TARGET_CSS_MIN / MIN_SCALE - 1e-9);

/** Steps per unit the fitted scale is rounded down to, so the scene never
 *  overflows its area by a rounding error. */
export const SCALE_STEPS = 10_000;

/** The scene's scale in a sessions area of `area` CSS px (AD-61): the
 *  canvas fitted to the area, then held within `MIN_SCALE` and
 *  `MAX_SCALE`; below the minimum area the area scrolls. */
export function fitScale(area: { w: number; h: number }, size: { w: number; h: number }): number {
  const fitted = Math.floor(Math.min(area.w / size.w, area.h / size.h) * SCALE_STEPS) / SCALE_STEPS;
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, fitted));
}

// ------------------------------------------------------------ plaques

/** A plaque: a pill whose bottom sits `lift` px above its anchor, capped
 *  at `cap` px wide with CSS ellipsis (AD-61). */
export const PLAQUE = { lift: 4, h: 25, cap: 140 } as const;
/** Where each room's plaque hangs along its left partition, in tiles. */
export const PLAQUE_ALONG: Record<RoomKind, number> = {
  reception: 3.3,
  decision: 2.7,
  lounge: 3.1,
  alert: 1.6,
  office: 0.8,
};
/** The "high priority" pill, stacked above its office's plaque: beside it,
 *  as in the prototype, it met the desk targets of the office below. */
export const PRIORITY_PILL = { gap: 3, cap: 110, h: 25 } as const;
/** The alert room's beacon (AD-175): its post at room-local (`lx`, `ly`)
 *  in tiles, its lamp `z` units up, and its glow, an ellipse of `rx` by
 *  `ry` logical px centred on the lamp. The post is layer 3's, the glow
 *  layer 2's: both read this one point. */
export const BEACON = { lx: 0.5, ly: 0.5, z: 2.0, glow: { rx: 34, ry: 26 } } as const;

/** The beacon's lamp on screen. */
export function beaconPoint(columns: number): Point {
  const { gx, gy } = roomOrigin(SHARED_CELLS.alert);
  return projector(columns)(gx + BEACON.lx, gy + BEACON.ly, BEACON.z);
}

/** The "Entrance and exit" sign above the door, at grid (0, gy, z). */
export const ENTRANCE_SIGN = { gy: M + 1.6, z: 2.75 } as const;

/** The point a room's plaque is centred on, its bottom edge. */
export function plaqueAnchor(columns: number, cell: Cell, kind: RoomKind): Point {
  const { gx, gy } = roomOrigin(cell);
  const p = projector(columns)(gx, gy + PLAQUE_ALONG[kind], PART);
  return { x: p.x, y: p.y - PLAQUE.lift };
}

/** A room's plaque at its cap width. */
export function plaqueRect(columns: number, cell: Cell, kind: RoomKind): Rect {
  const a = plaqueAnchor(columns, cell, kind);
  return { x: a.x - PLAQUE.cap / 2, y: a.y - PLAQUE.h, w: PLAQUE.cap, h: PLAQUE.h };
}

/** An office's "high priority" pill at its cap width. */
export function priorityPillRect(columns: number, cell: Cell): Rect {
  const a = plaqueAnchor(columns, cell, "office");
  const bottom = a.y - PLAQUE.h - PRIORITY_PILL.gap;
  return { x: a.x - PRIORITY_PILL.cap / 2, y: bottom - PRIORITY_PILL.h, w: PRIORITY_PILL.cap, h: PRIORITY_PILL.h };
}

/** The entrance sign at the plaque cap width. */
export function entranceSignRect(columns: number): Rect {
  const p = projector(columns)(0, ENTRANCE_SIGN.gy, ENTRANCE_SIGN.z);
  return { x: p.x - PLAQUE.cap / 2, y: p.y - PLAQUE.h, w: PLAQUE.cap, h: PLAQUE.h };
}

// ------------------------------------------------------------ targets

/** A group target pinned to its plaque's left end: an office's "+n" chip,
 *  and the overflow of reception, the lounge and the alert room (D-104).
 *  The left end, not D-104's right end: at the right end an office's chip
 *  met the chip spot of the office to its right, and the alert room's met
 *  the queue in the corridor. */
export const PLAQUE_GROUP_TARGET = { w: 48, h: 48 } as const;
/** The register in the decision office's corner of the canvas: a header
 *  and one row per shown session, then its overflow target (D-103). */
export const REGISTER = { x: PAD.left, y: 6, w: 156, head: 24, rowH: 19, rows: 5 } as const;
export const REGISTER_OVERFLOW_TARGET = { w: 116, h: 48 } as const;

/** A figure's target at the floor point `at` (screen px). */
export function figureTarget(at: Point): Rect {
  return { x: at.x - FIGURE_TARGET.w / 2, y: at.y - FIGURE_TARGET.rise, w: FIGURE_TARGET.w, h: FIGURE_TARGET.h };
}

/** A group target left and bottom aligned with a plaque's cap rectangle. */
function plaqueGroupTarget(plaque: Rect): Rect {
  return {
    x: plaque.x,
    y: plaque.y + plaque.h - PLAQUE_GROUP_TARGET.h,
    w: PLAQUE_GROUP_TARGET.w,
    h: PLAQUE_GROUP_TARGET.h,
  };
}

/** An office's "+n" chip. */
export function chipTarget(columns: number, cell: Cell): Rect {
  return plaqueGroupTarget(plaqueRect(columns, cell, "office"));
}

/** The group of a shared zone beyond its spots (D-484): the register's
 *  overflow for the decision office, the plaque's "+n" for the others. */
export function zoneGroupTarget(columns: number, zone: SharedZone): Rect {
  if (zone === "decision") {
    return {
      x: REGISTER.x,
      y: REGISTER.y + REGISTER.head + REGISTER.rows * REGISTER.rowH,
      w: REGISTER_OVERFLOW_TARGET.w,
      h: REGISTER_OVERFLOW_TARGET.h,
    };
  }
  return plaqueGroupTarget(plaqueRect(columns, SHARED_CELLS[zone], zone));
}

/** More offices: the rectangle inscribed in its cell, centred, half the
 *  cell's screen width and height (D-104). */
export function moreOfficesTarget(columns: number, cell: Cell): Rect {
  const { gx, gy } = roomOrigin(cell);
  const centre = projector(columns)(gx + R / 2, gy + R / 2);
  const w = R * HW;
  const h = R * HH;
  return { x: centre.x - w / 2, y: centre.y - h / 2, w, h };
}

/** True when two rectangles share interior area; touching edges do not. */
export function intersects(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** A rectangle's centre. */
export function centreOf(r: Rect): Point {
  return { x: r.x + r.w / 2, y: r.y + r.h / 2 };
}
