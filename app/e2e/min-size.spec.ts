// The window's minimum size (story 4.15, the owner's item 5, D-546), read
// from tauri.conf.json, checked against the layouts it is derived from:
// at that size the header holds on two lines with every control shown,
// and the base floor of the scene (four offices) is drawn whole at
// MIN_SCALE, with no scrolling, under it.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { BASE_COLUMNS, MIN_SCALE, canvas } from "../src/lib/scene/geometry";
import { loadBoard, loadRegistry, loadRelay, openBoard, sceneSettled } from "./fixtures";

const config = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../src-tauri/tauri.conf.json"), "utf8")) as {
  app: { windows: { label: string; minWidth: number; minHeight: number }[] };
};
const main = config.app.windows.find((w) => w.label === "main")!;

test("the minimum window holds the header on two lines and the base floor at MIN_SCALE", async ({ page }) => {
  // The base floor at the smallest scale fits the minimum width.
  expect(Math.ceil(canvas(BASE_COLUMNS).w * MIN_SCALE)).toBeLessThanOrEqual(main.minWidth);
  await page.setViewportSize({ width: main.minWidth, height: main.minHeight });
  // Every header control: the relay in use, rows to sort. A pending
  // catalogue's banner, which lists its diff, is left out: the sessions
  // area gives way to it, and the scene then scrolls (AD-61).
  await openBoard(page, loadBoard("scene-a"), {}, loadRelay("relay-clean"), loadRegistry("triage"));
  const lines = await page.locator("header.bar").evaluate((h) => {
    const items = Array.from(h.querySelectorAll(":scope > *, .tools > *, .ended > *"));
    return new Set(items.map((e) => Math.round(e.getBoundingClientRect().top))).size;
  });
  expect(lines, "lines of the header").toBeLessThanOrEqual(2);
  await page.getByTestId("mode-scene").click();
  await sceneSettled(page);
  const scene = page.getByTestId("scene");
  expect(Number(await scene.getAttribute("data-columns"))).toBe(BASE_COLUMNS);
  expect(Number(await scene.getAttribute("data-scale"))).toBeGreaterThanOrEqual(MIN_SCALE);
  const area = await scene.evaluate((s) => {
    const a = s.parentElement!;
    return { sw: a.scrollWidth, cw: a.clientWidth, sh: a.scrollHeight, ch: a.clientHeight };
  });
  expect(area.sw, "the scene scrolls sideways").toBeLessThanOrEqual(area.cw);
  expect(area.sh, "the scene scrolls down").toBeLessThanOrEqual(area.ch);
  // The board's five columns side by side.
  await page.getByTestId("mode-board").click();
  const tops = await page.locator('[data-testid^="column-"]').evaluateAll((all) => all.map((c) => Math.round(c.getBoundingClientRect().top)));
  expect(new Set(tops).size).toBe(1);
});
