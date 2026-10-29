// The session file's polish (story 4.19): the owner found the panel cut off
// abruptly against the header. The panel is a surface standing in the
// sessions area, a gap under the header and its right edge on the header's
// last control, its shadow whole (D-702).
import { expect, test, type Page } from "@playwright/test";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";
import { loadBoard, openScene, transitionsDone } from "./fixtures";

/** The owner's screens (D-543): the file lies over the scene on a
 *  maximised Full HD window and docks beside it on the ultrawide. */
const SCREENS = [
  { width: 1920, height: 1032, docked: false },
  { width: 3440, height: 1392, docked: true },
] as const;
const THEMES = ["light", "dark"] as const;

const cards = (board: Board): Card[] => Object.values(board.columns).flat();
const file = (page: Page) => page.getByTestId("session-file");

async function openFile(page: Page, card: Card): Promise<void> {
  await page.getByTestId("scene").locator(`[data-layer="targets"] [data-key=${JSON.stringify(card.key)}]`).click();
  await expect(file(page)).toBeVisible();
  // The slide-in starts on the page's next frame, which the installed
  // clock drives.
  await page.clock.runFor(1_000);
  await transitionsDone(page);
}

/** How far a computed `box-shadow` reaches past each edge of its box. */
function shadowReach(shadow: string): { top: number; right: number; bottom: number; left: number } {
  const lengths = [...shadow.replace(/rgba?\([^)]*\)/, "").matchAll(/(-?[\d.]+)px/g)].map((m) => Number(m[1]));
  const [x = 0, y = 0, blur = 0, spread = 0] = lengths;
  const reach = blur + spread;
  return { top: Math.max(0, reach - y), right: Math.max(0, reach + x), bottom: Math.max(0, reach + y), left: Math.max(0, reach - x) };
}

/** A card of scene-c by its session name. */
function named(board: Board, name: string): Card {
  const card = cards(board).find((c) => c.name === name);
  if (!card) throw new Error(`no card ${name}`);
  return card;
}

/** A scene-c session in the lounge, its file opened over the scene. */
const MARKUP_SESSION = "link-fixes";

for (const { width, height, docked } of SCREENS) {
  for (const theme of THEMES) {
    test(`the session file stands under the header with whole edges, ${docked ? "docked" : "over the scene"}, ${theme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: theme });
      await page.setViewportSize({ width, height });
      const board = loadBoard("scene-c");
      await openScene(page, board);
      await openFile(page, named(board, MARKUP_SESSION));

      const geometry = await page.evaluate(() => {
        const box = (el: Element) => el.getBoundingClientRect();
        const header = document.querySelector("header.bar")!;
        const controls = Array.from(header.querySelectorAll("button, [role=status], .gauges > *")).map(box).filter((r) => r.width > 0);
        const panel = document.querySelector("[data-testid=session-file]")!;
        const style = getComputedStyle(panel);
        const root = getComputedStyle(document.documentElement);
        return {
          header: box(header),
          controlsRight: Math.max(...controls.map((r) => r.right)),
          headerContentBottom: box(header).bottom - parseFloat(getComputedStyle(header).paddingBottom),
          area: box(document.querySelector("[data-testid=sessions-area]")!),
          scene: box(document.querySelector("[data-testid=scene]")!),
          panel: box(panel),
          register: box(document.querySelector("[data-testid=register]")!),
          shadow: style.boxShadow,
          borders: [style.borderTopWidth, style.borderRightWidth, style.borderBottomWidth, style.borderLeftWidth].map(parseFloat),
          borderColours: [style.borderTopColor, style.borderRightColor, style.borderBottomColor, style.borderLeftColor],
          radii: [style.borderTopLeftRadius, style.borderTopRightRadius, style.borderBottomRightRadius, style.borderBottomLeftRadius].map(parseFloat),
          radiusSurface: parseFloat(root.getPropertyValue("--radius-surface")),
        };
      });
      const { header, area, panel } = geometry;
      const reach = shadowReach(geometry.shadow);

      // Under the header, never over it, and a deliberate gap: the panel's
      // shadow lies whole inside the sessions area, which clips it.
      expect(panel.top).toBeGreaterThanOrEqual(header.bottom);
      expect(panel.top - area.top).toBeGreaterThanOrEqual(reach.top);
      expect(panel.top - area.top).toBeGreaterThan(0);
      expect(area.right - panel.right).toBeGreaterThanOrEqual(reach.right);
      expect(area.bottom - panel.bottom).toBeGreaterThanOrEqual(reach.bottom);
      // Aligned on the header: its right edge on the last control's, and as
      // far below the header's content as it stands from the area's right
      // edge.
      expect(Math.abs(panel.right - geometry.controlsRight)).toBeLessThanOrEqual(1);
      expect(Math.abs(panel.top - geometry.headerContentBottom - (area.right - panel.right))).toBeLessThanOrEqual(1);
      // Four drawn edges, rounded as every surface is (D-539).
      for (const width of geometry.borders) expect(width).toBeGreaterThanOrEqual(1);
      expect(new Set(geometry.borderColours).size).toBe(1);
      for (const radius of geometry.radii) expect(radius).toBe(geometry.radiusSurface);
      // Its placement (D-543) holds with the insets. Over the scene it
      // covers the register whole (D-555), no strip of it left showing
      // above (D-704); docked, the register stays in view.
      expect(geometry.scene.right <= panel.left + 0.5).toBe(docked);
      const { register } = geometry;
      const covered = register.top >= panel.top && register.bottom <= panel.bottom && register.left >= panel.left && register.right <= panel.right;
      expect(covered).toBe(!docked);
    });
  }
}
