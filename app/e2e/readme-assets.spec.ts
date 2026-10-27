// README screenshot and animated demo (AD-17), written to docs/assets/.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "@playwright/test";
import gifenc from "gifenc";
import pngjs from "pngjs";
import { loadBoard, openBoard } from "./fixtures";

// Both packages are CommonJS: Node exposes them as a default export only.
const { GIFEncoder, applyPalette, quantize } = gifenc;
const { PNG } = pngjs;

const assets = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "docs", "assets");

test.use({ viewport: { width: 1440, height: 820 }, deviceScaleFactor: 1 });

test("screenshot: busy board", async ({ page }) => {
  mkdirSync(assets, { recursive: true });
  await openBoard(page, loadBoard("busy"));
  await page.screenshot({ path: join(assets, "board.png") });
});

test("animated demo: a session needs you, one click, it works again", async ({ page }) => {
  const board = loadBoard("busy");
  await openBoard(page, board);
  const frames: { png: Buffer; delay: number }[] = [];
  const shot = async (delay: number) => frames.push({ png: await page.screenshot(), delay });

  await shot(1800);
  const key = board.columns.needs_you[1]!.key;
  const card = page.locator(`[data-key="${key}"] button.main`);
  await card.hover();
  await shot(800);
  await card.click();
  await shot(1800);

  const answered = loadBoard("busy");
  const moved = answered.columns.needs_you.splice(1, 1)[0]!;
  moved.column = "working";
  moved.waiting_for = null;
  answered.columns.working.unshift(moved);
  answered.waiting_count = answered.columns.needs_you.length;
  await page.evaluate((b) => window.__officinaPushBoard!(b), answered);
  await page.mouse.move(5, 5);
  await page.clock.fastForward(11_000);
  await shot(2600);

  const gif = GIFEncoder();
  for (const frame of frames) {
    const png = PNG.sync.read(frame.png);
    const rgba = new Uint8Array(png.data.buffer, png.data.byteOffset, png.data.length);
    const palette = quantize(rgba, 256);
    gif.writeFrame(applyPalette(rgba, palette), png.width, png.height, { palette, delay: frame.delay });
  }
  gif.finish();
  writeFileSync(join(assets, "demo.gif"), gif.bytes());
});
