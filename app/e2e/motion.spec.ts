// The window's transitions (story 4.15, the owner's item 8, D-545): short
// fades and slides when views, modes, cards, panels, menus, the alert band
// and the header's chips come and go; opacity and transform only; none
// under reduced motion; none left running once they end (FR-9).
import { expect, test, type Page } from "@playwright/test";
import { MOTION_MS } from "../src/lib/motion";
import { loadBoard, loadRelay, openBoard, sceneSettled, transitionsDone } from "./fixtures";

/** What a Web Animation started by a transition does, read as it runs. */
interface Seen {
  properties: string[];
  duration: number;
}

/** Starts recording every transition the page runs from now on. */
async function record(page: Page): Promise<void> {
  await page.evaluate(() => {
    const seen: Seen[] = [];
    (window as unknown as { __seen: Seen[] }).__seen = seen;
    const original = Element.prototype.animate;
    Element.prototype.animate = function (keyframes, options) {
      const animation = original.call(this, keyframes, options);
      const frames = animation.effect instanceof KeyframeEffect ? animation.effect.getKeyframes() : [];
      const properties = [...new Set(frames.flatMap((f) => Object.keys(f)))].filter((k) => !["offset", "computedOffset", "easing", "composite"].includes(k));
      const timing = animation.effect?.getTiming();
      seen.push({ properties, duration: Number(timing?.duration ?? 0) });
      return animation;
    };
  });
}

async function seen(page: Page): Promise<Seen[]> {
  return page.evaluate(() => (window as unknown as { __seen: Seen[] }).__seen);
}

/** Switches modes and views, opens a card's menu, shows the alert band
 *  and the header's chips. */
async function drive(page: Page): Promise<void> {
  await page.getByTestId("mode-scene").click();
  await sceneSettled(page);
  await page.getByTestId("mode-focus").click();
  await page.getByTestId("mode-board").click();
  await page.getByTestId("view-subjects").click();
  await page.getByTestId("view-relay").click();
  await page.getByTestId("view-sessions").click();
  const ended = page.getByTestId("column-to_resume").getByTestId("card-menu").first();
  await ended.click();
  await page.keyboard.press("Escape");
  await page.evaluate(() => {
    const board = window.__officinaBoard!;
    const first = board.columns.needs_you[0]!;
    return window.__officinaPushBoard?.({
      ...board,
      alert_count: 1,
      columns: { ...board.columns, needs_you: [{ ...first, scene: { ...first.scene!, zone: "alert", alert: "priority" } }, ...board.columns.needs_you.slice(1)] },
    });
  });
  await expect(page.getByTestId("alert-band")).toBeVisible();
  await page.evaluate(() => window.__officinaPushRelay?.({ ...window.__officinaRelay!, stopped: true }));
  await expect(page.getByTestId("relay-stopped-chip")).toBeVisible();
}

test("every transition is short and moves opacity or transform only, and none outlives itself", async ({ page }) => {
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  await record(page);
  await drive(page);
  const all = await seen(page);
  expect(all.filter((one) => one.duration > 0).length, "transitions ran").toBeGreaterThan(5);
  for (const one of all) {
    expect(one.duration).toBeLessThanOrEqual(MOTION_MS);
    for (const property of one.properties) expect(["opacity", "transform"], property).toContain(property);
  }
  await transitionsDone(page);
});

test("under reduced motion no transition takes any time", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  await record(page);
  await drive(page);
  expect((await seen(page)).filter((one) => one.duration > 0)).toEqual([]);
});
