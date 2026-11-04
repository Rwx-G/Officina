// The workshop card and the board's workshop cards (story 6.19 AC 1, 2 and
// 6; AD-122, FR-6, FR-17, D-163): the card shows what the run will and
// will not hold and issues a new card when its base changes; each run sits
// in the column its state places it in, from the real `board --json` of
// story 6.18's fixture; Receive opens the review panel; no workshop chip
// wears the attention colour, in either theme.
import { expect, test, type Page } from "@playwright/test";
import { fixtureOf } from "./fixture-index";
import { calls, loadBoard, loadRelay, openBoard, transitionsDone } from "./fixtures";
import { review, workshopCard } from "./workshop-fixtures";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";
import type { CardAnswer } from "../src/lib/generated/CardAnswer";
import type { OrchestratorView } from "../src/lib/generated/OrchestratorView";
import type { ReviewView } from "../src/lib/generated/ReviewView";
import type { WorkOrderCard } from "../src/lib/generated/WorkOrderCard";

const runsBoard = (): Board => fixtureOf<Board>("workshop", "workshop-runs-board");
const workshopCards = (board: Board): Card[] =>
  Object.values(board.columns)
    .flat()
    .filter((c: Card) => c.kind === "workshop");
const cardOf = (page: Page, key: string) => page.locator(`[data-testid="card"][data-key=${JSON.stringify(key)}]`);

async function answerCards(page: Page, answer: CardAnswer): Promise<void> {
  await page.evaluate((a) => (window.__officinaCardAnswer = a), answer);
}

async function serveReviews(page: Page, reviews: Record<string, ReviewView>): Promise<void> {
  await page.evaluate((r) => (window.__officinaReviews = r), reviews);
}

/** Composes `issued` in the day's Orchestrator view; the backend answers
 *  it. */
async function composeCard(page: Page, issued: WorkOrderCard): Promise<void> {
  await page.addInitScript((view) => {
    window.__officinaOrchestrator = view;
  }, fixtureOf<OrchestratorView>("orchestrator", "orchestrator-day"));
  await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  await page.getByTestId("view-orchestrator").click();
  await expect(page.getByTestId("orchestrator-view")).toBeVisible();
  await transitionsDone(page);
  await answerCards(page, { outcome: "issued", value: issued });
  await page.getByTestId("composer-template").selectOption(issued.template);
  await page.getByTestId("composer-project").selectOption(issued.project);
  for (const param of issued.values) {
    await page.locator(`[data-testid="composer-param"][data-param="${param.name}"]`).selectOption(param.value);
  }
  await expect(page.getByTestId("work-order-card")).toHaveAttribute("data-card", issued.id);
}

const bump = (): WorkOrderCard => fixtureOf<WorkOrderCard>("card", "card-bump");

test("a workshop card shows its facts", async ({ page }) => {
  const issued = workshopCard(bump());
  const facts = issued.workshop!;
  await composeCard(page, issued);
  const card = page.getByTestId("work-order-card");
  await expect(card.getByTestId("workshop-chip")).toHaveText("workshop");
  await expect(card.getByTestId("workshop-base-option")).toHaveCount(facts.bases.length);
  await expect(card.getByTestId("workshop-uncommitted")).toContainText("not carried");
  await expect(card.getByTestId("workshop-allow")).toContainText(facts.allow[0]!.host);
  await expect(card.getByTestId("workshop-mount")).toContainText("read-only");
  await expect(card.getByTestId("workshop-image")).toContainText(facts.image.short_id!);
  await expect(card.getByTestId("workshop-limits")).toContainText(`${facts.max_minutes} minutes`);
  await expect(card.getByTestId("launch")).toBeEnabled();
});

test("changing the base issues a new card", async ({ page }) => {
  const first = workshopCard(bump());
  await composeCard(page, first);
  const release = first.workshop!.bases[1]!.choice;
  const second: WorkOrderCard = {
    ...workshopCard(bump(), { base: release, readiness: [{ refusal: "base_moved", detail: null }] }),
    id: `${first.id.slice(0, 31)}9`,
  };
  await answerCards(page, { outcome: "issued", value: second });
  await page.getByTestId("workshop-base").selectOption("1");
  await expect(page.getByTestId("work-order-card")).toHaveAttribute("data-card", second.id);
  await expect(page.getByTestId("launch")).toBeDisabled();
  const edits = (await calls(page)).filter((c) => c.cmd === "edit_card").map((c) => c.args);
  expect(edits).toEqual([{ cardId: first.id, change: { kind: "base", base: release } }]);
});

test("each run state sits in its column", async ({ page }) => {
  const board = runsBoard();
  const cards = workshopCards(board);
  expect(cards.length).toBeGreaterThan(0);
  await openBoard(page, board);
  for (const card of cards) {
    const shown = page.getByTestId(`column-${card.column}`).locator(`[data-testid="card"][data-key=${JSON.stringify(card.key)}]`);
    await expect(shown, card.run_state ?? "").toHaveAttribute("data-run-state", card.run_state!);
    await expect(shown.getByTestId("isolated-chip")).toHaveCount(card.column === "working" ? 1 : 0);
  }
  const noChange = cards.find((c) => c.run_state === "no_change")!;
  await expect(cardOf(page, noChange.key).getByTestId("badge")).toHaveText("no change");
});

test("receive opens the review panel", async ({ page }) => {
  const board = runsBoard();
  const inReview = workshopCards(board).find((c) => c.run_state === "review")!;
  const runId = inReview.key.slice("ws:".length);
  await openBoard(page, board);
  await serveReviews(page, { [runId]: { ...review(), run_id: runId } });
  await cardOf(page, inReview.key).getByTestId("receive").click();
  const panel = page.getByTestId("review-panel");
  await expect(panel).toHaveAttribute("data-run", runId);
  await expect(panel.getByTestId("review-report")).toHaveText(review().report_tail!);
  expect((await calls(page)).filter((c) => c.cmd === "open_review").map((c) => c.args)).toEqual([{ runId }]);
  expect(Math.round((await panel.boundingBox())!.width)).toBe(640);
  await panel.getByTestId("review-close").click();
  await expect(panel).toHaveCount(0);
  await expect(cardOf(page, inReview.key).getByTestId("receive")).toBeFocused();
});

for (const theme of ["light", "dark"] as const) {
  test(`chips never use the attention colour in the ${theme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const board = runsBoard();
    await openBoard(page, board);
    await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
    await expect(page.getByTestId("isolated-chip").first()).toBeVisible();
    const found = await page.evaluate(() => {
      const probe = document.createElement("span");
      probe.style.color = "var(--color-attention)";
      document.body.append(probe);
      const attention = getComputedStyle(probe).color;
      probe.remove();
      const chips = Array.from(
        document.querySelectorAll('[data-testid="isolated-chip"], [data-kind="workshop"] [data-testid="badge"]'),
      );
      return {
        attention,
        count: chips.length,
        colours: chips.flatMap((c) => [getComputedStyle(c).color, getComputedStyle(c).backgroundColor]),
      };
    });
    expect(found.count).toBeGreaterThan(1);
    expect(found.colours).not.toContain(found.attention);
  });

  test(`the card's workshop chip never uses the attention colour in the ${theme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    await composeCard(page, workshopCard(bump()));
    await expect(page.getByTestId("theme-switch")).toHaveAttribute("data-theme-resolved", theme);
    const chip = await page.getByTestId("work-order-card").getByTestId("workshop-chip").evaluate((c) => {
      const probe = document.createElement("span");
      probe.style.color = "var(--color-attention)";
      document.body.append(probe);
      const attention = getComputedStyle(probe).color;
      probe.remove();
      return { attention, colours: [getComputedStyle(c).color, getComputedStyle(c).backgroundColor] };
    });
    expect(chip.colours).not.toContain(chip.attention);
  });
}
