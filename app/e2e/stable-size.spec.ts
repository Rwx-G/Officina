// A control keeps its size whatever its state, so toggling it moves
// nothing beside it. The toggles are found on the page, never listed: a
// button with `aria-pressed`, a radio, a control whose label is a
// `StableLabel`, or a slot marked `data-stable-slot`. Each scene fails when it shows a toggle no
// case below stands for, so a new toggle cannot go untested silently.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { loadBoard, loadRegistry, loadRelay, openBoard, openScene } from "./fixtures";
import type { RegistryView } from "../src/lib/generated/RegistryView";

/** Sub-pixel rounding between two layouts, never a real move. */
const TOLERANCE_PX = 0.5;

/** What makes an element a toggle this spec must cover. */
const TOGGLE = '[aria-pressed], [role="radio"], [data-stable-slot], :has(> [data-stable-label])';

type SceneName = "board" | "sceneMode" | "relay" | "subjects" | "triage";

const SCENES: Record<SceneName, (page: Page) => Promise<void>> = {
  board: async (page) => {
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  },
  // The Sessions view's Scene mode: the header on one line (D-525).
  sceneMode: async (page) => {
    await openScene(page, loadBoard("busy"), loadRelay("relay-clean"));
  },
  relay: async (page) => {
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
    await page.getByTestId("view-relay").click();
    await expect(page.getByTestId("relay-view")).toBeVisible();
  },
  subjects: async (page) => {
    await openBoard(page, loadBoard("model-board"), {}, null, loadRegistry("model"));
    await page.getByTestId("view-subjects").click();
    await expect(page.getByTestId("triage-evidence")).toBeVisible();
  },
  // Unfiled rows with subjects to pick.
  triage: async (page) => {
    await openBoard(page, loadBoard("triage-board"), {}, null, loadRegistry("triage"));
    await page.getByTestId("view-subjects").click();
    await expect(page.getByTestId("triage-panel")).toBeVisible();
  },
};

interface ToggleCase {
  name: string;
  /** The controls the case stands for. */
  selector: string;
  scene: SceneName;
  /** Flips the control's state; a click on it unless said otherwise. */
  act?: (page: Page, control: Locator) => Promise<void>;
  /** The instance flipped starts pressed, for a control another one releases. */
  pressed?: boolean;
}

const CASES: ToggleCase[] = [
  { name: "view switch", selector: '[data-testid="view-switch"] > button', scene: "board" },
  { name: "header relay toggle", selector: '[data-testid="relay-toggle"]', scene: "board" },
  { name: "pin on top", selector: '[data-testid="pin-toggle"]', scene: "board" },
  {
    name: "mode switch",
    selector: '[data-testid="mode-switch"] [role="radio"]',
    scene: "board",
    // Choosing the other mode releases the chosen one. Scene mode lays the
    // header on one line, which must move neither radio (D-517, D-525).
    act: (page) => page.locator('[data-testid="mode-switch"] [aria-checked="false"]').click(),
  },
  // The theme control (AD-67, D-517), in both header layouts.
  { name: "theme switch", selector: '[data-testid="theme-switch"]', scene: "board" },
  { name: "theme switch on one line", selector: '[data-testid="theme-switch"]', scene: "sceneMode" },
  // Scene mode's Walking toggle (D-469, D-517): a fixed control.
  { name: "walking toggle", selector: '[data-testid="walking-toggle"]', scene: "sceneMode", pressed: true },
  { name: "copy attach command", selector: '[data-testid="copy-attach"]', scene: "board" },
  { name: "Relay view stop toggle", selector: '[data-testid="relay-view-stop"]', scene: "relay" },
  { name: "triage verdict", selector: '[data-testid^="triage-verdict-"]', scene: "subjects" },
  {
    name: "subject actions",
    selector: '[data-testid="subject-rename"], [data-testid="subject-merge"], [data-testid="subject-archive"]',
    scene: "subjects",
  },
  {
    name: "subject picker",
    selector: '[data-testid="pick-subject"]',
    scene: "triage",
    // The proposed subject is chosen; a new subject releases it.
    pressed: true,
    act: async (_page, control) => {
      const picker = control.locator("xpath=ancestor::*[@data-testid='subject-picker']");
      await picker.getByTestId("picker-new-name").fill("a new subject");
      await picker.getByTestId("picker-create").click();
    },
  },
  {
    name: "triage model switch",
    selector: 'label:has(> [data-testid="model-toggle"])',
    scene: "subjects",
  },
  {
    name: "evaluate and its progress",
    selector: '[data-testid="evaluate-slot"]',
    scene: "subjects",
    // A dry run starts when the backend says so, with the next view.
    act: (page) =>
      page.evaluate(() => {
        const view: RegistryView = window.__officinaRegistry!;
        // Two-digit counts, the widest progress a capped dry run shows.
        const total = 48;
        return window.__officinaPushRegistry?.({
          ...view,
          model: { ...view.model, dry_run: { done: total - 1, total, started_at_ms: view.generated_at_ms } },
        });
      }),
  },
];

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The attribute that marks, for the second measure, the element found
 *  after the control by the first. */
const FOLLOWER = "data-stable-follower";

/** The control's box and the position of what follows it (its next
 *  sibling, or its nearest ancestor's), in page coordinates. The second
 *  measure takes the follower the first one marked. */
function layout(control: Locator, first: boolean): Promise<{ own: Box; next: Box | null; nextName: string }> {
  return control.evaluate(
    (element, [follower, first]) => {
      const box = (rect: DOMRect): Box => ({
        x: rect.x + window.scrollX,
        y: rect.y + window.scrollY,
        width: rect.width,
        height: rect.height,
      });
      let next: Element | null;
      if (first) {
        let anchor: Element | null = element;
        while (anchor && !anchor.nextElementSibling) anchor = anchor.parentElement;
        next = anchor?.nextElementSibling ?? null;
        next?.setAttribute(follower, "");
      } else {
        next = document.querySelector(`[${follower}]`);
      }
      return {
        own: box(element.getBoundingClientRect()),
        next: next ? box(next.getBoundingClientRect()) : null,
        nextName: next ? `${next.tagName.toLowerCase()}.${next.className}` : "none",
      };
    },
    [FOLLOWER, first] as const,
  );
}

/** Everything the page shows of a control's state. */
function signature(control: Locator): Promise<string> {
  return control.evaluate((element) => {
    const checked = Array.from(element.querySelectorAll("input")).map((input) => input.checked);
    return `${element.outerHTML}|${checked.join(",")}`;
  });
}

function expectClose(after: number, before: number, what: string): void {
  expect(Math.abs(after - before), `${what}: ${before} then ${after}`).toBeLessThanOrEqual(TOLERANCE_PX);
}

test.describe("controls keep their size whatever their state", () => {
  for (const scene of Object.keys(SCENES) as SceneName[]) {
    test(`every toggle of the ${scene} scene has a case`, async ({ page }) => {
      await SCENES[scene](page);
      const covered = CASES.map((c) => c.selector).join(", ");
      const uncovered = await page
        .locator(TOGGLE)
        .evaluateAll((elements, selector) => elements.filter((e) => !e.matches(selector)).map((e) => e.outerHTML), covered);
      expect(uncovered, "toggles no case stands for").toEqual([]);
    });
  }

  for (const toggle of CASES) {
    test(`${toggle.name} keeps its box and moves nothing after it`, async ({ page }) => {
      await SCENES[toggle.scene](page);
      // An instance that can be flipped (shown, enabled, pressed only when
      // the case says so), preferably one with a sibling of its own after
      // it: a last child's follower is further away and may come and go
      // with the state.
      const index = await page.locator(toggle.selector).evaluateAll((elements, pressed) => {
        const flippable = elements.filter(
          (e) =>
            e.checkVisibility() &&
            !e.matches(":disabled, :has(:disabled)") &&
            (e.getAttribute("aria-pressed") === "true") === pressed,
        );
        const chosen = flippable.find((e) => e.nextElementSibling !== null) ?? flippable[0];
        return chosen ? elements.indexOf(chosen) : -1;
      }, toggle.pressed ?? false);
      expect(index, `no ${toggle.name} to flip in the ${toggle.scene} scene`).toBeGreaterThanOrEqual(0);
      const control = page.locator(toggle.selector).nth(index);

      const before = await layout(control, true);
      const state = await signature(control);
      await (toggle.act ? toggle.act(page, control) : control.click());
      await expect.poll(() => signature(control), { message: `${toggle.name} did not change state` }).not.toBe(state);
      const after = await layout(control, false);

      for (const key of ["x", "y", "width", "height"] as const) {
        expectClose(after.own[key], before.own[key], `${toggle.name} ${key}`);
      }
      expect(after.nextName).toBe(before.nextName);
      if (before.next && after.next) {
        expectClose(after.next.x, before.next.x, `after ${toggle.name}, ${before.nextName} x`);
        expectClose(after.next.y, before.next.y, `after ${toggle.name}, ${before.nextName} y`);
      }
    });
  }

  // D-517: a status chip coming or going moves neither the gauges nor any
  // header button. Every chip the header can show is shown at once, then
  // none; the boxes of the gauges and the buttons outside the status slot
  // are compared.
  for (const mode of ["board", "scene"] as const) test(`header status chips move neither the gauges nor the buttons (${mode} mode)`, async ({ page }) => {
    const board = loadBoard("busy");
    const clean = loadRelay("relay-clean");
    if (mode === "board") await openBoard(page, board, {}, clean);
    else await openScene(page, board, clean);
    const fixed = page.locator(
      'header [data-testid^="gauge-"], header button:not([data-testid="header-status"] button)',
    );
    const boxes = () =>
      fixed.evaluateAll((elements) =>
        elements.map((e) => {
          const r = e.getBoundingClientRect();
          return { name: e.getAttribute("data-testid") ?? e.textContent?.trim(), x: r.x, y: r.y, w: r.width, h: r.height };
        }),
      );
    await page.evaluate(
      ([b, r]) =>
        Promise.all([
          window.__officinaPushBoard?.({
            ...b,
            waiting_count: 0,
            columns: { ...b.columns, needs_you: [] },
            contract: { ...b.contract, held: true, missing: [], hook_missing: [] },
          }),
          window.__officinaPushRelay?.({ ...r, stopped: false, pending: null }),
        ]),
      [board, clean] as const,
    );
    await expect(page.getByTestId("header-status").locator(":scope > *")).toHaveCount(1);
    const quiet = await boxes();
    expect(quiet.length).toBeGreaterThan(3);
    const pending = loadRelay("relay-pending");
    await page.evaluate(
      ([b, r]) =>
        Promise.all([
          window.__officinaPushBoard?.({
            ...b,
            contract: { ...b.contract, held: false, missing: ["a field"], hook_missing: ["Stop.cwd"] },
          }),
          window.__officinaPushRelay?.({ ...r, stopped: true }),
        ]),
      [board, pending] as const,
    );
    for (const chip of ["waiting-pill", "relay-stopped-chip", "catalogue-chip", "contract", "contract-hook"]) {
      await expect(page.getByTestId(chip)).toBeVisible();
    }
    const busy = await boxes();
    expect(busy.map((b) => b.name)).toEqual(quiet.map((b) => b.name));
    for (const [i, before] of quiet.entries()) {
      for (const key of ["x", "y", "w", "h"] as const) {
        expectClose(busy[i]![key], before[key], `${before.name} ${key}`);
      }
    }
  });

  // D-516: what Confirm all leaves out is a line beside the button, so the
  // button keeps its box when more rows are left out.
  test("Confirm all keeps its box whatever it leaves out", async ({ page }) => {
    await SCENES.triage(page);
    const all = page.getByTestId("confirm-all");
    const before = await all.boundingBox();
    const leftOut = await page.getByTestId("confirm-all-left-out").textContent();
    await page.evaluate(() => {
      const view: RegistryView = window.__officinaRegistry!;
      const row = view.triage.find((r) => r.kind !== "unfiled_session")!;
      return window.__officinaPushRegistry?.({
        ...view,
        triage: view.triage.map((r) =>
          r.key === row.key
            ? { ...r, proposal: { verdict: "unsure", source: "model", reason: "model", call_id: 9 }, verdict: null }
            : r,
        ),
      });
    });
    await expect(page.getByTestId("confirm-all-left-out")).not.toHaveText(leftOut ?? "");
    const after = await all.boundingBox();
    for (const key of ["x", "y", "width", "height"] as const) {
      expectClose(after![key], before![key], `Confirm all ${key}`);
    }
  });

  test("the scrollbar's room is kept in every view", async ({ page }) => {
    await SCENES.board(page);
    // Headless Chromium hides scrollbars, so the style is what can be seen.
    expect(await page.evaluate(() => getComputedStyle(document.documentElement).scrollbarGutter)).toBe("stable");
  });
});
