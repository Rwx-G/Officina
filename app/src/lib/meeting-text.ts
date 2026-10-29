// The meeting panel's words (AD-84, D-168): one per value of the core
// enums the payload carries, typed so a new variant fails `pnpm check`.
import type { Changes } from "./generated/Changes";
import type { Delivery } from "./generated/Delivery";
import type { MeetingRun } from "./generated/MeetingRun";

export const DELIVERY_TEXT: Record<Delivery, string> = {
  resume_live: "An answer goes to it as its next turn (claude --resume), then its console opens.",
  resume_ended: "An answer resumes it in its folder as its next turn (claude --resume), then its console opens.",
  copy_and_attach: "Claude Code sends it no prompt while it waits: an answer is copied and claude attach opens.",
  copy_only:
    "Claude Code reported no folder for it: an answer is copied and nothing starts; resume it by hand in its folder.",
  focus_only: "It runs in your terminal: answer it there.",
};

export const CHANGES_TEXT: Record<Exclude<Changes["state"], "read" | "unavailable">, string> = {
  folder_unknown: "Folder unknown: claude agents reported none for this session, so no git ran.",
  outside_git: "Its folder is outside git.",
};

/** How one run went, as the panel says it. */
export function runResultText(run: MeetingRun): string {
  switch (run.result) {
    case "succeeded":
      return "succeeded";
    case "failed":
      return run.exit_code !== null ? `failed, exit ${run.exit_code}` : "failed";
    case "refused":
      return run.refusal ? `refused: ${run.refusal}` : "refused";
    case "unfinished":
      return "no end recorded";
  }
}
