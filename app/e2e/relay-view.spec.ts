// Story 2.9: the Relay view (AD-35, FR-10) on relay views printed by the real
// `officina relay --json` (D-25): `relay-activity` holds real runs and
// refusals, a gate deny and a gate error, a seeded abandoned run and an
// unreadable line, and a pending change with issues.
import { expect, test, type Page } from "@playwright/test";
import { activityRow } from "../src/lib/relay-text";
import { runDuration } from "../src/lib/time";
import { loadBoard, loadRelay, openBoard } from "./fixtures";
import type { RelayView } from "../src/lib/generated/RelayView";

async function openRelayView(page: Page, relay: RelayView): Promise<void> {
  await openBoard(page, loadBoard("busy"), {}, relay);
  await page.getByTestId("view-relay").click();
  await expect(page.getByTestId("relay-view")).toBeVisible();
}

test.describe("view toggle", () => {
  test("the toggle switches between Board and Relay", async ({ page }) => {
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-activity"));
    const board = page.getByTestId("view-board");
    const relay = page.getByTestId("view-relay");
    await expect(board).toHaveAttribute("aria-pressed", "true");
    await expect(relay).toHaveAttribute("aria-pressed", "false");

    await relay.click();
    await expect(page.getByTestId("relay-view")).toBeVisible();
    await expect(page.getByTestId("column-needs_you")).toHaveCount(0);
    await expect(relay).toHaveAttribute("aria-pressed", "true");

    await board.click();
    await expect(page.getByTestId("column-needs_you")).toBeVisible();
    await expect(page.getByTestId("relay-view")).toHaveCount(0);
    await expect(board).toHaveAttribute("aria-pressed", "true");
  });

  test("chips and stop toggle stay visible in the Relay view", async ({ page }) => {
    await openRelayView(page, loadRelay("relay-activity"));
    await expect(page.getByTestId("catalogue-chip")).toBeVisible();
    await expect(page.getByTestId("relay-toggle")).toHaveText("Stop relay");

    await page.evaluate(async (stopped) => window.__officinaPushRelay?.(stopped), loadRelay("relay-stopped"));
    await expect(page.getByTestId("relay-view")).toBeVisible();
    await expect(page.getByTestId("relay-stopped-chip")).toBeVisible();
    await expect(page.getByTestId("relay-toggle")).toHaveText("Resume relay");
    await expect(page.getByTestId("relay-view-stop")).toHaveText("Resume relay");
    await expect(page.getByTestId("relay-state")).toContainText("Stopped since");
  });

  test("a workstation without the relay has no view switch", async ({ page }) => {
    await openBoard(page, loadBoard("busy"));
    await expect(page.getByTestId("view-switch")).toHaveCount(0);
  });
});

test.describe("Relay view", () => {
  test("every section renders from the relay fixture", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    await openRelayView(page, relay);
    const view = page.getByTestId("relay-view");

    await expect(view.getByTestId("relay-stop")).toContainText("Running");
    await expect(view.getByTestId("relay-floor")).toHaveText(relay.floor!);

    const banner = view.getByTestId("catalogue-banner");
    for (const entry of relay.pending!.diff.actions.changed) {
      const changed = banner.locator(`[data-testid="action-changed"][data-name="${entry.name}"]`);
      for (const field of entry.fields) await expect(changed.locator(`[data-field="${field.field}"]`)).toBeVisible();
    }
    await expect(banner.getByTestId("approve-catalogue")).toBeVisible();

    const issues = view.getByTestId("relay-issue");
    await expect(issues).toHaveCount(relay.issues.length);
    for (const [i, issue] of relay.issues.entries()) {
      await expect(issues.nth(i)).toContainText(issue.message);
      await expect(issues.nth(i).getByTestId("relay-issue-line")).toHaveText(`line ${issue.line}`);
    }

    const catalogue = relay.catalogue!;
    const rows = view.getByTestId("catalogue-action");
    await expect(rows).toHaveCount(Object.keys(catalogue.actions).length);
    for (const [name, action] of Object.entries(catalogue.actions)) {
      const row = view.locator(`[data-testid="catalogue-action"][data-name="${name}"]`);
      await expect(row.getByTestId("catalogue-class")).toContainText(action.class);
      const projects = Object.entries(catalogue.projects)
        .filter(([, project]) => project.actions.includes(name))
        .map(([project]) => project);
      await expect(row.getByTestId("catalogue-projects")).toHaveText(projects.join(", "));
      await expect(row).toContainText(action.program);
    }
    await expect(view.getByTestId("catalogue-project")).toHaveCount(Object.keys(catalogue.projects).length);

    await expect(view.getByTestId("recent-entry")).toHaveCount(relay.recent.length);
  });

  test("recent activity shows outcomes and abandoned runs", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    await openRelayView(page, relay);
    const rows = page.getByTestId("recent-entry");
    for (const [i, entry] of relay.recent.entries()) {
      const row = rows.nth(i);
      await expect(row).toHaveAttribute("data-kind", entry.event.kind);
      await expect(row.getByTestId("recent-outcome")).toHaveText(activityRow(entry).outcome);
    }

    const abandoned = page.locator('[data-testid="recent-entry"][data-state="abandoned"]');
    await expect(abandoned).toHaveCount(relay.recent.filter((e) => e.event.kind === "run" && e.event.state === "abandoned").length);
    await expect(abandoned.first().getByTestId("recent-outcome")).toHaveText("abandoned");

    const failed = relay.recent.find(
      (e) => e.event.kind === "run" && e.event.finished?.outcome === "exited" && e.event.finished.exit_code !== 0,
    )!;
    expect(failed).toBeDefined();
    const failedRow = rows.nth(relay.recent.indexOf(failed));
    await expect(failedRow.getByTestId("recent-outcome")).toHaveText(activityRow(failed).outcome);
    await expect(failedRow).toContainText(runDuration(activityRow(failed).durationMs!));

    const refused = page.locator('[data-testid="recent-entry"][data-kind="refused"]');
    await expect(refused.first().getByTestId("recent-outcome")).toContainText("refused:");
    await expect(page.locator('[data-testid="recent-entry"][data-kind="gate_denied"]').first()).toContainText("ssh");
  });

  test("journal and gate diagnostics are surfaced", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    expect(relay.journal_corrupt_lines).toBeGreaterThan(0);
    expect(relay.gate_errors_24h).toBeGreaterThan(0);
    await openRelayView(page, relay);
    await expect(page.getByTestId("journal-corrupt")).toContainText(`${relay.journal_corrupt_lines} unreadable journal`);
    await expect(page.getByTestId("gate-errors")).toContainText(`${relay.gate_errors_24h} gate error`);
    await expect(page.locator('[data-testid="recent-entry"][data-kind="gate_error"]').first()).toContainText(
      "gate error",
    );
  });

  test("journal text renders as text, never as HTML", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    const excerpt = relay.recent
      .map((e) => (e.event.kind === "gate_denied" ? e.event.command_excerpt : null))
      .find((text) => text?.includes("<img"));
    expect(excerpt).toBeDefined();
    await openRelayView(page, relay);
    const view = page.getByTestId("relay-view");
    await expect(view.locator('[data-testid="recent-entry"][data-kind="gate_denied"]').first()).toContainText(excerpt!);
    await expect(view.locator('[data-testid="recent-entry"][data-kind="refused"]').first()).toContainText(
      "host=<b>web02</b>",
    );
    await expect(view.getByTestId("relay-issue").first()).toContainText("<i>broken</i>");
    await expect(view.locator("img")).toHaveCount(0);
    await expect(view.locator("b, i")).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  });

  test("the Relay view's stop toggle calls set_relay_stopped", async ({ page }) => {
    await openRelayView(page, loadRelay("relay-activity"));
    const stop = page.getByTestId("relay-view-stop");
    await stop.click();
    await expect(stop).toHaveText("Resume relay");
    await expect(page.getByTestId("relay-stopped-chip")).toBeVisible();
  });
});
