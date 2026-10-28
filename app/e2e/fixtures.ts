import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Page } from "@playwright/test";
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
  | "subjects-board";

/** Registry views printed by `officina subjects --json` after the
 *  `registry_fixture` example wrote the registry (story 3.5, D-72). */
export type RegistryScenario = "subjects" | "subjects-synced" | "subjects-restored" | "subjects-unavailable";

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
  await page.clock.install({ time: board.generated_at_ms });
  await page.addInitScript(
    ([b, f, r, g]) => {
      window.__officinaBoard = b;
      window.__officinaFocus = f;
      window.__officinaRelay = r;
      window.__officinaRegistry = g;
    },
    [board, focus, relay, registry] as const,
  );
  await page.goto("/");
  if (board) await page.getByTestId("column-needs_you").waitFor();
}

export async function calls(page: Page): Promise<{ cmd: string; args: unknown }[]> {
  return page.evaluate(() => window.__officinaCalls ?? []);
}
