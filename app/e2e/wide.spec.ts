// The owner's screens (story 4.15, item 2, D-543): a 24-inch Full HD
// window maximised (1920 x 1032) at work, a 3440 x 1392 ultrawide at home.
// The board and the scene use the width; views of rows and tables stop at
// a reading width; no card stretches. Each case attaches its screenshot.
import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { loadBoard, loadRegistry, loadRelay, openBoard, openScene } from "./fixtures";

const SCREENS = [
  { width: 1920, height: 1032 },
  { width: 3440, height: 1392 },
] as const;

/** The widest a card may grow, in CSS px: a card is read, not stretched. */
const CARD_MAX_PX = 480;

async function attach(page: Page, info: TestInfo, name: string): Promise<void> {
  await info.attach(name, { body: await page.screenshot(), contentType: "image/png" });
}

for (const { width, height } of SCREENS) {
  test(`the board uses the width without stretching a card at ${width} x ${height}`, async ({ page }, info) => {
    await page.setViewportSize({ width, height });
    await openBoard(page, loadBoard("busy"));
    await attach(page, info, `board-${width}`);
    const columns = await page.locator('[data-testid^="column-"]').evaluateAll((all) => all.map((c) => c.getBoundingClientRect().right));
    expect(Math.max(...columns), "the columns reach the right edge").toBeGreaterThan(width - 60);
    const cards = await page.getByTestId("card").evaluateAll((all) => all.map((c) => c.getBoundingClientRect().width));
    expect(Math.max(...cards)).toBeLessThanOrEqual(CARD_MAX_PX);
    // Working's cards: one per line on Full HD, side by side on the ultrawide.
    const working = await page.getByTestId("column-working").getByTestId("card").evaluateAll((all) => all.map((c) => Math.round(c.getBoundingClientRect().top)));
    const perLine = working.filter((top) => top === working[0]).length;
    if (width >= 3000) expect(perLine).toBeGreaterThan(1);
    else expect(perLine).toBe(1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });

  test(`views to read stop at a reading width at ${width} x ${height}`, async ({ page }, info) => {
    await page.setViewportSize({ width, height });
    await openBoard(page, loadBoard("triage-board"), {}, loadRelay("relay-activity"), loadRegistry("triage"));
    const reading = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--reading-width")));
    for (const view of ["subjects", "relay"] as const) {
      await page.getByTestId(`view-${view}`).click();
      const main = page.getByTestId(`${view}-view`);
      await expect(main).toBeVisible();
      await attach(page, info, `${view}-${width}`);
      const box = (await main.boundingBox())!;
      expect(box.width).toBeLessThanOrEqual(reading + 0.5);
      // Centred in the room the window gives it.
      const room = (await main.evaluate((m) => {
        const r = m.parentElement!.getBoundingClientRect();
        return { x: r.x, w: r.width };
      }));
      expect(Math.abs(box.x - room.x - (room.x + room.w - box.x - box.width))).toBeLessThanOrEqual(1);
    }
  });

  test(`the session file docks beside the scene when there is room at ${width} x ${height}`, async ({ page }, info) => {
    await page.setViewportSize({ width, height });
    await openScene(page, loadBoard("scene-c"));
    await page.locator("[data-target]").first().click();
    const file = page.getByTestId("session-file");
    await expect(file).toBeVisible();
    await attach(page, info, `scene-file-${width}`);
    const scene = (await page.getByTestId("scene").boundingBox())!;
    const panel = (await file.boundingBox())!;
    const docked = scene.x + scene.width <= panel.x + 0.5;
    // The ultrawide has the room; a maximised Full HD window gives the
    // scene its whole width, and the file lies over its right edge.
    expect(docked).toBe(width >= 3000);
  });
}
