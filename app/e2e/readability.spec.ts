// Story 4.17: what the owner's two-second test reads in Scene mode (story
// 4.14 AC 9). No queue badge is ever covered (D-554); the register is
// overlay UI at a fixed size that covers nothing of the scene, its heading
// counting everyone waiting (D-555, D-556); at eight projects on the
// owner's screen it reads as large as anywhere else.
// Boards are `officina board --json` output (AD-17); what each register
// shows comes from `layoutScene`, imported, never retyped.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { openScene } from "./fixtures";
import { fixturesOf } from "./fixture-index";
import { REGISTER, REGISTER_MAX_H } from "../src/lib/scene/geometry";
import { layoutScene } from "../src/lib/scene/layout";
import { emptyMemory } from "../src/lib/scene/memory";
import { registerHead, registerMore } from "../src/lib/scene/text";
import type { Board } from "../src/lib/generated/Board";

const config = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../src-tauri/tauri.conf.json"), "utf8")) as {
  app: { windows: { label: string; minWidth: number; minHeight: number }[] };
};
const main = config.app.windows.find((w) => w.label === "main")!;

/** The owner's screen with the window maximised (D-520), the window's
 *  minimum (D-546), and a wide screen (D-543). */
const VIEWPORTS = [
  { width: 1920, height: 1032 },
  { width: main.minWidth, height: main.minHeight },
  { width: 3440, height: 1392 },
];

/** The smallest text the register may show, in CSS px. */
const MIN_FONT_PX = 14;

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
    for (const viewport of [VIEWPORTS[0]!, VIEWPORTS[2]!]) {
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

  test(`the register counts everyone waiting at a fixed size and covers nothing: ${name}`, async ({ page }) => {
    const layout = layoutScene({ board }, emptyMemory(board.floor));
    const waiting = layout.employees.filter((e) => e.zone === "decision" || e.zone === "alert");
    for (const viewport of VIEWPORTS) {
      await page.setViewportSize(viewport);
      await openScene(page, board);
      const at = `${name} at ${viewport.width} x ${viewport.height}`;
      const register = page.getByTestId("register");
      await expect(page.getByTestId("register-count"), at).toHaveText(registerHead(waiting.length));
      const rows = await register.locator("[data-register-row]").evaluateAll((all) => all.map((r) => r.getAttribute("data-register-row")));
      expect(rows, at).toEqual(waiting.slice(0, REGISTER.rows).map((e) => e.key));
      if (waiting.length > REGISTER.rows) await expect(register, at).toContainText(registerMore(waiting.length - REGISTER.rows));
      for (const e of waiting.slice(0, REGISTER.rows)) {
        const approval = register.locator(`[data-register-row=${JSON.stringify(e.key)}] [data-approval]`);
        await expect(approval, `${at}: ${e.key}`).toHaveCount(e.marks.ticket === null && e.marks.approval ? 1 : 0);
      }

      // Its size is CSS px, whatever the floor's scale.
      const box = (await register.boundingBox())!;
      expect(Math.abs(box.width - REGISTER.w), at).toBeLessThan(0.5);
      expect(box.height, at).toBeLessThanOrEqual(REGISTER_MAX_H + 0.5);
      const sizes = await register.evaluate((r) => Array.from(r.querySelectorAll(".count, .name"), (el) => parseFloat(getComputedStyle(el).fontSize)));
      for (const size of sizes) expect(size, at).toBeGreaterThanOrEqual(MIN_FONT_PX);

      // Nothing drawn under it, no target and no plaque it could hide.
      const scene = page.getByTestId("scene");
      const targets = await scene.locator("[data-target]").evaluateAll((all) => all.map((t) => ({ id: t.getAttribute("data-target")!, r: t.getBoundingClientRect().toJSON() as DOMRect })));
      const meets = (r: DOMRect) => r.left < box.x + box.width && box.x < r.right && r.top < box.y + box.height && box.y < r.bottom;
      expect(targets.filter((t) => meets(t.r)).map((t) => t.id), at).toEqual([]);
      await hitTestPaint(page);
      const under = await register.evaluate((r) => {
        const b = r.getBoundingClientRect();
        const out: string[] = [];
        for (let fy = 0; fy <= 1; fy += 0.05) {
          for (let fx = 0; fx <= 1; fx += 0.05) {
            const x = b.left + 1 + (b.width - 2) * fx;
            const y = b.top + 1 + (b.height - 2) * fy;
            const hit = document.elementFromPoint(x, y);
            const layer = hit?.closest("[data-layer]")?.getAttribute("data-layer");
            if (layer) out.push(`${layer} at (${Math.round(x)}, ${Math.round(y)})`);
          }
        }
        return out;
      });
      expect(under, at).toEqual([]);
    }
  });
}

test("at eight projects on the owner's screen the register reads as large as with four", async ({ page }) => {
  await page.setViewportSize(VIEWPORTS[0]!);
  const read = async (fixture: string) => {
    const board = fixturesOf<Board>("board").find((f) => f.name === fixture)!.data;
    await openScene(page, board);
    return {
      scale: Number(await page.getByTestId("scene").getAttribute("data-scale")),
      text: await page.getByTestId("register").evaluate((r) =>
        Array.from(r.querySelectorAll(".count, .name"), (el) => ({ size: parseFloat(getComputedStyle(el).fontSize), height: el.getBoundingClientRect().height })),
      ),
    };
  };
  const eight = await read("scene-eight");
  const four = await read("scene-c");
  // The floor is drawn smaller at eight projects; the register is not.
  expect(eight.scale).toBeLessThan(four.scale);
  for (const { size } of eight.text) expect(size).toBeGreaterThanOrEqual(MIN_FONT_PX);
  expect(eight.text[0]).toEqual(four.text[0]);
  expect(eight.text[1]).toEqual(four.text[1]);
});
