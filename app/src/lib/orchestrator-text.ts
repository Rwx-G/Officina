// The Orchestrator view's wording, one exhaustive `Record` per closed enum
// of the payload: a value added in Rust fails `pnpm check` here until it is
// worded.
import type { ChangedItem } from "./generated/ChangedItem";
import type { Column } from "./generated/Column";
import type { Cursor } from "./generated/Cursor";
import type { LaunchOutcome } from "./generated/LaunchOutcome";
import type { RunKind } from "./generated/RunKind";
import type { SessionPhase } from "./generated/SessionPhase";
import type { SubjectStatus } from "./generated/SubjectStatus";
import type { TemplateChange } from "./generated/TemplateChange";
import type { WorkOrderSource } from "./generated/WorkOrderSource";
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
