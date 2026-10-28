import { describe, expect, it } from "vitest";
import { fixturesOf } from "../../e2e/fixture-index";
import type { Board } from "./generated/Board";
import type { Card } from "./generated/Card";
import { SETTLE_TARGETS, actionFor, targetsFor, undoOf } from "./settle";

/** Every card of every fixture board the real CLI printed. */
const cards: Card[] = fixturesOf<Board>("board").flatMap(({ data }) => Object.values(data.columns).flat());

const ofKind = (match: (card: Card) => boolean): Card => {
  const found = cards.find(match);
  if (!found) throw new Error("no fixture card of that kind: run `pnpm e2e:fixtures`");
  return found;
};

describe("settling an ended card", () => {
  it("a live card, or one without a session id, goes nowhere", () => {
    for (const card of cards.filter((c) => !["to_resume", "done"].includes(c.column) || c.session_id === null)) {
      expect(targetsFor(card), card.key).toEqual([]);
    }
  });

  it("a card to resume can be marked done or discarded", () => {
    const card = ofKind((c) => c.column === "to_resume" && c.session_id !== null);
    expect(targetsFor(card)).toEqual(["done", "discard"]);
    expect(actionFor(card, "done")).toEqual({ kind: "settle", session: card.session_id, settlement: "done" });
    expect(undoOf(card, actionFor(card, "done")!)).toEqual({ kind: "settle", session: card.session_id, settlement: null });
  });

  it("a card marked done goes back to To resume or is discarded, and undo marks it done again", () => {
    const card = ofKind((c) => c.settled === "marked_done");
    expect(targetsFor(card)).toEqual(["to_resume", "discard"]);
    expect(actionFor(card, "to_resume")).toEqual({ kind: "settle", session: card.session_id, settlement: null });
    expect(undoOf(card, actionFor(card, "discard")!)).toEqual({ kind: "settle", session: card.session_id, settlement: "done" });
  });

  it("a card done by its archived subject goes back by reopening the subject, with no undo", () => {
    const card = ofKind((c) => c.settled === "subject_archived");
    const back = actionFor(card, "to_resume");
    expect(back).toEqual({ kind: "reopen", subject: card.subject!.id });
    expect(undoOf(card, back!)).toBeNull();
  });

  it("a card Claude Code reports done can only be discarded", () => {
    const card = ofKind((c) => c.column === "done" && c.settled === null && c.session_id !== null);
    expect(targetsFor(card)).toEqual(["discard"]);
  });

  it("every target is reachable from some fixture card", () => {
    for (const target of SETTLE_TARGETS) expect(cards.some((c) => actionFor(c, target) !== null), target).toBe(true);
  });
});
