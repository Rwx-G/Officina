// What the author can tell the board about an ended session (story 4.15,
// D-536, D-542): Mark done, Back to To resume, Discard. A drag onto a
// column or the discard target, the card's menu and the keyboard all go
// through `actionFor`, so the three paths cannot disagree. Nothing here
// moves a card: the backend records the verdict and the next board places
// it (AD-13).
import type { Card } from "./generated/Card";
import type { Settlement } from "./generated/Settlement";

/** Where an ended card can go: a column, or out of the board. */
export type SettleTarget = "done" | "to_resume" | "discard";

/** Every target, in the order the card's menu lists them. */
export const SETTLE_TARGETS: readonly SettleTarget[] = ["done", "to_resume", "discard"];

/** What the menu item, the drop target and the note call each target. */
export const TARGET_TEXT: Record<SettleTarget, string> = {
  done: "Mark done",
  to_resume: "Back to To resume",
  discard: "Discard",
};

/** What reaching a target writes: the session's own verdict (or `null`
 *  to take it back), or, for a card in Done because its subject was
 *  archived, that subject reopened (D-542). */
export type SettleAction = { kind: "settle"; session: string; settlement: Settlement | null } | { kind: "reopen"; subject: number };

/** The verdict the card carries now, as `settle_session` takes it. */
function current(card: Card): Settlement | null {
  return card.settled === "marked_done" ? "done" : null;
}

/** What dropping `card` on `target` does, or `null` when it means nothing
 *  there: a live card, a card without a session id, or a target the card
 *  is already in. */
export function actionFor(card: Card, target: SettleTarget): SettleAction | null {
  const session = card.session_id;
  if (session === null || (card.column !== "to_resume" && card.column !== "done")) return null;
  const settle = (settlement: Settlement | null): SettleAction => ({ kind: "settle", session, settlement });
  switch (target) {
    case "done":
      return card.column === "to_resume" ? settle("done") : null;
    case "discard":
      return settle("discarded");
    case "to_resume":
      if (card.settled === "marked_done") return settle(null);
      if (card.settled === "subject_archived" && card.subject) return { kind: "reopen", subject: card.subject.id };
      return null;
  }
}

/** The targets `card` can go to, in menu order. */
export function targetsFor(card: Card): SettleTarget[] {
  return SETTLE_TARGETS.filter((target) => actionFor(card, target) !== null);
}

/** What puts `card` back as it was before `action`: its own verdict
 *  restored; `null` for a reopened subject, which is archived again from
 *  the Subjects view, where its routine box is (D-542). */
export function undoOf(card: Card, action: SettleAction): SettleAction | null {
  if (action.kind === "reopen") return null;
  return { kind: "settle", session: action.session, settlement: current(card) };
}

/** What the note says once `action` was recorded for `card`. */
export function doneText(card: Card, target: SettleTarget, action: SettleAction): string {
  if (action.kind === "reopen") {
    return `Reopened the subject of ${card.name}: it is to resume again. Archive it again from Subjects.`;
  }
  switch (target) {
    case "done":
      return `Marked ${card.name} done.`;
    case "discard":
      return `Discarded ${card.name}. Archived subjects in Subjects can restore it.`;
    case "to_resume":
      return `${card.name} is to resume again.`;
  }
}
