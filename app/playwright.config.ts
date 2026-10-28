import { defineConfig, devices } from "@playwright/test";

// The real UI, built in `e2e` mode (mocked IPC), served by `vite preview`.
// Boards come from `pnpm e2e:fixtures`, i.e. real `officina board --json`
// output against the fake claude (AD-17).

// Another port for a second checkout run at the same time: a local run
// reuses a server already listening, which would serve the other build.
const port = Number(process.env.OFFICINA_E2E_PORT ?? 4173);

export default defineConfig({
  testDir: "./e2e",
  // Specs only: the `*.test.ts` files beside them are Vitest's.
  testMatch: "**/*.spec.ts",
  testIgnore: "**/readme-assets.spec.ts",
  fullyParallel: true,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    ...devices["Desktop Chrome"],
    baseURL: `http://localhost:${port}`,
    viewport: { width: 1400, height: 900 },
    locale: "en-GB",
    timezoneId: "Europe/Paris",
  },
  webServer: {
    command: `pnpm build --mode e2e && pnpm exec vite preview --port ${port} --strictPort`,
    url: `http://localhost:${port}`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
