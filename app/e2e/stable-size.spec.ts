// A control keeps its size whatever its state, so toggling it moves
// nothing beside it. The toggles are found on the page, never listed: a
// button with `aria-pressed`, a radio, a control whose label is a
// `StableLabel`, or a slot marked `data-stable-slot`. Each scene fails when it shows a toggle no
// case below stands for, so a new toggle cannot go untested silently.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { fixtureOf } from "./fixture-index";
import { PENDING_LIMIT, loadBoard, loadRegistry, loadRelay, openBoard, openScene, sceneBoardOrchestrator, sceneView, transitionsDone, withPending } from "./fixtures";
import type { MeetingView } from "../src/lib/generated/MeetingView";
import type { OrchestratorView } from "../src/lib/generated/OrchestratorView";
import type { RegistryView } from "../src/lib/generated/RegistryView";
import type { WorkOrderCard } from "../src/lib/generated/WorkOrderCard";
import { DEFAULT_WALKING } from "../src/lib/view.svelte";

/** Sub-pixel rounding between two layouts, never a real move. */
const TOLERANCE_PX = 0.5;

/** What makes an element a toggle this spec must cover. */
const TOGGLE = '[aria-pressed], [role="radio"], [data-stable-slot], :has(> [data-stable-label])';

type SceneName = "board" | "autoApproval" | "sceneMode" | "chairFile" | "relay" | "relayAuto" | "subjects" | "triage" | "orchestrator" | "meeting";

const SCENES: Record<SceneName, (page: Page) => Promise<void>> = {
  board: async (page) => {
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
  },
  // An `auto` request in Needs you (story 5.12): its rejection reasons.
  autoApproval: async (page) => {
    await openBoard(page, loadBoard("approvals-auto"), {}, loadRelay("relay-clean"));
  },
  // The Sessions view's Scene mode: the header on one line (D-525).
  sceneMode: async (page) => {
    await openScene(page, loadBoard("busy"), loadRelay("relay-clean"));
  },
  // The orchestrator's empty chair, its file open (story 5.18).
  chairFile: async (page) => {
    await openScene(page, loadBoard("busy"), loadRelay("relay-clean"));
    await page.getByTestId("scene").locator('[data-target="chair"]').click();
    await expect(page.getByTestId("chair-start")).toBeVisible();
    // The panel slides in (D-545): measured once it stands still.
    const start = page.getByTestId("chair-start");
    await expect
      .poll(async () => {
        const before = (await start.boundingBox())!.x;
        await page.clock.runFor(500);
        await transitionsDone(page);
        return (await start.boundingBox())!.x - before;
      })
      .toBe(0);
  },
  relay: async (page) => {
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
    await page.getByTestId("view-relay").click();
    await expect(page.getByTestId("relay-view")).toBeVisible();
  },
  // The Relay view's `auto` section with its gate holding (story 5.14).
  relayAuto: async (page) => {
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-auto"));
    await page.getByTestId("view-relay").click();
    await expect(page.getByTestId("relay-auto")).toBeVisible();
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
  // A work order card composed in the Orchestrator view (story 5.9), on
  // the day's view and a card core issued from its home.
  orchestrator: async (page) => {
    const view = fixtureOf<OrchestratorView>("orchestrator", "orchestrator-day");
    const card = fixtureOf<WorkOrderCard>("card", "card-bump");
    await page.addInitScript(
      ([v, c]) => {
        window.__officinaOrchestrator = v;
        window.__officinaCardAnswer = { outcome: "issued", value: c };
      },
      [view, card] as const,
    );
    await openBoard(page, loadBoard("busy"), {}, loadRelay("relay-clean"));
    await page.getByTestId("view-orchestrator").click();
    await page.getByTestId("composer-template").selectOption(card.template);
    await page.getByTestId("composer-project").selectOption(card.project);
    for (const param of card.values) {
      await page.locator(`[data-testid="composer-param"][data-param="${param.name}"]`).selectOption(param.value);
    }
    await expect(page.getByTestId("work-order-card")).toBeVisible();
  },
  // The busy board's first Needs you card received, its meeting the
  // background fixture's (story 5.11): the composer's Send, and the
  // proposed answer's slot and Send (story 5.21).
  meeting: async (page) => {
    const board = loadBoard("busy");
    const key = board.columns.needs_you[0]!.key;
    const view = { ...fixtureOf<MeetingView>("meeting", "meeting-background"), key };
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.addInitScript((m) => (window.__officinaMeetings = { [m.key]: m }), view);
    await openBoard(page, board, {}, loadRelay("relay-clean"));
    await page.locator(`[data-testid="card"][data-key=${JSON.stringify(key)}]`).getByTestId("receive").click();
    await expect(page.getByTestId("answer-composer")).toBeVisible();
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
  // Story 5.18 AC 9 (D-565, D-833): presence turns live and the count
  // goes to the most proposals that can be pending; the button keeps its
  // box and the mode switch after it stays.
  {
    name: "orchestrator view switch mark",
    selector: '[data-testid="view-orchestrator"]',
    scene: "board",
    act: async (page) => {
      await page.evaluate(
        async ([presence, view]) => {
          await window.__officinaPushBoard?.({ ...window.__officinaBoard!, orchestrator: presence });
          await window.__officinaPushOrchestrator?.(view);
        },
        [sceneBoardOrchestrator(), withPending(sceneView(), PENDING_LIMIT)] as const,
      );
    },
  },
  // The absent orchestrator's file in the scene (story 5.18 AC 6): held in
  // flight, Start reads Starting.
  {
    name: "chair file Start",
    selector: '[data-testid="chair-start"]',
    scene: "chairFile",
    act: async (page, control) => {
      await page.evaluate(() => (window.__officinaStartHeld = true));
      await control.click();
    },
  },
  { name: "header relay toggle", selector: '[data-testid="relay-toggle"]', scene: "board" },
  { name: "pin on top", selector: '[data-testid="pin-toggle"]', scene: "board" },
  {
    name: "mode switch",
    selector: '[data-testid="mode-switch"] [role="radio"]',
    scene: "board",
    // Choosing Scene releases Board. Scene mode lays the header on one
    // line, which must move no radio (D-517, D-525).
    act: (page) => page.getByTestId("mode-scene").click(),
  },
  // The theme control (AD-67, D-517), in both header layouts.
  { name: "theme switch", selector: '[data-testid="theme-switch"]', scene: "board" },
  { name: "theme switch on one line", selector: '[data-testid="theme-switch"]', scene: "sceneMode" },
  // The Walking toggle (D-469, D-517): a fixed control in every mode, as
  // the default leaves it (D-700).
  { name: "walking toggle", selector: '[data-testid="walking-toggle"]', scene: "sceneMode", pressed: DEFAULT_WALKING },
  // To resume and Done in the header (D-541): a count that changes moves
  // nothing beside it.
  {
    name: "ended counts",
    selector: '[data-testid="ended-to-resume"], [data-testid="ended-done"]',
    scene: "board",
    act: (page) =>
      page.evaluate(() => {
        const board = window.__officinaBoard!;
        const more = (cards: typeof board.columns.done) =>
          [...cards, ...cards.map((c, i) => ({ ...c, key: `${c.key}-more-${i}`, session_id: null }))];
        return window.__officinaPushBoard?.({
          ...board,
          columns: { ...board.columns, to_resume: more(board.columns.to_resume), done: more(board.columns.done) },
        });
      }),
  },
  { name: "copy attach command", selector: '[data-testid="copy-attach"]', scene: "board" },
  {
    name: "work order Launch",
    selector: '[data-testid="launch"]',
    scene: "orchestrator",
    // Held in flight, its label reads Launching (D-564).
    act: async (page, control) => {
      await page.evaluate(() => (window.__officinaLaunchHeld = true));
      await control.click();
    },
  },
  // Story 5.24 (D-986): the permission switch and the line naming the
  // mode, flipped by a click on the switch inside its tooltip.
  {
    name: "work order permission switch",
    selector: '[data-testid="card-permission-switch"]',
    scene: "orchestrator",
    act: async (page, control) => {
      await page.getByTestId("card-permission-text").hover();
      await control.click();
    },
  },
  {
    name: "work order permission line and its tooltip's text",
    selector: '[data-testid="card-permission-text"], [data-testid="card-permission-tip-text"]',
    scene: "orchestrator",
    act: async (page, control) => {
      await control.hover();
      await page.getByTestId("card-permission-switch").click();
    },
  },
  {
    name: "orchestrator Start",
    selector: '[data-testid="orchestrator-start"]',
    scene: "orchestrator",
    // Held in flight, its label reads Starting (D-564).
    act: async (page, control) => {
      await page.evaluate(() => (window.__officinaStartHeld = true));
      await control.click();
    },
  },
  {
    name: "auto rejection reasons",
    selector: '[data-testid="reject-reasons"]',
    scene: "autoApproval",
    // Reject on its card asks the reason in the slot (story 5.12 AC 6).
    act: (_page, control) =>
      control.locator("xpath=ancestor::*[@data-testid='approval-card']").getByTestId("reject").click(),
  },
  { name: "Relay view stop toggle", selector: '[data-testid="relay-view-stop"]', scene: "relay" },
  {
    name: "answer Send",
    selector: '[data-testid="answer-send"]',
    scene: "meeting",
    // Held in flight, its label reads Sending (D-564).
    act: async (page, control) => {
      await page.getByTestId("answer-text").fill("Keep the 1.x line.");
      await page.evaluate(() => (window.__officinaAnswerHeld = true));
      await control.click();
    },
  },
  // Story 5.21 (D-564): Send the proposed answer, held in flight, reads
  // Sending in its own box; the proposal's slot keeps its box when the
  // proposal goes.
  {
    name: "send the proposed answer",
    selector: '[data-testid="proposal-send"]',
    scene: "meeting",
    act: async (page, control) => {
      await page.evaluate(() => (window.__officinaAnswerHeld = true));
      await control.click();
    },
  },
  {
    name: "proposed answer slot",
    selector: '[data-testid="meeting-proposal-slot"]',
    scene: "meeting",
    // Dismissed, the proposal leaves the meeting read again.
    act: (page) => page.getByTestId("meeting-proposal-dismiss").click(),
  },
  // Story 5.14 (D-564): the `auto` section's toggles and their slots, with
  // approval on and the gate holding; turning it off keeps it offered.
  {
    name: "auto approval toggle",
    selector: '[data-testid="auto-approval-slot"], [data-testid="auto-approval-toggle"]',
    scene: "relayAuto",
    pressed: true,
  },
  {
    name: "auto rule mode toggle",
    selector: '[data-testid="auto-mode-slot"], [data-testid="auto-mode-toggle"]',
    scene: "relayAuto",
    // The slot is measured; its button, not its sentence, takes the click.
    act: (page) => page.getByTestId("auto-mode-toggle").click(),
  },
  {
    name: "approval reviewing slot",
    selector: '[data-testid="approval-actions"]',
    scene: "autoApproval",
    // A live review takes the request: the buttons give way to a line.
    act: async (page, control) => {
      const id = await control.locator("xpath=ancestor::*[@data-testid='approval-card']").getAttribute("data-id");
      await page.evaluate((reviewed) => {
        const board = window.__officinaBoard!;
        return window.__officinaPushBoard?.({
          ...board,
          approvals: board.approvals.map((a) => (a.id === reviewed ? { ...a, state: "reviewing" as const } : a)),
        });
      }, id);
    },
  },
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
    // A held contract shows nothing (D-679): the quiet slot is empty.
    await expect(page.getByTestId("waiting-pill")).toHaveCount(0);
    await expect(page.getByTestId("header-status").locator(":scope > *")).toHaveCount(0);
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
