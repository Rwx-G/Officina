// Story 2.8: approval requests in Needs you (AD-35, D-27, D-28, D-170), on a
// board printed by the real `officina board --json` after real
// `officina run --wait 1` requests and one forged request file (D-25).
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, openBoard } from "./fixtures";

const board = loadBoard("approvals");

async function decisions(page: Page): Promise<unknown[]> {
  return (await calls(page)).filter((c) => c.cmd === "decide_approval").map((c) => c.args);
}

test.describe("approval requests", () => {
  test("approval cards come first in Needs you, oldest first", async ({ page }) => {
    await openBoard(page, board);
    const column = page.getByTestId("column-needs_you");
    const cards = column.locator('[data-testid="approval-card"], [data-testid="card"]');
    const ids = board.approvals.map((a) => a.id);
    const sessions = board.columns.needs_you.map((c) => c.key);
    await expect(cards).toHaveCount(ids.length + sessions.length);
    for (const [i, id] of ids.entries()) {
      await expect(cards.nth(i)).toHaveAttribute("data-id", id);
    }
    for (const [i, key] of sessions.entries()) {
      await expect(cards.nth(ids.length + i)).toHaveAttribute("data-key", key);
    }
    const times = board.approvals.map((a) => a.requested_at_ms);
    expect([...times].sort((a, b) => a - b)).toEqual(times);

    for (const approval of board.approvals.filter((a) => a.valid)) {
      const card = column.locator(`[data-id="${approval.id}"]`);
      await expect(card.getByTestId("arg")).toHaveText(approval.argv);
      await expect(card.getByTestId("approval-session")).toHaveText(approval.session_name ?? "session unknown");
    }
    await expect(page.getByTestId("count-needs_you")).toHaveText(String(ids.length + sessions.length));
  });

  test("a pointer click approves the request and it leaves Needs you", async ({ page }) => {
    await openBoard(page, board);
    const approval = board.approvals.find((a) => a.valid)!;
    const card = page.locator(`[data-id="${approval.id}"]`);
    await card.getByTestId("approve").click();
    expect(await decisions(page)).toEqual([{ id: approval.id, approve: true }]);
    await expect(card).toBeHidden();
  });

  test("an invalid request has no Approve button", async ({ page }) => {
    await openBoard(page, board);
    const invalid = board.approvals.find((a) => !a.valid)!;
    const card = page.locator(`[data-id="${invalid.id}"]`);
    await expect(card.getByTestId("invalid-reason")).toContainText(invalid.invalid_reason!);
    await expect(card.getByTestId("approve")).toHaveCount(0);
    await expect(card.getByTestId("reject")).toHaveCount(0);
    await expect(card.getByTestId("arg")).toHaveCount(0);
  });

  test("number keys never approve", async ({ page }) => {
    await openBoard(page, board);
    for (const key of ["1", "2", "3", "4", "5", "6", "7", "8", "9"]) {
      await page.keyboard.press(key);
    }
    expect(await decisions(page)).toEqual([]);
    const focused = (await calls(page)).filter((c) => c.cmd === "focus").map((c) => (c.args as { key: string }).key);
    expect(focused).toEqual(board.columns.needs_you.map((c) => c.key));
  });

  test("Enter and Space on a focused approval card do not approve", async ({ page }) => {
    await openBoard(page, board);
    const approval = board.approvals.find((a) => a.valid)!;
    const card = page.locator(`[data-id="${approval.id}"]`);
    for (const button of [card.getByTestId("approve"), card.getByTestId("reject")]) {
      await button.focus();
      await expect(button).toBeFocused();
      await page.keyboard.press("Enter");
      await page.keyboard.press("Space");
    }
    await card.getByTestId("approval-action").click();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Space");
    expect(await decisions(page)).toEqual([]);
    await expect(card).toBeVisible();
  });

  test("waiting pill counts approvals", async ({ page }) => {
    await openBoard(page, board);
    const valid = board.approvals.filter((a) => a.valid).length;
    expect(board.waiting_count).toBe(board.columns.needs_you.length + valid);
    await expect(page.getByTestId("waiting-pill")).toHaveText(`${board.waiting_count} waiting`);
  });

  test("an approval with a dry run shows its output", async ({ page }) => {
    await openBoard(page, board);
    const approval = board.approvals.find((a) => a.dry_run?.output)!;
    const dryRun = page.locator(`[data-id="${approval.id}"]`).getByTestId("dry-run");
    await expect(dryRun).toContainText(`Dry run: ${approval.dry_run!.check}`);
    await expect(dryRun.getByTestId("dry-run-output")).toHaveText(approval.dry_run!.output!);
  });
});
