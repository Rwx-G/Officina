// Boards for the scene's Vitest cases: the fixture boards built from the
// real CLI (D-181), and synthetic boards shaped around one rule each.
// Imported by tests only.
/// <reference types="node" />
import { fixturesOf } from "../../../e2e/fixture-index";
import type { AlertCause } from "../generated/AlertCause";
import type { ApprovalView } from "../generated/ApprovalView";
import type { Board } from "../generated/Board";
import type { Card } from "../generated/Card";
import type { Column } from "../generated/Column";
import type { SubagentView } from "../generated/SubagentView";
import type { Zone } from "../generated/Zone";

/** Every board of the fixture index. */
export function fixtureBoards(): { name: string; board: Board }[] {
  return fixturesOf<Board>("board").map(({ name, data }) => ({ name, board: data }));
}

/** The column a synthetic card of each zone is written in, as core places
 *  them (AD-167): test data, never read by the layout. */
export const COLUMN_OF: Record<Zone, Column> = {
  reception: "your_turn",
  lounge: "your_turn",
  office: "working",
  decision: "needs_you",
  alert: "needs_you",
  unknown: "working",
};

export interface CardOptions {
  project?: string;
  started?: number | null;
  column?: Column;
  approval?: boolean;
  alert?: AlertCause | null;
  subagents?: SubagentView[];
}

/** A live card of `zone`; its name is its key. */
export function card(key: string, zone: Zone, o: CardOptions = {}): Card {
  const column = o.column ?? COLUMN_OF[zone];
  return {
    key,
    column,
    badge: zone === "unknown" ? "unknown" : null,
    kind: "interactive",
    name: key,
    project: { name: o.project ?? "alpha", color: null, priority: "normal" },
    session_id: `session-${key}`,
    background_id: null,
    pid: null,
    cwd: null,
    waiting_for: column === "needs_you" ? "permission prompt" : null,
    last_prompt: null,
    last_message: null,
    last_message_tail: null,
    last_event_at_ms: null,
    started_at_ms: o.started === undefined ? 0 : o.started,
    attach_available: false,
    subject: null,
    settled: null,
    subagents: o.subagents ?? [],
    scene: {
      zone,
      waiting_since_ms: null,
      alert: o.alert !== undefined ? o.alert : zone === "alert" ? "waited" : null,
      approval: o.approval ?? false,
    },
  };
}

/** A pending approval for `card`'s session. */
export function approval(of: Card, requestedAt: number, valid = true): ApprovalView {
  return {
    id: `approval-${of.key}-${requestedAt}`,
    project: of.project.name,
    action: "deploy",
    class: "human",
    state: "pending",
    escalation: null,
    description: "",
    argv: [],
    http: null,
    valid,
    invalid_reason: valid ? null : "unknown action",
    session_id: of.session_id,
    session_name: of.name,
    requested_at_ms: requestedAt,
    expires_at_ms: requestedAt + 110_000,
    dry_run: null,
    binding: null,
    bound_files: null,
    files_unchanged: null,
  };
}

const BASE: Board = (() => {
  const first = fixtureBoards()[0];
  if (!first) throw new Error("no fixture board: run `pnpm e2e:fixtures` first");
  return first.board;
})();

/** A board holding `cards` in their columns, in the order given. */
export function board(cards: Card[], o: Partial<Board> = {}): Board {
  const columns: Board["columns"] = { needs_you: [], your_turn: [], working: [], to_resume: [], done: [] };
  for (const c of cards) columns[c.column].push(c);
  return {
    ...BASE,
    floor: null,
    stale: null,
    approvals: [],
    floor_projects: [],
    alert_count: cards.filter((c) => c.scene?.alert).length,
    next_escalation_at_ms: null,
    ...o,
    columns,
    waiting_count: columns.needs_you.length,
  };
}

/** A floor project. */
export function project(name: string, priority: "normal" | "high" = "normal") {
  return { name, color: null, priority };
}
