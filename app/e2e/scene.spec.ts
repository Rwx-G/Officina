// The scene drawn in place (story 4.7; AD-59, AD-61, AD-64, AD-70, AD-171,
// D-497): every live session of every fixture board drawn once, a changed
// board placing figures at once, nothing written while the payloads are
// unchanged, the scale following the sessions area, eight projects on the
// owner's screen (D-520, D-525), and the light and dark baselines (D-119,
// D-508).
// Boards are `officina board --json` output (AD-17); expected places come
// from `layoutScene`, imported, never retyped.
import { existsSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import {
  PENDING_LIMIT,
  SCREENSHOT_MAX_DIFF_RATIO,
  answerOrchestrator,
  calls,
  loadBoard,
  loadRelay,
  openScene,
  openWalkingScene,
  sceneBoardOrchestrator,
  sceneSettled,
  sceneView,
  transitionsDone,
  withPending,
} from "./fixtures";
import { fixtureOf, fixturesOf } from "./fixture-index";
import { beaconPair } from "../src/lib/alert";
import { HALO_BREATHING_MS, HALO_CYCLE_MS } from "../src/lib/halo";
import { MIN_SCALE, TARGET_CSS_MIN } from "../src/lib/scene/geometry";
import { ORCHESTRATOR_KEY, SUBAGENT_SCALE, layoutScene, liveCards, nearestTarget, type Direction, type SceneLayout, type Target } from "../src/lib/scene/layout";
import { STORAGE_KEY, emptyMemory, type SceneMemory } from "../src/lib/scene/memory";
import { FADE_MS, HELPER_HOLD_MS } from "../src/lib/scene/mover";
import { ZONE_WORDS } from "../src/lib/scene/text";
import { causeText } from "../src/lib/alert";
import { outcomeText } from "../src/lib/outcome";
import { hours } from "../src/lib/time";
import { denialsText } from "../src/lib/relay-text";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";
import type { FocusOutcome } from "../src/lib/generated/FocusOutcome";
import type { MeetingView } from "../src/lib/generated/MeetingView";

const scene = (page: Page) => page.getByTestId("scene");

function push(page: Page, board: Board): Promise<void> {
  return page.evaluate((b) => window.__officinaPushBoard!(b), board);
}

/** The layouts a window shows for `boards`, one after the other, from a
 *  fresh memory (a new page has empty storage). */
function layouts(...boards: Board[]): SceneLayout[] {
  let memory: SceneMemory = emptyMemory(boards[0]!.floor);
  return boards.map((board) => {
    const layout = layoutScene({ board }, memory);
    memory = layout.memory;
    return layout;
  });
}

interface FigureState {
  transform: string;
  /** The pose groups shown. */
  shown: string[];
}

/** Every session's figure's transform and shown pose group, by card key;
 *  sub-agents' figures are `helpers`'. */
function figures(page: Page): Promise<Record<string, FigureState>> {
  return scene(page).evaluate((root) => {
    const out: Record<string, FigureState> = {};
    for (const node of Array.from(root.querySelectorAll("[data-figure]:not([data-helper])"))) {
      const shown = Array.from(node.querySelectorAll('[data-part="stand"], [data-part="seated-facing"], [data-part="seated-working"]'))
        .filter((group) => group.getAttribute("display") !== "none")
        .map((group) => group.getAttribute("data-part")!);
      out[node.getAttribute("data-figure")!] = { transform: node.getAttribute("transform") ?? "", shown };
    }
    return out;
  });
}

/** The translate a figure at `at` gets at scale `scale` (device pixel
 *  ratio 1 in this suite). */
function placed(at: { x: number; y: number }, scale: number): [number, number] {
  return [Math.round(at.x * scale) / scale, Math.round(at.y * scale) / scale];
}

function parse(transform: string): [number, number] {
  const [, x, y] = /translate\(([-\d.e]+) ([-\d.e]+)\)/.exec(transform)!;
  return [Number(x), Number(y)];
}

/** Shows `board` in a fresh window: the next load answers it, and the
 *  scene's stored memory is dropped first, as on a new workstation. */
async function reopenScene(page: Page, board: Board): Promise<void> {
  await page.addInitScript((b) => {
    window.__officinaBoard = b;
  }, board);
  await page.evaluate((key) => localStorage.removeItem(key), STORAGE_KEY);
  await page.reload();
  await sceneSettled(page);
}

/** The running animations inside the scene. */
function runningInScene(page: Page): Promise<number> {
  return page.evaluate(
    () =>
      document.getAnimations().filter((a) => {
        const target = (a.effect as KeyframeEffect | null)?.target;
        return a.playState === "running" && target instanceof Element && target.closest("[data-testid=scene]") !== null;
      }).length,
  );
}

/** How long the beacon has left to breathe at first sight, in the case
 *  that waits for its end in real time. */
const BEACON_LEFT_MS = 6_000;

const POSE_PART = { standing: "stand", "seated-facing": "seated-facing", "seated-working": "seated-working" } as const;

/** `board` with one card moved to Working, in its office. */
function startsWorking(board: Board, key: string): Board {
  const next = structuredClone(board);
  let moved = null;
  for (const list of Object.values(next.columns)) {
    const at = list.findIndex((c) => c.key === key);
    if (at >= 0) moved = list.splice(at, 1)[0]!;
  }
  next.columns.working.push({ ...moved!, column: "working", waiting_for: null, scene: { ...moved!.scene!, zone: "office", alert: null, approval: false } });
  next.waiting_count = next.columns.needs_you.length;
  return next;
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The targets' boxes relative to the scene element, in CSS px, read in
 *  one evaluation so no scroll falls between. */
function boxes(page: Page): Promise<{ scene: Box; targets: { id: string; box: Box }[] }> {
  return scene(page).evaluate((root) => {
    const origin = root.getBoundingClientRect();
    const rel = (el: Element) => {
      const r = el.getBoundingClientRect();
      return { x: r.left - origin.left, y: r.top - origin.top, w: r.width, h: r.height };
    };
    return {
      scene: { x: 0, y: 0, w: origin.width, h: origin.height },
      targets: Array.from(root.querySelectorAll("[data-target]")).map((el) => ({ id: el.getAttribute("data-target")!, box: rel(el) })),
    };
  });
}

const meets = (a: Box, b: Box) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
/** A CSS box compared with a minimum, with D-104's rounding allowance. */
const atLeast = (value: number, min: number) => value >= min - 0.01;

async function expectTargetsAtLeastMinimum(page: Page): Promise<void> {
  for (const { id, box } of (await boxes(page)).targets) {
    expect(atLeast(box.w, TARGET_CSS_MIN) && atLeast(box.h, TARGET_CSS_MIN), `${id} is ${box.w} x ${box.h}`).toBe(true);
  }
}

/** Every sub-agent's figure drawn: its key, transform and opacity. */
function helpers(page: Page): Promise<{ key: string; transform: string; opacity: string | null }[]> {
  return scene(page).evaluate((root) =>
    Array.from(root.querySelectorAll("[data-helper]")).map((node) => ({
      key: node.getAttribute("data-figure")!,
      transform: node.getAttribute("transform") ?? "",
      opacity: node.getAttribute("opacity"),
    })),
  );
}

test.describe("the scene draws every live session of each fixture", () => {
  for (const { name, data: board } of fixturesOf<Board>("board")) {
    test(name, async ({ page }) => {
      await openScene(page, board);
      const [layout] = layouts(board);
      const drawn = Object.keys(await figures(page))
        .filter((key) => key !== ORCHESTRATOR_KEY)
        .sort();
      expect(drawn).toEqual(layout!.employees.filter((e) => e.spot !== null).map((e) => e.key).sort());
      // The orchestrator is drawn while the board lists it (D-567).
      await expect(scene(page).locator("[data-orchestrator]")).toHaveCount(board.orchestrator ? 1 : 0);
      expect((await helpers(page)).map((h) => h.key).sort()).toEqual(layout!.helpers.map((h) => h.key).sort());
      const listed = layout!.groups.flatMap((g) => g.keys);
      const all = [...drawn, ...listed].sort();
      expect(all).toEqual(liveCards(board).map((c) => c.key).sort());
      expect(new Set(all).size).toBe(all.length);
    });
  }
});

test.describe("scene", () => {
  test("a changed board places the figure at its new place at once", async ({ page }) => {
    // Under reduced motion: otherwise the figure walks there (story 4.10,
    // walk.spec.ts).
    await page.emulateMedia({ reducedMotion: "reduce" });
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const scale = Number(await scene(page).getAttribute("data-scale"));
    const mover = liveCards(board).find((c) => c.scene?.zone === "lounge")!;
    const next = startsWorking(board, mover.key);
    const [before, after] = layouts(board, next);
    const was = await figures(page);
    await push(page, next);
    const target = after!.employees.find((e) => e.key === mover.key)!;
    expect(target.spot).not.toBeNull();
    const [x, y] = placed(target.at!, scale);
    // The first state seen after the push is the final one: no in-between.
    await expect.poll(async () => (await figures(page))[mover.key]!.transform).not.toBe(was[mover.key]!.transform);
    const now = await figures(page);
    const [nx, ny] = parse(now[mover.key]!.transform);
    expect(Math.abs(nx - x)).toBeLessThan(1e-6);
    expect(Math.abs(ny - y)).toBeLessThan(1e-6);
    expect(now[mover.key]!.shown).toEqual([POSE_PART[target.pose]]);
    // Nothing else: a figure whose place stayed keeps its transform.
    for (const e of before!.employees) {
      const later = after!.employees.find((n) => n.key === e.key);
      if (!later || e.key === mover.key || !e.at || JSON.stringify(e.at) !== JSON.stringify(later.at)) continue;
      expect(now[e.key]!.transform, e.key).toBe(was[e.key]!.transform);
    }
  });

  // Story 4.12 (AD-176, D-476, D-548): a sub-agent fades in beside its
  // lead's desk, small and without a cap, and fades out after its hold
  // once its stop reaches the board; it is the same figure if it resumes
  // during the hold.
  test("a sub-agent appears beside its lead and leaves when it stops", async ({ page }) => {
    const board = loadBoard("scene-subagents");
    await openScene(page, board);
    await page.clock.pauseAt(board.generated_at_ms + 60_000);
    const lead = board.columns.working.find((c) => c.subagents.length === 0)!;
    const withOne = structuredClone(board);
    withOne.columns.working.find((c) => c.key === lead.key)!.subagents = [{ agent_id: "agent-new", agent_type: "Explore" }];
    const [, laid] = layouts(board, withOne);
    const helper = laid!.helpers.find((h) => h.lead === lead.key)!;
    expect(helper).toBeDefined();
    await push(page, withOne);
    const node = scene(page).locator(`[data-figure="${helper.key}"]`);
    await expect(node).toHaveAttribute("data-helper", "");
    await expect(node).toHaveAttribute("opacity", /^0(\.\d+)?$/);
    await expect(node.locator('[data-part="rig"]')).toHaveAttribute("transform", `scale(${SUBAGENT_SCALE})`);
    await page.clock.runFor(FADE_MS + 32);
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
    await expect(node).not.toHaveAttribute("opacity", /.*/);
    const scale = Number(await scene(page).getAttribute("data-scale"));
    const [x, y] = parse((await node.getAttribute("transform"))!);
    const [ex, ey] = placed(helper.at, scale);
    expect(Math.abs(x - ex)).toBeLessThan(1e-6);
    expect(Math.abs(y - ey)).toBeLessThan(1e-6);
    // Its file lists it by type.
    await scene(page).locator(`[data-target="figure:${lead.key}"]`).click();
    await expect(page.getByTestId("file-subagents")).toHaveText("Explore");
    await page.keyboard.press("Escape");

    // Stopped: it holds, then fades; back during the hold, it is the same node.
    const marked = await node.evaluate((el) => ((window as unknown as { __helper: Element }).__helper = el) !== null);
    expect(marked).toBe(true);
    await push(page, board);
    await page.clock.runFor(HELPER_HOLD_MS / 2);
    await expect(node).not.toHaveAttribute("opacity", /.*/);
    await push(page, withOne);
    await page.clock.runFor(HELPER_HOLD_MS + FADE_MS);
    expect(await node.evaluate((el) => el === (window as unknown as { __helper: Element }).__helper && el.isConnected)).toBe(true);
    await expect(node).not.toHaveAttribute("opacity", /.*/);
    await push(page, board);
    await page.clock.runFor(HELPER_HOLD_MS + 32);
    await expect(node).toHaveAttribute("opacity", /^0?\.\d+$|^1$/);
    await page.clock.runFor(FADE_MS + 32);
    await expect(node).toHaveCount(0);
    await expect(scene(page)).toHaveAttribute("data-moving", "0");
  });

  test("nothing moves once the sub-agents are placed", async ({ page }) => {
    const board = loadBoard("scene-subagents");
    await openScene(page, board);
    expect((await helpers(page)).length).toBeGreaterThan(0);
    await expectNothingChanges(page, board);
  });

  test("no CSS animation runs inside a figure", async ({ page }) => {
    const generated = loadBoard("scene-c");
    // Waits dated now, so the halos breathe: the positive control.
    const board = structuredClone(generated);
    for (const card of board.columns.needs_you) card.last_event_at_ms = board.generated_at_ms;
    await openScene(page, board);
    const count = () =>
      page.evaluate(() => {
        const all = document.getAnimations();
        const inFigure = all.filter((a) => {
          const target = (a.effect as KeyframeEffect | null)?.target;
          return target instanceof Element && target.closest("[data-figure]") !== null;
        });
        return { all: all.length, inFigure: inFigure.length };
      });
    await expect.poll(async () => (await count()).all).toBeGreaterThan(0);
    expect((await count()).inFigure).toBe(0);
    await push(page, startsWorking(board, liveCards(board).find((c) => c.scene?.zone === "lounge")!.key));
    expect((await count()).inFigure).toBe(0);
  });

  test("missing data leaves every employee at its place with a question mark", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const was = await figures(page);
    // The agents snapshot failed: core places every live card unknown.
    const stale = structuredClone(board);
    stale.stale = { since_ms: board.generated_at_ms, error: "agents snapshot failed" };
    for (const card of liveCards(stale)) card.scene = card.scene && { ...card.scene, zone: "unknown" };
    await push(page, stale);
    const marked = scene(page).locator('[data-figure] [data-mark="question"]');
    await expect(marked).toHaveCount(Object.keys(was).length);
    // The stale banner takes height from the sessions area, so the scene
    // may rescale: places are compared on the floor, at the new scale.
    await sceneSettled(page);
    const scale = Number(await scene(page).getAttribute("data-scale"));
    const [layout] = layouts(board);
    const now = await figures(page);
    expect(Object.keys(now).sort()).toEqual(Object.keys(was).sort());
    for (const [key, state] of Object.entries(was)) {
      const [x, y] = placed(layout!.employees.find((e) => e.key === key)!.at!, scale);
      const [nx, ny] = parse(now[key]!.transform);
      expect(Math.abs(nx - x) + Math.abs(ny - y), key).toBeLessThan(1e-6);
      expect(now[key]!.shown, key).toEqual(state.shown);
    }
  });

  test("under reduced motion nothing in the scene animates", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const board = structuredClone(loadBoard("scene-c"));
    for (const card of board.columns.needs_you) card.last_event_at_ms = board.generated_at_ms;
    await openScene(page, board);
    await expect(scene(page).locator("[data-halo]")).not.toHaveCount(0);
    const running = await page.evaluate(
      () =>
        document.getAnimations().filter((a) => {
          const target = (a.effect as KeyframeEffect | null)?.target;
          return a.playState === "running" && target instanceof Element && target.closest("[data-testid=scene]") !== null;
        }).length,
    );
    expect(running).toBe(0);
  });

  test("nothing in the scene changes while the payloads are unchanged", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    await expectNothingChanges(page, board);
  });

  // FR-3, FR-9 (story 4.9): with the alert room lit, once the beacon's two
  // minutes have passed, nothing animates for as long as the alert lasts.
  // Its wait is dated to end its breathing a few seconds after first sight
  // (D-101), so the case sees it breathe, then stop.
  test("nothing in the scene changes while the payloads are unchanged, the alert room lit", async ({ page }) => {
    const generated = loadBoard("scene-alert");
    const board = structuredClone(generated);
    const beacon = beaconPair(board)!;
    const card = liveCards(board).find((c) => c.key === beacon.key)!;
    // The beacon counts from when the alert began: a wait that reached the
    // threshold alerted the threshold after its start (D-527).
    const reached = card.scene!.alert === "waited" ? board.alert_after_ms : 0;
    card.scene = { ...card.scene!, waiting_since_ms: board.generated_at_ms - HALO_BREATHING_MS + BEACON_LEFT_MS - reached };
    await openScene(page, board);
    await expect(scene(page).locator("[data-beacon]")).toHaveAttribute("data-beacon", beacon.key);
    await expect.poll(() => runningInScene(page)).toBeGreaterThan(0);
    await expect.poll(() => runningInScene(page), { timeout: BEACON_LEFT_MS + 3 * HALO_CYCLE_MS }).toBe(0);
    await expectNothingChanges(page, board);
    expect(await runningInScene(page)).toBe(0);
  });

  /** Watches the scene through the store's ticks and an identical payload:
   *  no mutation; then a changed payload writes. */
  async function expectNothingChanges(page: Page, board: Board): Promise<void> {
    await scene(page).evaluate((root) => {
      const records: MutationRecord[] = [];
      new MutationObserver((list) => records.push(...list)).observe(root, {
        subtree: true,
        childList: true,
        attributes: true,
        characterData: true,
      });
      (window as unknown as { __sceneRecords: MutationRecord[] }).__sceneRecords = records;
    });
    const records = () => page.evaluate(() => (window as unknown as { __sceneRecords: MutationRecord[] }).__sceneRecords.length);
    // The store's 30 s tick runs five times; the same payload comes again.
    await page.clock.runFor(150_000);
    await push(page, structuredClone(board));
    await page.clock.runFor(1_000);
    expect(await records()).toBe(0);
    // A changed payload writes.
    await push(page, startsWorking(board, liveCards(board).find((c) => c.scene?.zone === "lounge")!.key));
    await expect.poll(records).toBeGreaterThan(0);
  }

  // D-185: the gauges' text changes with the store's tick; their width and
  // the header's height do not, so the scene keeps its scale.
  test("the quota gauges' tick never rescales the scene", async ({ page }) => {
    const board = loadBoard("busy");
    expect(board.quota.five_hour).not.toBeNull();
    await openScene(page, board);
    const header = async () => (await page.locator("header").boundingBox())!.height;
    const before = { scale: await scene(page).getAttribute("data-scale"), header: await header() };
    const gauge = await page.getByTestId("gauge-5h").textContent();
    await page.clock.runFor(31 * 60_000);
    await expect(page.getByTestId("gauge-5h")).not.toHaveText(gauge ?? "");
    expect(await header()).toBe(before.header);
    expect(await scene(page).getAttribute("data-scale")).toBe(before.scale);
  });

  test("the scene's scale follows the sessions area", async ({ page }) => {
    await openScene(page, loadBoard("scene-c"));
    const scales = new Set<string>();
    for (const [width, height] of [[1400, 900], [1700, 1000], [1250, 840]] as const) {
      await page.setViewportSize({ width, height });
      await sceneSettled(page);
      const scale = (await scene(page).getAttribute("data-scale"))!;
      scales.add(scale);
      expect(Number(scale)).toBeGreaterThan(MIN_SCALE);
      // The scene fits: the area does not scroll above the minimum.
      const area = await scene(page).evaluate((s) => {
        const a = s.parentElement!;
        return { sw: a.scrollWidth, cw: a.clientWidth, sh: a.scrollHeight, ch: a.clientHeight };
      });
      expect(area.sw).toBeLessThanOrEqual(area.cw);
      expect(area.sh).toBeLessThanOrEqual(area.ch);
    }
    expect(scales.size).toBe(3);
  });

  test("below the minimum area the scene keeps the minimum scale and the area scrolls", async ({ page }) => {
    await openScene(page, loadBoard("scene-c"));
    for (const [width, height] of [[1000, 760], [1400, 600]] as const) {
      await page.setViewportSize({ width, height });
      await sceneSettled(page);
      await expect(scene(page)).toHaveAttribute("data-scale", String(MIN_SCALE));
      const scrolls = await scene(page).evaluate((s) => {
        const a = s.parentElement!;
        return a.scrollWidth > a.clientWidth || a.scrollHeight > a.clientHeight;
      });
      expect(scrolls, `${width} x ${height}`).toBe(true);
      // The window itself does not scroll: the sessions area does.
      expect(await page.evaluate(() => document.documentElement.scrollHeight <= innerHeight)).toBe(true);
    }
  });

  test("a fifth project widens the canvas and rescales once", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const [layout] = layouts(board);
    const columns = layout!.columns;
    await scene(page).evaluate((root) => {
      const records: string[] = [];
      new MutationObserver((list) => list.forEach((r) => records.push(r.attributeName!))).observe(root, {
        attributes: true,
        attributeFilter: ["data-scale"],
      });
      (window as unknown as { __scaleRecords: string[] }).__scaleRecords = records;
    });
    const next = structuredClone(board);
    const newcomer = structuredClone(next.columns.working[0]!);
    newcomer.key = `${newcomer.key}-fifth`;
    newcomer.session_id = `${newcomer.session_id}-fifth`;
    newcomer.project = { ...newcomer.project, name: "fifth-project", color: null };
    next.columns.working.push(newcomer);
    const [, grown] = layouts(board, next);
    expect(grown!.columns).toBeGreaterThan(columns);
    await push(page, next);
    await expect(scene(page)).toHaveAttribute("data-columns", String(grown!.columns));
    await sceneSettled(page);
    expect(await page.evaluate(() => (window as unknown as { __scaleRecords: string[] }).__scaleRecords.length)).toBe(1);
  });

  // The owner's screen (D-520, D-525): 1920 x 1080 at 100 %, the window
  // maximised (an inner height of 1001), eight projects, no scrolling,
  // every target at least 44 CSS px; the orchestrator live, its count at
  // the 200 pending proposals a view can hold (story 5.18 AC 9, D-833).
  for (const alert of [false, true]) {
    test(`eight projects fit a maximised Full HD window${alert ? " with the alert band" : ""}`, async ({ page }) => {
      await page.setViewportSize({ width: 1920, height: 1001 });
      const board = loadBoard("scene-eight");
      board.orchestrator = sceneBoardOrchestrator();
      await answerOrchestrator(page, withPending(sceneView(), PENDING_LIMIT));
      if (alert) {
        const first = board.columns.needs_you[0]!;
        first.scene = { ...first.scene!, zone: "alert", alert: "priority" };
        board.alert_count = 1;
      }
      // The relay in use: its toggle is the widest set of fixed header
      // controls, beside the theme control (story 4.13).
      await openScene(page, board, loadRelay("relay-clean"));
      if (alert) await expect(page.getByTestId("alert-band")).toBeVisible();
      await expect(page.getByTestId("orchestrator-pending")).toHaveText(String(PENDING_LIMIT));
      await expect(scene(page).locator("[data-orchestrator]")).toHaveCount(1);
      // The header on one line (D-526), every fixed control included.
      const tops = await page.locator("header.bar > *").evaluateAll((items) => items.map((e) => Math.round(e.getBoundingClientRect().top)));
      expect(new Set(tops).size, `the header's items start at ${tops.join(", ")}`).toBe(1);
      expect(Number(await scene(page).getAttribute("data-columns"))).toBe(layouts(board)[0]!.columns);
      expect(Number(await scene(page).getAttribute("data-scale"))).toBeGreaterThanOrEqual(MIN_SCALE);
      const area = await scene(page).evaluate((s) => {
        const a = s.parentElement!;
        return { sw: a.scrollWidth, cw: a.clientWidth, sh: a.scrollHeight, ch: a.clientHeight };
      });
      expect(area.sw, "the sessions area scrolls sideways").toBeLessThanOrEqual(area.cw);
      expect(area.sh, "the sessions area scrolls down").toBeLessThanOrEqual(area.ch);
      await expectTargetsAtLeastMinimum(page);
    });
  }

  // The dark ones (story 4.13) are compared with the prototype's dark
  // render, `scene-interactive-dark-target.png`.
  // Story 5.18 AC 10: each density with the orchestrator live and the
  // proposals of `scene-orchestrator`, and that scenario itself.
  for (const theme of ["light", "dark"] as const) {
    test(`the densities match their ${theme} baselines`, async ({ page }, testInfo) => {
      const densities = ["scene-a", "scene-b", "scene-c", "scene-orchestrator"] as const;
      const updating = testInfo.config.updateSnapshots === "all" || testInfo.config.updateSnapshots === "changed";
      const missing = densities.filter((name) => !existsSync(testInfo.snapshotPath(`${name}-${theme}.png`)));
      // D-508: the baselines come from the CI snapshot run and are committed
      // only after the owner compared them with the prototype's render.
      test.fixme(missing.length > 0 && !updating, `baselines not committed yet (D-508): ${missing.join(", ")}`);
      const live = (name: (typeof densities)[number]) => ({ ...loadBoard(name), orchestrator: sceneBoardOrchestrator() });
      await page.emulateMedia({ colorScheme: theme });
      await answerOrchestrator(page, sceneView());
      await openScene(page, live(densities[0]));
      await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
      for (const name of densities) {
        await reopenScene(page, live(name));
        await expect(scene(page)).toHaveScreenshot(`${name}-${theme}.png`, {
          maxDiffPixelRatio: SCREENSHOT_MAX_DIFF_RATIO,
          animations: "disabled",
        });
      }
    });
  }
});

// ------------------------------------------------------------ acting from the scene (story 4.8)

const targetById = (page: Page, id: string) => scene(page).locator(`[data-target=${JSON.stringify(id)}]`);
const figureOf = (page: Page, key: string) => scene(page).locator(`[data-layer="targets"] [data-key=${JSON.stringify(key)}]`);
const sessionFile = (page: Page) => page.getByTestId("session-file");
const rowOf = (page: Page, key: string) => sessionFile(page).locator(`[data-row=${JSON.stringify(key)}]`);
const focusedTarget = (page: Page) => page.evaluate(() => document.activeElement?.getAttribute("data-target") ?? null);
const focusedTestId = (page: Page) => page.evaluate(() => document.activeElement?.getAttribute("data-testid") ?? null);
const focusCalls = async (page: Page) => (await calls(page)).filter((c) => c.cmd === "focus").map((c) => (c.args as { key: string }).key);
const cardOf = (board: Board, key: string): Card => Object.values(board.columns).flat().find((c) => c.key === key)!;
const ARROW_KEYS: Record<Direction, string> = { up: "ArrowUp", down: "ArrowDown", left: "ArrowLeft", right: "ArrowRight" };

/** Counts every write inside the scene element from now on. */
async function observeScene(page: Page): Promise<() => Promise<number>> {
  await scene(page).evaluate((root) => {
    const records: MutationRecord[] = [];
    new MutationObserver((list) => records.push(...list)).observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    (window as unknown as { __actRecords: MutationRecord[] }).__actRecords = records;
  });
  return () => page.evaluate(() => (window as unknown as { __actRecords: MutationRecord[] }).__actRecords.length);
}

/** The group target of the room `room` picks, from the layout. */
function groupTarget(layout: SceneLayout, room: (kind: string, id: string) => boolean): Target {
  const found = layout.targets.find((t) => t.kind === "group" && t.keys.length > 0 && layout.rooms.some((r) => r.id === t.group && room(r.kind, r.id)));
  if (!found) throw new Error("no such group in this layout");
  return found;
}

/** scene-overflow with someone at reception and someone in the alert room,
 *  so every room of AD-63's tab order holds a target. */
function everyZone(): Board {
  const board = structuredClone(loadBoard("scene-overflow"));
  const resting = board.columns.your_turn.find((c) => c.scene?.zone === "lounge")!;
  resting.scene = { ...resting.scene!, zone: "reception" };
  const first = board.columns.needs_you[0]!;
  first.scene = { ...first.scene!, zone: "alert", alert: "priority" };
  board.alert_count = 1;
  return board;
}

/** scene-overflow with every Working session in the lounge. */
function fullLounge(): Board {
  const board = structuredClone(loadBoard("scene-overflow"));
  for (const c of board.columns.working.splice(0)) board.columns.your_turn.push({ ...c, column: "your_turn", scene: { ...c.scene!, zone: "lounge" } });
  return board;
}

/** The list form shows `keys` in that order, each a row button. */
async function expectList(page: Page, keys: string[]): Promise<void> {
  await expect(sessionFile(page)).toHaveAttribute("data-form", "list");
  const rows = await sessionFile(page).locator("[data-row]").evaluateAll((all) => all.map((r) => r.getAttribute("data-row")));
  expect(rows).toEqual(keys);
}

test.describe("acting from the scene", () => {
  test("hovering and focusing targets changes nothing in the scene", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const records = await observeScene(page);
    const targets = scene(page).locator("[data-target]");
    const count = await targets.count();
    expect(count).toBe(layouts(board)[0]!.targets.length);
    for (let i = 0; i < count; i++) {
      const target = targets.nth(i);
      await expect(target.locator("[data-tag]")).toBeHidden();
      await target.hover();
      await expect(target.locator("[data-tag]")).toBeVisible();
    }
    await scene(page).focus();
    for (let i = 0; i < count; i++) {
      await page.keyboard.press("Tab");
      await expect(targets.nth(i)).toBeFocused();
      await expect(targets.nth(i).locator("[data-tag]")).toBeVisible();
    }
    expect(await records()).toBe(0);
  });

  test("Tab walks reception, the queue and its group, the alert room, the offices, the lounge and More offices in that order", async ({ page }) => {
    const board = everyZone();
    await openScene(page, board);
    const [layout] = layouts(board);
    await scene(page).focus();
    const walked: (string | null)[] = [];
    for (let i = 0; i < layout!.targets.length; i++) {
      await page.keyboard.press("Tab");
      walked.push(await focusedTarget(page));
    }
    expect(walked).toEqual(layout!.targets.map((t) => t.id));
    const rooms = layout!.targets.map((t) => t.group).filter((room, i, all) => room !== all[i - 1]);
    const offices = layout!.rooms.filter((r) => r.kind === "office" && layout!.targets.some((t) => t.group === r.id)).map((r) => r.id);
    expect(rooms).toEqual(["reception", "decision", "alert", ...offices, "lounge", "more"]);
    // The decision office's group comes right after the queue's figures
    // (D-103), at its plaque's end (D-557), then the orchestrator's empty
    // chair (D-567).
    const decision = layout!.targets.filter((t) => t.group === "decision");
    expect(decision.map((t) => t.kind)).toEqual([...decision.slice(0, -2).map(() => "figure"), "group", "chair"]);
  });

  test("a focused target shows the two-tone ring", async ({ page }) => {
    await openScene(page, loadBoard("scene-c"));
    await scene(page).focus();
    await page.keyboard.press("Tab");
    const ring = await page.evaluate(() => {
      const target = document.activeElement as HTMLElement;
      const probe = document.createElement("span");
      document.body.append(probe);
      probe.style.color = "var(--color-working)";
      const working = getComputedStyle(probe).color;
      probe.style.color = "var(--color-surface)";
      const surface = getComputedStyle(probe).color;
      probe.remove();
      const s = getComputedStyle(target);
      return { target: target.dataset.target, visible: target.matches(":focus-visible"), style: s.outlineStyle, width: s.outlineWidth, colour: s.outlineColor, offset: s.outlineOffset, shadow: s.boxShadow, working, surface };
    });
    expect(ring.target).toBeTruthy();
    expect(ring.visible).toBe(true);
    expect([ring.style, ring.width, ring.colour, ring.offset]).toEqual(["solid", "3px", ring.working, "2px"]);
    expect(ring.shadow).toBe(`${ring.surface} 0px 0px 0px 2px`);
  });

  test("a figure's target is at its new place as soon as the board changes", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const mover = liveCards(board).find((c) => c.scene?.zone === "lounge")!;
    const next = startsWorking(board, mover.key);
    const [before, after] = layouts(board, next);
    const targetOf = (layout: SceneLayout) => layout.targets.find((t) => t.kind === "figure" && t.keys[0] === mover.key)!;
    expect(targetOf(after!).rect).not.toEqual(targetOf(before!).rect);
    const scale = Number(await scene(page).getAttribute("data-scale"));
    const offset = async () => {
      const box = (await boxes(page)).targets.find((t) => t.id === targetOf(after!).id)!.box;
      const want = targetOf(after!).rect;
      return Math.abs(box.x - want.x * scale) + Math.abs(box.y - want.y * scale);
    };
    expect(await offset()).toBeGreaterThan(1);
    await push(page, next);
    await expect.poll(offset).toBeLessThan(0.5);
    await sceneSettled(page);
    expect(await offset()).toBeLessThan(0.5);
  });

  // D-471, story 4.10: the figure walks, its target does not. The clock is
  // paused so each frame is stepped: from the first frame after the push
  // the target is at the destination while the figure is still at its old
  // place, and the walking figure takes no click.
  test("a walking figure's target is at its destination from the first frame", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openWalkingScene(page, board);
    await page.clock.pauseAt(board.generated_at_ms + 60_000);
    const walker = liveCards(board).find((c) => c.scene?.zone === "lounge")!;
    const next = startsWorking(board, walker.key);
    const [before, after] = layouts(board, next);
    const targetOf = (layout: SceneLayout) => layout.targets.find((t) => t.kind === "figure" && t.keys[0] === walker.key)!;
    const scale = Number(await scene(page).getAttribute("data-scale"));
    const was = (await figures(page))[walker.key]!.transform;
    await push(page, next);
    await page.clock.runFor(16);
    await expect(scene(page)).toHaveAttribute("data-moving", "1");
    // Layer 5's target for the key: at the destination, not at the start.
    const box = (await boxes(page)).targets.find((t) => t.id === targetOf(after!).id)!.box;
    const want = targetOf(after!).rect;
    const old = targetOf(before!).rect;
    expect(Math.abs(box.x - want.x * scale) + Math.abs(box.y - want.y * scale)).toBeLessThan(0.5);
    expect(Math.abs(box.x - old.x * scale) + Math.abs(box.y - old.y * scale)).toBeGreaterThan(1);
    // The figure is on its way, near where it was.
    const [wx, wy] = parse(was);
    const [fx, fy] = parse((await figures(page))[walker.key]!.transform);
    expect(Math.hypot(fx - wx, fy - wy)).toBeLessThan(3);
    // A click on the walking figure reaches what lies under it, not it:
    // layer 3 ignores the pointer, and no target sits there any more.
    const hit = await scene(page).evaluate(
      (root, [x, y, s]) => {
        const origin = root.getBoundingClientRect();
        const el = document.elementFromPoint(origin.left + x * s, origin.top + (y - 20) * s);
        return { figure: el?.closest("[data-figure]") !== null, target: el?.closest("[data-target]")?.getAttribute("data-target") ?? null };
      },
      [fx, fy, scale] as const,
    );
    expect(hit.figure).toBe(false);
    expect(hit.target).not.toBe(targetOf(after!).id);
    // Each frame of the walk leaves the target where it is.
    for (let i = 0; i < 30; i++) {
      await page.clock.runFor(16);
      const now = (await boxes(page)).targets.find((t) => t.id === targetOf(after!).id)!.box;
      expect([now.x, now.y]).toEqual([box.x, box.y]);
    }
  });

  test("clicking a figure opens its file with Bring to front focused", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const scale = await scene(page).getAttribute("data-scale");
    const first = board.columns.needs_you[0]!;
    await figureOf(page, first.key).click();
    await expect(page.getByRole("dialog", { name: first.name })).toBeVisible();
    await expect(page.getByTestId("bring-to-front")).toBeFocused();
    await expect(page.getByTestId("file-number")).toHaveText("1");
    await expect(page.getByTestId("file-zone")).toHaveText(ZONE_WORDS.decision);
    await expect(page.getByTestId("file-waiting-for")).toHaveText(first.waiting_for!);
    // At the sessions area's right edge, its inset from it (D-702), 420 px
    // wide, the scene unscaled, once its slide in ended (D-545).
    await transitionsDone(page);
    const file = (await sessionFile(page).boundingBox())!;
    const area = (await page.getByTestId("sessions-area").boundingBox())!;
    const inset = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--file-inset")));
    expect(file.width).toBe(420);
    expect(Math.abs(file.x + file.width - (area.x + area.width - inset))).toBeLessThan(1);
    expect(await scene(page).getAttribute("data-scale")).toBe(scale);
    expect(await scene(page).evaluate((s, f) => s.contains(document.querySelector(f)), "[data-testid=session-file]")).toBe(false);
  });

  test("Enter then Enter brings the session to front as a board card does", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const first = board.columns.needs_you[0]!;
    await figureOf(page, first.key).focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("bring-to-front")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect.poll(() => focusCalls(page)).toEqual([first.key]);
    await expect(page.getByTestId("focus-outcome")).toContainText(outcomeText({ outcome: "focused" }));
    // Space opens a file too, and brings nothing to front by itself.
    const second = board.columns.needs_you[1]!;
    await figureOf(page, second.key).focus();
    await page.keyboard.press("Space");
    await expect(page.getByRole("dialog", { name: second.name })).toBeVisible();
    await expect(page.getByTestId("bring-to-front")).toBeFocused();
    expect(await focusCalls(page)).toEqual([first.key]);
  });

  test("Escape closes the file and returns focus to the figure", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const first = board.columns.needs_you[0]!;
    await figureOf(page, first.key).click();
    await expect(page.getByTestId("bring-to-front")).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(sessionFile(page)).toHaveCount(0);
    await expect(figureOf(page, first.key)).toBeFocused();
    // The file follows its card into another column; Escape then finds the
    // target that now represents it, at its desk.
    await figureOf(page, first.key).click();
    const working = startsWorking(board, first.key);
    await push(page, working);
    await expect(page.getByTestId("file-zone")).toHaveText(ZONE_WORDS.office);
    await page.keyboard.press("Escape");
    await expect(figureOf(page, first.key)).toBeFocused();
    // A card the scene no longer draws: focus goes to the scene element.
    await figureOf(page, first.key).click();
    const ended = structuredClone(working);
    const at = ended.columns.working.findIndex((c) => c.key === first.key);
    const [card] = ended.columns.working.splice(at, 1);
    ended.columns.to_resume.unshift({ ...card!, column: "to_resume", scene: null });
    await push(page, ended);
    await expect(page.getByTestId("file-zone")).toHaveCount(0);
    await expect(page.getByRole("dialog", { name: first.name })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(sessionFile(page)).toHaveCount(0);
    await expect(scene(page)).toBeFocused();
  });

  test("the file offers the card's actions with their outcomes", async ({ page }) => {
    const board = loadBoard("busy");
    const [layout] = layouts(board);
    const placed = new Set(layout!.employees.filter((e) => e.spot !== null).map((e) => e.key));
    const attach = liveCards(board).find((c) => c.attach_available && placed.has(c.key))!;
    const other = liveCards(board).find((c) => c.key !== attach.key && placed.has(c.key))!;
    const candidates: FocusOutcome = { outcome: "candidates", detail: ["Terminal - first", "Terminal - second"] };
    await openScene(page, board, null, null, { [other.key]: candidates });
    await figureOf(page, attach.key).click();
    await expect(page.getByTestId("copy-attach")).toHaveAccessibleName("Copy attach command");
    await page.getByTestId("bring-to-front").click();
    await expect(page.getByTestId("focus-outcome")).toContainText(outcomeText({ outcome: "focused" }));
    await page.getByRole("button", { name: "Wrong window?" }).click();
    await expect.poll(async () => (await calls(page)).filter((c) => c.cmd === "report_wrong_window").length).toBe(1);
    await expect(page.getByTestId("focus-outcome")).toHaveCount(0);
    await page.keyboard.press("Escape");
    await figureOf(page, other.key).click();
    await page.getByTestId("bring-to-front").click();
    const outcome = page.getByTestId("focus-outcome");
    await expect(outcome).toContainText(outcomeText(candidates));
    for (const title of (candidates as { detail: string[] }).detail) await expect(outcome).toContainText(title);
    expect(await focusCalls(page)).toEqual([attach.key, other.key]);
  });

  test("the file of an approval-only wait opens Board mode on its approval", async ({ page }) => {
    const board = loadBoard("scene-c");
    const waiting = liveCards(board).find((c) => c.scene?.approval)!;
    const request = board.approvals.find((a) => a.session_id === waiting.session_id)!;
    await openScene(page, board);
    await figureOf(page, waiting.key).click();
    await expect(page.getByTestId("file-approval")).toContainText(request.action);
    await expect(page.getByTestId("file-since")).not.toContainText("unknown");
    await expect(page.getByTestId("file-number")).toHaveCount(0);
    await page.getByTestId("open-in-board").click();
    await expect(page.locator(`[data-testid=approval-card][data-id=${JSON.stringify(request.id)}]`)).toBeVisible();
    await expect(page.getByRole("radio", { name: "Board" })).toBeChecked();
    await expect(scene(page)).toHaveCount(0);
  });

  // Story 5.12 AC 6: the request a session waits on says its class.
  test("the file of a session with an auto request names its class", async ({ page }) => {
    const board = loadBoard("approvals-auto");
    const request = board.approvals.find((a) => a.class === "auto")!;
    const waiting = liveCards(board).find((c) => c.session_id === request.session_id)!;
    expect(waiting.scene?.approval).toBe(true);
    await openScene(page, board);
    await figureOf(page, waiting.key).click();
    await expect(page.getByTestId("file-approval")).toContainText(request.action);
    await expect(page.getByTestId("file-approval-class")).toHaveText("(class auto)");
  });

  test("the file of an alerted session states its cause", async ({ page }) => {
    const board = loadBoard("scene-alert");
    await openScene(page, board);
    const alerted = liveCards(board).filter((c) => c.scene?.alert);
    expect(new Set(alerted.map((c) => c.scene!.alert)).size).toBeGreaterThan(1);
    for (const card of alerted) {
      await figureOf(page, card.key).click();
      await expect(page.getByTestId("file-zone")).toHaveText(ZONE_WORDS.alert);
      await expect(page.getByTestId("file-alert")).toHaveText(causeText(card, board.generated_at_ms));
      await page.keyboard.press("Escape");
    }
  });

  test("the file of an unknown session that keeps an alert says last known", async ({ page }) => {
    const board = loadBoard("scene-alert");
    await openScene(page, board);
    const card = liveCards(board).find((c) => c.scene?.alert)!;
    await figureOf(page, card.key).click();
    await expect(page.getByTestId("file-alert")).not.toContainText("last known");
    const stale = structuredClone(board);
    stale.stale = { since_ms: board.generated_at_ms, error: "agents snapshot failed" };
    for (const c of liveCards(stale)) c.scene = c.scene && { ...c.scene, zone: "unknown" };
    await push(page, stale);
    await expect(page.getByTestId("file-zone")).toHaveText(ZONE_WORDS.unknown);
    await expect(page.getByTestId("file-alert")).toHaveText(`${causeText(card, board.generated_at_ms)}, last known`);
  });

  test("opening and closing the file changes nothing in the scene", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const records = await observeScene(page);
    for (const card of board.columns.needs_you) {
      await figureOf(page, card.key).click();
      await expect(page.getByRole("dialog", { name: card.name })).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(sessionFile(page)).toHaveCount(0);
    }
    await figureOf(page, board.columns.needs_you[0]!.key).focus();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Escape");
    expect(await records()).toBe(0);
  });

  test("the file closes when its session leaves the board", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    const first = board.columns.needs_you[0]!;
    await figureOf(page, first.key).click();
    await expect(page.getByTestId("bring-to-front")).toBeFocused();
    const without = structuredClone(board);
    without.columns.needs_you = without.columns.needs_you.filter((c) => c.key !== first.key);
    without.waiting_count -= 1;
    await push(page, without);
    await expect(sessionFile(page)).toHaveCount(0);
    await expect(scene(page)).toBeFocused();
    // The key comes back: its file stays closed.
    await push(page, board);
    await expect(figureOf(page, first.key)).toHaveCount(1);
    await expect(sessionFile(page)).toHaveCount(0);
  });

  test("the decision office's group lists the waiting beyond the queue, each opening its file", async ({ page }) => {
    const board = loadBoard("scene-overflow");
    await openScene(page, board);
    const [layout] = layouts(board);
    const queueGroup = groupTarget(layout!, (kind) => kind === "decision");
    const queued = layout!.targets.filter((t) => t.group === "decision" && t.kind === "figure").flatMap((t) => t.keys);
    expect([...queued, ...queueGroup.keys]).toEqual(board.columns.needs_you.map((c) => c.key));
    await targetById(page, queueGroup.id).click();
    await expectList(page, queueGroup.keys);
    for (const key of queueGroup.keys) {
      await rowOf(page, key).click();
      await expect(page.getByRole("dialog", { name: cardOf(board, key).name })).toBeVisible();
      await expect(page.getByTestId("bring-to-front")).toBeFocused();
      await page.keyboard.press("Escape");
      await expect(rowOf(page, key)).toBeFocused();
    }
  });

  test("a chip lists the office's sessions without a desk", async ({ page }) => {
    const board = loadBoard("scene-overflow");
    await openScene(page, board);
    const [layout] = layouts(board);
    const chip = groupTarget(layout!, (kind) => kind === "office");
    for (const key of chip.keys) expect(layout!.desks.some((d) => d.key === key), key).toBe(false);
    await targetById(page, chip.id).click();
    await expectList(page, chip.keys);
    await rowOf(page, chip.keys[0]!).click();
    await expect(page.getByRole("dialog", { name: cardOf(board, chip.keys[0]!).name })).toBeVisible();
  });

  test("More offices lists the extra projects' sessions", async ({ page }) => {
    const board = loadBoard("scene-overflow");
    await openScene(page, board);
    const [layout] = layouts(board);
    const more = groupTarget(layout!, (kind) => kind === "more");
    const room = layout!.rooms.find((r) => r.id === more.group)!;
    expect(more.keys.map((k) => cardOf(board, k).project.name).every((p) => room.projects.includes(p))).toBe(true);
    // Their sessions at work: the waiting queue and the lounge hold the others.
    const atWork = liveCards(board).filter((c) => room.projects.includes(c.project.name) && c.scene?.zone === "office");
    expect(more.keys).toEqual(atWork.map((c) => c.key));
    await targetById(page, more.id).click();
    await expectList(page, more.keys);
    await expect(sessionFile(page).getByRole("heading")).toHaveText(room.plaque.text);
  });

  test("a full lounge lists the rest", async ({ page }) => {
    const board = fullLounge();
    await openScene(page, board);
    const [layout] = layouts(board);
    const lounge = groupTarget(layout!, (kind) => kind === "lounge");
    const seated = layout!.targets.filter((t) => t.group === lounge.group && t.kind === "figure").flatMap((t) => t.keys);
    const inLounge = liveCards(board).filter((c) => c.scene?.zone === "lounge").map((c) => c.key);
    expect([...seated, ...lounge.keys].sort()).toEqual([...inLounge].sort());
    await targetById(page, lounge.id).click();
    await expectList(page, lounge.keys);
  });

  test("Escape steps back from a listed file to the list, then to the group", async ({ page }) => {
    const board = loadBoard("scene-overflow");
    await openScene(page, board);
    const [layout] = layouts(board);
    const queueGroup = groupTarget(layout!, (kind) => kind === "decision");
    const key = queueGroup.keys[1]!;
    await targetById(page, queueGroup.id).click();
    await rowOf(page, key).click();
    await expect(sessionFile(page)).toHaveAttribute("data-form", "file");
    await expect(page.getByTestId("file-close")).toHaveText("Back to the list");
    await page.keyboard.press("Escape");
    await expectList(page, queueGroup.keys);
    await expect(rowOf(page, key)).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(sessionFile(page)).toHaveCount(0);
    await expect(targetById(page, queueGroup.id)).toBeFocused();
  });

  test("a list whose group empties closes, focus on the scene", async ({ page }) => {
    const board = loadBoard("scene-overflow");
    await openScene(page, board);
    const queueGroup = groupTarget(layouts(board)[0]!, (kind) => kind === "decision");
    await targetById(page, queueGroup.id).click();
    await expect(rowOf(page, queueGroup.keys[0]!)).toBeFocused();
    // A board arrives while the list is open: one row fewer, the list stays.
    const fewer = structuredClone(board);
    fewer.columns.needs_you = fewer.columns.needs_you.filter((c) => c.key !== queueGroup.keys.at(-1));
    await push(page, fewer);
    await expectList(page, queueGroup.keys.slice(0, -1));
    // The queue now holds everyone: no group, no list.
    const emptied = structuredClone(board);
    emptied.columns.needs_you = emptied.columns.needs_you.filter((c) => !queueGroup.keys.includes(c.key));
    await push(page, emptied);
    await expect(sessionFile(page)).toHaveCount(0);
    await expect(targetById(page, queueGroup.id)).toHaveCount(0);
    await expect(scene(page)).toBeFocused();
  });

  test("an empty board has no session target and the header's counts say zero", async ({ page }) => {
    const board = loadBoard("empty");
    expect(liveCards(board)).toEqual([]);
    await openScene(page, board);
    // Only the orchestrator's empty chair, which is no session (D-567).
    await expect(scene(page).locator("[data-target]")).toHaveCount(1);
    await expect(scene(page).locator('[data-target="chair"]')).toHaveCount(1);
    await expect(page.getByTestId("ended-to-resume")).toHaveAccessibleName("0 to resume");
    await expect(page.getByTestId("ended-done")).toHaveAccessibleName("0 done");
    await expect(page.getByTestId("ended-done")).toHaveAttribute("title", new RegExp(`in the last ${hours(board.done_window_ms)}:`));
    // Escape with nothing open changes nothing.
    await scene(page).focus();
    await page.keyboard.press("Escape");
    await expect(scene(page)).toBeFocused();
  });

  test("list rows are 44 px or more", async ({ page }) => {
    const board = loadBoard("scene-overflow");
    await openScene(page, board);
    const groups = layouts(board)[0]!.targets.filter((t) => t.kind === "group" && t.keys.length > 0);
    expect(groups.length).toBeGreaterThan(2);
    for (const group of groups) {
      await targetById(page, group.id).click();
      const rows = sessionFile(page).locator("[data-row]");
      await expect(rows).toHaveCount(group.keys.length);
      for (const box of await rows.evaluateAll((all) => all.map((r) => r.getBoundingClientRect().toJSON() as DOMRect))) {
        expect(atLeast(box.width, TARGET_CSS_MIN) && atLeast(box.height, TARGET_CSS_MIN), `${group.id}: ${box.width} x ${box.height}`).toBe(true);
      }
      await page.keyboard.press("Escape");
      await expect(targetById(page, group.id)).toBeFocused();
    }
  });

  test("arrows move within a zone, stop at its edge and do not scroll the area", async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 760 });
    const board = loadBoard("scene-overflow");
    await openScene(page, board);
    const [layout] = layouts(board);
    await page.evaluate(() => {
      const seen: boolean[] = [];
      (window as unknown as { __arrows: boolean[] }).__arrows = seen;
      addEventListener("keydown", (e) => e.key.startsWith("Arrow") && seen.push(e.defaultPrevented));
    });
    const scroll = () => scene(page).evaluate((s) => ({ top: s.parentElement!.scrollTop, left: s.parentElement!.scrollLeft }));
    const office = layout!.rooms.find((r) => r.kind === "office" && layout!.targets.filter((t) => t.group === r.id).length > 2)!;
    let moves = 0;
    let edges = 0;
    for (const target of layout!.targets.filter((t) => t.group === "decision" || t.group === office.id)) {
      for (const direction of Object.keys(ARROW_KEYS) as Direction[]) {
        await targetById(page, target.id).focus();
        const before = await scroll();
        await page.keyboard.press(ARROW_KEYS[direction]);
        const expected = nearestTarget(layout!.targets, target.id, direction);
        expect(await focusedTarget(page), `${direction} from ${target.id}`).toBe(expected ?? target.id);
        if (expected === null) {
          edges++;
          expect(await scroll(), `${direction} from ${target.id}`).toEqual(before);
        } else {
          moves++;
          expect(layout!.targets.find((t) => t.id === expected)!.group).toBe(target.group);
        }
      }
    }
    expect(moves).toBeGreaterThan(0);
    expect(edges).toBeGreaterThan(0);
    const prevented = await page.evaluate(() => (window as unknown as { __arrows: boolean[] }).__arrows);
    expect(prevented.length).toBe(moves + edges);
    expect(prevented.every(Boolean)).toBe(true);
  });

  test("keys 1 to 9 bring the nth waiting session to front from the scene, in the alert room too", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const waiting = board.columns.needs_you;
    expect(waiting.length).toBeGreaterThan(1);
    expect(waiting.every((c) => c.scene?.zone === "alert")).toBe(true);
    await openScene(page, board);
    // With a target focused, then with the file open.
    await figureOf(page, waiting[1]!.key).focus();
    await page.keyboard.press("1");
    await figureOf(page, waiting[0]!.key).click();
    await page.keyboard.press("2");
    await expect.poll(() => focusCalls(page)).toEqual([waiting[0]!.key, waiting[1]!.key]);
    // A digit with no session behind it does nothing.
    await page.keyboard.press(String(waiting.length + 1));
    expect(await focusCalls(page)).toEqual([waiting[0]!.key, waiting[1]!.key]);
  });

  test("every target is 44 px or more and topmost at its centre", async ({ page }) => {
    await openScene(page, loadBoard("scene-c"));
    for (const name of ["scene-c", "scene-overflow"] as const) {
      for (const [width, height] of [[1000, 760], [1400, 900]] as const) {
        await page.setViewportSize({ width, height });
        const board = loadBoard(name);
        await reopenScene(page, board);
        const ids = layouts(board)[0]!.targets.map((t) => t.id);
        await expect(scene(page).locator("[data-target]")).toHaveCount(ids.length);
        for (const id of ids) {
          const target = targetById(page, id);
          await target.scrollIntoViewIfNeeded();
          const box = (await target.boundingBox())!;
          expect(atLeast(box.width, TARGET_CSS_MIN) && atLeast(box.height, TARGET_CSS_MIN), `${name} ${width} x ${height} ${id}`).toBe(true);
          const topmost = await target.evaluate((el) => {
            const r = el.getBoundingClientRect();
            return document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2) === el;
          });
          expect(topmost, `${name} ${width} x ${height} ${id}`).toBe(true);
        }
      }
    }
  });

  // Story 4.15 (the owner's item 4, D-541): To resume and Done are counted
  // in the header, not under the scene; each opens Board mode on its column.
  test("the header counts To resume and Done and opens Board mode on the column", async ({ page }) => {
    const board = loadBoard("busy");
    expect(board.columns.to_resume.length).toBeGreaterThan(0);
    await openScene(page, board);
    await expect(scene(page).getByTestId("ended-counts")).toHaveCount(0);
    await expect(page.locator("header").getByTestId("ended-counts")).toBeVisible();
    await expect(page.getByTestId("ended-to-resume")).toHaveAccessibleName(`${board.columns.to_resume.length} to resume`);
    await expect(page.getByTestId("ended-done")).toHaveAccessibleName(`${board.columns.done.length} done`);
    await page.getByTestId("ended-counts").evaluate((root) => {
      const records: MutationRecord[] = [];
      new MutationObserver((list) => records.push(...list)).observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
      (window as unknown as { __countRecords: MutationRecord[] }).__countRecords = records;
    });
    const countRecords = () => page.evaluate(() => (window as unknown as { __countRecords: MutationRecord[] }).__countRecords.length);
    // Another board with the same counts: the counts are not written.
    const renamed = structuredClone(board);
    renamed.columns.needs_you[0]!.name = "renamed-session";
    await push(page, renamed);
    await expect(page.getByTestId("scene-summary")).toContainText("renamed-session");
    expect(await countRecords()).toBe(0);
    // One more Done: they are, and the button keeps its box (D-517).
    const before = await page.getByTestId("ended-done").boundingBox();
    const more = structuredClone(renamed);
    more.columns.done.push({ ...more.columns.done[0]!, key: `${more.columns.done[0]!.key}-again`, session_id: null });
    await push(page, more);
    await expect(page.getByTestId("ended-done")).toHaveAccessibleName(`${more.columns.done.length} done`);
    expect(await countRecords()).toBeGreaterThan(0);
    expect(await page.getByTestId("ended-done").boundingBox()).toEqual(before);
    await page.getByTestId("ended-done").click();
    await expect(page.getByTestId("column-done")).toBeVisible();
    await expect(page.locator("#title-done")).toBeFocused();
    await page.keyboard.press("m");
    await sceneSettled(page);
    await page.getByTestId("ended-to-resume").click();
    await expect(page.getByTestId("column-to_resume")).toBeVisible();
    await expect(page.locator("#title-to_resume")).toBeFocused();
    // From another view too.
    await page.getByTestId("view-subjects").click();
    await page.getByTestId("ended-done").click();
    await expect(page.locator("#title-done")).toBeFocused();
  });

  test("session text renders as text in the scene and the file", async ({ page }) => {
    const markup = '<img src="x" onerror="window.__pwned = 1"><b>bold</b>';
    const board = structuredClone(loadBoard("scene-overflow"));
    const queued = board.columns.needs_you[0]!;
    const listed = board.columns.needs_you.at(-1)!;
    queued.name = `${markup} queued`;
    listed.name = `${markup} listed`;
    const project = queued.project.name;
    const renamed = `${markup} project`;
    board.floor_projects = board.floor_projects.map((p) => (p.name === project ? { ...p, name: renamed } : p));
    for (const c of Object.values(board.columns).flat()) if (c.project.name === project) c.project = { ...c.project, name: renamed };
    await openScene(page, board);
    const [layout] = layouts(board);
    const figure = figureOf(page, queued.key);
    expect(await figure.getAttribute("aria-label")).toContain(queued.name);
    await expect(figure.locator("[data-tag]")).toContainText(queued.name);
    await expect(page.getByTestId("register")).toContainText(queued.name);
    await expect(scene(page).locator("[data-plaque]").filter({ hasText: renamed })).toHaveCount(1);
    await figure.click();
    await expect(page.getByRole("dialog", { name: queued.name })).toBeVisible();
    await expect(page.getByTestId("file-project")).toContainText(renamed);
    await page.keyboard.press("Escape");
    const queueGroup = groupTarget(layout!, (kind) => kind === "decision");
    expect(queueGroup.keys).toContain(listed.key);
    expect(await targetById(page, queueGroup.id).getAttribute("aria-label")).toContain(listed.name);
    await targetById(page, queueGroup.id).click();
    await expect(rowOf(page, listed.key)).toContainText(listed.name);
    await rowOf(page, listed.key).click();
    await expect(page.getByRole("dialog", { name: listed.name })).toBeVisible();
    const parsed = await page.evaluate(() => ({
      images: document.querySelectorAll("img[onerror]").length,
      bold: Array.from(document.querySelectorAll("b")).filter((b) => b.textContent === "bold").length,
      pwned: (window as { __pwned?: number }).__pwned,
    }));
    expect(parsed).toEqual({ images: 0, bold: 0, pwned: undefined });
  });
});

test.describe("the text alternative names the waiting count and the oldest waiting session", () => {
  for (const { name, data: board } of fixturesOf<Board>("board").filter((f) => f.data.columns.needs_you.length > 0)) {
    test(name, async ({ page }) => {
      await openScene(page, board);
      const waiting = board.columns.needs_you;
      const summary = page.getByTestId("scene-summary");
      await expect(summary).toContainText(`${waiting.length} ${waiting.length === 1 ? "needs" : "need"} you`);
      // Needs you is in board order, oldest first: its first card.
      await expect(summary).toContainText(`Oldest: ${waiting[0]!.name}`);
    });
  }
});

// D-566, AD-70 (story 5.10): opening a meeting marks the received figure
// with one change of the scene, which then stays still; closing it is one
// more change, and the ring is gone.
test("opening a meeting marks the figure once", async ({ page }) => {
  const board = loadBoard("scene-c");
  const placed = layoutScene({ board }, emptyMemory(board.floor)).employees.find((e) => e.spot !== null)!;
  await openScene(page, board);
  const meetingView = { ...fixtureOf<MeetingView>("meeting", "meeting-background"), key: placed.key };
  await page.evaluate((m) => (window.__officinaMeetings = { [m.key]: m }), meetingView);
  const ring = scene(page).locator(`[data-figure=${JSON.stringify(placed.key)}] [data-mark="received"]`);
  await expect(ring).toHaveCount(0);
  await scene(page).locator(`[data-layer="targets"] [data-key=${JSON.stringify(placed.key)}]`).click();
  await page.clock.runFor(1_000);
  await scene(page).evaluate((root) => {
    const records: MutationRecord[] = [];
    new MutationObserver((list) => records.push(...list)).observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    (window as unknown as { __receivedRecords: MutationRecord[] }).__receivedRecords = records;
  });
  const records = () => page.evaluate(() => (window as unknown as { __receivedRecords: MutationRecord[] }).__receivedRecords.length);
  await page.getByTestId("file-receive").click();
  await expect(page.getByTestId("meeting-panel").getByTestId("meeting-decision")).toBeVisible();
  await expect(ring).toHaveCount(1);
  const marked = await records();
  expect(marked).toBeGreaterThan(0);
  // Then nothing moves while nothing changes.
  await page.clock.runFor(150_000);
  expect(await records()).toBe(marked);
  await page.getByTestId("meeting-close").click();
  await expect(ring).toHaveCount(0);
});

// Story 5.18 (AD-179, D-567, AC 6 and AC 8): the orchestrator at the
// decider's desk, its targets, its file and its screens, on the
// `scene-orchestrator` board and view printed from one home.
test.describe("the orchestrator in the decision office", () => {
  const orchestratorTarget = (page: Page) => scene(page).locator('[data-target="orchestrator"]');
  const chairTarget = (page: Page) => scene(page).locator('[data-target="chair"]');
  const absent = (): Board => ({ ...loadBoard("scene-orchestrator"), orchestrator: null });

  async function openLive(page: Page): Promise<Board> {
    const board = loadBoard("scene-orchestrator");
    await answerOrchestrator(page, sceneView());
    await openScene(page, board);
    await expect(scene(page).locator("[data-orchestrator]")).toHaveCount(1);
    return board;
  }

  test("Tab reaches the orchestrator after the decision office's queue", async ({ page }) => {
    await openLive(page);
    const ids = await scene(page).locator("[data-target]").evaluateAll((all) => all.map((t) => t.getAttribute("data-target")!));
    const at = ids.indexOf("orchestrator");
    expect(at).toBeGreaterThan(0);
    await scene(page).focus();
    for (let i = 0; i <= at; i++) await page.keyboard.press("Tab");
    expect(await focusedTarget(page)).toBe("orchestrator");
    // What comes before it is the queue's: a waiting figure or its "+n".
    const before = ids[at - 1]!;
    const queued = await scene(page).locator(`[data-target=${JSON.stringify(before)}]`).getAttribute("data-key");
    const board = loadBoard("scene-orchestrator");
    expect(before.startsWith("group:decision") || board.columns.needs_you.some((c) => c.key === queued)).toBe(true);
    await expect(orchestratorTarget(page)).toHaveAccessibleName(/^Orchestrator, reads and proposes; decides nothing that runs, working, \d+ proposals? pending/);
    // After it, the alert room or the offices: never the queue again.
    const after = ids[at + 1];
    if (after) expect(after.startsWith("group:decision")).toBe(false);
  });

  test("Enter on the orchestrator opens the Orchestrator view", async ({ page }) => {
    await openLive(page);
    await orchestratorTarget(page).focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("orchestrator-view")).toBeVisible();
    await expect(page.getByTestId("view-orchestrator")).toHaveAttribute("aria-pressed", "true");
    expect((await calls(page)).filter((c) => c.cmd === "start_orchestrator")).toEqual([]);
  });

  test("the empty chair's file offers Start, which sends start_orchestrator", async ({ page }) => {
    await answerOrchestrator(page, { ...sceneView(), presence: null });
    await openScene(page, absent());
    await expect(scene(page).locator("[data-orchestrator]")).toHaveCount(0);
    await expect(orchestratorTarget(page)).toHaveCount(0);
    await expect(chairTarget(page)).toHaveAccessibleName(/^Orchestrator: absent/);
    await chairTarget(page).click();
    const file = sessionFile(page);
    await expect(file).toHaveAttribute("data-form", "orchestrator");
    await expect(file.getByRole("heading")).toHaveText("Orchestrator: absent");
    await expect(file.getByTestId("chair-about")).toContainText("decides nothing that runs");
    expect((await calls(page)).filter((c) => c.cmd === "start_orchestrator")).toEqual([]);
    await file.getByTestId("chair-start").click();
    await expect(file.getByTestId("chair-answer")).toContainText("type any first message");
    expect((await calls(page)).filter((c) => c.cmd === "start_orchestrator")).toHaveLength(1);
    // The link shows the Orchestrator view.
    await expect(file.getByTestId("chair-view")).toBeVisible();
    // Once the board lists it, the file closes and the figure sits there.
    await push(page, loadBoard("scene-orchestrator"));
    await expect(file).toHaveCount(0);
    await expect(scene(page).locator("[data-orchestrator]")).toHaveCount(1);
    await expect(orchestratorTarget(page)).toHaveCount(1);
  });

  test("Start is reachable by keyboard from the chair's file", async ({ page }) => {
    await answerOrchestrator(page, { ...sceneView(), presence: null });
    await openScene(page, absent());
    await chairTarget(page).focus();
    // Enter on the chair opens its file and starts nothing (rework F1).
    await page.keyboard.press("Enter");
    await expect(sessionFile(page)).toHaveAttribute("data-form", "orchestrator");
    expect(await focusedTestId(page)).toBe("file-close");
    expect((await calls(page)).filter((c) => c.cmd === "start_orchestrator")).toEqual([]);
    await page.keyboard.press("Tab");
    expect(await focusedTestId(page)).toBe("chair-start");
    await page.keyboard.press("Enter");
    await expect.poll(async () => (await calls(page)).filter((c) => c.cmd === "start_orchestrator").length).toBe(1);
    // Escape closes the file, focus back on the chair.
    await page.keyboard.press("Escape");
    await expect(sessionFile(page)).toHaveCount(0);
    expect(await focusedTarget(page)).toBe("chair");
  });

  test("nothing in the scene changes while the payloads are unchanged, the orchestrator live", async ({ page }) => {
    const board = await openLive(page);
    const view = sceneView();
    expect(view.proposals.some((p) => p.status === "pending")).toBe(true);
    expect(view.work_orders.some((o) => o.bound_at_ms !== null)).toBe(true);
    await expect(scene(page).locator("[data-whiteboard] [data-label]").first()).toBeVisible();
    const records = await observeScene(page);
    await page.clock.runFor(150_000);
    await push(page, structuredClone(board));
    await page.evaluate((v) => window.__officinaPushOrchestrator!(v), structuredClone(view));
    await page.clock.runFor(1_000);
    expect(await records()).toBe(0);
  });

  test("a presence or proposal change changes the decision office once", async ({ page }) => {
    const board = await openLive(page);
    const view = sceneView();
    const records = await observeScene(page);
    const settledCount = async () => {
      await sceneSettled(page);
      const count = await records();
      // Then nothing more, however long nothing changes.
      await page.clock.runFor(60_000);
      expect(await records()).toBe(count);
      return count;
    };
    // Fewer proposals: the left screen and the whiteboard, once.
    const fewer = { ...view, proposals: view.proposals.filter((p) => p.status === "pending").slice(0, 1) };
    await page.evaluate((v) => window.__officinaPushOrchestrator!(v), fewer);
    await expect(scene(page).locator('[data-display-screen="proposals"]')).toHaveText("1");
    await expect(scene(page).locator("[data-whiteboard] [data-label]")).toHaveCount(1);
    const afterProposal = await settledCount();
    expect(afterProposal).toBeGreaterThan(0);
    // Absent: the figure, the screens and the plaque's name, once.
    await push(page, { ...board, orchestrator: null });
    await expect(scene(page).locator("[data-orchestrator]")).toHaveCount(0);
    await expect(scene(page).locator("[data-display-screen]")).toHaveCount(0);
    await expect(scene(page).locator('[data-plaque="decision"]')).toContainText("Decisions: you");
    expect(await settledCount()).toBeGreaterThan(afterProposal);
  });
});

// Story 5.23 (AD-181, D-564): a session's auto-mode denials in its file, as
// neutral text, from the real `officina relay --json` of `relay-denials`,
// whose sessions are `scene-a`'s; a session without any shows no line.
for (const theme of ["light", "dark"] as const) {
  test(`the file counts a session's auto-mode denials (${theme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const board = loadBoard("scene-a");
    const relay = loadRelay("relay-denials");
    await openScene(page, board, relay);
    await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
    const cards = liveCards(board);
    expect(cards.length).toBeGreaterThan(1);
    // Nothing of the denials reaches the title, the alert or the waiting
    // count: the same as with a relay view that holds none.
    const shown = async () => ({
      title: await page.title(),
      waiting: await page.getByTestId("waiting-pill").textContent(),
      alert: await page.getByTestId("alert-band").count(),
    });
    const withDenials = await shown();
    for (const card of cards) {
      const counted = relay.auto_mode_denials.find((d) => d.session_id === card.session_id);
      expect(counted, card.key).toBeDefined();
      await figureOf(page, card.key).click();
      await expect(page.getByRole("dialog", { name: card.name })).toBeVisible();
      await expect(page.getByTestId("file-denials")).toHaveText(denialsText(counted!.count, relay.auto_mode_denials_window_ms));
      await expect(page.getByTestId("file-denials")).toHaveCSS("color", await page.evaluate(() => {
        const probe = document.createElement("span");
        probe.style.color = "var(--color-text)";
        document.body.append(probe);
        const colour = getComputedStyle(probe).color;
        probe.remove();
        return colour;
      }));
      await page.getByTestId("file-close").click();
      await expect(sessionFile(page)).toHaveCount(0);
    }
    // No denial, no line: the same board with a relay view holding none.
    await openScene(page, board, loadRelay("relay-clean"));
    expect(await shown()).toEqual(withDenials);
    await figureOf(page, cards[0]!.key).click();
    await expect(sessionFile(page)).toBeVisible();
    await expect(page.getByTestId("file-denials")).toHaveCount(0);
  });
}
