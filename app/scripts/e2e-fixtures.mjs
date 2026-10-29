// Builds the boards the e2e suite and the README assets render, by running the
// real `officina board --json` against the fake claude (AD-17). Scenarios are
// hand-written and redacted: no real session data ever enters a fixture.
//
// Usage: node scripts/e2e-fixtures.mjs   (after `cargo build -p officina`)
//        node scripts/e2e-fixtures.mjs --home <dir> <scene scenario>
//   writes that scene scenario's Officina home and user profile under the
//   new or empty folder <dir> and prints the environment to start
//   officina-app.exe with (AD-71, D-120), instead of the fixtures.
import { execFileSync, spawnSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { checkSceneIntents, sceneScenarios, writeSceneHome } from "./scene-fixtures.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const exe = process.platform === "win32" ? ".exe" : "";
const targetDir = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const officina = join(targetDir, "debug", `officina${exe}`);
const fakeClaude = join(targetDir, "debug", `officina-fake-claude${exe}`);
const outDir = join(root, "app", "e2e", ".fixtures");

/** Each fixture file written, and its kind (D-181). */
const fixtureKinds = {};
function writeFixture(name, kind, text) {
  writeFileSync(join(outDir, `${name}.json`), text);
  fixtureKinds[`${name}.json`] = kind;
}

// --home: one scene scenario as a home for the real window, nothing else.
const homeFlag = process.argv.indexOf("--home");
if (homeFlag >= 0) {
  const [dir, name] = process.argv.slice(homeFlag + 1);
  const scenarios = sceneScenarios(Date.now());
  if (!dir || !name || !scenarios[name]) {
    console.error(`usage: node scripts/e2e-fixtures.mjs --home <dir> <${Object.keys(scenarios).join("|")}>`);
    process.exit(64);
  }
  if (existsSync(dir) && readdirSync(dir).length > 0) {
    console.error(`${dir} exists and is not empty: give a new or empty folder`);
    process.exit(64);
  }
  mkdirSync(dir, { recursive: true });
  const { env } = writeSceneHome({ dir, name, scenario: scenarios[name], officina, fakeClaude, approveFixture });
  console.log(`# ${name} written under ${dir}. Quit any running Officina (single instance), then start`);
  console.log("# officina-app.exe from a PowerShell with this environment:");
  for (const [key, value] of Object.entries(env)) console.log(`$env:${key} = "${value}"`);
  process.exit(0);
}

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
  // Story 3.2 AC 8: a registry file holding bytes that are no database; the
  // board reads it read-only, reports it unavailable and keeps its sessions.
  "registry-unavailable": {
    agents: busyAgents,
    events: busyEvents,
    quota: null,
    registry: "this is not a SQLite database\n".repeat(64),
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
  if (scenario.registry) {
    mkdirSync(join(officinaHome, "registry"), { recursive: true });
    writeFileSync(join(officinaHome, "registry", "registry.db"), scenario.registry);
  }

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
  writeFixture(name, "board", run.stdout);
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
actions = ["check", "apply", "apply-unattended"]

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
name = "apply-unattended"
description = "Apply the alerting playbook after its dry run, without asking"
class = "auto"
check = "check"
auto_max_per_hour = 4
runner = "wsl"
distro = "Debian"
program = "/home/user01/venv/bin/ansible-playbook"
env = { ANSIBLE_CONFIG = "/home/user01/ansible.cfg" }
args = ["-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[action.params.playbook]
values = ["alerting.yml"]

[[template]]
name = "dependency-bump"
description = "Bump one dependency and fix what breaks"
projects = ["infra-scripts", "docs-site"]
prompt = "Bump {package} to its latest {level} release, run the tests, fix what breaks, commit."
model = "sonnet"

[template.params.package]
values = ["serde_json", "@scope/pkg"]

[template.params.level]
values = ["patch", "minor"]

[auto_review]
model = "haiku"

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

// Story 5.4 (AD-76, D-131): the golden floor of the catalogue hash test,
// approved by the previous version in the previous canonical format, read
// today: the pending change shows the format change and the templates added.
const goldenFloor = readFileSync(join(root, "crates", "officina-core", "tests", "data", "relay", "golden-floor.toml"), "utf8");

const relayScenarios = {
  "relay-clean": [["floor", relayFloor], ["approve"]],
  "relay-previous-format": [["floor", goldenFloor], ["previous-format"]],
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
  writeFixture(name, "relay", run.stdout);
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
  writeFixture(name, "relay", view.stdout);
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
  writeFixture(name, "board", run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}

// An `auto` request beside a `human` one (story 5.12, AD-86): a catalogue
// approved through `approve_fixture`, the `auto` action's check run, then the
// `auto` request made by the real `officina run --wait 1` from the busy
// board's working session, and a `human` request from a terminal. The board
// is `officina board --json` on the busy sessions.
{
  const name = "approvals-auto";
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
description = "Apply the playbook checked by its dry run, unattended once allowed"
class = "auto"
check = "check"
program = "${fakeTool}"
args = ["apply", "{group}"]

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
  if (relayRun(["apply", "group=lab-web", "--wait", "1"], { CLAUDE_CODE_SESSION_ID: sid(5) }) !== 126) {
    throw new Error(`${name}: the auto request did not wait`);
  }
  if (relayRun(["restart", "service=db01", "--wait", "1"]) !== 126) {
    throw new Error(`${name}: the restart request did not wait`);
  }
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
  const classes = board.approvals.map((a) => `${a.class}:${a.state}:${a.valid}`).sort();
  if (classes.join(",") !== "auto:pending:true,human:pending:true") {
    throw new Error(`${name}: expected one valid auto and one valid human request: ${JSON.stringify(board.approvals)}`);
  }
  writeFixture(name, "board", run.stdout);
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
  if (!view.pending || view.issues.length === 0 || view.journal_corrupt_lines === 0 || view.gate_errors === 0) {
    throw new Error(`${name}: expected a pending change, issues and journal diagnostics: ${run.stdout}`);
  }
  writeFixture(name, "relay", run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}

// Subjects (story 3.5, D-72, D-82): event files written here, then the
// window's own writer, through the `registry_fixture` example at an explicit
// `now_ms`, ingests them and runs the author's actions; with that writer
// gone, the real `officina subjects --json` and `officina board --json`
// print the fixtures. No SQL from Node, no command line write path.
const registryFixture = join(targetDir, "debug", "examples", `registry_fixture${exe}`);
execFileSync(
  "cargo",
  ["build", "--quiet", "--locked", "-p", "officina-host", "--example", "registry_fixture", "--features", "window-authority"],
  { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
);

const hostile = `<img src=x onerror="window.__officinaXss = 1">`;
const subjectsAgents = [
  { pid: 4201, cwd: work("billing-api"), kind: "interactive", startedAt: now - 40 * MIN, sessionId: sid(101), name: "invoice-export", status: "busy" },
];
const prompted = (n, project, at, prompt) => ({ ts: at, event: "UserPromptSubmit", session_id: sid(n), cwd: work(project), prompt });
// The hook keeps the start of a long answer and its end apart (AD-45); a
// hand-written line carries both, as the hook would write them.
const stopped = (n, project, at, message, tail) => ({
  ts: at,
  event: "Stop",
  session_id: sid(n),
  cwd: work(project),
  last_message: message,
  ...(tail ? { last_message_tail: tail } : {}),
});
const ended = (n, project, at, reason) => ({ ts: at, event: "SessionEnd", session_id: sid(n), cwd: work(project), reason });
const nonInteractive = [...Array(12).keys()].flatMap((i) => [
  { ts: now - (3 * HOUR) + i * MIN, event: "SessionStart", session_id: sid(300 + i), cwd: work("billing-api"), source: "startup" },
  ended(300 + i, "billing-api", now - (3 * HOUR) + i * MIN + 20_000, "other"),
]);
const answerStart = "Found it: the retry test shares a clock with the payment mock, so a slow run sees the retry window close early.";
const answerEnd = "...the clock is pinned per test and the shared fixture is gone. Both suites pass ten times in a row now.";
const subjectsEvents = [
  prompted(102, "billing-api", now - 26 * HOUR, "Sketch the CSV export of invoices"),
  stopped(102, "billing-api", now - 26 * HOUR + 5 * MIN, "Here is a plan in three steps."),
  ended(102, "billing-api", now - 26 * HOUR + 6 * MIN, "prompt_input_exit"),
  prompted(101, "billing-api", now - 3 * MIN, "Add CSV export to the invoice list, with tests"),
  prompted(104, "billing-api", now - 5 * HOUR, "Investigate the flaky payment retry test"),
  stopped(104, "billing-api", now - 5 * HOUR + 20 * MIN, answerStart, answerEnd),
  ended(104, "billing-api", now - 5 * HOUR + 21 * MIN, "prompt_input_exit"),
  prompted(105, "web-portal", now - 2 * 24 * HOUR, "Clean up the ledger exports"),
  ended(105, "web-portal", now - 2 * 24 * HOUR + 30 * MIN, "prompt_input_exit"),
  prompted(106, "docs-site", now - 3 * 24 * HOUR, "Draft the release notes"),
  ended(106, "docs-site", now - 3 * 24 * HOUR + 10 * MIN, "prompt_input_exit"),
  prompted(107, "docs-site", now - 3 * 24 * HOUR, "Draft the release notes again"),
  ended(107, "docs-site", now - 3 * 24 * HOUR + 12 * MIN, "prompt_input_exit"),
  prompted(110, "docs-site", now - 70 * MIN, hostile),
  ended(110, "docs-site", now - 60 * MIN, "prompt_input_exit"),
  prompted(111, "web-portal", now - 50 * MIN, "Bump the chart library"),
  ended(111, "web-portal", now - 45 * MIN, "logout"),
  ...nonInteractive,
].map((e) => ({ v: 1, ...e }));

// Subject ids follow creation order in a fresh registry.
const create = (name, origin) => ({ author: { kind: "create", name, origin } });
const subjectsSteps = [
  "ingest",
  { agents: subjectsAgents },
  create("invoice export", { sessions: [sid(101), sid(102)] }),
  create("flaky retry", { sessions: [sid(104)] }),
  create("ledger cleanup", { sessions: [sid(105)] }),
  { author: { kind: "archive", subject: 3, routine: false } },
  { author: { kind: "reopen", subject: 3 } },
  create("release notes", { sessions: [sid(106)] }),
  { author: { kind: "archive", subject: 4, routine: true } },
  create("release notes draft", { sessions: [sid(107)] }),
  create("notes copy", { project: "docs-site" }),
  { author: { kind: "merge", source: 6, target: 5 } },
  { author: { kind: "archive", subject: 5, routine: false } },
  create(hostile, { sessions: [sid(110)] }),
];

// What every Subjects scenario must show, beside its own checks.
const subjectsChecks = (printed, board) => [
  [printed.subjects.some((s) => s.status === "active") && printed.subjects.some((s) => s.status === "to_resume"), "active and to_resume subjects"],
  [printed.subjects.some((s) => s.sessions.some((x) => x.card !== null)), "a session joined to its live card"],
  [printed.unfiled.some((p) => p.non_interactive_count > p.non_interactive.length), "non-interactive runs counted beyond those listed"],
  [board.columns.working.some((c) => c.subject !== null), "a board card naming its subject"],
];

// Triage (story 3.7, D-72): a subject with two ended sessions, a background
// session Claude Code reports done (proposed from its state), a session cut
// mid-turn under a subject whose name is markup, an empty subject, an
// unfiled session whose project has one open subject and one whose project
// has none, its prompt markup too.
const triageAgents = [
  { pid: 4301, id: "bg0001", cwd: work("ops-scripts"), kind: "background", startedAt: now - 3 * HOUR, sessionId: sid(203), name: "nightly-report", status: "idle", state: "done" },
];
const triageEvents = [
  prompted(201, "billing-api", now - 30 * HOUR, "Sketch the CSV export of invoices"),
  stopped(201, "billing-api", now - 30 * HOUR + 4 * MIN, "Here is a plan in three steps."),
  ended(201, "billing-api", now - 30 * HOUR + 5 * MIN, "prompt_input_exit"),
  prompted(202, "billing-api", now - 6 * HOUR, "Write the CSV export, with tests"),
  stopped(202, "billing-api", now - 6 * HOUR + 30 * MIN, "The export is in place.", "...and both suites pass ten times in a row."),
  ended(202, "billing-api", now - 6 * HOUR + 31 * MIN, "prompt_input_exit"),
  prompted(203, "ops-scripts", now - 3 * HOUR, "Build the nightly usage report"),
  stopped(203, "ops-scripts", now - 2 * HOUR, "Report built and saved."),
  prompted(204, "ops-scripts", now - 4 * HOUR, "Rotate the logs"),
  stopped(204, "ops-scripts", now - 4 * HOUR + 2 * MIN, "Rotation configured."),
  prompted(204, "ops-scripts", now - 4 * HOUR + 3 * MIN, "Now compress the old ones"),
  ended(204, "ops-scripts", now - 4 * HOUR + 4 * MIN, "logout"),
  prompted(205, "billing-api", now - 2 * HOUR, "Fix the rounding in the invoice totals"),
  stopped(205, "billing-api", now - 2 * HOUR + 10 * MIN, "Rounded half up, with a test."),
  ended(205, "billing-api", now - 2 * HOUR + 11 * MIN, "prompt_input_exit"),
  prompted(206, "web-portal", now - 90 * MIN, hostile),
  stopped(206, "web-portal", now - 85 * MIN, "Checked."),
  ended(206, "web-portal", now - 80 * MIN, "prompt_input_exit"),
  { ts: now - 5 * HOUR, event: "SessionStart", session_id: sid(207), cwd: work("docs-site"), source: "startup" },
  ended(207, "docs-site", now - 5 * HOUR + MIN, "prompt_input_exit"),
].map((e) => ({ v: 1, ...e }));
const triageSteps = [
  "ingest",
  { agents: triageAgents },
  create("invoice export", { sessions: [sid(201), sid(202)] }),
  create("nightly report", { sessions: [sid(203)] }),
  create(hostile, { sessions: [sid(204)] }),
  create("release notes", { project: "docs-site" }),
];
const triageChecks = (printed) => {
  const rows = printed.triage;
  const kinds = new Set(rows.map((r) => r.kind));
  const unfiled = rows.filter((r) => r.kind === "unfiled_session");
  return [
    [["subject", "empty_subject", "unfiled_session"].every((k) => kinds.has(k)), "a row of every kind"],
    [rows.some((r) => r.proposal.source === "state"), "a row proposed from Claude Code's state"],
    [rows.some((r) => r.proposal.reason === "cut_mid_turn"), "a row cut mid-turn"],
    [unfiled.some((r) => r.proposed_subject !== null) && unfiled.some((r) => r.proposed_subject === null && r.choices.length === 0), "an unfiled row with one obvious subject and one with none"],
    [printed.subjects.some((s) => s.sessions.length > 1 && rows.some((r) => r.subject?.id === s.id)), "a row for a subject of several sessions"],
    [printed.to_sort === rows.length, "the count of rows to sort"],
  ];
};

// Evidence (stories 3.9 and 3.10, D-72), at explicit times: a nightly job
// whose state proposals the author confirmed (two flipped, then fifty
// kept), auto-archive turned on with its gate holding and three of the
// job's later runs auto-archived (one over 7 days ago, two within them, the
// last reopened since), the pass that then finds the gate broken; empty
// subjects confirmed and flipped; sessions confirmed from their default
// proposal; a dry run of the disabled model on them (answered by the
// fixture tool, no process started) and a call refused before it started.
const DAY = 24 * HOUR;
const evidenceTasks = [
  ["Add CSV export to the invoice list", "finished", "finished"],
  ["Fix the rounding in the totals", "finished", "to_resume"],
  ["Investigate the flaky retry test", "to_resume", "to_resume"],
  ["Draft the release notes", "finished", "unsure"],
  ["Bump the chart library", "to_resume", "finished"],
  ["Rotate the logs", "to_resume", "timeout"],
];
// The nightly job's runs: ids 500 and up; when each is reported done.
const nightly = [
  ...[0, 1].map((i) => ({ n: 500 + i, doneAt: now - 16 * DAY, verdict: "to_resume" })),
  ...[...Array(50).keys()].map((i) => ({ n: 510 + i, doneAt: now - 15 * DAY, verdict: "finished" })),
];
const autoRuns = [
  { n: 600, doneAt: now - 10 * DAY },
  { n: 601, doneAt: now - 3 * DAY },
  { n: 602, doneAt: now - 2 * DAY },
];
const nightlyAgent = ({ n, doneAt }) => ({
  pid: 5000 + n,
  id: `bg${String(n).padStart(4, "0")}`,
  cwd: work("ops-nightly"),
  kind: "background",
  startedAt: doneAt - HOUR,
  sessionId: sid(n),
  name: `nightly-${n}`,
  status: "idle",
  state: "done",
});
const evidenceEvents = [
  ...evidenceTasks.flatMap(([task], i) => {
    const at = now - 20 * HOUR + i * 30 * MIN;
    return [
      prompted(400 + i, "billing-api", at, task),
      stopped(400 + i, "billing-api", at + 5 * MIN, "Done, with tests."),
      ended(400 + i, "billing-api", at + 6 * MIN, "prompt_input_exit"),
    ];
  }),
  ...[...nightly, ...autoRuns].flatMap(({ n, doneAt }) => [
    prompted(n, "ops-nightly", doneAt - HOUR, "Build the nightly usage report"),
    stopped(n, "ops-nightly", doneAt - 30 * MIN, "Report built and saved."),
  ]),
].map((e) => ({ v: 1, ...e }));
const nightlyStep = (run, verdict) => {
  const subject = `nightly report ${run.n}`;
  return [
    { at: run.doneAt },
    { agents: [nightlyAgent(run)] },
    { author: { kind: "create", name: subject, origin: { sessions: [sid(run.n)] } } },
    ...(verdict ? [{ at: run.doneAt + MIN }, { confirm: { key: `subject:SUBJECT(${subject})`, verdict } }] : []),
  ];
};
const evidenceSteps = [
  { at: now - 17 * DAY },
  "ingest",
  ...nightly.flatMap((run) => nightlyStep(run, run.verdict)),
  { at: now - 15 * DAY + HOUR },
  { auto_archive: true },
  ...autoRuns.flatMap((run) => [...nightlyStep(run, null), { at: run.doneAt + MIN }, "auto_archive_pass"]),
  { at: now - DAY },
  { author: { kind: "reopen", subject: "SUBJECT(nightly report 602)" } },
  { at: now - 12 * HOUR },
  "auto_archive_pass",
  { at: now - 6 * HOUR },
  { author: { kind: "create", name: "spike on caching", origin: { project: "billing-api" } } },
  { author: { kind: "create", name: "spike on queues", origin: { project: "billing-api" } } },
  { at: now - 5 * HOUR },
  { confirm: { key: "subject:SUBJECT(spike on caching)", verdict: "finished" } },
  { confirm: { key: "subject:SUBJECT(spike on queues)", verdict: "to_resume" } },
  { at: now - 2 * HOUR },
  ...evidenceTasks.map(([task, verdict], i) => ({
    confirm: { key: `session:${sid(400 + i)}`, verdict, file_under: { new: { name: task } } },
  })),
  { at: now - HOUR },
  { dry_run: evidenceTasks.map(([, , answer]) => answer).reverse() },
  { at: now - 30 * MIN },
  { refused_call: "cmd_shim" },
];
const evidenceChecks = (printed) => {
  const run = printed.model.latest_dry_run;
  const rate = (source) => printed.rates.find((r) => r.source === source);
  return [
    [run !== null && run.tally.cases === evidenceTasks.length, "a dry run over every case"],
    [run !== null && run.tally.unanswered > 0 && run.tally.model_false_finished > 0, "unanswered and false finished cases"],
    [printed.model.calls.today.refused === 1 && printed.model.calls.refusals.length === 1, "a refused call with its cause"],
    [printed.model.enabled === false, "the model disabled"],
    [rate("state").finished !== null && rate("state").finished.cases === 50 && rate("state").finished.flipped === 1, "a state source with one flip among its last 50 finished"],
    [rate("rules").finished !== null && rate("rules").all.flipped > 0, "a rules source with empty subjects and flips"],
    [rate("model").finished === null, "a source with no finished proposal"],
    [printed.auto_archive.on === false && printed.auto_archive.reason === "too_many_flips" && printed.auto_archive.src === "rules", "auto-archive turned off by its broken gate"],
    [printed.recent_auto_archives.length === 2 && printed.recent_auto_archives.some((a) => a.archived) && printed.recent_auto_archives.some((a) => a.reopened_at_ms !== null), "recent auto-archives, one reopened"],
  ];
};

// The model (story 3.11, D-72): sessions confirmed from their default
// proposal, a dry run the model wins on them, the model turned on, then the
// rows that fall to the default sent to it in turn (answered by the fixture
// tool, no process started): the latest answered with a reason that is
// markup, the others left pending. The paused scenario then ends the next
// three calls without an answer, which pauses the model.
const modelCases = [...Array(32).keys()].map((i) => ({ n: 700 + i, verdict: i < 20 ? "finished" : "to_resume" }));
const modelPending = [
  ["Bump the chart library", 740],
  ["Rotate the logs", 741],
  ["Draft the release notes", 742],
  ["Add CSV export to the invoice list", 743],
];
const modelEvents = [
  ...modelCases.flatMap(({ n }, i) => {
    const at = now - 30 * HOUR + i * 10 * MIN;
    return [
      prompted(n, "billing-api", at, `Task ${i} of the sprint`),
      stopped(n, "billing-api", at + 5 * MIN, "Done, with tests."),
      ended(n, "billing-api", at + 6 * MIN, "prompt_input_exit"),
    ];
  }),
  ...modelPending.flatMap(([task, n], i) => {
    const at = now - 3 * HOUR + i * 20 * MIN;
    return [
      prompted(n, "web-portal", at, task),
      stopped(n, "web-portal", at + 5 * MIN, "Done."),
      ended(n, "web-portal", at + 6 * MIN, "prompt_input_exit"),
    ];
  }),
].map((e) => ({ v: 1, ...e }));
const modelSteps = [
  { at: now - 4 * HOUR },
  "ingest",
  ...modelCases.map(({ n, verdict }, i) => ({
    confirm: { key: `session:${sid(n)}`, verdict, file_under: { new: { name: `sprint task ${i}` } } },
  })),
  { at: now - 2 * HOUR },
  { dry_run: modelCases.map(({ verdict }) => verdict).reverse() },
  { at: now - 50 * MIN },
  { triage_model: true },
  { at: now - 40 * MIN },
  { proposals: [{ answer: "finished", reason: hostile }] },
];
const modelChecks = (printed) => {
  const rows = printed.triage;
  return [
    [printed.model.enabled === true && printed.gates.model_enable.state === "holds", "the model on, its gate holding"],
    [rows.some((r) => r.proposal.source === "model" && r.model_reason === hostile), "a model proposal with its markup reason"],
  ];
};
const modelOpenChecks = (printed) => [
  ...modelChecks(printed),
  [printed.model.paused_at_ms === null && printed.triage.some((r) => r.model_pending), "rows pending for the model"],
];
const modelPausedChecks = (printed) => [
  ...modelChecks(printed),
  [printed.model.paused_at_ms !== null && printed.triage.every((r) => !r.model_pending), "the model paused, nothing pending"],
  [printed.triage.filter((r) => r.model_call !== null).length === 3, "three rows whose call gave no answer"],
];

// Subject ids follow creation order in a fresh registry: an author's
// `create`, and a confirmation filing a session under a new subject. Steps
// name a subject as `SUBJECT(<name>)`, resolved here to its id.
function resolveSubjects(steps) {
  const ids = new Map();
  const created = (name) => ids.set(name, ids.size + 1);
  const resolve = (text) =>
    text.replace(/SUBJECT\(([^)]*)\)/g, (_, name) => {
      if (!ids.has(name)) throw new Error(`no subject ${name} created before it is named`);
      return String(ids.get(name));
    });
  return steps.map((step) => {
    if (step?.author?.kind === "create") created(step.author.name);
    if (step?.confirm?.file_under?.new) created(step.confirm.file_under.new.name);
    const resolved = JSON.parse(resolve(JSON.stringify(step)));
    if (typeof resolved?.author?.subject === "string") resolved.author.subject = Number(resolved.author.subject);
    return resolved;
  });
}

// The author's verdicts on ended sessions (story 4.15): five sessions that
// ended in the last hours with a prompt, so each is a To resume card. In
// `settle-before`, one's subject is archived (the owner's item 7: it leaves
// To resume for Done) and another's archived then reopened (it comes
// back); `settle-after` then discards the ten-second hook test and marks
// one done from the board.
const settleEvents = [
  [201, "billing-api", 40 * MIN, "try the hook"],
  [202, "billing-api", 3 * HOUR, "Export invoices as CSV"],
  [203, "web-portal", 2 * HOUR, "Bump the chart library"],
  [204, "web-portal", 90 * MIN, "Fix the date picker"],
  [205, "docs-site", 4 * HOUR, "Draft the release notes"],
].flatMap(([n, project, ago, prompt]) => [
  prompted(n, project, now - ago, prompt),
  ended(n, project, now - ago + (n === 201 ? 10_000 : 20 * MIN), "prompt_input_exit"),
]).map((e) => ({ v: 1, ...e }));
const settleBeforeSteps = [
  "ingest",
  create("invoice export", { sessions: [sid(202)] }),
  create("release notes", { sessions: [sid(205)] }),
  { author: { kind: "archive", subject: "SUBJECT(invoice export)", routine: false } },
  { author: { kind: "archive", subject: "SUBJECT(release notes)", routine: false } },
  { author: { kind: "reopen", subject: "SUBJECT(release notes)" } },
];
const settleAfterSteps = [
  ...settleBeforeSteps,
  { author: { kind: "settle", session: sid(201), settlement: "discarded" } },
  { author: { kind: "settle", session: sid(203), settlement: "done" } },
];
const placedIn = (board, n) => {
  const found = Object.entries(board.columns).find(([, cards]) => cards.some((c) => c.session_id === sid(n)));
  if (!found) return null;
  const card = found[1].find((c) => c.session_id === sid(n));
  return `${found[0]}:${card.settled ?? "-"}`;
};
const settleBeforeChecks = (printed, board) => [
  [placedIn(board, 202) === "done:subject_archived", "an archived subject's session in Done"],
  [placedIn(board, 205) === "to_resume:-", "a reopened subject's session back in To resume"],
  [[201, 203, 204].every((n) => placedIn(board, n) === "to_resume:-"), "three sessions to resume"],
];
const settleAfterChecks = (printed, board) => [
  [placedIn(board, 201) === null, "the discarded session off the board"],
  [placedIn(board, 203) === "done:marked_done", "a session marked done in Done"],
  [placedIn(board, 204) === "to_resume:-", "the untouched session still to resume"],
  [!printed.triage.some((r) => r.session_id === sid(201) || r.session_id === sid(203)), "no triage row for a settled session"],
];

// `board: true` writes the scenario's board too, for the specs that open
// it; every scenario's board is still printed for its checks.
const registryScenarios = {
  subjects: { steps: subjectsSteps, checks: subjectsChecks, board: true },
  // AD-39: the home lies under a sync client's folder marker.
  "subjects-synced": { steps: subjectsSteps, synced: true, checks: subjectsChecks },
  // D-93: a daily copy, then the database damaged and restored from it.
  "subjects-restored": { steps: [...subjectsSteps, "copy", "damage"], checks: subjectsChecks },
  triage: { steps: triageSteps, events: triageEvents, agents: triageAgents, checks: triageChecks, board: true },
  evidence: { steps: evidenceSteps, events: evidenceEvents, agents: [], checks: evidenceChecks, board: true },
  model: { steps: modelSteps, events: modelEvents, agents: [], checks: modelOpenChecks, board: true },
  "settle-before": { steps: settleBeforeSteps, events: settleEvents, agents: [], checks: settleBeforeChecks, board: true },
  "settle-after": { steps: settleAfterSteps, events: settleEvents, agents: [], checks: settleAfterChecks, board: true },
  "model-paused": {
    steps: [
      ...modelSteps,
      { at: now - 30 * MIN },
      { proposals: [{ answer: "timeout" }, { answer: "invalid" }, { answer: "failed" }] },
    ],
    events: modelEvents,
    agents: [],
    checks: modelPausedChecks,
  },
};

function officinaJson(args, env) {
  const run = spawnSync(officina, args, { env, encoding: "utf8" });
  return { status: run.status, stdout: run.stdout, stderr: run.stderr };
}

for (const [name, scenario] of Object.entries(registryScenarios)) {
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(join(officinaHome, "events"), { recursive: true });
  if (scenario.synced) mkdirSync(join(tmp, ".SynologyWorkingDirectory"));
  const agents = scenario.agents ?? subjectsAgents;
  const byDay = new Map();
  for (const event of scenario.events ?? subjectsEvents) {
    const day = utcDay(event.ts);
    byDay.set(day, (byDay.get(day) ?? "") + JSON.stringify(event) + "\n");
  }
  for (const [day, content] of byDay) writeFileSync(join(officinaHome, "events", `${day}.jsonl`), content);
  const steps = join(tmp, "steps.json");
  writeFileSync(steps, JSON.stringify({ now_ms: now, steps: resolveSubjects(scenario.steps) }));
  execFileSync(registryFixture, [officinaHome, steps], { stdio: ["ignore", "ignore", "inherit"] });

  const fixture = join(tmp, "agents.json");
  writeFileSync(fixture, JSON.stringify(agents));
  const env = {
    SYSTEMROOT: process.env.SYSTEMROOT ?? "",
    USERPROFILE: home,
    HOME: home,
    OFFICINA_HOME: officinaHome,
    OFFICINA_CLAUDE_BIN: fakeClaude,
    FAKE_CLAUDE_FIXTURE: fixture,
    FAKE_CLAUDE_MODE: "ok",
  };
  const view = officinaJson(["subjects", "--json"], env);
  if (view.status !== 0) throw new Error(`${name}: officina subjects exited ${view.status}: ${view.stderr}`);
  const board = officinaJson(["board", "--json"], env);
  if (board.status !== 0) throw new Error(`${name}: officina board exited ${board.status}: ${board.stderr}`);
  const printed = JSON.parse(view.stdout);
  const checks = [
    [printed.registry.state === "ready", "the registry reads ready"],
    [!scenario.synced || printed.registry.exclude_from_sync !== null, "a folder to exclude from sync"],
    [!scenario.steps.includes("damage") || printed.restore_notice !== null, "the copy a restore used"],
    ...scenario.checks(printed, JSON.parse(board.stdout)),
  ];
  for (const [held, what] of checks) {
    if (!held) throw new Error(`${name}: expected ${what}: ${view.stdout}`);
  }
  writeFixture(name, "registry", view.stdout);
  if (scenario.board) writeFixture(`${name}-board`, "board", board.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${view.status}, board exit ${board.status}`);
}

// D-47: a registry that cannot be read prints the view with the reason,
// exit 3.
{
  const name = "subjects-unavailable";
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(join(officinaHome, "registry"), { recursive: true });
  writeFileSync(join(officinaHome, "registry", "registry.db"), "this is not a SQLite database\n".repeat(64));
  const fixture = join(tmp, "agents.json");
  writeFileSync(fixture, "[]");
  const env = {
    SYSTEMROOT: process.env.SYSTEMROOT ?? "",
    USERPROFILE: home,
    HOME: home,
    OFFICINA_HOME: officinaHome,
    OFFICINA_CLAUDE_BIN: fakeClaude,
    FAKE_CLAUDE_FIXTURE: fixture,
    FAKE_CLAUDE_MODE: "ok",
  };
  const view = officinaJson(["subjects", "--json"], env);
  if (view.status !== 3 || JSON.parse(view.stdout).registry.state !== "unavailable") {
    throw new Error(`${name}: expected exit 3 and an unavailable registry: ${view.status} ${view.stdout}`);
  }
  writeFixture(name, "registry", view.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${view.status}`);
}

// Story 3.8 (AD-56, D-71): the contract canary of the hook fields. The real
// `officina board --json` starts the running version's record, the real
// `officina hook` records a `Stop` whose payload lacks a field the board
// reads, the window's ingestion (through `registry_fixture`) folds it into
// the record, and the board printed after names the field.
{
  const name = "contract-hook-missing";
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(join(officinaHome, "events"), { recursive: true });
  const fixture = join(tmp, "agents.json");
  writeFileSync(fixture, JSON.stringify(busyAgents.slice(2, 4)));
  const env = {
    SYSTEMROOT: process.env.SYSTEMROOT ?? "",
    USERPROFILE: home,
    HOME: home,
    OFFICINA_HOME: officinaHome,
    OFFICINA_CLAUDE_BIN: fakeClaude,
    FAKE_CLAUDE_FIXTURE: fixture,
    FAKE_CLAUDE_MODE: "ok",
  };
  const first = officinaJson(["board", "--json"], env);
  if (first.status !== 0) throw new Error(`${name}: officina board exited ${first.status}: ${first.stderr}`);
  const version = JSON.parse(first.stdout).contract.claude_version;
  if (!version) throw new Error(`${name}: the first board knows no claude version`);
  // A Stop whose payload lacks the answer, with every other field there.
  const stop = { hook_event_name: "Stop", session_id: sid(3), cwd: work("web-portal") };
  const hook = spawnSync(officina, ["hook"], { env, input: JSON.stringify(stop), encoding: "utf8" });
  if (hook.status !== 0) throw new Error(`${name}: officina hook exited ${hook.status}`);
  const steps = join(tmp, "steps.json");
  writeFileSync(steps, JSON.stringify({ now_ms: Date.now(), steps: [{ hook_fields: version }] }));
  execFileSync(registryFixture, [officinaHome, steps], { stdio: ["ignore", "ignore", "inherit"] });
  const board = officinaJson(["board", "--json"], env);
  if (board.status !== 0) throw new Error(`${name}: officina board exited ${board.status}: ${board.stderr}`);
  if (JSON.parse(board.stdout).contract.hook_missing.length === 0) {
    throw new Error(`${name}: expected a missing hook field: ${board.stdout}`);
  }
  writeFixture(name, "board", board.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${board.status}`);
}

// Scene scenarios (story 4.2): defined in `scene-fixtures.mjs`, each
// checked against the intents of its helpers.
const sceneScenarioTable = sceneScenarios(now);
for (const [name, scenario] of Object.entries(sceneScenarioTable).filter(([, s]) => !s.homeOnly)) {
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const { env, sessions } = writeSceneHome({ dir: tmp, name, scenario, officina, fakeClaude, approveFixture });
  const run = officinaJson(["board", "--json"], env);
  if (run.status !== 0) throw new Error(`${name}: officina board exited ${run.status}: ${run.stderr}`);
  const board = JSON.parse(run.stdout);
  checkSceneIntents(name, board, sessions, scenario);
  writeFixture(name, "board", run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}

// Orchestrator views (story 5.8, D-72's pattern, D-181's kind
// `orchestrator`): a hand-written, redacted floor with templates approved
// through `approve_fixture`; subjects through `registry_fixture`; work
// orders written, bound and the briefing's reading recorded by the window's
// writer through `orchestrator_fixture`, at explicit times, which then
// prints the `OrchestratorView` the window would build from the real
// `officina board --json` of the same home. No hash, no SQL, no view built
// here.
const orchestratorFixture = join(targetDir, "debug", "examples", `orchestrator_fixture${exe}`);
execFileSync(
  "cargo",
  ["build", "--quiet", "--locked", "-p", "officina-host", "--example", "orchestrator_fixture", "--features", "window-authority"],
  { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
);

const orchestratorFloor = `schema = 2

[[project]]
name = "billing-api"
path = "C:/work/billing-api"

[[project]]
name = "docs-site"
path = "C:/work/docs-site"

[[template]]
name = "dependency-bump"
description = "Bump one dependency and fix what breaks"
projects = ["billing-api", "docs-site"]
prompt = "Bump {package} to its latest {level} release, run the tests, fix what breaks, commit."
model = "sonnet"

[template.params.package]
values = ["serde_json", "@scope/pkg"]

[template.params.level]
values = ["patch", "minor"]

[[template]]
name = "lint-sweep"
description = "Fix the lint warnings of one project"
projects = ["billing-api"]
prompt = "Fix the {level} lint warnings, run the tests, commit."

[template.params.level]
values = ["error", "warning"]
`;

// A template added since the approval: it awaits the author's.
const orchestratorPendingFloor = `${orchestratorFloor}
[[template]]
name = "release-notes"
description = "Draft the release notes of one version"
projects = ["docs-site"]
prompt = "Draft the release notes of {version} from the merged changes."

[template.params.version]
values = ["v0.5.0"]
`;

const bump = (card, seq, extra) => ({
  card,
  template: "dependency-bump",
  project: "billing-api",
  values: [["package", "serde_json"], ["level", "minor"]],
  seq,
  subject: { kind: "new", value: "dependency upgrades" },
  outcome: "started",
  exit_code: 0,
  ...extra,
});

const orchestratorScenarios = {
  // The approved catalogue, nothing launched, nothing read yet.
  "orchestrator-empty": { steps: [], orders: [], journal: [], agents: [], events: [], readAt: null, pending: false },
  // The same, with the orchestrator listed by `claude agents` in its own
  // folder (story 5.16, E24): the board's presence, never a card.
  "orchestrator-live": {
    steps: [],
    orders: [],
    journal: [],
    agents: [],
    orchestrator: { pid: 4510, startedAt: now - 20 * MIN, sessionId: sid(510), status: "idle" },
    events: [],
    readAt: null,
    pending: false,
  },
  "orchestrator-day": {
    events: [
      { ts: now - 3 * HOUR, event: "UserPromptSubmit", session_id: sid(504), cwd: work("docs-site"), prompt: hostile },
      { ts: now - 170 * MIN, event: "SessionEnd", session_id: sid(504), cwd: work("docs-site"), reason: "prompt_input_exit" },
      { ts: now - 40 * MIN, event: "UserPromptSubmit", session_id: sid(503), cwd: work("docs-site"), prompt: "Draft the install guide" },
      { ts: now - 20 * MIN, event: "Stop", session_id: sid(503), cwd: work("docs-site"), last_message: "Drafted. Which tone do you want?" },
      { ts: now - 6 * MIN, event: "UserPromptSubmit", session_id: sid(501), cwd: work("billing-api"), prompt: "Run the migration dry-run" },
      { ts: now - 4 * MIN, event: "Notification", session_id: sid(501), cwd: work("billing-api"), notification_type: "permission_prompt", message: "Claude needs your permission to use Bash" },
    ].map((e) => ({ v: 1, ...e })),
    steps: [
      "ingest",
      { at: now - 3 * HOUR },
      { author: { kind: "create", name: "install guide", origin: { sessions: [sid(503)] } } },
      { author: { kind: "create", name: "old spike", origin: { project: "billing-api" } } },
      { at: now - 30 * MIN },
      { author: { kind: "archive", subject: 2, routine: false } },
    ],
    orders: [
      bump("card-bound", 1, { launched_at_ms: now - 50 * MIN, printed: "backgrounded · bg0101 · dependency-bump-1", bind: { bg_id: "bg0101", session_id: sid(502) } }),
      bump("card-unbound", 2, { launched_at_ms: now - 25 * MIN, printed: "backgrounded · bg0102 · dependency-bump-2" }),
      bump("card-differs", 3, { launched_at_ms: now - 15 * MIN, printed: "backgrounded · bg9999 · dependency-bump-3", bind: { bg_id: "bg0103", session_id: sid(505) } }),
      {
        card: "card-failed",
        template: "lint-sweep",
        project: "billing-api",
        values: [["level", "warning"]],
        seq: 1,
        subject: { kind: "new", value: "lint" },
        launched_at_ms: now - 10 * MIN,
        outcome: "failed",
        exit_code: 1,
        error: `error: ${hostile}`,
      },
    ],
    // Runs through the relay that failed or were refused, as `officina run`
    // journals them.
    journal: [
      { ts: now - 35 * MIN, kind: "refused", project: "billing-api", session_id: sid(501), action: hostile, reason: "action_not_allowed", detail: "not in the catalogue" },
    ],
    agents: [
      { pid: 4501, cwd: work("billing-api"), kind: "interactive", startedAt: now - HOUR, sessionId: sid(501), name: "billing-api", status: "waiting", waitingFor: "permission prompt" },
      { pid: 4503, cwd: work("docs-site"), kind: "interactive", startedAt: now - HOUR, sessionId: sid(503), name: hostile, status: "idle" },
      { pid: 4502, id: "bg0101", cwd: work("billing-api"), kind: "background", startedAt: now - 50 * MIN + 1_000, sessionId: sid(502), name: "dependency-bump-1", status: "busy", state: "working" },
    ],
    readAt: now - HOUR,
    pending: true,
    // The board the cards take their quota windows from.
    quota: quotaPayload(Math.floor((now + 2 * HOUR) / 1000), Math.floor((now + 3 * 24 * HOUR) / 1000)),
    // Work order cards issued from this home by core (story 5.9, kind
    // `card`): a first card, the card a value edit issues in its place,
    // and the card a new subject's name issues in place of that one.
    cards: [
      {
        name: "card-bump",
        request: { template: "dependency-bump", project: "billing-api", values: [["package", "serde_json"], ["level", "patch"]], subject: null },
      },
      { name: "card-bump-minor", edit: { of: "card-bump", change: { kind: "value", name: "level", value: "minor" } } },
      {
        name: "card-bump-new-subject",
        edit: { of: "card-bump-minor", change: { kind: "subject", subject: { kind: "new", value: "<b>serde</b> upgrades" } } },
      },
    ],
  },
};

for (const [name, scenario] of Object.entries(orchestratorScenarios)) {
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(join(officinaHome, "events"), { recursive: true });
  const byDay = new Map();
  for (const event of scenario.events) {
    const day = utcDay(event.ts);
    byDay.set(day, (byDay.get(day) ?? "") + JSON.stringify(event) + "\n");
  }
  for (const [day, content] of byDay) writeFileSync(join(officinaHome, "events", `${day}.jsonl`), content);
  writeFileSync(join(officinaHome, "floor.toml"), orchestratorFloor);
  approveFixture(officinaHome, "approve");
  if (scenario.steps.length > 0) {
    const steps = join(tmp, "steps.json");
    writeFileSync(steps, JSON.stringify({ now_ms: now, steps: scenario.steps }));
    execFileSync(registryFixture, [officinaHome, steps], { stdio: ["ignore", "ignore", "inherit"] });
  }
  const script = join(tmp, "orders.json");
  writeFileSync(script, JSON.stringify({ now_ms: now, orders: scenario.orders }));
  execFileSync(orchestratorFixture, [officinaHome, script], { stdio: ["ignore", "ignore", "inherit"] });
  if (scenario.journal.length > 0) {
    mkdirSync(join(officinaHome, "journal"), { recursive: true });
    for (const line of scenario.journal) {
      appendFileSync(join(officinaHome, "journal", `${utcDay(line.ts)}.jsonl`), JSON.stringify({ v: 1, ...line }) + "\n");
    }
  }
  if (scenario.pending) writeFileSync(join(officinaHome, "floor.toml"), orchestratorPendingFloor);
  const fixture = join(tmp, "agents.json");
  const orchestratorEntry = scenario.orchestrator && {
    ...scenario.orchestrator,
    cwd: join(officinaHome, "orchestrator", "workdir"),
    kind: "interactive",
    name: "officina-orchestrator",
  };
  writeFileSync(fixture, JSON.stringify([...scenario.agents, ...(orchestratorEntry ? [orchestratorEntry] : [])]));
  const env = {
    SYSTEMROOT: process.env.SYSTEMROOT ?? "",
    USERPROFILE: home,
    HOME: home,
    OFFICINA_HOME: officinaHome,
    OFFICINA_CLAUDE_BIN: fakeClaude,
    FAKE_CLAUDE_FIXTURE: fixture,
    FAKE_CLAUDE_MODE: "ok",
  };
  if (scenario.quota) execFileSync(officina, ["quota", "write"], { env, input: scenario.quota });
  const board = officinaJson(["board", "--json"], env);
  if (board.status !== 0) throw new Error(`${name}: officina board exited ${board.status}: ${board.stderr}`);
  const boardFile = join(tmp, "board.json");
  writeFileSync(boardFile, board.stdout);
  writeFileSync(script, JSON.stringify({ now_ms: now, board_file: boardFile, read_at_ms: scenario.readAt }));
  const view = spawnSync(orchestratorFixture, [officinaHome, script], { encoding: "utf8" });
  if (view.status !== 0) throw new Error(`${name}: orchestrator_fixture exited ${view.status}: ${view.stderr}`);
  const printed = JSON.parse(view.stdout);
  const { briefing } = printed;
  const day = name === "orchestrator-day";
  const checks = [
    [printed.templates.length === 2, "the two approved templates"],
    [day === (printed.awaiting_templates.length > 0), "a template awaiting approval on the day only"],
    [day === (briefing.cursor.kind === "read_at"), "the day's last reading"],
    ...["waiting", "subjects", "triage", "runs", "work_orders", "changed"].map((section) => [
      day === (briefing[section].count > 0),
      `${day ? "items" : "nothing"} in ${section}`,
    ]),
    [!day || printed.work_orders.some((o) => o.unbound), "an unbound order"],
    [!day || printed.work_orders.some((o) => o.printed_id_differs), "an order whose printed id differs"],
    [!day || printed.work_orders.some((o) => o.session !== null), "an order bound to its session"],
    [!day || printed.work_orders.some((o) => o.outcome === "failed"), "a failed order"],
    [Boolean(scenario.orchestrator) === (printed.presence !== null), "the orchestrator's presence where it is listed"],
    [!scenario.orchestrator || printed.presence.status === scenario.orchestrator.status, "its status"],
  ];
  for (const [held, what] of checks) {
    if (!held) throw new Error(`${name}: expected ${what}: ${view.stdout}`);
  }
  writeFixture(name, "orchestrator", view.stdout);
  if (scenario.cards) {
    // A pending change refuses every card (catalogue_changed): they are
    // issued from the floor the catalogue was approved for.
    writeFileSync(join(officinaHome, "floor.toml"), orchestratorFloor);
    writeFileSync(script, JSON.stringify({ now_ms: now, board_file: boardFile, cards: scenario.cards }));
    const issued = spawnSync(orchestratorFixture, [officinaHome, script], { encoding: "utf8" });
    if (issued.status !== 0) throw new Error(`${name}: orchestrator_fixture exited ${issued.status} on its cards: ${issued.stderr}`);
    const cards = JSON.parse(issued.stdout);
    const first = cards["card-bump"];
    const edited = cards["card-bump-minor"];
    const renamed = cards["card-bump-new-subject"];
    const cardChecks = [
      [Object.keys(cards).length === scenario.cards.length, "one card per step"],
      [first.argv.length > 1 && first.prompt.length > 0, "a vector and a prompt"],
      [first.quota?.five_hour != null && first.quota?.seven_day != null, "the board's quota windows"],
      [first.subject.kind === "existing" && first.subject_options.length > 0, "the project's open subject as the default"],
      [edited.seq === first.seq && edited.id !== first.id && edited.prompt !== first.prompt, "a value edit under the same seq"],
      [renamed.subject.kind === "new" && renamed.prompt === edited.prompt, "a subject edit that keeps the prompt"],
    ];
    for (const [held, what] of cardChecks) {
      if (!held) throw new Error(`${name}: expected ${what}: ${issued.stdout}`);
    }
    for (const [cardName, card] of Object.entries(cards)) writeFixture(cardName, "card", JSON.stringify(card, null, 2));
  }
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${view.status}`);
}

// The `auto` class in the Relay view (story 5.14, AD-88, AD-92): a floor
// with an `auto` action approved through `approve_fixture`; shadow reviews
// met by the author's decisions, two runs a review and the rule approved,
// auto approval turned on under its gate and the decisions of the last days
// written by the window's writer through `orchestrator_fixture` at explicit
// times; then printed by the real `officina relay --json`, which carries no
// review's reason (D-143).
{
  const name = "relay-auto";
  const tmp = mkdtempSync(join(tmpdir(), `officina-fixture-${name}-`));
  const home = join(tmp, "home");
  const officinaHome = join(tmp, "officina");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(officinaHome, { recursive: true });
  writeFileSync(
    join(officinaHome, "floor.toml"),
    `schema = 2

[[project]]
name = "infra-scripts"
path = "${work("infra-scripts")}"
actions = ["check", "apply"]

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"
program = "C:/path/to/ansible-playbook.exe"
args = ["--check", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply"
description = "Apply the playbook checked by its dry run"
class = "auto"
check = "check"
auto_max_per_hour = 4
program = "C:/path/to/ansible-playbook.exe"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[auto_review]
model = "haiku"
`,
  );
  approveFixture(officinaHome, "approve");
  const id = (n) => `0123456789abcdef-${n}`;
  const review = (n, at, extra) => ({
    approval_id: id(n),
    project: "infra-scripts",
    action: "apply",
    requested_at_ms: at,
    mode: "shadow",
    outcome: "answered",
    cause: "auto_approval_off",
    decision: "approve",
    reason: "The dry run changed one host and the last run succeeded.",
    rule_baseline: true,
    author: "approved",
    ...extra,
  });
  // 21 shadow cases over the last days, one where review and rule differ.
  const shadow = Array.from({ length: 21 }, (_, n) =>
    review(n, now - (n + 1) * 7 * HOUR, n === 4 ? { rule_baseline: false } : {}),
  );
  const auto = [
    review(30, now - 3 * HOUR, { mode: "live", cause: null, author: null, run_id: "19a2b3c4d5e-30" }),
    review(31, now - 2 * DAY, { mode: "rule", outcome: "no_call", cause: null, author: null, reason: null, run_id: "19a2b3c4d5e-31" }),
  ];
  // The author's decisions, one a day or two, and the review's and the
  // rule's approvals; the shadow cases of the last two days show their
  // review in the recent activity.
  const decisions = [
    ...shadow.map((r) => ({ at_ms: r.requested_at_ms + MIN, approval_id: r.approval_id, approve: true, by: "author" })),
    { at_ms: now - 3 * HOUR + MIN, approval_id: id(30), approve: true, by: "review" },
    { at_ms: now - 2 * DAY + MIN, approval_id: id(31), approve: true, by: "rule" },
  ];
  const script = join(tmp, "auto.json");
  writeFileSync(script, JSON.stringify({ now_ms: now, reviews: [...shadow, ...auto], auto_approval: true, decisions }));
  execFileSync(orchestratorFixture, [officinaHome, script], { stdio: ["ignore", "ignore", "inherit"] });
  const env = { SYSTEMROOT: process.env.SYSTEMROOT ?? "", USERPROFILE: home, HOME: home, OFFICINA_HOME: officinaHome };
  const run = spawnSync(officina, ["relay", "--json"], { env, encoding: "utf8" });
  if (run.status !== 0) throw new Error(`${name}: officina relay exited ${run.status}: ${run.stderr}`);
  const printed = JSON.parse(run.stdout);
  const checks = [
    [printed.auto.approval_on, "auto approval on"],
    [printed.auto.stats.gate && printed.auto.stats.rule_offer, "the gate and the rule offer"],
    [printed.auto.stats.rule_diff === 1, "one case where review and rule differ"],
    [printed.auto.runs.length === 2, "two auto-approved runs"],
    [printed.recent.some((e) => e.review !== null), "a decision showing its review"],
    [!run.stdout.includes("The dry run changed one host"), "no review's reason"],
  ];
  for (const [held, what] of checks) {
    if (!held) throw new Error(`${name}: expected ${what}: ${run.stdout}`);
  }
  writeFixture(name, "relay", run.stdout);
  rmSync(tmp, { recursive: true, force: true });
  console.log(`fixture ${name}: exit ${run.status}`);
}

// D-181: every fixture file and its kind, for the loader the specs and the
// Vitest cases share (`e2e/fixture-index.ts`).
writeFileSync(join(outDir, "index.json"), JSON.stringify({ files: fixtureKinds }, null, 2) + "\n");
console.log(`fixture index: ${Object.keys(fixtureKinds).length} files`);
