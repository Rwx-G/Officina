// Walking (story 4.10; AD-64, AD-70, AD-171, AD-172, D-469, D-483, D-502):
// the prototype v2's flicker check on the real window, arrivals and
// departures through the entrance, a reload, walking off and reduced
// motion. The clock is paused after the scene opens and each frame is
// stepped with `page.clock.runFor(16)`, which fires one fake
// `requestAnimationFrame`. Boards are `officina board --json` output;
// places come from `layoutScene`, imported, never retyped.
import { expect, test, type Page } from "@playwright/test";
import { answerOrchestrator, loadBoard, openWalkingScene, sceneSettled, sceneView } from "./fixtures";
import { DEPTH_BANDS, projector } from "../src/lib/scene/geometry";
import { ORCHESTRATOR_KEY, layoutScene, liveCards, type SceneLayout } from "../src/lib/scene/layout";
import { STORAGE_KEY, emptyMemory, parse, prune, type SceneMemory } from "../src/lib/scene/memory";
import { BRIEFING_DWELL_MS, GLIDE_MS } from "../src/lib/scene/mover";
import { briefingSpot, navGraph, staffSpots } from "../src/lib/scene/places";
import type { Board } from "../src/lib/generated/Board";
import type { OrchestratorView } from "../src/lib/generated/OrchestratorView";
import type { Card } from "../src/lib/generated/Card";

const FRAME_MS = 16;
/** The prototype's flicker check: 72 frames of a walk. */
const CHECKED_FRAMES = 72;
/** A walk across the floor ends well within this many frames. */
const MAX_FRAMES = 1_500;
/** The largest step of a figure per frame, in logical px (AD-70). */
const MAX_STEP_PX = 3;
/** A facing change does not come back within this many frames (AD-70). */
const FACING_HOLD = 8;
/** Frames between samples where a case checks no single frame: a fade
 *  still spans several samples. */
const SPARSE = 3;

const scene = (page: Page) => page.getByTestId("scene");

function push(page: Page, board: Board): Promise<void> {
  return page.evaluate((b) => window.__officinaPushBoard!(b), board);
}

function layouts(...boards: Board[]): SceneLayout[] {
  let memory: SceneMemory = emptyMemory(boards[0]!.floor);
  return boards.map((board) => {
    const layout = layoutScene({ board }, memory);
    memory = layout.memory;
    return layout;
  });
}

/** `board` with `key` moved by `edit` into another column. */
function moved(board: Board, key: string, edit: (card: Card) => Card): Board {
  const next = structuredClone(board);
  let card: Card | null = null;
  for (const list of Object.values(next.columns)) {
    const at = list.findIndex((c) => c.key === key);
    if (at >= 0) card = list.splice(at, 1)[0]!;
  }
  const after = edit(card!);
  next.columns[after.column].push(after);
  next.waiting_count = next.columns.needs_you.length;
  return next;
}

/** A Working session that now waits on a permission prompt. */
const startsWaiting = (board: Board, key: string) =>
  moved(board, key, (c) => ({
    ...c,
    column: "needs_you",
    waiting_for: "permission prompt",
    scene: { ...c.scene!, zone: "decision", waiting_since_ms: board.generated_at_ms, alert: null, approval: false },
  }));

/** A Needs you or Working session that moves to Working in its office. */
const startsWorking = (board: Board, key: string) =>
  moved(board, key, (c) => ({ ...c, column: "working", waiting_for: null, scene: { ...c.scene!, zone: "office", alert: null, approval: false } }));

/** A session that ended: out of the live set, into Done. */
const ends = (board: Board, key: string) => moved(board, key, (c) => ({ ...c, column: "done", waiting_for: null, scene: null }));

interface Sample {
  /** The node is the one marked when the walk began. */
  same: boolean;
  x: number;
  y: number;
  opacity: string | null;
  poses: string[];
  views: string[];
  flip: string | null;
  animations: number;
  moving: string | null;
}

/** Marks `key`'s figure node, so later samples can tell it is the same. */
function mark(page: Page, key: string): Promise<void> {
  return scene(page).evaluate((root, k) => {
    (window as unknown as { __walker: Element | null }).__walker = root.querySelector(`[data-figure="${k}"]`);
  }, key);
}

/** `key`'s figure as drawn now, or `null` when it has none. */
function sample(page: Page, key: string): Promise<Sample | null> {
  return scene(page).evaluate((root, k) => {
    const node = root.querySelector(`[data-figure="${k}"]`);
    if (!node) return null;
    const shown = (el: Element) => el.getAttribute("display") !== "none";
    const stand = node.querySelector('[data-part="stand"]')!;
    const poses = Array.from(node.querySelectorAll('[data-part="stand"], [data-part="seated-facing"], [data-part="seated-working"]'))
      .filter(shown)
      .map((g) => g.getAttribute("data-part")!);
    const views = shown(stand) ? [...new Set(Array.from(stand.querySelectorAll("[data-view]")).filter(shown).map((v) => v.getAttribute("data-view")!))] : [];
    const [, x, y] = /translate\(([-\d.e]+) ([-\d.e]+)\)/.exec(node.getAttribute("transform") ?? "translate(NaN NaN)")!;
    const animations = document.getAnimations().filter((a) => {
      const target = (a.effect as KeyframeEffect | null)?.target;
      return target instanceof Element && target.closest("[data-figure]") !== null;
    }).length;
    return {
      same: node === (window as unknown as { __walker: Element | null }).__walker && node.isConnected,
      x: Number(x),
      y: Number(y),
      opacity: node.getAttribute("opacity"),
      poses,
      views,
      flip: node.querySelector('[data-part="flip"]')!.getAttribute("transform"),
      animations,
      moving: root.getAttribute("data-moving"),
    };
  }, key);
}

/** Steps frames until the scene stops moving, sampling `key` after each,
 *  or after every `every` frames where a case needs no frame-by-frame
 *  check (each sample costs the page a round trip). */
async function stepUntilSettled(page: Page, key: string, every = 1): Promise<(Sample | null)[]> {
  const out: (Sample | null)[] = [];
  for (let i = 0; i < MAX_FRAMES; i += every) {
    await page.clock.runFor(FRAME_MS * every);
    const s = await sample(page, key);
    out.push(s);
    if (s?.moving === "0" || (s === null && (await scene(page).getAttribute("data-moving")) === "0")) break;
  }
  expect(await scene(page).getAttribute("data-moving"), "still moving").toBe("0");
  return out;
}

/** Opens `board` in Scene mode and pauses the clock, so each frame is
 *  stepped by the test. */
async function openPaused(page: Page, board: Board): Promise<number> {
  await openWalkingScene(page, board);
  await page.clock.pauseAt(board.generated_at_ms + 60_000);
  return Number(await scene(page).getAttribute("data-scale"));
}

const snapped = (v: number, scale: number) => Math.round(v * scale) / scale;

/** A working session of scene-b and the board where it waits. */
function waitingCase(): { board: Board; next: Board; key: string } {
  const board = loadBoard("scene-b");
  const key = board.columns.working[board.columns.working.length - 1]!.key;
  return { board, next: startsWaiting(board, key), key };
}

test.describe("walking", () => {
  test("a walking figure stays one stable node", async ({ page }) => {
    const { board, next, key } = waitingCase();
    const scale = await openPaused(page, board);
    await mark(page, key);
    const start = (await sample(page, key))!;
    await push(page, next);
    const frames = await stepUntilSettled(page, key);
    expect(frames.length).toBeGreaterThan(CHECKED_FRAMES);
    let previous = start;
    const facings: string[] = [];
    let largest = 0;
    let moved = 0;
    for (const [i, s] of frames.entries()) {
      expect(s, `frame ${i}`).not.toBeNull();
      expect(s!.same, `frame ${i}: another node`).toBe(true);
      expect(s!.poses, `frame ${i}`).toHaveLength(1);
      if (s!.poses[0] === "stand") expect(s!.views, `frame ${i}`).toHaveLength(1);
      expect(s!.opacity ?? "1", `frame ${i}`).toBe("1");
      const step = Math.hypot(s!.x - previous.x, s!.y - previous.y);
      largest = Math.max(largest, step);
      if (step > 0) moved++;
      expect(step, `frame ${i}`).toBeLessThan(MAX_STEP_PX);
      facings.push(`${s!.views.join()}|${s!.flip}`);
      previous = s!;
    }
    expect(moved).toBeGreaterThan(CHECKED_FRAMES / 2);
    // A facing that changes does not come back within FACING_HOLD frames.
    for (let i = 1; i < facings.length; i++) {
      if (facings[i] === facings[i - 1]) continue;
      const back = facings.slice(i, i + FACING_HOLD).indexOf(facings[i - 1]!);
      expect(back, `facing ${facings[i - 1]} came back at frame ${i + back}`).toBe(-1);
    }
    // It ends on its place in the decision office.
    const [, after] = layouts(board, next);
    const at = after!.employees.find((e) => e.key === key)!.at!;
    expect([previous.x, previous.y]).toEqual([snapped(at.x, scale), snapped(at.y, scale)]);
  });

  // Story 4.16 (D-550): layer 3 is split into depth bands, each its own
  // <svg>, so a walk dirties the band it walks in and Chromium keeps the
  // others' recorded paint. Every write of a walk frame is inside the
  // walker's node, or moves figure nodes among their neighbours (the depth
  // sort); a frame touches one band, two when the walker crosses from one
  // to the next.
  test("a walk writes only the walker, in one depth band at a time", async ({ page }) => {
    const { board, next, key } = waitingCase();
    await openPaused(page, board);
    const bands = await scene(page).evaluate((root) =>
      Array.from(root.querySelectorAll('[data-layer="objects"] > svg'), (svg) => svg.querySelectorAll("[data-static]").length),
    );
    expect(bands).toHaveLength(DEPTH_BANDS);
    const statics = bands.reduce((a, b) => a + b, 0);
    for (const n of bands) expect(n).toBeLessThanOrEqual(Math.ceil(statics / DEPTH_BANDS));
    await push(page, next);
    await scene(page).evaluate((root, k) => {
      const out: { band: number; inWalker: boolean; figuresMoved: boolean; layer: string | null }[] = [];
      const svgs = Array.from(root.querySelectorAll('[data-layer="objects"] > svg'));
      const walker = root.querySelector(`[data-figure="${k}"]`)!;
      new MutationObserver((list) => {
        for (const r of list) {
          const target = r.target as Element;
          out.push({
            band: svgs.findIndex((svg) => svg.contains(target)),
            inWalker: walker.contains(target),
            figuresMoved: r.type === "childList" && [...Array.from(r.addedNodes), ...Array.from(r.removedNodes)].every((n) => n instanceof Element && n.hasAttribute("data-figure")),
            layer: target.closest("[data-layer]")?.getAttribute("data-layer") ?? null,
          });
        }
      }).observe(root, { subtree: true, childList: true, attributes: true });
      (window as unknown as { __bandRecords: typeof out }).__bandRecords = out;
    }, key);
    const take = () =>
      page.evaluate(() => {
        const w = window as unknown as { __bandRecords: { band: number; inWalker: boolean; figuresMoved: boolean; layer: string | null }[] };
        return w.__bandRecords.splice(0);
      });
    let frames = 0;
    let oneBand = 0;
    for (let i = 0; i < MAX_FRAMES && (await scene(page).getAttribute("data-moving")) !== "0"; i++) {
      await page.clock.runFor(FRAME_MS);
      const records = (await take()).filter((r) => r.layer === "objects");
      for (const r of records) expect(r.inWalker || r.figuresMoved, `frame ${i}: a write outside the walker`).toBe(true);
      const touched = new Set(records.map((r) => r.band));
      expect(touched.size, `frame ${i}`).toBeLessThanOrEqual(2);
      frames++;
      if (touched.size <= 1) oneBand++;
    }
    expect(frames).toBeGreaterThan(CHECKED_FRAMES);
    expect(oneBand / frames).toBeGreaterThan(0.9);
  });

  test("no CSS animation runs inside a walking figure", async ({ page }) => {
    const { board, next, key } = waitingCase();
    await openPaused(page, board);
    await push(page, next);
    for (let i = 0; i < CHECKED_FRAMES; i++) {
      await page.clock.runFor(FRAME_MS);
      const s = (await sample(page, key))!;
      expect(s.moving, `frame ${i}`).toBe("1");
      expect(s.animations, `frame ${i}`).toBe(0);
    }
  });

  test("the frame loop stops after arrival", async ({ page }) => {
    const { board, next, key } = waitingCase();
    await openPaused(page, board);
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
    await push(page, next);
    await page.clock.runFor(FRAME_MS);
    await expect(scene(page)).toHaveAttribute("data-moving", "1");
    await stepUntilSettled(page, key);
    // Nothing is written afterwards: the loop no longer runs. The arrived
    // figure's halo may still take its delay when the browser starts its
    // animation (D-522), an event of the halo, not of the loop.
    await scene(page).evaluate((root) => {
      const records: MutationRecord[] = [];
      const haloStart = (r: MutationRecord) => r.attributeName === "style" && (r.target as Element).hasAttribute("data-halo");
      new MutationObserver((list) => records.push(...list.filter((r) => !haloStart(r)))).observe(root, { subtree: true, childList: true, attributes: true });
      (window as unknown as { __walkRecords: MutationRecord[] }).__walkRecords = records;
    });
    await page.clock.runFor(2_000);
    expect(await page.evaluate(() => (window as unknown as { __walkRecords: MutationRecord[] }).__walkRecords.length)).toBe(0);
  });

  test("a new session walks in from the entrance and an ended one walks out", async ({ page }) => {
    const board = loadBoard("scene-b");
    const scale = await openPaused(page, board);
    // A new session in the project of the first Working one.
    const template = board.columns.working[0]!;
    const arrived = { ...structuredClone(template), key: `${template.key}-new`, session_id: `${template.session_id}-new`, started_at_ms: board.generated_at_ms };
    const withNew = structuredClone(board);
    withNew.columns.working.push(arrived);
    const [, layout] = layouts(board, withNew);
    const graph = navGraph(layout!.columns);
    const exitNode = graph.nodes.get(graph.exit)!;
    const exit = projector(layout!.columns)(exitNode.gx, exitNode.gy);
    await push(page, withNew);
    await page.clock.runFor(FRAME_MS);
    const first = (await sample(page, arrived.key))!;
    expect(Math.hypot(first.x - exit.x, first.y - exit.y), "appears at the entrance").toBeLessThan(MAX_STEP_PX);
    expect(Number(first.opacity)).toBeLessThan(1);
    await mark(page, arrived.key);
    const walkIn = await stepUntilSettled(page, arrived.key, SPARSE);
    expect(walkIn.every((s) => s!.same)).toBe(true);
    const last = walkIn[walkIn.length - 1]!;
    expect(last.opacity).toBeNull();
    const at = layout!.employees.find((e) => e.key === arrived.key)!.at!;
    expect([last.x, last.y]).toEqual([snapped(at.x, scale), snapped(at.y, scale)]);

    // A session of the same office ends: it walks out and fades at the exit.
    const leaving = template.key;
    await mark(page, leaving);
    await push(page, ends(withNew, leaving));
    const walkOut = await stepUntilSettled(page, leaving, SPARSE);
    expect(walkOut[walkOut.length - 1]).toBeNull();
    const shown = walkOut.filter((s): s is Sample => s !== null);
    expect(shown.every((s) => s.same)).toBe(true);
    const atExit = shown.findIndex((s) => Math.hypot(s.x - exit.x, s.y - exit.y) < 1);
    expect(atExit, "reaches the entrance").toBeGreaterThan(10);
    expect(shown.slice(0, atExit).every((s) => s.opacity === null)).toBe(true);
    expect(shown.slice(atExit + 1).some((s) => Number(s.opacity) < 1)).toBe(true);
  });

  test("a reload places everyone at once", async ({ page }) => {
    const { board, next } = waitingCase();
    await openWalkingScene(page, board);
    await push(page, next);
    await page.addInitScript((b) => {
      window.__officinaBoard = b;
    }, next);
    const stored = await page.evaluate((k) => localStorage.getItem(k), STORAGE_KEY);
    await page.reload();
    await expect(scene(page)).toBeVisible();
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
    await sceneSettled(page);
    const scale = Number(await scene(page).getAttribute("data-scale"));
    const layout = layoutScene({ board: next }, prune(parse(stored), next));
    for (const e of layout.employees.filter((x) => x.at !== null)) {
      const s = (await sample(page, e.key))!;
      expect([s.x, s.y], e.key).toEqual([snapped(e.at!.x, scale), snapped(e.at!.y, scale)]);
      expect(s.opacity, e.key).toBeNull();
    }
  });

  test("walking off glides in under 400 ms", async ({ page }) => {
    const { board, next, key } = waitingCase();
    const scale = await openPaused(page, board);
    await page.getByTestId("walking-toggle").click();
    await expect(page.getByTestId("walking-toggle")).toHaveAttribute("aria-pressed", "false");
    const start = (await sample(page, key))!;
    await push(page, next);
    const frames = await stepUntilSettled(page, key);
    expect(frames.length * FRAME_MS).toBeLessThan(400);
    expect(frames.length * FRAME_MS).toBeGreaterThanOrEqual(GLIDE_MS);
    const [, after] = layouts(board, next);
    const at = after!.employees.find((e) => e.key === key)!.at!;
    const end = [snapped(at.x, scale), snapped(at.y, scale)] as const;
    // No route: every frame on the straight line from start to end.
    for (const s of frames) {
      const cross = (s!.x - start.x) * (end[1] - start.y) - (s!.y - start.y) * (end[0] - start.x);
      const length = Math.hypot(end[0] - start.x, end[1] - start.y);
      expect(Math.abs(cross) / length).toBeLessThan(1.5);
    }
    expect([frames.at(-1)!.x, frames.at(-1)!.y]).toEqual(end);
  });

  test("reduced motion places at once", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const { board, next, key } = waitingCase();
    const scale = await openPaused(page, board);
    // A new session too: it appears at its place, unfaded.
    const template = board.columns.working[0]!;
    const arrived = { ...structuredClone(template), key: `${template.key}-new`, session_id: `${template.session_id}-new` };
    next.columns.working.push(arrived);
    await push(page, next);
    await page.clock.runFor(FRAME_MS);
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
    const [, after] = layouts(board, next);
    for (const k of [key, arrived.key]) {
      const at = after!.employees.find((e) => e.key === k)!.at!;
      const s = (await sample(page, k))!;
      expect([s.x, s.y], k).toEqual([snapped(at.x, scale), snapped(at.y, scale)]);
      expect(s.opacity, k).toBeNull();
    }
  });

  test("turning walking off mid-walk places the walkers", async ({ page }) => {
    const { board, next, key } = waitingCase();
    const scale = await openPaused(page, board);
    await push(page, next);
    for (let i = 0; i < 20; i++) await page.clock.runFor(FRAME_MS);
    await expect(scene(page)).toHaveAttribute("data-moving", "1");
    await page.getByTestId("walking-toggle").click();
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
    const [, after] = layouts(board, next);
    const at = after!.employees.find((e) => e.key === key)!.at!;
    const s = (await sample(page, key))!;
    expect([s.x, s.y]).toEqual([snapped(at.x, scale), snapped(at.y, scale)]);
  });

  test("a hidden window places every walker at once", async ({ page }) => {
    const { board, next, key } = waitingCase();
    const scale = await openPaused(page, board);
    await push(page, next);
    for (let i = 0; i < 20; i++) await page.clock.runFor(FRAME_MS);
    await page.evaluate(() => {
      Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
    const [, after] = layouts(board, next);
    const at = after!.employees.find((e) => e.key === key)!.at!;
    const s = (await sample(page, key))!;
    expect([s.x, s.y]).toEqual([snapped(at.x, scale), snapped(at.y, scale)]);
  });
});

// Story 5.18 AC 7 (D-568): a work order bound while the orchestrator is
// live is briefed at reception's counter once, on the `scene-orchestrator`
// board and view; its order is first shown unbound, then bound.
test.describe("the briefing walk", () => {
  function briefingCase(): { board: Board; before: OrchestratorView; after: OrchestratorView; key: string } {
    const board = loadBoard("scene-orchestrator");
    const after = sceneView();
    const live = liveCards(board);
    const order = after.work_orders.find((o) => o.bound_at_ms !== null && live.some((c) => c.session_id === o.session?.session_id))!;
    const key = live.find((c) => c.session_id === order.session!.session_id)!.key;
    const before = { ...after, work_orders: after.work_orders.map((o) => (o.id === order.id ? { ...o, bound_at_ms: null, bg_id: null, session: null } : o)) };
    return { board, before, after, key };
  }
  const pushView = (page: Page, view: OrchestratorView) => page.evaluate((v) => window.__officinaPushOrchestrator!(v), view);

  test("a bound work order is briefed at the counter once, then both return", async ({ page }) => {
    const { board, before, after, key } = briefingCase();
    await answerOrchestrator(page, before);
    const scale = await openPaused(page, board);
    const home = { w: (await sample(page, key))!, o: (await sample(page, ORCHESTRATOR_KEY))! };
    expect(home.o).not.toBeNull();
    const columns = Number(await scene(page).getAttribute("data-columns"));
    const point = (s: { gx: number; gy: number }) => {
      const at = projector(columns)(s.gx, s.gy);
      return [snapped(at.x, scale), snapped(at.y, scale)];
    };
    const visitor = point(briefingSpot());
    const counter = point(staffSpots().counter);
    await pushView(page, after);
    const both: { w: Sample; o: Sample }[] = [];
    for (let i = 0; i < MAX_FRAMES; i += SPARSE) {
      await page.clock.runFor(FRAME_MS * SPARSE);
      both.push({ w: (await sample(page, key))!, o: (await sample(page, ORCHESTRATOR_KEY))! });
      if (both.at(-1)!.w.moving === "0") break;
    }
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
    const atCounter = both.filter(({ w, o }) => w.x === visitor[0] && w.y === visitor[1] && o.x === counter[0] && o.y === counter[1]);
    // The last samples there may catch them setting off, not yet a snapped
    // pixel away and already turned to walk back: the dwell ends there.
    const setOff = atCounter.findIndex(({ o }) => o.views[0] !== "front");
    const there = setOff < 0 ? atCounter : atCounter.slice(0, setOff);
    expect(atCounter.length - there.length).toBeLessThanOrEqual(2);
    expect(there.length, "both at the counter together").toBeGreaterThan(0);
    for (const { w, o } of there) {
      expect(w.poses).toEqual(["stand"]);
      expect(o.poses).toEqual(["stand"]);
      // The employee's back to the author, facing the counter; the
      // orchestrator across it, facing the author.
      expect(w.views).toEqual(["back"]);
      expect(o.views).toEqual(["front"]);
    }
    // They hold BRIEFING_DWELL_MS, sampled every SPARSE frames.
    const held = there.length * SPARSE * FRAME_MS;
    expect(held).toBeGreaterThanOrEqual(BRIEFING_DWELL_MS - 2 * SPARSE * FRAME_MS);
    expect(held).toBeLessThanOrEqual(BRIEFING_DWELL_MS + 2 * SPARSE * FRAME_MS);
    // Then both are back, as they sat.
    const end = { w: (await sample(page, key))!, o: (await sample(page, ORCHESTRATOR_KEY))! };
    for (const who of ["w", "o"] as const) {
      expect([end[who].x, end[who].y, end[who].poses], who).toEqual([home[who].x, home[who].y, home[who].poses]);
    }
    // Once: the same binding again moves nobody.
    await pushView(page, structuredClone(after));
    await page.clock.runFor(FRAME_MS * 10);
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
  });

  test("with walking off a bound order goes straight to its place", async ({ page }) => {
    const { board, before, after, key } = briefingCase();
    await answerOrchestrator(page, before);
    await openPaused(page, board);
    await page.getByTestId("walking-toggle").click();
    await expect(page.getByTestId("walking-toggle")).toHaveAttribute("aria-pressed", "false");
    const home = { w: (await sample(page, key))!, o: (await sample(page, ORCHESTRATOR_KEY))! };
    await pushView(page, after);
    for (let i = 0; i < 20; i++) {
      await page.clock.runFor(FRAME_MS);
      await expect(scene(page)).toHaveAttribute("data-moving", "0");
    }
    const end = { w: (await sample(page, key))!, o: (await sample(page, ORCHESTRATOR_KEY))! };
    for (const who of ["w", "o"] as const) expect([end[who].x, end[who].y], who).toEqual([home[who].x, home[who].y]);
  });
});
