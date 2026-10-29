// The open meeting (AD-84, AD-93, D-138): one at a time, in the side
// panel's slot. Receive asks the backend for the meeting of a card key, a
// `sid:` session or a `subject:` to resume; the panel asks again whenever
// the board shows that card's state changed. Nothing is stored.
import { openMeeting } from "./ipc";
import type { Board } from "./generated/Board";
import type { Card } from "./generated/Card";
import type { MeetingView } from "./generated/MeetingView";

/** What the board shows of a card that a meeting depends on: a change of
 *  any of it asks the meeting again. `absent` when the board has no card
 *  of that key or session. */
export function cardState(board: Board, key: string, sessionId: string | null): string {
  const cards: Card[] = Object.values(board.columns).flat();
  const card = cards.find((c) => c.key === key) ?? (sessionId !== null ? cards.find((c) => c.session_id === sessionId) : undefined);
  if (!card) return "absent";
  return JSON.stringify([card.column, card.badge, card.waiting_for, card.pid, card.scene?.zone ?? null]);
}

/** True when a board card offers Receive (FR-5): Needs you and Your turn,
 *  and a background session idle in Done with a pid, which is live and
 *  waits for its next prompt (E1). */
export function offersReceive(card: Card): boolean {
  return (
    card.column === "needs_you" ||
    card.column === "your_turn" ||
    (card.kind === "background" && card.column === "done" && card.pid !== null)
  );
}

/** The key Receive on a subject to resume sends (D-138). */
export const subjectKey = (subject: number): string => `subject:${subject}`;

export class MeetingStore {
  /** The key Receive was given, `null` while no meeting is open. */
  key = $state<string | null>(null);
  view = $state<MeetingView | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);
  /** Called when a meeting opens, so the window shows the sessions. */
  onOpen: () => void = () => {};
  #asked = 0;
  #state: string | null = null;
  #returnFocus: HTMLElement | null = null;

  /** Opens the meeting of `key`, closing any other (one panel at a time,
   *  D-111); focus returns to what had it when the meeting closes. */
  receive(key: string): Promise<void> {
    const active = document.activeElement;
    this.#returnFocus = active instanceof HTMLElement ? active : null;
    this.key = key;
    this.view = null;
    this.error = null;
    this.#state = null;
    this.onOpen();
    return this.#ask(key);
  }

  async #ask(key: string): Promise<void> {
    const asked = ++this.#asked;
    this.loading = true;
    try {
      const view = await openMeeting(key);
      if (asked !== this.#asked) return;
      // The board read next is the baseline of the view just read.
      this.#state = null;
      this.view = view;
      this.error = null;
    } catch (error) {
      if (asked !== this.#asked) return;
      this.error = (error as { message?: string }).message ?? String(error);
    } finally {
      if (asked === this.#asked) this.loading = false;
    }
  }

  /** Closes the meeting; `refocus` returns the focus to its opener. */
  close(refocus = true): void {
    if (this.key === null) return;
    this.#asked++;
    this.key = null;
    this.view = null;
    this.error = null;
    this.loading = false;
    this.#state = null;
    const back = this.#returnFocus;
    this.#returnFocus = null;
    if (refocus && back?.isConnected) back.focus();
  }

  /** Each new board: the meeting asks again when its card's state there
   *  changed (AD-84). */
  boardChanged(board: Board): void {
    const key = this.key;
    if (key === null) return;
    const state = cardState(board, this.view?.key ?? key, this.view?.session_id ?? null);
    const before = this.#state;
    this.#state = state;
    if (before !== null && before !== state) void this.#ask(key);
  }
}

/** The window's one meeting. */
export const meeting = new MeetingStore();
