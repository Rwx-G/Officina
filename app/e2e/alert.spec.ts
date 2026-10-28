// The alert band (story 4.5; FR-10, AD-175, D-473, D-494, D-496): who it
// names and why, in every view, how it counts and leaves, the time
// escalation from two boards the real CLI built either side of the
// threshold, and no theme change. The alert room (story 4.9; AD-65,
// AD-175, D-474): lit while occupied, its beacon's glow, the high-priority
// pill and the escalation seen in the scene. Boards are `officina board
// --json` output (AD-17); expected places come from `layoutScene`,
// imported, never retyped.
import { expect, test, type Page } from "@playwright/test";
import { loadBoard, loadRelay, openBoard, openScene, sceneSettled } from "./fixtures";
import { beaconPair } from "../src/lib/alert";
import { HALO_ANIMATION, HALO_BREATHING_MS, HALO_CYCLE_MS } from "../src/lib/halo";
import { ALERT_FLOOR, ALERT_PAINTS, paint } from "../src/lib/scene/furniture";
import { layoutScene, liveCards, type SceneLayout } from "../src/lib/scene/layout";
import { emptyMemory, type SceneMemory } from "../src/lib/scene/memory";
import { sharedSpots } from "../src/lib/scene/places";
import { SIGN_WORDS } from "../src/lib/scene/text";
import type { Board } from "../src/lib/generated/Board";

const band = (page: Page) => page.getByTestId("alert-band");
const bandText = (page: Page) => page.getByTestId("alert-band-text");

function push(page: Page, board: Board): Promise<void> {
  return page.evaluate((b) => window.__officinaPushBoard!(b), board);
}

/** The board's alerted cards, in board order. */
function alerted(board: Board) {
  const { needs_you, your_turn, working } = board.columns;
  return [...needs_you, ...your_turn, ...working].filter((c) => c.scene?.alert);
}

/** The theme's custom properties resolved on `:root`: the window's colours
 *  and the scene's materials, figures and people. */
function colourTokens(page: Page): Promise<Record<string, string>> {
  return page.evaluate(() => {
    const style = getComputedStyle(document.documentElement);
    const families = ["--color-", "--scene-", "--figure-", "--skin-", "--hair-"];
    const out: Record<string, string> = {};
    for (const sheet of Array.from(document.styleSheets)) {
      for (const rule of Array.from(sheet.cssRules)) {
        if (!(rule instanceof CSSStyleRule)) continue;
        for (const name of Array.from(rule.style)) {
          if (families.some((family) => name.startsWith(family))) out[name] = style.getPropertyValue(name).trim();
        }
      }
    }
    return out;
  });
}

test.describe("alert band", () => {
  test("the band names the first alerted session, its reason and cause", async ({ page }) => {
    const board = loadBoard("scene-alert");
    await openBoard(page, board);
    const first = alerted(board)[0]!;
    await expect(band(page)).toBeVisible();
    await expect(band(page)).toHaveAttribute("role", "status");
    await expect(bandText(page)).toContainText(`${first.name} (${first.project.name}) is blocked:`);
    await expect(bandText(page)).toContainText(first.waiting_for ?? "");
    await expect(bandText(page)).toContainText(first.scene!.alert === "priority" ? "high-priority project" : "waiting");
  });

  test("the band shows in every view and mode", async ({ page }) => {
    await openBoard(page, loadBoard("scene-alert"), {}, loadRelay("relay-clean"));
    for (const view of ["subjects", "relay", "sessions"]) {
      await page.getByTestId(`view-${view}`).click();
      await expect(band(page)).toBeVisible();
    }
    // Scene mode: the band stays under the header, above the scene.
    await page.getByTestId("mode-scene").click();
    await expect(page.getByTestId("scene")).toBeVisible();
    await expect(band(page)).toBeVisible();
    const bandBox = (await band(page).boundingBox())!;
    expect(bandBox.y + bandBox.height).toBeLessThanOrEqual((await page.getByTestId("scene").boundingBox())!.y);
    for (const view of ["subjects", "relay", "sessions"]) {
      await page.getByTestId(`view-${view}`).click();
      await expect(band(page)).toBeVisible();
    }
  });

  test("the band counts the other alerted sessions", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const others = alerted(board).length - 1;
    expect(others).toBeGreaterThan(0);
    await openBoard(page, board);
    await expect(bandText(page)).toContainText(`, and ${others} more.`);
  });

  test("the band leaves with the last alert", async ({ page }) => {
    await openBoard(page, loadBoard("scene-alert"));
    await expect(band(page)).toBeVisible();
    await push(page, loadBoard("scene-c"));
    await expect(band(page)).toHaveCount(0);
  });

  // In both themes (story 4.13): the system's scheme decides, on Auto.
  for (const theme of ["light", "dark"] as const) {
    test(`an alert changes no theme token (${theme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: theme });
      await openScene(page, loadBoard("scene-c"));
      const switcher = page.getByTestId("theme-switch");
      await expect(switcher).toHaveAttribute("data-theme-resolved", theme);
      const attribute = () => page.evaluate(() => document.documentElement.getAttribute("data-theme"));
      const before = await attribute();
      const calm = await colourTokens(page);
      expect(Object.keys(calm).length).toBeGreaterThan(5);
      expect(Object.keys(calm).filter((name) => name.startsWith("--scene-")).length).toBeGreaterThan(5);
      await push(page, loadBoard("scene-alert"));
      await expect(band(page)).toBeVisible();
      expect(await colourTokens(page)).toEqual(calm);
      expect(await attribute()).toBe(before);
      await expect(switcher).toHaveAttribute("data-theme-resolved", theme);
      await expect(switcher).toHaveAttribute("data-theme-choice", "auto");
    });
  }

  // Two boards the real CLI built, the timed wait one minute short of the
  // floor's threshold, then past it: the band follows from the board alone.
  test("a wait that reaches the threshold raises the band", async ({ page }) => {
    const early = loadBoard("scene-alert-early");
    const late = loadBoard("scene-alert");
    const timed = late.columns.needs_you.find((c) => c.scene?.alert === "waited")!;
    // The same session in both boards: its session id (the fixture script
    // numbers each scenario's processes on).
    const before = early.columns.needs_you.find((c) => c.session_id === timed.session_id);
    expect(before?.scene?.zone).toBe("decision");
    expect(before?.scene?.alert).toBeNull();
    await openBoard(page, early);
    await expect(bandText(page)).not.toContainText("and 1 more");
    await push(page, late);
    await expect(bandText(page)).toContainText("and 1 more");
  });

  test("an approval-only wait on a normal project raises no band", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const approvalOnly = board.columns.working.find((c) => c.scene?.approval)!;
    expect(approvalOnly.scene!.alert).toBeNull();
    expect(approvalOnly.project.priority).toBe("normal");
    expect(approvalOnly.scene!.zone).toBe("decision");
    await openBoard(page, board);
    await expect(bandText(page)).not.toContainText(approvalOnly.name);
    // Alone, it raises no band at all; core's vectors cover a request older
    // than the threshold (tests/vectors/scene.json).
    const alone: Board = {
      ...board,
      alert_count: 0,
      columns: { ...board.columns, needs_you: [] },
    };
    await push(page, alone);
    await expect(band(page)).toHaveCount(0);
  });
});

const scene = (page: Page) => page.getByTestId("scene");

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

const alertRoomOf = (layout: SceneLayout) => layout.rooms.find((r) => r.kind === "alert")!;

/** `board` with every alerted session answered: Working in its office. */
function alertsAnswered(board: Board): Board {
  const next = structuredClone(board);
  const answered = new Set(alerted(next).map((c) => c.key));
  const moved = liveCards(next).filter((c) => answered.has(c.key));
  next.columns.needs_you = next.columns.needs_you.filter((c) => !answered.has(c.key));
  next.columns.working = next.columns.working.filter((c) => !answered.has(c.key));
  for (const c of moved) {
    next.columns.working.push({ ...c, column: "working", waiting_for: null, scene: { ...c.scene!, zone: "office", alert: null, approval: false } });
  }
  next.alert_count = 0;
  next.waiting_count = next.columns.needs_you.length;
  return next;
}

/** `board` with the beacon's wait, and its last event, dated `atMs` (D-101). */
function beaconSince(board: Board, atMs: number): Board {
  const next = structuredClone(board);
  const key = beaconPair(next)!.key;
  const card = liveCards(next).find((c) => c.key === key)!;
  card.last_event_at_ms = atMs;
  card.scene = { ...card.scene!, waiting_since_ms: atMs };
  return next;
}

interface Glow {
  playState: string;
  properties: string[];
  /** When the breathing ends, in wall-clock time. */
  endMs: number;
}

interface Running {
  target: string;
  properties: string[];
}

/** The scene's animations: the beacon's `halo-breathe`, and every one
 *  running. */
function animations(page: Page): Promise<{ beacon: Glow[]; running: Running[] }> {
  return page.evaluate((name) => {
    const nowMs = Date.now();
    const meta = new Set(["offset", "computedOffset", "easing", "composite"]);
    const targetOf = (a: Animation) => (a.effect as KeyframeEffect | null)?.target ?? null;
    const inScene = document.getAnimations().filter((a) => {
      const target = targetOf(a);
      return target instanceof Element && target.closest("[data-testid=scene]") !== null;
    });
    const properties = (a: Animation) =>
      [...new Set((a.effect as KeyframeEffect).getKeyframes().flatMap((k) => Object.keys(k)))].filter((p) => !meta.has(p));
    const beacon = inScene
      .filter((a) => a instanceof CSSAnimation && a.animationName === name && (targetOf(a) as Element).hasAttribute("data-beacon"))
      .map((a) => {
        const effect = a.effect as KeyframeEffect;
        const timing = effect.getComputedTiming();
        const startMs = nowMs - (Number(timing.localTime) - Number(effect.getTiming().delay));
        return { playState: a.playState, properties: properties(a), endMs: startMs + Number(timing.activeDuration) };
      });
    const running = inScene
      .filter((a) => a.playState === "running")
      .map((a) => ({ target: (targetOf(a) as Element).outerHTML.slice(0, 80), properties: properties(a) }));
    return { beacon, running };
  }, HALO_ANIMATION);
}

/** How far a measured end may sit from the rule: a frame or two between
 *  the browser starting an animation and the page reading the clock. */
const END_TOLERANCE_MS = 200;

/** The alert room's lit parts as drawn: floor, hazard, lamp, beam, status
 *  board, partitions, plaque and beacon. */
async function alertRoom(page: Page) {
  const root = scene(page);
  const floor = root.locator("[data-alert-floor]");
  return {
    floor: { fill: await floor.getAttribute("fill"), stroke: await floor.getAttribute("stroke") },
    hazard: await root.locator("[data-hazard]").count(),
    lamp: await root.locator("[data-beacon-lamp]").getAttribute("fill"),
    beam: await root.locator("[data-beacon-beam]").getAttribute("display"),
    panel: await root.locator("[data-status-panel]").getAttribute("fill"),
    mark: await root.locator("[data-status-mark]").getAttribute("fill"),
    partitions: [...new Set(await root.locator('[data-partition="alert"] [data-top]').evaluateAll((tops) => tops.map((t) => t.getAttribute("fill"))))],
    plaque: (await root.locator('[data-plaque="alert"] .plaque').getAttribute("class"))!.split(/\s+/),
    beacon: await root.locator("[data-beacon]").count(),
  };
}

const haloColour = (page: Page, key: string) =>
  scene(page)
    .locator(`[data-halo="${key}"]`)
    .evaluate((el) => (el as HTMLElement).style.getPropertyValue("--halo-colour"));

test.describe("alert room", () => {
  test("the alert room lights up while occupied and goes quiet when it empties", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const answered = alertsAnswered(board);
    const [lit, quiet] = layouts(board, answered);
    expect(alertRoomOf(lit!).plaque.count).toBeGreaterThan(0);
    expect(alertRoomOf(quiet!).plaque.count).toBe(0);
    await openScene(page, board);
    const on = await alertRoom(page);
    expect(on).toMatchObject({
      floor: ALERT_FLOOR,
      hazard: 1,
      lamp: paint(ALERT_PAINTS.lamp.lit),
      beam: null,
      panel: paint(ALERT_PAINTS.panel.lit),
      mark: paint(ALERT_PAINTS.mark.lit),
      partitions: [paint(ALERT_PAINTS.partition.lit)],
      beacon: 1,
    });
    expect(on.plaque).toContain("alert");
    await expect(scene(page).locator('[data-plaque="alert"]')).toContainText(String(alertRoomOf(lit!).plaque.count));
    // Every alerted session answered: the room empties and goes quiet.
    await push(page, answered);
    await expect(scene(page).locator("[data-hazard]")).toHaveCount(0);
    await sceneSettled(page);
    const off = await alertRoom(page);
    expect(off).toMatchObject({
      floor: { fill: paint("scene-room"), stroke: paint("scene-room-edge") },
      hazard: 0,
      lamp: paint(ALERT_PAINTS.lamp.quiet),
      beam: "none",
      panel: paint(ALERT_PAINTS.panel.quiet),
      mark: paint(ALERT_PAINTS.mark.quiet),
      partitions: [paint(ALERT_PAINTS.partition.quiet)],
      beacon: 0,
    });
    expect(off.plaque).toContain("quiet");
  });

  // D-494: an alerted employee whose data goes unknown keeps its place and
  // its alert, so the room it stands in stays lit.
  test("missing data keeps the alert room lit", async ({ page }) => {
    const board = loadBoard("scene-alert");
    await openScene(page, board);
    const stale = structuredClone(board);
    stale.stale = { since_ms: board.generated_at_ms, error: "agents snapshot failed" };
    for (const card of liveCards(stale)) card.scene = card.scene && { ...card.scene, zone: "unknown" };
    await push(page, stale);
    await expect(scene(page).locator('[data-figure] [data-mark="question"]')).toHaveCount(liveCards(stale).length);
    await sceneSettled(page);
    const [, kept] = layouts(board, stale);
    expect(alertRoomOf(kept!).plaque.count).toBe(alerted(board).length);
    expect(await alertRoom(page)).toMatchObject({ floor: ALERT_FLOOR, hazard: 1, lamp: paint(ALERT_PAINTS.lamp.lit), beacon: 1 });
    await expect(scene(page).locator("[data-beacon]")).toHaveAttribute("data-beacon", beaconPair(stale)!.key);
    // Their tickets keep the alert colour.
    for (const card of alerted(board)) {
      await expect(scene(page).locator(`[data-figure="${card.key}"] [data-mark="ticket"] circle`)).toHaveAttribute("fill", paint("color-alert"));
    }
  });

  test("the beacon glow animates opacity only and stops after two minutes", async ({ page }) => {
    const generated = loadBoard("scene-alert");
    const board = beaconSince(generated, generated.generated_at_ms);
    await openScene(page, board);
    await expect(scene(page).locator("[data-beacon]")).toHaveAttribute("data-beacon", beaconPair(board)!.key);
    await expect.poll(async () => (await animations(page)).beacon.length).toBe(1);
    const { beacon, running } = await animations(page);
    expect(beacon[0]!.playState).toBe("running");
    expect(beacon[0]!.properties).toEqual(["opacity"]);
    expect(beacon[0]!.endMs).toBeGreaterThanOrEqual(board.generated_at_ms + HALO_BREATHING_MS - END_TOLERANCE_MS);
    expect(beacon[0]!.endMs).toBeLessThan(board.generated_at_ms + HALO_BREATHING_MS + HALO_CYCLE_MS + END_TOLERANCE_MS);
    // Nothing in the scene animates anything but opacity: the beam is static.
    expect(running.length).toBeGreaterThan(0);
    for (const animation of running) expect(animation.properties, animation.target).toEqual(["opacity"]);
  });

  test("under reduced motion the beacon is steady", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const generated = loadBoard("scene-alert");
    await openScene(page, beaconSince(generated, generated.generated_at_ms));
    const beacon = scene(page).locator("[data-beacon]");
    await expect(beacon).toHaveCount(1);
    // Its timing says breathe: reduced motion alone holds it steady.
    await expect.poll(async () => Number(await beacon.evaluate((el) => (el as HTMLElement).style.getPropertyValue("--halo-count")))).toBeGreaterThan(0);
    const { beacon: glows, running } = await animations(page);
    expect(glows).toEqual([]);
    expect(running).toEqual([]);
    const { opacity, rest } = await beacon.evaluate((el) => {
      const style = getComputedStyle(el);
      return { opacity: Number(style.opacity), rest: Number(style.getPropertyValue("--halo-rest")) };
    });
    expect(rest).toBeGreaterThan(0);
    expect(opacity).toBe(rest);
  });

  test("a high-priority office carries its pill", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const [layout] = layouts(board);
    const high = layout!.rooms.filter((r) => r.slot !== null && r.plaque.highPriority);
    expect(high.length).toBeGreaterThan(0);
    await openScene(page, board);
    await expect(scene(page).locator("[data-priority]")).toHaveCount(high.length);
    for (const room of high) {
      const pill = scene(page).locator(`[data-priority="${room.id}"]`);
      await expect(pill).toHaveText(SIGN_WORDS.highPriority);
      // Above its office's plaque (D-524), not beside it.
      const p = (await pill.boundingBox())!;
      const plaque = (await scene(page).locator(`[data-plaque="${room.id}"]`).boundingBox())!;
      expect(p.y + p.height).toBeLessThanOrEqual(plaque.y);
      expect(Math.abs(p.x + p.width / 2 - (plaque.x + plaque.width / 2))).toBeLessThan(1);
    }
  });

  // The same two boards as the band's case: the scene follows from the
  // board alone, once nothing walks.
  test("a wait that reaches the threshold moves to the alert room", async ({ page }) => {
    const early = loadBoard("scene-alert-early");
    const late = loadBoard("scene-alert");
    const timed = late.columns.needs_you.find((c) => c.scene?.alert === "waited")!;
    const before = early.columns.needs_you.find((c) => c.session_id === timed.session_id)!;
    const [first, second] = layouts(early, late);
    expect(sharedSpots("decision").map((s) => s.id)).toContain(first!.employees.find((e) => e.key === before.key)!.spot);
    const after = second!.employees.find((e) => e.key === timed.key)!;
    expect(sharedSpots("alert").map((s) => s.id)).toContain(after.spot);
    expect(alertRoomOf(second!).plaque.count).toBe(alertRoomOf(first!).plaque.count + 1);

    await openScene(page, early);
    await expect(scene(page).locator('[data-plaque="alert"]')).toContainText(String(alertRoomOf(first!).plaque.count));
    await push(page, late);
    await expect(scene(page).locator(`[data-figure="${timed.key}"]`)).toHaveCount(1);
    await sceneSettled(page);
    const scale = Number(await scene(page).getAttribute("data-scale"));
    const transform = (await scene(page).locator(`[data-figure="${timed.key}"]`).getAttribute("transform"))!;
    const [, x, y] = /translate\(([-\d.e]+) ([-\d.e]+)\)/.exec(transform)!;
    expect(Math.abs(Number(x) - Math.round(after.at!.x * scale) / scale)).toBeLessThan(1e-6);
    expect(Math.abs(Number(y) - Math.round(after.at!.y * scale) / scale)).toBeLessThan(1e-6);
    await expect(scene(page).locator('[data-plaque="alert"]')).toContainText(String(alertRoomOf(second!).plaque.count));
    expect((await alertRoom(page)).hazard).toBe(1);
    expect(await haloColour(page, timed.key)).toBe(paint("color-alert"));
  });

  test("an approval-only wait on a normal project stays in the queue", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const approvalOnly = board.columns.working.find((c) => c.scene?.approval)!;
    expect(approvalOnly.project.priority).toBe("normal");
    expect(approvalOnly.scene!.alert).toBeNull();
    const [layout] = layouts(board);
    expect(sharedSpots("decision").map((s) => s.id)).toContain(layout!.employees.find((e) => e.key === approvalOnly.key)!.spot);
    await openScene(page, board);
    await expect(scene(page).locator(`[data-figure="${approvalOnly.key}"]`)).toHaveCount(1);
    expect(await haloColour(page, approvalOnly.key)).toBe(paint("color-attention"));
  });
});
