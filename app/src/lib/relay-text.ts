// What the Relay view says about each journal entry (AD-35). Every Rust
// vocabulary the view shows is mapped through an exhaustive `Record`, so a
// variant added on the Rust side fails `pnpm check` until it has a text. No
// relay rule is computed here: the entries come paired and dated from Rust.
import type { ActionIssue } from "./generated/ActionIssue";
import type { AnomalyKind } from "./generated/AnomalyKind";
import type { ApprovalBy } from "./generated/ApprovalBy";
import type { ApprovalView } from "./generated/ApprovalView";
import type { AuthorReason } from "./generated/AuthorReason";
import type { AutoMode } from "./generated/AutoMode";
import type { AutoOffReason } from "./generated/AutoOffReason";
import type { ReviewShown } from "./generated/ReviewShown";
import type { ShadowStats } from "./generated/ShadowStats";
import type { EscalationCause } from "./generated/EscalationCause";
import type { DenyKind } from "./generated/DenyKind";
import type { GateFault } from "./generated/GateFault";
import type { IssueTable } from "./generated/IssueTable";
import type { RecentEntry } from "./generated/RecentEntry";
import type { RefusalReason } from "./generated/RefusalReason";
import type { RunOutcome } from "./generated/RunOutcome";
import type { RunState } from "./generated/RunState";
import type { Tone } from "./generated/Tone";
import type { TreeBinding } from "./generated/TreeBinding";
import { hours } from "./time";

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

/** Why an `auto` request is the author's, as its card says it (AD-87).
 *  In shadow, the board only ever sends a cause found before any call or
 *  `auto_approval_off` before he decides (D-143); in live and rule modes
 *  it sends the cause itself. */
export const ESCALATION_TEXT: Record<EscalationCause, string> = {
  rule_baseline_false: "the rule baseline does not hold: no recent check on these files, or the last run failed",
  approval_refused: "the review approved it, but the request changed, expired or was decided meanwhile",
  claude_not_found: "no claude binary was found, so no review ran",
  claude_shim: "claude is not the native claude.exe, so no review ran",
  workdir_not_empty: "the review's empty folder holds a file, so no review ran",
  no_session_id: "the request names no session, so no review ran",
  session_unbound:
    "the session the request names did not make it, or is a background session Officina did not launch, so no review ran",
  no_excerpts: "excerpts are off, so nothing may be sent to a review",
  quota_high: "the 5-hour window is nearly spent, so no review ran",
  hourly_cap: "this action reached its approvals for the hour",
  budget_spent: "the review's own budget is spent for now",
  review_config_invalid: "the [auto_review] table fails validation",
  call_failed: "the review call failed",
  call_timeout: "the review did not answer in time",
  invalid_answer: "the review's answer did not fit its schema",
  review_rejected: "the review rejected it",
  review_escalated: "the review left it to you",
  auto_approval_off: "auto approval is off; its review runs in shadow and approves nothing",
};

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
  workshop: null,
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

/** What a review said, once the author decided (D-143): its decision or
 *  how its call ended, the rule baseline, and its reason when the payload
 *  carries one (the window's; `officina relay --json` never does). */
export function reviewText(review: ReviewShown): string {
  const said = review.decision ? `review: ${review.decision}` : `review: ${review.outcome}`;
  const rule = review.rule_baseline ? "rule baseline held" : "rule baseline did not hold";
  return review.reason ? `${said} (${rule}): ${review.reason}` : `${said} (${rule})`;
}

/** Auto approval's toggle, in each state: it names what a click does. */
export const AUTO_APPROVAL_TOGGLE_TEXT = {
  on: "Turn auto approval off",
  off: "Turn auto approval on",
} as const;

/** The mode toggle, offered with the rule offer: it names what a click
 *  does. */
export const AUTO_MODE_TOGGLE_TEXT: Record<AutoMode, string> = {
  review: "Decide by the rule instead",
  rule: "Decide by the review again",
};

/** What each mode does, in a sentence. */
export const AUTO_MODE_TEXT: Record<AutoMode, string> = {
  review: "a review's approve runs the action",
  rule: "the rule baseline decides, without any model call",
};

/** Why auto approval went off when a mark turned it off. */
export const AUTO_OFF_TEXT: Record<AutoOffReason, string> = {
  should_not_have_run: "a run was marked as one that should not have run",
};

/** Who approved an auto-approved run. */
export const APPROVED_BY_TEXT: Record<ApprovalBy, string> = {
  author: "you",
  review: "its review",
  rule: "the rule",
};

/** The weakness of the evidence, from the payload's numbers only (AD-5):
 *  `n` clean cases bound the false-approve rate to 3/n by the rule of
 *  three. */
export function boundText(stats: ShadowStats): string {
  if (stats.false_approve_bound_percent === null) {
    return "No shadow case met your decision yet: nothing bounds how often a review would approve wrongly.";
  }
  const bound = Math.round(stats.false_approve_bound_percent);
  return `${stats.cases} clean ${stats.cases === 1 ? "case bounds" : "cases bound"} the rate of wrong approvals only to about ${bound} %, not to zero.`;
}

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
      row.outcome = event.by && event.by !== "author" ? `${verb[event.kind]} by the ${event.by}` : verb[event.kind];
      row.detail = entry.review ? `${event.approval_id}\n${reviewText(entry.review)}` : event.approval_id;
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
    case "auto_mode_denied":
      row.action = "auto mode";
      row.outcome = event.tool_name ? `denied ${event.tool_name}` : "denied";
      row.detail = event.reason;
      return row;
  }
}

/** A session's auto-mode denials, as neutral text (AD-181, story 5.23):
 *  "2 auto-mode denials in the last 24 h", over the window the backend
 *  sends. */
export function denialsText(count: number, windowMs: number): string {
  return `${count} auto-mode ${count === 1 ? "denial" : "denials"} in the last ${hours(windowMs)}`;
}
