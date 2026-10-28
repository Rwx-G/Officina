import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The built window's Content Security Policy, read from the Tauri config
// rather than copied: `vite preview` serves the e2e build under it, so the
// Playwright suite runs the UI under the policy the window enforces.
const tauri = JSON.parse(readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf8")) as {
  app: { security: { csp: string } };
};

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  preview: { headers: { "Content-Security-Policy": tauri.app.security.csp } },
  // No asset inlined as a `data:` URI: the policy loads fonts from the app
  // itself only (`font-src 'self'`), and the smallest font subsets would
  // otherwise be inlined and refused.
  build: { outDir: "dist", emptyOutDir: true, target: "es2022", assetsInlineLimit: 0 },
});
