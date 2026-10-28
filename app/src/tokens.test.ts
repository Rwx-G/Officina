// The design tokens of app.css, its only committed copy, against what the
// code expects of them. Values are read from the file, never retyped.
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { PROJECT_COLORS } from "./lib/colors";
import { TARGET_CSS_MIN } from "./lib/scene/geometry";

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
];

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
