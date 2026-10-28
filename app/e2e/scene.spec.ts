// The scene drawn in place (story 4.7; AD-59, AD-61, AD-64, AD-70, AD-171,
// D-497): every live session of every fixture board drawn once, a changed
// board placing figures at once, nothing written while the payloads are
// unchanged, the scale following the sessions area, eight projects on the
// owner's screen (D-520, D-525), and the light baselines (D-119, D-508).
// Boards are `officina board --json` output (AD-17); expected places come
// from `layoutScene`, imported, never retyped.
import { existsSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { SCREENSHOT_MAX_DIFF_RATIO, loadBoard, openScene, sceneSettled } from "./fixtures";
import { fixturesOf } from "./fixture-index";
import { beaconPair } from "../src/lib/alert";
import { HALO_BREATHING_MS, HALO_CYCLE_MS } from "../src/lib/halo";
import { MIN_SCALE } from "../src/lib/scene/geometry";
import { layoutScene, liveCards, type SceneLayout } from "../src/lib/scene/layout";
import { STORAGE_KEY, emptyMemory, type SceneMemory } from "../src/lib/scene/memory";
import type { Board } from "../src/lib/generated/Board";

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

/** Every figure's transform and shown pose group, by card key. */
function figures(page: Page): Promise<Record<string, FigureState>> {
  return scene(page).evaluate((root) => {
    const out: Record<string, FigureState> = {};
    for (const node of Array.from(root.querySelectorAll("[data-figure]"))) {
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

test.describe("the scene draws every live session of each fixture", () => {
  for (const { name, data: board } of fixturesOf<Board>("board")) {
    test(name, async ({ page }) => {
      await openScene(page, board);
      const [layout] = layouts(board);
      const drawn = Object.keys(await figures(page)).sort();
      expect(drawn).toEqual(layout!.employees.filter((e) => e.spot !== null).map((e) => e.key).sort());
      const listed = layout!.groups.flatMap((g) => g.keys);
      const all = [...drawn, ...listed].sort();
      expect(all).toEqual(liveCards(board).map((c) => c.key).sort());
      expect(new Set(all).size).toBe(all.length);
    });
  }
});

test.describe("scene", () => {
  test("a changed board places the figure at its new place at once", async ({ page }) => {
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
    card.scene = { ...card.scene!, waiting_since_ms: board.generated_at_ms - HALO_BREATHING_MS + BEACON_LEFT_MS };
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
  // every target at least 44 CSS px.
  for (const alert of [false, true]) {
    test(`eight projects fit a maximised Full HD window${alert ? " with the alert band" : ""}`, async ({ page }) => {
      await page.setViewportSize({ width: 1920, height: 1001 });
      const board = loadBoard("scene-eight");
      if (alert) {
        const first = board.columns.needs_you[0]!;
        first.scene = { ...first.scene!, zone: "alert", alert: "priority" };
        board.alert_count = 1;
      }
      await openScene(page, board);
      if (alert) await expect(page.getByTestId("alert-band")).toBeVisible();
      expect(Number(await scene(page).getAttribute("data-columns"))).toBe(layouts(board)[0]!.columns);
      expect(Number(await scene(page).getAttribute("data-scale"))).toBeGreaterThanOrEqual(MIN_SCALE);
      const area = await scene(page).evaluate((s) => {
        const a = s.parentElement!;
        return { sw: a.scrollWidth, cw: a.clientWidth, sh: a.scrollHeight, ch: a.clientHeight };
      });
      expect(area.sw, "the sessions area scrolls sideways").toBeLessThanOrEqual(area.cw);
      expect(area.sh, "the sessions area scrolls down").toBeLessThanOrEqual(area.ch);
    });
  }

  test("the three densities match their light baselines", async ({ page }, testInfo) => {
    const densities = ["scene-a", "scene-b", "scene-c"] as const;
    const updating = testInfo.config.updateSnapshots === "all" || testInfo.config.updateSnapshots === "changed";
    const missing = densities.filter((name) => !existsSync(testInfo.snapshotPath(`${name}-light.png`)));
    // D-508: the baselines come from the CI snapshot run and are committed
    // only after the owner compared them with the prototype's render.
    test.fixme(missing.length > 0 && !updating, `baselines not committed yet (D-508): ${missing.join(", ")}`);
    await page.emulateMedia({ colorScheme: "light" });
    await openScene(page, loadBoard(densities[0]));
    for (const name of densities) {
      await reopenScene(page, loadBoard(name));
      await expect(scene(page)).toHaveScreenshot(`${name}-light.png`, {
        maxDiffPixelRatio: SCREENSHOT_MAX_DIFF_RATIO,
        animations: "disabled",
      });
    }
  });
});
