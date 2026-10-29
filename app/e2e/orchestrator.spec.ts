// Story 5.8: the Orchestrator view (AD-92, AD-93) on views printed by the
// `orchestrator_fixture` example from homes the window's own writer filled
// (D-72's pattern), found through the fixture index (D-181): the briefing
// with each section and since when, the history with its outcomes and
// binding, the fourth view button, and the briefing's reading on leaving.
import { expect, test, type Page } from "@playwright/test";
import { fixtureOf } from "./fixture-index";
import { calls, loadBoard, loadRelay, openBoard, openScene, transitionsDone } from "./fixtures";
import type { OrchestratorView } from "../src/lib/generated/OrchestratorView";

function orchestrator(name: "orchestrator-empty" | "orchestrator-day"): OrchestratorView {
  return fixtureOf<OrchestratorView>("orchestrator", name);
}

async function answering(page: Page, view: OrchestratorView): Promise<void> {
  await page.addInitScript((answer) => {
    window.__officinaOrchestrator = answer;
  }, view);
}

async function openOrchestrator(page: Page, view: OrchestratorView): Promise<void> {
  await answering(page, view);
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  await page.getByTestId("view-orchestrator").click();
  await expect(page.getByTestId("orchestrator-view")).toBeVisible();
  await transitionsDone(page);
}

async function marks(page: Page): Promise<number> {
  return (await calls(page)).filter((call) => call.cmd === "mark_briefing_read").length;
}

test("the header switches to the Orchestrator view", async ({ page }) => {
  await answering(page, orchestrator("orchestrator-day"));
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  const button = page.getByTestId("view-orchestrator");
  await expect(page.getByTestId("view-switch").locator("> button")).toHaveCount(4);
  await expect(button).toHaveAttribute("aria-pressed", "false");
  // D-565: the presence mark is a shape and a name, never colour alone.
  await expect(button).toHaveAccessibleName("Orchestrator: absent");
  await expect(page.getByTestId("orchestrator-presence")).toHaveAttribute("data-presence", "absent");
  await button.click();
  await expect(page.getByTestId("orchestrator-view")).toBeVisible();
  await expect(page.getByTestId("column-needs_you")).toHaveCount(0);
  await expect(button).toHaveAttribute("aria-pressed", "true");
  // `m`, `f` and the digits keep acting in the Sessions view only.
  await page.keyboard.press("m");
  await page.keyboard.press("1");
  await expect(page.getByTestId("orchestrator-view")).toBeVisible();
  expect((await calls(page)).filter((call) => call.cmd === "focus")).toEqual([]);
  await page.getByTestId("view-sessions").click();
  await expect(page.getByTestId("column-needs_you")).toBeVisible();
  await expect(page.getByTestId("mode-board")).toHaveAttribute("aria-checked", "true");
});

test("the Orchestrator button is the same fixed control in Scene mode", async ({ page }) => {
  await answering(page, orchestrator("orchestrator-day"));
  await openScene(page, loadBoard("busy"), loadRelay("relay-clean"));
  const button = page.getByTestId("view-orchestrator");
  await expect(button).toBeVisible();
  await button.click();
  await expect(page.getByTestId("orchestrator-view")).toBeVisible();
  await expect(page.getByTestId("view-switch").locator("> button")).toHaveCount(4);
});

// D-564, D-163: every colour the view shows is a token of the theme
// resolved on the document root, and none is the attention colour.
for (const theme of ["light", "dark"] as const) {
  test(`the Orchestrator view reads the same in both themes but for its tokens (${theme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    await openOrchestrator(page, orchestrator("orchestrator-day"));
    await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
    const seen = await page.evaluate(() => {
      const resolve = (token: string): string => {
        const probe = document.createElement("span");
        probe.style.color = `var(${token})`;
        document.body.append(probe);
        const colour = getComputedStyle(probe).color;
        probe.remove();
        return colour;
      };
      const tokens = {
        surface: resolve("--color-surface"),
        text: resolve("--color-text"),
        muted: resolve("--color-text-muted"),
        attention: resolve("--color-attention"),
        fault: resolve("--color-stopped-text"),
      };
      const view = document.querySelector('[data-testid="orchestrator-view"]')!;
      const colours = new Set<string>();
      const backgrounds = new Set<string>();
      for (const element of [view, ...Array.from(view.querySelectorAll("*"))]) {
        const style = getComputedStyle(element);
        colours.add(style.color);
        if (style.backgroundColor !== "rgba(0, 0, 0, 0)") backgrounds.add(style.backgroundColor);
      }
      const briefing = getComputedStyle(document.querySelector('[data-testid="briefing"]')!);
      const mark = getComputedStyle(document.querySelector('[data-testid="orchestrator-presence"]')!);
      return {
        tokens,
        colours: [...colours],
        backgrounds: [...backgrounds],
        briefing: { background: briefing.backgroundColor, color: briefing.color },
        mark: mark.borderTopColor,
      };
    });
    expect(seen.briefing).toEqual({ background: seen.tokens.surface, color: seen.tokens.text });
    expect(seen.backgrounds).toEqual([seen.tokens.surface]);
    const allowed = [seen.tokens.text, seen.tokens.muted, seen.tokens.fault];
    for (const colour of seen.colours) expect(allowed, colour).toContain(colour);
    expect(seen.colours).not.toContain(seen.tokens.attention);
    expect(seen.backgrounds).not.toContain(seen.tokens.attention);
    // The button is pressed: its mark takes the button's own text colour.
    const pressed = await page.getByTestId("view-orchestrator").evaluate((b) => getComputedStyle(b).color);
    expect(seen.mark).toBe(pressed);
  });
}

test("the briefing shows each section and since when", async ({ page }) => {
  const day = orchestrator("orchestrator-day");
  await openOrchestrator(page, day);
  const briefing = page.getByTestId("briefing");
  await expect(page.getByTestId("briefing-since")).toContainText("since your last reading");
  for (const section of ["changed", "waiting", "approvals", "subjects", "triage", "runs", "work_orders"] as const) {
    await expect(briefing.getByTestId(`briefing-section-${section}`)).toContainText(String(day.briefing[section].count));
  }
  await expect(briefing.getByTestId("briefing-waiting")).toHaveCount(day.briefing.waiting.items.length);
  await expect(briefing.getByTestId("briefing-change")).toHaveCount(day.briefing.changed.items.length);
  await expect(briefing.getByTestId("briefing-run")).toHaveCount(day.briefing.runs.items.length);
  await expect(briefing.getByTestId("briefing-triage")).toHaveCount(day.briefing.triage.items.length);
  await expect(briefing.getByTestId("briefing-order")).toHaveCount(day.briefing.work_orders.items.length);
  // Session text with markup stays text (the fixture's names carry some).
  expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  await expect(page.locator('[data-testid="orchestrator-view"] img')).toHaveCount(0);
  await expect(page.getByTestId("template-option")).toHaveCount(day.templates.length);
  await expect(page.getByTestId("template-awaiting")).toHaveCount(day.awaiting_templates.length);

  // Before the first reading, everything is new.
  await page.evaluate(async (empty) => window.__officinaPushOrchestrator?.(empty), orchestrator("orchestrator-empty"));
  await expect(page.getByTestId("briefing-since")).toContainText("first reading");
  await expect(page.getByTestId("briefing-waiting")).toHaveCount(0);
  await expect(page.getByTestId("history-empty")).toBeVisible();
});

test("dropped items are counted", async ({ page }) => {
  const day = orchestrator("orchestrator-day");
  await openOrchestrator(page, day);
  await expect(page.getByTestId("briefing-dropped-changed")).toHaveCount(0);
  const crowded: OrchestratorView = {
    ...day,
    briefing: {
      ...day.briefing,
      changed: { ...day.briefing.changed, count: day.briefing.changed.count + 7, dropped: 7 },
      waiting: { ...day.briefing.waiting, count: day.briefing.waiting.count + 1, dropped: 1 },
    },
  };
  await page.evaluate(async (view) => window.__officinaPushOrchestrator?.(view), crowded);
  await expect(page.getByTestId("briefing-dropped-changed")).toContainText("7 older items left out");
  await expect(page.getByTestId("briefing-dropped-waiting")).toContainText("1 older item left out");
  await expect(page.getByTestId("briefing-section-changed")).toContainText(String(crowded.briefing.changed.count));
  // The window never cuts a list itself: every item sent is shown.
  await expect(page.getByTestId("briefing-change")).toHaveCount(day.briefing.changed.items.length);
});

test("leaving the view marks the briefing read", async ({ page }) => {
  await openOrchestrator(page, orchestrator("orchestrator-day"));
  expect(await marks(page)).toBe(0);
  await page.getByTestId("view-subjects").click();
  await expect.poll(() => marks(page)).toBe(1);
  // Coming back and leaving through another path reads it again.
  await page.getByTestId("view-orchestrator").click();
  await expect(page.getByTestId("orchestrator-view")).toBeVisible();
  expect(await marks(page)).toBe(1);
  await page.getByTestId("view-sessions").click();
  await expect.poll(() => marks(page)).toBe(2);
});

test("the history shows outcome, binding and unbound orders", async ({ page }) => {
  const day = orchestrator("orchestrator-day");
  await openOrchestrator(page, day);
  const orders = page.getByTestId("work-order");
  await expect(orders).toHaveCount(day.work_orders.length);
  for (const [index, order] of day.work_orders.entries()) {
    const item = orders.nth(index);
    await expect(item.getByTestId("work-order-name")).toHaveText(order.name);
    await expect(item).toHaveAttribute("data-outcome", order.outcome);
    if (order.bg_id !== null) await expect(item.getByTestId("work-order-binding")).toContainText(order.bg_id);
    await expect(item.getByTestId("work-order-unbound")).toHaveCount(order.unbound ? 1 : 0);
    await expect(item.getByTestId("work-order-printed-differs")).toHaveCount(order.printed_id_differs ? 1 : 0);
    for (const value of Object.values(order.values)) await expect(item).toContainText(value);
  }
  const failed = day.work_orders.find((order) => order.outcome === "failed")!;
  const failedItem = orders.nth(day.work_orders.indexOf(failed));
  await expect(failedItem.getByTestId("work-order-outcome")).toContainText("failed, exit 1");
  // Its error is the session's text: shown as text, never markup.
  await expect(failedItem.getByTestId("work-order-error")).toContainText("<img");
  // The bound session's state: on the board, its column; else unknown.
  const working = day.work_orders.find((order) => order.session?.phase === "working")!;
  await expect(orders.nth(day.work_orders.indexOf(working)).getByTestId("work-order-binding")).toContainText("working");
  const offBoard = day.work_orders.find((order) => order.session?.phase === "unknown")!;
  await expect(orders.nth(day.work_orders.indexOf(offBoard)).getByTestId("work-order-binding")).toContainText(
    "state unknown",
  );
});
