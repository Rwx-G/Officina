// The alert band (story 4.5; FR-10, AD-175, D-473, D-494, D-496): who it
// names and why, in every view, how it counts and leaves, the time
// escalation from two boards the real CLI built either side of the
// threshold, and no theme change. Boards are `officina board --json`
// output (AD-17).
import { expect, test, type Page } from "@playwright/test";
import { loadBoard, loadRelay, openBoard } from "./fixtures";
import type { Board } from "../src/lib/generated/Board";

const band = (page: Page) => page.getByTestId("alert-band");
const bandText = (page: Page) => page.getByTestId("alert-band-text");

function push(page: Page, board: Board): Promise<void> {
  return page.evaluate((b) => window.__officinaPushBoard!(b), board);
}

/** The board's alerted cards, in board order. */
function alerted(board: Board) {
  const { needs_you, your_turn, working } = board.columns;
  return [...needs_you, ...your_turn, ...working].filter((c) => c.scene?.alert);
}

/** Every `--color-*` custom property resolved on `:root`. */
function colourTokens(page: Page): Promise<Record<string, string>> {
  return page.evaluate(() => {
    const style = getComputedStyle(document.documentElement);
    const out: Record<string, string> = {};
    for (const sheet of Array.from(document.styleSheets)) {
      for (const rule of Array.from(sheet.cssRules)) {
        if (!(rule instanceof CSSStyleRule)) continue;
        for (const name of Array.from(rule.style)) {
          if (name.startsWith("--color-")) out[name] = style.getPropertyValue(name).trim();
        }
      }
    }
    return out;
  });
}

test.describe("alert band", () => {
  test("the band names the first alerted session, its reason and cause", async ({ page }) => {
    const board = loadBoard("scene-alert");
    await openBoard(page, board);
    const first = alerted(board)[0]!;
    await expect(band(page)).toBeVisible();
    await expect(band(page)).toHaveAttribute("role", "status");
    await expect(bandText(page)).toContainText(`${first.name} (${first.project.name}) is blocked:`);
    await expect(bandText(page)).toContainText(first.waiting_for ?? "");
    await expect(bandText(page)).toContainText(first.scene!.alert === "priority" ? "high-priority project" : "waiting");
  });

  test("the band shows in every view and mode", async ({ page }) => {
    await openBoard(page, loadBoard("scene-alert"), {}, loadRelay("relay-clean"));
    for (const view of ["subjects", "relay", "board"]) {
      await page.getByTestId(`view-${view}`).click();
      await expect(band(page)).toBeVisible();
    }
  });

  test("the band counts the other alerted sessions", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const others = alerted(board).length - 1;
    expect(others).toBeGreaterThan(0);
    await openBoard(page, board);
    await expect(bandText(page)).toContainText(`, and ${others} more.`);
  });

  test("the band leaves with the last alert", async ({ page }) => {
    await openBoard(page, loadBoard("scene-alert"));
    await expect(band(page)).toBeVisible();
    await push(page, loadBoard("scene-c"));
    await expect(band(page)).toHaveCount(0);
  });

  test("an alert changes no theme token", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"));
    const calm = await colourTokens(page);
    expect(Object.keys(calm).length).toBeGreaterThan(5);
    await push(page, loadBoard("scene-alert"));
    await expect(band(page)).toBeVisible();
    expect(await colourTokens(page)).toEqual(calm);
  });

  // Two boards the real CLI built, the timed wait one minute short of the
  // floor's threshold, then past it: the band follows from the board alone.
  test("a wait that reaches the threshold raises the band", async ({ page }) => {
    const early = loadBoard("scene-alert-early");
    const late = loadBoard("scene-alert");
    const timed = late.columns.needs_you.find((c) => c.scene?.alert === "waited")!;
    // The same session in both boards: its session id (the fixture script
    // numbers each scenario's processes on).
    const before = early.columns.needs_you.find((c) => c.session_id === timed.session_id);
    expect(before?.scene?.zone).toBe("decision");
    expect(before?.scene?.alert).toBeNull();
    await openBoard(page, early);
    await expect(bandText(page)).not.toContainText("and 1 more");
    await push(page, late);
    await expect(bandText(page)).toContainText("and 1 more");
  });

  test("an approval-only wait on a normal project raises no band", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const approvalOnly = board.columns.working.find((c) => c.scene?.approval)!;
    expect(approvalOnly.scene!.alert).toBeNull();
    expect(approvalOnly.project.priority).toBe("normal");
    expect(approvalOnly.scene!.zone).toBe("decision");
    await openBoard(page, board);
    await expect(bandText(page)).not.toContainText(approvalOnly.name);
    // Alone, it raises no band at all; core's vectors cover a request older
    // than the threshold (tests/vectors/scene.json).
    const alone: Board = {
      ...board,
      alert_count: 0,
      columns: { ...board.columns, needs_you: [] },
    };
    await push(page, alone);
    await expect(band(page)).toHaveCount(0);
  });
});
