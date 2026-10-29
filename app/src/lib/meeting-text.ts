// The meeting panel's words (AD-84, D-168): one per value of the core
// enums the payload carries, typed so a new variant fails `pnpm check`.
import type { AnswerRefusal } from "./generated/AnswerRefusal";
import type { AnswerResult } from "./generated/AnswerResult";
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

/** Send's labels: the button keeps the box of the longer (D-564). */
export const SEND_LABELS = { send: "Send", sending: "Sending" } as const;

/** Send the proposed answer's labels, one box for both (AD-182, D-564). */
export const SEND_PROPOSAL_LABELS = { send: "Send the proposed answer", sending: "Sending" } as const;

export const ANSWER_REFUSAL_TEXT: Record<AnswerRefusal, string> = {
  empty: "It is empty.",
  too_long: "It is longer than an answer may be.",
  control_character: "It holds a control character other than a line break or a tab.",
  format_character: "It holds an invisible or bidirectional formatting character.",
  leading_slash: "It starts with /, which Claude Code would read as a command.",
  leading_bang: "It starts with !, which Claude Code would run as a shell command.",
};

/** What became of an answer, as the composer says it. */
export function answerResultText(result: AnswerResult): string {
  switch (result.outcome) {
    case "confirmed":
      return "Sent: the session took it as its next turn.";
    case "not_confirmed":
      return "Not confirmed: the session was not seen working within 30 s. Your text is still here and on the clipboard.";
    case "attached":
      return "Copied, and claude attach opened: paste it there once the session asks.";
    case "copied":
      return "Copied: resume the session by hand in its folder and paste it.";
    case "spawn_failed":
      return `Not sent: the console could not start (${result.value}). Your text is still here and on the clipboard.`;
    case "refused":
      switch (result.value.kind) {
        case "state_changed":
          return "Not sent: the session changed meanwhile; the meeting was read again.";
        case "no_composer":
          return "Not sent: this session runs in your terminal; answer it there.";
        case "unknown_session":
          return "Not sent: the session is no longer known.";
        case "proposal_not_pending":
          return "Not sent: the proposal is no longer pending; the meeting was read again.";
        case "answer":
          return "Not sent: ";
      }
  }
}

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
