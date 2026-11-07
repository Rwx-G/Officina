import { expect, test } from "@playwright/test";
import { loadBoard, openBoard } from "./fixtures";

// The readiness wait itself (story 6.19): a stylesheet whose request
// failed is loaded again, never waited on.

test("a failed stylesheet request is loaded again before any measure", async ({ page }) => {
  let aborted = 0;
  await page.route("**/*.css", async (route) => {
    if (aborted === 0) {
      aborted += 1;
      await route.abort("connectionreset");
    } else {
      await route.continue();
    }
  });
  await openBoard(page, loadBoard("busy"));
  expect(aborted).toBe(1);
  const surface = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--color-surface").trim());
  expect(surface).not.toBe("");
  const noted = test.info().annotations.filter((a) => a.type === "stylesheet reloaded");
  expect(noted).toHaveLength(1);
  expect(noted[0].description).toContain("status no entry");
});
