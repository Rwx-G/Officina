// The lint gate is narrow on purpose: it blocks the XSS and code-injection
// constructs that the type checker cannot see. Session prompts and assistant
// messages are untrusted text, so `{@html}` is an error, not a warning.
import { defineConfig, globalIgnores } from "eslint/config";
import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import svelteParser from "svelte-eslint-parser";
import tsParser from "@typescript-eslint/parser";
import globals from "globals";

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
  // on a changed payload only, and its layout and furniture are pure. Story
  // 4.10 exempts the mover's own module, and that module only.
  {
    files: ["src/lib/scene/**", "src/components/scene/**"],
    rules: {
      "no-restricted-globals": [
        "error",
        { name: "setInterval", message: "The scene has no timer: it changes on a changed payload only (AD-64)." },
        { name: "setTimeout", message: "The scene has no timer: it changes on a changed payload only (AD-64)." },
        { name: "requestAnimationFrame", message: "The scene's only frame loop is the mover's (D-470)." },
      ],
      "no-restricted-properties": [
        "error",
        { object: "Date", property: "now", message: "The scene never reads the clock (AD-64)." },
        { object: "performance", property: "now", message: "The scene never reads the clock (AD-64)." },
        { object: "Math", property: "random", message: "The scene is drawn from seeds, never at random (AD-60, AD-173)." },
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
