// What the window says about the registry (AD-54, D-47, D-57). Each closed
// set comes from Rust and is mapped through an exhaustive `Record`, so a
// value added there fails `pnpm check` until it has a text here.
import type { AuthorRefusal } from "./generated/AuthorRefusal";
import type { Column } from "./generated/Column";
import type { EndHow } from "./generated/EndHow";
import type { RegistryState } from "./generated/RegistryState";
import type { SubjectStatus } from "./generated/SubjectStatus";

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
