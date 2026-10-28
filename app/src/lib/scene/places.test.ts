// The spots of AD-170 as amended, from `places.ts` and `geometry.ts`.
import { describe, expect, it } from "vitest";
import { FLOOR_DEPTH, G, M, MAX_COLUMNS, MAX_OFFICES, R, SHARED_CELLS, floorWidth, roomOrigin, type SharedZone } from "./geometry";
import { CAPACITY, DECIDER, allSpots, officeCell, officeSpots, restPoint, sharedSpots, type Spot } from "./places";

const SHARED = Object.keys(SHARED_CELLS) as SharedZone[];

/** Strictly inside the cell of the spot's room. */
function insideRoom(s: Spot, gx: number, gy: number): boolean {
  const cell = s.slot === null ? SHARED_CELLS[s.room as SharedZone] : officeCell(s.slot);
  const o = roomOrigin(cell);
  return gx > o.gx && gx < o.gx + R && gy > o.gy && gy < o.gy + R;
}

const key = (gx: number, gy: number) => `${gx.toFixed(6)},${gy.toFixed(6)}`;

describe("places", () => {
  it("every zone has its spots in order", () => {
    for (const zone of SHARED) {
      const spots = sharedSpots(zone);
      expect(spots.length, zone).toBe(CAPACITY[zone]);
      expect(spots.length, zone).toBeGreaterThan(0);
      spots.forEach((s, i) => {
        expect(s.id).toBe(`${zone}:${i}`);
        expect(s.room).toBe(zone);
        expect(s.slot).toBeNull();
      });
      expect(sharedSpots(zone)).toEqual(spots);
    }
    // The queue fills its room before the corridor.
    const queue = sharedSpots("decision");
    const firstCorridor = queue.findIndex((s) => s.corridor);
    expect(firstCorridor).toBeGreaterThan(0);
    expect(queue.slice(firstCorridor).every((s) => s.corridor)).toBe(true);
    for (let slot = 0; slot < MAX_OFFICES; slot++) {
      const { desks, chips } = officeSpots(slot);
      expect(desks.length).toBe(CAPACITY.desks);
      expect(chips.length).toBe(CAPACITY.chips);
      desks.forEach((s, d) => expect(s.id).toBe(`office${slot}:desk${d}`));
      chips.forEach((s, i) => expect(s.id).toBe(`office${slot}:chip${i}`));
      for (const s of [...desks, ...chips]) expect([s.room, s.slot]).toEqual(["office", slot]);
    }
  });

  it("each spot carries its posture and facing", () => {
    for (const s of sharedSpots("reception")) expect([s.posture, s.facing]).toEqual(["standing", "back"]);
    for (const zone of ["decision", "alert"] as const) {
      for (const s of sharedSpots(zone)) expect([s.posture, s.facing]).toEqual(["standing", "front"]);
    }
    for (const s of sharedSpots("lounge")) {
      expect(s.facing).toBe("front");
      expect(s.sit !== null).toBe(s.posture === "seated");
    }
    const { desks, chips } = officeSpots(0);
    for (const s of desks) expect([s.posture, s.facing, s.sit]).toEqual(["seated", "back", null]);
    for (const s of chips) expect([s.posture, s.facing]).toEqual(["standing", "front"]);
  });

  it("every spot lies inside its room", () => {
    const spots = allSpots(MAX_COLUMNS);
    expect(spots.length).toBeGreaterThan(0);
    for (const s of spots) {
      if (s.corridor) {
        // Only the queue leaves its room, along the corridor between rows.
        expect(s.room, s.id).toBe("decision");
        expect(s.gy, s.id).toBeGreaterThan(M + R);
        expect(s.gy, s.id).toBeLessThan(M + R + G);
        expect(s.gx, s.id).toBeGreaterThan(M);
        expect(s.gx, s.id).toBeLessThan(floorWidth(MAX_COLUMNS) - M);
        continue;
      }
      expect(insideRoom(s, s.gx, s.gy), s.id).toBe(true);
      const rest = restPoint(s);
      expect(insideRoom(s, rest.gx, rest.gy), `${s.id} at rest`).toBe(true);
    }
    expect(FLOOR_DEPTH).toBeGreaterThan(M + R + G);
  });

  it("no two spots of one zone coincide", () => {
    const rooms = new Map<string, Spot[]>();
    for (const s of allSpots(MAX_COLUMNS)) {
      const room = s.slot === null ? s.room : `office${s.slot}`;
      rooms.set(room, [...(rooms.get(room) ?? []), s]);
    }
    for (const [room, spots] of rooms) {
      const points = spots.map((s) => restPoint(s)).map((p) => key(p.gx, p.gy));
      expect(new Set(points).size, room).toBe(points.length);
      expect(new Set(spots.map((s) => s.id)).size, room).toBe(spots.length);
    }
  });

  it("the decider's chair is furniture, not a spot", () => {
    const decision = roomOrigin(SHARED_CELLS.decision);
    const chair = key(decision.gx + DECIDER.chair.lx, decision.gy + DECIDER.chair.ly);
    for (const s of allSpots(MAX_COLUMNS)) {
      expect(key(s.gx, s.gy), s.id).not.toBe(chair);
      expect(key(restPoint(s).gx, restPoint(s).gy), s.id).not.toBe(chair);
    }
  });
});
