// The scene's words (AD-63, D-107): the text alternative, every target's
// `aria-label` and name tag, and the plaques' room names. Counts and names
// only, never a relative time (AD-64): nothing here reads a clock. The
// records are exhaustive, so a new Rust variant fails `pnpm check` here.
import type { AlertCause } from "../generated/AlertCause";
import type { Badge } from "../generated/Badge";
import type { Board } from "../generated/Board";
import type { Card } from "../generated/Card";
import type { Column } from "../generated/Column";
import type { SubagentView } from "../generated/SubagentView";
import type { Zone } from "../generated/Zone";
import type { SharedZone } from "./geometry";
import type { Employee } from "./layout";

export const ZONE_WORDS: Record<Zone, string> = {
  reception: "at reception",
  office: "in its office",
  decision: "in the decision office",
  alert: "in alert",
  lounge: "in the lounge",
  unknown: "unknown",
};

export const COLUMN_WORDS: Record<Column, string> = {
  needs_you: "needs you",
  your_turn: "your turn",
  working: "working",
  to_resume: "to resume",
  done: "done",
};

export const BADGE_WORDS: Record<Badge, string> = {
  unknown: "data unknown",
  interrupted: "interrupted",
  stopped: "stopped",
  ended: "ended",
};

export const ALERT_WORDS: Record<AlertCause, string> = {
  priority: "high priority project",
  waited: "waited past the alert threshold",
};

/** Room names on plaques; an office's plaque carries its project's name. */
export const ROOM_NAMES: Record<SharedZone | "more", string> = {
  reception: "Reception",
  decision: "Decisions: you",
  alert: "Alert",
  lounge: "Lounge",
  more: "More offices",
};

export function plaqueText(room: SharedZone | "more"): string {
  return ROOM_NAMES[room];
}

/** The scene's fixed signs: the door's, the high-priority pill's and the
 *  register's heading. */
export const SIGN_WORDS = {
  entrance: "Entrance and exit",
  highPriority: "high priority",
  register: "Needs you, board order",
} as const;

/** The register's last row when it shows fewer sessions than wait there. */
export function registerMore(hidden: number): string {
  return `+${hidden} more`;
}

/** What a figure's words are built from. */
export interface FigureFacts {
  card: Card;
  employee: Employee;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** The zone as words; an unknown employee keeping an alert says so (D-494). */
function zoneWords({ card, employee }: FigureFacts): string {
  if (employee.zone === "unknown" && card.scene?.alert) return `${ZONE_WORDS.unknown}, ${ZONE_WORDS.alert}, last known`;
  return ZONE_WORDS[employee.zone];
}

/** A sub-agent's type as words; Claude Code may leave it empty. */
export function agentTypeWords(agent: SubagentView): string {
  return agent.agent_type || "sub-agent";
}

/** How many sub-agents a lead runs, as words; `null` for none. */
export function subagentCount(card: Card): string | null {
  const n = card.subagents.length;
  return n === 0 ? null : `running ${plural(n, "sub-agent", "sub-agents")}`;
}

/** A sub-agent's `aria-label` and name tag: its type and its lead. */
export function helperWords(lead: Card, agent: SubagentView): { label: string; tag: string } {
  const type = agentTypeWords(agent);
  return { label: `${type}, sub-agent of ${lead.name}`, tag: type };
}

/** A figure's `aria-label` (session, project, zone, state, reason, number,
 *  sub-agents) and its name tag. */
export function figureWords(facts: FigureFacts): { label: string; tag: string } {
  const { card, employee } = facts;
  const parts = [card.name, `project ${card.project.name}`, zoneWords(facts), COLUMN_WORDS[card.column]];
  if (card.badge) parts.push(BADGE_WORDS[card.badge]);
  if (employee.marks.approval) parts.push("waiting on an approval");
  if (card.column === "needs_you" && card.waiting_for) parts.push(`waiting: ${card.waiting_for}`);
  if (employee.alert && employee.zone !== "unknown") parts.push(ALERT_WORDS[employee.alert]);
  const number = employee.marks.ticket;
  if (number !== null) parts.push(`number ${number}`);
  const helpers = subagentCount(card);
  if (helpers !== null) parts.push(helpers);
  return { label: parts.join(", "), tag: number !== null ? `${number}. ${card.name}` : card.name };
}

/** A group target's words: its room, its count and its members' names.
 *  More offices also counts its projects. */
export function groupWords(room: string, members: FigureFacts[], projects: number | null): { label: string; tag: string } {
  const sessions = plural(members.length, "session", "sessions");
  const names = members.map((m) => m.card.name).join(", ");
  const head = projects === null ? `${room}: ${members.length} more ${members.length === 1 ? "session" : "sessions"}` : `${room}: ${plural(projects, "project", "projects")}, ${sessions}`;
  return {
    label: names ? `${head}: ${names}` : head,
    tag: projects === null ? `+${members.length}` : plural(projects, "project", "projects"),
  };
}

/** The visually hidden summary of the scene (AD-63's example, D-107). */
export function summaryText(board: Board, employees: Employee[]): string {
  const waiting = board.columns.needs_you;
  const alerts = employees.filter((e) => e.alert !== null).length;
  const count = (zone: Zone) => employees.filter((e) => e.zone === zone).length;
  const sentences: string[] = [];
  const head = waiting.length === 0 ? "Nobody needs you" : `${waiting.length} ${waiting.length === 1 ? "needs" : "need"} you`;
  sentences.push(alerts > 0 ? `${head}, ${alerts} in alert` : head);
  const oldest = waiting[0];
  if (oldest) {
    sentences.push(["Oldest: " + oldest.name, `project ${oldest.project.name}`, ...(oldest.waiting_for ? [oldest.waiting_for] : [])].join(", "));
  }
  const approvals = employees.filter((e) => e.marks.approval).length;
  if (approvals > 0) sentences.push(`${approvals} waiting on an approval`);
  if (count("reception") > 0) sentences.push(`${count("reception")} at reception`);
  if (count("lounge") > 0) sentences.push(`${count("lounge")} your turn`);
  const working = employees.filter((e) => e.zone === "office");
  if (working.length > 0) {
    const offices = new Set(working.map((e) => e.project)).size;
    sentences.push(`${working.length} working in ${plural(offices, "office", "offices")}`);
  }
  if (count("unknown") > 0) sentences.push(`${count("unknown")} unknown`);
  if (employees.length === 0) sentences.push("No live session");
  return sentences.map((s) => `${s}.`).join(" ");
}
