import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";
import { canvas, fitScale } from "../src/lib/scene/geometry";
import { VIEW_STORAGE_KEY, defaultViewState, serialiseViewState } from "../src/lib/view.svelte";
import type { Board } from "../src/lib/generated/Board";
import type { FocusOutcome } from "../src/lib/generated/FocusOutcome";
import type { RegistryView } from "../src/lib/generated/RegistryView";
import type { RelayView } from "../src/lib/generated/RelayView";

const dir = join(dirname(fileURLToPath(import.meta.url)), ".fixtures");

export type Scenario =
  | "busy"
  | "empty"
  | "stale"
  | "quota-reset"
  | "approvals"
  | "registry-unavailable"
  | "subjects-board"
  | "triage-board"
  | "evidence-board"
  | "model-board"
  | "settle-before-board"
  | "settle-after-board"
  | "contract-hook-missing"
  | "scene-a"
  | "scene-b"
  | "scene-c"
  | "scene-alert"
  | "scene-alert-early"
  | "scene-eight"
  | "scene-overflow"
  | "scene-subagents";

/** Registry views printed by `officina subjects --json` after the
 *  `registry_fixture` example wrote the registry (stories 3.5, 3.7, 3.9 and
 *  3.11, D-72). */
export type RegistryScenario =
  | "subjects"
  | "subjects-synced"
  | "subjects-restored"
  | "subjects-unavailable"
  | "triage"
  | "evidence"
  | "model"
  | "model-paused"
  | "settle-before"
  | "settle-after";

export type RelayScenario =
  | "relay-clean"
  | "relay-pending"
  | "relay-pending-again"
  | "relay-pending-approved"
  | "relay-stopped"
  | "relay-activity"
  | "relay-anomalies";

export function loadBoard(scenario: Scenario): Board {
  return JSON.parse(readFileSync(join(dir, `${scenario}.json`), "utf8")) as Board;
}

/** A relay view printed by `officina relay --json` (D-25). */
export function loadRelay(scenario: RelayScenario): RelayView {
  return JSON.parse(readFileSync(join(dir, `${scenario}.json`), "utf8")) as RelayView;
}

/** A registry view printed by `officina subjects --json` (D-72). */
export function loadRegistry(scenario: RegistryScenario): RegistryView {
  return JSON.parse(readFileSync(join(dir, `${scenario}.json`), "utf8")) as RegistryView;
}

/** The share of differing pixels a scene screenshot tolerates (D-119). */
export const SCREENSHOT_MAX_DIFF_RATIO = 0.005;

/** Installs the clock at the board's generation time, so relative times are
 *  deterministic, and the backend's answers; then loads the window. */
async function load(
  page: Page,
  board: Board,
  focus: Record<string, FocusOutcome>,
  relay: RelayView | null,
  registry: RegistryView | null,
  stored: Record<string, string> = {},
): Promise<void> {
  await page.clock.install({ time: board.generated_at_ms });
  await page.addInitScript(
    ([b, f, r, g, s]) => {
      window.__officinaBoard = b;
      window.__officinaFocus = f;
      window.__officinaRelay = r;
      window.__officinaRegistry = g;
      // Once per page: a reload keeps what the window stored since.
      if (!sessionStorage.getItem("officina.e2e.stored")) {
        sessionStorage.setItem("officina.e2e.stored", "1");
        for (const [key, value] of Object.entries(s)) localStorage.setItem(key, value);
      }
    },
    [board, focus, relay, registry, stored] as const,
  );
  await page.goto("/");
}

/** Opens the board with `board` as the backend's answer and the clock frozen
 *  at the board's generation time, so relative times are deterministic; the
 *  relay view, when given, is what `get_relay` answers. */
export async function openBoard(
  page: Page,
  board: Board,
  focus: Record<string, FocusOutcome> = {},
  relay: RelayView | null = null,
  registry: RegistryView | null = null,
): Promise<void> {
  await load(page, board, focus, relay, registry);
  if (board) await page.getByTestId("column-needs_you").waitFor();
  // The board fades in once it arrives (D-545): measured settled.
  await transitionsDone(page);
}

/** The scale the scene should have in its sessions area, from geometry.ts
 *  and the area's client size, as `Scene.svelte` measures it. */
export async function expectedScale(page: Page): Promise<string> {
  const { w, h, columns } = await page.getByTestId("scene").evaluate((scene) => {
    const area = scene.parentElement!;
    return { w: area.clientWidth, h: area.clientHeight, columns: Number(scene.getAttribute("data-columns")) };
  });
  return String(fitScale({ w, h }, canvas(columns)));
}

/** How far a settling scene's clock is run at a time, in ms, and for how
 *  long at most: a walk across the floor lasts longer than an expectation
 *  waits in real time. */
const SETTLE_STEP_MS = 5_000;
const SETTLE_MAX_MS = 120_000;

/** Waits until the scene has its scale for the area and nothing moves
 *  (D-185, `data-moving="0"`, the mover's): polled, never through
 *  `requestAnimationFrame`, which the installed clock fakes. A scene still
 *  walking has its clock run ahead, so its walks end without real seconds
 *  passing. */
export async function sceneSettled(page: Page): Promise<void> {
  const scene = page.getByTestId("scene");
  await expect(scene).toBeVisible();
  await expect.poll(async () => (await scene.getAttribute("data-scale")) === (await expectedScale(page))).toBe(true);
  for (let run = 0; run < SETTLE_MAX_MS && (await scene.getAttribute("data-moving")) !== "0"; run += SETTLE_STEP_MS) {
    await page.clock.runFor(SETTLE_STEP_MS);
  }
  await expect(scene).toHaveAttribute("data-moving", "0");
}

/** Opens the window in Scene mode, as a stored choice, on `board`. */
export async function openScene(
  page: Page,
  board: Board,
  relay: RelayView | null = null,
  registry: RegistryView | null = null,
  focus: Record<string, FocusOutcome> = {},
): Promise<void> {
  await load(page, board, focus, relay, registry, { [VIEW_STORAGE_KEY]: serialiseViewState({ ...defaultViewState(), base: "scene" }) });
  await sceneSettled(page);
  await transitionsDone(page);
}

/** Opens the window in Scene mode with walking on, both stored choices:
 *  for the cases about walking, which is off by default (D-480, E82). */
export async function openWalkingScene(page: Page, board: Board): Promise<void> {
  await load(page, board, {}, null, null, {
    [VIEW_STORAGE_KEY]: serialiseViewState({ ...defaultViewState(), base: "scene", walking: true }),
  });
  await sceneSettled(page);
  await transitionsDone(page);
}

/** Waits for the window's transitions to end (story 4.15, D-545): the
 *  Web Animations a view, a card or a panel runs once when it comes or
 *  goes. CSS animations (the halos) are not transitions and are left
 *  alone. */
export async function transitionsDone(page: Page): Promise<void> {
  await expect
    .poll(() => page.evaluate(() => document.getAnimations().filter((a) => !(a instanceof CSSAnimation) && a.playState === "running").length))
    .toBe(0);
}

export async function calls(page: Page): Promise<{ cmd: string; args: unknown }[]> {
  return page.evaluate(() => window.__officinaCalls ?? []);
}
