// What the Relay view says about each journal entry (AD-35). Every Rust
// vocabulary the view shows is mapped through an exhaustive `Record`, so a
// variant added on the Rust side fails `pnpm check` until it has a text. No
// relay rule is computed here: the entries come paired and dated from Rust.
import type { ActionIssue } from "./generated/ActionIssue";
import type { AnomalyKind } from "./generated/AnomalyKind";
import type { ApprovalView } from "./generated/ApprovalView";
import type { AuthorReason } from "./generated/AuthorReason";
import type { DenyKind } from "./generated/DenyKind";
import type { GateFault } from "./generated/GateFault";
import type { IssueTable } from "./generated/IssueTable";
import type { RecentEntry } from "./generated/RecentEntry";
import type { RefusalReason } from "./generated/RefusalReason";
import type { RunOutcome } from "./generated/RunOutcome";
import type { RunState } from "./generated/RunState";
import type { Tone } from "./generated/Tone";
import type { TreeBinding } from "./generated/TreeBinding";

/** The stop toggle's label in each relay state, in the header and the
 *  Relay view alike: it names what a click does. */
export const RELAY_TOGGLE_TEXT = { stopped: "Resume relay", running: "Stop relay" } as const;

export const REFUSAL_TEXT: Record<RefusalReason, string> = {
  stopped: "the relay is stopped",
  not_approved: "no catalogue approved",
  approved_unreadable: "approved catalogue unreadable",
  decision_record_unreadable: "the window's record cannot vouch",
  catalogue_unconfirmed: "catalogue not approved in the window",
  floor_unreadable: "relay floor unreadable",
  catalogue_changed: "catalogue changed, approval pending",
  outside_projects: "outside every project",
  action_not_allowed: "action not listed for the project",
  missing_parameter: "missing parameter",
  unknown_parameter: "unknown parameter",
  invalid_value: "invalid value",
  git_unavailable: "no git to bind the files",
  tree_too_large: "work tree too large to bind",
  tree_unreadable: "work tree could not be read",
  check_required: "its check has not run",
  approval_rejected: "rejected in Officina",
  approval_unavailable: "request could not be written",
  approval_pending: "approval still pending",
  approval_not_given: "approval not given in the window",
  check_not_satisfied: "code changed since the dry run",
  tree_changed: "files changed since the request",
  credential_missing: "credential not in the Credential Manager",
  credential_unreadable: "credential cannot be read",
};

/** The buttons an `auto` rejection asks with (FR-9, AD-86): only "should
 *  not run" against a review's approve is a disagreement for the gate. */
const REJECT_REASON_TEXT: Record<AuthorReason, string> = {
  should_not_run: "should not run",
  not_now: "not now",
};

export const REJECT_REASONS = Object.entries(REJECT_REASON_TEXT) as [AuthorReason, string][];

/** What an issue's table is called before the entry's name (D-132); a key
 *  or a single table is named by itself. */
const ISSUE_TABLE_TEXT: Record<IssueTable, string | null> = {
  action: "action",
  credential: "credential",
  project: "project",
  template: "template",
  guarded_programs: null,
  orchestrator: null,
  auto_review: null,
};

/** What a floor issue is about, as the floor names it: `template bump`,
 *  `action probe`, `auto_review`. */
export function issueSubject(issue: ActionIssue): string {
  const table = ISSUE_TABLE_TEXT[issue.table];
  return table === null ? issue.name : `${table} ${issue.name}`;
}

export const DENY_TEXT: Record<DenyKind, string> = {
  unparsable: "command could not be read",
  guarded_program: "guarded program",
  encoded_command: "encoded command",
  dynamic: "dynamic command name",
  home_redirection: "home redirected",
  officina_environment: "officina's environment changed",
  officina_variable: "names an OFFICINA_ variable",
  protected_dir: "names the relay's files",
  powershell_in_guarded_project: "PowerShell in a guarded project",
  relay_state_write: "writes Officina's files",
};

export const GATE_FAULT_TEXT: Record<GateFault, string> = {
  approved_unreadable: "approved catalogue unreadable",
  approved_missing: "approved catalogue missing",
  floor_unreadable: "relay floor unreadable",
  floor_invalid: "relay floor invalid",
  floor_missing: "relay floor missing",
  known_projects_unreadable: "known projects unreadable",
  known_projects_unwritable: "known projects not written",
  panic: "internal panic",
};

export const RUN_OUTCOME_TEXT: Record<RunOutcome, string> = {
  exited: "exit",
  timeout: "timed out",
  spawn_failed: "could not start",
  http: "http",
  connect_failed: "no response",
};

/** What each decision the window did not make is (D-173). */
export const ANOMALY_TEXT: Record<AnomalyKind, string> = {
  foreign_approval: "approval not given here",
  run_without_decision: "run without an approval given here",
  foreign_catalogue: "catalogue not approved here",
  stop_flag_removed: "relay resumed outside the window",
  record_broken: "the window's record is missing or unreadable",
};

/** What a request's files are compared with before its run (D-173). */
const BINDING_SINCE: Record<Exclude<TreeBinding, "not_git_tree">, string> = {
  dry_run: "the dry run",
  request: "the request",
};

/** What the card says of a request's files (D-170, D-173): bound and
 *  unchanged, changed, bound but not checked yet, or not bound at all.
 *  `changed` marks the line the run will refuse. Null for an invalid
 *  request, which binds nothing. */
export function bindingText(view: ApprovalView): { text: string; changed: boolean } | null {
  const binding = view.binding;
  if (binding === null) return null;
  if (binding === "not_git_tree") return { text: "no file binding: not a git tree", changed: false };
  const since = BINDING_SINCE[binding];
  if (view.files_unchanged === true) {
    return { text: `tracked and unignored files unchanged since ${since}`, changed: false };
  }
  if (view.files_unchanged === false) {
    return {
      text: `tracked or unignored files changed since ${since}: the run will refuse this approval`,
      changed: true,
    };
  }
  return { text: `tracked and unignored files bound to ${since}`, changed: false };
}

export const RUN_STATE_TEXT: Record<RunState, string> = {
  running: "running",
  finished: "finished",
  abandoned: "abandoned",
};

/** How an entry reads in a row: `fault` marks what failed or was refused,
 *  `abandoned` a run that never reported its end. */
export type RowTone = "ok" | "fault" | "abandoned" | "neutral";

/** The row class of each tone Rust computes for an entry. */
export const TONE_CLASS: Record<Tone, RowTone> = {
  ok: "ok",
  failed: "fault",
  refused: "fault",
};

/** One row of the recent activity, every field as text. */
export interface ActivityRow {
  project: string;
  action: string;
  params: [string, string][];
  outcome: string;
  /** Text that came with the entry (a refusal's detail, a command excerpt),
   *  shown as text. */
  detail: string;
  durationMs: number | null;
  tone: RowTone;
}

function entries(params: { [key in string]?: string } | null | undefined): [string, string][] {
  return Object.entries(params ?? {}).map(([key, value]) => [key, value ?? ""]);
}

/** The row of one recent entry (AD-35: time, project, action, params,
 *  outcome, duration, "abandoned" when so). */
export function activityRow(entry: RecentEntry): ActivityRow {
  const event = entry.event;
  const tone: RowTone = entry.tone === null ? "neutral" : TONE_CLASS[entry.tone];
  const row: ActivityRow = { project: "", action: "", params: [], outcome: "", detail: "", durationMs: null, tone };
  switch (event.kind) {
    case "run": {
      const { started, finished, state } = event;
      row.project = started.project;
      row.action = started.action;
      row.params = entries(started.params);
      if (state !== "finished" || !finished) {
        row.outcome = RUN_STATE_TEXT[state];
        if (state === "abandoned") row.tone = "abandoned";
        return row;
      }
      row.durationMs = finished.duration_ms;
      if (finished.outcome === "exited") {
        row.outcome = `${RUN_OUTCOME_TEXT.exited} ${finished.exit_code ?? "?"}`;
      } else if (finished.outcome === "http") {
        row.outcome = `${RUN_OUTCOME_TEXT.http} ${finished.status ?? "?"}`;
      } else {
        row.outcome = RUN_OUTCOME_TEXT[finished.outcome];
      }
      row.detail = finished.detail ?? "";
      return row;
    }
    case "refused":
      row.project = event.project ?? "";
      row.action = event.action;
      row.params = entries(event.params);
      row.outcome = `refused: ${REFUSAL_TEXT[event.reason]}`;
      row.detail = event.detail;
      return row;
    case "gate_denied":
      row.project = event.project;
      row.action = "gate";
      row.outcome = `denied: ${DENY_TEXT[event.deny]}${event.matched ? ` ${event.matched}` : ""}`;
      row.detail = event.command_excerpt ?? "";
      return row;
    case "gate_error":
      row.project = event.project ?? "";
      row.action = "gate";
      row.outcome = `gate error: ${GATE_FAULT_TEXT[event.cause]}`;
      row.detail = event.detail;
      return row;
    case "catalogue_approved":
      row.action = "catalogue";
      row.outcome = "approved";
      row.detail = event.hash;
      return row;
    case "approval_requested":
    case "approval_given":
    case "approval_rejected": {
      const verb = { approval_requested: "requested", approval_given: "approved", approval_rejected: "rejected" };
      row.action = "approval";
      row.outcome = verb[event.kind];
      row.detail = event.approval_id;
      return row;
    }
    case "relay_stopped":
      row.action = "relay";
      row.outcome = "stopped";
      return row;
    case "relay_resumed":
      row.action = "relay";
      row.outcome = "resumed";
      return row;
  }
}
