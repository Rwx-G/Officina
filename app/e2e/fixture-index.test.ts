// D-181: the loader every scene test finds its fixtures through.
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { fixturesOf, readFixtureIndex } from "./fixture-index";

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
});
