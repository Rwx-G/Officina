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
import type { LiveStatus } from "./generated/LiveStatus";
import type { ProposalContent } from "./generated/ProposalContent";
import type { ProposalInvalidReason } from "./generated/ProposalInvalidReason";
import type { ProposalKind } from "./generated/ProposalKind";
import type { ProposalRefusal } from "./generated/ProposalRefusal";
import type { ProposalStatus } from "./generated/ProposalStatus";
import type { StartAnswer } from "./generated/StartAnswer";
import type { StartRefusal } from "./generated/StartRefusal";
import type { RunKind } from "./generated/RunKind";
import type { SessionPhase } from "./generated/SessionPhase";
import type { SubjectRefusal } from "./generated/SubjectRefusal";
import type { SubjectStatus } from "./generated/SubjectStatus";
import type { TargetView } from "./generated/TargetView";
import type { TemplateChange } from "./generated/TemplateChange";
import type { TemplateUnavailable } from "./generated/TemplateUnavailable";
import type { WorkOrderCard } from "./generated/WorkOrderCard";
import type { WorkOrderPermission } from "./generated/WorkOrderPermission";
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

/** The orchestrator's live status, as its agents entry gives it. */
export const LIVE_STATUS_TEXT: Record<LiveStatus, string> = {
  busy: "working",
  waiting: "waiting",
  idle: "idle",
};

/** Why a Start was refused (AD-89), and what to do about it. */
export const START_REFUSAL_TEXT: Record<StartRefusal, string> = {
  claude_not_found: "Refused: claude was not found. Install it or set OFFICINA_CLAUDE_BIN.",
  claude_not_exe:
    "Refused: claude is not a native claude.exe, and Officina runs no other. Install the native claude.exe.",
  workdir_not_empty:
    "Refused: the orchestrator's folder is not empty. Officina never deletes it; empty it yourself (officina doctor names it).",
  officina_exe_missing:
    "Refused: officina.exe is missing beside the window's executable; the orchestrator's tools run it. Reinstall Officina.",
  registry_unavailable: "Refused: the registry cannot record the start now (see the board's diagnostics).",
  unsupported: "Refused: the orchestrator opens in a console of its own, on Windows only.",
};

/** What the author is told after a Start, beside the button. */
export function startAnswerText(answer: StartAnswer): string {
  switch (answer.outcome) {
    case "started":
      return "Started in a console of its own. Accept its folder there if Claude Code asks, then type any first message: it reads the briefing before it answers.";
    case "already_running":
      switch (answer.detail.outcome) {
        case "focused":
          return "Already running: its console is in front.";
        case "candidates":
          return `Already running: several windows carry its name (${answer.detail.detail.join(", ")}); bring the right one forward yourself.`;
        case "refused":
          return "Already running: Windows did not bring its console forward, so it flashes in the taskbar.";
        default:
          return "Already running: no window carries its name; find its console in the taskbar.";
      }
    case "refused":
      return START_REFUSAL_TEXT[answer.detail];
    case "failed":
      return `Could not start: ${answer.detail}`;
  }
}

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
    case "proposal_received":
      return `received, ${PROPOSAL_STATUS_TEXT[item.status]}`;
  }
}

/** A proposal's kind; a file that did not parse has none (D-133). */
export const PROPOSAL_KIND_TEXT: Record<ProposalKind, string> = {
  recap: "Recap",
  work_order: "Work order",
  answer: "Answer",
};

/** The kind of a proposal, or of a file that did not parse. */
export function proposalKindText(kind: ProposalKind | null): string {
  return kind === null ? "Unreadable file" : PROPOSAL_KIND_TEXT[kind];
}

/** Where a proposal stands (AD-91, D-145). */
export const PROPOSAL_STATUS_TEXT: Record<ProposalStatus, string> = {
  pending: "pending",
  invalid: "invalid",
  accepted: "accepted: a card from it launched",
  dismissed: "dismissed",
  expired: "expired unopened",
  superseded: "superseded by a newer recap",
};

/** Why the window stored a proposal invalid (AD-91, D-155). */
export const PROPOSAL_INVALID_TEXT: Record<ProposalInvalidReason, string> = {
  unparsable: "the file is not a proposal this version reads",
  oversized: "the file is larger than a proposal may be, so it was not read",
  text_too_long: "a text is over its length",
  forbidden_characters: "a text holds a control, bidirectional or invisible character",
  unknown_session: "the session it names is not in the registry",
  unknown_subject: "the subject it names is not in the registry",
  subject_closed: "the subject it names is archived or merged",
  subject_other_project: "the subject it names belongs to another project",
  subject_name_invalid: "the new subject's name it suggests is not one a subject can have",
  unknown_template: "the approved catalogue holds no such template",
  project_not_allowed: "the template does not run in that project",
  unknown_parameter: "it gives a parameter the template does not have",
  missing_parameter: "a parameter of the template has no value",
  value_not_in_list: "a value is not in its parameter's list",
  rationale_missing: "the answer it proposes gives no rationale",
  answer_refused: "the answer it proposes breaks a rule your own answers follow",
  session_not_answerable: "the session it names was not a background session idle in its turn",
};

/** The invalid reason the row carries, worded; a code this window does
 *  not know is shown as the code it is. */
export function invalidReasonText(reason: string): string {
  return reason in PROPOSAL_INVALID_TEXT ? PROPOSAL_INVALID_TEXT[reason as ProposalInvalidReason] : reason;
}

/** Why no card opens from a proposal (AD-91). */
const PROPOSAL_REFUSAL_TEXT: Record<ProposalRefusal, string> = {
  unknown_proposal: "the registry no longer holds this proposal",
  not_a_work_order: "a recap has nothing to launch",
  not_pending: "the proposal is no longer pending",
  expired: "the proposal expired unopened",
};

/** Why core refused a launch (AD-78): nothing started, nothing retries. */
export const LAUNCH_REFUSAL_TEXT: Record<LaunchRefusal, string> = {
  unknown_card: "Officina no longer knows this card: compose the order again.",
  superseded: "A newer card replaced this one: launch that one.",
  consumed: "This card was already launched: a card launches once.",
  prompt_hash_mismatch: "The prompt shown is not the one that would run: compose the order again.",
  catalogue_changed: "The catalogue changed since this card was issued: review it in the Relay view, then compose again.",
  permission_changed: "This project's work order permission changed since this card was issued: compose the order again.",
  template_not_approved: "The approved catalogue no longer holds this template.",
  claude_not_executable: "No native claude executable was found: see officina doctor.",
  project_folder_missing: "The project's folder is not a folder any more.",
  subject_closed: "The subject this card files under was archived or merged since: choose another.",
  name_taken: "The registry already holds this session's name: compose the order again.",
};

/** Where a work order's permission mode comes from (story 5.24, D-980). */
export const PERMISSION_TEXT: Record<WorkOrderPermission, string> = {
  project: "the project's own mode",
  officina: "Officina's default, auto mode",
};

/** The tooltip under the permission line: what the mode is now and what
 *  the switch would change (D-982, D-986). */
export const PERMISSION_TIP_TEXT: Record<WorkOrderPermission, string> = {
  project:
    "This project's work orders start in the permission mode its own settings set, else your default, as a session you start there would; one that stops at a permission prompt waits in Needs you. Officina's default runs them in Claude Code's auto mode, whose classifier decides each tool call; never bypass.",
  officina:
    "This project's work orders start in Claude Code's auto mode, Officina's default, whatever the project's settings say: its classifier decides each tool call. Going back gives them the project's own mode again.",
};

/** The switch's label, by the permission it moves the project to. */
export const PERMISSION_SWITCH_TEXT: Record<WorkOrderPermission, string> = {
  officina: "Use Officina's default for this project",
  project: "Back to the project's mode",
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
    case "proposal":
      return `No card: ${PROPOSAL_REFUSAL_TEXT[refusal.detail]}.`;
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

/** The subject a card files its session under, as words. */
export function cardSubjectText(card: WorkOrderCard): string {
  const chosen = card.subject;
  if (chosen.kind === "new") return `a new subject, ${chosen.value}`;
  return card.subject_options.find((s) => s.id === chosen.value)?.name ?? `subject ${chosen.value}`;
}

/** The composer's status line (D-1001): which card its choices gave and
 *  where it files its session, so a choice shows its effect beside it. */
export function composerStatusText(card: WorkOrderCard): string {
  return `The card below is ready: ${card.name}, filed under ${cardSubjectText(card)}.`;
}

/** What follows a work order's folder when its session worked in another
 *  one than the order's (D-1002). */
export const SESSION_FOLDER_TEXT =
  "not in the project's folder: its changes are there, on that worktree's branch when Claude made one. Merging and removing a worktree is yours.";

/** What the orchestrator is, beside its Start (AD-89, R110). */
export const ORCHESTRATOR_ABOUT =
  "A claude session in a console of its own, whose only tools are Officina's: it reads and proposes, and acts on nothing.";

/** A count of pending proposals as words. */
export function pendingText(pending: number): string {
  return `${pending} ${pending === 1 ? "proposal" : "proposals"} pending`;
}

/** The view switch's Orchestrator button (D-565): its accessible name and
 *  tooltip, presence first, then what is pending. */
export function switchText(live: boolean, pending: number): string {
  const head = `Orchestrator: ${live ? "live" : "absent"}`;
  return pending > 0 ? `${head}, ${pendingText(pending)}` : head;
}

/** What a proposal is about, in a few words, for the decision office's
 *  whiteboard (D-567, D-831): a recap's or an answer's target by name, a
 *  work order's template and project. Every word comes from the
 *  orchestrator or from a session, so it renders as text only. `names`
 *  finds a session's name, `subjects` a subject's; an unknown one is named
 *  by its id. */
export function proposalLabel(
  content: ProposalContent,
  names: ReadonlyMap<string, string>,
  subjects: ReadonlyMap<number, string>,
): string {
  // One variant, one body: what a kind carries decides its label, so a
  // kind that names a target reads like a recap.
  const body: { template?: string; project?: string; target?: TargetView } = Object.values(content)[0]!;
  if (body.template !== undefined) return `${body.template}, ${body.project ?? ""}`;
  const target = body.target!;
  if ("subject_id" in target) return subjects.get(target.subject_id) ?? `subject ${target.subject_id}`;
  return names.get(target.session_id) ?? `session ${target.session_id}`;
}
