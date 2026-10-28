// Window state: the current board, a clock for relative times, and the
// outcome of recent clicks, kept for 10 s per card (AD-20).
import type { Board } from "./generated/Board";
import type { Card } from "./generated/Card";
import type { FocusOutcome } from "./generated/FocusOutcome";
import * as ipc from "./ipc";

export const OUTCOME_VISIBLE_MS = 10_000;
/** A second focus of the same card within this delay is ignored: a held key
 *  must not open one attach console per auto-repeat. */
export const FOCUS_REPEAT_GUARD_MS = 1_500;
const CLOCK_TICK_MS = 30_000;

export interface ShownOutcome {
  outcome: FocusOutcome;
  atMs: number;
  /** Set while the backend has not answered yet; nothing is shown. */
  pending?: boolean;
}

export class BoardStore {
  board = $state<Board | null>(null);
  nowMs = $state(Date.now());
  outcomes = $state<Record<string, ShownOutcome>>({});
  copied = $state<string | null>(null);
  alwaysOnTop = $state(false);
  /** Why the backend refused a decision, by request id. */
  approvalErrors = $state<Record<string, string>>({});
  /** Requests whose decision is on its way to the backend. */
  deciding = $state<Record<string, boolean>>({});

  /** The session cards keys 1 to 9 focus: approval cards are not among
   *  them, so no key ever lands on Approve (D-28). */
  needsYou = $derived(this.board?.columns.needs_you ?? []);
  approvals = $derived(this.board?.approvals ?? []);

  async start(): Promise<() => void> {
    const unlisten = await ipc.onBoard((board) => {
      this.board = board;
      this.nowMs = Date.now();
    });
    const initial = await ipc.getBoard();
    if (initial && !this.board) this.board = initial;
    const timer = setInterval(() => {
      this.nowMs = Date.now();
      this.dropExpiredOutcomes();
    }, CLOCK_TICK_MS);
    return () => {
      unlisten();
      clearInterval(timer);
    };
  }

  dropExpiredOutcomes(): void {
    const fresh: Record<string, ShownOutcome> = {};
    for (const [key, shown] of Object.entries(this.outcomes)) {
      if (this.nowMs - shown.atMs < OUTCOME_VISIBLE_MS) fresh[key] = shown;
    }
    this.outcomes = fresh;
  }

  async focus(card: Card): Promise<void> {
    const last = this.outcomes[card.key];
    if (last && Date.now() - last.atMs < FOCUS_REPEAT_GUARD_MS) return;
    this.outcomes = { ...this.outcomes, [card.key]: { outcome: { outcome: "focused" }, atMs: Date.now(), pending: true } };
    let outcome: FocusOutcome;
    try {
      outcome = await ipc.focus(card.key);
    } catch (error) {
      outcome = { outcome: "failed", detail: String(error) };
    }
    const atMs = Date.now();
    this.outcomes = { ...this.outcomes, [card.key]: { outcome, atMs } };
    setTimeout(() => {
      this.nowMs = Date.now();
      this.dropExpiredOutcomes();
    }, OUTCOME_VISIBLE_MS + 50);
  }

  async wrongWindow(card: Card): Promise<void> {
    await ipc.reportWrongWindow(card.key);
    const { [card.key]: _dropped, ...rest } = this.outcomes;
    this.outcomes = rest;
  }

  async copyAttach(card: Card): Promise<void> {
    const command = await ipc.attachCommand(card.key);
    if (!command) return;
    await navigator.clipboard.writeText(command);
    this.copied = card.key;
    setTimeout(() => {
      if (this.copied === card.key) this.copied = null;
    }, 2000);
  }

  /** Approves or rejects request `id`, once at a time; a refusal is kept to
   *  show on its card. The card leaves Needs you with the next board. */
  async decideApproval(id: string, approve: boolean): Promise<void> {
    if (this.deciding[id]) return;
    this.deciding = { ...this.deciding, [id]: true };
    const { [id]: _previous, ...others } = this.approvalErrors;
    this.approvalErrors = others;
    try {
      await ipc.decideApproval(id, approve);
    } catch (error) {
      this.approvalErrors = { ...this.approvalErrors, [id]: String(error) };
    } finally {
      const { [id]: _done, ...rest } = this.deciding;
      this.deciding = rest;
    }
  }

  async toggleAlwaysOnTop(): Promise<void> {
    this.alwaysOnTop = !this.alwaysOnTop;
    await ipc.setAlwaysOnTop(this.alwaysOnTop);
  }

  refresh(): Promise<void> {
    return ipc.refresh();
  }
}
