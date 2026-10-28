// The Sessions view and its modes (story 4.7 AC 1; AD-66, D-113, D-481):
// the view switch's names, the mode switch, `m`, the stored mode and its
// defaults, and the banners above the sessions area.
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, loadRelay, openBoard, openScene, sceneSettled } from "./fixtures";
import { VIEW_STORAGE_KEY } from "../src/lib/view.svelte";

const scene = (page: Page) => page.getByTestId("scene");
const boardColumns = (page: Page) => page.getByTestId("column-needs_you");

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

  test("the mode switch offers Board and Scene, Board by default", async ({ page }) => {
    await openBoard(page, loadBoard("scene-c"));
    const group = page.getByRole("radiogroup", { name: "Mode" });
    await expect(group.getByRole("radio")).toHaveText(["Board", "Scene"]);
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

  test("digits act on Needs you in both modes", async ({ page }) => {
    const board = loadBoard("scene-c");
    await openScene(page, board);
    await page.keyboard.press("1");
    expect((await calls(page)).filter((c) => c.cmd === "focus")).toEqual([
      { cmd: "focus", args: { key: board.columns.needs_you[0]!.key } },
    ]);
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
});
