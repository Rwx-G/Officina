// Builds the boards the e2e suite and the README assets render, by running the
// real `officina board --json` against the fake claude (AD-17). Scenarios are
// hand-written and redacted: no real session data ever enters a fixture.
//
// Usage: node scripts/e2e-fixtures.mjs   (after `cargo build -p officina`)
import { execFileSync, spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const exe = process.platform === "win32" ? ".exe" : "";
const targetDir = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const officina = join(targetDir, "debug", `officina${exe}`);
const fakeClaude = join(targetDir, "debug", `officina-fake-claude${exe}`);
const outDir = join(root, "app", "e2e", ".fixtures");

const MIN = 60_000;
const HOUR = 60 * MIN;
const now = Date.now();
const sid = (n) => `00000000-0000-0000-0000-${String(n).padStart(12, "0")}`;
const work = (name) => `C:/work/${name}`;

function utcDay(ms) {
  return new Date(ms).toISOString().slice(0, 10);
}

const busyAgents = [
  { pid: 4101, cwd: work("billing-api"), kind: "interactive", startedAt: now - 3 * HOUR, sessionId: sid(1), name: "billing-api", status: "waiting", waitingFor: "permission prompt" },
  { pid: 4102, id: "a1b2c3d4", cwd: work("infra-scripts"), kind: "background", startedAt: now - 50 * MIN, sessionId: sid(2), name: "rotate-certs", status: "waiting", state: "blocked", waitingFor: "input needed" },
  { pid: 4103, cwd: work("web-portal"), kind: "interactive", startedAt: now - 2 * HOUR, sessionId: sid(3), name: "web-portal", status: "idle" },
  { pid: 4104, cwd: work("docs-site"), kind: "interactive", startedAt: now - 40 * MIN, sessionId: sid(4), name: "docs-site", status: "idle" },
  { pid: 4105, cwd: work("billing-api/src/invoices"), kind: "interactive", startedAt: now - 25 * MIN, sessionId: sid(5), name: "invoice-export", status: "busy" },
  { pid: 4106, id: "e5f6a7b8", cwd: work("web-portal"), kind: "background", startedAt: now - 15 * MIN, sessionId: sid(6), name: "upgrade-deps", status: "busy", state: "working" },
  { id: "c9d0e1f2", cwd: work("infra-scripts"), kind: "background", startedAt: now - 5 * MIN, sessionId: sid(7), name: "audit-playbooks" },
  { id: "b0000001", cwd: work("docs-site"), kind: "background", startedAt: now - 7 * HOUR, sessionId: sid(8), name: "nightly-linkcheck", state: "failed" },
  ...[1, 2, 3, 4, 5, 6, 7].map((i) => ({
    id: `d000000${i}`, cwd: work(i % 2 ? "web-portal" : "billing-api"), kind: "background",
    startedAt: now - (i + 1) * HOUR, sessionId: sid(20 + i), name: `report-${i}`, state: "done",
  })),
];

const busyEvents = [
  { ts: now - 4 * MIN, event: "Notification", session_id: sid(1), notification_type: "permission_prompt", message: "Claude needs your permission to use Bash" },
  { ts: now - 6 * MIN, event: "UserPromptSubmit", session_id: sid(1), prompt: "Run the migration dry-run against the staging schema and summarise the diff" },
  { ts: now - 12 * MIN, event: "UserPromptSubmit", session_id: sid(3), prompt: "Refactor the session middleware so tokens refresh before expiry" },
  { ts: now - 9 * MIN, event: "Stop", session_id: sid(3), last_message: "Done. Two options for the refresh window: 5 min or 10 % of the token lifetime. Which one do you want?" },
  { ts: now - 31 * MIN, event: "UserPromptSubmit", session_id: sid(4), prompt: "Proofread the install guide" },
  { ts: now - 28 * MIN, event: "Stop", session_id: sid(4), last_message: "Fixed 14 typos and one broken link. Ready for review." },
  { ts: now - 2 * MIN, event: "UserPromptSubmit", session_id: sid(5), prompt: "Add CSV export to the invoice list, with tests" },
  { ts: now - 3 * HOUR, event: "UserPromptSubmit", session_id: sid(30), cwd: work("billing-api"), prompt: "Investigate the flaky payment retry test" },
  { ts: now - 170 * MIN, event: "SessionEnd", session_id: sid(30), cwd: work("billing-api"), reason: "prompt_input_exit" },
  { ts: now - 90 * MIN, event: "UserPromptSubmit", session_id: sid(31), cwd: work("web-portal"), prompt: "Bump the chart library and fix the breaking changes" },
].map((e) => ({ v: 1, ...e }));

const quotaPayload = (fiveResetsS, weekResetsS) =>
  JSON.stringify({
    session_id: sid(1),
    rate_limits: {
      five_hour: { used_percentage: 62, resets_at: fiveResetsS },
      seven_day: { used_percentage: 41, resets_at: weekResetsS },
    },
  });

const scenarios = {
  busy: {
    agents: busyAgents,
    events: busyEvents,
    quota: quotaPayload(Math.floor((now + 2 * HOUR) / 1000), Math.floor((now + 3 * 24 * HOUR) / 1000)),
  },
  empty: { agents: [], events: [], quota: null },
  stale: { agents: busyAgents, events: busyEvents, quota: null, mode: "exit1" },
  "quota-reset": {
    agents: busyAgents.slice(4, 6),
    events: [],
    quota: quotaPayload(Math.floor((now - 10 * MIN) / 1000), Math.floor((now + 3 * 24 * HOUR) / 1000)),
  },
};

mkdirSync(outDir, { recursive: true });
for (const [name, scenario] of Object.entries(scenarios)) {
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(join(officinaHome, "events"), { recursive: true });
  const fixture = join(tmp, "agents.json");
  writeFileSync(fixture, JSON.stringify(scenario.agents));
  const byDay = new Map();
  for (const event of scenario.events) {
    const day = utcDay(event.ts);
    byDay.set(day, (byDay.get(day) ?? "") + JSON.stringify(event) + "\n");
  }
  for (const [day, content] of byDay) writeFileSync(join(officinaHome, "events", `${day}.jsonl`), content);

  const env = {
    SYSTEMROOT: process.env.SYSTEMROOT ?? "",
    USERPROFILE: home,
    HOME: home,
    OFFICINA_HOME: officinaHome,
    OFFICINA_CLAUDE_BIN: fakeClaude,
    FAKE_CLAUDE_FIXTURE: fixture,
    FAKE_CLAUDE_MODE: scenario.mode ?? "ok",
  };
  if (scenario.quota) execFileSync(officina, ["quota", "write"], { env, input: scenario.quota });
  const run = spawnSync(officina, ["board", "--json"], { env, encoding: "utf8" });
  if (run.status !== 0 && run.status !== 3) {
    throw new Error(`${name}: officina board exited ${run.status}: ${run.stderr}`);
  }
  writeFileSync(join(outDir, `${name}.json`), run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}

// Relay views (stories 2.6 and 2.7, D-25): `officina relay --json` on a home
// whose floor is hand-written and redacted, approved, stopped and resumed
// through the `approve_fixture` example, which calls the window's own
// authority functions: no hash is ever computed here, and no command line
// path of `officina` approves.
const relayFloor = `schema = 2

[[project]]
name = "infra-scripts"
path = "C:/work/infra-scripts"
actions = ["check", "apply"]

[[project]]
name = "docs-site"
path = "C:/work/docs-site"
autonomy = "free"
actions = ["linkcheck"]

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"
runner = "wsl"
distro = "Debian"
program = "/home/user01/venv/bin/ansible-playbook"
env = { ANSIBLE_CONFIG = "/home/user01/ansible.cfg" }
args = ["--check", "--diff", "-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[action.params.playbook]
values = ["alerting.yml", "node-exporter.yml"]

[[action]]
name = "apply"
description = "Apply a playbook checked in the last 30 minutes"
class = "after-check"
check = "check"
runner = "wsl"
distro = "Debian"
program = "/home/user01/venv/bin/ansible-playbook"
env = { ANSIBLE_CONFIG = "/home/user01/ansible.cfg" }
args = ["-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[action.params.playbook]
values = ["alerting.yml", "node-exporter.yml"]

[[action]]
name = "linkcheck"
description = "Check the links of the published docs"
class = "free"
program = "C:/path/to/linkcheck.exe"
args = ["--site", "{site}"]

[action.params.site]
values = ["docs"]
`;

// A session's edit: one more playbook value, an action that starts cmd.exe
// with markup in its description, the docs action dropped, and an invalid
// action whose name is markup.
const pendingFloor = relayFloor
  .replaceAll(`values = ["alerting.yml", "node-exporter.yml"]`, `values = ["alerting.yml", "backup.yml", "node-exporter.yml"]`)
  .replace(`actions = ["check", "apply"]`, `actions = ["check", "apply", "restart"]`)
  .replace(`actions = ["linkcheck"]`, `actions = []`)
  .replace(/\[\[action\]\]\nname = "linkcheck"[\s\S]*$/, "") +
  `[[action]]
name = "restart"
description = '<img src=x onerror="window.__officinaXss = 1">Restart <b>one</b> service'
class = "free"
program = "C:/Windows/System32/cmd.exe"
args = ["/c", "sc", "stop", "{service}"]

[action.params.service]
values = ["web01"]

[[action]]
name = "<b>bad</b>"
class = "free"
program = "relative.exe"

[[action]]
name = "status"
description = "Service status on one host"
class = "free"
program = "C:/Windows/System32/OpenSSH/ssh.exe"
args = ["web01", "systemctl", "status", "{unit}"]

[action.params.unit]
values = ["nginx.service"]
`;

// The floor edited again after the window displayed the pending change.
const againFloor = pendingFloor.replace(
  `check = "check"\n`,
  `check = "check"\ntimeout_s = 600\n`,
);

const relayScenarios = {
  "relay-clean": [["floor", relayFloor], ["approve"]],
  "relay-pending": [["floor", relayFloor], ["approve"], ["floor", pendingFloor]],
  "relay-pending-again": [["floor", relayFloor], ["approve"], ["floor", againFloor]],
  "relay-pending-approved": [["floor", relayFloor], ["approve"], ["floor", pendingFloor], ["approve"]],
  "relay-stopped": [["floor", relayFloor], ["approve"], ["stop"]],
};

function approveFixture(officinaHome, verb) {
  execFileSync(
    "cargo",
    ["run", "--quiet", "-p", "officina-host", "--example", "approve_fixture", "--features", "window-authority", "--", officinaHome, verb],
    { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
  );
}

for (const [name, steps] of Object.entries(relayScenarios)) {
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(home, { recursive: true });
  mkdirSync(officinaHome, { recursive: true });
  for (const [step, floor] of steps) {
    if (step === "floor") writeFileSync(join(officinaHome, "floor.toml"), floor);
    else approveFixture(officinaHome, step);
  }
  const env = { SYSTEMROOT: process.env.SYSTEMROOT ?? "", USERPROFILE: home, HOME: home, OFFICINA_HOME: officinaHome };
  const run = spawnSync(officina, ["relay", "--json"], { env, encoding: "utf8" });
  if (run.status !== 0) throw new Error(`${name}: officina relay exited ${run.status}: ${run.stderr}`);
  writeFileSync(join(outDir, `${name}.json`), run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}

// Anomalies (story 2.14, D-173): a catalogue approved and the relay stopped
// through `approve_fixture`, which start the window's decision record; then,
// behind the window's back, the stop flag deleted, an approval file written
// for a request the window never decided, and a run journaled with it.
// Printed by `officina relay --json`.
{
  const name = "relay-anomalies";
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(home, { recursive: true });
  mkdirSync(officinaHome, { recursive: true });
  writeFileSync(join(officinaHome, "floor.toml"), relayFloor);
  approveFixture(officinaHome, "approve");
  approveFixture(officinaHome, "stop");
  rmSync(join(officinaHome, "relay", "stopped"));
  const forgedId = `0123456789abcdef-${Date.now()}`;
  const approvalsDir = join(officinaHome, "relay", "approvals");
  mkdirSync(approvalsDir, { recursive: true });
  writeFileSync(
    join(approvalsDir, `${forgedId}.approved.json`),
    JSON.stringify({ decided_at_ms: Date.now(), request_key: "f".repeat(64) }),
  );
  const approvedHash = JSON.parse(readFileSync(join(officinaHome, "relay", "approved.json"), "utf8")).hash;
  const ranAt = Date.now();
  const run = {
    v: 1,
    ts: ranAt,
    kind: "run_started",
    run_id: "00000000000000000000000000f0f0f0",
    project: "infra-scripts",
    session_id: sid(2),
    action: "check",
    class: "free",
    params: { group: "lab-web", playbook: "alerting.yml" },
    catalogue_hash: approvedHash,
    approval_id: forgedId,
    timeout_s: 60,
  };
  mkdirSync(join(officinaHome, "journal"), { recursive: true });
  appendFileSync(join(officinaHome, "journal", `${utcDay(ranAt)}.jsonl`), JSON.stringify(run) + "\n");
  const env = { SYSTEMROOT: process.env.SYSTEMROOT ?? "", USERPROFILE: home, HOME: home, OFFICINA_HOME: officinaHome };
  const view = spawnSync(officina, ["relay", "--json"], { env, encoding: "utf8" });
  if (view.status !== 0) throw new Error(`${name}: officina relay exited ${view.status}: ${view.stderr}`);
  const kinds = new Set(JSON.parse(view.stdout).anomalies.map((a) => a.kind));
  for (const kind of ["foreign_approval", "run_without_decision", "stop_flag_removed"]) {
    if (!kinds.has(kind)) throw new Error(`${name}: no ${kind} anomaly: ${view.stdout}`);
  }
  writeFileSync(join(outDir, `${name}.json`), view.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${view.status}`);
}

// Approval requests (story 2.8, D-25, D-170): a catalogue approved through
// `approve_fixture`, a dry run and two requests made by the real
// `officina run --wait 1` (one from a session the busy board lists), and one
// forged request file whose key belongs to another parameter set, which the
// board must show as invalid. The board is `officina board --json` on the
// busy sessions.
{
  const name = "approvals";
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  const project = join(tmp, "work", "infra-scripts");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(join(officinaHome, "events"), { recursive: true });
  mkdirSync(project, { recursive: true });
  const fakeTool = join(targetDir, "debug", `officina-fake-tool${exe}`).replaceAll("\\", "/");
  writeFileSync(
    join(officinaHome, "floor.toml"),
    `schema = 2

[[project]]
name = "infra-scripts"
path = "${project.replaceAll("\\", "/")}"
actions = ["check", "apply", "restart"]

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"
program = "${fakeTool}"
env = { FAKE_TOOL_MODE = "echo" }
args = ["TASK [deploy {group}]", "changed=1", "unreachable=0"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply"
description = "Apply the playbook checked by its dry run"
class = "human"
check = "check"
program = "/home/user01/venv/bin/ansible-playbook"
runner = "wsl"
args = ["-i", "inventory/lab", "--limit", "{group}", "playbooks/site.yml"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "restart"
description = "Restart one service"
class = "human"
program = "C:/path/to/service-tool.exe"
args = ["restart", "{service}"]

[action.params.service]
values = ["web01", "db01"]
`,
  );
  approveFixture(officinaHome, "approve");
  const base = { SYSTEMROOT: process.env.SYSTEMROOT ?? "", USERPROFILE: home, HOME: home, OFFICINA_HOME: officinaHome };
  const relayRun = (args, extra = {}) => {
    const run = spawnSync(officina, ["run", ...args], { env: { ...base, ...extra }, cwd: project, encoding: "utf8" });
    if (run.status !== 0 && run.status !== 126) console.error(run.stderr);
    return run.status;
  };
  if (relayRun(["check", "group=lab-web"]) !== 0) throw new Error(`${name}: the dry run failed`);
  // From the session of the busy board's first card, then from a terminal.
  if (relayRun(["apply", "group=lab-web", "--wait", "1"], { CLAUDE_CODE_SESSION_ID: sid(1) }) !== 126) {
    throw new Error(`${name}: the apply request did not wait`);
  }
  if (relayRun(["restart", "service=db01", "--wait", "1"]) !== 126) {
    throw new Error(`${name}: the restart request did not wait`);
  }
  const approvalsDir = join(officinaHome, "relay", "approvals");
  const restartFile = readdirSync(approvalsDir)
    .filter((f) => f.endsWith(".request.json"))
    .map((f) => join(approvalsDir, f))
    .find((f) => JSON.parse(readFileSync(f, "utf8")).action === "restart");
  const forged = JSON.parse(readFileSync(restartFile, "utf8"));
  forged.params = { service: "web01" };
  forged.argv = ["C:/path/to/harmless.exe"];
  forged.created_at_ms += 1;
  writeFileSync(
    join(approvalsDir, `${forged.request_key.slice(0, 16)}-${forged.created_at_ms}.request.json`),
    JSON.stringify(forged, null, 2),
  );

  const fixture = join(tmp, "agents.json");
  writeFileSync(fixture, JSON.stringify(busyAgents));
  const byDay = new Map();
  for (const event of busyEvents) {
    const day = utcDay(event.ts);
    byDay.set(day, (byDay.get(day) ?? "") + JSON.stringify(event) + "\n");
  }
  for (const [day, content] of byDay) writeFileSync(join(officinaHome, "events", `${day}.jsonl`), content);
  const env = { ...base, OFFICINA_CLAUDE_BIN: fakeClaude, FAKE_CLAUDE_FIXTURE: fixture, FAKE_CLAUDE_MODE: "ok" };
  const run = spawnSync(officina, ["board", "--json"], { env, encoding: "utf8" });
  if (run.status !== 0) throw new Error(`${name}: officina board exited ${run.status}: ${run.stderr}`);
  const board = JSON.parse(run.stdout);
  if (board.approvals.length !== 3 || board.approvals.filter((a) => a.valid).length !== 2) {
    throw new Error(`${name}: expected two valid requests and a forged one: ${JSON.stringify(board.approvals)}`);
  }
  writeFileSync(join(outDir, `${name}.json`), run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}

// Relay activity (story 2.9, D-25): a catalogue approved through
// `approve_fixture`; real runs and refusals through `officina run` with the
// fake tool; a gate deny and a gate error through `officina hook`; then, as
// v0.1 seeds events, a seeded journal adds a run whose end line never came
// (abandoned) and an unreadable line; last, a floor edit leaves a pending
// change with an issue. Printed by `officina relay --json`.
{
  const name = "relay-activity";
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  const project = join(tmp, "work", "infra-scripts");
  mkdirSync(home, { recursive: true });
  mkdirSync(officinaHome, { recursive: true });
  mkdirSync(project, { recursive: true });
  const fakeTool = join(targetDir, "debug", `officina-fake-tool${exe}`).replaceAll("\\", "/");
  const floorFile = join(officinaHome, "floor.toml");
  const floor = `schema = 2

[[project]]
name = "infra-scripts"
path = "${project.replaceAll("\\", "/")}"
actions = ["check", "apply", "probe", "health", "status-page"]

[[credential]]
name = "monitoring"
hosts = ["monitoring.example.com"]
header = "Authorization"
scheme = "Bearer"

[[credential]]
name = "backup"
hosts = ["backup.example.com"]
header = "X-Api-Key"
scheme = "Token"

[[action]]
name = "health"
description = "Read one check of the monitoring API"
class = "free"
kind = "http"
method = "GET"
url = "https://monitoring.example.com/api/{check}?verbose=1"
credential = "monitoring"

[action.params.check]
values = ["health", "ready"]

[[action]]
name = "status-page"
description = "Is the local status page up"
class = "free"
kind = "http"
method = "HEAD"
url = "http://127.0.0.1:9/status"

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"
program = "${fakeTool}"
env = { FAKE_TOOL_MODE = "echo" }
args = ["--check", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply"
description = "Apply a playbook checked in the last 30 minutes"
class = "after-check"
check = "check"
program = "${fakeTool}"
env = { FAKE_TOOL_MODE = "echo" }
args = ["--apply", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "probe"
description = "Read one host's health"
class = "free"
program = "${fakeTool}"
env = { FAKE_TOOL_MODE = "exit:3" }
args = ["--host", "{host}"]

[action.params.host]
values = ["web01", "db01"]
`;
  writeFileSync(floorFile, floor);
  approveFixture(officinaHome, "approve");
  const base = { SYSTEMROOT: process.env.SYSTEMROOT ?? "", USERPROFILE: home, HOME: home, OFFICINA_HOME: officinaHome };
  const relayRun = (args, expected) => {
    const run = spawnSync(officina, ["run", ...args], {
      env: { ...base, CLAUDE_CODE_SESSION_ID: sid(1) },
      cwd: project,
      encoding: "utf8",
    });
    if (run.status !== expected) throw new Error(`${name}: run ${args.join(" ")} exited ${run.status}: ${run.stderr}`);
  };
  relayRun(["check", "group=lab-web"], 0);
  relayRun(["apply", "group=lab-web"], 0);
  relayRun(["probe", "host=db01"], 3);
  relayRun(["apply", "group=lab-db"], 126);
  relayRun(["probe", "host=<b>web02</b>"], 126);
  // A real HTTP run with no response (nothing listens on that local port);
  // `health` is never run here: its credential would read the real store.
  relayRun(["status-page"], 127);

  const hook = (command) => {
    const payload = {
      cwd: project,
      hook_event_name: "PreToolUse",
      session_id: sid(1),
      tool_name: "Bash",
      tool_input: { command, description: "fixture" },
    };
    const run = spawnSync(officina, ["hook"], { env: base, input: JSON.stringify(payload), encoding: "utf8" });
    if (run.status !== 0) throw new Error(`${name}: officina hook exited ${run.status}: ${run.stderr}`);
    return run.stdout;
  };
  const denied = hook(`ssh host01 'echo <img src=x onerror="window.__officinaXss = 1">'`);
  if (!denied.includes("deny")) throw new Error(`${name}: the gate did not deny: ${denied}`);
  // A floor that fails phase 1 while a catalogue is approved: the gate falls
  // back to the snapshot and journals a gate error. The unknown key goes
  // first: after a table header it would belong to that table.
  writeFileSync(floorFile, `not_a_floor_key = 1\n${floor}`);
  hook("ls");
  writeFileSync(floorFile, floor);

  const approvedHash = JSON.parse(readFileSync(join(officinaHome, "relay", "approved.json"), "utf8")).hash;
  const abandonedAt = Date.now() - 20 * MIN;
  const abandoned = {
    v: 1,
    ts: abandonedAt,
    kind: "run_started",
    run_id: "00000000000000000000000000abcdef",
    project: "infra-scripts",
    session_id: sid(3),
    action: "probe",
    class: "free",
    params: { host: "web01" },
    argv: [fakeTool, "--host", "web01"],
    catalogue_hash: approvedHash,
    timeout_s: 60,
  };
  appendFileSync(join(officinaHome, "journal", `${utcDay(abandonedAt)}.jsonl`), JSON.stringify(abandoned) + "\n");
  appendFileSync(join(officinaHome, "journal", `${utcDay(Date.now())}.jsonl`), "an unreadable line\n");

  writeFileSync(
    floorFile,
    floor.replace(`values = ["web01", "db01"]`, `values = ["web01", "db01", "web02"]`) +
      `
[[action]]
name = "<i>broken</i>"
class = "free"
program = "relative.exe"
`,
  );
  const run = spawnSync(officina, ["relay", "--json"], { env: base, encoding: "utf8" });
  if (run.status !== 0) throw new Error(`${name}: officina relay exited ${run.status}: ${run.stderr}`);
  const view = JSON.parse(run.stdout);
  const kinds = new Set(view.recent.map((entry) => entry.event.kind));
  const states = new Set(view.recent.filter((e) => e.event.kind === "run").map((e) => e.event.state));
  for (const kind of ["run", "refused", "gate_denied", "gate_error"]) {
    if (!kinds.has(kind)) throw new Error(`${name}: no ${kind} entry: ${run.stdout}`);
  }
  if (!states.has("abandoned") || !states.has("finished")) throw new Error(`${name}: runs ${[...states]}`);
  if (!view.pending || view.issues.length === 0 || view.journal_corrupt_lines === 0 || view.gate_errors_24h === 0) {
    throw new Error(`${name}: expected a pending change, issues and journal diagnostics: ${run.stdout}`);
  }
  writeFileSync(join(outDir, `${name}.json`), run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}
