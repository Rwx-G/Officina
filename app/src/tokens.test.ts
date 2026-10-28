// The design tokens of app.css, its only committed copy, against what the
// code expects of them. Values are read from the file, never retyped.
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { PROJECT_COLORS } from "./lib/colors";

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

describe("tokens", () => {
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
});
