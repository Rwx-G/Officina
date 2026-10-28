import { describe, expect, it } from "vitest";
import type { Board } from "../generated/Board";
import type { ProjectColor } from "../generated/ProjectColor";
import { layoutScene, liveCards, type SceneLayout } from "./layout";
import { emptyMemory, type SceneMemory } from "./memory";
import { SceneObjects } from "./objects";
import { allSpots } from "./places";
import { board, card, fixtureBoards } from "./testing";

const SVG_NS = "http://www.w3.org/2000/svg";

function mount(): { root: SVGGElement; objects: SceneObjects } {
  const svg = document.createElementNS(SVG_NS, "svg");
  const root = document.createElementNS(SVG_NS, "g");
  svg.append(root);
  document.body.append(svg);
  return { root, objects: new SceneObjects(root) };
}

function colours(b: Board): Map<string, ProjectColor | null> {
  return new Map(liveCards(b).map((c) => [c.project.name, c.project.color]));
}

/** Lays out `boards` one after the other, as the scene does. */
function layouts(...boards: Board[]): SceneLayout[] {
  let memory: SceneMemory = emptyMemory(boards[0]!.floor);
  return boards.map((b) => {
    const layout = layoutScene({ board: b }, memory);
    memory = layout.memory;
    return layout;
  });
}

const figure = (root: Element, key: string) =>
  [...root.querySelectorAll("[data-figure]")].find((node) => node.getAttribute("data-figure") === key) ?? null;
const translate = (x: number, y: number) => `translate(${x} ${y})`;

describe("objects", () => {
  it("a placed figure sits among statics by its depth", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const [layout] = layouts(b);
      const { root, objects } = mount();
      objects.update({ layout: layout!, colours: colours(b), snap: 1 });
      const spots = new Map(allSpots(layout!.columns).map((s) => [s.id, s]));
      const placed = layout!.employees.filter((e) => e.spot !== null);
      expect(objects.figureOrder().sort(), name).toEqual(placed.map((e) => e.key).sort());
      for (const e of placed) {
        const spot = spots.get(e.spot!)!;
        const depth = spot.gx + spot.gy;
        const node = figure(root, e.key)!;
        let before = node.previousElementSibling;
        while (before && before.hasAttribute("data-figure")) before = before.previousElementSibling;
        let after = node.nextElementSibling;
        while (after && after.hasAttribute("data-figure")) after = after.nextElementSibling;
        if (before) expect(Number(before.getAttribute("data-depth")), `${name} ${e.key} after`).toBeLessThanOrEqual(depth + 0.01);
        if (after) expect(Number(after.getAttribute("data-depth")), `${name} ${e.key} before`).toBeGreaterThan(depth);
      }
    }
  });

  it("figures on one static keep their creation order", () => {
    const b = board([card("a", "decision"), card("b", "decision")]);
    const [layout] = layouts(b);
    const { objects } = mount();
    objects.update({ layout: layout!, colours: colours(b), snap: 1 });
    const keys = objects.figureOrder();
    objects.update({ layout: layouts(b)[0]!, colours: colours(b), snap: 1 });
    expect(objects.figureOrder()).toEqual(keys);
  });

  it("a settled figure receives no write", () => {
    const b = fixtureBoards().find(({ board: f }) => liveCards(f).length > 5)!.board;
    const { root, objects } = mount();
    const [first] = layouts(b);
    objects.update({ layout: first!, colours: colours(b), snap: 1 });
    const observer = new MutationObserver(() => undefined);
    observer.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    // The same board again, as a new payload: nothing is written.
    const [, again] = layouts(b, structuredClone(b));
    objects.update({ layout: again!, colours: colours(b), snap: 1 });
    expect(observer.takeRecords()).toEqual([]);

    // One card starts working: only what changed is written, and nothing
    // inside a figure whose place and marks stayed.
    const moved = liveCards(b).find((c) => c.scene?.zone === "lounge" || c.scene?.zone === "decision")!;
    const changed = structuredClone(b);
    for (const list of Object.values(changed.columns)) {
      const at = list.findIndex((c) => c.key === moved.key);
      if (at >= 0) list.splice(at, 1);
    }
    changed.columns.working.push({ ...moved, column: "working", scene: { ...moved.scene!, zone: "office", alert: null, approval: false } });
    const [before, after] = layouts(b, changed);
    objects.update({ layout: after!, colours: colours(changed), snap: 1 });
    const records = observer.takeRecords();
    expect(records.length).toBeGreaterThan(0);
    const settled = before!.employees.filter((e) => {
      const next = after!.employees.find((n) => n.key === e.key);
      return next && JSON.stringify(next) === JSON.stringify(e);
    });
    expect(settled.length).toBeGreaterThan(0);
    for (const e of settled) {
      const node = figure(root, e.key)!;
      for (const record of records) {
        const target = record.target as Node;
        if (record.type === "childList" && target === root) continue;
        expect(node.contains(target), `${e.key} written`).toBe(false);
      }
    }
    observer.disconnect();
  });

  it("a new key's node appears at its place and a departed key's node is removed", () => {
    const one = board([card("a", "office")]);
    const two = board([card("a", "office"), card("b", "office", { started: 1 })]);
    const three = board([card("b", "office", { started: 1 })]);
    const [l1, l2, l3] = layouts(one, two, three);
    const { root, objects } = mount();
    objects.update({ layout: l1!, colours: colours(one), snap: 1 });
    const a = figure(root, "a")!;
    expect(figure(root, "b")).toBeNull();
    objects.update({ layout: l2!, colours: colours(two), snap: 1 });
    const b = figure(root, "b")!;
    const at = l2!.employees.find((e) => e.key === "b")!.at!;
    expect(b.getAttribute("transform")).toBe(translate(Math.round(at.x), Math.round(at.y)));
    expect(figure(root, "a")).toBe(a);
    objects.update({ layout: l3!, colours: colours(three), snap: 1 });
    expect(figure(root, "a")).toBeNull();
    expect(a.isConnected).toBe(false);
    expect(figure(root, "b")).toBe(b);
  });

  it("positions are snapped to whole device pixels and follow a new scale", () => {
    const b = board([card("a", "office")]);
    const [layout] = layouts(b);
    const { root, objects } = mount();
    const at = layout!.employees[0]!.at!;
    for (const snap of [1, 1.25, 2, 0.9166666666666666]) {
      objects.update({ layout: layout!, colours: colours(b), snap });
      const [, x, y] = /translate\(([-\d.e]+) ([-\d.e]+)\)/.exec(figure(root, "a")!.getAttribute("transform")!)!;
      for (const [value, raw] of [[Number(x), at.x], [Number(y), at.y]] as const) {
        expect(Math.abs(value * snap - Math.round(value * snap))).toBeLessThan(1e-6);
        expect(Math.abs(value - raw)).toBeLessThanOrEqual(0.5 / snap + 1e-9);
      }
    }
  });

  it("a kept desk shows its session's number and a free desk is faded", () => {
    // `a` works at desk 1, then waits in the decision office: its desk is
    // kept with its board number; desk 2 stays free.
    const working = board([card("a", "office")]);
    const waiting = board([card("a", "decision")]);
    const [l1, l2] = layouts(working, waiting);
    const { root, objects } = mount();
    objects.update({ layout: l1!, colours: colours(working), snap: 1 });
    const desk = (index: number, part: string) => root.querySelector(`[data-desk="0:${index}"][data-desk-part="${part}"]`)!;
    expect(desk(0, "desk").hasAttribute("opacity")).toBe(false);
    expect(desk(1, "desk").getAttribute("opacity")).toBe("0.42");
    expect(desk(0, "desk").querySelector("[data-lines]")!.hasAttribute("display")).toBe(false);
    objects.update({ layout: l2!, colours: colours(waiting), snap: 1 });
    expect(desk(0, "chair").querySelector("[data-away]")!.textContent).toBe("1");
    expect(desk(0, "desk").querySelector("[data-lines]")!.getAttribute("display")).toBe("none");
  });
});
