// The spots and the navigation graph of AD-170 as amended, from
// `places.ts` and `geometry.ts`.
import { describe, expect, it } from "vitest";
import {
  COLUMN_COUNTS,
  CORR_Y,
  FLOOR_DEPTH,
  G,
  M,
  MAX_COLUMNS,
  MAX_OFFICES,
  R,
  SHARED_CELLS,
  floorWidth,
  officesIn,
  roomOrigin,
  type SharedZone,
} from "./geometry";
import { route } from "./paths";
import {
  CAPACITY,
  COUNTER,
  DECIDER,
  STAFF_CHAIR,
  STAFF_COUNTER,
  allSpots,
  briefingSpot,
  staffSpots,
  navGraph,
  officeCell,
  officeSpots,
  restPoint,
  roomOf,
  sharedSpots,
  type NavGraph,
  type Spot,
} from "./places";

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

/** The rooms of a floor of `columns` columns, by id. */
function rooms(columns: number): string[] {
  return [...(Object.keys(SHARED_CELLS) as SharedZone[]), ...Array.from({ length: officesIn(columns) }, (_, slot) => `office${slot}`)];
}

const onCentreLine = (graph: NavGraph, id: string) => Math.abs(graph.nodes.get(id)!.gy - CORR_Y) < 1e-9;

describe("navigation graph", () => {
  it("every spot is reachable from the exit", () => {
    for (const columns of COLUMN_COUNTS) {
      const graph = navGraph(columns);
      // Links go both ways, once, between known nodes.
      for (const node of graph.nodes.values()) {
        expect(new Set(node.adj).size, node.id).toBe(node.adj.length);
        for (const id of node.adj) expect(graph.nodes.get(id)?.adj, `${node.id} - ${id}`).toContain(node.id);
      }
      const spots = allSpots(columns);
      expect(spots.length).toBeGreaterThan(0);
      for (const s of spots) {
        expect(graph.nodes.get(s.id), `${columns} columns: ${s.id}`).toMatchObject({ gx: s.gx, gy: s.gy });
        const ids = route(graph, graph.exit, s.id);
        expect(ids, `${columns} columns: ${s.id}`).not.toBeNull();
        expect(ids![0]).toBe(graph.exit);
        expect(ids![ids!.length - 1]).toBe(s.id);
        // A spot inside its room is reached from its door through its room.
        if (s.corridor) continue;
        const inside = route(graph, graph.doors.get(roomOf(s))!, s.id)!;
        for (const id of inside) expect(graph.nodes.get(id)!.room, `${s.id} via ${id}`).toBe(roomOf(s));
      }
      // Nothing of the graph is cut off but the staff chain (D-568).
      for (const node of graph.nodes.values()) if (!node.staff) expect(route(graph, graph.exit, node.id), node.id).not.toBeNull();
    }
  });

  // Story 5.18 AC 7 (D-568): every pair of employee spots, at every
  // column count. A route stays within the nodes its ends reach, so the
  // nodes every employee spot reaches holding no staff node covers every
  // pair; each spot's route to the next one is also walked.
  it("no route between two employee spots passes a staff node", () => {
    for (const columns of COLUMN_COUNTS) {
      const graph = navGraph(columns);
      const spots = allSpots(columns);
      const reached = new Set([spots[0]!.id]);
      const todo = [spots[0]!.id];
      while (todo.length > 0) {
        for (const id of graph.nodes.get(todo.pop()!)!.adj) {
          if (!reached.has(id)) {
            reached.add(id);
            todo.push(id);
          }
        }
      }
      for (const s of spots) expect(reached.has(s.id), `${columns} columns: ${s.id}`).toBe(true);
      for (const id of reached) expect(graph.nodes.get(id)!.staff, `${columns} columns: ${id}`).toBe(false);
      spots.forEach((from, i) => {
        const to = spots[(i + 1) % spots.length]!;
        for (const id of route(graph, from.id, to.id)!) expect(graph.nodes.get(id)!.staff, `${from.id} to ${to.id}`).toBe(false);
      });
      expect([...graph.nodes.values()].some((n) => n.staff)).toBe(true);
    }
  });

  it("a staff route joins the decider's chair and the counter", () => {
    const decision = roomOrigin(SHARED_CELLS.decision);
    const { chair, counter } = staffSpots();
    expect([chair.gx, chair.gy]).toEqual([decision.gx + DECIDER.chair.lx, decision.gy + DECIDER.chair.ly]);
    expect(chair.posture).toBe("seated");
    for (const columns of COLUMN_COUNTS) {
      const graph = navGraph(columns);
      for (const [from, to] of [
        [STAFF_CHAIR, STAFF_COUNTER],
        [STAFF_COUNTER, STAFF_CHAIR],
      ] as const) {
        const ids = route(graph, from, to)!;
        expect(ids[0]).toBe(from);
        expect(ids[ids.length - 1]).toBe(to);
        for (const id of ids) expect(graph.nodes.get(id)!.staff, id).toBe(true);
        // It leaves the decision office for reception, nowhere else.
        expect(new Set(ids.map((id) => graph.nodes.get(id)!.room))).toEqual(new Set(["decision", "reception"]));
      }
      expect([graph.nodes.get(STAFF_CHAIR)!.gx, graph.nodes.get(STAFF_COUNTER)!.gy]).toEqual([chair.gx, counter.gy]);
    }
    // The counter's staff side is behind the counter, facing the visitor
    // spot a briefed employee stands at, across it.
    const reception = roomOrigin(SHARED_CELLS.reception);
    expect(counter.gy).toBeLessThan(reception.gy + COUNTER.ly);
    expect(counter.facing).toBe("front");
    const visitor = briefingSpot();
    expect(visitor.room).toBe("reception");
    expect(visitor.gy).toBeGreaterThan(reception.gy + COUNTER.ly + COUNTER.d);
    expect(visitor.facing).toBe("back");
  });

  it("each door links its room to the corridor", () => {
    for (const columns of COLUMN_COUNTS) {
      const graph = navGraph(columns);
      expect([...graph.doors.keys()].sort()).toEqual(rooms(columns).sort());
      for (const [room, door] of graph.doors) {
        const node = graph.nodes.get(door)!;
        expect(node.room).toBe(room);
        const outside = node.adj.filter((id) => graph.nodes.get(id)!.room !== room);
        // One way out, onto the centre line in front of the door.
        expect(outside, room).toHaveLength(1);
        expect(onCentreLine(graph, outside[0]!), room).toBe(true);
        expect(Math.abs(graph.nodes.get(outside[0]!)!.gx - node.gx), room).toBeLessThan(G / 2);
        // No other node of the room leaves it, but the entrance to the exit
        // and the queue's places on the centre line, which are corridor.
        for (const other of graph.nodes.values()) {
          if (other.room !== room || other.staff || other.id === door || onCentreLine(graph, other.id)) continue;
          for (const id of other.adj) {
            const to = graph.nodes.get(id)!;
            if (to.room !== room) expect([other.id, to.id], room).toEqual(["reception:entry", graph.exit]);
          }
        }
      }
    }
  });

  it("the corridor nodes are chained in gx order", () => {
    for (const columns of COLUMN_COUNTS) {
      const graph = navGraph(columns);
      const corridor = [...graph.nodes.keys()]
        .filter((id) => onCentreLine(graph, id))
        .sort((a, b) => graph.nodes.get(a)!.gx - graph.nodes.get(b)!.gx);
      // At least one node in front of each column's doors.
      expect(corridor.length).toBeGreaterThanOrEqual(columns);
      corridor.forEach((id, i) => {
        const gx = graph.nodes.get(id)!.gx;
        if (i > 0) expect(gx, id).toBeGreaterThan(graph.nodes.get(corridor[i - 1]!)!.gx);
        // Linked to its neighbours on the line, and to no other node on it.
        const along = graph.nodes.get(id)!.adj.filter((a) => onCentreLine(graph, a)).sort();
        expect(along, id).toEqual([corridor[i - 1], corridor[i + 1]].filter((a): a is string => a !== undefined).sort());
        expect(gx, id).toBeGreaterThan(M);
        expect(gx, id).toBeLessThan(floorWidth(columns) - M);
      });
    }
  });

  it("the same column count gives the same graph", () => {
    const flat = (g: NavGraph) => JSON.stringify([[...g.nodes.values()], g.exit, [...g.doors]]);
    for (const columns of COLUMN_COUNTS) expect(flat(navGraph(columns))).toBe(flat(navGraph(columns)));
  });
});
