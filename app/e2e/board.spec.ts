import { expect, test } from "@playwright/test";
import { calls, loadBoard, openBoard } from "./fixtures";

test.describe("busy morning", () => {
  test("every column shows the cards the backend classified, in order", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    for (const [column, cards] of Object.entries(board.columns)) {
      await expect(page.getByTestId(`count-${column}`)).toHaveText(String(cards.length));
    }
    const needsYou = page.getByTestId("column-needs_you").getByTestId("card");
    await expect(needsYou).toHaveCount(board.columns.needs_you.length);
    for (const [i, card] of board.columns.needs_you.entries()) {
      await expect(needsYou.nth(i)).toHaveAttribute("data-key", card.key);
    }
  });

  test("the waiting pill and window counts match the board", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    await expect(page.getByTestId("waiting-pill")).toHaveText(`${board.waiting_count} waiting`);
  });

  test("cards show waiting reason, excerpts and badges as plain text", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    const blocked = page.locator(`[data-key="${board.columns.needs_you[1]!.key}"]`);
    await expect(blocked.getByTestId("waiting-for")).toHaveText("permission prompt");
    await expect(blocked.getByTestId("last-prompt")).toContainText("migration dry-run");
    const yourTurn = page.getByTestId("column-your_turn");
    await expect(yourTurn.getByTestId("last-message").first()).toContainText("Ready for review");
    const toResume = page.getByTestId("column-to_resume");
    await expect(toResume.getByTestId("badge")).toHaveText(["interrupted", "ended", "interrupted"]);
    await expect(page.getByTestId("column-working").getByTestId("badge")).toHaveText(["unknown"]);
  });

  test("done collapses beyond five cards and expands on demand", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    const done = page.getByTestId("column-done");
    await expect(done.getByTestId("card")).toHaveCount(5);
    await done.getByRole("button", { name: `Show ${board.columns.done.length - 5} more` }).click();
    await expect(done.getByTestId("card")).toHaveCount(board.columns.done.length);
    await done.getByRole("button", { name: "Show less" }).click();
    await expect(done.getByTestId("card")).toHaveCount(5);
  });

  test("quota gauges show value, reset time, week share and freshness", async ({ page }) => {
    await openBoard(page, loadBoard("busy"));
    const five = page.getByTestId("gauge-5h");
    await expect(five).toContainText("62 %");
    await expect(five).toContainText("resets at");
    await expect(five).toContainText("updated just now");
    const week = page.getByTestId("gauge-week");
    await expect(week).toContainText("41 %");
    await expect(week).toMatchAriaSnapshot(`- meter "Week"`);
    await expect(week).toContainText("% of the week elapsed");
  });

  test("quota turns stale after 30 minutes of silence", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    await page.clock.fastForward("31:00");
    await expect(page.getByTestId("gauge-5h")).toContainText("stale, updated 31 min ago");
  });
});

test.describe("focus", () => {
  test("clicking a card asks the backend and shows the outcome", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    const card = board.columns.your_turn[0]!;
    await page.locator(`[data-key="${card.key}"] button.main`).click();
    await expect(page.locator(`[data-key="${card.key}"]`).getByTestId("focus-outcome")).toContainText(
      "Window brought to front",
    );
    expect(await calls(page)).toContainEqual({ cmd: "focus", args: { key: card.key } });
  });

  test("ambiguous windows are listed, never guessed", async ({ page }) => {
    const board = loadBoard("busy");
    const card = board.columns.your_turn[1]!;
    await openBoard(page, board, {
      [card.key]: { outcome: "candidates", detail: ["web-portal - Code", "web-portal - Code [2]"] },
    });
    await page.locator(`[data-key="${card.key}"] button.main`).click();
    const outcome = page.locator(`[data-key="${card.key}"]`).getByTestId("focus-outcome");
    await expect(outcome).toContainText("Several windows could match");
    await expect(outcome.locator("li")).toHaveText(["web-portal - Code", "web-portal - Code [2]"]);
  });

  test("wrong window is reported and the outcome disappears after ten seconds", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    const key = board.columns.needs_you[1]!.key;
    const card = page.locator(`[data-key="${key}"]`);
    await card.locator("button.main").click();
    await card.getByRole("button", { name: "Wrong window?" }).click();
    expect(await calls(page)).toContainEqual({ cmd: "report_wrong_window", args: { key } });
    await expect(card.getByTestId("focus-outcome")).toHaveCount(0);

    await card.locator("button.main").click();
    await expect(card.getByTestId("focus-outcome")).toBeVisible();
    await page.clock.fastForward(10_500);
    await expect(card.getByTestId("focus-outcome")).toHaveCount(0);
  });

  test("failures are explained on the card", async ({ page }) => {
    const board = loadBoard("busy");
    const key = board.columns.needs_you[0]!.key;
    await openBoard(page, board, { [key]: { outcome: "failed", detail: "session folder not found" } });
    await page.locator(`[data-key="${key}"] button.main`).click();
    await expect(page.locator(`[data-key="${key}"]`).getByTestId("focus-outcome")).toContainText(
      "Could not open: session folder not found",
    );
  });

  test("keys 1-9 focus the matching Needs you card, r refreshes", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    await page.keyboard.press("2");
    await page.keyboard.press("r");
    const made = await calls(page);
    expect(made).toContainEqual({ cmd: "focus", args: { key: board.columns.needs_you[1]!.key } });
    expect(made.some((c) => c.cmd === "refresh")).toBe(true);
    await page.keyboard.press("9");
    expect((await calls(page)).filter((c) => c.cmd === "focus")).toHaveLength(1);
  });

  test("a held key opens a session once, not once per auto-repeat", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    for (let i = 0; i < 5; i++) await page.keyboard.press("1");
    expect((await calls(page)).filter((c) => c.cmd === "focus")).toHaveLength(1);
  });

  test("background cards copy their attach command", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const board = loadBoard("busy");
    await openBoard(page, board);
    const bg = board.columns.working.find((c) => c.attach_available)!;
    await page.locator(`[data-key="${bg.key}"]`).getByRole("button", { name: "Copy attach command" }).click();
    await expect(page.locator(`[data-key="${bg.key}"]`).getByRole("button", { name: "Copied" })).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(`claude attach ${bg.background_id}`);
  });
});

test.describe("degraded states", () => {
  test("a stale board keeps its cards and says why", async ({ page }) => {
    await openBoard(page, loadBoard("stale"));
    await expect(page.getByTestId("stale-banner")).toContainText("claude exited with 1: fake failure");
    await expect(page.getByTestId("stale-banner")).toContainText("Showing the last known state");
    await expect(page.getByTestId("stale-banner")).toContainText(/not refreshed since \d{2}:\d{2}/);
  });

  test("an empty workstation shows empty columns and no quota data", async ({ page }) => {
    await openBoard(page, loadBoard("empty"));
    await expect(page.getByTestId("waiting-pill")).toHaveCount(0);
    await expect(page.getByText("Nothing here.")).toHaveCount(5);
    await expect(page.getByTestId("gauge-5h")).toContainText("no data: see officina doctor");
  });

  test("a passed reset is shown as such, never as a stale percentage", async ({ page }) => {
    await openBoard(page, loadBoard("quota-reset"));
    await expect(page.getByTestId("gauge-5h")).toContainText("reset, no new data");
    await expect(page.getByTestId("gauge-5h")).toContainText("--");
    await expect(page.getByTestId("gauge-week")).toContainText("41 %");
  });

  test("diagnostics and a broken contract are surfaced", async ({ page }) => {
    const board = loadBoard("busy");
    board.diagnostics.corrupt_event_lines = 3;
    board.diagnostics.floor_error = "officina.toml: unknown field `colour`";
    board.contract = { claude_version: "9.9.9", held: false, missing: ["sessionId"] };
    await openBoard(page, board);
    await expect(page.getByTestId("diagnostics")).toContainText("3 unreadable event lines");
    await expect(page.getByTestId("floor-error")).toContainText("unknown field `colour`");
    await expect(page.getByTestId("contract")).toHaveText("contract broken: sessionId");
  });

  test("an unavailable registry is reported and the board still shows sessions", async ({ page }) => {
    const board = loadBoard("registry-unavailable");
    expect(board.diagnostics.registry?.state).toBe("unavailable");
    await openBoard(page, board);
    const banner = page.getByTestId("registry-banner");
    await expect(banner).toContainText("The registry is unavailable");
    await expect(banner).toContainText(board.diagnostics.registry!.detail!);
    await expect(banner).toContainText("The board keeps working");
    for (const [column, cards] of Object.entries(board.columns)) {
      await expect(page.getByTestId(`count-${column}`)).toHaveText(String(cards.length));
    }
    expect(board.columns.needs_you.length).toBeGreaterThan(0);
  });

  test("a ready or absent registry shows no banner", async ({ page }) => {
    const board = loadBoard("busy");
    expect(board.diagnostics.registry?.state).toBe("absent");
    await openBoard(page, board);
    await expect(page.getByTestId("registry-banner")).toHaveCount(0);
    board.diagnostics.registry = { state: "ready", detail: null, exclude_from_sync: null };
    await page.evaluate((b) => window.__officinaPushBoard!(b), board);
    await expect(page.getByTestId("registry-banner")).toHaveCount(0);
  });

  test("before the first board the window says it is asking", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("loading")).toBeVisible();
  });
});

test.describe("live updates and safety", () => {
  test("a pushed board replaces the current one", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board);
    const next = loadBoard("busy");
    next.columns.needs_you = [];
    next.waiting_count = 0;
    await page.evaluate((b) => window.__officinaPushBoard!(b), next);
    await expect(page.getByTestId("waiting-pill")).toHaveCount(0);
    await expect(page.getByTestId("count-needs_you")).toHaveText("0");
  });

  test("session text is rendered as text, never as HTML", async ({ page }) => {
    const board = loadBoard("busy");
    const payload = `<img src=x onerror="window.__pwned=1"><b>bold</b>`;
    board.columns.your_turn[0]!.last_prompt = payload;
    board.columns.your_turn[0]!.last_message = payload;
    board.columns.your_turn[0]!.name = payload;
    await openBoard(page, board);
    const card = page.locator(`[data-key="${board.columns.your_turn[0]!.key}"]`);
    await expect(card.getByTestId("last-prompt")).toHaveText(payload);
    await expect(card.locator("img")).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned)).toBeUndefined();
  });
});

test.describe("subjects on the board", () => {
  // Story 3.5 AC 8: a card names the subject its session is filed under,
  // from the registry the real `officina board --json` read.
  test("cards show their subject", async ({ page }) => {
    const board = loadBoard("subjects-board");
    const filed = Object.values(board.columns)
      .flat()
      .filter((card) => card.subject !== null);
    expect(filed.length).toBeGreaterThan(0);
    await openBoard(page, board);
    for (const card of filed) {
      await expect(page.locator(`[data-key="${card.key}"]`).getByTestId("card-subject")).toHaveText(card.subject!.name);
    }
    const unfiled = Object.values(board.columns)
      .flat()
      .filter((card) => card.subject === null);
    for (const card of unfiled) {
      await expect(page.locator(`[data-key="${card.key}"]`).getByTestId("card-subject")).toHaveCount(0);
    }
  });
});
