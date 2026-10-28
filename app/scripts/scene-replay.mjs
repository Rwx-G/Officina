// The busy day of AD-71's walking runs (story 4.14 AC 4, D-120): on a home
// written by `e2e-fixtures.mjs --home`, one state change every 10 s for 10
// minutes, each moving a session between its office and the decision
// office, so every change walks across the corridor. It rewrites only that
// home's agents fixture and event files, as the fake claude and the hook
// would, never the real Officina home or the Claude config dir.
//
// Usage: node scripts/scene-replay.mjs --home <dir> [--every-ms 10000] [--count 60]
import { appendFileSync, existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const EVERY_MS = 10_000;
export const COUNT = 60;

/** The two states a replayed session moves between: `busy` sits at its
 *  desk, `waiting` queues at the decision office. */
const FLIP = { busy: "waiting", waiting: "busy" };

/** The changes to make: `count` steps `everyMs` apart from `startMs`, each
 *  flipping the next working or waiting interactive session in turn. */
export function replayPlan(agents, { startMs, everyMs = EVERY_MS, count = COUNT }) {
  const movers = agents.filter((a) => a.kind === "interactive" && a.status in FLIP).map((a) => a.sessionId);
  if (movers.length === 0) throw new Error("no working or waiting interactive session to replay");
  const state = new Map(agents.map((a) => [a.sessionId, a.status]));
  return Array.from({ length: count }, (_, i) => {
    const sessionId = movers[i % movers.length];
    const to = FLIP[state.get(sessionId)];
    state.set(sessionId, to);
    return { atMs: startMs + i * everyMs, sessionId, to };
  });
}

function utcDay(ms) {
  return new Date(ms).toISOString().slice(0, 10);
}

/** Applies `step` to the home at `dir` at `nowMs`: the agents fixture says
 *  the new status, and the event line the hook would write is appended (a
 *  permission prompt on the way to waiting, a prompt on the way back). */
export function applyStep(dir, step, nowMs) {
  const fixture = join(dir, "agents.json");
  const agents = JSON.parse(readFileSync(fixture, "utf8"));
  const agent = agents.find((a) => a.sessionId === step.sessionId);
  if (!agent) throw new Error(`no session ${step.sessionId} in ${fixture}`);
  agent.status = step.to;
  if (step.to === "waiting") agent.waitingFor = "permission prompt";
  else delete agent.waitingFor;
  const line =
    step.to === "waiting"
      ? { v: 1, ts: nowMs, event: "Notification", session_id: step.sessionId, cwd: agent.cwd, notification_type: "permission_prompt", message: "Claude needs your permission to use Bash" }
      : { v: 1, ts: nowMs, event: "UserPromptSubmit", session_id: step.sessionId, cwd: agent.cwd, prompt: "Go ahead" };
  // The fixture is replaced whole, so the fake claude never reads half of it.
  writeFileSync(`${fixture}.next`, JSON.stringify(agents));
  renameSync(`${fixture}.next`, fixture);
  appendFileSync(join(dir, "officina", "events", `${utcDay(nowMs)}.jsonl`), JSON.stringify(line) + "\n");
  return `${new Date(nowMs).toISOString()} ${agent.name}: ${step.to === "waiting" ? "office to decision office" : "decision office to office"}`;
}

function main() {
  const arg = (flag, fallback) => {
    const at = process.argv.indexOf(flag);
    return at >= 0 ? process.argv[at + 1] : fallback;
  };
  const dir = arg("--home");
  if (!dir || !existsSync(join(dir, "agents.json")) || !existsSync(join(dir, "officina", "events"))) {
    console.error("usage: node scripts/scene-replay.mjs --home <dir written by e2e-fixtures.mjs --home> [--every-ms 10000] [--count 60]");
    process.exit(64);
  }
  const everyMs = Number(arg("--every-ms", EVERY_MS));
  const count = Number(arg("--count", COUNT));
  const agents = JSON.parse(readFileSync(join(dir, "agents.json"), "utf8"));
  const plan = replayPlan(agents, { startMs: Date.now() + everyMs, everyMs, count });
  console.log(`replaying ${plan.length} changes, one every ${everyMs / 1000} s`);
  let next = 0;
  const tick = () => {
    console.log(applyStep(dir, plan[next], Date.now()));
    next += 1;
    if (next < plan.length) setTimeout(tick, plan[next].atMs - Date.now());
    else console.log("done");
  };
  setTimeout(tick, Math.max(0, plan[0].atMs - Date.now()));
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
