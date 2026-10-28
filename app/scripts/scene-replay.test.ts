// Story 4.14 AC 4 (AD-71, D-120): the replay's schedule and what one step
// writes, on a temporary home built from the scene scenarios' own helpers.
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { COUNT, EVERY_MS, applyStep, replayPlan } from "./scene-replay.mjs";
import { sceneScenarios } from "./scene-fixtures.mjs";

const NOW = 1_793_102_400_000;

interface Agent {
  sessionId: string;
  kind: string;
  status: string;
  name: string;
}

function measureAgents(): Agent[] {
  const scenario = sceneScenarios(NOW)["scene-measure"] as { sessions: { agent: Agent }[] };
  return scenario.sessions.map((s) => s.agent);
}

describe("scene replay", () => {
  it("the replay schedules one crossing change every 10 s", () => {
    const agents = measureAgents();
    const plan = replayPlan(agents, { startMs: NOW });
    expect(plan).toHaveLength(COUNT);
    expect(COUNT * EVERY_MS).toBe(10 * 60_000);
    plan.forEach((step, i) => expect(step.atMs).toBe(NOW + i * EVERY_MS));
    // Each step moves a session between its desk (busy) and the decision
    // office (waiting), the other side of the corridor, from the state it
    // has at that point.
    const state = new Map(agents.map((a) => [a.sessionId, a.status]));
    for (const step of plan) {
      const from = state.get(step.sessionId);
      expect([from, step.to].sort()).toEqual(["busy", "waiting"]);
      state.set(step.sessionId, step.to);
    }
    expect(new Set(plan.map((s) => s.sessionId)).size).toBeGreaterThan(1);
  });

  it("a step rewrites the agents fixture and appends the hook's line", () => {
    const dir = mkdtempSync(join(tmpdir(), "officina-replay-"));
    mkdirSync(join(dir, "officina", "events"), { recursive: true });
    const agents = measureAgents();
    writeFileSync(join(dir, "agents.json"), JSON.stringify(agents));
    const [first] = replayPlan(agents, { startMs: NOW, count: 1 });
    applyStep(dir, first!, NOW);
    const after: Agent[] = JSON.parse(readFileSync(join(dir, "agents.json"), "utf8"));
    expect(after.find((a) => a.sessionId === first!.sessionId)?.status).toBe(first!.to);
    const [file] = readdirSync(join(dir, "officina", "events"));
    const line = JSON.parse(readFileSync(join(dir, "officina", "events", file!), "utf8").trim());
    expect(line.session_id).toBe(first!.sessionId);
    expect(line.event).toBe(first!.to === "waiting" ? "Notification" : "UserPromptSubmit");
  });
});
