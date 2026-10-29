// The scene's words (AD-63, D-107): built from the layout, counts and
// names, never a relative time.
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { Board } from "../generated/Board";
import { MAX_NUMBER, layoutScene, type SceneLayout } from "./layout";
import { emptyMemory, type SceneMemory } from "./memory";
import { CAPACITY } from "./places";
import { approval, board, card, fixtureBoards } from "./testing";
import { ALERT_WORDS, BADGE_WORDS, COLUMN_WORDS, ROOM_NAMES, ZONE_WORDS } from "./text";

const lay = (b: Board, memory: SceneMemory = emptyMemory()) => layoutScene({ board: b }, memory);
const label = (l: SceneLayout, key: string) => l.targets.find((t) => t.keys.includes(key) && t.kind === "figure")!.label;

describe("text", () => {
  it("the summary names the waiting count, the alert count and the oldest waiting session", () => {
    const oldest = { ...card("billing-api", "decision", { project: "billing" }), waiting_for: "permission prompt" };
    const l = lay(
      board([
        oldest,
        card("web", "alert", { project: "web" }),
        card("arrival", "reception", { started: 1 }),
        card("rest", "lounge", { started: 2 }),
        card("w1", "office", { started: 3 }),
        card("w2", "office", { project: "beta", started: 4 }),
      ]),
    );
    expect(l.summary).toBe(
      "2 need you, 1 in alert. Oldest: billing-api, project billing, permission prompt. 1 at reception. 1 your turn. 2 working in 2 offices.",
    );
    const one = lay(board([card("solo", "decision")]));
    expect(one.summary).toMatch(/^1 needs you\. Oldest: solo, project alpha, permission prompt\./);
  });

  it("a lead's name counts its sub-agents", () => {
    const one = card("lead", "office", { subagents: [{ agent_id: "a1", agent_type: "Explore" }] });
    expect(label(lay(board([one])), "lead")).toMatch(/, running 1 sub-agent$/);
    const two = card("lead", "office", { subagents: [{ agent_id: "a1", agent_type: "Explore" }, { agent_id: "a2", agent_type: null }] });
    expect(label(lay(board([two])), "lead")).toMatch(/, running 2 sub-agents$/);
    expect(label(lay(board([card("lead", "office")])), "lead")).not.toContain("sub-agent");
  });

  it("nobody waiting reads as such", () => {
    expect(lay(board([card("w", "office")])).summary).toBe("Nobody needs you. 1 working in 1 office.");
    expect(lay(board([])).summary).toBe("Nobody needs you. No live session.");
    const approvalOnly = card("a", "decision", { column: "working", approval: true });
    expect(lay(board([approvalOnly], { approvals: [approval(approvalOnly, 1)] })).summary).toBe("Nobody needs you. 1 waiting on an approval.");
  });

  it("every figure's name states session, project, zone, state and its number up to nine", () => {
    const waiting = Array.from({ length: MAX_NUMBER + 1 }, (_, i) => card(`n${i}`, "decision", { started: i, project: `p${i % 3}` }));
    const l = lay(board([...waiting, card("w", "office", { started: 99 })]));
    expect(label(l, "n0")).toBe(`n0, project p0, ${ZONE_WORDS.decision}, ${COLUMN_WORDS.needs_you}, waiting: permission prompt, number 1`);
    expect(label(l, "w")).toBe(`w, project alpha, ${ZONE_WORDS.office}, ${COLUMN_WORDS.working}`);
    for (const t of l.targets.filter((x) => x.kind === "figure")) {
      const e = l.employees.find((x) => x.key === t.keys[0])!;
      const number = e.marks.ticket;
      if (number !== null) {
        expect(number).toBeLessThanOrEqual(MAX_NUMBER);
        expect(t.label).toContain(`number ${number}`);
        expect(t.tag).toBe(`${number}. ${t.keys[0]}`);
      } else {
        expect(t.label).not.toContain("number");
      }
    }
    const alerted = lay(board([card("hot", "alert", { alert: "priority" })]));
    expect(label(alerted, "hot")).toContain(ALERT_WORDS.priority);
  });

  it("an unknown employee reads as unknown", () => {
    const l = lay(board([card("u", "unknown")]));
    expect(label(l, "u")).toBe(`u, project alpha, ${ZONE_WORDS.unknown}, ${COLUMN_WORDS.working}, ${BADGE_WORDS.unknown}`);
    expect(l.summary).toBe("Nobody needs you. 1 unknown.");
  });

  it("an unknown employee that keeps an alert says so", () => {
    const l = lay(board([card("u", "unknown", { alert: "waited", column: "needs_you" })]));
    expect(label(l, "u")).toContain(`${ZONE_WORDS.unknown}, ${ZONE_WORDS.alert}, last known`);
    expect(l.summary).toMatch(/^1 needs you, 1 in alert\./);
  });

  it("group targets name their count", () => {
    const cards = Array.from({ length: CAPACITY.lounge + 2 }, (_, i) => card(`l${i}`, "lounge", { started: i, project: `p${i}` }));
    const l = lay(board(cards));
    const group = l.targets.find((t) => t.id === "group:lounge")!;
    const extra = cards.slice(CAPACITY.lounge).map((c) => c.name);
    expect(group.label).toBe(`${ROOM_NAMES.lounge}: ${extra.length} more sessions: ${extra.join(", ")}`);
    expect(group.tag).toBe(`+${extra.length}`);
    const one = lay(board(cards.slice(0, CAPACITY.lounge + 1)));
    expect(one.targets.find((t) => t.id === "group:lounge")!.label).toMatch(/: 1 more session: /);
  });

  it("every zone, column and badge has a word", () => {
    for (const words of [ZONE_WORDS, COLUMN_WORDS, BADGE_WORDS, ALERT_WORDS, ROOM_NAMES]) {
      const values = Object.values(words);
      expect(values.length).toBeGreaterThan(0);
      for (const w of values) expect(w.trim()).not.toBe("");
      expect(new Set(values).size).toBe(values.length);
    }
  });

  it("draws no relative time", () => {
    const source = readFileSync(join(process.cwd(), "src", "lib", "scene", "text.ts"), "utf8");
    expect(source).not.toMatch(/from "\.\.\/time"|lib\/time/);
    for (const { board: b } of fixtureBoards()) {
      const l = lay(b);
      for (const text of [l.summary, ...l.targets.flatMap((t) => [t.label, t.tag])]) {
        expect(text).not.toMatch(/\b(ago|min|minutes?|seconds?|hours?)\b/);
      }
    }
  });
});
