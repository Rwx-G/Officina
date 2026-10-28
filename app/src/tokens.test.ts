// The design tokens of app.css, its only committed copy, against what the
// code expects of them, in both themes (AD-67). Values are read from the
// file, never retyped.
/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { parse } from "svelte/compiler";
import { describe, expect, it } from "vitest";
import { PROJECT_COLORS } from "./lib/colors";
import { PAINTS } from "./lib/scene/furniture";
import { TARGET_CSS_MIN } from "./lib/scene/geometry";
import { LOOKS } from "./lib/scene/rig";

// Read from disk, relative to the app folder Vitest runs in: it replaces
// imported CSS, `?raw` included, by nothing, and jsdom's module URLs are not
// file URLs.
const SRC = join(process.cwd(), "src");
const css = readFileSync(join(SRC, "app.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

/** The body of the block `selector { ... }` found in `text`, braces
 *  matched; the empty string when there is none. */
function blockBody(text: string, selector: string): string {
  const head = text.indexOf(`${selector} {`);
  if (head < 0) return "";
  const open = head + selector.length + 1;
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}" && --depth === 0) return text.slice(open + 1, i);
  }
  throw new Error(`unclosed block ${selector}`);
}

/** Every declaration of a block body, in order. */
function declarations(body: string): [string, string][] {
  return [...body.matchAll(/([\w-]+)\s*:\s*([^;{}]+);/g)].map((m) => [m[1]!, m[2]!.trim()]);
}

const customProperties = (body: string) => new Map(declarations(body).filter(([name]) => name.startsWith("--")));

const LIGHT_BODY = blockBody(css, ":root");
const DARK_BODY = blockBody(css, ':root[data-theme="dark"]');
const MEDIA_DARK_BODY = blockBody(blockBody(css, "@media (prefers-color-scheme: dark)"), ':root:not([data-theme="light"])');

type Theme = "light" | "dark";
const THEMES: Theme[] = ["light", "dark"];

/** The custom properties in force in each theme: the light ones, with the
 *  dark block's laid over them. */
function themeTokens(theme: Theme): Map<string, string> {
  const light = customProperties(LIGHT_BODY);
  return theme === "light" ? light : new Map([...light, ...customProperties(DARK_BODY)]);
}

/** A token's value with `var()` aliases followed, as the cascade does on
 *  the root element. */
function resolve(tokens: Map<string, string>, name: string): string {
  const value = tokens.get(name);
  if (value === undefined) throw new Error(`no token ${name}`);
  const alias = /^var\((--[\w-]+)\)$/.exec(value);
  return alias ? resolve(tokens, alias[1]!) : value;
}

type Rgba = [number, number, number, number];

/** A `#rrggbb` or `rgb(r g b / a)` value. */
function parseColour(value: string): Rgba {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex) return [0, 2, 4].map((i) => parseInt(hex[1]!.slice(i, i + 2), 16)).concat(1) as Rgba;
  const rgb = /^rgb\((\d+) (\d+) (\d+)(?: \/ ([\d.]+))?\)$/.exec(value);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), rgb[4] === undefined ? 1 : Number(rgb[4])];
  throw new Error(`not a colour this test reads: ${value}`);
}

/** `top` laid over the opaque `under`. */
function composite(top: Rgba, under: Rgba): Rgba {
  const a = top[3];
  return [0, 1, 2].map((i) => top[i]! * a + under[i]! * (1 - a)).concat(1) as Rgba;
}

/** A pair's side: a token, or a translucent token and the one it lies on. */
type Side = string | [string, string];

function sideColour(tokens: Map<string, string>, side: Side): Rgba {
  if (typeof side === "string") {
    const colour = parseColour(resolve(tokens, side));
    if (colour[3] !== 1) throw new Error(`${side} is translucent: say what it lies on`);
    return colour;
  }
  return composite(parseColour(resolve(tokens, side[0])), sideColour(tokens, side[1]));
}

const sideName = (side: Side) => (typeof side === "string" ? side : `${side[0]} over ${side[1]}`);

/** WCAG 2 relative luminance of an opaque colour. */
function luminance([r, g, b]: Rgba): number {
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG 2 contrast ratio of two opaque colours. */
function contrast(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/** Role pairs and their targets, held in both themes (AD-67, D-117,
 *  D-189): the foreground, the background, the ratio. The values are read
 *  from app.css; each story adds the pairs of the tokens it brings. The
 *  alert colour on amber and amber on the background are decor, without a
 *  target, and stay out. */
const PAIRS: [Side, Side, number][] = [
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
  // Errors and broken contracts (D-189): the danger colour as text and
  // border, text on the danger surface, and text on the column washes as
  // the page shows them.
  ["--color-danger", "--color-bg", 4.5],
  ["--color-danger", "--color-surface", 4.5],
  ["--color-danger", "--color-danger-surface", 4.5],
  ["--color-text", "--color-danger-surface", 7],
  // The "relay stopped" chip's label.
  ["--color-surface", "--color-danger", 4.5],
  ["--color-text", ["--color-column", "--color-bg"], 7],
  ["--color-text", ["--color-attention-wash", "--color-bg"], 7],
  // The figures on what they stand or sit on (the prototype v2's
  // `scenePairs` and build.mjs): tunic and trousers on the room floor, the
  // alert room's floor and each rug, the tunic on the sofas.
  ["--figure-tunic", "--scene-room", 3],
  ["--figure-legs", "--scene-room", 3],
  ["--figure-tunic", "--scene-alert-floor", 3],
  ["--figure-legs", "--scene-alert-floor", 3],
  ["--figure-tunic", "--scene-rug", 3],
  ["--figure-legs", "--scene-rug", 3],
  ["--figure-tunic", "--scene-rug-2", 3],
  ["--figure-legs", "--scene-rug-2", 3],
  ["--figure-tunic", "--scene-sofa-t", 3],
];

/** Hair against the rim the head is outlined with, on each floor a figure
 *  stands on in PAIRS: the rim keeps the ink hair readable on dark floors
 *  (AD-67, D-478). */
const RIM_TARGET = 3;

/** The tokens of a family `--name-0`, `--name-1`, ... in index order. */
function family(tokens: Map<string, string>, name: string): string[] {
  const out: string[] = [];
  for (let i = 0; tokens.has(`--${name}-${i}`); i++) out.push(`--${name}-${i}`);
  return out;
}

/** The people's families: the same in both themes (AD-67). */
const PEOPLE = ["skin", "hair"];
const isPeople = (name: string) => PEOPLE.some((f) => new RegExp(`^--${f}-\\d+$`).test(name));
const isColourLiteral = (value: string) => /^(#|rgb\()/i.test(value);

describe("tokens", () => {
  it("every role pair meets its contrast target in both themes", () => {
    for (const theme of THEMES) {
      const tokens = themeTokens(theme);
      for (const [fg, bg, target] of PAIRS) {
        const ratio = contrast(sideColour(tokens, fg), sideColour(tokens, bg));
        expect(ratio, `${theme}: ${sideName(fg)} on ${sideName(bg)}`).toBeGreaterThanOrEqual(target);
      }
      const floors = [...new Set(PAIRS.filter(([fg]) => fg === "--figure-tunic").map(([, bg]) => bg as string))];
      expect(floors.length).toBeGreaterThan(0);
      for (const hair of family(tokens, "hair")) {
        for (const floor of floors) {
          const rim = sideColour(tokens, ["--figure-rim", floor]);
          expect(contrast(sideColour(tokens, hair), rim), `${theme}: ${hair} on the rim over ${floor}`).toBeGreaterThanOrEqual(
            RIM_TARGET,
          );
        }
      }
    }
  });

  it("the dark values are the same under the theme attribute and under the media query", () => {
    expect(declarations(DARK_BODY).length).toBeGreaterThan(0);
    expect(declarations(MEDIA_DARK_BODY)).toEqual(declarations(DARK_BODY));
  });

  it("the people tokens have no dark override", () => {
    const light = [...customProperties(LIGHT_BODY).keys()];
    for (const name of PEOPLE) expect(family(customProperties(LIGHT_BODY), name).length, name).toBeGreaterThan(0);
    expect(light.filter(isPeople).length).toBeGreaterThan(0);
    expect([...customProperties(DARK_BODY).keys()].filter(isPeople)).toEqual([]);
  });

  it("every colour token has a dark value, and every dark value a light token", () => {
    const light = customProperties(LIGHT_BODY);
    const dark = customProperties(DARK_BODY);
    // An alias follows its target; the people are the same in both themes.
    const themed = [...light].filter(([name, value]) => isColourLiteral(value) && !isPeople(name)).map(([name]) => name);
    expect(themed.filter((name) => !dark.has(name))).toEqual([]);
    expect([...dark.keys()].filter((name) => !light.has(name))).toEqual([]);
    for (const [name, value] of dark) expect(isColourLiteral(value), `${name}: ${value}`).toBe(true);
  });

  it("every project colour meets 3:1 on the surface in both themes", () => {
    for (const theme of THEMES) {
      const tokens = themeTokens(theme);
      for (const [colour, value] of Object.entries(PROJECT_COLORS)) {
        const name = /^var\((--[\w-]+)\)$/.exec(value)![1]!;
        expect(contrast(sideColour(tokens, name), sideColour(tokens, "--color-surface")), `${theme}: ${colour}`).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it("the allowed looks are the skin and hair pairs at 3:1 or more", () => {
    // Theme-independent tokens: the light block holds the only copy.
    const tokens = themeTokens("light");
    const skins = family(tokens, "skin");
    const hairs = family(tokens, "hair");
    expect(skins.length).toBeGreaterThan(0);
    expect(hairs.length).toBeGreaterThan(0);
    const pairs: [number, number][] = [];
    skins.forEach((skin, s) =>
      hairs.forEach((hair, h) => contrast(sideColour(tokens, skin), sideColour(tokens, hair)) >= 3 && pairs.push([s, h])),
    );
    expect(LOOKS).toEqual(pairs);
    // Every skin keeps at least one hair colour, so no skin is left out.
    for (let s = 0; s < skins.length; s++) expect(pairs.some(([skin]) => skin === s), `skin ${s}`).toBe(true);
  });

  it("every paint the scene draws with is a token of app.css", () => {
    const tokens = themeTokens("light");
    for (const name of PAINTS) expect(tokens.has(`--${name}`), name).toBe(true);
  });

  it("every project colour has a token", () => {
    const tokens = themeTokens("light");
    const colours = Object.entries(PROJECT_COLORS);
    expect(colours.length).toBeGreaterThan(0);
    for (const [colour, value] of colours) {
      const name = /^var\((--[\w-]+)\)$/.exec(value)?.[1];
      expect(name, colour).toBeDefined();
      expect(tokens.get(name!), `${colour}: ${name}`).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it("TARGET_CSS_MIN equals app.css's --target-min", () => {
    expect(themeTokens("light").get("--target-min")).toBe(`${TARGET_CSS_MIN}px`);
  });
});

// ------------------------------------------------------------ colour literals

/** Keywords a colour property takes that are not a colour of their own
 *  (AD-67's exceptions, and the CSS-wide keywords). */
const NOT_A_COLOUR = new Set(["transparent", "currentcolor", "inherit", "none", "initial", "unset", "revert", "revert-layer"]);

const COLOUR_NOTATION = /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(/i;

// jsdom's CSS parser decides what a named colour is: a word it accepts as a
// `color` value. No list of names is kept here.
const probe = document.createElement("span").style;
function isNamedColour(word: string): boolean {
  if (NOT_A_COLOUR.has(word.toLowerCase())) return false;
  probe.color = "";
  probe.color = word;
  return probe.color !== "";
}

/** A CSS declaration's value holds a colour literal: a notation, or any of
 *  its words that is a named colour (strings left out). */
function cssValueHasColour(value: string): boolean {
  const bare = value.replace(/(["'])(?:\\.|(?!\1).)*\1/g, "");
  if (COLOUR_NOTATION.test(bare)) return true;
  return [...bare.matchAll(/(?<![\w-])-?[a-z_][\w-]*/gi)].some((m) => isNamedColour(m[0]));
}

/** An attribute's text is a colour literal: a notation, or a named colour
 *  on its own. */
function attributeIsColour(text: string): boolean {
  return COLOUR_NOTATION.test(text) || isNamedColour(text.trim());
}

/** A script string holds a colour notation. Named colours are left out:
 *  CSS keeps deprecated system colours whose names are ordinary words
 *  (`Background`, `Menu`), which script strings use for other things. */
const scriptStringIsColour = (text: string) => COLOUR_NOTATION.test(text);

/** Every colour literal of a component, with where it is. */
function colourLiterals(source: string): string[] {
  const ast = parse(source, { modern: true });
  const found: string[] = [];
  const seen = new WeakSet<object>();
  const visit = (node: unknown): void => {
    if (typeof node !== "object" || node === null || seen.has(node)) return;
    seen.add(node);
    const n = node as Record<string, unknown>;
    if (n.type === "Declaration" && typeof n.value === "string" && cssValueHasColour(n.value)) {
      found.push(`${String(n.property)}: ${n.value}`);
    }
    if ((n.type === "Attribute" || n.type === "StyleDirective") && Array.isArray(n.value)) {
      for (const part of n.value as { type: string; data?: string }[]) {
        if (part.type === "Text" && part.data && attributeIsColour(part.data)) found.push(`${String(n.name)}="${part.data}"`);
      }
    }
    if (n.type === "Literal" && typeof n.value === "string" && scriptStringIsColour(n.value)) found.push(`"${n.value}"`);
    if (n.type === "TemplateElement") {
      const cooked = (n.value as { cooked?: string }).cooked;
      if (cooked && scriptStringIsColour(cooked)) found.push(`\`${cooked}\``);
    }
    for (const value of Object.values(n)) visit(value);
  };
  visit(ast);
  return found;
}

describe("colour literals", () => {
  it("the detector finds every kind of colour literal and nothing else", () => {
    const style = (body: string) => `<div class="a"></div><style>.a { ${body} }</style>`;
    for (const literal of ["#abc", "#a1b2c3", "#a1b2c3d4", "rgb(1 2 3)", "rgba(1, 2, 3, 0.5)", "hsl(1 2% 3%)", "hsla(1, 2%, 3%, 1)", "color(srgb 1 0 0)", "red", "RebeccaPurple"]) {
      expect(colourLiterals(style(`border: 1px solid ${literal};`)), literal).toHaveLength(1);
    }
    expect(colourLiterals(`<rect fill="tomato" />`)).toHaveLength(1);
    expect(colourLiterals(`<div style:color="#fff"></div>`)).toHaveLength(1);
    expect(colourLiterals(`<script>const c = "#ffffff";</script>`)).toHaveLength(1);
    const clean = style(
      "color: var(--color-text); background: transparent; border: 1px solid currentColor; fill: none; outline: inherit; white-space: pre-wrap; font-family: \"Red Hat\", sans-serif;",
    );
    expect(colourLiterals(clean)).toEqual([]);
  });

  it("no component hard-codes a colour", () => {
    const components = readdirSync(SRC, { recursive: true, encoding: "utf8" }).filter((file) => file.endsWith(".svelte"));
    expect(components.length).toBeGreaterThan(0);
    const offenders = components.flatMap((file) =>
      colourLiterals(readFileSync(join(SRC, file), "utf8")).map((literal) => `${file}: ${literal}`),
    );
    expect(offenders).toEqual([]);
  });
});
