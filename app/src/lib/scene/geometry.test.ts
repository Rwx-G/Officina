// AD-169's geometry and AD-70's target property, built from `geometry.ts`
// and `places.ts` alone, at full occupancy for every column count.
import { describe, expect, it } from "vitest";
import {
  COLUMN_COUNTS,
  MAX_COLUMNS,
  MAX_OFFICES,
  MIN_SCALE,
  OFFICE_CELLS,
  SHARED_CELLS,
  TARGET_CSS_MIN,
  TARGET_MIN,
  canvas,
  chipTarget,
  columnsFor,
  entranceSignRect,
  figureTarget,
  intersects,
  moreOfficesTarget,
  officeCells,
  officesIn,
  plaqueRect,
  priorityPillRect,
  projector,
  roomCorners,
  zoneGroupTarget,
  type Rect,
  type SharedZone,
} from "./geometry";
import { allSpots, officeCell, restPoint } from "./places";

interface Owned {
  id: string;
  rect: Rect;
  /** The plaques this target may meet. */
  owns: string[];
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
  for (const s of allSpots(columns)) {
    if (s.slot === moreSlot) continue;
    const rest = restPoint(s);
    out.push({ id: s.id, rect: figureTarget(p(rest.gx, rest.gy)), owns: [] });
  }
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
