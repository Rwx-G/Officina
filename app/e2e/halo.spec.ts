// The board's attention halo (story 4.4, AD-65, D-108): the ring on each
// Needs you card's `::after` breathes on opacity only, in one phase shared
// with the wall clock, for two minutes from the start of the wait, then
// rests; under reduced motion it rests from the start.
//
// Playwright's installed clock moves `Date.now`, not CSS animations, so a
// breathing case dates each Needs you card's wait at the installed time
// (D-101), and a case that needs time to pass for the animations waits in
// real time, which both clocks share.
import { expect, test, type Page } from "@playwright/test";
import { loadBoard, openBoard } from "./fixtures";
import { HALO_ANIMATION, HALO_BREATHING_MS, HALO_CYCLE_MS } from "../src/lib/halo";
import type { Board } from "../src/lib/generated/Board";

/** How far a measured phase may sit from the cycle: a frame or two between
 *  the browser starting an animation and the page reading the clock. */
const PHASE_TOLERANCE_MS = 200;
/** Real time let pass where a halo keyed on the wrong moment would show it:
 *  well beyond `PHASE_TOLERANCE_MS`. */
const DRIFT_MS = 1_500;

interface HaloAnimation {
  key: string | null;
  pseudoElement: string | null;
  playState: string;
  /** Animated properties, from the keyframes. */
  properties: string[];
  /** The wall-clock time the breathing counts from, read off the
   *  animation's position: `Date.now()` minus its active time. */
  startMs: number;
  /** When the breathing ends, in wall-clock time. */
  endMs: number;
}

/** The running `halo-breathe` animations of the page. */
function halos(page: Page): Promise<HaloAnimation[]> {
  return page.evaluate((name) => {
    const nowMs = Date.now();
    const meta = new Set(["offset", "computedOffset", "easing", "composite"]);
    return document
      .getAnimations()
      .filter((a): a is CSSAnimation => a instanceof CSSAnimation && a.animationName === name)
      .map((a) => {
        const effect = a.effect as KeyframeEffect;
        const timing = effect.getComputedTiming();
        const active = Number(timing.localTime) - Number(effect.getTiming().delay);
        const startMs = nowMs - active;
        return {
          key: (effect.target as HTMLElement | null)?.dataset.key ?? null,
          pseudoElement: effect.pseudoElement,
          playState: a.playState,
          properties: [...new Set(effect.getKeyframes().flatMap((k) => Object.keys(k)))].filter((p) => !meta.has(p)),
          startMs,
          endMs: startMs + Number(timing.activeDuration),
        };
      });
  }, HALO_ANIMATION);
}

async function runningHalos(page: Page): Promise<HaloAnimation[]> {
  return (await halos(page)).filter((halo) => halo.playState === "running");
}

/** Distance from `ms` to the nearest multiple of the cycle. */
function offPhase(ms: number): number {
  const into = ((ms % HALO_CYCLE_MS) + HALO_CYCLE_MS) % HALO_CYCLE_MS;
  return Math.min(into, HALO_CYCLE_MS - into);
}

/** `board` with every wait dated at `atMs` (D-101). */
function waitingSince(board: Board, atMs: number): Board {
  const next = structuredClone(board);
  for (const card of next.columns.needs_you) card.last_event_at_ms = atMs;
  return next;
}

/** Each Needs you card's ring: its resting opacity once the action ran. */
async function ringOpacities(page: Page, board: Board): Promise<{ rest: number; rings: number[] }> {
  for (const card of board.columns.needs_you) {
    await expect
      .poll(() => page.locator(`[data-key="${card.key}"]`).evaluate((el) => (el as HTMLElement).style.getPropertyValue("--halo-count")))
      .not.toBe("");
  }
  return page.evaluate((keys) => {
    const rest = Number(getComputedStyle(document.documentElement).getPropertyValue("--halo-rest"));
    const rings = keys.map((key) => Number(getComputedStyle(document.querySelector(`[data-key="${key}"]`)!, "::after").opacity));
    return { rest, rings };
  }, board.columns.needs_you.map((card) => card.key));
}

function realDelay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function openBreathing(page: Page): Promise<Board> {
  const generated = loadBoard("scene-c");
  const board = waitingSince(generated, generated.generated_at_ms);
  expect(board.columns.needs_you.length).toBeGreaterThan(1);
  await openBoard(page, board);
  await expect.poll(async () => (await runningHalos(page)).length).toBe(board.columns.needs_you.length);
  return board;
}

test.describe("board halo", () => {
  test("the board halo animates opacity only", async ({ page }) => {
    const board = await openBreathing(page);
    const running = await runningHalos(page);
    // Approval cards keep their amber border and get no halo.
    expect(board.approvals.length).toBeGreaterThan(0);
    expect(running.map((halo) => halo.key).sort()).toEqual(board.columns.needs_you.map((card) => card.key).sort());
    for (const halo of running) {
      expect(halo.pseudoElement, halo.key!).toBe("::after");
      expect(halo.properties, halo.key!).toEqual(["opacity"]);
    }
  });

  test("every halo breathes in the wall-clock phase for two minutes from its wait", async ({ page }) => {
    const board = await openBreathing(page);
    const enteredAt = board.generated_at_ms;
    for (const halo of await runningHalos(page)) {
      expect(offPhase(halo.startMs), halo.key!).toBeLessThan(PHASE_TOLERANCE_MS);
      expect(halo.endMs, halo.key!).toBeGreaterThanOrEqual(enteredAt + HALO_BREATHING_MS - PHASE_TOLERANCE_MS);
      expect(halo.endMs, halo.key!).toBeLessThan(enteredAt + HALO_BREATHING_MS + HALO_CYCLE_MS + PHASE_TOLERANCE_MS);
    }
  });

  test("board halos breathe without style recalculation", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "CDP metrics are Chromium's");
    await openBreathing(page);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Performance.enable");
    const recalcs = async (): Promise<number> => {
      const { metrics } = await cdp.send("Performance.getMetrics");
      return metrics.find((metric) => metric.name === "RecalcStyleCount")!.value;
    };
    await realDelay(100);
    const before = await recalcs();
    await realDelay(3_000);
    expect(await recalcs()).toBe(before);
  });

  test("under reduced motion the board ring is steady", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const generated = loadBoard("scene-c");
    const board = waitingSince(generated, generated.generated_at_ms);
    await openBoard(page, board);
    const { rest, rings } = await ringOpacities(page, board);
    expect(await halos(page)).toEqual([]);
    expect(rest).toBeGreaterThan(0);
    expect(rings).toEqual(board.columns.needs_you.map(() => rest));
  });

  test("a wait older than two minutes at first sight is steady", async ({ page }) => {
    const board = loadBoard("busy");
    expect(board.columns.needs_you.length).toBeGreaterThan(0);
    await openBoard(page, board);
    const { rest, rings } = await ringOpacities(page, board);
    expect(await runningHalos(page)).toEqual([]);
    expect(rings).toEqual(board.columns.needs_you.map(() => rest));
  });

  test("a session that waits again breathes again, in the shared phase", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    await ringOpacities(page, board);
    expect(await runningHalos(page)).toEqual([]);
    await realDelay(DRIFT_MS);
    const nowMs = await page.evaluate(() => Date.now());
    const next = structuredClone(board);
    const again = next.columns.needs_you[0]!;
    again.last_event_at_ms = nowMs;
    await page.evaluate((b) => window.__officinaPushBoard!(b), next);
    await expect.poll(async () => (await runningHalos(page)).map((halo) => halo.key)).toEqual([again.key]);
    const [halo] = await runningHalos(page);
    expect(offPhase(halo!.startMs)).toBeLessThan(PHASE_TOLERANCE_MS);
    expect(halo!.endMs).toBeGreaterThanOrEqual(nowMs + HALO_BREATHING_MS - PHASE_TOLERANCE_MS);
  });

  test("a session that leaves Needs you and comes back breathes again", async ({ page }) => {
    const board = await openBreathing(page);
    const leaving = board.columns.needs_you[0]!;
    const away = structuredClone(board);
    away.columns.needs_you = away.columns.needs_you.filter((card) => card.key !== leaving.key);
    away.columns.working.push({ ...leaving, column: "working" });
    await page.evaluate((b) => window.__officinaPushBoard!(b), away);
    await expect.poll(async () => (await runningHalos(page)).length).toBe(away.columns.needs_you.length);
    await realDelay(DRIFT_MS);
    const nowMs = await page.evaluate(() => Date.now());
    const back = structuredClone(board);
    back.columns.needs_you[0]!.last_event_at_ms = nowMs;
    await page.evaluate((b) => window.__officinaPushBoard!(b), back);
    await expect.poll(async () => (await runningHalos(page)).length).toBe(board.columns.needs_you.length);
    await expect
      .poll(async () => Math.max(...(await runningHalos(page)).map((halo) => offPhase(halo.startMs))))
      .toBeLessThan(PHASE_TOLERANCE_MS);
    const returned = (await runningHalos(page)).find((halo) => halo.key === leaving.key)!;
    expect(returned.endMs).toBeGreaterThanOrEqual(nowMs + HALO_BREATHING_MS - PHASE_TOLERANCE_MS);
  });

  test("cards that change places keep their phase and their end", async ({ page }) => {
    const board = await openBreathing(page);
    const before = new Map((await runningHalos(page)).map((halo) => [halo.key, halo]));
    await realDelay(DRIFT_MS);
    const reordered = structuredClone(board);
    reordered.columns.needs_you.reverse();
    await page.evaluate((b) => window.__officinaPushBoard!(b), reordered);
    const order = () =>
      page
        .getByTestId("column-needs_you")
        .locator("[data-testid=card]")
        .evaluateAll((cards) => cards.map((card) => card.getAttribute("data-key")));
    await expect.poll(order).toEqual(reordered.columns.needs_you.map((card) => card.key));
    await expect
      .poll(async () =>
        Math.max(
          ...(await runningHalos(page)).map((halo) =>
            Math.max(Math.abs(halo.startMs - before.get(halo.key)!.startMs), Math.abs(halo.endMs - before.get(halo.key)!.endMs)),
          ),
        ),
      )
      .toBeLessThan(PHASE_TOLERANCE_MS);
    expect((await runningHalos(page)).length).toBe(before.size);
  });

  test("a remount keeps the phase and the end of the breathing", async ({ page }) => {
    await openBreathing(page);
    const before = new Map((await runningHalos(page)).map((halo) => [halo.key, halo]));
    await realDelay(DRIFT_MS);
    await page.getByTestId("view-subjects").click();
    await expect(page.getByTestId("column-needs_you")).toHaveCount(0);
    await page.getByTestId("view-board").click();
    await expect.poll(async () => (await runningHalos(page)).length).toBe(before.size);
    for (const halo of await runningHalos(page)) {
      const first = before.get(halo.key)!;
      expect(Math.abs(halo.startMs - first.startMs), halo.key!).toBeLessThan(PHASE_TOLERANCE_MS);
      expect(Math.abs(halo.endMs - first.endMs), halo.key!).toBeLessThan(PHASE_TOLERANCE_MS);
    }
  });

  test("reduced motion turned off later resumes the shared phase, not a fresh breathing", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const generated = loadBoard("scene-c");
    const board = waitingSince(generated, generated.generated_at_ms);
    await openBoard(page, board);
    await ringOpacities(page, board);
    await realDelay(DRIFT_MS);
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await expect.poll(async () => (await runningHalos(page)).length).toBe(board.columns.needs_you.length);
    // The browser reports the start with the next frame; the delay follows.
    await expect
      .poll(async () => Math.max(...(await runningHalos(page)).map((halo) => offPhase(halo.startMs))))
      .toBeLessThan(PHASE_TOLERANCE_MS);
    for (const halo of await runningHalos(page)) {
      expect(halo.endMs, halo.key!).toBeLessThan(board.generated_at_ms + HALO_BREATHING_MS + HALO_CYCLE_MS + PHASE_TOLERANCE_MS);
    }
  });
});
