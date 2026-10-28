// Window state: the current board, a clock for relative times, and the
// outcome of recent clicks, kept `OUTCOME_VISIBLE_MS` per card (AD-20).
import type { Board } from "./generated/Board";
import type { Card } from "./generated/Card";
import type { FocusOutcome } from "./generated/FocusOutcome";
import { haloMemo } from "./halo";
import * as ipc from "./ipc";

export const OUTCOME_VISIBLE_MS = 10_000;
/** A second focus of the same card within this delay is ignored: a held key
 *  must not open one attach console per auto-repeat. */
export const FOCUS_REPEAT_GUARD_MS = 1_500;
/** How long a card says its attach command was copied. */
export const COPIED_VISIBLE_MS = 2_000;
/** How long past an outcome's visibility its expiry pass runs, so the pass
 *  finds it expired whatever the timer's jitter. */
const EXPIRY_SLACK_MS = 50;
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

  /** The one-shot timers the store has pending, by what they are for: a
   *  new one for the same purpose replaces the old, and the stop `start`
   *  returns clears them all. Bookkeeping, never rendered: not state. */
  #timers: Record<string, ReturnType<typeof setTimeout>> = {};

  #later(purpose: string, delayMs: number, run: () => void): void {
    clearTimeout(this.#timers[purpose]);
    this.#timers[purpose] = setTimeout(() => {
      delete this.#timers[purpose];
      run();
    }, delayMs);
  }

  async start(): Promise<() => void> {
    const unlisten = await ipc.onBoard((board) => {
      this.#show(board);
      this.nowMs = Date.now();
    });
    const initial = await ipc.getBoard();
    if (initial && !this.board) this.#show(initial);
    const clock = setInterval(() => {
      this.nowMs = Date.now();
      this.dropExpiredOutcomes();
    }, CLOCK_TICK_MS);
    return () => {
      unlisten();
      clearInterval(clock);
      for (const timer of Object.values(this.#timers)) clearTimeout(timer);
      this.#timers = {};
    };
  }

  /** Shows `board`, and forgets the halo timings of the sessions it no
   *  longer has waiting: one that waits again later breathes again. A
   *  session waiting on a relay approval alone is still Working, but its
   *  scene halo breathes in the decision office (D-464). */
  #show(board: Board): void {
    const { needs_you, your_turn, working } = board.columns;
    const waiting = [...needs_you, ...your_turn, ...working].filter(
      (card) => card.column === "needs_you" || card.scene?.zone === "decision" || card.scene?.zone === "alert",
    );
    haloMemo.retain(waiting.map((card) => card.key));
    this.board = board;
  }

  /** Drops the outcomes shown longer than `OUTCOME_VISIBLE_MS`; leaves the
   *  record as it is when none expired, so nothing derived from it reruns. */
  dropExpiredOutcomes(): void {
    const entries = Object.entries(this.outcomes);
    const fresh = entries.filter(([, shown]) => this.nowMs - shown.atMs < OUTCOME_VISIBLE_MS);
    if (fresh.length < entries.length) this.outcomes = Object.fromEntries(fresh);
  }

  async focus(card: Card): Promise<void> {
    const last = this.outcomes[card.key];
    if (last && Date.now() - last.atMs < FOCUS_REPEAT_GUARD_MS) return;
    this.outcomes = { ...this.outcomes, [card.key]: { outcome: { outcome: "focused" }, atMs: Date.now(), pending: true } };
    let outcome: FocusOutcome;
    try {
      outcome = await ipc.focus(card.key);
    } catch (error) {
      outcome = { outcome: "failed", detail: ipc.errorMessage(error) };
    }
    const atMs = Date.now();
    this.outcomes = { ...this.outcomes, [card.key]: { outcome, atMs } };
    this.#later(`outcome:${card.key}`, OUTCOME_VISIBLE_MS + EXPIRY_SLACK_MS, () => {
      this.nowMs = Date.now();
      this.dropExpiredOutcomes();
    });
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
    this.#later("copied", COPIED_VISIBLE_MS, () => {
      if (this.copied === card.key) this.copied = null;
    });
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
      this.approvalErrors = { ...this.approvalErrors, [id]: ipc.errorMessage(error) };
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
