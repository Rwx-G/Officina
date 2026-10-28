// The alert band's words (FR-10, AD-175): which session it names and why,
// and the first alerted card the beacon breathes for. Core decides who is
// in alert (`Card.scene`, AD-167); this module only words it, from the
// board and the store's clock tick, and picks the beacon's card.
import type { AlertCause } from "./generated/AlertCause";
import type { Board } from "./generated/Board";
import type { Card } from "./generated/Card";
import { haloEnteredAt, type HaloPair } from "./halo";

/** How each alert cause is told; a waited alert adds its minutes. */
export const CAUSE_TEXT: Record<AlertCause, string> = {
  priority: "high-priority project",
  waited: "waiting",
};

/** What the band says when nothing names the wait. */
export const UNNAMED_WAIT = "waiting for you";
/** What an approval-only wait waits on (D-496). */
export const APPROVAL_WAIT = "a relay approval";
/** The sentence that ends the band while an alert rests on unknown data
 *  (D-494). */
export const LAST_KNOWN = "Last known state: data unknown.";

export interface AlertSummary {
  /** The first alerted session in board order. */
  first: Card;
  /** How many others are in alert. */
  more: number;
  /** An alerted card's zone is unknown: its alert is the last known. */
  lastKnown: boolean;
}

/** The cards in alert, in board order (columns in display order). */
export function alertedCards(board: Board): Card[] {
  const { needs_you, your_turn, working, to_resume, done } = board.columns;
  return [...needs_you, ...your_turn, ...working, ...to_resume, ...done].filter((card) => card.scene?.alert);
}

/** What the alert room's beacon breathes for (AD-175, D-474 as refined by
 *  D-527): the first alerted card in board order, keyed on when its alert
 *  began: the wait's start for a high-priority project, the start plus the
 *  board's threshold for a wait that reached it, or, the start unknown, when
 *  its halo counts from; `null` with no alert. */
export function beaconPair(board: Board): HaloPair | null {
  const first = alertedCards(board)[0];
  if (!first) return null;
  const since = first.scene?.waiting_since_ms ?? null;
  if (since === null) return { key: first.key, enteredAt: haloEnteredAt(first) };
  const reached = first.scene?.alert === "waited" ? board.alert_after_ms : 0;
  return { key: first.key, enteredAt: since + reached };
}

/** What the band shows for `board`, or `null` when nobody is in alert. */
export function alertSummary(board: Board): AlertSummary | null {
  const alerted = alertedCards(board);
  const first = alerted[0];
  if (board.alert_count === 0 || !first) return null;
  return {
    first,
    more: alerted.length - 1,
    lastKnown: alerted.some((card) => card.scene?.zone === "unknown"),
  };
}

/** Why `card` is in alert: its cause, with the minutes waited for a waited
 *  one, worded at `nowMs`. */
export function causeText(card: Card, nowMs: number): string {
  const scene = card.scene;
  if (!scene?.alert) return "";
  const text = CAUSE_TEXT[scene.alert];
  if (scene.alert !== "waited" || scene.waiting_since_ms === null) return text;
  const minutes = Math.max(0, Math.floor((nowMs - scene.waiting_since_ms) / 60_000));
  return `${text} ${minutes} min`;
}

/** What `card` waits on, as the board says it. */
export function waitText(card: Card): string {
  if (card.waiting_for) return card.waiting_for;
  return card.scene?.approval ? APPROVAL_WAIT : UNNAMED_WAIT;
}

/** The band's sentence: "{name} ({project}) is blocked: {wait}, {cause},
 *  and N more." plus the last-known mark. Plain text: the component
 *  renders it with `{text}`. */
export function bandText(summary: AlertSummary, nowMs: number): string {
  const { first, more, lastKnown } = summary;
  const head = `${first.name} (${first.project.name}) is blocked: ${waitText(first)}, ${causeText(first, nowMs)}`;
  const tail = more > 0 ? `, and ${more} more.` : ".";
  return lastKnown ? `${head}${tail} ${LAST_KNOWN}` : `${head}${tail}`;
}
