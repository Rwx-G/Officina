// @vitest-environment node
// The scene's lint (AD-64, D-110, D-470): every name the config restricts
// under the scene's globs is reported in a file of each glob and in no file
// outside them; the mover's module alone may use the frame loop and the
// page's clock, and nothing else.
// The names come from the config itself, never from a list here; whether
// they still cover AD-64's list is a review item (D-110).
/// <reference types="node" />
import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";

const eslint = new ESLint({ cwd: process.cwd() });

/** A file of each scene glob, files next to the mover's or named like it,
 *  and files outside. */
const INSIDE = [
  "src/lib/scene/probe.ts",
  "src/components/scene/probe.ts",
  "src/lib/scene/mover.test.ts",
  "src/lib/scene/objects.ts",
  "src/components/scene/mover.ts",
  "src/lib/scene/sub/mover.ts",
];
const MOVER = "src/lib/scene/mover.ts";
/** What story 4.10 AC 12 lets the mover use. */
const MOVER_MAY_USE = ["requestAnimationFrame", "cancelAnimationFrame", "performance.now"];
const OUTSIDE = ["src/lib/probe.ts", "src/components/probe.ts", "e2e/probe.ts"];

type RuleEntry = [unknown, ...unknown[]];

/** One snippet per restricted name of the rule, as the config for `file`
 *  lists them. */
async function snippets(file: string): Promise<{ rule: string; name: string; code: string }[]> {
  const config = (await eslint.calculateConfigForFile(file)) as { rules?: Record<string, RuleEntry> };
  const out: { rule: string; name: string; code: string }[] = [];
  const globals = config.rules?.["no-restricted-globals"]?.slice(1) ?? [];
  for (const entry of globals) {
    const name = typeof entry === "string" ? entry : (entry as { name: string }).name;
    out.push({ rule: "no-restricted-globals", name, code: `export const probe = ${name};\n` });
  }
  const properties = config.rules?.["no-restricted-properties"]?.slice(1) ?? [];
  for (const entry of properties) {
    const { object, property } = entry as { object: string; property: string };
    out.push({ rule: "no-restricted-properties", name: `${object}.${property}`, code: `export const probe = ${object}.${property}();\n` });
  }
  return out;
}

async function reported(code: string, file: string): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath: file });
  return (result?.messages ?? []).map((m) => m.ruleId ?? "");
}

// The first case to run loads ESLint's parsers and plugins: 4 s alone and
// 5.7 s beside the other files on the workstation, against 0.1 s for the
// same work once loaded; past Vitest's 5 s default for a cost that is
// ESLint's, not the scene's (D-1070).
describe("scene lint", { timeout: 20_000 }, () => {
  it("each restricted name is reported under both scene globs and not outside them", async () => {
    const restricted = await snippets(INSIDE[0]!);
    expect(restricted.length).toBeGreaterThan(0);
    // Both globs carry the same restrictions.
    for (const file of INSIDE) expect((await snippets(file)).map((s) => s.name), file).toEqual(restricted.map((s) => s.name));
    for (const { rule, name, code } of restricted) {
      for (const file of INSIDE) expect(await reported(code, file), `${name} in ${file}`).toContain(rule);
      for (const file of OUTSIDE) expect(await reported(code, file), `${name} in ${file}`).not.toContain(rule);
    }
  });

  it("the mover alone may use the frame loop and performance.now", async () => {
    const restricted = await snippets(INSIDE[0]!);
    for (const name of MOVER_MAY_USE) expect(restricted.map((s) => s.name), name).toContain(name);
    const allowed = new Set(MOVER_MAY_USE);
    for (const { rule, name, code } of restricted) {
      const messages = await reported(code, MOVER);
      if (allowed.has(name)) expect(messages, `${name} in the mover`).not.toContain(rule);
      else expect(messages, `${name} in the mover`).toContain(rule);
    }
  });
});
