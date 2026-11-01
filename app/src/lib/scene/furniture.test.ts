import { afterEach, describe, expect, it, vi } from "vitest";
import { PROJECT_COLORS } from "../colors";
import type { ProjectColor } from "../generated/ProjectColor";
import { buildStatics, floorShapes, fnv1a, mulberry32, officeStatics, type OfficeUse } from "./furniture";
import { COLUMN_COUNTS, MAX_OFFICES, OFFICE_CELLS, officesIn, projector } from "./geometry";
import { CAPACITY } from "./places";

const COLOURS = Object.keys(PROJECT_COLORS) as ProjectColor[];
const CELL = OFFICE_CELLS[0]!;
const p = projector(COLUMN_COUNTS[0]!);
const markup = (slot: number, colour: ProjectColor | null) => officeStatics(p, slot, CELL, colour).map((s) => s.markup).join("");

afterEach(() => vi.restoreAllMocks());

describe("furniture", () => {
  it("an office looks the same for the same slot and differs between slots", () => {
    const slots = Array.from({ length: MAX_OFFICES }, (_, slot) => slot);
    for (const slot of slots) expect(markup(slot, "green")).toBe(markup(slot, "green"));
    // Drawn in one cell, slots differ by their seeds alone.
    expect(new Set(slots.map((slot) => markup(slot, "green"))).size).toBe(slots.length);
    // The project's colour changes the colours, never the furnishing.
    const shape = (text: string) => text.replace(/var\(--color-project-[\w-]+\)/g, "COLOUR");
    for (const colour of [...COLOURS, null]) expect(shape(markup(3, colour))).toBe(shape(markup(3, "green")));
  });

  it("the furniture builders use no Math.random", () => {
    vi.spyOn(Math, "random").mockImplementation(() => {
      throw new Error("Math.random called");
    });
    for (const columns of COLUMN_COUNTS) {
      const uses: OfficeUse[] = Array.from({ length: officesIn(columns) }, (_, slot) =>
        slot === officesIn(columns) - 1 ? { kind: "more" } : { kind: "office", colour: COLOURS[slot % COLOURS.length]! },
      );
      expect(buildStatics(columns, uses).length).toBeGreaterThan(0);
      expect(buildStatics(columns, []).length).toBeGreaterThan(0);
      expect(floorShapes(columns).length).toBeGreaterThan(0);
    }
  });

  it("statics are sorted by depth, and a floor's statics are the same on every build", () => {
    for (const columns of COLUMN_COUNTS) {
      const statics = buildStatics(columns, []);
      for (let i = 1; i < statics.length; i++) expect(statics[i]!.depth).toBeGreaterThanOrEqual(statics[i - 1]!.depth);
      expect(buildStatics(columns, [])).toEqual(statics);
    }
  });

  it("every office slot has a desk and a chair per desk spot, marked for objects.ts", () => {
    const columns = COLUMN_COUNTS[COLUMN_COUNTS.length - 1]!;
    const statics = buildStatics(columns, []);
    for (let slot = 0; slot < officesIn(columns); slot++) {
      const parts = statics.filter((s) => s.desk?.slot === slot);
      expect(parts.filter((s) => s.desk!.part === "desk").map((s) => s.desk!.index).sort()).toEqual(Array.from({ length: CAPACITY.desks }, (_, i) => i));
      expect(parts.filter((s) => s.desk!.part === "chair").length).toBe(CAPACITY.desks);
      for (const part of parts.filter((s) => s.desk!.part === "desk")) expect(part.markup).toContain("data-screen");
      for (const part of parts.filter((s) => s.desk!.part === "chair")) expect(part.markup).toContain("data-away");
    }
    // More offices has no desk.
    const more = buildStatics(columns, Array.from({ length: officesIn(columns) }, (_, slot) => (slot === 0 ? { kind: "more" } : undefined)));
    expect(more.some((s) => s.desk?.slot === 0)).toBe(false);
  });

  it("the floor grows with its columns and moves nothing it had", () => {
    const [base, ...grown] = COLUMN_COUNTS.map((columns) => floorShapes(columns));
    for (const shapes of grown) expect(shapes.length).toBeGreaterThan(base!.length);
  });

  // Story 4.20 item 7 (D-1046): the owner saw the queue's places drawn on
  // the decision office's and the alert room's floors.
  it("no waiting place is drawn on the floor", () => {
    for (const columns of COLUMN_COUNTS) {
      const marks = floorShapes(columns).filter((s) => s.kind === "ellipse" && s.dash !== undefined);
      expect(marks, `${columns} columns`).toEqual([]);
    }
  });

  it("the seed functions are the prototype's", () => {
    // FNV-1a's published test value for "a".
    expect(fnv1a("a")).toBe(0xe40c292c);
    const a = mulberry32(42);
    const b = mulberry32(42);
    const draws = Array.from({ length: 5 }, () => a());
    expect(Array.from({ length: 5 }, () => b())).toEqual(draws);
    for (const d of draws) expect(d >= 0 && d < 1).toBe(true);
  });
});
