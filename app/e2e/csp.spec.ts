// Story 2.14 (AC 10): the UI runs under the Content Security Policy the
// built window enforces, read from the Tauri config by `vite preview`, with
// no inline style or script needed: no violation fires on the board, on an
// approval card or in the Relay view.
import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { loadBoard, loadRelay, openBoard } from "./fixtures";

const tauri = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8")) as {
  app: { security: { csp: string } };
};

test("the window's CSP holds with no violation", async ({ page }) => {
  await page.addInitScript(() => {
    const seen: string[] = [];
    (window as unknown as { __cspViolations: string[] }).__cspViolations = seen;
    document.addEventListener("securitypolicyviolation", (event) => {
      seen.push(`${event.violatedDirective} ${event.blockedURI}`);
    });
  });
  const served = page.waitForResponse((response) => new URL(response.url()).pathname === "/");
  await openBoard(page, loadBoard("approvals"), {}, loadRelay("relay-activity"));
  expect((await served).headers()["content-security-policy"]).toBe(tauri.app.security.csp);
  await expect(page.getByTestId("approval-card").first()).toBeVisible();
  await page.getByTestId("view-relay").click();
  await expect(page.getByTestId("relay-view")).toBeVisible();
  const violations = await page.evaluate(
    () => (window as unknown as { __cspViolations: string[] }).__cspViolations,
  );
  expect(violations).toEqual([]);
});
