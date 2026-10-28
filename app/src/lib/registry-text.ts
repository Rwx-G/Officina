// What the window says about the registry (AD-54, D-47, D-57). Each closed
// set comes from Rust and is mapped through an exhaustive `Record`, so a
// value added there fails `pnpm check` until it has a text here.
import type { AuthorRefusal } from "./generated/AuthorRefusal";
import type { Column } from "./generated/Column";
import type { ConfirmRefusal } from "./generated/ConfirmRefusal";
import type { DryRunStop } from "./generated/DryRunStop";
import type { EndHow } from "./generated/EndHow";
import type { GateState } from "./generated/GateState";
import type { ModelVerdict } from "./generated/ModelVerdict";
import type { NotNeeded } from "./generated/NotNeeded";
import type { ProposalSource } from "./generated/ProposalSource";
import type { ReasonCode } from "./generated/ReasonCode";
import type { RegistryState } from "./generated/RegistryState";
import type { SubjectStatus } from "./generated/SubjectStatus";
import type { TriageRefusal } from "./generated/TriageRefusal";
import type { Verdict } from "./generated/Verdict";

/** The Board view's banner per state; `null` where nothing needs saying.
 *  The window creates the registry before its first board, so only a board
 *  printed by `officina board --json` can say `absent`. */
export const REGISTRY_STATE_TEXT: Record<RegistryState, string | null> = {
  ready: null,
  absent: null,
  older: "The registry was written by an older Officina: the window migrates it when it starts.",
  newer:
    "The registry was written by a newer Officina: it is read only, so nothing is recorded and no event file is deleted.",
  unavailable:
    "The registry is unavailable: nothing is recorded and no event file is deleted until the window starts again.",
  network:
    "The registry is on a network folder, where it cannot work: nothing is recorded and no event file is deleted. Set OFFICINA_HOME to a local folder.",
};

/** Why the registry refused an author action; nothing was changed. */
export const AUTHOR_REFUSAL_TEXT: Record<AuthorRefusal, string> = {
  name_empty: "The name is empty.",
  name_too_long: "The name is too long.",
  name_has_forbidden_characters: "The name holds a control or hidden formatting character.",
  name_taken: "Another open subject of this project already has this name.",
  unknown_subject: "This subject no longer exists.",
  unknown_session: "The registry does not hold this session.",
  unknown_project: "The registry knows no session or subject of this project.",
  no_session: "No session is selected.",
  sessions_of_several_projects: "The selected sessions belong to several projects: a subject has one.",
  archived: "This subject is archived: reopen it first.",
  not_archived: "Only an archived subject can be reopened.",
  merged: "This subject was merged into another, where its sessions are.",
  same_subject: "A subject cannot be merged into itself.",
};

/** A subject's status as the Subjects view words it. */
export const SUBJECT_STATUS_TEXT: Record<SubjectStatus, string> = {
  active: "active",
  to_resume: "to resume",
  archived: "archived",
};

/** A live session's state, from the board column its card is in. */
export const LIVE_STATE_TEXT: Record<Column, string> = {
  needs_you: "needs you",
  your_turn: "your turn",
  working: "working",
  to_resume: "to resume",
  done: "done",
};

/** How an ended session ended. */
export const END_TEXT: Record<EndHow, string> = {
  session_end: "ended",
  interrupted: "interrupted",
  state: "finished",
};

/** Why a proposal says what it says (AD-40, AD-54): a reason code, never
 *  prose, worded here. */
export const REASON_TEXT: Record<ReasonCode, string> = {
  state_done: "Claude Code reports the background session done.",
  state_failed: "Claude Code reports the background session failed.",
  state_stopped: "Claude Code reports the background session stopped.",
  cut_mid_turn: "The session ended in the middle of a turn: the last prompt got no answer.",
  default_to_resume: "Nothing says it is finished: to resume by default.",
  empty_subject: "No session of this subject ever saw a prompt.",
  model: "The triage model proposes it.",
};

/** Where a proposal came from (FR-8). */
export const PROPOSAL_SOURCE_TEXT: Record<ProposalSource, string> = {
  state: "from Claude Code's state",
  rules: "from Officina's rules",
  model: "from the triage model",
};

/** A proposed verdict, `unsure` included (D-59). */
export const PROPOSED_VERDICT_TEXT: Record<ModelVerdict, string> = {
  finished: "finished",
  to_resume: "to resume",
  unsure: "unsure",
};

/** A verdict the author confirms. */
export const VERDICT_TEXT: Record<Verdict, string> = {
  finished: "Finished",
  to_resume: "To resume",
};

/** Why one confirmed row was refused; nothing was written for it. */
export const CONFIRM_REFUSAL_TEXT: Record<ConfirmRefusal, string> = {
  row_gone: "This row no longer needs a verdict.",
  proposal_changed: "The proposal changed since it was shown: read it again before confirming.",
  no_subject: "Choose the subject to file this session under, or name a new one.",
};

/** Why a filed session's verdict was left to its subject's own row (D-60). */
export const NOT_NEEDED_TEXT: Record<NotNeeded, string> = {
  not_latest: "a later session of that subject decides its verdict",
  live_session: "a session of that subject is still running",
};

/** Why a triage call was refused before anything started (AD-51, D-65). */
export const TRIAGE_REFUSAL_TEXT: Record<TriageRefusal, string> = {
  claude_not_found: "claude was not found",
  cmd_shim: "claude resolves to a .cmd or .bat shim, which the call refuses: install the native claude.exe",
  no_excerpts: "no_excerpts is set, so nothing may be sent",
  workdir_not_empty: "the empty folder the call runs in holds something",
};

/** How the last dry run ended (D-67). */
export const DRY_RUN_STOP_TEXT: Record<DryRunStop, string> = {
  completed: "The last dry run ran every case.",
  budget_spent: "The last dry run stopped when the call budget was spent; what ran is recorded.",
  refused: "The last dry run stopped on a refused call; its cause is listed below.",
};

/** A cost in millionths of a dollar, as the evidence shows it. */
export function usd(microusd: number | null): string {
  return microusd === null ? "unknown" : `USD ${(microusd / 1_000_000).toFixed(4)}`;
}

/** Why a gate does not hold, or that it does (AD-50, D-69). */
export const GATE_STATE_TEXT: Record<GateState, string> = {
  holds: "its evidence holds",
  not_enough_cases: "not enough confirmed cases yet",
  too_many_flips: "you flipped too many of its finished proposals",
  no_dry_run: "no dry run yet",
  dry_run_too_old: "the latest dry run is too old",
  dry_run_too_small: "the latest dry run ran too few cases",
  model_not_better: "the latest dry run did not beat the default",
  worse_than_default: "the model was right less often than to resume would have been",
  needs_newer_dry_run: "the model turned itself off after the latest dry run: run a new one",
};
