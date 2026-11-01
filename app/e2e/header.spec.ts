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
      await page.getByTestId("work-counts").click();
      await page.getByTestId("counts-to_sort").click();
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
    // A board with the relay in use and rows to sort: every kind of header
    // control is shown. Its contract holds, which the header does not show
    // (D-679).
    await openBoard(page, loadBoard("triage-board"), {}, loadRelay("relay-clean"), loadRegistry("triage"));
    await expect(page.getByTestId("work-count-to_sort")).toHaveText(String(loadRegistry("triage").to_sort));
    await expect(page.getByTestId("contract")).toHaveCount(0);
    const first = await headerBoxes(page);
    expect(Object.keys(first).some((k) => k.startsWith("gauge-week"))).toBe(true);
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

// Story 4.20 item 2 (the owner's revision of D-541, D-1041): To sort, To
// resume and Done are one fixed-size indicator whose panel names each
// count with its action, opened on hover, on keyboard focus and on a click,
// closed by Escape.
test.describe("the sessions indicator", () => {
  const indicator = (page: Page) => page.getByTestId("work-counts");
  const panel = (page: Page) => page.getByTestId("work-counts-panel");

  test("it shows the three counts and its panel opens on hover, on focus and on a click, Escape closing it", async ({ page }) => {
    const board = loadBoard("busy");
    const registry = loadRegistry("triage");
    await openBoard(page, board, {}, null, registry);
    await expect(indicator(page)).toHaveAccessibleName(
      `Sessions: ${registry.to_sort} to sort, ${board.columns.to_resume.length} to resume, ${board.columns.done.length} done`,
    );
    await expect(panel(page)).toHaveCount(0);
    // Hover opens it, leaving closes it.
    await indicator(page).hover();
    await expect(panel(page)).toBeVisible();
    await expect(panel(page).getByRole("button")).toHaveText([/to sort/, /to resume/, /done/]);
    await page.mouse.move(5, 500);
    await expect(panel(page)).toHaveCount(0);
    // A click keeps it open after the pointer leaves; a second closes it.
    await indicator(page).click();
    await page.mouse.move(5, 500);
    await expect(panel(page)).toBeVisible();
    await indicator(page).click();
    await page.mouse.move(5, 500);
    await expect(panel(page)).toHaveCount(0);
    // The keyboard: focus opens it, Tab walks its rows, Escape closes it
    // and gives the focus back.
    await page.locator("body").click({ position: { x: 5, y: 500 } });
    await indicator(page).focus();
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Tab");
    await expect(indicator(page)).toBeFocused();
    await expect(panel(page)).toBeVisible();
    await expect(indicator(page)).toHaveAttribute("aria-expanded", "true");
    await page.keyboard.press("Tab");
    await expect(page.getByTestId("counts-to_sort")).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(panel(page)).toHaveCount(0);
    await expect(indicator(page)).toBeFocused();
    // Its rows act: Done opens Board mode on its column, To sort the
    // Triage panel.
    await indicator(page).click();
    await page.getByTestId("counts-done").click();
    await expect(page.locator("#title-done")).toBeFocused();
    await expect(panel(page)).toHaveCount(0);
    await indicator(page).click();
    await page.getByTestId("counts-to_sort").click();
    await expect(page.getByTestId("triage-panel")).toBeVisible();
  });

  test("it keeps its box whatever the counts, and its panel moves nothing", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board, {}, null, loadRegistry("triage"));
    const fixed = page.locator('header [data-testid^="gauge-"], header button');
    const boxes = () =>
      fixed.evaluateAll((elements) =>
        elements.filter((e) => !e.closest("[data-testid=work-counts-panel]")).map((e) => JSON.stringify(e.getBoundingClientRect())),
      );
    const quiet = await boxes();
    const own = await indicator(page).boundingBox();
    const many = (cards: typeof board.columns.done, n: number) =>
      Array.from({ length: n }, (_, i) => ({ ...cards[i % cards.length]!, key: `${cards[i % cards.length]!.key}-many-${i}`, session_id: null }));
    await page.evaluate((b) => window.__officinaPushBoard?.(b), {
      ...board,
      columns: { ...board.columns, to_resume: many(board.columns.to_resume, 150), done: many(board.columns.done, 42) },
    });
    await expect(page.getByTestId("work-count-to_resume")).toHaveText("99+");
    expect(await indicator(page).boundingBox()).toEqual(own);
    await indicator(page).click();
    await expect(panel(page)).toBeVisible();
    expect(await boxes()).toEqual(quiet);
  });

  test("its counts are neutral, never amber", async ({ page }) => {
    await openBoard(page, loadBoard("triage-board"), {}, null, loadRegistry("triage"));
    const colours = await indicator(page).evaluate((element) => {
      const probe = document.createElement("div");
      probe.style.background = "var(--color-attention)";
      document.body.append(probe);
      const amber = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return { own: getComputedStyle(element).backgroundColor, border: getComputedStyle(element).borderColor, amber };
    });
    expect(colours.amber).not.toBe("");
    expect(colours.own).not.toBe(colours.amber);
    expect(colours.border).not.toBe(colours.amber);
  });
});
