// The Sessions view and its modes (story 4.7 AC 1; AD-66, D-113, D-481):
// the view switch's names, the mode switch, `m`, the stored mode and its
// defaults, and the banners above the sessions area.
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, loadRelay, openBoard, openScene, sceneSettled } from "./fixtures";
import { DEFAULT_WALKING, VIEW_STORAGE_KEY, defaultViewState, readViewState, serialiseViewState } from "../src/lib/view.svelte";
import { FOCUS_REPEAT_GUARD_MS } from "../src/lib/board.svelte";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";

const scene = (page: Page) => page.getByTestId("scene");
const boardColumns = (page: Page) => page.getByTestId("column-needs_you");
const focusList = (page: Page) => page.getByTestId("focus-list");
const focusRows = (page: Page) => page.getByTestId("focus-row");

/** The mode the switch shows as chosen. */
async function chosenMode(page: Page): Promise<string | null> {
  return page.getByTestId("mode-switch").locator('[aria-checked="true"]').getAttribute("data-mode");
}

/** The focus calls the backend received, by card key. */
async function focusCalls(page: Page): Promise<string[]> {
  return (await calls(page)).filter((c) => c.cmd === "focus").map((c) => (c.args as { key: string }).key);
}

/** Opens the window in Focus mode over Board, as a stored choice. */
async function openFocus(page: Page, board: Board): Promise<void> {
  await page.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    [VIEW_STORAGE_KEY, serialiseViewState({ ...defaultViewState(), focus: true })] as const,
  );
  await page.clock.install({ time: board.generated_at_ms });
  await page.addInitScript((b) => (window.__officinaBoard = b), board);
  await page.goto("/");
  await expect(focusList(page)).toBeVisible();
}

/** Opens the window with `raw` stored as the view state. */
async function openStored(page: Page, raw: string): Promise<void> {
  await page.addInitScript(([key, value]) => localStorage.setItem(key, value), [VIEW_STORAGE_KEY, raw] as const);
  await openBoard(page, loadBoard("scene-c"));
}

test.describe("modes", () => {
  test("the header names the Sessions view", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"), {}, loadRelay("relay-clean"));
    await expect(page.getByTestId("view-switch").getByRole("button")).toHaveText(["Sessions", "Subjects", "Relay"]);
    await expect(page.getByTestId("view-sessions")).toHaveAttribute("aria-pressed", "true");
  });

  test("the mode switch offers Board, Scene and Focus", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"));
    const group = page.getByRole("radiogroup", { name: "Mode" });
    await expect(group.getByRole("radio")).toHaveText(["Board", "Scene", "Focus"]);
    await expect(group.getByRole("radio", { name: "Board" })).toHaveAttribute("aria-checked", "true");
    await expect(group.getByRole("radio", { name: "Scene" })).toHaveAttribute("aria-checked", "false");
    await expect(boardColumns(page)).toBeVisible();
    await expect(scene(page)).toHaveCount(0);

    await group.getByRole("radio", { name: "Scene" }).click();
    await sceneSettled(page);
    await expect(boardColumns(page)).toHaveCount(0);
    await expect(group.getByRole("radio", { name: "Scene" })).toHaveAttribute("aria-checked", "true");
    // One tab stop; the arrows move the choice.
    await expect(group.getByRole("radio", { name: "Board" })).toHaveAttribute("tabindex", "-1");
    await group.getByRole("radio", { name: "Scene" }).press("ArrowLeft");
    await expect(boardColumns(page)).toBeVisible();
    await expect(group.getByRole("radio", { name: "Board" })).toBeFocused();
    // Focus, by click; the arrows wrap from it to Board.
    await group.getByRole("radio", { name: "Focus" }).click();
    await expect(focusList(page)).toBeVisible();
    await expect(boardColumns(page)).toHaveCount(0);
    await group.getByRole("radio", { name: "Focus" }).press("ArrowRight");
    await expect(boardColumns(page)).toBeVisible();
    await expect(group.getByRole("radio", { name: "Board" })).toBeFocused();
  });

  test("the mode switch belongs to the Sessions view", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"), {}, loadRelay("relay-clean"));
    const group = page.getByTestId("mode-switch");
    const box = await group.boundingBox();
    for (const view of ["subjects", "relay"]) {
      await page.getByTestId(`view-${view}`).click();
      await expect(group).toBeHidden();
      // Its room is kept, so switching views moves nothing beside it.
      expect(await group.boundingBox()).toEqual(box);
    }
    await page.getByTestId("view-sessions").click();
    await expect(group).toBeVisible();
  });

  test("m switches between Board and Scene and ignores a held key", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"));
    await page.keyboard.press("m");
    await sceneSettled(page);
    await page.keyboard.press("m");
    await expect(boardColumns(page)).toBeVisible();

    // A held key: one press, then auto-repeats.
    await page.keyboard.down("m");
    for (let i = 0; i < 4; i++) await page.evaluate(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "m", repeat: true })));
    await page.keyboard.up("m");
    await sceneSettled(page);

    // Not with a modifier, not in a text field, not outside Sessions.
    await page.keyboard.press("Control+m");
    await page.keyboard.press("Alt+m");
    await expect(scene(page)).toBeVisible();
    await page.evaluate(() => {
      const input = document.createElement("input");
      input.setAttribute("data-testid", "probe-input");
      document.body.append(input);
    });
    await page.getByTestId("probe-input").press("m");
    await expect(scene(page)).toBeVisible();
    await page.getByTestId("view-subjects").click();
    await page.keyboard.press("m");
    await page.getByTestId("view-sessions").click();
    await sceneSettled(page);
  });

  test("keys 1 to 9 act on the nth waiting session in Board, Scene and Focus", async ({ page }) => {
    const board = loadBoard("scene-c");
    const waiting = board.columns.needs_you;
    expect(waiting.length).toBeGreaterThan(1);
    expect(waiting.length).toBeLessThan(9);
    await openBoard(page, board);
    const expected: string[] = [];
    for (const mode of ["board", "scene", "focus"]) {
      await page.getByTestId(`mode-${mode}`).click();
      if (mode === "scene") await sceneSettled(page);
      for (const [n, card] of waiting.entries()) {
        await page.keyboard.press(String(n + 1));
        expected.push(card.key);
      }
      // A digit past the last waiting session does nothing.
      await page.keyboard.press(String(waiting.length + 1));
      await expect.poll(() => focusCalls(page), mode).toEqual(expected);
      // The same key acts again only after the repeat guard.
      await page.clock.runFor(FOCUS_REPEAT_GUARD_MS + 1);
    }
  });

  test("the mode survives a reload", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"));
    await page.getByTestId("mode-scene").click();
    await sceneSettled(page);
    await page.reload();
    await sceneSettled(page);
    await page.getByTestId("mode-board").click();
    await page.reload();
    await expect(boardColumns(page)).toBeVisible();
    await expect(scene(page)).toHaveCount(0);
  });

  test("an unreadable stored view falls back to Board", async ({ page }) => {
    await openStored(page, "{not json");
    await expect(page.getByTestId("mode-board")).toHaveAttribute("aria-checked", "true");
    await page.getByTestId("mode-scene").click();
    await sceneSettled(page);
  });

  test("a stored view with an unknown mode falls back to Board", async ({ page }) => {
    await openStored(page, JSON.stringify({ v: 1, base: "focus" }));
    await expect(page.getByTestId("mode-board")).toHaveAttribute("aria-checked", "true");
  });

  test("a storage that throws keeps Board and still switches", async ({ page }) => {
    await page.addInitScript(() => {
      const deny = () => {
        throw new DOMException("denied", "SecurityError");
      };
      Object.defineProperty(window, "localStorage", { get: deny, configurable: true });
    });
    await openBoard(page, loadBoard("scene-c"));
    await expect(page.getByTestId("mode-board")).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("m");
    await sceneSettled(page);
  });

  // Walking (story 4.10; AD-66 as amended, D-469, D-517).
  test("walking is on by default", async ({ page }) => {
    expect(DEFAULT_WALKING).toBe(true);
    await openScene(page, loadBoard("scene-c"));
    const toggle = page.getByRole("button", { name: "Walking" });
    await expect(toggle).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await expect(toggle).toHaveText("Walking");
  });

  test("the walking toggle shows in Scene mode only", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"), {}, loadRelay("relay-clean"));
    const toggle = page.getByTestId("walking-toggle");
    await expect(toggle).toHaveCount(0);
    await page.getByTestId("mode-scene").click();
    await sceneSettled(page);
    await expect(toggle).toBeVisible();
    await expect(toggle).toHaveText("Walking");
    // Its appearance moves no gauge and no button (D-517, D-526): their
    // boxes with the toggle equal those with it taken out of the layout,
    // the relay's toggle shown too. Below the scene's minimum area (1000
    // px wide) the header already spans several lines and the buttons may
    // wrap under the gauges; the gauges still keep their places.
    const fixed = (gaugesOnly: boolean) =>
      page.locator(gaugesOnly ? 'header [data-testid^="gauge-"]' : 'header [data-testid^="gauge-"], header button:not([data-testid="header-status"] button)').evaluateAll((elements) =>
        elements
          .filter((e) => e.getAttribute("data-testid") !== "walking-toggle")
          .map((e) => {
            const r = e.getBoundingClientRect();
            return { name: e.getAttribute("data-testid") ?? e.textContent?.trim(), x: r.x, y: r.y, w: r.width, h: r.height };
          }),
      );
    await expect(page.getByTestId("relay-toggle")).toBeVisible();
    for (const [width, height] of [[1000, 760], [1400, 900], [1920, 1001]] as const) {
      await page.setViewportSize({ width, height });
      const gaugesOnly = width === 1000;
      const shown = await fixed(gaugesOnly);
      expect(shown.length).toBeGreaterThan(1);
      await toggle.evaluate((e) => ((e as HTMLElement).style.display = "none"));
      const absent = await fixed(gaugesOnly);
      await toggle.evaluate((e) => ((e as HTMLElement).style.display = ""));
      expect(absent.map((b) => b.name)).toEqual(shown.map((b) => b.name));
      for (const [i, before] of absent.entries()) {
        for (const key of ["x", "y", "w", "h"] as const) {
          expect(Math.abs(shown[i]![key] - before[key]), `${width} x ${height}: ${before.name} ${key}`).toBeLessThanOrEqual(0.5);
        }
      }
    }
    // On the owner's screen the header keeps one line (D-526): the toggle
    // and the last button on the brand's line.
    const top = (await page.locator("header .brand").boundingBox())!.y;
    for (const id of ["walking-toggle", "pin-toggle"]) expect((await page.getByTestId(id).boundingBox())!.y, id).toBe(top);
    await page.setViewportSize({ width: 1400, height: 900 });
    // Outside the Sessions view it keeps its room, unseen and unreachable.
    const box = await toggle.boundingBox();
    await page.getByTestId("view-subjects").click();
    await expect(toggle).toBeHidden();
    expect(await toggle.boundingBox()).toEqual(box);
    await page.getByTestId("view-sessions").click();
    await expect(toggle).toBeVisible();
    await page.getByTestId("mode-board").click();
    await expect(toggle).toHaveCount(0);
  });

  test("the walking setting survives a reload", async ({ page }) => {
    await openScene(page, loadBoard("scene-c"));
    const toggle = page.getByTestId("walking-toggle");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(readViewState(await page.evaluate((key) => localStorage.getItem(key), VIEW_STORAGE_KEY)).walking).toBe(false);
    await page.reload();
    await sceneSettled(page);
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await toggle.click();
    await page.reload();
    await sceneSettled(page);
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
  });

  test("banners stay above the scene", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const stale = { ...board, stale: { since_ms: board.generated_at_ms - 60_000, error: "agents snapshot failed" } };
    await openScene(page, stale, loadRelay("relay-stopped"));
    const top = (await scene(page).boundingBox())!.y;
    for (const id of ["alert-band", "stale-banner", "relay-stopped-banner"]) {
      const box = (await page.getByTestId(id).boundingBox())!;
      expect(box.y + box.height, id).toBeLessThanOrEqual(top);
    }
    // The band first, under the header.
    const band = (await page.getByTestId("alert-band").boundingBox())!;
    const banner = (await page.getByTestId("stale-banner").boundingBox())!;
    expect(band.y).toBeLessThan(banner.y);
  });

  test("tall banners scroll the window and leave the scene half of it", async ({ page }) => {
    // A pending catalogue's diff, a stopped relay and a stale board at once.
    const board = loadBoard("scene-c");
    const stale = { ...board, stale: { since_ms: board.generated_at_ms - 60_000, error: "agents snapshot failed" } };
    await openScene(page, stale, { ...loadRelay("relay-pending"), stopped: true });
    const { area, half } = await scene(page).evaluate((s) => ({ area: s.parentElement!.clientHeight, half: innerHeight / 2 }));
    expect(area).toBeGreaterThanOrEqual(Math.floor(half));
    await expect(scene(page)).toBeVisible();
  });

  // Focus (story 4.11; FR-7, AD-66, D-113, D-114).
  test("f turns Focus on and off back to the previous mode", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"));
    for (const base of ["board", "scene"]) {
      await page.getByTestId(`mode-${base}`).click();
      if (base === "scene") await sceneSettled(page);
      await expect(page.getByTestId("walking-toggle")).toHaveCount(base === "scene" ? 1 : 0);
      await page.keyboard.press("f");
      await expect(focusList(page)).toBeVisible();
      await expect.poll(() => chosenMode(page)).toBe("focus");
      await expect(scene(page)).toHaveCount(0);
      await expect(boardColumns(page)).toHaveCount(0);
      // The walking toggle is hidden in Focus, its room kept.
      if (base === "scene") await expect(page.getByTestId("walking-toggle")).toBeHidden();
      await page.keyboard.press("f");
      await expect.poll(() => chosenMode(page)).toBe(base);
      await expect(focusList(page)).toHaveCount(0);
      if (base === "scene") {
        await sceneSettled(page);
        await expect(page.getByTestId("walking-toggle")).toBeVisible();
      } else {
        await expect(boardColumns(page)).toBeVisible();
      }
    }
    // A held key switches once.
    await page.keyboard.down("f");
    for (let i = 0; i < 4; i++) await page.evaluate(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", repeat: true })));
    await page.keyboard.up("f");
    await expect(focusList(page)).toBeVisible();
  });

  test("m from Focus switches the base and leaves Focus", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"));
    await page.keyboard.press("f");
    await page.keyboard.press("m");
    await expect.poll(() => chosenMode(page)).toBe("scene");
    await sceneSettled(page);
    await page.keyboard.press("f");
    await page.keyboard.press("m");
    await expect.poll(() => chosenMode(page)).toBe("board");
    await expect(boardColumns(page)).toBeVisible();
  });

  test("Focus survives a reload", async ({ page }) => {
    await openScene(page, loadBoard("scene-c"));
    await page.keyboard.press("f");
    await expect(focusList(page)).toBeVisible();
    expect(readViewState(await page.evaluate((key) => localStorage.getItem(key), VIEW_STORAGE_KEY))).toMatchObject({ base: "scene", focus: true });
    await page.reload();
    await expect(focusList(page)).toBeVisible();
    await expect.poll(() => chosenMode(page)).toBe("focus");
    // Back to the base it hid.
    await page.keyboard.press("f");
    await sceneSettled(page);
  });

  test("Focus lists Needs you then Your turn, with Working as a count", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openFocus(page, board);
    const { needs_you, your_turn, working } = board.columns;
    await expect(focusRows(page)).toHaveCount(needs_you.length + your_turn.length);
    const keys = await focusRows(page).evaluateAll((rows) => rows.map((row) => row.getAttribute("data-key")));
    expect(keys).toEqual([...needs_you, ...your_turn].map((c) => c.key));
    // Needs you rows are numbered as the board numbers them.
    const numbers = await page.getByTestId("focus-needs-you").locator("kbd").allTextContents();
    expect(numbers).toEqual(needs_you.slice(0, 9).map((_, i) => String(i + 1)));
    await expect(page.getByTestId("focus-your-turn").locator("kbd")).toHaveCount(0);
    await expect(page.getByTestId("focus-working")).toHaveText(`${working.length} sessions working`);
    for (const card of working) await expect(page.locator(`[data-testid="focus-row"][data-key="${card.key}"]`)).toHaveCount(0);
  });

  test("a Focus row brings its session to front", async ({ page }) => {
    const board = loadBoard("scene-c");
    const target = board.columns.your_turn[0]!;
    await openFocus(page, board);
    const row = page.locator(`[data-testid="focus-row"][data-key="${target.key}"]`);
    await row.getByRole("button").click();
    await expect.poll(() => focusCalls(page)).toEqual([target.key]);
    // Its outcome shows in the row, as on the board card.
    await expect(row.getByTestId("focus-outcome")).toBeVisible();
  });

  test("Focus shows the approvals waiting and opens them on the board", async ({ page }) => {
    const board = loadBoard("approvals");
    expect(board.approvals.length).toBeGreaterThan(1);
    await openFocus(page, board);
    const row = page.getByTestId("focus-approvals");
    await expect(row).toHaveText(`${board.approvals.length} approvals waiting`);
    // First, above the sessions.
    expect((await row.boundingBox())!.y).toBeLessThan((await focusRows(page).first().boundingBox())!.y);
    await row.click();
    await expect.poll(() => chosenMode(page)).toBe("board");
    await expect(page.getByTestId("approval-card")).toHaveCount(board.approvals.length);
  });

  test("Focus marks alerted sessions, a last known one included", async ({ page }) => {
    const board = loadBoard("scene-alert");
    const waitingCards = (b: Board): Card[] => [...b.columns.needs_you, ...b.columns.your_turn];
    const [first, ...others] = waitingCards(board).filter((c) => c.scene?.alert);
    expect(first).toBeDefined();
    // The first one's data is unknown: its alert is the last known (D-494).
    const unknown = structuredClone(board);
    const changed = waitingCards(unknown).find((c) => c.key === first!.key)!;
    changed.scene!.zone = "unknown";
    changed.badge = "unknown";
    await openFocus(page, unknown);
    const pill = (c: Card) => page.locator(`[data-testid="focus-row"][data-key="${c.key}"]`).getByTestId("focus-alert");
    await expect(pill(first!)).toContainText("Alert");
    await expect(pill(first!)).toContainText("last known");
    for (const other of others) {
      await expect(pill(other)).toBeVisible();
      await expect(pill(other)).not.toContainText("last known");
    }
    // A session out of alert has no pill.
    for (const c of waitingCards(board).filter((c) => !c.scene?.alert)) await expect(pill(c)).toHaveCount(0);
  });

  test("Focus runs no animation", async ({ page }) => {
    await openFocus(page, loadBoard("scene-alert"));
    await expect(scene(page)).toHaveCount(0);
    expect(await page.evaluate(() => document.getAnimations().length)).toBe(0);
    await page.clock.runFor(5_000);
    expect(await page.evaluate(() => document.getAnimations().length)).toBe(0);
  });

  test("Focus rows are 44 px or more", async ({ page }) => {
    await openFocus(page, loadBoard("approvals"));
    const targets = page.locator('[data-testid="focus-row"] > button, [data-testid="focus-approvals"]');
    expect(await targets.count()).toBeGreaterThan(1);
    for (const height of await targets.evaluateAll((elements) => elements.map((e) => e.getBoundingClientRect().height))) {
      expect(height).toBeGreaterThanOrEqual(44);
    }
  });

  test("m, f and digits do nothing outside the Sessions view", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"), {}, loadRelay("relay-clean"));
    for (const view of ["subjects", "relay"]) {
      await page.getByTestId(`view-${view}`).click();
      for (const key of ["m", "f", "1", "2"]) await page.keyboard.press(key);
    }
    expect(await focusCalls(page)).toEqual([]);
    await page.getByTestId("view-sessions").click();
    await expect.poll(() => chosenMode(page)).toBe("board");
    await expect(boardColumns(page)).toBeVisible();
  });

  test("a mode change closes the session file", async ({ page }) => {
    const board = loadBoard("scene-c");
    const key = board.columns.needs_you[0]!.key;
    await openScene(page, board);
    const figure = scene(page).locator(`[data-layer="targets"] [data-key=${JSON.stringify(key)}]`);
    for (const leave of ["m", "f"]) {
      await figure.click();
      await expect(page.getByTestId("session-file")).toBeVisible();
      await page.keyboard.press(leave);
      await expect(page.getByTestId("session-file")).toHaveCount(0);
      await page.keyboard.press(leave);
      await sceneSettled(page);
      await expect(page.getByTestId("session-file")).toHaveCount(0);
    }
  });

  test("leaving Scene mid-walk leaves no frame loop running", async ({ page }) => {
    const board = loadBoard("scene-b");
    await openScene(page, board);
    // Count the frame callbacks the page asked for and has neither run nor
    // cancelled, over the installed clock's own.
    await page.evaluate(() => {
      const pending = new Set<number>();
      const request = window.requestAnimationFrame.bind(window);
      const cancel = window.cancelAnimationFrame.bind(window);
      window.requestAnimationFrame = (callback) => {
        const id: number = request((time) => {
          pending.delete(id);
          callback(time);
        });
        pending.add(id);
        return id;
      };
      window.cancelAnimationFrame = (id) => {
        pending.delete(id);
        cancel(id);
      };
      (window as unknown as { __pendingFrames: () => number }).__pendingFrames = () => pending.size;
    });
    const pendingFrames = () => page.evaluate(() => (window as unknown as { __pendingFrames: () => number }).__pendingFrames());
    // A Working session starts waiting: its figure walks to the decision office.
    const next = structuredClone(board);
    const card = next.columns.working.splice(0, 1)[0]!;
    next.columns.needs_you.push({
      ...card,
      column: "needs_you",
      waiting_for: "permission prompt",
      scene: { ...card.scene!, zone: "decision", waiting_since_ms: board.generated_at_ms, alert: null, approval: false },
    });
    next.waiting_count = next.columns.needs_you.length;
    await page.evaluate((b) => window.__officinaPushBoard!(b), next);
    await page.clock.runFor(160);
    await expect(scene(page)).toHaveAttribute("data-moving", "1");
    expect(await pendingFrames()).toBeGreaterThan(0);
    for (const leave of ["f", "m"]) {
      await page.keyboard.press(leave);
      await expect(scene(page)).toHaveCount(0);
      expect(await pendingFrames(), leave).toBe(0);
      await page.clock.runFor(160);
      expect(await pendingFrames(), leave).toBe(0);
      // Back to Scene: everyone placed at once (D-469), nothing walking.
      await page.keyboard.press(leave);
      await expect(scene(page)).toHaveAttribute("data-moving", "0");
      expect(await pendingFrames(), leave).toBe(0);
    }
  });
});
