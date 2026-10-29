// Story 4.17: what the owner's two-second test reads in Scene mode (story
// 4.14 AC 9). No queue badge is ever covered (D-554).
// Boards are `officina board --json` output (AD-17); who waits comes from
// `layoutScene`, imported, never retyped.
import { expect, test, type Page } from "@playwright/test";
import { openScene } from "./fixtures";
import { fixturesOf } from "./fixture-index";
import { layoutScene } from "../src/lib/scene/layout";
import { emptyMemory } from "../src/lib/scene/memory";
import type { Board } from "../src/lib/generated/Board";

/** The owner's screen with the window maximised (D-520) and a wide
 *  screen (D-543). */
const VIEWPORTS = [
  { width: 1920, height: 1032 },
  { width: 3440, height: 1392 },
];

/** Every fixture board with someone waiting in the scene. */
const WAITING = fixturesOf<Board>("board").filter(({ data }) =>
  layoutScene({ board: data }, emptyMemory(data.floor)).employees.some((e) => e.zone === "decision" || e.zone === "alert"),
);

/** Lets what the scene's layers paint take the pointer, and hides the
 *  targets, so `elementFromPoint` returns what is painted on top: an SVG
 *  shape by its painted area, an HTML box only when it paints a
 *  background, never a layer's own box. Through the CSSOM: the page's CSP
 *  refuses an injected style sheet. For these hit tests only. */
async function hitTestPaint(page: Page): Promise<void> {
  await page.getByTestId("scene").evaluate((root) => {
    for (const layer of Array.from(root.querySelectorAll<HTMLElement>("[data-layer]"))) {
      if (layer.dataset.layer === "targets") {
        layer.style.setProperty("visibility", "hidden");
        continue;
      }
      for (const el of Array.from(layer.querySelectorAll<HTMLElement | SVGElement>("*"))) {
        if (el instanceof SVGSVGElement || el instanceof SVGForeignObjectElement) continue;
        const paints = el instanceof SVGElement || !/rgba\(.*, 0\)|transparent/.test(getComputedStyle(el).backgroundColor);
        if (paints) el.style.setProperty("pointer-events", el instanceof SVGElement ? "visiblePainted" : "auto");
      }
    }
  });
}

/** Points inside a badge's disc or diamond, as fractions of its box from
 *  its centre: both shapes hold every point whose |dx| + |dy| is 0.5 or
 *  less. */
const INSIDE: [number, number][] = [
  [0, 0],
  [0.3, 0],
  [-0.3, 0],
  [0, 0.3],
  [0, -0.3],
  [0.15, 0.15],
  [-0.15, 0.15],
  [0.15, -0.15],
  [-0.15, -0.15],
];

test("WAITING holds the scene fixtures with someone waiting", () => {
  expect(WAITING.length).toBeGreaterThan(1);
});

for (const { name, data: board } of WAITING) {
  test(`no queue badge is covered: ${name}`, async ({ page }) => {
    for (const viewport of VIEWPORTS) {
      await page.setViewportSize(viewport);
      await openScene(page, board);
      await hitTestPaint(page);
      const covered = await page.getByTestId("scene").evaluate((root, inside) => {
        const out: string[] = [];
        const badges = Array.from(root.querySelectorAll('[data-layer="badges"] [data-mark="ticket"]'));
        for (const badge of badges) {
          const r = badge.getBoundingClientRect();
          for (const [fx, fy] of inside) {
            const x = r.left + r.width * (0.5 + fx);
            const y = r.top + r.height * (0.5 + fy);
            const hit = document.elementFromPoint(x, y);
            if (!hit || !badge.contains(hit)) {
              const owner = badge.closest("[data-ticket-of], [data-away-ticket]");
              const what = owner?.getAttribute("data-ticket-of") ?? `desk ${owner?.getAttribute("data-away-ticket")}`;
              out.push(`${what} at (${Math.round(x)}, ${Math.round(y)}) under ${hit?.closest("[data-layer], [data-testid]")?.outerHTML.slice(0, 80) ?? "nothing"}`);
            }
          }
        }
        return { badges: badges.length, covered: out };
      }, INSIDE);
      expect(covered.badges, `${name} at ${viewport.width}`).toBeGreaterThan(0);
      expect(covered.covered, `${name} at ${viewport.width}`).toEqual([]);
    }
  });
}
