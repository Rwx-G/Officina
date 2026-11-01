// AD-169's geometry and AD-70's target property, built from `geometry.ts`
// and `places.ts` alone, at full occupancy for every column count.
import { describe, expect, it } from "vitest";
import {
  COLUMN_COUNTS,
  FLOOR_DEPTH,
  MAX_COLUMNS,
  MAX_OFFICES,
  MAX_SCALE,
  MIN_SCALE,
  OFFICE_CELLS,
  SHARED_CELLS,
  SLAB,
  TARGET_CSS_MIN,
  TARGET_MIN,
  canvas,
  chipTarget,
  columnsFor,
  deciderScreens,
  entranceSignRect,
  figureTarget,
  fitScale,
  floorWidth,
  intersects,
  moreOfficesTarget,
  officeCells,
  officesIn,
  plaqueRect,
  priorityPillRect,
  projector,
  roomCorners,
  whiteboardPlane,
  zoneGroupTarget,
  type Point,
  type Rect,
  type SharedZone,
} from "./geometry";
import { WAITING_ZONES, allSpots, officeCell, restPoint, staffSpots, waitingOffsets } from "./places";

/** True when the convex polygon `quad` and `rect` share interior area: no
 *  axis of either separates them. */
function quadMeetsRect(quad: readonly Point[], rect: Rect): boolean {
  const box: Point[] = [
    { x: rect.x, y: rect.y },
    { x: rect.x + rect.w, y: rect.y },
    { x: rect.x + rect.w, y: rect.y + rect.h },
    { x: rect.x, y: rect.y + rect.h },
  ];
  for (const shape of [quad, box]) {
    for (let i = 0; i < shape.length; i++) {
      const a = shape[i]!;
      const b = shape[(i + 1) % shape.length]!;
      const axis = { x: a.y - b.y, y: b.x - a.x };
      const span = (points: readonly Point[]) => points.map((q) => q.x * axis.x + q.y * axis.y);
      const [p, r] = [span(quad), span(box)];
      if (Math.max(...p) <= Math.min(...r) + 1e-9 || Math.max(...r) <= Math.min(...p) + 1e-9) return false;
    }
  }
  return true;
}

interface Owned {
  id: string;
  rect: Rect;
  /** The plaques this target may meet. */
  owns: string[];
  /** The spot it stands at, one of several offsets of a waiting spot
   *  (D-1046): two offsets of one spot are one figure, never two. */
  spot?: string;
}

const SHARED = Object.keys(SHARED_CELLS) as SharedZone[];

/** Every plaque of the floor at its cap width, by owner id. */
function plaques(columns: number): Owned[] {
  const out: Owned[] = SHARED.map((zone) => ({ id: zone, rect: plaqueRect(columns, SHARED_CELLS[zone], zone), owns: [] }));
  officeCells(columns).forEach((cell, slot) => {
    out.push({ id: `office${slot}`, rect: plaqueRect(columns, cell, "office"), owns: [] });
    out.push({ id: `office${slot}:priority`, rect: priorityPillRect(columns, cell), owns: [] });
  });
  out.push({ id: "entrance", rect: entranceSignRect(columns), owns: [] });
  return out;
}

/** Every target at full occupancy: a figure on every spot, every group,
 *  every chip, and, when `more`, More offices in the last slot. */
function fullOccupancy(columns: number, more: boolean): Owned[] {
  const p = projector(columns);
  const moreSlot = more ? officesIn(columns) - 1 : -1;
  const out: Owned[] = [];
  // A waiting spot at every offset a figure may take there (D-1046).
  for (const s of allSpots(columns)) {
    if (s.slot === moreSlot) continue;
    const rest = restPoint(s);
    waitingOffsets(s.id).forEach((o, i) => {
      out.push({ id: i === 0 ? s.id : `${s.id}@${i}`, rect: figureTarget(p(rest.gx + o.dgx, rest.gy + o.dgy)), owns: [], spot: s.id });
    });
  }
  // The orchestrator's figure, or its empty chair: one rectangle (D-567).
  const chair = restPoint(staffSpots().chair);
  out.push({ id: "orchestrator", rect: figureTarget(p(chair.gx, chair.gy)), owns: [] });
  for (const zone of SHARED) out.push({ id: `group:${zone}`, rect: zoneGroupTarget(columns, zone), owns: [zone] });
  officeCells(columns).forEach((cell, slot) => {
    const owns = [`office${slot}`, `office${slot}:priority`];
    if (slot === moreSlot) out.push({ id: "more", rect: moreOfficesTarget(columns, cell), owns });
    else out.push({ id: `chip${slot}`, rect: chipTarget(columns, cell), owns });
  });
  return out;
}

const FLOORS = [
  ...COLUMN_COUNTS.map((columns) => ({ columns, more: false })),
  { columns: MAX_COLUMNS, more: true },
];
const floorName = (f: { columns: number; more: boolean }) => `${f.columns} columns${f.more ? " with More offices" : ""}`;

describe("geometry", () => {
  it("the canvas follows the columns", () => {
    expect(COLUMN_COUNTS.length).toBeGreaterThan(1);
    for (let i = 1; i < COLUMN_COUNTS.length; i++) {
      const before = COLUMN_COUNTS[i - 1]!;
      const after = COLUMN_COUNTS[i]!;
      expect(canvas(after).w).toBeGreaterThan(canvas(before).w);
      expect(canvas(after).h).toBeGreaterThan(canvas(before).h);
      const cells = [...Object.values(SHARED_CELLS), ...officeCells(before)];
      for (const cell of cells) expect(roomCorners(after, cell), `${cell}`).toEqual(roomCorners(before, cell));
    }
  });

  it("each column count holds its offices and columnsFor picks the smallest", () => {
    expect(officesIn(MAX_COLUMNS)).toBe(MAX_OFFICES);
    expect(OFFICE_CELLS.length).toBe(MAX_OFFICES);
    for (let n = 0; n <= MAX_OFFICES + 2; n++) {
      const columns = columnsFor(n);
      expect(COLUMN_COUNTS).toContain(columns);
      if (n <= MAX_OFFICES) expect(officesIn(columns)).toBeGreaterThanOrEqual(n);
      const smaller = COLUMN_COUNTS.filter((c) => c < columns);
      for (const c of smaller) expect(officesIn(c)).toBeLessThan(n);
    }
    expect(columnsFor(MAX_OFFICES + 1)).toBe(MAX_COLUMNS);
  });

  it("an office cell never takes a shared zone's cell", () => {
    const shared = Object.values(SHARED_CELLS).map((c) => `${c}`);
    const offices = OFFICE_CELLS.map((c) => `${c}`);
    expect(new Set(offices).size).toBe(offices.length);
    for (const cell of offices) expect(shared).not.toContain(cell);
    for (let slot = 0; slot < MAX_OFFICES; slot++) expect(officeCell(slot)).toEqual(OFFICE_CELLS[slot]);
  });

  for (const floor of FLOORS) {
    describe(floorName(floor), () => {
      const targets = fullOccupancy(floor.columns, floor.more);

      it("no two targets intersect at full occupancy", () => {
        const meets: string[] = [];
        for (let i = 0; i < targets.length; i++) {
          for (let j = i + 1; j < targets.length; j++) {
            if (targets[i]!.spot !== undefined && targets[i]!.spot === targets[j]!.spot) continue;
            if (intersects(targets[i]!.rect, targets[j]!.rect)) meets.push(`${targets[i]!.id} x ${targets[j]!.id}`);
          }
        }
        expect(meets).toEqual([]);
      });

      it("no target meets a plaque it does not own", () => {
        const meets: string[] = [];
        for (const t of targets) {
          for (const p of plaques(floor.columns)) {
            if (!t.owns.includes(p.id) && intersects(t.rect, p.rect)) meets.push(`${t.id} x ${p.id}`);
          }
        }
        expect(meets).toEqual([]);
      });

      it("every target is 44 CSS px or more at the minimum scale", () => {
        for (const t of targets) {
          expect(Math.min(t.rect.w, t.rect.h), t.id).toBeGreaterThanOrEqual(TARGET_MIN);
          expect(Math.min(t.rect.w, t.rect.h) * MIN_SCALE, t.id).toBeGreaterThanOrEqual(TARGET_CSS_MIN - 1e-9);
        }
      });

      // Story 5.18 AC 5 (D-567): what the overlay writes on the screens
      // and the whiteboard covers no target and no plaque, so no focus
      // ring, name tag or plaque is ever under it.
      it("the screen and whiteboard boxes meet no target at any column count", () => {
        const surfaces = [...deciderScreens(floor.columns).map((plane, i) => ({ id: `screen${i}`, plane })), { id: "whiteboard", plane: whiteboardPlane(floor.columns) }];
        const meets: string[] = [];
        for (const { id, plane } of surfaces) {
          expect(plane.w * plane.h, id).toBeGreaterThan(0);
          for (const t of [...targets, ...plaques(floor.columns)]) if (quadMeetsRect(plane.corners, t.rect)) meets.push(`${id} x ${t.id}`);
        }
        expect(meets).toEqual([]);
        // The test sees a meeting where there is one.
        const left = surfaces[0]!.plane;
        expect(quadMeetsRect(left.corners, { ...left.rect, w: 4, h: 4, x: left.corners[0]!.x, y: left.corners[0]!.y })).toBe(true);
      });

      it("every target lies inside the canvas", () => {
        const { w, h } = canvas(floor.columns);
        for (const t of targets) {
          expect(t.rect.x, t.id).toBeGreaterThanOrEqual(0);
          expect(t.rect.y, t.id).toBeGreaterThanOrEqual(0);
          expect(t.rect.x + t.rect.w, t.id).toBeLessThanOrEqual(w);
          expect(t.rect.y + t.rect.h, t.id).toBeLessThanOrEqual(h);
        }
      });
    });
  }
});

// Story 4.20 item 7 (D-1046): a waiting figure stands a little off its
// spot, chosen from its key; the places are kept apart whatever it takes.
describe("waiting places", () => {
  it("only the waiting zones' spots move, each keeping the spot itself and some room to vary", () => {
    for (const s of allSpots(MAX_COLUMNS)) {
      const offsets = waitingOffsets(s.id);
      expect(offsets[0], s.id).toEqual({ dgx: 0, dgy: 0 });
      const waiting = s.slot === null && (WAITING_ZONES as readonly string[]).includes(s.room);
      if (!waiting) expect(offsets, s.id).toHaveLength(1);
    }
    // The variation is real, not a no-op: most of each waiting zone's spots
    // vary. A spot hemmed in by its neighbours and its room's plaque (the
    // alert room's first two places) keeps its place.
    for (const zone of WAITING_ZONES) {
      const spots = allSpots(MAX_COLUMNS).filter((s) => s.slot === null && s.room === zone);
      const varying = spots.filter((s) => waitingOffsets(s.id).length > 1);
      expect(varying.length * 2, zone).toBeGreaterThan(spots.length);
    }
  });
});

describe("scale", () => {
  it("fits the canvas to the area, within MIN_SCALE and MAX_SCALE, never over the area", () => {
    for (const columns of COLUMN_COUNTS) {
      const size = canvas(columns);
      // At the minimum area and below it, the minimum; far above, the maximum.
      expect(fitScale({ w: size.w * MIN_SCALE, h: size.h * MIN_SCALE }, size)).toBe(MIN_SCALE);
      expect(fitScale({ w: 100, h: 100 }, size)).toBe(MIN_SCALE);
      expect(fitScale({ w: size.w * 10, h: size.h * 10 }, size)).toBe(MAX_SCALE);
      // In between, the smaller ratio, and the scene fits the area.
      const areas: [number, number][] = [[size.w * 1.2, size.h * 1.05], [size.w * 1.01, size.h * 1.3], [1890.5, 928]];
      for (const [w, h] of areas) {
        const s = fitScale({ w, h }, size);
        if (s > MIN_SCALE && s < MAX_SCALE) {
          expect(s).toBeLessThanOrEqual(Math.min(w / size.w, h / size.h));
          expect(size.w * s).toBeLessThanOrEqual(w);
          expect(size.h * s).toBeLessThanOrEqual(h);
        }
      }
    }
  });
});
