// Story 4.15: the author's verdicts on ended sessions, as the real CLI
// prints the board after the window's writer recorded them
// (`settle-before` and `settle-after` in `scripts/e2e-fixtures.mjs`).
import { expect, test } from "@playwright/test";
import { loadBoard, openBoard } from "./fixtures";

const sid = (n: number): string => `00000000-0000-0000-0000-${String(n).padStart(12, "0")}`;
const card = (n: number): string => `[data-testid="card"][data-key="sid:${sid(n)}"]`;

// The owner's item 7: archiving a subject in the Subjects view left its
// ended session in To resume.
test("an archived subject's ended session leaves To resume for Done", async ({ page }) => {
  await openBoard(page, loadBoard("settle-before-board"));
  await expect(page.getByTestId("column-to_resume").locator(card(202))).toHaveCount(0);
  await expect(page.getByTestId("column-done").locator(card(202))).toBeVisible();
  // Archived then reopened: back in To resume.
  await expect(page.getByTestId("column-to_resume").locator(card(205))).toBeVisible();
});
