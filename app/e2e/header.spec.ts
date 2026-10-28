// One header in every view and mode (story 4.15, the owner's item 9,
// D-540): the quota gauges, the buttons and the status chips have the same
// boxes whatever the window shows below them, at the owner's screens.
import { expect, test, type Page } from "@playwright/test";
import { loadBoard, loadRegistry, loadRelay, openBoard, sceneSettled } from "./fixtures";

/** What the window shows below the header, and how to get there. */
const SHOWN: { name: string; go: (page: Page) => Promise<void> }[] = [
  { name: "Board", go: (page) => page.getByTestId("mode-board").click() },
  {
    name: "Scene",
    go: async (page) => {
      await page.getByTestId("mode-scene").click();
      await sceneSettled(page);
    },
  },
  { name: "Focus", go: (page) => page.getByTestId("mode-focus").click() },
  {
    name: "Subjects",
    go: async (page) => {
      await page.getByTestId("view-subjects").click();
      await expect(page.getByTestId("subjects-view")).toBeVisible();
    },
  },
  {
    name: "Triage",
    go: async (page) => {
      await page.getByTestId("to-sort-chip").click();
      await expect(page.getByTestId("triage-panel")).toBeVisible();
    },
  },
  {
    name: "Relay",
    go: async (page) => {
      await page.getByTestId("view-relay").click();
      await expect(page.getByTestId("relay-view")).toBeVisible();
    },
  },
];

/** Every box of the header, by name: the gauges, every button and chip,
 *  the logo; the header's own box too. */
function headerBoxes(page: Page): Promise<Record<string, { x: number; y: number; w: number; h: number }>> {
  return page.locator("header.bar").evaluate((header) => {
    // A view that scrolled (the Triage panel takes the focus) is measured
    // from the top, as its header shows there.
    window.scrollTo(0, 0);
    const out: Record<string, { x: number; y: number; w: number; h: number }> = {};
    const items = [header, ...Array.from(header.querySelectorAll('[data-testid], button, [role="meter"]'))];
    for (const [i, el] of items.entries()) {
      const r = el.getBoundingClientRect();
      const name = el === header ? "header" : `${el.getAttribute("data-testid") ?? el.textContent?.trim() ?? el.tagName}#${i}`;
      out[name] = { x: r.x, y: r.y, w: r.width, h: r.height };
    }
    return out;
  });
}

for (const [width, height] of [
  [1920, 1032],
  [3440, 1392],
] as const) {
  test(`the header is the same in every view and mode at ${width} x ${height}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    // A board with chips (waiting, contract held), the relay in use, rows
    // to sort: every kind of header control is shown.
    await openBoard(page, loadBoard("triage-board"), {}, loadRelay("relay-clean"), loadRegistry("triage"));
    await expect(page.getByTestId("contract")).toHaveText("contract held");
    await expect(page.getByTestId("to-sort-chip")).toBeVisible();
    const first = await headerBoxes(page);
    expect(Object.keys(first).some((k) => k.startsWith("gauge-5h"))).toBe(true);
    for (const shown of SHOWN) {
      // Every view is reached from the Sessions view, as the author would.
      await page.getByTestId("view-sessions").click();
      await shown.go(page);
      expect(await headerBoxes(page), shown.name).toEqual(first);
    }
    // On one line on both screens (D-525, D-526).
    const tops = await page.locator("header.bar > *").evaluateAll((items) => items.map((e) => Math.round(e.getBoundingClientRect().top)));
    expect(new Set(tops).size, `the header's items start at ${tops.join(", ")}`).toBe(1);
  });
}
