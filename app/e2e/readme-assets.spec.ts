// README screenshots and animated demos (AD-17, AD-72), written to
// docs/assets/: the board, and the scene in both themes and walking.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "@playwright/test";
import gifenc from "gifenc";
import pngjs from "pngjs";
import { loadBoard, openBoard, openScene, openWalkingScene } from "./fixtures";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";

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

/** `board` with `key` moved by `edit` into the column it names. */
function moved(board: Board, key: string, edit: (card: Card) => Card): Board {
  const next = structuredClone(board);
  let card: Card | null = null;
  for (const list of Object.values(next.columns)) {
    const at = list.findIndex((c) => c.key === key);
    if (at >= 0) card = list.splice(at, 1)[0]!;
  }
  const after = edit(card!);
  next.columns[after.column].push(after);
  next.waiting_count = next.columns.needs_you.length;
  return next;
}

const working = (c: Card): Card => ({ ...c, column: "working", waiting_for: null, scene: { ...c.scene!, zone: "office", alert: null, approval: false } });

for (const theme of ["light", "dark"] as const) {
  test(`screenshot: the scene, ${theme}`, async ({ page }) => {
    mkdirSync(assets, { recursive: true });
    await page.emulateMedia({ colorScheme: theme });
    await openScene(page, loadBoard("scene-c"));
    await page.screenshot({ path: join(assets, theme === "light" ? "scene.png" : "scene-dark.png") });
  });
}

test("animated scene: a first prompt, a wait, its file, and back to work", async ({ page }) => {
  const board = loadBoard("scene-c");
  await openWalkingScene(page, board);
  await page.clock.pauseAt(board.generated_at_ms + 60_000);
  const frames: { png: Buffer; delay: number }[] = [];
  const shot = async (delay: number) => frames.push({ png: await page.screenshot(), delay });
  /** Runs the walks a frame at a time, a picture every `every` ms, until
   *  nothing moves. */
  const walk = async (every: number) => {
    for (let ms = 0; ms < 60_000; ms += every) {
      await page.clock.runFor(every);
      await shot(every);
      if ((await page.getByTestId("scene").getAttribute("data-moving")) === "0") return;
    }
  };
  const push = (b: Board) => page.evaluate((next) => window.__officinaPushBoard!(next), b);

  await shot(1500);
  // A session at reception gets its first prompt and walks to its desk.
  const arriving = board.columns.your_turn.find((c) => c.scene?.zone === "reception")!;
  let now = moved(board, arriving.key, working);
  await push(now);
  await walk(400);
  // A working session waits on a permission prompt: it walks to the
  // decision office, where its halo breathes.
  const waiting = now.columns.working.find((c) => c.key !== arriving.key)!;
  now = moved(now, waiting.key, (c) => ({
    ...c,
    column: "needs_you",
    waiting_for: "permission prompt",
    scene: { ...c.scene!, zone: "decision", waiting_since_ms: board.generated_at_ms + 60_000, alert: null, approval: false },
  }));
  await push(now);
  await walk(400);
  await shot(1200);
  // The author opens its file and brings it to front.
  await page.locator(`[data-layer="targets"] [data-key=${JSON.stringify(waiting.key)}]`).click();
  await shot(1500);
  await page.getByTestId("bring-to-front").click();
  await shot(1500);
  // Answered, it walks back to its desk.
  await page.keyboard.press("Escape");
  await push(moved(now, waiting.key, working));
  await walk(400);
  await shot(2000);

  writeFileSync(join(assets, "scene.gif"), changesOnlyGif(frames));
});

/** The frames as a GIF with the first frame's palette for all, each later
 *  frame carrying only the pixels that changed and the rest transparent over
 *  the frame before: a scene changes little from one frame to the next, so
 *  the file stays small. */
function changesOnlyGif(frames: { png: Buffer; delay: number }[]): Uint8Array {
  const gif = GIFEncoder();
  let colours: number[][] | null = null;
  let shown: Uint8Array | null = null;
  for (const frame of frames) {
    const png = PNG.sync.read(frame.png);
    const rgba = new Uint8Array(png.data.buffer, png.data.byteOffset, png.data.length);
    // One slot kept free for the transparent index.
    colours ??= quantize(rgba, 255);
    const index = applyPalette(rgba, colours);
    const transparentIndex = colours.length;
    if (!shown) {
      shown = index.slice();
      gif.writeFrame(index, png.width, png.height, { palette: [...colours, [0, 0, 0]], delay: frame.delay, dispose: 1 });
      continue;
    }
    const changes = new Uint8Array(index.length).fill(transparentIndex);
    for (let i = 0; i < index.length; i++) {
      if (index[i] !== shown[i]) changes[i] = shown[i] = index[i]!;
    }
    gif.writeFrame(changes, png.width, png.height, { delay: frame.delay, transparent: true, transparentIndex, dispose: 1 });
  }
  gif.finish();
  return gif.bytes();
}
