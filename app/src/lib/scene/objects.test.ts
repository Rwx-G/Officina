import { describe, expect, it } from "vitest";
import type { Board } from "../generated/Board";
import type { ProjectColor } from "../generated/ProjectColor";
import { ORCHESTRATOR_KEY, layoutScene, liveCards, type SceneLayout } from "./layout";
import { emptyMemory, type SceneMemory } from "./memory";
import { restingFrames, walkingMarks, type FigureFrame } from "./mover";
import { DEPTH_BANDS } from "./geometry";
import { DEPTH_QUANTUM, DEPTH_WINDOW, SceneObjects } from "./objects";
import { deskLetter } from "./rig";
import { allSpots, waitingOffset } from "./places";
import { board, card, fixtureBoards, presence } from "./testing";

const SVG_NS = "http://www.w3.org/2000/svg";

/** A group in an svg of its own, as the scene mounts each layer. */
function layer(): SVGGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  const root = document.createElementNS(SVG_NS, "g");
  svg.append(root);
  document.body.append(svg);
  return root;
}

function mount(): { root: SVGGElement; badges: SVGGElement; objects: SceneObjects } {
  const root = layer();
  const badges = layer();
  return { root, badges, objects: new SceneObjects(root, badges) };
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
  // One case per fixture board, not one loop over them all: the jsdom work
  // grows with every fixture a story adds, and a loop crossed Vitest's
  // 5 s budget on the hosted Windows runner (D-1070).
  it.each(fixtureBoards())("a placed figure sits among statics by its depth ($name)", ({ name, board: b }) => {
    const [layout] = layouts(b);
    const { root, objects } = mount();
    objects.update({ layout: layout!, colours: colours(b), snap: 1, figures: restingFrames(layout!) });
    const spots = new Map(allSpots(layout!.columns).map((s) => [s.id, s]));
    const placed = layout!.employees.filter((e) => e.spot !== null);
    expect(objects.figureOrder().sort(), name).toEqual([...placed.map((e) => e.key), ...layout!.helpers.map((h) => h.key)].sort());
    for (const e of placed) {
      const spot = spots.get(e.spot!)!;
      // Its floor point, off its spot in a waiting zone (D-1046).
      const off = waitingOffset(spot, e.key);
      const depth = spot.gx + off.dgx + spot.gy + off.dgy;
      const node = figure(root, e.key)!;
      let before = node.previousElementSibling;
      while (before && before.hasAttribute("data-figure")) before = before.previousElementSibling;
      let after = node.nextElementSibling;
      while (after && after.hasAttribute("data-figure")) after = after.nextElementSibling;
      if (before) expect(Number(before.getAttribute("data-depth")), `${name} ${e.key} after`).toBeLessThanOrEqual(depth + 0.01);
      if (after) expect(Number(after.getAttribute("data-depth")), `${name} ${e.key} before`).toBeGreaterThan(depth);
    }
  });

  it.each(fixtureBoards())("split into depth bands, the layer paints in the order of one root ($name)", ({ name, board: b }) => {
    const order = (roots: Element[]) => roots.flatMap((r) => [...r.children].map((n) => n.getAttribute("data-figure") ?? n.getAttribute("data-depth")));
    const [first, second] = layouts(b, b);
    const single = mount();
    const svgs = Array.from({ length: DEPTH_BANDS }, layer);
    const banded = new SceneObjects(svgs, layer());
    for (const layout of [first!, second!]) {
      for (const objects of [single.objects, banded]) objects.update({ layout, colours: colours(b), snap: 1, figures: restingFrames(layout) });
    }
    expect(svgs.every((g) => g.children.length > 0), name).toBe(true);
    expect(order(svgs), name).toEqual(order([single.root]));
    expect(banded.figureOrder(), name).toEqual(single.objects.figureOrder());
  });

  it("figures on one static keep their creation order", () => {
    const b = board([card("a", "decision"), card("b", "decision")]);
    const [layout] = layouts(b);
    const { objects } = mount();
    objects.update({ layout: layout!, colours: colours(b), snap: 1, figures: restingFrames(layout!) });
    const keys = objects.figureOrder();
    objects.update({ layout: layouts(b)[0]!, colours: colours(b), snap: 1, figures: restingFrames(layouts(b)[0]!) });
    expect(objects.figureOrder()).toEqual(keys);
  });

  it("a settled figure receives no write", () => {
    const b = fixtureBoards().find(({ board: f }) => liveCards(f).length > 5)!.board;
    const { root, objects } = mount();
    const [first] = layouts(b);
    objects.update({ layout: first!, colours: colours(b), snap: 1, figures: restingFrames(first!) });
    const observer = new MutationObserver(() => undefined);
    observer.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    // The same board again, as a new payload: nothing is written.
    const [, again] = layouts(b, structuredClone(b));
    objects.update({ layout: again!, colours: colours(b), snap: 1, figures: restingFrames(again!) });
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
    objects.update({ layout: after!, colours: colours(changed), snap: 1, figures: restingFrames(after!) });
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

  // Story 5.18 AC 3 (D-567): one node for the run, its pose the only write.
  it("the orchestrator's node is created once and only its pose changes", () => {
    const b = board([card("a", "office")]);
    const lay = (status: "busy" | "idle") => layoutScene({ board: b, orchestrator: presence(status) }, emptyMemory(b.floor));
    const { root, objects } = mount();
    const idle = lay("idle");
    objects.update({ layout: idle, colours: colours(b), snap: 1, figures: restingFrames(idle) });
    const node = figure(root, ORCHESTRATOR_KEY)!;
    expect(node.hasAttribute("data-orchestrator")).toBe(true);
    expect(root.querySelectorAll("[data-orchestrator]").length).toBe(1);
    expect(node.innerHTML).toContain("orch-tunic");
    const shown = () => [...node.querySelectorAll('[data-part="stand"], [data-part="seated-facing"], [data-part="seated-working"]')].filter((g) => g.getAttribute("display") !== "none").map((g) => g.getAttribute("data-part"));
    expect(shown()).toEqual(["seated-facing"]);
    const observer = new MutationObserver(() => undefined);
    observer.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    const busy = lay("busy");
    objects.update({ layout: busy, colours: colours(b), snap: 1, figures: restingFrames(busy) });
    expect(figure(root, ORCHESTRATOR_KEY)).toBe(node);
    expect(shown()).toEqual(["seated-working"]);
    const records = observer.takeRecords().filter((r) => node.contains(r.target as Node));
    expect(records.length).toBeGreaterThan(0);
    for (const r of records) {
      expect(r.type).toBe("attributes");
      expect(r.attributeName).toBe("display");
    }
    // Absent, its node goes.
    const [gone] = layouts(b);
    objects.update({ layout: gone!, colours: colours(b), snap: 1, figures: restingFrames(gone!) });
    expect(figure(root, ORCHESTRATOR_KEY)).toBeNull();
    observer.disconnect();
  });

  it("a new key's node appears at its place and a departed key's node is removed", () => {
    const one = board([card("a", "office")]);
    const two = board([card("a", "office"), card("b", "office", { started: 1 })]);
    const three = board([card("b", "office", { started: 1 })]);
    const [l1, l2, l3] = layouts(one, two, three);
    const { root, objects } = mount();
    objects.update({ layout: l1!, colours: colours(one), snap: 1, figures: restingFrames(l1!) });
    const a = figure(root, "a")!;
    expect(figure(root, "b")).toBeNull();
    objects.update({ layout: l2!, colours: colours(two), snap: 1, figures: restingFrames(l2!) });
    const b = figure(root, "b")!;
    const at = l2!.employees.find((e) => e.key === "b")!.at!;
    expect(b.getAttribute("transform")).toBe(translate(Math.round(at.x), Math.round(at.y)));
    expect(figure(root, "a")).toBe(a);
    objects.update({ layout: l3!, colours: colours(three), snap: 1, figures: restingFrames(l3!) });
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
      objects.update({ layout: layout!, colours: colours(b), snap, figures: restingFrames(layout!) });
      const [, x, y] = /translate\(([-\d.e]+) ([-\d.e]+)\)/.exec(figure(root, "a")!.getAttribute("transform")!)!;
      for (const [value, raw] of [[Number(x), at.x], [Number(y), at.y]] as const) {
        expect(Math.abs(value * snap - Math.round(value * snap))).toBeLessThan(1e-6);
        expect(Math.abs(value - raw)).toBeLessThanOrEqual(0.5 / snap + 1e-9);
      }
    }
  });

  it("a kept desk shows its session's number and a free desk is faded", () => {
    // `a` works at desk 1, then waits in the decision office: its desk is
    // kept with its board number, in the badge layer; desk 2 stays free.
    const working = board([card("a", "office")]);
    const waiting = board([card("a", "decision")]);
    const [l1, l2] = layouts(working, waiting);
    const { root, badges, objects } = mount();
    objects.update({ layout: l1!, colours: colours(working), snap: 1, figures: restingFrames(l1!) });
    const desk = (index: number, part: string) => root.querySelector(`[data-desk="0:${index}"][data-desk-part="${part}"]`)!;
    expect(desk(0, "desk").hasAttribute("opacity")).toBe(false);
    expect(desk(1, "desk").getAttribute("opacity")).toBe("0.42");
    expect(desk(0, "desk").querySelector("[data-lines]")!.hasAttribute("display")).toBe(false);
    objects.update({ layout: l2!, colours: colours(waiting), snap: 1, figures: restingFrames(l2!) });
    expect(badges.querySelector('[data-away-ticket="0:0"]')!.textContent).toBe("1");
    expect(desk(0, "chair").querySelector("[data-away]")!.textContent).toBe("");
    expect(desk(0, "desk").querySelector("[data-lines]")!.getAttribute("display")).toBe("none");
  });

  it("a kept desk whose session rests in the lounge shows the desk's letter on its chair", () => {
    const working = board([card("a", "office")]);
    const resting = board([card("a", "lounge")]);
    const [l1, l2] = layouts(working, resting);
    const { root, badges, objects } = mount();
    objects.update({ layout: l1!, colours: colours(working), snap: 1, figures: restingFrames(l1!) });
    objects.update({ layout: l2!, colours: colours(resting), snap: 1, figures: restingFrames(l2!) });
    const away = root.querySelector('[data-desk="0:0"][data-desk-part="chair"] [data-away]')!;
    expect(away.textContent).toBe(deskLetter(l2!.employees[0]!.marks.deskBadge!));
    expect(away.querySelector("rect")).not.toBeNull();
    expect(away.querySelector("circle")).toBeNull();
    expect(badges.textContent).toBe("");
  });

  // D-566, D-531: the received ring is drawn under the figure at rest, in
  // the working colour, and hidden while it walks like every mark but the
  // question mark.
  it("the received ring is hidden while walking", () => {
    const b = board([card("a", "decision"), card("b", "office", { started: 1 })]);
    const layout = layoutScene({ board: b, received: "a" }, emptyMemory(b.floor));
    const { root, objects } = mount();
    const resting = restingFrames(layout);
    objects.update({ layout, colours: colours(b), snap: 1, figures: resting });
    const ring = () => figure(root, "a")!.querySelector('[data-mark="received"]');
    expect(ring()).not.toBeNull();
    expect(ring()!.getAttribute("stroke")).toContain("color-working");
    expect(figure(root, "b")!.querySelector('[data-mark="received"]')).toBeNull();
    const walking: FigureFrame = { ...resting.get("a")!, moving: true, marks: walkingMarks(resting.get("a")!.marks) };
    objects.frame(new Map([...resting, ["a", walking]]));
    expect(ring()).toBeNull();
    objects.frame(resting);
    expect(ring()).not.toBeNull();
  });

  it.each(fixtureBoards())("queue badges are drawn in the badge layer, above every object, at their figure ($name)", ({ name, board: b }) => {
    const [layout] = layouts(b);
    const { root, badges, objects } = mount();
    objects.update({ layout: layout!, colours: colours(b), snap: 1, figures: restingFrames(layout!) });
    expect(root.querySelector('[data-mark="ticket"]'), name).toBeNull();
    const ticketed = layout!.employees.filter((e) => e.at !== null && (e.marks.ticket !== null || e.marks.approval) && e.facing === "front" && e.pose !== "seated-working");
    for (const e of ticketed) {
      const badge = badges.querySelector(`[data-ticket-of="${e.key}"]`)!;
      expect(badge.querySelector('[data-mark="ticket"]'), `${name} ${e.key}`).not.toBeNull();
      expect(badge.getAttribute("transform"), `${name} ${e.key}`).toBe(figure(root, e.key)!.getAttribute("transform"));
    }
    const shown = [...badges.querySelectorAll("[data-ticket-of]")].filter((g) => g.childElementCount > 0);
    expect(shown.length, name).toBe(ticketed.length);
  });

  describe("depth while moving (AD-171)", () => {
    const b = board([card("a", "office"), card("b", "office", { started: 1 })]);
    const [layout] = layouts(b);

    /** A figure frame of `key` moving at floor depth `depth` (gx + gy). */
    const moving = (key: string, depth: number, isMoving = true): FigureFrame => ({
      ...restingFrames(layout!).get(key)!,
      gx: depth,
      gy: 0,
      drawGx: depth,
      drawGy: 0,
      moving: isMoving,
    });

    /** A static with no other static within `gap` tiles of its depth. */
    function lonelyStatic(root: Element, gap: number): { node: Element; depth: number } {
      const statics = [...root.querySelectorAll("[data-static]")].map((node) => ({ node, depth: Number(node.getAttribute("data-depth")) }));
      const found = statics.find((s, i) => i > 0 && s.depth - statics[i - 1]!.depth > gap && (statics[i + 1]?.depth ?? Infinity) - s.depth > gap);
      if (!found) throw new Error("no lonely static");
      return found;
    }

    const before = (a: Element, b: Element) => (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;

    it("a moving figure's index among statics changes only past the hysteresis", () => {
      const { root, objects } = mount();
      objects.update({ layout: layout!, colours: colours(b), snap: 1, figures: restingFrames(layout!) });
      const s = lonelyStatic(root, 4 * DEPTH_WINDOW);
      const frame = (aDepth: number, bDepth: number, aMoving = true) =>
        objects.frame(new Map([["a", moving("a", aDepth, aMoving)], ["b", moving("b", bDepth)]]));
      const a = () => figure(root, "a")!;
      const far = s.depth + 3;
      frame(s.depth + 2 * DEPTH_WINDOW, far);
      expect(before(s.node, a())).toBe(true);
      // Just behind the static, within the window: it keeps its index.
      frame(s.depth - DEPTH_WINDOW / 2, far);
      expect(before(s.node, a())).toBe(true);
      // Past the window: it goes behind.
      frame(s.depth - DEPTH_WINDOW - 0.01, far);
      expect(before(a(), s.node)).toBe(true);
      // Back within the window on the other side: it stays behind.
      frame(s.depth + DEPTH_WINDOW / 2, far);
      expect(before(a(), s.node)).toBe(true);
      // At rest a figure takes its exact index at once.
      frame(s.depth + DEPTH_WINDOW / 2, far, false);
      expect(before(s.node, a())).toBe(true);

      // Two moving figures between the same statics: the tie depth changes
      // only past DEPTH_QUANTUM.
      const base = s.depth + 2 * DEPTH_WINDOW;
      frame(base, base + DEPTH_QUANTUM * 0.6);
      expect(objects.figureOrder()).toEqual(["a", "b"]);
      frame(base + DEPTH_QUANTUM * 0.9, base + DEPTH_QUANTUM * 0.6);
      expect(objects.figureOrder()).toEqual(["a", "b"]);
      frame(base + DEPTH_QUANTUM * 1.2, base + DEPTH_QUANTUM * 0.6);
      expect(objects.figureOrder()).toEqual(["b", "a"]);
    });

    it("only figure nodes move, and only when their neighbour changed", () => {
      const { root, objects } = mount();
      objects.update({ layout: layout!, colours: colours(b), snap: 1, figures: restingFrames(layout!) });
      const s = lonelyStatic(root, 4 * DEPTH_WINDOW);
      const far = s.depth + 3;
      const frame = (aDepth: number) => objects.frame(new Map([["a", moving("a", aDepth)], ["b", moving("b", far)]]));
      frame(s.depth + 2 * DEPTH_WINDOW);
      const observer = new MutationObserver(() => undefined);
      observer.observe(root, { childList: true });
      // A walk toward the static and past it, in small steps.
      const moves: number[] = [];
      for (let d = s.depth + 2 * DEPTH_WINDOW; d > s.depth - 3 * DEPTH_WINDOW; d -= 0.01) {
        frame(d);
        moves.push(observer.takeRecords().length);
      }
      observer.disconnect();
      // One move, when it crossed the window, of the figure's node only.
      expect(moves.filter((n) => n > 0)).toHaveLength(1);
      const observer2 = new MutationObserver(() => undefined);
      observer2.observe(root, { childList: true });
      frame(s.depth + 2 * DEPTH_WINDOW);
      const records = observer2.takeRecords();
      expect(records.length).toBeGreaterThan(0);
      for (const r of records) {
        for (const node of [...r.addedNodes, ...r.removedNodes]) expect((node as Element).getAttribute("data-figure")).toBe("a");
      }
      observer2.disconnect();
    });
  });
});
