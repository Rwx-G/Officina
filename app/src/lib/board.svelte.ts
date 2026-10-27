// Window state: the current board, a clock for relative times, and the
// outcome of recent clicks, kept for 10 s per card (AD-20).
import type { Board } from "./generated/Board";
import type { Card } from "./generated/Card";
import type { FocusOutcome } from "./generated/FocusOutcome";
import * as ipc from "./ipc";

export const OUTCOME_VISIBLE_MS = 10_000;
const CLOCK_TICK_MS = 30_000;

export interface ShownOutcome {
  outcome: FocusOutcome;
  atMs: number;
}

export class BoardStore {
  board = $state<Board | null>(null);
  nowMs = $state(Date.now());
  outcomes = $state<Record<string, ShownOutcome>>({});
  copied = $state<string | null>(null);
  alwaysOnTop = $state(false);

  needsYou = $derived(this.board?.columns.needs_you ?? []);

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
    const outcome = await ipc.focus(card.key);
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

  async toggleAlwaysOnTop(): Promise<void> {
    this.alwaysOnTop = !this.alwaysOnTop;
    await ipc.setAlwaysOnTop(this.alwaysOnTop);
  }

  refresh(): Promise<void> {
    return ipc.refresh();
  }
}
