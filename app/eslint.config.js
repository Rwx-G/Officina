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
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: {
      parser: svelteParser,
      parserOptions: { parser: tsParser, extraFileExtensions: [".svelte"], sourceType: "module" },
    },
  },
  globalIgnores(["dist/**", "node_modules/**", "coverage/**", "test-results/**", "**/*.generated.ts", "src/lib/generated/**"]),
]);
