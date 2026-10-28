// Story 4.15: the author's verdicts on ended sessions. The boards are the
// real CLI's after the window's writer recorded them (`settle-before`:
// a subject archived, another archived then reopened; `settle-after`: the
// ten-second hook test discarded and one session marked done, from
// `scripts/e2e-fixtures.mjs`). A verdict goes to the backend; the card
// moves only with the next board, which a test pushes after checking the
// call, as the collector would publish it.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { calls, loadBoard, loadRegistry, openBoard } from "./fixtures";

const sid = (n: number): string => `00000000-0000-0000-0000-${String(n).padStart(12, "0")}`;
const cardOf = (page: Page, n: number): Locator => page.locator(`[data-testid="card"][data-key="sid:${sid(n)}"]`);
const inColumn = (page: Page, column: string, n: number): Locator =>
  page.getByTestId(`column-${column}`).locator(`[data-testid="card"][data-key="sid:${sid(n)}"]`);

async function push(page: Page, scenario: "settle-before-board" | "settle-after-board"): Promise<void> {
  const board = loadBoard(scenario);
  await page.evaluate((b) => window.__officinaPushBoard?.(b), board);
}

/** The settle and reopen calls the window made, in order. */
async function verdicts(page: Page): Promise<unknown[]> {
  return (await calls(page))
    .filter((c) => c.cmd === "settle_session" || c.cmd === "reopen_subject")
    .map((c) => ({ cmd: c.cmd, ...(c.args as object) }));
}

/** Drags `card` by the mouse to the middle of `to`, through a few moves. */
async function dragTo(page: Page, card: Locator, to: Locator, release = true): Promise<void> {
  const from = (await card.locator(".name").boundingBox())!;
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + from.width / 2 + 20, from.y + from.height / 2 + 10, { steps: 3 });
  await expect(to).toBeVisible();
  const box = (await to.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + Math.min(box.height / 2, 120), { steps: 8 });
  if (release) await page.mouse.up();
}

// The owner's item 7: archiving a subject in the Subjects view left its
// ended session in To resume.
test("an archived subject's ended session leaves To resume for Done", async ({ page }) => {
  await openBoard(page, loadBoard("settle-before-board"));
  await expect(inColumn(page, "to_resume", 202)).toHaveCount(0);
  await expect(inColumn(page, "done", 202)).toBeVisible();
  await expect(inColumn(page, "done", 202).getByTestId("settled")).toHaveText("subject archived");
  // Archived then reopened: back in To resume.
  await expect(inColumn(page, "to_resume", 205)).toBeVisible();
});

test("dragging a card to Done marks it done, and Undo takes it back", async ({ page }) => {
  await openBoard(page, loadBoard("settle-before-board"));
  await dragTo(page, cardOf(page, 203), page.getByTestId("column-done"));
  await expect.poll(() => verdicts(page)).toEqual([{ cmd: "settle_session", session: sid(203), settlement: "done" }]);
  await push(page, "settle-after-board");
  await expect(inColumn(page, "done", 203).getByTestId("settled")).toHaveText("marked done");
  await expect(page.getByTestId("settle-note")).toContainText("Marked web-portal done.");
  // A drag leaves the focus where it was: the note does not take it.
  await expect(page.getByTestId("settle-undo")).not.toBeFocused();
  await page.getByTestId("settle-undo").click();
  await expect.poll(() => verdicts(page)).toEqual([
    { cmd: "settle_session", session: sid(203), settlement: "done" },
    { cmd: "settle_session", session: sid(203), settlement: null },
  ]);
  await expect(page.getByTestId("settle-note")).toHaveCount(0);
  await push(page, "settle-before-board");
  await expect(inColumn(page, "to_resume", 203)).toBeVisible();
});

test("dragging a card to the discard target discards it; Escape cancels a drag", async ({ page }) => {
  await openBoard(page, loadBoard("settle-before-board"));
  await expect(page.getByTestId("discard-target")).toHaveCount(0);
  // Cancelled: nothing recorded, no session focused, nothing left behind.
  await dragTo(page, cardOf(page, 201), page.getByTestId("column-done"), false);
  await expect(page.getByTestId("column-done")).toHaveClass(/drop-over/);
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(page.getByTestId("discard-target")).toHaveCount(0);
  expect(await verdicts(page)).toEqual([]);
  expect((await calls(page)).filter((c) => c.cmd === "focus")).toEqual([]);
  // Dropped on the discard target, which shows only while a card is dragged.
  await dragTo(page, cardOf(page, 201), page.getByTestId("discard-target"));
  await expect.poll(() => verdicts(page)).toEqual([{ cmd: "settle_session", session: sid(201), settlement: "discarded" }]);
  await expect(page.getByTestId("discard-target")).toHaveCount(0);
  await push(page, "settle-after-board");
  await expect(cardOf(page, 201)).toHaveCount(0);
  await expect(page.getByTestId("settle-note")).toContainText("Discarded billing-api.");
});

test("a live card has no menu and does not drag", async ({ page }) => {
  await openBoard(page, loadBoard("busy"));
  const live = page.getByTestId("column-working").getByTestId("card").first();
  await expect(live.getByTestId("card-menu")).toHaveCount(0);
  await dragTo(page, live, page.getByTestId("column-done"));
  await expect(page.getByTestId("discard-target")).toHaveCount(0);
  expect(await verdicts(page)).toEqual([]);
});

test("the card menu does it from the keyboard, the focus on Undo, then back on the card", async ({ page }) => {
  await openBoard(page, loadBoard("settle-before-board"));
  const menu = cardOf(page, 201).getByTestId("card-menu");
  await expect(menu).toHaveAccessibleName("Actions for billing-api");
  await menu.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("card-menu-done")).toBeFocused();
  await expect(cardOf(page, 201).getByRole("menuitem")).toHaveText(["Mark done", "Discard"]);
  // Escape closes it, the focus back on its button.
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("card-menu-list")).toHaveCount(0);
  await expect(menu).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expect(page.getByTestId("card-menu-discard")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("card-menu-done")).toBeFocused();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect.poll(() => verdicts(page)).toEqual([{ cmd: "settle_session", session: sid(201), settlement: "discarded" }]);
  await push(page, "settle-after-board");
  await expect(page.getByTestId("settle-undo")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect.poll(() => verdicts(page)).toHaveLength(2);
  expect((await verdicts(page))[1]).toEqual({ cmd: "settle_session", session: sid(201), settlement: null });
  await push(page, "settle-before-board");
  await expect(cardOf(page, 201).locator("button.main")).toBeFocused();
});

test("a card done by its archived subject goes back by reopening the subject", async ({ page }) => {
  const board = loadBoard("settle-before-board");
  await openBoard(page, board);
  const card = inColumn(page, "done", 202);
  await card.getByTestId("card-menu").click();
  await expect(card.getByRole("menuitem")).toHaveText(["Back to To resume", "Discard"]);
  await card.getByTestId("card-menu-to_resume").click();
  const subject = board.columns.done.find((c) => c.session_id === sid(202))!.subject!.id;
  await expect.poll(() => verdicts(page)).toEqual([{ cmd: "reopen_subject", subject }]);
  await expect(page.getByTestId("settle-note")).toContainText("Archive it again from Subjects");
  await expect(page.getByTestId("settle-undo")).toHaveCount(0);
  await expect(page.getByTestId("settle-dismiss")).toBeFocused();
});

test("a refused verdict says why and moves nothing", async ({ page }) => {
  await openBoard(page, loadBoard("settle-before-board"));
  await page.evaluate(() => (window.__officinaRefuse = { settle_session: "unknown_session" }));
  await cardOf(page, 204).getByTestId("card-menu").click();
  await page.getByTestId("card-menu-done").click();
  await expect(page.getByTestId("settle-note")).toContainText("Not recorded:");
  await expect(inColumn(page, "to_resume", 204)).toBeVisible();
});

test("a discarded session is listed with the archived subjects and restored from there", async ({ page }) => {
  await openBoard(page, loadBoard("settle-after-board"), {}, null, loadRegistry("settle-after"));
  await page.evaluate(
    (id) =>
      (window.__officinaDiscarded = [
        { session_id: id, project: "billing-api", name: null, prompt: "try the hook", discarded_at_ms: Date.now() },
      ]),
    sid(201),
  );
  await page.getByTestId("view-subjects").click();
  await page.getByTestId("show-archived").click();
  const listed = page.getByTestId("discarded-session");
  await expect(listed).toHaveCount(1);
  await expect(listed).toContainText("try the hook");
  await listed.getByTestId("restore-session").click();
  await expect.poll(() => verdicts(page)).toEqual([{ cmd: "settle_session", session: sid(201), settlement: null }]);
});
