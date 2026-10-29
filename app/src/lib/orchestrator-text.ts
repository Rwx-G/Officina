// The Orchestrator view's wording, one exhaustive `Record` per closed enum
// of the payload: a value added in Rust fails `pnpm check` here until it is
// worded.
import type { CardState } from "./generated/CardState";
import type { ChangedItem } from "./generated/ChangedItem";
import type { Column } from "./generated/Column";
import type { Cursor } from "./generated/Cursor";
import type { IssueRefusal } from "./generated/IssueRefusal";
import type { LaunchOutcome } from "./generated/LaunchOutcome";
import type { LaunchRefusal } from "./generated/LaunchRefusal";
import type { RunKind } from "./generated/RunKind";
import type { SessionPhase } from "./generated/SessionPhase";
import type { SubjectRefusal } from "./generated/SubjectRefusal";
import type { SubjectStatus } from "./generated/SubjectStatus";
import type { TemplateChange } from "./generated/TemplateChange";
import type { TemplateUnavailable } from "./generated/TemplateUnavailable";
import type { WorkOrderRefusal } from "./generated/WorkOrderRefusal";
import type { WorkOrderSource } from "./generated/WorkOrderSource";
import { AUTHOR_REFUSAL_TEXT } from "./registry-text";
import { clock } from "./time";

export const PHASE_TEXT: Record<SessionPhase, string> = {
  needs_you: "needs you",
  your_turn: "your turn",
  working: "working",
  to_resume: "to resume",
  done: "done",
  ended: "ended",
  unknown: "state unknown",
};

export const WAITING_COLUMN_TEXT: Record<Column, string> = {
  needs_you: "Needs you",
  your_turn: "Your turn",
  working: "Working",
  to_resume: "To resume",
  done: "Done",
};

export const RUN_KIND_TEXT: Record<RunKind, string> = {
  failed: "failed",
  abandoned: "abandoned",
  refused: "refused",
  gate_denied: "denied by the gate",
  gate_error: "gate error",
};

export const OUTCOME_TEXT: Record<LaunchOutcome, string> = {
  launching: "launching",
  started: "started",
  untrusted_workspace: "workspace not trusted",
  failed: "failed",
  unknown: "outcome unknown",
};

export const SUBJECT_STATUS_TEXT: Record<SubjectStatus, string> = {
  active: "active",
  to_resume: "to resume",
  archived: "archived",
};

export const TEMPLATE_CHANGE_TEXT: Record<TemplateChange, string> = {
  added: "added",
  changed: "changed",
  removed: "removed",
};

export const SOURCE_TEXT: Record<WorkOrderSource, string> = {
  composer: "composer",
  proposal: "proposal",
};

/** Since when `changed` counts: the cursor's time, or the first reading. */
export function sinceText(cursor: Cursor, nowMs: number): string {
  switch (cursor.kind) {
    case "read_at":
      return `since your last reading, ${clock(cursor.at_ms, nowMs)}`;
    case "previous_start":
      return `since the previous orchestrator start, ${clock(cursor.at_ms, nowMs)}`;
    case "none":
      return "first reading: everything is new";
  }
}

/** A change's verb, after its subject. */
export function changeText(item: ChangedItem): string {
  switch (item.kind) {
    case "run_finished":
      return item.succeeded ? "ran" : "failed";
    case "started_waiting":
      return `began to wait (${WAITING_COLUMN_TEXT[item.column]})`;
    case "subject_status":
      return `is now ${SUBJECT_STATUS_TEXT[item.status]}`;
    case "order_launched":
      return `launched, ${OUTCOME_TEXT[item.outcome]}`;
  }
}

/** Why core refused a launch (AD-78): nothing started, nothing retries. */
export const LAUNCH_REFUSAL_TEXT: Record<LaunchRefusal, string> = {
  unknown_card: "Officina no longer knows this card: compose the order again.",
  superseded: "A newer card replaced this one: launch that one.",
  consumed: "This card was already launched: a card launches once.",
  prompt_hash_mismatch: "The prompt shown is not the one that would run: compose the order again.",
  catalogue_changed: "The catalogue changed since this card was issued: review it in the Relay view, then compose again.",
  template_not_approved: "The approved catalogue no longer holds this template.",
  claude_not_executable: "No native claude executable was found: see officina doctor.",
  project_folder_missing: "The project's folder is not a folder any more.",
  subject_closed: "The subject this card files under was archived or merged since: choose another.",
  name_taken: "The registry already holds this session's name: compose the order again.",
};

const TEMPLATE_UNAVAILABLE_TEXT: Record<TemplateUnavailable, string> = {
  catalogue_changed: "the catalogue changed since its approval: review it in the Relay view",
  not_approved: "the template is not in the approved catalogue",
};

const WORK_ORDER_REFUSAL_TEXT: Record<WorkOrderRefusal, string> = {
  project_not_allowed: "the template does not run in this project",
  unknown_parameter: "the template has no such parameter",
  duplicate_parameter: "a parameter was given twice",
  missing_parameter: "a parameter has no value",
  value_not_in_list: "a value is not in its parameter's list",
  unrenderable_prompt: "the prompt cannot be rendered with these values",
  prompt_too_long: "the rendered prompt is too long",
  prompt_start: "the rendered prompt starts with a character claude would read as an option",
};

const SUBJECT_REFUSAL_TEXT: Record<SubjectRefusal, string> = {
  unknown_subject: "the subject no longer exists",
  subject_closed: "the subject is archived or merged",
  other_project: "the subject belongs to another project",
};

const CARD_STATE_TEXT: Record<CardState, string> = {
  open: "open",
  superseded: "replaced by a newer card",
  consumed: "already launched",
};

/** Why no card was issued, or an edit was refused (D-664). */
export function issueRefusalText(refusal: IssueRefusal): string {
  switch (refusal.kind) {
    case "template":
      return `No card: ${TEMPLATE_UNAVAILABLE_TEXT[refusal.detail]}.`;
    case "order":
      return `No card: ${WORK_ORDER_REFUSAL_TEXT[refusal.detail]}.`;
    case "subject":
      return `No card: ${SUBJECT_REFUSAL_TEXT[refusal.detail]}.`;
    case "subject_name":
      return `No card: ${AUTHOR_REFUSAL_TEXT[refusal.detail]}`;
    case "unknown_card":
      return LAUNCH_REFUSAL_TEXT.unknown_card;
    case "card_not_open":
      return `This card is ${CARD_STATE_TEXT[refusal.detail]}: compose the order again.`;
  }
}

/** What a launch's outcome tells the author on its card (FR-4, AD-81);
 *  the session, the exit code and the error come from the history's row
 *  for the card. */
export function launchOutcomeText(outcome: LaunchOutcome, cwd: string): string {
  switch (outcome) {
    case "launching":
      return "Launched: its outcome has not come back yet; the history shows it when it does.";
    case "started":
      return "Started in the background.";
    case "untrusted_workspace":
      return `Not started: claude has never been trusted in ${cwd}. Open claude once in that folder and accept its trust dialog, then compose the order again. Officina does not retry.`;
    case "failed":
      return "Failed: claude did not start the session. Officina does not retry.";
    case "unknown":
      return "Outcome unknown: the launch was recorded, its end was not. The history shows what the registry knows.";
  }
}
