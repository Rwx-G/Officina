// What a click on a session did (AD-20), worded once for every surface that
// brings a session to front: the board card, the session file and Focus's
// rows (D-111). The record is exhaustive, so a new Rust outcome fails
// `pnpm check` here.
import type { FocusOutcome } from "./generated/FocusOutcome";
import type { ShownOutcome } from "./board.svelte";

export type OutcomeKind = FocusOutcome["outcome"];

export const OUTCOME_TEXT: Record<OutcomeKind, string> = {
  focused: "Window brought to front",
  attach_opened: "Attach console opened",
  candidates: "Several windows could match, none chosen:",
  no_window: "No window found for this session",
  refused: "Windows refused the switch; the window is flashing",
  unsupported: "Not supported on this platform",
  failed: "Could not open:",
};

/** The sentence an outcome shows; a failure adds its detail. */
export function outcomeText(outcome: FocusOutcome): string {
  return outcome.outcome === "failed" ? `${OUTCOME_TEXT.failed} ${outcome.detail}` : OUTCOME_TEXT[outcome.outcome];
}

/** The outcome to show for a card, none while the backend has not answered. */
export function visibleOutcome(outcomes: Record<string, ShownOutcome>, key: string): ShownOutcome | undefined {
  const shown = outcomes[key];
  return shown?.pending ? undefined : shown;
}
