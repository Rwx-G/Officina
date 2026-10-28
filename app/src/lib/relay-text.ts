// What the Relay view says about each journal entry (AD-35). Every Rust
// vocabulary the view shows is mapped through an exhaustive `Record`, so a
// variant added on the Rust side fails `pnpm check` until it has a text. No
// relay rule is computed here: the entries come paired and dated from Rust.
import type { DenyKind } from "./generated/DenyKind";
import type { GateFault } from "./generated/GateFault";
import type { RecentEntry } from "./generated/RecentEntry";
import type { RefusalReason } from "./generated/RefusalReason";
import type { RunOutcome } from "./generated/RunOutcome";
import type { RunState } from "./generated/RunState";

export const REFUSAL_TEXT: Record<RefusalReason, string> = {
  stopped: "the relay is stopped",
  not_approved: "no catalogue approved",
  approved_unreadable: "approved catalogue unreadable",
  floor_unreadable: "relay floor unreadable",
  catalogue_changed: "catalogue changed, approval pending",
  outside_projects: "outside every project",
  action_not_allowed: "action not listed for the project",
  missing_parameter: "missing parameter",
  unknown_parameter: "unknown parameter",
  invalid_value: "invalid value",
  check_required: "its check has not run",
  approval_rejected: "rejected in Officina",
  approval_unavailable: "request could not be written",
  approval_pending: "approval still pending",
  check_not_satisfied: "code changed since the dry run",
};

export const DENY_TEXT: Record<DenyKind, string> = {
  unparsable: "command could not be read",
  guarded_program: "guarded program",
  encoded_command: "encoded command",
  dynamic: "dynamic command name",
  home_redirection: "home redirected",
  officina_variable: "names an OFFICINA_ variable",
  protected_dir: "names the relay's files",
  powershell_in_guarded_project: "PowerShell in a guarded project",
};

export const GATE_FAULT_TEXT: Record<GateFault, string> = {
  approved_unreadable: "approved catalogue unreadable",
  approved_missing: "approved catalogue missing",
  floor_unreadable: "relay floor unreadable",
  floor_invalid: "relay floor invalid",
  floor_missing: "relay floor missing",
  known_projects_unreadable: "known projects unreadable",
  io: "internal error",
  panic: "internal panic",
};

export const RUN_OUTCOME_TEXT: Record<RunOutcome, string> = {
  exited: "exit",
  timeout: "timed out",
  spawn_failed: "could not start",
};

export const RUN_STATE_TEXT: Record<RunState, string> = {
  running: "running",
  finished: "finished",
  abandoned: "abandoned",
};

/** How an entry reads in a row: `fault` marks what failed or was refused,
 *  `abandoned` a run that never reported its end. */
export type Tone = "ok" | "fault" | "abandoned" | "neutral";

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
  tone: Tone;
}

function entries(params: { [key in string]?: string } | null | undefined): [string, string][] {
  return Object.entries(params ?? {}).map(([key, value]) => [key, value ?? ""]);
}

/** The row of one recent entry (AD-35: time, project, action, params,
 *  outcome, duration, "abandoned" when so). */
export function activityRow(entry: RecentEntry): ActivityRow {
  const event = entry.event;
  const row: ActivityRow = { project: "", action: "", params: [], outcome: "", detail: "", durationMs: null, tone: "neutral" };
  switch (event.kind) {
    case "run": {
      const { started, finished, state } = event;
      row.project = started.project;
      row.action = started.action;
      row.params = entries(started.params);
      if (state !== "finished" || !finished) {
        row.outcome = RUN_STATE_TEXT[state];
        row.tone = state === "abandoned" ? "abandoned" : "neutral";
        return row;
      }
      row.durationMs = finished.duration_ms;
      if (finished.outcome === "exited") {
        row.outcome = `${RUN_OUTCOME_TEXT.exited} ${finished.exit_code ?? "?"}`;
        row.tone = finished.exit_code === 0 ? "ok" : "fault";
      } else {
        row.outcome = RUN_OUTCOME_TEXT[finished.outcome];
        row.tone = "fault";
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
      row.tone = "fault";
      return row;
    case "gate_denied":
      row.project = event.project;
      row.action = "gate";
      row.outcome = `denied: ${DENY_TEXT[event.deny]}${event.matched ? ` ${event.matched}` : ""}`;
      row.detail = event.command_excerpt ?? "";
      row.tone = "fault";
      return row;
    case "gate_error":
      row.project = event.project ?? "";
      row.action = "gate";
      row.outcome = `gate error: ${GATE_FAULT_TEXT[event.cause]}`;
      row.detail = event.detail;
      row.tone = "fault";
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
