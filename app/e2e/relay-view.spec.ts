// Story 2.9: the Relay view (AD-35, FR-10) on relay views printed by the real
// `officina relay --json` (D-25): `relay-activity` holds real runs and
// refusals, a gate deny and a gate error, a seeded abandoned run and an
// unreadable line, and a pending change with issues.
import { expect, test, type Page } from "@playwright/test";
import { runDuration } from "../src/lib/time";
import { loadBoard, loadRelay, openBoard } from "./fixtures";
import type { RelayView } from "../src/lib/generated/RelayView";

async function openRelayView(page: Page, relay: RelayView): Promise<void> {
  await openBoard(page, loadBoard("busy"), {}, relay);
  await page.getByTestId("view-relay").click();
  await expect(page.getByTestId("relay-view")).toBeVisible();
}

test.describe("view toggle", () => {
  test("the toggle switches between Sessions and Relay", async ({ page }) => {
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-activity"));
    const board = page.getByTestId("view-sessions");
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
    await expect(page.getByTestId("relay-toggle")).toHaveAccessibleName("Stop relay");

    await page.evaluate(async (stopped) => window.__officinaPushRelay?.(stopped), loadRelay("relay-stopped"));
    await expect(page.getByTestId("relay-view")).toBeVisible();
    await expect(page.getByTestId("relay-stopped-chip")).toBeVisible();
    await expect(page.getByTestId("relay-toggle")).toHaveAccessibleName("Resume relay");
    await expect(page.getByTestId("relay-view-stop")).toHaveAccessibleName("Resume relay");
    await expect(page.getByTestId("relay-state")).toContainText("Stopped since");
  });

  test("a workstation without the relay has no Relay view", async ({ page }) => {
    await openBoard(page, loadBoard("busy"));
    await expect(page.getByTestId("view-switch")).toBeVisible();
    await expect(page.getByTestId("view-relay")).toHaveCount(0);
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
    await expect(rows).toHaveCount(relay.recent.length);
    // Each row is its entry, coloured by the tone Rust computed for it.
    const toneClass = { ok: /\bok\b/, failed: /\bfault\b/, refused: /\bfault\b/ } as const;
    for (const [i, entry] of relay.recent.entries()) {
      const row = rows.nth(i);
      await expect(row).toHaveAttribute("data-kind", entry.event.kind);
      const abandoned = entry.event.kind === "run" && entry.event.state === "abandoned";
      if (abandoned) await expect(row).toHaveClass(/\babandoned\b/);
      else if (entry.tone) await expect(row).toHaveClass(toneClass[entry.tone]);
    }

    // What the fixture script ran, as the window must say it.
    const rowOf = (match: (entry: RelayView["recent"][number]) => boolean) => {
      const index = relay.recent.findIndex(match);
      expect(index).toBeGreaterThanOrEqual(0);
      return rows.nth(index);
    };
    const ran = (action: string) => (e: RelayView["recent"][number]) =>
      e.event.kind === "run" && e.event.state === "finished" && e.event.started.action === action;
    const refused = (action: string) => (e: RelayView["recent"][number]) =>
      e.event.kind === "refused" && e.event.action === action;
    await expect(rowOf(ran("check")).getByTestId("recent-outcome")).toHaveText("exit 0");
    await expect(rowOf(ran("check"))).toHaveClass(/\bok\b/);
    const probe = rowOf(ran("probe"));
    await expect(probe.getByTestId("recent-outcome")).toHaveText("exit 3");
    await expect(probe).toHaveClass(/\bfault\b/);
    const probed = relay.recent.find(ran("probe"))!;
    if (probed.event.kind === "run") await expect(probe).toContainText(runDuration(probed.event.finished!.duration_ms));
    await expect(rowOf(ran("status-page")).getByTestId("recent-outcome")).toHaveText("no response");
    await expect(rowOf(refused("apply")).getByTestId("recent-outcome")).toHaveText("refused: its check has not run");
    await expect(rowOf(refused("probe")).getByTestId("recent-outcome")).toHaveText("refused: invalid value");
    await expect(rowOf((e) => e.event.kind === "gate_denied").getByTestId("recent-outcome")).toHaveText(
      "denied: guarded program ssh",
    );

    const abandoned = page.locator('[data-testid="recent-entry"][data-state="abandoned"]');
    await expect(abandoned).toHaveCount(relay.recent.filter((e) => e.event.kind === "run" && e.event.state === "abandoned").length);
    await expect(abandoned.first().getByTestId("recent-outcome")).toHaveText("abandoned");
  });

  test("journal and gate diagnostics are surfaced", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    expect(relay.journal_corrupt_lines).toBeGreaterThan(0);
    expect(relay.gate_errors).toBeGreaterThan(0);
    await openRelayView(page, relay);
    await expect(page.getByTestId("journal-corrupt")).toContainText(`${relay.journal_corrupt_lines} unreadable journal`);
    await expect(page.getByTestId("gate-errors")).toContainText(`${relay.gate_errors} gate error`);
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

  test("HTTP actions show their method and URL template", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    await openRelayView(page, relay);
    for (const [name, action] of Object.entries(relay.catalogue!.actions)) {
      if (!action.http) continue;
      const row = page.locator(`[data-testid="catalogue-action"][data-name="${name}"]`);
      await expect(row.getByTestId("http-method")).toHaveText(action.http.method);
      await expect(row.getByTestId("http-url")).toHaveText(action.http.url);
      if (action.http.credential) {
        await expect(row.getByTestId("http-credential")).toHaveText(action.http.credential);
      }
    }
    const http = relay.recent.find((e) => e.event.kind === "run" && e.event.started.method)!;
    expect(http).toBeDefined();
    const row = page.getByTestId("recent-entry").nth(relay.recent.indexOf(http));
    // The fixture's one HTTP run had nothing listening on its port.
    await expect(row.getByTestId("recent-outcome")).toHaveText("no response");
  });

  test("credential status shows present, missing and unchecked", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    expect(relay.credentials.length).toBeGreaterThanOrEqual(2);
    await openRelayView(page, relay);
    const cells = page.getByTestId("credential-presence");
    await expect(cells).toHaveCount(relay.credentials.length);
    for (let i = 0; i < relay.credentials.length; i++) await expect(cells.nth(i)).toHaveText("unchecked");

    // The window's collector fills presence (D-41); `relay --json` never does.
    const checked = {
      ...relay,
      credentials: relay.credentials.map((c, i) => ({ ...c, present: i === 0 })),
    };
    await page.evaluate(async (view) => window.__officinaPushRelay?.(view), checked);
    await expect(cells.nth(0)).toHaveText("present");
    await expect(cells.nth(1)).toHaveText("missing");
    for (const credential of relay.credentials) {
      const row = page.locator(`[data-testid="catalogue-credential"][data-name="${credential.name}"]`);
      await expect(row).toContainText(credential.hosts.join(", "));
    }
  });

  test("credential and URL text renders as text", async ({ page }) => {
    const relay = loadRelay("relay-activity");
    const hostile = `<img src=x onerror="window.__officinaXss = 1"><b>x</b>`;
    // A view no floor could produce (validation refuses such names), pushed
    // to prove the window renders whatever it receives as text.
    const forged = structuredClone(relay);
    const name = Object.keys(forged.catalogue!.actions).find((n) => forged.catalogue!.actions[n]!.http)!;
    forged.catalogue!.actions[name]!.http!.url = `https://example.com/${hostile}`;
    forged.catalogue!.actions[name]!.http!.credential = hostile;
    forged.credentials[0]!.name = hostile;
    forged.credentials[0]!.hosts = [hostile];
    await openRelayView(page, forged);
    const view = page.getByTestId("relay-view");
    await expect(view.getByTestId("catalogue-credential").first()).toContainText(hostile);
    await expect(view.locator(`[data-testid="catalogue-action"][data-name="${name}"]`)).toContainText(hostile);
    await expect(view.locator("img")).toHaveCount(0);
    await expect(view.locator("b, i")).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  });

  test("the Relay view's stop toggle calls set_relay_stopped", async ({ page }) => {
    await openRelayView(page, loadRelay("relay-activity"));
    const stop = page.getByTestId("relay-view-stop");
    await stop.click();
    await expect(stop).toHaveAccessibleName("Resume relay");
    await expect(page.getByTestId("relay-stopped-chip")).toBeVisible();
  });
});

/** The colour `token` resolves to on the document root, as a computed
 *  `color` (D-163): jsdom resolves no `var()`, the browser does. */
async function tokenColour(page: Page, token: string): Promise<string> {
  return page.evaluate((name) => {
    const probe = document.createElement("span");
    probe.style.color = `var(${name})`;
    document.body.append(probe);
    const colour = getComputedStyle(probe).color;
    probe.remove();
    return colour;
  }, token);
}

// Story 5.4 (AD-76, D-564): templates, the review model and the `auto`
// class in the catalogue table and the pending diff, on views printed by
// the real `officina relay --json`. They add no colour: the text takes the
// catalogue's tokens in both themes, set as themes.spec.ts sets them.
for (const theme of ["light", "dark"] as const) {
  test.describe(`templates and auto in the ${theme} theme`, () => {
    test.beforeEach(async ({ page }) => {
      await page.emulateMedia({ colorScheme: theme });
    });

    test("the catalogue lists templates with their projects, values and model", async ({ page }) => {
      const relay = loadRelay("relay-clean");
      const templates = Object.entries(relay.catalogue!.templates);
      expect(templates.length).toBeGreaterThan(0);
      await openRelayView(page, relay);
      await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
      const table = page.getByTestId("catalogue-templates-table");
      await expect(table.getByTestId("catalogue-template")).toHaveCount(templates.length);
      for (const [name, template] of templates) {
        const row = table.locator(`[data-testid="catalogue-template"][data-name="${name}"]`);
        await expect(row.getByTestId("template-description")).toHaveText(template.description);
        await expect(row.getByTestId("template-projects")).toHaveText(template.projects.join(", "));
        await expect(row.getByTestId("template-prompt")).toHaveText(template.prompt);
        for (const [param, values] of Object.entries(template.params)) {
          await expect(row.locator(`[data-testid="template-param"][data-name="${param}"]`)).toHaveText(
            `${param}: one of ${values.join(", ")}`,
          );
        }
        await expect(row.getByTestId("template-model")).toHaveText(template.model ?? "default");
      }
      await expect(page.getByTestId("catalogue-review-model")).toContainText(relay.catalogue!.auto_review.model!);

      const text = await tokenColour(page, "--color-text");
      const muted = await tokenColour(page, "--color-text-muted");
      const first = table.getByTestId("catalogue-template").first();
      await expect(first.getByTestId("template-projects")).toHaveCSS("color", text);
      await expect(first.getByTestId("template-description")).toHaveCSS("color", muted);
      await expect(table.locator("th").first()).toHaveCSS("color", muted);
    });

    test("an auto action shows its check and hourly cap", async ({ page }) => {
      const relay = loadRelay("relay-clean");
      const auto = Object.entries(relay.catalogue!.actions).find(([, action]) => action.class === "auto");
      expect(auto).toBeDefined();
      const [name, action] = auto!;
      expect(action.check).not.toBeNull();
      expect(action.auto_max_per_hour).toBeDefined();
      await openRelayView(page, relay);
      const cell = page.locator(`[data-testid="catalogue-action"][data-name="${name}"]`).getByTestId("catalogue-class");
      await expect(cell).toHaveText(`auto (after ${action.check}, at most ${action.auto_max_per_hour} per hour)`);
      await expect(cell).toHaveCSS("color", await tokenColour(page, "--color-text"));
    });

    test("the pending diff shows the format change and the templates added", async ({ page }) => {
      const relay = loadRelay("relay-previous-format");
      const pending = relay.pending!;
      expect(pending.diff.format).not.toBeNull();
      expect(pending.diff.templates.added.length).toBeGreaterThan(0);
      await openRelayView(page, relay);
      const banner = page.getByTestId("catalogue-banner");
      await expect(banner.locator('[data-testid="catalogue-field"][data-field="format"]')).toHaveText(
        `format: ${pending.diff.format!.before} to ${pending.diff.format!.after}`,
      );
      await expect(banner.getByTestId("catalogue-format-note")).toContainText("approving once covers it");
      await expect(banner.getByTestId("template-added")).toHaveCount(pending.diff.templates.added.length);
      for (const name of pending.diff.templates.added) {
        const added = banner.locator(`[data-testid="template-added"][data-name="${name}"]`);
        const template = pending.current.templates[name]!;
        await expect(added.getByTestId("template-added-projects")).toHaveText(template.projects.join(", "));
        await expect(added.getByTestId("template-added-prompt")).toHaveText(template.prompt);
      }
      // The previous approval still stands until the author approves.
      expect(relay.catalogue!.format).toBe(pending.diff.format!.before);
      await expect(page.getByTestId("catalogue-table")).toBeVisible();
      await expect(banner.getByTestId("template-added").first()).toHaveCSS(
        "color",
        await tokenColour(page, "--color-text"),
      );
    });
  });
}
