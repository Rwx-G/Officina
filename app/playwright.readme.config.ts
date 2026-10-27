import { defineConfig, devices } from "@playwright/test";

// README screenshot and animated demo (AD-17): same build and fixtures as the
// e2e suite, run by hand with `pnpm readme-assets`, never a CI gate.
export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/readme-assets.spec.ts",
  reporter: "list",
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://localhost:4173",
    locale: "en-GB",
    timezoneId: "Europe/Paris",
  },
  webServer: {
    command: "pnpm build --mode e2e && pnpm exec vite preview --port 4173 --strictPort",
    url: "http://localhost:4173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
