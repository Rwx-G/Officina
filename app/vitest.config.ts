import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  resolve: { conditions: ["browser"] },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "e2e/**/*.test.ts", "scripts/*.test.ts"],
    // Stubs for the browser APIs jsdom lacks.
    setupFiles: ["src/test-setup.ts"],
  },
});
