// Renders an SVG of docs/assets to PNG with the Playwright Chromium already
// used by the e2e suite, so no image toolchain is needed. Usage:
//   node scripts/render-logo.mjs <source.svg> <size> <output.png>
import { readFile } from "node:fs/promises";
import { chromium } from "@playwright/test";

const [source, size, output] = [process.argv[2], Number(process.argv[3]), process.argv[4]];
if (!source || !size || !output) {
  console.error("usage: render-logo.mjs <source.svg> <size> <output.png>");
  process.exit(64);
}
const svg = await readFile(source, "utf8");
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: size, height: size } });
await page.setContent(
  `<html><body style="margin:0;background:transparent">${svg.replace(
    /width="512" height="512"/,
    `width="${size}" height="${size}"`,
  )}</body></html>`,
);
await page.locator("svg").screenshot({ path: output, omitBackground: true });
await browser.close();
