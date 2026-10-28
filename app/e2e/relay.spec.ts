// Story 2.7: catalogue approval and stop in the window (AD-24, AD-34, AD-35),
// on relay views printed by the real `officina relay --json` (D-25).
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, loadRelay, openBoard } from "./fixtures";
import type { RelayView } from "../src/lib/generated/RelayView";

async function open(page: Page, relay: RelayView): Promise<void> {
  await openBoard(page, loadBoard("busy"), {}, relay);
}

async function approvals(page: Page): Promise<unknown[]> {
  return (await calls(page)).filter((c) => c.cmd === "approve_catalogue").map((c) => c.args);
}

test.describe("catalogue approval", () => {
  test("a pending catalogue change shows the diff and approves the displayed hash", async ({ page }) => {
    const pending = loadRelay("relay-pending");
    const diff = pending.pending!.diff;
    await page.addInitScript((approved) => {
      window.__officinaRelayApproved = approved;
    }, loadRelay("relay-pending-approved"));
    await open(page, pending);

    const banner = page.getByTestId("catalogue-banner");
    await expect(banner).toBeVisible();
    await expect(banner.getByRole("heading", { level: 2 })).toHaveText("The relay catalogue changed");
    for (const name of diff.actions.added) {
      const added = banner.locator(`[data-testid="action-added"][data-name="${name}"]`);
      await expect(added).toContainText(pending.pending!.current.actions[name]!.program);
    }
    for (const entry of diff.actions.changed) {
      const changed = banner.locator(`[data-testid="action-changed"][data-name="${entry.name}"]`);
      for (const field of entry.fields) {
        await expect(changed.locator(`[data-field="${field.field}"]`)).toBeVisible();
      }
    }
    for (const name of diff.actions.removed) {
      await expect(banner.locator(`[data-testid="action-removed"][data-name="${name}"]`)).toBeVisible();
    }
    for (const entry of diff.projects.changed) {
      await expect(banner.locator(`[data-testid="project-changed"][data-name="${entry.name}"]`)).toBeVisible();
    }
    await expect(banner.getByTestId("catalogue-issues").locator("li")).toHaveCount(pending.issues.length);

    await banner.getByTestId("approve-catalogue").click();
    expect(await approvals(page)).toEqual([{ hash: pending.pending!.hash }]);
    await expect(banner).toBeHidden();
    await expect(page.getByTestId("catalogue-chip")).toBeHidden();
  });

  test("the header chip marks a pending change", async ({ page }) => {
    await open(page, loadRelay("relay-pending"));
    const chip = page.getByTestId("catalogue-chip");
    await expect(chip).toHaveText("catalogue change");
    const amber = await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue("--color-attention").trim(),
    );
    await expect(chip).toHaveCSS("background-color", hexToRgb(amber));

    await page.evaluate(async (clean) => window.__officinaPushRelay?.(clean), loadRelay("relay-clean"));
    await expect(chip).toBeHidden();
    await expect(page.getByTestId("catalogue-banner")).toBeHidden();
  });

  test("a refused approval says the catalogue changed again", async ({ page }) => {
    const shown = loadRelay("relay-pending");
    const again = loadRelay("relay-pending-again");
    expect(again.pending!.hash).not.toBe(shown.pending!.hash);
    await open(page, shown);
    // The floor changes again after the window displayed the diff: the
    // backend holds the new state, the window has not seen it yet.
    await page.evaluate((next) => {
      window.__officinaRelay = next;
    }, again);
    const banner = page.getByTestId("catalogue-banner");
    await banner.getByTestId("approve-catalogue").click();

    expect(await approvals(page)).toEqual([{ hash: shown.pending!.hash }]);
    await expect(banner.getByTestId("approval-refused")).toContainText("changed again");
    await expect(banner).toBeVisible();
    const changed = again.pending!.diff.actions.changed.find((c) => c.name === "apply")!;
    await expect(
      banner.locator(`[data-testid="action-changed"][data-name="apply"] [data-field="timeout_s"]`),
    ).toContainText(String(changed.fields.find((f) => f.field === "timeout_s")!.after));

    await banner.getByTestId("approve-catalogue").click();
    expect(await approvals(page)).toEqual([{ hash: shown.pending!.hash }, { hash: again.pending!.hash }]);
  });

  test("the banner lists catalogue notices", async ({ page }) => {
    const pending = loadRelay("relay-pending");
    expect(pending.notices.length).toBeGreaterThan(0);
    await open(page, pending);
    const items = page.getByTestId("catalogue-banner").getByTestId("catalogue-notice");
    await expect(items).toHaveCount(pending.notices.length);
    for (const [i, notice] of pending.notices.entries()) {
      await expect(items.nth(i)).toHaveAttribute("data-kind", notice.kind);
      await expect(items.nth(i)).toContainText(notice.action);
      await expect(items.nth(i)).toContainText(notice.explanation);
    }
  });

  test("no key approves", async ({ page }) => {
    await open(page, loadRelay("relay-pending"));
    const approve = page.getByTestId("approve-catalogue");
    await approve.focus();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Space");
    for (const key of ["1", "2", "a", "r"]) await page.keyboard.press(key);
    expect(await approvals(page)).toEqual([]);
    await expect(page.getByTestId("catalogue-banner")).toBeVisible();
  });

  test("catalogue text renders as text, never as HTML", async ({ page }) => {
    const pending = loadRelay("relay-pending");
    const description = pending.pending!.current.actions["restart"]!.description;
    expect(description).toContain("<img");
    await open(page, pending);
    const banner = page.getByTestId("catalogue-banner");
    await expect(banner.locator('[data-testid="action-added"][data-name="restart"]')).toContainText(description);
    await expect(banner.getByTestId("catalogue-issues")).toContainText("<b>bad</b>");
    await expect(banner.locator("img")).toHaveCount(0);
    await expect(banner.locator("b")).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  });
});

test.describe("stop", () => {
  test("the stop toggle calls set_relay_stopped", async ({ page }) => {
    await open(page, loadRelay("relay-clean"));
    const toggle = page.getByTestId("relay-toggle");
    await expect(toggle).toHaveAccessibleName("Stop relay");
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await toggle.click();
    await expect(toggle).toHaveAccessibleName("Resume relay");
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await toggle.click();
    await expect(toggle).toHaveAccessibleName("Stop relay");
    const stops = (await calls(page)).filter((c) => c.cmd === "set_relay_stopped").map((c) => c.args);
    expect(stops).toEqual([{ stopped: true }, { stopped: false }]);
  });

  // The chip's label reads 4.5:1 on the danger colour in both themes (D-529).
  test("a stopped relay shows the danger chip and the board banner", async ({ page }) => {
    await open(page, loadRelay("relay-stopped"));
    const chip = page.getByTestId("relay-stopped-chip");
    await expect(chip).toHaveText("relay stopped");
    const danger = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--color-danger").trim());
    await expect(chip).toHaveCSS("background-color", hexToRgb(danger));
    await expect(page.getByTestId("relay-stopped-banner")).toContainText("The relay is stopped since");
    await expect(page.getByTestId("relay-toggle")).toHaveAccessibleName("Resume relay");
    await expect(page.getByTestId("catalogue-chip")).toBeHidden();
  });

  test("a workstation without the relay shows no relay control", async ({ page }) => {
    await openBoard(page, loadBoard("busy"));
    await expect(page.getByTestId("column-needs_you")).toBeVisible();
    await expect(page.getByTestId("relay-toggle")).toHaveCount(0);
    await expect(page.getByTestId("catalogue-chip")).toHaveCount(0);
  });
});

function hexToRgb(hex: string): string {
  const value = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(value.slice(i, i + 2), 16));
  return `rgb(${r}, ${g}, ${b})`;
}

// Story 2.14 (D-173): decisions the window did not make, from files written
// behind its back, are listed in the Relay view until acknowledged.
test.describe("anomalies", () => {
  test("anomalies are listed", async ({ page }) => {
    const relay = loadRelay("relay-anomalies");
    await open(page, relay);
    await page.getByTestId("view-relay").click();
    const section = page.getByTestId("relay-anomalies");
    await expect(section).toBeVisible();
    const rows = section.getByTestId("relay-anomaly");
    await expect(rows).toHaveCount(relay.anomalies.length);
    for (const anomaly of relay.anomalies) {
      const row = section.locator(`[data-testid="relay-anomaly"][data-kind="${anomaly.kind}"]`);
      await expect(row).toContainText(anomaly.detail);
    }

    const first = relay.anomalies[0]!;
    await rows.first().getByTestId("acknowledge-anomaly").press("Enter");
    expect((await calls(page)).filter((c) => c.cmd === "acknowledge_anomaly")).toEqual([]);
    await rows.first().getByTestId("acknowledge-anomaly").click();
    expect((await calls(page)).filter((c) => c.cmd === "acknowledge_anomaly").map((c) => c.args)).toEqual([
      { key: first.key },
    ]);
    await expect(rows.first().getByTestId("anomaly-acknowledged")).toBeVisible();
    await expect(section.getByTestId("acknowledge-anomaly")).toHaveCount(relay.anomalies.length - 1);
  });

  test("a relay without anomalies lists none", async ({ page }) => {
    await open(page, loadRelay("relay-clean"));
    await page.getByTestId("view-relay").click();
    await expect(page.getByTestId("relay-view")).toBeVisible();
    await expect(page.getByTestId("relay-anomalies")).toHaveCount(0);
  });
});
