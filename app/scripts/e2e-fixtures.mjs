// Builds the boards the e2e suite and the README assets render, by running the
// real `officina board --json` against the fake claude (AD-17). Scenarios are
// hand-written and redacted: no real session data ever enters a fixture.
//
// Usage: node scripts/e2e-fixtures.mjs   (after `cargo build -p officina`)
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
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
