// The review panel (story 6.19 AC 3 and 4; AD-122, FR-10 to FR-12, D-245):
// Receive on a workshop card of the real `board --json` of story 6.18's
// fixture opens the run's review, whose views are typed literals (D-1236).
// A package is approved then imported by pointer clicks, a stale approval
// shows its refusal, a run with no change offers nothing, and worker text
// stays text: markup is not parsed and a seeded right-to-left override
// does not reorder the line the author reads.
import { expect, test, type Page } from "@playwright/test";
import { fixtureOf } from "./fixture-index";
import { calls, openBoard } from "./fixtures";
import { approvedReview, noChangeReview, review } from "./workshop-fixtures";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";
import type { ReviewView } from "../src/lib/generated/ReviewView";
import type { WorkshopRunState } from "../src/lib/generated/WorkshopRunState";

const panel = (page: Page) => page.getByTestId("review-panel");

/** Opens the board of the workshop fixture with `view` as the review of
 *  the run in `state`, and Receive on its card. */
async function receive(page: Page, state: WorkshopRunState, view: ReviewView): Promise<string> {
  const board = fixtureOf<Board>("workshop", "workshop-runs-board");
  const card = Object.values(board.columns)
    .flat()
    .find((c: Card) => c.kind === "workshop" && c.run_state === state)!;
  const runId = card.key.slice("ws:".length);
  await openBoard(page, board);
  await page.evaluate((r) => (window.__officinaReviews = r), { [runId]: { ...view, run_id: runId } });
  await page.locator(`[data-testid="card"][data-key=${JSON.stringify(card.key)}]`).getByTestId("receive").click();
  await expect(panel(page)).toHaveAttribute("data-run", runId);
  return runId;
}

const argsOf = async (page: Page, cmd: string) => (await calls(page)).filter((c) => c.cmd === cmd).map((c) => c.args);

test("a package is approved then imported", async ({ page }) => {
  const runId = await receive(page, "review", review());
  await expect(panel(page).getByTestId("review-import")).toBeDisabled();
  // The backend's next reading shows the group approved.
  await page.evaluate((r) => (window.__officinaReviews = r), { [runId]: { ...approvedReview(), run_id: runId } });
  await panel(page).getByTestId("approve-group").click();
  await expect(panel(page).getByTestId("sensitive-group")).toHaveAttribute("data-approved", "true");
  expect(await argsOf(page, "approve_sensitive")).toEqual([{ runId, groupKey: "ci", hunksSha256: review().groups[0]!.hunks_sha256 }]);
  const importButton = panel(page).getByTestId("review-import");
  await expect(importButton).toBeEnabled();
  // A key on the focused button imports nothing (D-28).
  await importButton.focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Space");
  expect(await page.evaluate(() => window.__officinaImportCalls ?? 0)).toBe(0);
  await importButton.click();
  await expect(panel(page).getByTestId("review-imported")).toContainText(`officina/${runId}`);
  expect(await argsOf(page, "import_package")).toEqual([{ runId, tip: review().tip }]);
});

test("a stale approval shows the refusal", async ({ page }) => {
  await receive(page, "review", review());
  await page.evaluate(() => (window.__officinaApprovalAnswer = { outcome: "refused", detail: "hunks_changed" }));
  await panel(page).getByTestId("approve-group").click();
  await expect(panel(page).getByTestId("approve-refused")).toContainText("hunks changed");
  await expect(panel(page).getByTestId("review-import")).toBeDisabled();
});

test("a no change run shows nothing to review", async ({ page }) => {
  const view = noChangeReview();
  await receive(page, "no_change", view);
  await expect(panel(page).getByTestId("review-nothing")).toContainText(
    `Nothing to review: the work branch has no commit past main @ ${view.base_commit.slice(0, 12)}`,
  );
  await expect(panel(page).getByTestId("review-report")).toHaveText(view.report_tail!);
  await expect(panel(page).getByTestId("review-import")).toHaveCount(0);
  await expect(panel(page).getByTestId("review-discard")).toHaveCount(0);
  await expect(panel(page).locator("button")).toHaveCount(1);
});

test("markup in a report stays text", async ({ page }) => {
  const markup = `<img src=x onerror="window.__hit = 1"><b>bold</b>`;
  await receive(page, "review", review({ report_tail: markup }));
  await expect(panel(page).getByTestId("review-report")).toHaveText(markup);
  await expect(panel(page).locator("img, b")).toHaveCount(0);
  expect(await page.evaluate(() => (window as { __hit?: number }).__hit)).toBeUndefined();
});

test("a seeded RLO does not reorder the rendered line", async ({ page }) => {
  const raw = "approve abc‮fed.exe";
  await receive(page, "review", review({ report_tail: raw }));
  const report = panel(page).getByTestId("review-report");
  await expect(report).toHaveText("approve abc<U+202E>fed.exe");
  // Each character's box, in text order, lies right of the one before: the
  // glyphs read in the order of the raw text, the override shown, not obeyed.
  const lefts = await report.evaluate((pre) => {
    const node = pre.firstChild!;
    const text = node.textContent ?? "";
    const out: number[] = [];
    for (let i = 0; i < text.length; i++) {
      const range = document.createRange();
      range.setStart(node, i);
      range.setEnd(node, i + 1);
      out.push(range.getBoundingClientRect().left);
    }
    return out;
  });
  for (let i = 1; i < lefts.length; i++) expect(lefts[i]!, `character ${i}`).toBeGreaterThan(lefts[i - 1]!);
});
