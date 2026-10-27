import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Page } from "@playwright/test";
import type { Board } from "../src/lib/generated/Board";
import type { FocusOutcome } from "../src/lib/generated/FocusOutcome";

const dir = join(dirname(fileURLToPath(import.meta.url)), ".fixtures");

export type Scenario = "busy" | "empty" | "stale" | "quota-reset";

export function loadBoard(scenario: Scenario): Board {
  return JSON.parse(readFileSync(join(dir, `${scenario}.json`), "utf8")) as Board;
}

/** Opens the board with `board` as the backend's answer and the clock frozen
 *  at the board's generation time, so relative times are deterministic. */
export async function openBoard(
  page: Page,
  board: Board,
  focus: Record<string, FocusOutcome> = {},
): Promise<void> {
  await page.clock.install({ time: board.generated_at_ms });
  await page.addInitScript(
    ([b, f]) => {
      window.__officinaBoard = b;
      window.__officinaFocus = f;
    },
    [board, focus] as const,
  );
  await page.goto("/");
}

export async function calls(page: Page): Promise<{ cmd: string; args: unknown }[]> {
  return page.evaluate(() => window.__officinaCalls ?? []);
}
