// The design tokens of app.css, its only committed copy, against what the
// code expects of them. Values are read from the file, never retyped.
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { PROJECT_COLORS } from "./lib/colors";
import { PAINTS } from "./lib/scene/furniture";
import { TARGET_CSS_MIN } from "./lib/scene/geometry";
import { LOOKS } from "./lib/scene/rig";

// Read from disk, relative to the app folder Vitest runs in: it replaces
// imported CSS, `?raw` included, by nothing, and jsdom's module URLs are not
// file URLs.
const css = readFileSync(join(process.cwd(), "src", "app.css"), "utf8");

/** The custom properties declared in the first `:root { ... }` block. */
function rootTokens(text: string): Map<string, string> {
  const start = text.indexOf(":root {");
  const block = text.slice(start, text.indexOf("\n}", start));
  const tokens = new Map<string, string>();
  for (const match of block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) tokens.set(match[1]!, match[2]!.trim());
  return tokens;
}

/** WCAG 2 relative luminance of a `#rrggbb` colour. */
function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(1 + 2 * i, 3 + 2 * i), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
}

/** WCAG 2 contrast ratio of two `#rrggbb` colours. */
function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/** Role pairs of the light theme and their targets (AD-67, D-117): the
 *  foreground token, the background token, the ratio. The values are read
 *  from app.css; each story adds the pairs of the tokens it brings. */
const LIGHT_PAIRS: [string, string, number][] = [
  // Text on the alert badge and pill (prototype v2's build.mjs).
  ["--color-on-alert", "--color-alert", 4.5],
  // The alert colour as a border, a stripe and a tag on the background.
  ["--color-alert", "--color-bg", 3],
  // The window's roles the scene draws with (the prototype's `pairs`).
  ["--color-text", "--color-bg", 7],
  ["--color-text-muted", "--color-bg", 4.5],
  ["--color-text", "--color-surface", 7],
  ["--color-text-muted", "--color-surface", 4.5],
  ["--color-attention-text", "--color-attention", 7],
  ["--color-attention-border", "--color-bg", 3],
  ["--color-working", "--color-bg", 3],
  ["--color-working", "--color-surface", 3],
  // Plaque text: the page's background on an ink pill.
  ["--color-bg", "--color-text", 7],
  // The figures on what they stand or sit on (the prototype v2's
  // `scenePairs` and build.mjs): tunic and trousers on the room floors,
  // the rugs, the sofas and the alert room's floor.
  ["--figure-tunic", "--scene-room", 3],
  ["--figure-legs", "--scene-room", 3],
  ["--figure-legs", "--scene-rug-2", 3],
  ["--figure-tunic", "--scene-alert-floor", 3],
  ["--figure-legs", "--scene-alert-floor", 3],
  ["--figure-tunic", "--scene-sofa-t", 3],
  ["--figure-tunic", "--scene-rug", 3],
];

/** The tokens of a family `--name-0`, `--name-1`, ... in index order. */
function family(tokens: Map<string, string>, name: string): string[] {
  const out: string[] = [];
  for (let i = 0; tokens.has(`--${name}-${i}`); i++) out.push(tokens.get(`--${name}-${i}`)!);
  return out;
}

describe("tokens", () => {
  it("every role pair meets its contrast target in the light theme", () => {
    const tokens = rootTokens(css);
    for (const [fg, bg, target] of LIGHT_PAIRS) {
      const [a, b] = [tokens.get(fg), tokens.get(bg)];
      expect(a, fg).toMatch(/^#[0-9a-f]{6}$/i);
      expect(b, bg).toMatch(/^#[0-9a-f]{6}$/i);
      expect(contrast(a!, b!), `${fg} on ${bg}`).toBeGreaterThanOrEqual(target);
    }
  });

  it("every project colour meets 3:1 on the surface", () => {
    const tokens = rootTokens(css);
    for (const [colour, value] of Object.entries(PROJECT_COLORS)) {
      const name = /^var\((--[\w-]+)\)$/.exec(value)![1]!;
      expect(contrast(tokens.get(name)!, tokens.get("--color-surface")!), colour).toBeGreaterThanOrEqual(3);
    }
  });

  it("the allowed looks are the skin and hair pairs at 3:1 or more", () => {
    const tokens = rootTokens(css);
    const skins = family(tokens, "skin");
    const hairs = family(tokens, "hair");
    expect(skins.length).toBeGreaterThan(0);
    expect(hairs.length).toBeGreaterThan(0);
    const pairs: [number, number][] = [];
    skins.forEach((skin, s) => hairs.forEach((hair, h) => contrast(skin, hair) >= 3 && pairs.push([s, h])));
    expect(LOOKS).toEqual(pairs);
    // Every skin keeps at least one hair colour, so no skin is left out.
    for (let s = 0; s < skins.length; s++) expect(pairs.some(([skin]) => skin === s), `skin ${s}`).toBe(true);
  });

  it("every paint the scene draws with is a token of app.css", () => {
    const tokens = rootTokens(css);
    for (const name of PAINTS) expect(tokens.has(`--${name}`), name).toBe(true);
  });

  it("every project colour has a token", () => {
    const tokens = rootTokens(css);
    const colours = Object.entries(PROJECT_COLORS);
    expect(colours.length).toBeGreaterThan(0);
    for (const [colour, value] of colours) {
      const name = /^var\((--[\w-]+)\)$/.exec(value)?.[1];
      expect(name, colour).toBeDefined();
      expect(tokens.get(name!), `${colour}: ${name}`).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it("TARGET_CSS_MIN equals app.css's --target-min", () => {
    expect(rootTokens(css).get("--target-min")).toBe(`${TARGET_CSS_MIN}px`);
  });
});
