// The mover (AD-172 as amended, D-469, D-470, D-483, D-502), driven by an
// injected clock, frame source and visibility.
import { describe, expect, it } from "vitest";
import type { Board } from "../generated/Board";
import { HH, HW } from "./geometry";
import { layoutScene, type SceneLayout } from "./layout";
import { emptyMemory, type SceneMemory } from "./memory";
import { FADE_MS, FRAME_CAP_S, FRAME_GAP_S, GLIDE_MS, HELPER_HOLD_MS, MIN_FRAME_MS, Mover, WALK, restingFrames, type FigureFrame, type MotionEnv } from "./mover";
import { CORNER_RADIUS, route } from "./paths";
import { allSpots, navGraph, restPoint } from "./places";
import { board, card, fixtureBoards } from "./testing";

/** A frame source and clock the test steps by hand. */
class Frames implements MotionEnv {
  t = 1_000;
  isHidden = false;
  reduced = false;
  #id = 0;
  #queue = new Map<number, (at: number) => void>();
  #hiddenListeners = new Set<() => void>();
  now = () => this.t;
  request = (frame: (at: number) => void) => {
    this.#queue.set(++this.#id, frame);
    return this.#id;
  };
  cancel = (handle: number) => void this.#queue.delete(handle);
  hidden = () => this.isHidden;
  onHidden = (listener: () => void) => {
    this.#hiddenListeners.add(listener);
    return () => void this.#hiddenListeners.delete(listener);
  };
  reducedMotion = () => this.reduced;
  get pending(): number {
    return this.#queue.size;
  }
  /** Advances the clock by `ms` and runs the frames requested before. */
  step(ms = 16): void {
    this.t += ms;
    const frames = [...this.#queue.values()];
    this.#queue.clear();
    for (const frame of frames) frame(this.t);
  }
  hide(): void {
    this.isHidden = true;
    for (const listener of this.#hiddenListeners) listener();
  }
}

/** The layouts a window shows for `boards`, one after the other. */
function layouts(...boards: Board[]): SceneLayout[] {
  let memory: SceneMemory = emptyMemory(boards[0]!.floor);
  return boards.map((b) => {
    const layout = layoutScene({ board: b }, memory);
    memory = layout.memory;
    return layout;
  });
}

function setup(): { frames: Frames; mover: Mover; host: HTMLElement } {
  const frames = new Frames();
  const mover = new Mover(frames);
  const host = document.createElement("div");
  host.setAttribute("data-moving", "0");
  mover.attach(host);
  return { frames, mover, host };
}

/** Steps frames until nothing moves, returning `key`'s frame after each. */
function walk(frames: Frames, mover: Mover, key: string, ms: number | (() => number) = 16, max = 4_000): FigureFrame[] {
  const out: FigureFrame[] = [];
  for (let i = 0; i < max && mover.moving; i++) {
    frames.step(typeof ms === "number" ? ms : ms());
    const f = mover.figures().get(key);
    if (f) out.push(f);
  }
  expect(mover.moving, "still moving").toBe(false);
  return out;
}

const spotOf = (layout: SceneLayout, key: string) => {
  const id = layout.employees.find((e) => e.key === key)!.spot!;
  return allSpots(layout.columns).find((s) => s.id === id)!;
};

/** Screen px per tile along a grid displacement. */
const screen = (dgx: number, dgy: number) => ({ x: (dgx - dgy) * HW, y: (dgx + dgy) * HH });

const working = board([card("w", "office"), card("other", "office", { started: 1 })]);
const waiting = board([card("w", "decision"), card("other", "office", { started: 1 })]);
const resting = board([card("w", "lounge"), card("other", "office", { started: 1 })]);

describe("mover", () => {
  it("a walk follows the route with continuous positions and stops on the spot", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(working, waiting);
    mover.update(a!, true);
    const from = spotOf(a!, "w");
    mover.update(b!, true);
    expect(mover.moving).toBe(true);
    expect(mover.transit()).toEqual(new Set(["w"]));
    const path = walk(frames, mover, "w");
    expect(path.length).toBeGreaterThan(30);
    // Along the graph's route, within a corner's radius of it.
    const graph = navGraph(b!.columns);
    const to = spotOf(b!, "w");
    const nodes = route(graph, from.id, to.id)!.map((id) => graph.nodes.get(id)!);
    const near = (f: FigureFrame) =>
      nodes.slice(1).some((n, i) => {
        const m = nodes[i]!;
        const dx = n.gx - m.gx;
        const dy = n.gy - m.gy;
        const t = Math.max(0, Math.min(1, ((f.gx - m.gx) * dx + (f.gy - m.gy) * dy) / (dx * dx + dy * dy)));
        return Math.hypot(f.gx - m.gx - t * dx, f.gy - m.gy - t * dy) <= CORNER_RADIUS;
      });
    let previous = { gx: from.gx, gy: from.gy };
    for (const f of path) {
      expect(near(f), `${f.gx}, ${f.gy}`).toBe(true);
      expect(Math.hypot(f.gx - previous.gx, f.gy - previous.gy)).toBeLessThanOrEqual(WALK.vmax * 0.016 + 1e-9);
      previous = f;
    }
    const last = path[path.length - 1]!;
    expect([last.gx, last.gy]).toEqual([to.gx, to.gy]);
    expect(last).toMatchObject({ moving: false, stride: null, pose: "standing", facing: to.facing, opacity: 1 });
    expect(mover.figures().get("w")).toEqual({ ...restingFrames(b!).get("w")!, flip: last.flip });
    expect(mover.transit()).toEqual(new Set());
  });

  it("speed never exceeds WALK.vmax", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(working, waiting);
    mover.update(a!, true);
    mover.update(b!, true);
    // Uneven frames, some longer than the cap.
    const steps = [16, 7, 33, 120, 16, 50];
    let i = 0;
    let previous = { gx: spotOf(a!, "w").gx, gy: spotOf(a!, "w").gy };
    let fastest = 0;
    const path = walk(frames, mover, "w", () => steps[i++ % steps.length]!);
    // A vsync sooner than MIN_FRAME_MS after the last drawn frame draws
    // nothing; the next one advances by the whole time since.
    let since = 0;
    for (const [n, f] of path.entries()) {
      since += steps[n % steps.length]!;
      if (since < MIN_FRAME_MS) {
        expect([f.gx, f.gy]).toEqual([previous.gx, previous.gy]);
        continue;
      }
      const dt = Math.min(since / 1000, FRAME_CAP_S);
      since = 0;
      const speed = Math.hypot(f.gx - previous.gx, f.gy - previous.gy) / dt;
      expect(speed).toBeLessThanOrEqual(WALK.vmax + 1e-9);
      fastest = Math.max(fastest, speed);
      previous = f;
    }
    // It reaches full speed on the way.
    expect(fastest).toBeGreaterThan(WALK.vmax * 0.9);
  });

  it("a fast display draws one frame per MIN_FRAME_MS, a 60 Hz one every vsync", () => {
    for (const [vsync, drawnEvery] of [
      [1000 / 240, 4],
      [1000 / 144, 2],
      [1000 / 75, 1],
      [1000 / 60, 1],
    ] as const) {
      const { frames, mover } = setup();
      const [a, b] = layouts(working, waiting);
      mover.update(a!, true);
      mover.update(b!, true);
      let notified = 0;
      mover.subscribe(() => notified++);
      const vsyncs = 48;
      for (let i = 0; i < vsyncs; i++) frames.step(vsync);
      expect(notified, `${Math.round(1000 / vsync)} Hz`).toBe(vsyncs / drawnEvery);
    }
  });

  it("an object at rest is handed out again unchanged, a walking one anew", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(working, waiting);
    mover.update(a!, true);
    const resting = mover.figures();
    expect(mover.figures().get("w")).toBe(resting.get("w"));
    mover.update(b!, true);
    frames.step();
    const first = mover.figures();
    frames.step();
    const second = mover.figures();
    expect(second.get("w")).not.toBe(first.get("w"));
    expect(second.get("other")).toBe(first.get("other"));
  });

  it("facing changes only past the velocity thresholds", () => {
    const { frames, mover } = setup();
    const [a, b, c] = layouts(working, waiting, resting);
    mover.update(a!, true);
    mover.update(b!, true);
    const path = walk(frames, mover, "w");
    mover.update(c!, true);
    path.push(...walk(frames, mover, "w"));
    const moving = path.filter((f) => f.moving);
    let changes = 0;
    for (let i = 1; i < moving.length; i++) {
      const before = moving[i - 1]!;
      const f = moving[i]!;
      const v = screen(f.gx - before.gx, f.gy - before.gy);
      if (f.flip !== before.flip) {
        changes++;
        // Turned the way it moves, and not back within eight frames.
        expect(f.flip ? v.x < 0 : v.x > 0, `flip at frame ${i}`).toBe(true);
        expect(moving.slice(i, i + 8).every((g) => g.flip === f.flip), `flip back after frame ${i}`).toBe(true);
      }
      if (f.facing !== before.facing) {
        changes++;
        expect(f.facing === "front" ? v.y > 0 : v.y < 0, `view at frame ${i}`).toBe(true);
        expect(moving.slice(i, i + 8).every((g) => g.facing === f.facing), `view back after frame ${i}`).toBe(true);
      }
    }
    expect(changes).toBeGreaterThan(0);
  });

  it("a glide ends in GLIDE_MS", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(working, waiting);
    mover.update(a!, false);
    const from = spotOf(a!, "w");
    const to = spotOf(b!, "w");
    mover.update(b!, false);
    const path = walk(frames, mover, "w");
    expect(path.length).toBe(Math.ceil(GLIDE_MS / 16));
    // A straight move: every point on the segment.
    for (const f of path) {
      const cross = (f.gx - from.gx) * (to.gy - from.gy) - (f.gy - from.gy) * (to.gx - from.gx);
      expect(Math.abs(cross)).toBeLessThan(1e-9);
      expect(f.stride).toBeNull();
    }
    expect(path[path.length - 1]).toMatchObject({ gx: to.gx, gy: to.gy, moving: false });
  });

  it("reduced motion places at once and fades nothing", () => {
    const { frames, mover, host } = setup();
    const [a, b, c] = layouts(working, board([card("w", "decision"), card("new", "office", { started: 2 })]), board([card("new", "office", { started: 2 })]));
    mover.update(a!, true);
    frames.reduced = true;
    mover.update(b!, true);
    expect(frames.pending).toBe(0);
    expect(host.getAttribute("data-moving")).toBe("0");
    expect(mover.figures()).toEqual(restingFrames(b!));
    mover.update(c!, true);
    expect(mover.figures()).toEqual(restingFrames(c!));
    expect(frames.pending).toBe(0);
  });

  it("the first layout places everyone at once", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const { frames, mover, host } = setup();
      const [layout] = layouts(b);
      mover.update(layout!, true);
      expect(mover.figures(), name).toEqual(restingFrames(layout!));
      expect(frames.pending, name).toBe(0);
      expect(mover.moving, name).toBe(false);
      expect(host.getAttribute("data-moving"), name).toBe("0");
    }
  });

  it("a new key walks in from the exit and a departed key walks out", () => {
    const { frames, mover } = setup();
    const [a, b, c] = layouts(working, board([card("w", "office"), card("other", "office", { started: 1 }), card("new", "office", { started: 2 })]), board([card("new", "office", { started: 2 }), card("other", "office", { started: 1 })]));
    const graph = navGraph(a!.columns);
    const exit = graph.nodes.get(graph.exit)!;
    mover.update(a!, true);
    mover.update(b!, true);
    const first = mover.figures().get("new")!;
    expect(first).toMatchObject({ gx: exit.gx, gy: exit.gy, opacity: 0, moving: true });
    const path = walk(frames, mover, "new");
    // Fades in over FADE_MS while it walks, then stays at 1.
    const faded = Math.ceil(FADE_MS / 16);
    expect(path[faded - 2]!.opacity).toBeLessThan(1);
    expect(path.slice(faded).every((f) => f.opacity === 1)).toBe(true);
    const to = spotOf(b!, "new");
    expect(path[path.length - 1]).toMatchObject({ gx: to.gx, gy: to.gy });

    // `w` ends: it walks to the exit, fades there, and is gone.
    mover.update(c!, true);
    const out = walk(frames, mover, "w");
    const atExit = out.findIndex((f) => f.gx === exit.gx && f.gy === exit.gy);
    expect(atExit).toBeGreaterThan(10);
    expect(out.slice(0, atExit).every((f) => f.opacity === 1)).toBe(true);
    expect(out[out.length - 1]!.opacity).toBeLessThan(1);
    expect(mover.figures().has("w")).toBe(false);
    expect([...mover.figures().keys()].sort()).toEqual(["new", "other"]);
  });

  it("a key that leaves and returns turns back on its way out", () => {
    const { frames, mover } = setup();
    const [a, b, c] = layouts(working, board([card("other", "office", { started: 1 })]), working);
    mover.update(a!, true);
    mover.update(b!, true);
    for (let i = 0; i < 20; i++) frames.step();
    mover.update(c!, true);
    const path = walk(frames, mover, "w");
    expect(path.every((f) => f.opacity === 1)).toBe(true);
    const home = spotOf(c!, "w");
    expect(path[path.length - 1]).toMatchObject({ gx: home.gx, gy: home.gy });
  });

  it("a new layout mid-walk re-routes from the current position", () => {
    const { frames, mover } = setup();
    const [a, b, c] = layouts(working, waiting, resting);
    mover.update(a!, true);
    mover.update(b!, true);
    for (let i = 0; i < 40; i++) frames.step();
    const there = mover.figures().get("w")!;
    expect(there.moving).toBe(true);
    mover.update(c!, true);
    // No jump: the next frames go on from where it was.
    expect(mover.figures().get("w")).toMatchObject({ gx: there.gx, gy: there.gy });
    const path = walk(frames, mover, "w");
    expect(Math.hypot(path[0]!.gx - there.gx, path[0]!.gy - there.gy)).toBeLessThanOrEqual(WALK.vmax * 0.016 + 1e-9);
    const seat = spotOf(c!, "w");
    const rest = restPoint(seat);
    expect(path[path.length - 1]).toMatchObject({ gx: seat.gx, gy: seat.gy, drawGx: rest.gx, drawGy: rest.gy, pose: "seated-facing" });
  });

  it("a seat is taken after a pause, sliding in", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(working, resting);
    mover.update(a!, true);
    mover.update(b!, true);
    const path = walk(frames, mover, "w");
    const arrived = path.findIndex((f) => !f.moving);
    const pause = path.slice(arrived).filter((f) => f.pose === "standing").length;
    expect(pause * 16).toBeGreaterThanOrEqual(WALK.sitPause * 1000 - 16);
    expect(pause * 16).toBeLessThanOrEqual(WALK.sitPause * 1000 + 16);
    // The slide moves the drawing a little each frame.
    for (let i = arrived + 1; i < path.length; i++) {
      const d = screen(path[i]!.drawGx - path[i - 1]!.drawGx, path[i]!.drawGy - path[i - 1]!.drawGy);
      expect(Math.hypot(d.x, d.y)).toBeLessThan(3);
    }
  });

  it("a seated figure walks off from its seat, without a jump", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(resting, waiting);
    mover.update(a!, true);
    const seated = mover.figures().get("w")!;
    expect(seated.pose).toBe("seated-facing");
    expect([seated.drawGx, seated.drawGy]).not.toEqual([seated.gx, seated.gy]);
    mover.update(b!, true);
    const start = mover.figures().get("w")!;
    expect([start.drawGx, start.drawGy]).toEqual([seated.drawGx, seated.drawGy]);
    const path = walk(frames, mover, "w");
    let previous = { x: seated.drawGx, y: seated.drawGy };
    for (const f of path) {
      expect(Math.hypot(f.drawGx - previous.x, f.drawGy - previous.y)).toBeLessThanOrEqual(WALK.vmax * 0.016 + 1e-9);
      previous = { x: f.drawGx, y: f.drawGy };
    }
  });

  it("a hidden page places every walker at once", () => {
    const { frames, mover, host } = setup();
    const [a, b] = layouts(working, board([card("w", "decision"), card("other", "office", { started: 1 }), card("new", "office", { started: 2 })]));
    mover.update(a!, true);
    mover.update(b!, true);
    for (let i = 0; i < 10; i++) frames.step();
    expect(host.getAttribute("data-moving")).toBe("1");
    const notified: ReadonlyMap<string, FigureFrame>[] = [];
    mover.subscribe((f) => notified.push(f));
    frames.hide();
    expect(mover.figures()).toEqual(withFlips(restingFrames(b!), mover));
    expect(notified).toHaveLength(1);
    expect(frames.pending).toBe(0);
    expect(host.getAttribute("data-moving")).toBe("0");
    // A layout while hidden places at once too.
    const [, , c] = layouts(working, waiting, resting);
    mover.update(c!, true);
    expect(mover.moving).toBe(false);
    expect(mover.figures().get("w")).toMatchObject({ pose: "seated-facing", opacity: 1 });
  });

  it("a frame gap above 0.25 s places every walker at once", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(working, waiting);
    mover.update(a!, true);
    mover.update(b!, true);
    frames.step();
    const before = mover.figures().get("w")!;
    // A gap just under the threshold only advances by the capped frame.
    frames.step(FRAME_GAP_S * 1000 - 10);
    const slow = mover.figures().get("w")!;
    expect(slow.moving).toBe(true);
    expect(Math.hypot(slow.gx - before.gx, slow.gy - before.gy)).toBeLessThanOrEqual(WALK.vmax * FRAME_CAP_S + 1e-9);
    frames.step(FRAME_GAP_S * 1000 + 10);
    expect(mover.figures()).toEqual(withFlips(restingFrames(b!), mover));
    expect(mover.moving).toBe(false);
  });

  it("the loop stops when nothing moves and data-moving follows", () => {
    const { frames, mover, host } = setup();
    const [a, b] = layouts(working, resting);
    const seen: string[] = [];
    new MutationObserver((records) => records.forEach((r) => seen.push(r.oldValue!))).observe(host, {
      attributes: true,
      attributeOldValue: true,
      attributeFilter: ["data-moving"],
    });
    mover.update(a!, true);
    expect(frames.pending).toBe(0);
    let frameCount = 0;
    mover.subscribe(() => frameCount++);
    mover.update(b!, true);
    expect(host.getAttribute("data-moving")).toBe("1");
    expect(frames.pending).toBe(1);
    const path = walk(frames, mover, "w");
    expect(frameCount).toBe(path.length);
    // Still "1" through the seat's pause and slide.
    expect(path[path.length - 1]!.pose).toBe("seated-facing");
    expect(host.getAttribute("data-moving")).toBe("0");
    expect(frames.pending).toBe(0);
    frames.step();
    expect(frameCount).toBe(path.length);
    // The same layout again starts nothing.
    mover.update(b!, true);
    expect(frames.pending).toBe(0);
    // Written twice, on start and on stop: "0" then "1" were replaced.
    return Promise.resolve().then(() => expect(seen).toEqual(["0", "1"]));
  });

  it("turning walking off mid-walk places the walkers", () => {
    const { frames, mover } = setup();
    const [a, b] = layouts(working, waiting);
    mover.update(a!, true);
    mover.update(b!, true);
    for (let i = 0; i < 10; i++) frames.step();
    mover.update(b!, false);
    expect(mover.moving).toBe(false);
    expect(mover.figures()).toEqual(withFlips(restingFrames(b!), mover));
  });
});

describe("mover: sub-agents (story 4.12, D-548)", () => {
  const agent = { agent_id: "a1", agent_type: "Explore" };
  const lead = (subagents: (typeof agent)[]) => board([card("lead", "office", { subagents })]);
  const helperOf = (mover: Mover) => [...mover.figures().values()].find((f) => f.helper) ?? null;
  /** Runs `ms` of 16 ms frames: a longer frame is a stall (D-502). */
  const run = (frames: Frames, ms: number) => {
    for (let left = ms; left > 0; left -= 16) frames.step(Math.min(16, left));
  };

  it("a sub-agent is placed at once on the first layout and never walks", () => {
    const { mover } = setup();
    const [l] = layouts(lead([agent]));
    mover.update(l!, true);
    expect(mover.moving).toBe(false);
    expect(mover.figures()).toEqual(restingFrames(l!));
    expect(helperOf(mover)).toMatchObject({ helper: true, moving: false, stride: null, pose: "standing", opacity: 1 });
  });

  it("a new sub-agent fades in where it stands, then nothing runs", () => {
    const { frames, mover, host } = setup();
    const [none, one] = layouts(lead([]), lead([agent]));
    mover.update(none!, true);
    mover.update(one!, true);
    expect(helperOf(mover)!.opacity).toBe(0);
    expect(host.getAttribute("data-moving")).toBe("1");
    const at = helperOf(mover)!;
    while (frames.pending > 0) frames.step();
    expect(helperOf(mover)).toEqual({ ...at, opacity: 1 });
    expect(host.getAttribute("data-moving")).toBe("0");
  });

  it("a stopped sub-agent holds, then fades out", () => {
    const { frames, mover } = setup();
    const [one, none] = layouts(lead([agent]), lead([]));
    mover.update(one!, true);
    mover.update(none!, true);
    run(frames, HELPER_HOLD_MS - 16);
    expect(helperOf(mover)!.opacity).toBe(1);
    run(frames, 16 + FADE_MS / 2);
    expect(helperOf(mover)!.opacity).toBeCloseTo(0.5, 1);
    run(frames, FADE_MS);
    expect(helperOf(mover)).toBeNull();
    expect(frames.pending).toBe(0);
  });

  it("a sub-agent that resumes during its hold or its fade is the same figure, its fade turned back", () => {
    const { frames, mover } = setup();
    const [one, none, again, gone, back] = layouts(lead([agent]), lead([]), lead([agent]), lead([]), lead([agent]));
    mover.update(one!, true);
    mover.update(none!, true);
    run(frames, HELPER_HOLD_MS / 2);
    mover.update(again!, true);
    expect(helperOf(mover)!.opacity).toBe(1);
    expect(mover.moving).toBe(false);
    mover.update(gone!, true);
    run(frames, HELPER_HOLD_MS + FADE_MS / 2);
    const faded = helperOf(mover)!.opacity;
    expect(faded).toBeGreaterThan(0);
    expect(faded).toBeLessThan(1);
    mover.update(back!, true);
    expect(helperOf(mover)!.opacity).toBe(faded);
    while (frames.pending > 0) frames.step();
    expect(helperOf(mover)!.opacity).toBe(1);
  });

  it("a hidden page or reduced motion places sub-agents and drops the stopped ones at once", () => {
    const { frames, mover } = setup();
    const [one, none] = layouts(lead([agent]), lead([]));
    mover.update(one!, true);
    mover.update(none!, true);
    frames.hide();
    expect(helperOf(mover)).toBeNull();
    const reduced = setup();
    reduced.mover.update(none!, true);
    reduced.frames.reduced = true;
    reduced.mover.update(one!, true);
    expect(helperOf(reduced.mover)!.opacity).toBe(1);
    reduced.mover.update(none!, true);
    expect(helperOf(reduced.mover)).toBeNull();
  });
});

/** `frames` with each figure's flip as the mover left it: a walk may end
 *  turned left. */
function withFlips(frames: Map<string, FigureFrame>, mover: Mover): Map<string, FigureFrame> {
  const now = mover.figures();
  return new Map([...frames].map(([key, f]) => [key, { ...f, flip: now.get(key)?.flip ?? f.flip }]));
}
