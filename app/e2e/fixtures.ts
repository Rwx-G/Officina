import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Page } from "@playwright/test";
import type { Board } from "../src/lib/generated/Board";
import type { FocusOutcome } from "../src/lib/generated/FocusOutcome";
import type { RelayView } from "../src/lib/generated/RelayView";

const dir = join(dirname(fileURLToPath(import.meta.url)), ".fixtures");

export type Scenario = "busy" | "empty" | "stale" | "quota-reset" | "approvals";

export type RelayScenario =
  | "relay-clean"
  | "relay-pending"
  | "relay-pending-again"
  | "relay-pending-approved"
  | "relay-stopped"
  | "relay-activity";

export function loadBoard(scenario: Scenario): Board {
  return JSON.parse(readFileSync(join(dir, `${scenario}.json`), "utf8")) as Board;
}

/** A relay view printed by `officina relay --json` (D-25). */
export function loadRelay(scenario: RelayScenario): RelayView {
  return JSON.parse(readFileSync(join(dir, `${scenario}.json`), "utf8")) as RelayView;
}

/** Opens the board with `board` as the backend's answer and the clock frozen
 *  at the board's generation time, so relative times are deterministic; the
 *  relay view, when given, is what `get_relay` answers. */
export async function openBoard(
  page: Page,
  board: Board,
  focus: Record<string, FocusOutcome> = {},
  relay: RelayView | null = null,
): Promise<void> {
  await page.clock.install({ time: board.generated_at_ms });
  await page.addInitScript(
    ([b, f, r]) => {
      window.__officinaBoard = b;
      window.__officinaFocus = f;
      window.__officinaRelay = r;
    },
    [board, focus, relay] as const,
  );
  await page.goto("/");
  if (board) await page.getByTestId("column-needs_you").waitFor();
}

export async function calls(page: Page): Promise<{ cmd: string; args: unknown }[]> {
  return page.evaluate(() => window.__officinaCalls ?? []);
}
