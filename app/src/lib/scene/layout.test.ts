// The scene layout (AD-60 to AD-63 as amended, AD-167, D-102 to D-107,
// D-187, D-462 to D-464, D-482, D-484, D-496): offices, desks, places,
// poses, targets and arrows, on synthetic boards and on every fixture board.
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Board } from "../generated/Board";
import type { Card } from "../generated/Card";
import type { Zone } from "../generated/Zone";
import {
  BASE_COLUMNS,
  MAX_OFFICES,
  SHARED_CELLS,
  canvas,
  columnsFor,
  figureTarget,
  intersects,
  officesIn,
  projector,
  type SharedZone,
} from "./geometry";
import { HELPER_OFFSETS, MAX_NUMBER, alertLit, helperKey, layoutScene, liveCards, nearestTarget, zoneOf, type Direction, type Pose, type SceneLayout } from "./layout";
import { emptyMemory, type SceneMemory } from "./memory";
import { CAPACITY, allSpots, officeSpots, restPoint, sharedSpots } from "./places";
import { approval, board, card, fixtureBoards, project } from "./testing";

const SHARED = Object.keys(SHARED_CELLS) as SharedZone[];

function lay(b: Board, memory: SceneMemory = emptyMemory(b.floor)): SceneLayout {
  return layoutScene({ board: b }, memory);
}

/** Lays out each board in turn, each with the memory the previous returned. */
function run(...boards: Board[]): SceneLayout[] {
  const out: SceneLayout[] = [];
  let memory = emptyMemory(boards[0]?.floor ?? null);
  for (const b of boards) {
    const l = lay(b, memory);
    out.push(l);
    memory = l.memory;
  }
  return out;
}

const employee = (l: SceneLayout, key: string) => {
  const e = l.employees.find((x) => x.key === key);
  if (!e) throw new Error(`no employee ${key}`);
  return e;
};
const officeProjects = (l: SceneLayout) => l.rooms.filter((r) => r.kind === "office").map((r) => r.project!.name);
const deskOf = (l: SceneLayout, key: string) => l.desks.find((d) => d.key === key) ?? null;
const placeOf = (l: SceneLayout, key: string) => {
  const e = l.employees.find((x) => x.key === key);
  return e ? `${e.spot ?? ""}|${e.group ?? ""}` : null;
};

/** Working cards in `n` projects, one each, named p0, p1, ... */
function projects(n: number, start = 0): Card[] {
  return Array.from({ length: n }, (_, i) => card(`k${start + i}`, "office", { project: `p${start + i}`, started: start + i }));
}

afterEach(() => vi.restoreAllMocks());

describe("layoutScene: purity", () => {
  it("identical input gives a deep-equal layout", () => {
    const boards = fixtureBoards();
    expect(boards.length).toBeGreaterThan(0);
    for (const { name, board: b } of boards) {
      expect(lay(structuredClone(b)), name).toEqual(lay(b));
    }
  });

  it("the layout is a fixed point of its own memory", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const first = lay(b);
      const second = lay(b, first.memory);
      expect(second, name).toEqual(first);
    }
  });

  it("reads no clock and no randomness", () => {
    const random = vi.spyOn(Math, "random");
    const now = vi.spyOn(Date, "now");
    for (const { board: b } of fixtureBoards()) lay(b);
    expect(random).not.toHaveBeenCalled();
    expect(now).not.toHaveBeenCalled();
  });

  it("every live card of every fixture board is in exactly one target", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const l = lay(b);
      const listed = l.targets.filter((t) => t.kind !== "helper").flatMap((t) => t.keys);
      expect([...listed].sort(), name).toEqual(liveCards(b).map((c) => c.key).sort());
      for (let i = 0; i < l.targets.length; i++) {
        for (let j = i + 1; j < l.targets.length; j++) {
          expect(intersects(l.targets[i]!.rect, l.targets[j]!.rect), `${name}: ${l.targets[i]!.id} x ${l.targets[j]!.id}`).toBe(false);
        }
      }
    }
  });
});

describe("layoutScene: offices", () => {
  it("floor projects take the first offices in file order", () => {
    const l = lay(board([card("a", "office", { project: "alpha" }), card("b", "office", { project: "beta", started: -1 })], { floor_projects: [project("zeta"), project("alpha")] }));
    expect(officeProjects(l)).toEqual(["zeta", "alpha", "beta"]);
  });

  it("a repeated floor project name takes no second office", () => {
    const l = lay(board([], { floor_projects: [project("zeta"), project("alpha"), project("zeta")] }));
    expect(officeProjects(l)).toEqual(["zeta", "alpha"]);
  });

  it("remembered projects with a live card come next", () => {
    const memory: SceneMemory = { ...emptyMemory(), offices: ["gone", "beta", "gamma"] };
    const b = board(
      [
        card("g", "office", { project: "gamma", started: 1 }),
        card("d", "office", { project: "delta", started: 0 }),
        card("b", "office", { project: "beta", started: 2 }),
      ],
      { floor_projects: [project("alpha")] },
    );
    expect(officeProjects(lay(b, memory))).toEqual(["alpha", "beta", "gamma", "delta"]);
  });

  it("new projects follow in order of first appearance", () => {
    const [, second, third] = run(
      board([card("a", "office", { project: "alpha", started: 5 })]),
      board([card("a", "office", { project: "alpha", started: 5 }), card("b", "office", { project: "beta", started: 1 })]),
      board([card("c", "office", { project: "gamma", started: 0 }), card("b", "office", { project: "beta", started: 1 })]),
    );
    expect(officeProjects(second!)).toEqual(["alpha", "beta"]);
    expect(officeProjects(third!)).toEqual(["alpha", "beta", "gamma"]);
  });

  it("ties go to the earliest start, then the name", () => {
    const l = lay(
      board([
        card("x1", "office", { project: "beta", started: 10 }),
        card("x2", "office", { project: "alpha", started: 10 }),
        card("x3", "office", { project: "gamma", started: null }),
        card("x4", "office", { project: "delta", started: 5 }),
      ]),
    );
    expect(officeProjects(l)).toEqual(["delta", "alpha", "beta", "gamma"]);
    // Desks follow the same ties, then the card key.
    const desks = lay(board([card("kb", "office", { started: 3 }), card("ka", "office", { started: 3 }), card("kn", "office", { started: null })]));
    expect([deskOf(desks, "ka")?.index, deskOf(desks, "kb")?.index, deskOf(desks, "kn")?.index]).toEqual([0, 1, 2]);
  });

  it("an office keeps its slot and dims when its sessions end", () => {
    const [first, second] = run(
      board([card("a", "office", { project: "alpha" }), card("b", "office", { project: "beta", started: 1 })]),
      board([card("b", "office", { project: "beta", started: 1 })]),
    );
    expect(officeProjects(first!)).toEqual(["alpha", "beta"]);
    expect(officeProjects(second!)).toEqual(["alpha", "beta"]);
    const plaque = (l: SceneLayout, name: string) => l.rooms.find((r) => r.project?.name === name)!.plaque;
    expect(plaque(first!, "alpha").state).toBe("ink");
    expect(plaque(second!, "alpha")).toMatchObject({ state: "dimmed", count: 0 });
    expect(plaque(second!, "beta")).toMatchObject({ state: "ink", count: 1 });
  });

  it("a floor project added during the run takes the next office", () => {
    const [, second] = run(
      board([card("b", "office", { project: "beta" })], { floor_projects: [project("alpha")] }),
      board([card("b", "office", { project: "beta" })], { floor_projects: [project("zeta"), project("alpha")] }),
    );
    expect(officeProjects(second!)).toEqual(["alpha", "beta", "zeta"]);
  });

  it("other and unknown are ordinary projects", () => {
    const l = lay(board([card("o", "office", { project: "other" }), card("u", "lounge", { project: "unknown", started: 1 })]));
    expect(officeProjects(l)).toEqual(["other", "unknown"]);
    expect(employee(l, "o").spot).toBe(officeSpots(0).desks[0]!.id);
    expect(employee(l, "u").zone).toBe("lounge");
    expect(deskOf(l, "u")).toMatchObject({ slot: 1, index: 0 });
  });

  it("a fifth project adds a column and the floor never shrinks", () => {
    const base = officesIn(BASE_COLUMNS);
    let memory = emptyMemory();
    let columns = 0;
    for (let n = 1; n <= MAX_OFFICES; n++) {
      const l = lay(board(projects(n)), memory);
      expect(l.columns, `${n} projects`).toBe(columnsFor(n));
      expect(l.canvas).toEqual(canvas(l.columns));
      expect(l.columns).toBeGreaterThanOrEqual(columns);
      if (n === base + 1) expect(l.columns).toBeGreaterThan(BASE_COLUMNS);
      columns = l.columns;
      memory = l.memory;
    }
    // Every session ends: the offices and the columns stay until restart.
    const after = lay(board([]), memory);
    expect(after.columns).toBe(columns);
    expect(officeProjects(after)).toHaveLength(MAX_OFFICES);
    expect(after.rooms.filter((r) => r.kind === "office").every((r) => r.plaque.state === "dimmed")).toBe(true);
  });

  it("a ninth project turns office 8 into More offices", () => {
    const cards = projects(MAX_OFFICES + 1);
    const lastTwo = cards.slice(MAX_OFFICES - 1);
    const waitingInMore = card("w", "decision", { project: lastTwo[0]!.project.name, started: MAX_OFFICES + 1 });
    const l = lay(board([...cards, waitingInMore]));
    const offices = l.rooms.filter((r) => r.kind === "office");
    const more = l.rooms.find((r) => r.kind === "more")!;
    expect(offices).toHaveLength(MAX_OFFICES - 1);
    expect(more.slot).toBe(MAX_OFFICES - 1);
    expect(more.projects).toEqual(lastTwo.map((c) => c.project.name));
    expect(more.plaque.count).toBe(lastTwo.length);
    for (const c of lastTwo) expect(employee(l, c.key)).toMatchObject({ spot: null, group: "more" });
    expect(deskOf(l, waitingInMore.key)).toBeNull();
    expect(employee(l, "w").spot).toBe(sharedSpots("decision")[0]!.id);
    const target = l.targets.find((t) => t.id === "group:more")!;
    expect(target.keys).toEqual(lastTwo.map((c) => c.key));
    expect(l.targets.at(-1)).toBe(target);
    // More offices stays when its sessions end.
    const after = lay(board(cards.slice(0, 1)), l.memory);
    expect(after.rooms.find((r) => r.kind === "more")).toMatchObject({ plaque: { state: "dimmed" } });
    expect(after.targets.find((t) => t.id === "group:more")?.keys).toEqual([]);
  });
});

describe("layoutScene: the alert room and priorities", () => {
  const alertRoom = (l: SceneLayout) => l.rooms.find((r) => r.kind === "alert")!;

  it("the alert room's state follows its occupants", () => {
    const [empty, occupied, kept, cleared] = run(
      board([card("d", "decision"), card("o", "office", { started: 1 })]),
      board([card("d", "decision"), card("a", "alert", { started: 2 }), card("b", "alert", { started: 3 })]),
      // The data goes stale: `a` keeps its last place, and its alert (D-494).
      board([card("d", "decision"), card("a", "unknown", { started: 2, alert: "waited" }), card("b", "office", { started: 3 })]),
      board([card("d", "decision"), card("a", "office", { started: 2 }), card("b", "office", { started: 3 })]),
    );
    expect(alertRoom(empty!).plaque).toMatchObject({ state: "quiet", count: 0 });
    expect(alertLit(empty!)).toBe(false);
    expect(alertRoom(occupied!).plaque).toMatchObject({ state: "alert", count: 2 });
    expect(alertLit(occupied!)).toBe(true);
    // An unknown employee standing in the room keeps it lit, and counts.
    expect(employee(kept!, "a").spot).toBe(employee(occupied!, "a").spot);
    expect(sharedSpots("alert").map((s) => s.id)).toContain(employee(kept!, "a").spot);
    expect(alertRoom(kept!).plaque).toMatchObject({ state: "alert", count: 1 });
    expect(alertRoom(cleared!).plaque).toMatchObject({ state: "quiet", count: 0 });
    expect(alertLit(cleared!)).toBe(false);
    // An unknown employee elsewhere leaves the room quiet.
    const elsewhere = lay(board([card("u", "unknown", { alert: "waited" })]));
    expect(employee(elsewhere, "u").spot).not.toBeNull();
    expect(alertRoom(elsewhere).plaque.state).toBe("quiet");
    // Beyond its spots, the room's group counts too.
    const full = Array.from({ length: CAPACITY.alert + 1 }, (_, i) => card(`a${i}`, "alert", { started: i }));
    const crowded = lay(board(full));
    expect(alertRoom(crowded).plaque).toMatchObject({ state: "alert", count: full.length, more: 1 });
  });

  it("a high-priority office carries its pill", () => {
    const l = lay(
      board([card("h", "office", { project: "urgent" }), card("n", "office", { project: "calm", started: 1 })], {
        floor_projects: [project("urgent", "high"), project("calm")],
      }),
    );
    const pill = (name: string) => l.rooms.find((r) => r.project?.name === name)!.plaque.highPriority;
    expect(pill("urgent")).toBe(true);
    expect(pill("calm")).toBe(false);
    // Shared rooms never carry one.
    expect(l.rooms.filter((r) => r.slot === null).every((r) => !r.plaque.highPriority)).toBe(true);
    // More offices carries it when one of its projects is high priority.
    const cards = projects(MAX_OFFICES + 1);
    const last = cards.at(-1)!.project.name;
    const over = lay(board(cards, { floor_projects: cards.map((c) => project(c.project.name, c.project.name === last ? "high" : "normal")) }));
    expect(over.rooms.find((r) => r.kind === "more")!.plaque.highPriority).toBe(true);
    expect(over.rooms.filter((r) => r.kind === "office").every((r) => !r.plaque.highPriority)).toBe(true);
  });
});

/** Board order of a zone as AD-62 states it, for the expected movers. */
function boardOrder(b: Board, zone: "decision" | "alert"): string[] {
  const at = (c: Card) => {
    const times = b.approvals.filter((a) => a.valid && a.session_id === c.session_id).map((a) => a.requested_at_ms);
    return times.length ? Math.min(...times) : (c.scene?.waiting_since_ms ?? Infinity);
  };
  const live = liveCards(b);
  return [
    ...b.columns.needs_you.filter((c) => zoneOf(c) === zone).map((c) => c.key),
    ...live
      .filter((c) => c.column !== "needs_you" && zoneOf(c) === zone)
      .sort((x, y) => at(x) - at(y) || (x.key < y.key ? -1 : 1))
      .map((c) => c.key),
  ];
}

/** `b` with `key` moved to `zone`, placed first or last in its column. */
function moved(b: Board, key: string, zone: Zone, first: boolean): Board {
  const cards = liveCards(b).filter((c) => c.key !== key);
  const old = liveCards(b).find((c) => c.key === key)!;
  const next = card(key, zone, { project: old.project.name, started: old.started_at_ms });
  return board(first ? [next, ...cards] : [...cards, next], { ...b, columns: b.columns });
}

describe("layoutScene: places", () => {
  it("each zone places its employees", () => {
    const cards: Record<Zone, Card> = {
      reception: card("r", "reception", { started: 6 }),
      office: card("o", "office", { started: 1 }),
      decision: card("d", "decision", { started: 2 }),
      alert: card("a", "alert", { started: 3, project: "beta" }),
      lounge: card("l", "lounge", { started: 4, project: "beta" }),
      unknown: card("u", "unknown", { started: 5, project: "beta" }),
    };
    const l = lay(board(Object.values(cards)));
    const expected: Record<Zone, string> = {
      reception: sharedSpots("reception")[0]!.id,
      office: deskOf(l, "o")!.spot,
      decision: sharedSpots("decision")[0]!.id,
      alert: sharedSpots("alert")[0]!.id,
      lounge: sharedSpots("lounge")[0]!.id,
      unknown: deskOf(l, "u")!.spot,
    };
    for (const [zone, c] of Object.entries(cards) as [Zone, Card][]) {
      expect(employee(l, c.key), zone).toMatchObject({ zone, spot: expected[zone], group: null });
    }
  });

  it("the queue follows board order with approvals after the numbered", () => {
    const n1 = card("n1", "decision", { started: 1 });
    const n2 = card("n2", "decision", { started: 2 });
    const a1 = card("a1", "decision", { column: "working", approval: true, started: 3 });
    const a2 = card("a2", "decision", { column: "working", approval: true, started: 4 });
    const b = board([n2, n1, a1, a2], { approvals: [approval(a1, 200), approval(a1, 10, false), approval(a2, 100)] });
    const l = lay(b);
    const queue = sharedSpots("decision").map((s) => s.id);
    expect(["n2", "n1", "a2", "a1"].map((k) => employee(l, k).spot)).toEqual(queue.slice(0, 4));
    expect(employee(l, "a2").marks).toMatchObject({ ticket: null, approval: true });
    expect(employee(l, "n2").marks).toMatchObject({ ticket: 1, approval: false });
  });

  it("the alert room follows board order", () => {
    const x = card("x", "alert");
    const y = card("y", "decision");
    const z = card("z", "alert");
    const p = card("p", "alert", { column: "working", approval: true, alert: "priority" });
    const l = lay(board([x, y, z, p], { approvals: [approval(p, 5)] }));
    const room = sharedSpots("alert").map((s) => s.id);
    expect(["x", "z", "p"].map((k) => employee(l, k).spot)).toEqual(room.slice(0, 3));
    expect(employee(l, "y").spot).toBe(sharedSpots("decision")[0]!.id);
    expect(["x", "z", "p"].map((k) => employee(l, k).marks.ticket)).toEqual([1, 3, null]);
  });

  it("reception and lounge spots are kept while the employee stays", () => {
    for (const zone of ["reception", "lounge"] as const) {
      const spots = sharedSpots(zone).map((s) => s.id);
      const [first, second, third] = run(
        board([card("a", zone, { started: 1 }), card("b", zone, { started: 2 })]),
        board([card("a", "office", { started: 1 }), card("b", zone, { started: 2 })]),
        board([card("a", "office", { started: 1 }), card("b", zone, { started: 2 }), card("c", zone, { started: 3 })]),
      );
      expect([employee(first!, "a").spot, employee(first!, "b").spot], zone).toEqual(spots.slice(0, 2));
      expect(employee(second!, "b").spot, zone).toBe(spots[1]);
      expect(employee(third!, "b").spot, zone).toBe(spots[1]);
      expect(employee(third!, "c").spot, zone).toBe(spots[0]);
    }
  });

  it("an unknown employee keeps its last place", () => {
    const queue = sharedSpots("decision").map((s) => s.id);
    const [first, second] = run(
      board([card("n1", "decision"), card("x", "decision"), card("o", "office", { started: 5 })]),
      board([card("n3", "decision", { started: 9 }), card("n4", "decision", { started: 10 }), card("x", "unknown"), card("o", "office", { started: 5 })]),
    );
    expect(employee(first!, "x")).toMatchObject({ spot: queue[1], pose: "standing" });
    expect(employee(second!, "x")).toMatchObject({ zone: "unknown", spot: queue[1], pose: "standing", facing: "front" });
    expect(employee(second!, "x").marks.question).toBe(true);
    // The spot it holds is taken: the queue fills around it.
    expect([employee(second!, "n3").spot, employee(second!, "n4").spot]).toEqual([queue[0], queue[2]]);
    expect(employee(second!, "o").spot).toBe(employee(first!, "o").spot);
    // A stale board: everyone is unknown and stays where they were.
    const stale = board([card("n3", "unknown", { started: 9 }), card("n4", "unknown", { started: 10 }), card("x", "unknown"), card("o", "unknown", { started: 5 })]);
    const third = lay(stale, second!.memory);
    for (const k of ["n3", "n4", "x", "o"]) expect(placeOf(third, k), k).toBe(placeOf(second!, k));
    expect(employee(third, "o").pose).toBe("seated-working");
  });

  it("an unknown employee whose last place left the floor is placed as first seen", () => {
    const memory: SceneMemory = {
      ...emptyMemory(),
      last: { u: { spot: "office99:desk0", group: null, pose: "standing", facing: "front" } },
    };
    const l = lay(board([card("u", "unknown")]), memory);
    expect(employee(l, "u")).toMatchObject({ spot: officeSpots(0).desks[0]!.id, pose: "seated-facing" });
    expect(deskOf(l, "u")).toMatchObject({ index: 0, state: "occupied" });
  });

  it("a live card without a scene state is placed as unknown", () => {
    const bare = { ...card("s", "office"), scene: null };
    const l = lay(board([bare]));
    expect(employee(l, "s")).toMatchObject({ zone: "unknown", spot: officeSpots(0).desks[0]!.id, marks: { question: true } });
  });

  it("a chip list member that takes a desk frees its chip spot", () => {
    const full = Array.from({ length: CAPACITY.desks }, (_, i) => card(`w${i}`, "office", { started: i }));
    const chip = officeSpots(0).chips[0]?.id ?? null;
    const [first, second] = run(
      board([...full, card("u", "unknown", { started: 10 })]),
      board([...full.slice(1), card("u", "office", { started: 10 }), card("x", "office", { started: 11 })]),
    );
    expect(employee(first!, "u").spot).toBe(chip);
    expect(employee(second!, "u").spot).toBe(officeSpots(0).desks[0]!.id);
    expect(employee(second!, "x").spot).toBe(chip);
  });

  it("an unknown employee first seen sits at its desk", () => {
    const l = lay(board([card("w", "office", { started: 0 }), card("u", "unknown", { started: 1 })]));
    expect(employee(l, "u")).toMatchObject({ spot: officeSpots(0).desks[1]!.id, pose: "seated-facing", group: null });
    expect(employee(l, "u").marks.question).toBe(true);
    expect(deskOf(l, "u")).toMatchObject({ state: "occupied", lit: false });
    // Without a free desk it stands with its office's chip list.
    const full = Array.from({ length: CAPACITY.desks }, (_, i) => card(`w${i}`, "office", { started: i }));
    const crowded = lay(board([...full, card("u", "unknown", { started: 99 })]));
    expect(employee(crowded, "u").spot).toBe(officeSpots(0).chips[0]?.id ?? null);
  });

  it("beyond its spots a zone fills its group", () => {
    for (const zone of SHARED) {
      const cards = Array.from({ length: CAPACITY[zone] + 2 }, (_, i) => card(`${zone}${i}`, zone, { started: i, project: `p${i}` }));
      const l = lay(board(cards));
      const spots = sharedSpots(zone).map((s) => s.id);
      cards.forEach((c, i) => {
        if (i < spots.length) expect(employee(l, c.key), c.key).toMatchObject({ spot: spots[i], group: null });
        else expect(employee(l, c.key), c.key).toMatchObject({ spot: null, group: zone, at: null });
      });
      const extra = cards.slice(spots.length).map((c) => c.key);
      expect(l.targets.find((t) => t.id === `group:${zone}`)?.keys, zone).toEqual(extra);
      expect(l.rooms.find((r) => r.id === zone)?.plaque.more, zone).toBe(extra.length);
    }
    const office = Array.from({ length: CAPACITY.desks + CAPACITY.chips + 1 }, (_, i) => card(`w${i}`, "office", { started: i }));
    const l = lay(board(office));
    const last = office.at(-1)!.key;
    expect(employee(l, last)).toMatchObject({ spot: null, group: "office0" });
    expect(l.targets.find((t) => t.id === "group:office0")?.keys).toEqual([last]);
    expect(l.rooms.find((r) => r.id === "office0")?.plaque.more).toBe(1);
  });

  it("only the changed card and the queue members whose order changed get a new place", () => {
    const zones: Zone[] = ["reception", "office", "decision", "alert", "lounge", "unknown"];
    let cases = 0;
    for (const { name, board: b } of fixtureBoards()) {
      const before = lay(b, lay(b).memory);
      for (const c of liveCards(b)) {
        for (const zone of zones) {
          if (zone === zoneOf(c)) continue;
          for (const first of [true, false]) {
            const next = moved(b, c.key, zone, first);
            const after = lay(next, before.memory);
            const movers = new Set([c.key]);
            for (const z of ["decision", "alert"] as const) {
              const was = boardOrder(b, z);
              const now = boardOrder(next, z);
              for (const k of new Set([...was, ...now])) if (was.indexOf(k) !== now.indexOf(k)) movers.add(k);
            }
            for (const other of liveCards(b)) {
              if (movers.has(other.key)) continue;
              expect(placeOf(after, other.key), `${name}: ${c.key} to ${zone}${first ? " first" : ""}, ${other.key}`).toBe(placeOf(before, other.key));
            }
            cases++;
          }
        }
      }
    }
    expect(cases).toBeGreaterThan(0);
  });
});

describe("layoutScene: desks", () => {
  it("a session at reception has no desk yet", () => {
    const l = lay(board([card("r", "reception")]));
    expect(deskOf(l, "r")).toBeNull();
    expect(l.desks.every((d) => d.state === "free")).toBe(true);
    const [, later] = run(board([card("r", "reception")]), board([card("r", "office")]));
    expect(deskOf(later!, "r")).toMatchObject({ index: 0, state: "occupied", lit: true });
  });

  it("a session keeps its desk while it waits or rests", () => {
    const [first, second] = run(
      board([card("w", "office", { started: 0 }), card("v", "office", { started: 1 })]),
      board([card("w", "decision", { started: 0 }), card("v", "lounge", { started: 1 })]),
    );
    expect([deskOf(first!, "w")?.index, deskOf(first!, "v")?.index]).toEqual([0, 1]);
    expect(deskOf(second!, "w")).toMatchObject({ index: 0, state: "kept" });
    expect(deskOf(second!, "v")).toMatchObject({ index: 1, state: "kept" });
  });

  it("a kept desk shows the number up to nine or the desk number in the lounge", () => {
    const waiting = Array.from({ length: MAX_NUMBER }, (_, i) => card(`n${i}`, "decision", { started: 10 + i, project: `q${i % 3}` }));
    const late = card("late", "decision", { started: 0 });
    const lounge = card("v", "lounge", { started: 1 });
    const approvalOnly = card("a", "decision", { column: "working", approval: true, started: 2 });
    const l = lay(board([...waiting, late, lounge, approvalOnly], { approvals: [approval(approvalOnly, 1)] }));
    expect(deskOf(l, "late")).toMatchObject({ state: "kept", number: null });
    expect(deskOf(l, "v")).toMatchObject({ state: "kept", number: deskOf(l, "v")!.index + 1 });
    expect(deskOf(l, "a")).toMatchObject({ state: "kept", number: null });
    expect(deskOf(l, "n0")).toMatchObject({ state: "kept", number: 1 });
    expect(deskOf(l, `n${MAX_NUMBER - 1}`)).toMatchObject({ number: MAX_NUMBER });
  });

  it("an ended session frees its desk", () => {
    const [, second, third] = run(
      board([card("w0", "office", { started: 0 }), card("w1", "office", { started: 1 })]),
      board([card("w1", "office", { started: 1 })]),
      board([card("w1", "office", { started: 1 }), card("w2", "office", { started: 2 })]),
    );
    expect(second!.desks.find((d) => d.slot === 0 && d.index === 0)).toMatchObject({ state: "free", key: null });
    expect(deskOf(third!, "w2")?.index).toBe(0);
    expect(deskOf(third!, "w1")?.index).toBe(1);
  });

  it("a fifth live session goes to the chip list and stays there until its zone changes", () => {
    const full = Array.from({ length: CAPACITY.desks }, (_, i) => card(`w${i}`, "office", { started: i }));
    const extra = card("x", "office", { started: CAPACITY.desks });
    const chip = officeSpots(0).chips[0]?.id ?? null;
    const [first, second, third, fourth] = run(
      board([...full, extra]),
      board([...full.slice(1), extra]),
      board([...full.slice(1), card("x", "lounge", { started: CAPACITY.desks })]),
      board([...full.slice(1), extra]),
    );
    expect(deskOf(first!, "x")).toBeNull();
    expect(employee(first!, "x")).toMatchObject({ spot: chip, pose: "standing" });
    // A desk frees: the chip list keeps its member while its zone holds.
    expect(deskOf(second!, "x")).toBeNull();
    expect(employee(second!, "x").spot).toBe(chip);
    expect(second!.desks.find((d) => d.slot === 0 && d.index === 0)?.state).toBe("free");
    // Its zone changes: it takes the lowest free desk.
    expect(deskOf(third!, "x")).toMatchObject({ index: 0, state: "kept", number: 1 });
    expect(employee(third!, "x").marks.deskBadge).toBe(1);
    expect(employee(fourth!, "x")).toMatchObject({ spot: officeSpots(0).desks[0]!.id, pose: "seated-working" });
  });
});

describe("layoutScene: sub-agents", () => {
  const agents = (n: number) => Array.from({ length: n }, (_, i) => ({ agent_id: `a${i}`, agent_type: i % 2 ? "Plan" : "Explore" }));
  const chairOf = (l: SceneLayout, key: string) => {
    const desk = deskOf(l, key)!;
    return officeSpots(desk.slot).desks[desk.index]!;
  };

  it("sub-agents stand beside their lead's desk, three at most", () => {
    const lead = card("lead", "office", { subagents: agents(HELPER_OFFSETS.length + 2) });
    const l = lay(board([lead, card("other", "office", { started: 1 })]));
    const chair = chairOf(l, "lead");
    expect(l.helpers.map((h) => h.key)).toEqual(agents(HELPER_OFFSETS.length).map((a) => helperKey("lead", a.agent_id)));
    l.helpers.forEach((h, i) => {
      const [dgx, dgy] = HELPER_OFFSETS[i]!;
      expect([h.gx, h.gy]).toEqual([chair.gx + dgx, chair.gy + dgy]);
      expect(h.at).toEqual(projector(l.columns)(h.gx, h.gy));
      expect([h.lead, h.project]).toEqual(["lead", "alpha"]);
    });
    expect(lay(board([card("solo", "office")])).helpers).toEqual([]);
  });

  it("sub-agents stay at the desk while the lead waits or rests", () => {
    const subagents = agents(2);
    const [working, waiting, resting, unknown] = run(
      board([card("lead", "office", { subagents })]),
      board([card("lead", "decision", { subagents })]),
      board([card("lead", "lounge", { subagents })]),
      board([card("lead", "unknown", { subagents })]),
    );
    for (const l of [waiting!, resting!, unknown!]) expect(l.helpers).toEqual(working!.helpers);
    expect(employee(waiting!, "lead").spot).not.toBe(employee(working!, "lead").spot);
  });

  it("a lead without a desk draws no sub-agent", () => {
    const subagents = agents(1);
    expect(lay(board([card("r", "reception", { subagents })])).helpers).toEqual([]);
    // A fifth session at work in an office of four desks stands at the chip spot.
    const full = Array.from({ length: CAPACITY.desks }, (_, i) => card(`d${i}`, "office", { started: i }));
    const l = lay(board([...full, card("chip", "office", { started: 9, subagents })]));
    expect(deskOf(l, "chip")).toBeNull();
    expect(l.helpers).toEqual([]);
  });

  it("a sub-agent's target sits at its place and names its lead, where it overlaps no target", () => {
    let seen = 0;
    for (const { name, board: b } of fixtureBoards()) {
      const l = lay(b);
      for (const t of l.targets.filter((x) => x.kind === "helper")) {
        const h = l.helpers.find((x) => `helper:${x.key}` === t.id)!;
        expect(t.keys, name).toEqual([h.lead]);
        expect(t.rect, name).toEqual(figureTarget(h.at));
        seen++;
      }
    }
    expect(seen).toBeGreaterThan(0);
  });
});

describe("layoutScene: poses and marks", () => {
  it("the pose and marks follow the zone", () => {
    const lounge = sharedSpots("lounge")[0]!;
    const expected: Record<Zone, { pose: Pose; facing: "front" | "back"; marks: Partial<SceneLayout["employees"][number]["marks"]> }> = {
      office: { pose: "seated-working", facing: "back", marks: { ticket: null, hand: false, ring: false, question: false } },
      decision: { pose: "standing", facing: "front", marks: { ticket: 1, approval: false, question: false } },
      alert: { pose: "standing", facing: "front", marks: { ticket: 2, approval: false } },
      lounge: {
        pose: lounge.posture === "seated" ? "seated-facing" : "standing",
        facing: "front",
        marks: { hand: true, ring: true, deskBadge: 2, ticket: null },
      },
      reception: { pose: "standing", facing: "back", marks: { ticket: null, hand: false, deskBadge: null } },
      unknown: { pose: "seated-facing", facing: "front", marks: { question: true, hand: false } },
    };
    const cards: Record<Zone, Card> = {
      office: card("o", "office", { started: 0 }),
      lounge: card("l", "lounge", { started: 1 }),
      decision: card("d", "decision", { started: 2 }),
      alert: card("a", "alert", { started: 3 }),
      reception: card("r", "reception", { started: 4 }),
      unknown: card("u", "unknown", { started: 5, project: "beta" }),
    };
    const l = lay(board(Object.values(cards)));
    for (const zone of Object.keys(expected) as Zone[]) {
      const e = employee(l, cards[zone].key);
      expect({ pose: e.pose, facing: e.facing }, zone).toEqual({ pose: expected[zone].pose, facing: expected[zone].facing });
      expect(e.marks, zone).toMatchObject(expected[zone].marks);
    }
    const approvalOnly = card("p", "decision", { column: "working", approval: true });
    const withApproval = lay(board([approvalOnly], { approvals: [approval(approvalOnly, 1)] }));
    expect(employee(withApproval, "p").marks).toMatchObject({ ticket: null, approval: true });
    // A desk lights only while its session works there.
    expect(deskOf(l, "o")).toMatchObject({ state: "occupied", lit: true });
  });
});

describe("layoutScene: targets", () => {
  it("targets follow the tab order", () => {
    const cards = [
      card("r", "reception", { started: 9 }),
      card("d1", "decision", { project: "beta", started: 5 }),
      card("d2", "decision", { started: 6 }),
      card("a", "alert", { started: 7 }),
      ...Array.from({ length: CAPACITY.desks }, (_, i) => card(`w${i}`, "office", { started: i })),
      card("b", "office", { project: "beta", started: 8 }),
      card("l", "lounge", { project: "beta", started: 10 }),
    ];
    const l = lay(board(cards));
    const rooms = ["reception", "decision", "alert", "office0", "office1", "lounge"];
    const groups = l.targets.map((t) => t.group).filter((g, i, all) => all.indexOf(g) === i);
    expect(groups).toEqual(rooms);
    expect(l.targets.filter((t) => t.group === "decision").map((t) => t.id)).toEqual(["figure:d1", "figure:d2"]);
    // Desks front to back: the nearest the author first.
    const office = l.targets.filter((t) => t.group === "office0").map((t) => t.keys[0]!);
    const depth = (key: string) => {
      const spot = allSpots(l.columns).find((s) => s.id === employee(l, key).spot)!;
      return spot.gx + spot.gy;
    };
    const depths = office.map(depth);
    expect([...depths].sort((x, y) => y - x)).toEqual(depths);
  });

  it("a figure's target sits at its place", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const l = lay(b);
      const p = projector(l.columns);
      const spots = new Map(allSpots(l.columns).map((s) => [s.id, s]));
      for (const t of l.targets.filter((x) => x.kind === "figure")) {
        const e = employee(l, t.keys[0]!);
        const spot = spots.get(e.spot!)!;
        const rest = restPoint(spot);
        expect(e.at, `${name} ${e.key}`).toEqual(p(rest.gx, rest.gy));
        expect(t.rect, `${name} ${e.key}`).toEqual(figureTarget(e.at!));
      }
      for (const e of l.employees) expect(e.spot === null, `${name} ${e.key}`).toBe(e.group !== null);
    }
  });

  it("arrows follow the vectors of a full office and a full queue", () => {
    // A full office: four desks, its chip spot and its chip (D-187).
    const office = Array.from({ length: CAPACITY.desks + CAPACITY.chips + 1 }, (_, i) => card(`w${i}`, "office", { started: i }));
    const officeLayout = lay(board(office));
    const desk = (d: number) => `figure:w${d}`;
    const chipSpot = `figure:w${CAPACITY.desks}`;
    const chip = "group:office0";
    const officeVectors: [string, Direction, string | null][] = [
      [desk(3), "up", desk(0)],
      [desk(3), "down", null],
      [desk(3), "left", desk(2)],
      [desk(3), "right", desk(1)],
      [desk(1), "up", null],
      [desk(1), "down", null],
      [desk(1), "left", desk(3)],
      [desk(1), "right", null],
      [desk(2), "up", chip],
      [desk(2), "down", null],
      [desk(2), "left", chipSpot],
      [desk(2), "right", desk(0)],
      [desk(0), "up", null],
      [desk(0), "down", desk(3)],
      [desk(0), "left", chip],
      [desk(0), "right", desk(1)],
      [chipSpot, "up", chip],
      [chipSpot, "down", null],
      [chipSpot, "left", null],
      [chipSpot, "right", desk(2)],
      [chip, "up", null],
      [chip, "down", desk(2)],
      [chip, "left", null],
      [chip, "right", desk(0)],
    ];
    for (const [from, direction, to] of officeVectors) {
      expect(nearestTarget(officeLayout.targets, from, direction), `${from} ${direction}`).toBe(to);
    }

    // A full queue: every queue spot taken, one more in the group, whose
    // "+n" is at the right end of the office's plaque, above the queue's
    // first places (D-557).
    const queue = Array.from({ length: CAPACITY.decision + 1 }, (_, i) => card(`n${i}`, "decision", { started: i, project: `p${i}` }));
    const queueLayout = lay(board(queue));
    const q = (i: number) => `figure:n${i}`;
    const group = "group:decision";
    const queueVectors: [string, Direction, string | null][] = [
      [q(0), "up", group],
      [q(0), "down", null],
      [q(0), "left", q(1)],
      [q(0), "right", null],
      [q(1), "up", group],
      [q(1), "down", null],
      [q(1), "left", q(2)],
      [q(1), "right", q(0)],
      [q(2), "up", null],
      [q(2), "down", null],
      [q(2), "left", q(3)],
      [q(2), "right", q(1)],
      [q(3), "up", null],
      [q(3), "down", null],
      [q(3), "left", q(4)],
      [q(3), "right", q(2)],
      [q(4), "up", null],
      [q(4), "down", null],
      [q(4), "left", q(5)],
      [q(4), "right", q(3)],
      [q(5), "up", null],
      [q(5), "down", null],
      [q(5), "left", null],
      [q(5), "right", q(4)],
      [group, "up", null],
      [group, "down", q(1)],
      [group, "left", q(2)],
      [group, "right", null],
    ];
    for (const [from, direction, to] of queueVectors) {
      expect(nearestTarget(queueLayout.targets, from, direction), `${from} ${direction}`).toBe(to);
    }
  });

  it("an arrow with no target within 45 degrees goes nowhere", () => {
    const l = lay(board([card("w0", "office", { started: 0 }), card("w1", "office", { started: 1 }), card("r", "reception")]));
    // Desk 1 lies right of and below desk 0, more right than below.
    expect(nearestTarget(l.targets, "figure:w0", "down")).toBeNull();
    expect(nearestTarget(l.targets, "figure:w0", "right")).toBe("figure:w1");
    // Arrows stay in the focused target's group.
    expect(nearestTarget(l.targets, "figure:r", "right")).toBeNull();
    expect(nearestTarget(l.targets, "no such target", "left")).toBeNull();
  });
});
