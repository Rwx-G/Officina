// Story 5.8: the Orchestrator view (AD-92, AD-93) on views printed by the
// `orchestrator_fixture` example from homes the window's own writer filled
// (D-72's pattern), found through the fixture index (D-181): the briefing
// with each section and since when, the history with its outcomes and
// binding, the fourth view button, and the briefing's reading on leaving.
// Story 5.9: the composer and the work order card, on cards core issued
// from the day's home through the same example (kind `card`); the mock
// answers issue, edit and launch as the test sets them.
// Story 5.24: the card's permission line and its tooltip, on the card core
// issued again once the project takes Officina's default.
import { expect, test, type Page } from "@playwright/test";
import { fixtureOf } from "./fixture-index";
import {
  PENDING_LIMIT,
  calls,
  loadBoard,
  loadRelay,
  openBoard,
  openScene,
  sceneBoardOrchestrator,
  sceneView,
  transitionsDone,
  withPending,
} from "./fixtures";
import { PROPOSAL_STATUS_TEXT, invalidReasonText } from "../src/lib/orchestrator-text";
import type { CardAnswer } from "../src/lib/generated/CardAnswer";
import type { LaunchAnswer } from "../src/lib/generated/LaunchAnswer";
import type { MeetingView } from "../src/lib/generated/MeetingView";
import type { OrchestratorView } from "../src/lib/generated/OrchestratorView";
import type { StartAnswer } from "../src/lib/generated/StartAnswer";
import type { WorkOrderCard } from "../src/lib/generated/WorkOrderCard";
import type { WorkOrderView } from "../src/lib/generated/WorkOrderView";

function orchestrator(name: "orchestrator-empty" | "orchestrator-day" | "orchestrator-live"): OrchestratorView {
  return fixtureOf<OrchestratorView>("orchestrator", name);
}

function card(name: "card-bump" | "card-bump-minor" | "card-bump-new-subject" | "card-bump-officina"): WorkOrderCard {
  return fixtureOf<WorkOrderCard>("card", name);
}

async function answerCards(page: Page, answer: CardAnswer): Promise<void> {
  await page.evaluate((a) => (window.__officinaCardAnswer = a), answer);
}

async function answerLaunch(page: Page, answer: LaunchAnswer): Promise<void> {
  await page.evaluate((a) => (window.__officinaLaunchAnswer = a), answer);
}

async function callsOf(page: Page, cmd: string): Promise<unknown[]> {
  return (await calls(page)).filter((call) => call.cmd === cmd).map((call) => call.args);
}

/** Opens the day's view and composes `issued`'s template, project and
 *  values in the composer; the backend answers with `issued`. */
async function composeCard(page: Page, issued: WorkOrderCard): Promise<void> {
  await openOrchestrator(page, orchestrator("orchestrator-day"));
  await answerCards(page, { outcome: "issued", value: issued });
  await page.getByTestId("composer-template").selectOption(issued.template);
  await page.getByTestId("composer-project").selectOption(issued.project);
  for (const param of issued.values) {
    await page.locator(`[data-testid="composer-param"][data-param="${param.name}"]`).selectOption(param.value);
  }
  await expect(page.getByTestId("work-order-card")).toHaveAttribute("data-card", issued.id);
}

/** The history row the launch of `launched` recorded, taken from the
 *  day's row that `pick` finds: the same row under the card's id. */
function rowFor(launched: WorkOrderCard, pick: (order: WorkOrderView) => boolean): WorkOrderView {
  const row = orchestrator("orchestrator-day").work_orders.find(pick)!;
  return { ...row, id: launched.id, name: launched.name };
}

async function pushRow(page: Page, row: WorkOrderView): Promise<void> {
  await page.evaluate(async (order) => {
    const view = window.__officinaOrchestrator!;
    await window.__officinaPushOrchestrator?.({ ...view, work_orders: [order, ...view.work_orders] });
  }, row);
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
  await expect(button).toHaveAccessibleName(/^Orchestrator: absent/);
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

async function answerStart(page: Page, answer: StartAnswer): Promise<void> {
  await page.evaluate((a) => (window.__officinaStartAnswer = a), answer);
}

// Story 5.16 AC 7 (AD-89, AD-92, D-691): Start sends `start_orchestrator`,
// from a click or the keyboard, since a Start approves nothing; once
// started, the author is told to type any first message.
test("Start sends start_orchestrator", async ({ page }) => {
  await openOrchestrator(page, orchestrator("orchestrator-empty"));
  await expect(page.getByTestId("orchestrator-presence-text")).toHaveText("Absent.");
  const start = page.getByTestId("orchestrator-start");
  await expect(start).toHaveAccessibleName("Start");
  await start.click();
  await expect(page.getByTestId("orchestrator-start-answer")).toContainText("type any first message");
  expect(await callsOf(page, "start_orchestrator")).toHaveLength(1);
  await start.focus();
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await callsOf(page, "start_orchestrator")).length).toBe(2);
});

// Story 5.16 AC 7 (AD-89): a refusal shows its cause beside the button,
// and a spawn that failed shows what it failed with, as text.
test("a refused start shows its cause", async ({ page }) => {
  await openOrchestrator(page, orchestrator("orchestrator-empty"));
  await answerStart(page, { outcome: "refused", detail: "workdir_not_empty" });
  await page.getByTestId("orchestrator-start").click();
  const answer = page.getByTestId("orchestrator-start-answer");
  await expect(answer).toContainText("the orchestrator's folder is not empty");
  await answerStart(page, { outcome: "failed", detail: "<b>access denied</b>" });
  await page.getByTestId("orchestrator-start").click();
  await expect(answer).toHaveText("Could not start: <b>access denied</b>");
  await expect(answer.locator("b")).toHaveCount(0);
});

// Story 5.16 AC 5 and 7 (AD-89, D-565, D-678): the board's live
// orchestrator, from the real CLI's board, shows its status in the view
// and the filled mark on the view switch; Start then reads "Bring to
// front" and says it already runs.
test("a live orchestrator shows its status", async ({ page }) => {
  await openOrchestrator(page, orchestrator("orchestrator-live"));
  await expect(page.getByTestId("orchestrator-presence-text")).toContainText("Live: idle, started");
  await expect(page.getByTestId("orchestrator-presence")).toHaveAttribute("data-presence", "live");
  await expect(page.getByTestId("view-orchestrator")).toHaveAccessibleName("Orchestrator: live");
  const start = page.getByTestId("orchestrator-start");
  await expect(start).toHaveAccessibleName("Bring to front");
  await answerStart(page, { outcome: "already_running", detail: { outcome: "no_window" } });
  await start.click();
  await expect(page.getByTestId("orchestrator-start-answer")).toContainText("Already running");
  expect(await callsOf(page, "start_orchestrator")).toHaveLength(1);
});

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

test("composing sends issue_card with the chosen template, project and values", async ({ page }) => {
  const issued = card("card-bump");
  await composeCard(page, issued);
  const sent = (await callsOf(page, "issue_card")) as { request: { template: string; project: string; values: [string, string][]; subject: unknown } }[];
  // One request, once every choice is made, with nothing but the choices.
  expect(sent).toHaveLength(1);
  const { request } = sent[0]!;
  expect(request.template).toBe(issued.template);
  expect(request.project).toBe(issued.project);
  expect(Object.fromEntries(request.values)).toEqual(Object.fromEntries(issued.values.map((p) => [p.name, p.value])));
  expect(request.subject).toBeNull();
  expect(await callsOf(page, "edit_card")).toEqual([]);
});

test("changing a value sends edit_card and shows the new card", async ({ page }) => {
  const first = card("card-bump");
  const edited = card("card-bump-minor");
  await composeCard(page, first);
  const changed = edited.values.find((p) => p.value !== first.values.find((q) => q.name === p.name)!.value)!;
  await answerCards(page, { outcome: "issued", value: edited });
  await page.locator(`[data-testid="composer-param"][data-param="${changed.name}"]`).selectOption(changed.value);
  await expect(page.getByTestId("work-order-card")).toHaveAttribute("data-card", edited.id);
  await expect(page.getByTestId("card-prompt")).toHaveText(edited.prompt);
  expect(await callsOf(page, "edit_card")).toEqual([
    { cardId: first.id, change: { kind: "value", name: changed.name, value: changed.value } },
  ]);

  // A new subject's name is an edit too; the name shows as text.
  const renamed = card("card-bump-new-subject");
  const name = renamed.subject.kind === "new" ? renamed.subject.value : "";
  await answerCards(page, { outcome: "issued", value: renamed });
  await page.getByTestId("picker-new-name").fill(name);
  await page.getByTestId("picker-create").click();
  await expect(page.getByTestId("work-order-card")).toHaveAttribute("data-card", renamed.id);
  await expect(page.getByTestId("card-subject")).toContainText(name);
  await expect(page.locator('[data-testid="work-order-card"] b')).toHaveCount(0);
  expect((await callsOf(page, "edit_card")).at(-1)).toEqual({
    cardId: edited.id,
    change: { kind: "subject", subject: { kind: "new", value: name } },
  });
});

test("a refusal is shown on the composer", async ({ page }) => {
  const first = card("card-bump");
  await composeCard(page, first);
  // The catalogue changed since: the edit is refused, the card is kept and
  // the select goes back to its value.
  const param = first.values[0]!;
  const other = param.options.find((o) => o !== param.value)!;
  await answerCards(page, { outcome: "refused", value: { kind: "template", detail: "catalogue_changed" } });
  const select = page.locator(`[data-testid="composer-param"][data-param="${param.name}"]`);
  await select.selectOption(other);
  await expect(page.getByTestId("composer-refusal")).toContainText("the catalogue changed since its approval");
  await expect(page.getByTestId("work-order-card")).toHaveAttribute("data-card", first.id);
  await expect(select).toHaveValue(param.value);
  // A card the backend no longer knows leaves the screen with its refusal.
  await answerCards(page, { outcome: "refused", value: { kind: "unknown_card" } });
  await select.selectOption(other);
  await expect(page.getByTestId("composer-refusal")).toContainText("no longer knows this card");
  await expect(page.getByTestId("work-order-card")).toHaveCount(0);
});

test("the card shows the prompt, the argument chips, the folder and the subject", async ({ page }) => {
  const issued = card("card-bump");
  await composeCard(page, issued);
  const shown = page.getByTestId("work-order-card");
  await expect(shown.getByTestId("card-template")).toHaveText(issued.template);
  await expect(shown.getByTestId("card-description")).toHaveText(issued.description);
  await expect(shown.getByTestId("card-project")).toHaveText(issued.project);
  await expect(shown.getByTestId("card-prompt")).toHaveText(issued.prompt);
  await expect(shown.getByTestId("card-arg")).toHaveText(issued.argv);
  await expect(shown.getByTestId("card-value")).toHaveCount(issued.values.length);
  await expect(shown.getByTestId("card-cwd")).toContainText(issued.cwd);
  const subject = issued.subject.kind === "existing" ? issued.subject.value : -1;
  await expect(shown.getByTestId("card-subject")).toContainText(issued.subject_options.find((s) => s.id === subject)!.name);
  await expect(shown.locator('[data-testid^="gauge-"]')).toHaveCount(2);
  await expect(shown.getByTestId("gauge-5h")).toContainText(`${Math.round(issued.quota!.five_hour!.used_percentage)} %`);
  // The card's subject is the one the picker shows chosen.
  await expect(page.locator(`[data-testid="pick-subject"][data-subject="${subject}"]`)).toHaveAttribute("aria-pressed", "true");
});

// D-163, review F22: computed colours in the real browser, against the
// value of `--color-attention` resolved on the document root.
for (const theme of ["light", "dark"] as const) {
  test(`the card and its chips never take the attention colour (${theme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    await composeCard(page, card("card-bump"));
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
      const shown = document.querySelector('[data-testid="work-order-card"]')!;
      const colours = new Set<string>();
      for (const element of [shown, ...Array.from(shown.querySelectorAll("*"))]) {
        const style = getComputedStyle(element);
        for (const colour of [
          style.color,
          style.backgroundColor,
          style.borderTopColor,
          style.borderRightColor,
          style.borderBottomColor,
          style.borderLeftColor,
        ]) {
          colours.add(colour);
        }
      }
      const chip = getComputedStyle(shown.querySelector('[data-testid="card-arg"]')!);
      const own = getComputedStyle(shown);
      return {
        attention: resolve("--color-attention"),
        attentionBorder: resolve("--color-attention-border"),
        border: resolve("--color-border"),
        bg: resolve("--color-bg"),
        surface: resolve("--color-surface"),
        colours: [...colours],
        chips: shown.querySelectorAll('[data-testid="card-arg"], [data-testid="card-value"]').length,
        chip: { background: chip.backgroundColor, border: chip.borderTopColor },
        card: { background: own.backgroundColor, border: own.borderTopColor },
      };
    });
    expect(seen.chips).toBeGreaterThan(0);
    expect(seen.colours).not.toContain(seen.attention);
    expect(seen.colours).not.toContain(seen.attentionBorder);
    expect(seen.card).toEqual({ background: seen.surface, border: seen.border });
    expect(seen.chip).toEqual({ background: seen.bg, border: seen.border });
  });
}

test("Launch sends the card id and the prompt hash", async ({ page }) => {
  const issued = card("card-bump");
  await composeCard(page, issued);
  await page.evaluate(() => (window.__officinaLaunchHeld = true));
  const launch = page.getByTestId("launch");
  await launch.click();
  // In flight: the card says so and the button is disabled.
  await expect(page.getByTestId("card-launching")).toBeVisible();
  await expect(launch).toBeDisabled();
  await expect(launch).toHaveAccessibleName("Launching");
  await page.evaluate(() => window.__officinaReleaseLaunch?.());
  await expect(page.getByTestId("card-outcome")).toHaveAttribute("data-outcome", "started");
  await expect(launch).toBeDisabled();
  await expect(launch).toHaveAccessibleName("Launch");
  expect(await callsOf(page, "launch_work_order")).toEqual([{ cardId: issued.id, promptSha256: issued.prompt_sha256 }]);
});

test("a keyboard press never launches", async ({ page }) => {
  await composeCard(page, card("card-bump"));
  const launch = page.getByTestId("launch");
  await launch.focus();
  await expect(launch).toBeFocused();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Space");
  await expect(launch).toBeEnabled();
  await expect(page.getByTestId("card-outcome")).toHaveCount(0);
  expect(await callsOf(page, "launch_work_order")).toEqual([]);
});

// Story 5.24 (D-980, D-986): the permission line says where the mode
// comes from; its tooltip shows on hover and on keyboard focus, laid over
// the card so nothing moves; Officina's default is chosen on a pointer
// click only, going back takes the keyboard too.
const OPACITY = (locator: import("@playwright/test").Locator) => locator.evaluate((e) => getComputedStyle(e).opacity);
/** The label a stable-label control shows, not the ones it keeps hidden
 *  for its size. */
const SHOWN = (locator: import("@playwright/test").Locator) => locator.locator("[data-stable-label] > :not([aria-hidden])");

for (const theme of ["light", "dark"] as const) {
  test(`the permission tooltip shows on hover and moves nothing (${theme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const issued = card("card-bump");
    await composeCard(page, issued);
    await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
    const line = page.getByTestId("card-permission");
    await expect(line).toHaveAttribute("data-permission", "project");
    await expect(SHOWN(page.getByTestId("card-permission-text"))).toHaveText("the project's own mode");
    const tip = page.getByTestId("card-permission-tip");
    expect(await OPACITY(tip)).toBe("0");
    // In the page's own coordinates: hovering may scroll the line into view.
    const boxes = () =>
      Promise.all(
        ["work-order-card", "card-subject", "launch"].map((id) =>
          page.getByTestId(id).evaluate((e) => {
            const r = e.getBoundingClientRect();
            return { x: r.x + window.scrollX, y: r.y + window.scrollY, width: r.width, height: r.height };
          }),
        ),
      );
    const before = await boxes();
    await page.getByTestId("card-permission-text").hover();
    await expect.poll(() => OPACITY(tip)).toBe("1");
    const button = page.getByTestId("card-permission-switch");
    await expect(button).toHaveAccessibleName("Use Officina's default for this project");
    await expect(SHOWN(page.getByTestId("card-permission-tip-text"))).toContainText("auto mode");
    expect(await boxes()).toEqual(before);
    const colours = await tip.evaluate((element) => {
      const probe = document.createElement("span");
      probe.style.color = "var(--color-attention)";
      document.body.append(probe);
      const attention = getComputedStyle(probe).color;
      probe.remove();
      const seen = [element, ...Array.from(element.querySelectorAll("*"))].flatMap((e) => {
        const style = getComputedStyle(e);
        return [style.color, style.backgroundColor, style.borderTopColor];
      });
      return { attention, seen, surface: getComputedStyle(element).backgroundColor, body: getComputedStyle(document.body).backgroundColor };
    });
    expect(colours.seen).not.toContain(colours.attention);
    expect(colours.surface).not.toBe("rgba(0, 0, 0, 0)");
    expect(await callsOf(page, "set_work_order_permission")).toEqual([]);
  });
}

test("the permission switch is reached with Tab, which shows its tooltip, and no key widens", async ({ page }) => {
  await composeCard(page, card("card-bump"));
  const button = page.getByTestId("card-permission-switch");
  await page.getByTestId("card-cwd").click();
  let reached = false;
  for (let i = 0; i < 40 && !reached; i += 1) {
    await page.keyboard.press("Tab");
    reached = await button.evaluate((e) => e === document.activeElement);
  }
  expect(reached, "Tab reaches the switch").toBe(true);
  await expect.poll(() => OPACITY(page.getByTestId("card-permission-tip"))).toBe("1");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Space");
  await expect(page.getByTestId("card-permission")).toHaveAttribute("data-permission", "project");
  expect(await callsOf(page, "set_work_order_permission")).toEqual([]);
});

test("a click switches the project to Officina's default; going back takes the keyboard", async ({ page }) => {
  const issued = card("card-bump");
  await composeCard(page, issued);
  await page.getByTestId("card-permission-text").hover();
  await page.getByTestId("card-permission-switch").click();
  const line = page.getByTestId("card-permission");
  await expect(line).toHaveAttribute("data-permission", "officina");
  await expect(SHOWN(page.getByTestId("card-permission-text"))).toHaveText("Officina's default, auto mode");
  const args = await page.getByTestId("card-arg").allTextContents();
  const separator = args.indexOf("--");
  expect(args.slice(separator - 2, separator)).toEqual(["--permission-mode", "auto"]);
  expect(await callsOf(page, "set_work_order_permission")).toEqual([{ cardId: issued.id, permission: "officina" }]);
  const back = page.getByTestId("card-permission-switch");
  await expect(back).toHaveAccessibleName("Back to the project's mode");
  await back.focus();
  await page.keyboard.press("Enter");
  await expect(line).toHaveAttribute("data-permission", "project");
  expect((await callsOf(page, "set_work_order_permission")).at(-1)).toMatchObject({ permission: "project" });
  expect(await page.getByTestId("card-arg").allTextContents()).not.toContain("--permission-mode");
});

test("the card core issued for Officina's default carries auto mode before the prompt", async ({ page }) => {
  const switched = card("card-bump-officina");
  await composeCard(page, switched);
  await expect(page.getByTestId("card-permission")).toHaveAttribute("data-permission", "officina");
  const args = await page.getByTestId("card-arg").allTextContents();
  expect(args).toEqual(switched.argv);
  expect(args.join(" ")).toContain("--permission-mode auto --");
});

test("a started launch shows its session once bound", async ({ page }) => {
  const issued = card("card-bump");
  await composeCard(page, issued);
  await answerLaunch(page, { outcome: "launched", value: "started" });
  await page.getByTestId("launch").click();
  await expect(page.getByTestId("card-session")).toContainText("Waiting for its background session");
  const bound = rowFor(issued, (order) => order.session !== null && order.session.phase !== "unknown");
  await pushRow(page, bound);
  await expect(page.getByTestId("card-session")).toContainText(bound.session!.name ?? bound.session!.session_id);
  await expect(page.getByTestId("card-session")).toContainText("working");
});

test("workspace not trusted is explained, not retried", async ({ page }) => {
  const issued = card("card-bump");
  await composeCard(page, issued);
  await answerLaunch(page, { outcome: "launched", value: "untrusted_workspace" });
  const launch = page.getByTestId("launch");
  await launch.click();
  const outcome = page.getByTestId("card-outcome");
  await expect(outcome).toHaveAttribute("data-outcome", "untrusted_workspace");
  await expect(outcome).toContainText("trust dialog");
  await expect(outcome).toContainText(issued.cwd);
  await expect(launch).toBeDisabled();
  await launch.click({ force: true });
  expect(await callsOf(page, "launch_work_order")).toHaveLength(1);
});

test("a failed launch shows its exit code and error as text", async ({ page }) => {
  const issued = card("card-bump");
  await composeCard(page, issued);
  await answerLaunch(page, { outcome: "launched", value: "failed" });
  await page.getByTestId("launch").click();
  const outcome = page.getByTestId("card-outcome");
  await expect(outcome).toHaveAttribute("data-outcome", "failed");
  const failed = rowFor(issued, (order) => order.outcome === "failed");
  await pushRow(page, failed);
  await expect(outcome).toContainText(`Exit code ${failed.exit_code}`);
  await expect(page.getByTestId("card-error")).toHaveText(failed.error!);
  await expect(page.locator('[data-testid="work-order-card"] img')).toHaveCount(0);
  expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
});

// Story 5.17 (AD-91, AD-93, FR-12, D-152, D-564): the orchestrator's
// proposals, ingested by the day's home's own writer, each under a
// "proposal" label with its status and, when invalid, its reason as worded
// by `PROPOSAL_INVALID_TEXT`; every text the orchestrator's, shown as text;
// a valid work order proposal opens as the card core issued from it.

/** The day's proposal pending as a work order that suggests a new subject. */
function suggesting(view: OrchestratorView): string {
  return view.proposals.find(
    (p) => p.status === "pending" && p.content !== null && "work_order" in p.content && "new" in p.content.work_order.subject,
  )!.id;
}

for (const theme of ["light", "dark"] as const) {
  test(`proposals are listed with their status and reason (${theme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const view = orchestrator("orchestrator-day");
    await openOrchestrator(page, view);
    const list = page.getByTestId("proposals");
    const open = view.proposals.filter((p) => p.status === "pending" || p.status === "invalid");
    const decided = view.proposals.length - open.length;
    await expect(list.locator('ul.list:not([data-testid="proposals-decided"]) > [data-testid="proposal"]')).toHaveCount(open.length);
    await expect(page.getByTestId("proposals-decided").getByTestId("proposal")).toHaveCount(decided);
    for (const proposal of view.proposals) {
      const item = list.locator(`[data-testid="proposal"][data-id="${proposal.id}"]`);
      await expect(item).toHaveAttribute("data-status", proposal.status);
      await expect(item.getByTestId("proposal-label")).toHaveText("proposal");
      await expect(item.getByTestId("proposal-status")).toHaveText(PROPOSAL_STATUS_TEXT[proposal.status]);
      if (proposal.invalid_reason !== null) {
        await expect(item.getByTestId("proposal-reason")).toContainText(invalidReasonText(proposal.invalid_reason));
      }
      // Open as card for a valid work order only; Dismiss while undecided.
      await expect(item.getByTestId("proposal-open")).toHaveCount(
        proposal.status === "pending" && proposal.kind === "work_order" ? 1 : 0,
      );
      await expect(item.getByTestId("proposal-dismiss")).toHaveCount(
        proposal.status === "pending" || proposal.status === "invalid" ? 1 : 0,
      );
    }
    await expect(list.locator("img")).toHaveCount(0);
    expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
    // Neutral: no colour of the list is the attention colour (D-564, D-565).
    const seen = await list.evaluate((shown) => {
      const probe = document.createElement("span");
      probe.style.color = "var(--color-attention)";
      document.body.append(probe);
      const attention = getComputedStyle(probe).color;
      probe.remove();
      const colours = new Set<string>();
      for (const element of [shown, ...Array.from(shown.querySelectorAll("*"))]) {
        const style = getComputedStyle(element);
        for (const colour of [style.color, style.backgroundColor, style.borderTopColor]) colours.add(colour);
      }
      return { attention, colours: [...colours] };
    });
    expect(seen.colours).not.toContain(seen.attention);
  });
}

test("a valid proposal opens as a card with its rationale under the label", async ({ page }) => {
  const view = orchestrator("orchestrator-day");
  const issued = fixtureOf<WorkOrderCard>("card", "card-proposal");
  await openOrchestrator(page, view);
  await answerCards(page, { outcome: "issued", value: issued });
  const id = suggesting(view);
  await page.locator(`[data-testid="proposal"][data-id="${id}"]`).getByTestId("proposal-open").click();
  expect(await callsOf(page, "issue_card_from_proposal")).toEqual([{ proposalId: id }]);
  const shown = page.getByTestId("work-order-card");
  await expect(shown).toHaveAttribute("data-card", issued.id);
  await expect(shown.getByTestId("card-proposal")).toHaveAttribute("data-proposal", issued.proposal!.id);
  await expect(shown.getByTestId("card-proposal-label")).toHaveText("proposal");
  await expect(shown.getByTestId("card-rationale")).toHaveText(issued.proposal!.rationale);
  await expect(shown.locator("img")).toHaveCount(0);
  // The card launches as the author's: on a click, its id and hash.
  await page.getByTestId("launch").click();
  expect(await callsOf(page, "launch_work_order")).toEqual([{ cardId: issued.id, promptSha256: issued.prompt_sha256 }]);
});

test("dismiss sends dismiss_proposal", async ({ page }) => {
  const view = orchestrator("orchestrator-day");
  await openOrchestrator(page, view);
  const id = suggesting(view);
  const item = page.locator(`[data-testid="proposal"][data-id="${id}"]`);
  await item.getByTestId("proposal-dismiss").click();
  expect(await callsOf(page, "dismiss_proposal")).toEqual([{ proposalId: id }]);
  // The next view shows it decided: it leaves the open list.
  await expect(item).toHaveAttribute("data-status", "dismissed");
  await expect(page.getByTestId("proposals-decided").locator(`[data-id="${id}"]`)).toHaveCount(1);
  await expect(item.getByTestId("proposal-dismiss")).toHaveCount(0);
});

test("a suggested subject name is shown, never preselected", async ({ page }) => {
  const view = orchestrator("orchestrator-day");
  const issued = fixtureOf<WorkOrderCard>("card", "card-proposal");
  const suggested = issued.proposal!.suggested_subject!;
  await openOrchestrator(page, view);
  await answerCards(page, { outcome: "issued", value: issued });
  await page.locator(`[data-testid="proposal"][data-id="${suggesting(view)}"]`).getByTestId("proposal-open").click();
  await expect(page.getByTestId("composer-suggested-subject")).toContainText(suggested);
  await expect(page.getByTestId("card-suggested-subject")).toContainText(suggested);
  // The card files under a new subject named after the template.
  expect(issued.subject).toEqual({ kind: "new", value: issued.template });
  await expect(page.getByTestId("card-subject")).toContainText(issued.template);
  await expect(page.getByTestId("card-subject")).not.toContainText(suggested);
  // Nothing in the picker carries it: no subject chosen, the name field empty.
  await expect(page.getByTestId("picker-new-name")).toHaveValue("");
  for (const option of await page.getByTestId("pick-subject").all()) {
    await expect(option).not.toContainText(suggested);
  }
  await expect(page.locator('[data-testid="composer"] img')).toHaveCount(0);
});

// Story 5.18 AC 9 (D-565, D-833): the view switch's Orchestrator button
// carries presence as a shape and, while proposals are pending, their
// count; both in its accessible name and tooltip.
const pendingOf = (view: OrchestratorView) => view.proposals.filter((p) => p.status === "pending").length;

test("the view switch shows presence and the proposal count", async ({ page }) => {
  const day = orchestrator("orchestrator-day");
  const pending = pendingOf(day);
  expect(pending).toBeGreaterThan(1);
  await answering(page, day);
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  const button = page.getByTestId("view-orchestrator");
  await expect(button).toHaveAccessibleName(`Orchestrator: absent, ${pending} proposals pending`);
  await expect(button).toHaveAttribute("title", `Orchestrator: absent, ${pending} proposals pending`);
  await expect(page.getByTestId("orchestrator-pending")).toHaveText(String(pending));
  // Live, from the board: the filled mark; none pending: no count.
  const board = { ...loadBoard("busy"), orchestrator: sceneBoardOrchestrator() };
  await page.evaluate((b) => window.__officinaPushBoard!(b), board);
  await expect(page.getByTestId("orchestrator-presence")).toHaveAttribute("data-presence", "live");
  await expect(button).toHaveAccessibleName(`Orchestrator: live, ${pending} proposals pending`);
  await page.evaluate((v) => window.__officinaPushOrchestrator!(v), { ...day, proposals: [] });
  await expect(page.getByTestId("orchestrator-pending")).toHaveCount(0);
  await expect(button).toHaveAccessibleName("Orchestrator: live");
  await page.evaluate((v) => window.__officinaPushOrchestrator!(v), withPending(day, 1));
  await expect(button).toHaveAccessibleName("Orchestrator: live, 1 proposal pending");
  // The same element in Scene mode.
  await page.getByTestId("mode-scene").click();
  await expect(button).toHaveAccessibleName("Orchestrator: live, 1 proposal pending");
  await expect(page.getByTestId("orchestrator-pending")).toHaveText("1");
});

test("the view switch's mark is not colour alone", async ({ page }) => {
  await answering(page, orchestrator("orchestrator-day"));
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  const mark = page.getByTestId("orchestrator-presence");
  const shape = () =>
    mark.evaluate((m) => {
      const s = getComputedStyle(m);
      return { border: s.borderTopColor, borderWidth: s.borderTopWidth, fill: s.backgroundColor };
    });
  const absent = await shape();
  const absentName = await page.getByTestId("view-orchestrator").getAttribute("aria-label");
  await page.evaluate((b) => window.__officinaPushBoard!(b), { ...loadBoard("busy"), orchestrator: sceneBoardOrchestrator() });
  await expect(mark).toHaveAttribute("data-presence", "live");
  const live = await shape();
  // The same colour, a hollow ring then a filled dot, and different words.
  expect(live.border).toBe(absent.border);
  expect(absent.fill).toBe("rgba(0, 0, 0, 0)");
  expect(live.fill).toBe(live.border);
  expect(Number.parseFloat(absent.borderWidth)).toBeGreaterThan(0);
  expect(await page.getByTestId("view-orchestrator").getAttribute("aria-label")).not.toBe(absentName);
});

// Review F40: proposals and the orchestrator reach neither the title nor,
// through `waiting_count`, the taskbar overlay (core and app tests hold
// the Rust side: `board.rs` and `taskbar.rs`).
test("proposals leave the title unchanged", async ({ page }) => {
  await answering(page, orchestrator("orchestrator-empty"));
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  const title = await page.title();
  const waiting = await page.getByTestId("waiting-pill").textContent();
  await page.evaluate((v) => window.__officinaPushOrchestrator!(v), withPending(orchestrator("orchestrator-day"), PENDING_LIMIT));
  await page.evaluate((b) => window.__officinaPushBoard!(b), { ...loadBoard("busy"), orchestrator: { ...sceneBoardOrchestrator(), status: "waiting" as const } });
  await expect(page.getByTestId("orchestrator-pending")).toHaveText(String(PENDING_LIMIT));
  expect(await page.title()).toBe(title);
  await expect(page.getByTestId("waiting-pill")).toHaveText(waiting!);
});

// D-163, D-564, review F22: computed colours in the real browser against
// `--color-attention` resolved on the document root, in both themes.
for (const theme of ["light", "dark"] as const) {
  test(`the view switch, the proposal list and the decision office never take the attention colour (${theme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    // Nobody waits: the decision office's own queue would be amber, and
    // rightly (AD-167); what is checked is the orchestrator's.
    const board = structuredClone(loadBoard("scene-orchestrator"));
    for (const card of board.columns.needs_you.splice(0)) {
      board.columns.working.push({ ...card, column: "working", waiting_for: null, scene: { ...card.scene!, zone: "office", alert: null, approval: false } });
    }
    board.waiting_count = 0;
    await answering(page, sceneView());
    await openScene(page, board);
    await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
    const colours = (selector: string) =>
      page.evaluate((sel) => {
        const probe = document.createElement("span");
        probe.style.color = "var(--color-attention)";
        document.body.append(probe);
        const attention = getComputedStyle(probe).color;
        probe.remove();
        const seen = new Set<string>();
        for (const root of Array.from(document.querySelectorAll(sel))) {
          for (const element of [root, ...Array.from(root.querySelectorAll("*"))]) {
            const s = getComputedStyle(element);
            for (const value of [s.color, s.backgroundColor, s.borderTopColor, s.fill, s.stroke, s.outlineColor]) seen.add(value);
          }
        }
        return { attention, seen: [...seen], count: document.querySelectorAll(sel).length };
      }, selector);
    const surfaces = [
      '[data-testid="view-switch"]',
      '[data-plaque="decision"]',
      "[data-display-screen]",
      "[data-whiteboard]",
      "[data-orchestrator]",
      '[data-partition="decision"]',
    ];
    for (const selector of surfaces) {
      const { attention, seen, count } = await colours(selector);
      expect(count, selector).toBeGreaterThan(0);
      expect(seen, selector).not.toContain(attention);
    }
    await page.getByTestId("view-orchestrator").click();
    await expect(page.getByTestId("proposals")).toBeVisible();
    for (const selector of ['[data-testid="proposals"]', '[data-testid="view-switch"]']) {
      const { attention, seen } = await colours(selector);
      expect(seen, selector).not.toContain(attention);
    }
  });
}


// Story 5.21 (AD-182): an answer proposal, the meeting fixture's as the
// window ingested it, lists its session's name and opens its meeting; it
// has no Open as card, and only the meeting's click sends it.
test("an answer proposal opens its meeting from the list", async ({ page }) => {
  const meeting = fixtureOf<MeetingView>("meeting", "meeting-background");
  const proposal = meeting.proposed_answer;
  if (!proposal?.content || !("answer" in proposal.content)) throw new Error("the meeting fixture has a proposed answer");
  const session = proposal.content.answer.session_id;
  const day = orchestrator("orchestrator-day");
  const board = loadBoard("busy");
  const named = board.columns.needs_you[0]!;
  const withSession = {
    ...board,
    columns: { ...board.columns, needs_you: [{ ...named, session_id: session }, ...board.columns.needs_you.slice(1)] },
  };
  const key = `sid:${session}`;
  await page.addInitScript(
    ([view, m]) => {
      window.__officinaOrchestrator = view;
      window.__officinaMeetings = { [m.key]: m };
    },
    [{ ...day, proposals: [proposal, ...day.proposals] }, { ...meeting, key }] as [OrchestratorView, MeetingView],
  );
  await openBoard(page, withSession, {}, loadRelay("relay-clean"));
  await page.getByTestId("view-orchestrator").click();
  const item = page.locator(`[data-testid="proposal"][data-id="${proposal.id}"]`);
  await expect(item).toHaveAttribute("data-kind", "answer");
  await expect(item.getByTestId("proposal-session")).toContainText(named.name);
  await expect(item.getByTestId("proposal-open")).toHaveCount(0);
  await expect(item.locator("img")).toHaveCount(0);
  await item.getByTestId("proposal-open-meeting").click();
  expect(await callsOf(page, "open_meeting")).toEqual([{ key }]);
  await expect(page.getByTestId("meeting-panel")).toHaveAttribute("data-key", key);
  await expect(page.getByTestId("meeting-proposal")).toHaveAttribute("data-id", proposal.id);
  expect(await callsOf(page, "send_answer")).toEqual([]);
});
