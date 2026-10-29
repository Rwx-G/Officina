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

// Story 5.12 AC 6 (AD-35 as amended, AD-86, D-28, D-564): an `auto` request
// beside a `human` one, both made by the real `officina run`.
test.describe("auto requests", () => {
  const autoBoard = loadBoard("approvals-auto");
  const auto = autoBoard.approvals.find((a) => a.class === "auto")!;
  const autoCard = (page: Page) => page.locator(`[data-id="${auto.id}"]`);

  test("an auto request shows its class and its check", async ({ page }) => {
    await openBoard(page, autoBoard);
    expect(auto.dry_run?.check).toBeTruthy();
    await expect(autoCard(page).getByTestId("approval-class")).toHaveText(
      `class auto, after its check ${auto.dry_run!.check}`,
    );
    const human = page.locator(`[data-id="${autoBoard.approvals.find((a) => a.class === "human")!.id}"]`);
    await expect(human.getByTestId("approval-class")).toHaveText("class human");
    await expect(human.getByTestId("reject-reasons")).toHaveCount(0);
  });

  test("rejecting an auto request asks should not run or not now", async ({ page }) => {
    await openBoard(page, autoBoard);
    const card = autoCard(page);
    const reasons = card.getByTestId("reject-reasons");
    await expect(card.getByTestId("reason-should_not_run")).toBeHidden();
    const approveBefore = await card.getByTestId("approve").boundingBox();
    const cardBefore = await card.boundingBox();
    await card.getByTestId("reject").click();
    expect(await decisions(page)).toEqual([]);
    await expect(reasons).toHaveClass(/shown/);
    await expect(card.getByTestId("reason-should_not_run")).toBeVisible();
    await expect(card.getByTestId("reason-not_now")).toBeVisible();
    expect(await card.getByTestId("approve").boundingBox()).toEqual(approveBefore);
    expect(await card.boundingBox()).toEqual(cardBefore);
    await card.getByTestId("reason-should_not_run").click();
    expect(await decisions(page)).toEqual([{ id: auto.id, approve: false, reason: "should_not_run" }]);
    await expect(card).toBeHidden();
  });

  test("the reason buttons ignore the keyboard", async ({ page }) => {
    await openBoard(page, autoBoard);
    const card = autoCard(page);
    await card.getByTestId("reject").focus();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Space");
    await expect(card.getByTestId("reject-reasons")).not.toHaveClass(/shown/);
    await card.getByTestId("reject").click();
    for (const reason of ["should_not_run", "not_now"]) {
      const button = card.getByTestId(`reason-${reason}`);
      await button.focus();
      await expect(button).toBeFocused();
      await page.keyboard.press("Enter");
      await page.keyboard.press("Space");
    }
    expect(await decisions(page)).toEqual([]);
    await expect(card).toBeVisible();
  });

  // Story 5.13 AC 5 (AD-87, D-143): the card says why the request is the
  // author's, the precheck's cause or auto approval off; a human card says
  // nothing of it.
  test("an auto request's card says why it is yours", async ({ page }) => {
    const withCause = (cause: "no_session_id" | "auto_approval_off") => ({
      ...autoBoard,
      approvals: autoBoard.approvals.map((a) => (a.id === auto.id ? { ...a, escalation: cause } : a)),
    });
    await openBoard(page, withCause("auto_approval_off"));
    const line = autoCard(page).getByTestId("approval-escalation");
    await expect(line).toContainText("auto approval is off");
    await page.evaluate((b) => window.__officinaPushBoard?.(b), withCause("no_session_id"));
    await expect(line).toContainText("names no session");
    const human = autoBoard.approvals.find((a) => a.class === "human")!;
    await expect(page.locator(`[data-id="${human.id}"]`).getByTestId("approval-escalation")).toHaveCount(0);
  });

  // Story 5.14 AC 8 (D-157, D-564): while its live review runs, the card
  // holds no button, says so in the buttons' place and keeps its height; it
  // is not counted as waiting.
  test("a reviewing request shows no buttons", async ({ page }) => {
    await openBoard(page, autoBoard);
    const card = autoCard(page);
    await expect(card.getByTestId("approve")).toBeVisible();
    const before = await card.boundingBox();
    const reviewing = {
      ...autoBoard,
      approvals: autoBoard.approvals.map((a) => (a.id === auto.id ? { ...a, state: "reviewing" as const } : a)),
      waiting_count: autoBoard.waiting_count - 1,
    };
    await page.evaluate((b) => window.__officinaPushBoard?.(b), reviewing);
    await expect(card.getByTestId("approval-reviewing")).toBeVisible();
    await expect(card.getByTestId("approve")).toHaveCount(0);
    await expect(card.getByTestId("reject")).toHaveCount(0);
    await expect(card.getByTestId("approval-escalation")).toHaveCount(0);
    const after = await card.boundingBox();
    expect(Math.abs(after!.height - before!.height)).toBeLessThanOrEqual(0.5);
    await card.focus();
    await page.keyboard.press("Enter");
    expect((await calls(page)).filter((c) => c.cmd === "decide_approval")).toEqual([]);
  });

  for (const theme of ["light", "dark"] as const) {
    test(`the approval card is the same in both themes but for its tokens (${theme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: theme });
      await openBoard(page, autoBoard);
      const card = autoCard(page);
      await card.getByTestId("reject").click();
      const resolved = (token: string) =>
        page.evaluate((name) => {
          const probe = document.createElement("div");
          probe.style.color = `var(${name})`;
          document.body.append(probe);
          const value = getComputedStyle(probe).color;
          probe.remove();
          return value;
        }, token);
      const style = (testId: string, property: "color" | "borderTopColor") =>
        card.getByTestId(testId).evaluate((e, p) => getComputedStyle(e)[p], property);
      expect(await card.evaluate((e) => getComputedStyle(e).borderTopColor)).toBe(await resolved("--color-attention"));
      expect(await style("approval-class", "color")).toBe(await resolved("--color-text-muted"));
      expect(await style("reason-not_now", "borderTopColor")).toBe(await resolved("--color-border"));
      expect(await style("reason-not_now", "color")).toBe(await resolved("--color-text"));
    });
  }
});
