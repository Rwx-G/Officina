// The lint gate is narrow on purpose: it blocks the XSS and code-injection
// constructs that the type checker cannot see. Session prompts and assistant
// messages are untrusted text, so `{@html}` is an error, not a warning.
import { defineConfig, globalIgnores } from "eslint/config";
import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import svelteParser from "svelte-eslint-parser";
import tsParser from "@typescript-eslint/parser";
import globals from "globals";

const TIMER = "The scene has no timer: it changes on a changed payload only (AD-64).";
const LOOP = "The scene's only frame loop is the mover's (D-470).";
const CLOCK = "The scene never reads the clock (AD-64).";

/** What the scene's modules may not use. */
const SCENE_GLOBALS = [
  { name: "setInterval", message: TIMER },
  { name: "setTimeout", message: TIMER },
  { name: "requestAnimationFrame", message: LOOP },
  { name: "cancelAnimationFrame", message: LOOP },
];
const SCENE_PROPERTIES = [
  { object: "Date", property: "now", message: CLOCK },
  { object: "performance", property: "now", message: CLOCK },
  { object: "Math", property: "random", message: "The scene is drawn from seeds, never at random (AD-60, AD-173)." },
];

/** The mover's module, and what it alone may use of the lists above. */
const MOVER = "src/lib/scene/mover.ts";
const MOVER_GLOBALS = ["requestAnimationFrame", "cancelAnimationFrame"];
const MOVER_PROPERTIES = ["performance.now"];

export default defineConfig([
  js.configs.recommended,
  ...svelte.configs["flat/recommended"],
  {
    files: ["src/**/*.{ts,svelte}", "e2e/**/*.ts"],
    languageOptions: {
      parser: tsParser,
      parserOptions: { extraFileExtensions: [".svelte"], sourceType: "module", ecmaVersion: 2023 },
      globals: { ...globals.browser, ...globals.node },
    },
    rules: {
      "no-eval": "error",
      "no-implied-eval": "error",
      "no-new-func": "error",
      "no-script-url": "error",
      "svelte/no-at-html-tags": "error",
      "no-unused-vars": "off",
      "no-undef": "off",
    },
  },
  // No clock and no randomness in the scene (AD-64, D-110, D-470): it moves
  // on a changed payload only, and its layout and furniture are pure.
  {
    files: ["src/lib/scene/**", "src/components/scene/**"],
    rules: {
      "no-restricted-globals": ["error", ...SCENE_GLOBALS],
      "no-restricted-properties": ["error", ...SCENE_PROPERTIES],
    },
  },
  // The mover runs the scene's one frame loop on the page's clock (AD-172,
  // D-470): its module alone may, and nothing else the scene is denied.
  {
    files: [MOVER],
    rules: {
      "no-restricted-globals": ["error", ...SCENE_GLOBALS.filter((g) => !MOVER_GLOBALS.includes(g.name))],
      "no-restricted-properties": [
        "error",
        ...SCENE_PROPERTIES.filter((p) => !MOVER_PROPERTIES.includes(`${p.object}.${p.property}`)),
      ],
    },
  },
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: {
      parser: svelteParser,
      parserOptions: { parser: tsParser, extraFileExtensions: [".svelte"], sourceType: "module" },
    },
  },
  globalIgnores(["dist/**", "node_modules/**", "coverage/**", "test-results/**", "**/*.generated.ts", "src/lib/generated/**"]),
]);
