// D-181: the loader every scene test finds its fixtures through.
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { Board } from "../src/lib/generated/Board";
import type { WorkshopView } from "../src/lib/generated/WorkshopView";
import { fixtureOf, fixturesOf, readFixtureIndex } from "./fixture-index";

function dirWith(files: Record<string, unknown>): string {
  const dir = mkdtempSync(join(tmpdir(), "officina-fixture-index-"));
  for (const [name, content] of Object.entries(files)) writeFileSync(join(dir, name), JSON.stringify(content));
  return dir;
}

describe("fixture index", () => {
  it("a missing index fails and names pnpm e2e:fixtures", () => {
    expect(() => readFixtureIndex(dirWith({}))).toThrow(/pnpm e2e:fixtures/);
  });

  it("an index without a board fails", () => {
    const dir = dirWith({ "index.json": { files: { "relay-clean.json": "relay" } }, "relay-clean.json": {} });
    expect(() => readFixtureIndex(dir)).toThrow(/no board.*pnpm e2e:fixtures/);
  });

  it("files are returned by kind", () => {
    const dir = dirWith({
      "index.json": { files: { "a.json": "board", "b.json": "relay", "c.json": "board" } },
      "a.json": { n: 1 },
      "b.json": { n: 2 },
      "c.json": { n: 3 },
    });
    expect(fixturesOf<{ n: number }>("board", dir)).toEqual([
      { name: "a", data: { n: 1 } },
      { name: "c", data: { n: 3 } },
    ]);
    expect(fixturesOf("registry", dir)).toEqual([]);
  });

  it("an index without an orchestrator output fails and names pnpm e2e:fixtures", () => {
    const dir = dirWith({ "index.json": { files: { "a.json": "board" } }, "a.json": {} });
    expect(() => fixtureOf("orchestrator", "orchestrator-day", dir)).toThrow(/no orchestrator output.*pnpm e2e:fixtures/);
    const listed = dirWith({
      "index.json": { files: { "a.json": "board", "orchestrator-day.json": "orchestrator" } },
      "a.json": {},
      "orchestrator-day.json": { schema: 1 },
    });
    expect(fixtureOf("orchestrator", "orchestrator-day", listed)).toEqual({ schema: 1 });
    expect(() => fixtureOf("orchestrator", "orchestrator-empty", listed)).toThrow(/named orchestrator-empty.*pnpm e2e:fixtures/);
  });

  // Story 6.18 AC 5: the kind `workshop` holds the real Workshop view and
  // the board of the same home, one card per run state.
  it("the workshop kind loads the view and its board", () => {
    const view = fixtureOf<WorkshopView>("workshop", "workshop-runs");
    expect(view.schema).toBe(1);
    expect(view.runs.live.length).toBeGreaterThan(0);
    const board = fixtureOf<Board>("workshop", "workshop-runs-board");
    const cards = Object.values(board.columns)
      .flat()
      .filter((card) => card.kind === "workshop");
    expect(new Set(cards.map((card) => card.run_state)).size).toBe(cards.length);
    expect(board.workshop_live).toBe(view.runs.live.length);
  });
});
