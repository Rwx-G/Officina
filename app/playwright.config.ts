import { defineConfig, devices } from "@playwright/test";

// The real UI, built in `e2e` mode (mocked IPC), served by `vite preview`.
// Boards come from `pnpm e2e:fixtures`, i.e. real `officina board --json`
// output against the fake claude (AD-17).
export default defineConfig({
  testDir: "./e2e",
  testIgnore: "**/readme-assets.spec.ts",
  fullyParallel: true,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://localhost:4173",
    viewport: { width: 1400, height: 900 },
    locale: "en-GB",
    timezoneId: "Europe/Paris",
  },
  webServer: {
    command: "pnpm build --mode e2e && pnpm exec vite preview --port 4173 --strictPort",
    url: "http://localhost:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
