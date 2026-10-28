// The scene scenarios (stories 4.2 and 4.14; D-101 as amended, D-120,
// D-181, D-519): a floor file and a set of sessions, each written by a
// helper that carries the column and the zone it is meant to produce.
// `e2e-fixtures.mjs` turns each into a fixture board and checks the board
// against the intents; `e2e-fixtures.mjs --home <dir> <scenario>` writes one
// as a home the real window starts on. Hand-written and redacted:
// placeholder names and paths only.
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const MIN = 60_000;
export const HOUR = 60 * MIN;
export const sid = (n) => `00000000-0000-0000-0000-${String(n).padStart(12, "0")}`;
export const work = (name) => `C:/work/${name}`;

function utcDay(ms) {
  return new Date(ms).toISOString().slice(0, 10);
}

// Scene scenarios (story 4.2, D-101 as amended, D-181, D-519): boards the
// real `officina board --json` prints for a floor file and a set of
// sessions, each session written by a helper that carries the column and
// the zone it is meant to produce. The intents
// are the self-check's oracle: the script compares the printed board with
// them and fails on a difference, never through a JavaScript classifier
// (AD-13). Every wait is at least 3 minutes old at generation, under the
// default threshold, except where a scenario tests the alert.

const SCENE_ACTION = `
[[action]]
name = "restart"
description = "Restart one service"
class = "human"
program = "C:/path/to/service-tool.exe"
args = ["restart", "{service}"]

[action.params.service]
values = ["web01"]
`;

/** The floor file of a scene scenario, schema 2 so a project may declare
 *  the action a pending request names; every project is free. */
function sceneFloorToml(floor, projectPaths) {
  let text = "schema = 2\n";
  if (floor.alertAfterMinutes !== undefined) text += `\n[alert]\nafter_minutes = ${floor.alertAfterMinutes}\n`;
  for (const p of floor.projects) {
    text += `\n[[project]]\nname = "${p.name}"\npath = "${projectPaths.get(p.name) ?? p.path}"\nautonomy = "free"\n`;
    if (p.color) text += `color = "${p.color}"\n`;
    if (p.priority) text += `priority = "${p.priority}"\n`;
    if (p.approvals) text += `actions = ["restart"]\n`;
  }
  return floor.projects.some((p) => p.approvals) ? text + SCENE_ACTION : text;
}

/** The scene scenarios, their times relative to `now`. */
export function sceneScenarios(now) {
  /** A project of a scene floor: placeholder names, a `C:/work` path unless
   *  the scenario needs a real folder (a relay request runs in it). */
  const sceneProject = (name, extra = {}) => ({ name, path: work(name), ...extra });

  let scenePid = 6000;
  /** One live session: its agents record, its event lines and its intent, the
   *  column and the zone core places it in (story 4.3). */
  function sceneSession(n, project, column, zone, name, events, extra = {}) {
    const status = { needs_you: "waiting", working: "busy", your_turn: "idle" }[column];
    return {
      n,
      project,
      intent: { column, zone },
      agent: {
        pid: scenePid++,
        cwd: work(project),
        kind: "interactive",
        startedAt: now - 3 * HOUR - n * MIN,
        sessionId: sid(n),
        name,
        status,
        ...(column === "needs_you" ? { waitingFor: "permission prompt" } : {}),
      },
      events: events.map((e) => ({ v: 1, session_id: sid(n), cwd: work(project), ...e })),
      ...extra,
    };
  }
  const prompt = (minsAgo, text = "Carry on with the plan") => ({ ts: now - minsAgo * MIN, event: "UserPromptSubmit", prompt: text });
  const stopAt = (minsAgo, text = "Done. Ready for review.") => ({ ts: now - minsAgo * MIN, event: "Stop", last_message: text });
  const startAt = (minsAgo, source) => ({ ts: now - minsAgo * MIN, event: "SessionStart", source });

  /** Needs you since `waitMin` minutes: a prompt, then a permission prompt;
   *  in the alert room when `alerted`. */
  const waitingOn = (n, project, name, waitMin, extra = {}) =>
    sceneSession(n, project, "needs_you", extra.alerted ? "alert" : "decision", name, [
      prompt(waitMin + 2),
      { ts: now - waitMin * MIN, event: "Notification", notification_type: "permission_prompt", message: "Claude needs your permission to use Bash" },
    ], extra);
  /** Working since a prompt `sinceMin` minutes ago; queued at the decision
   *  office while its relay request is pending. */
  const workingOn = (n, project, name, sinceMin = 6, extra = {}) =>
    sceneSession(n, project, "working", extra.request ? (extra.alerted ? "alert" : "decision") : "office", name, [prompt(sinceMin)], extra);
  /** Your turn after a finished round. */
  const resting = (n, project, name, stoppedMin = 12) =>
    sceneSession(n, project, "your_turn", "lounge", name, [prompt(stoppedMin + 5), stopAt(stoppedMin)]);
  /** Your turn, just started from the command line, no prompt yet. */
  const arriving = (n, project, name) => sceneSession(n, project, "your_turn", "reception", name, [startAt(4, "startup")]);
  /** Your turn, compacted after its round, no prompt since. */
  const compacted = (n, project, name) =>
    sceneSession(n, project, "your_turn", "lounge", name, [prompt(30), stopAt(25), startAt(20, "compact")]);

  const P = {
    billing: "billing-api",
    web: "web-portal",
    docs: "docs-site",
    infra: "infra-scripts",
    mobile: "mobile-app",
    data: "data-pipeline",
    auth: "auth-service",
    search: "search-index",
    ml: "ml-models",
    status: "status-page",
  };
  const fourProjects = [
    sceneProject(P.billing, { color: "green" }),
    sceneProject(P.web, { color: "blue" }),
    sceneProject(P.docs, { color: "terracotta" }),
    sceneProject(P.infra, { color: "plum" }),
  ];
  const tenProjects = Object.values(P).map((name) => sceneProject(name));

  const scenarios = {
    "scene-a": {
      floor: { projects: [sceneProject(P.billing, { color: "green" })] },
      sessions: [workingOn(1, P.billing, "invoice-export"), waitingOn(2, P.billing, "billing-api", 5)],
    },
    // A session just started at reception, and one compacted in the lounge.
    "scene-b": {
      floor: { projects: fourProjects },
      sessions: [
        workingOn(1, P.billing, "invoice-export"),
        workingOn(2, P.billing, "payment-retry", 9),
        workingOn(3, P.web, "upgrade-deps"),
        workingOn(4, P.docs, "install-guide", 15),
        workingOn(5, P.infra, "rotate-certs"),
        waitingOn(6, P.billing, "billing-api", 8),
        waitingOn(7, P.web, "web-portal", 4),
        resting(8, P.docs, "docs-site"),
        compacted(9, P.web, "session-middleware"),
        arriving(10, P.infra, "audit-playbooks"),
      ],
    },
    // Five waiting, and a Working session whose relay request is pending.
    "scene-c": {
      floor: { projects: fourProjects.map((p) => (p.name === P.infra ? { ...p, approvals: true } : p)) },
      sessions: [
        waitingOn(1, P.billing, "billing-api", 9),
        waitingOn(2, P.web, "web-portal", 7),
        waitingOn(3, P.docs, "docs-site", 6),
        waitingOn(4, P.infra, "infra-scripts", 5),
        waitingOn(5, P.billing, "payment-retry", 3),
        workingOn(6, P.billing, "invoice-export"),
        workingOn(7, P.web, "upgrade-deps"),
        workingOn(8, P.docs, "install-guide"),
        workingOn(9, P.infra, "rotate-certs", 6, { request: true }),
        resting(10, P.web, "session-middleware"),
        resting(11, P.docs, "link-fixes"),
        arriving(12, P.billing, "audit-ledger"),
      ],
    },
    // The alert's three cases: a high-priority project's wait, a wait past
    // the floor's threshold, and a relay request of a Working session on a
    // normal project, which never escalates by time.
    "scene-alert": {
      floor: {
        alertAfterMinutes: 5,
        projects: [
          sceneProject(P.billing, { color: "green", priority: "high" }),
          sceneProject(P.web, { color: "blue" }),
          sceneProject(P.docs, { color: "terracotta", approvals: true }),
        ],
      },
      sessions: [
        waitingOn(1, P.billing, "billing-api", 3, { alerted: true }),
        waitingOn(2, P.web, "web-portal", 8, { alerted: true }),
        workingOn(3, P.docs, "rebuild-index", 9, { request: true }),
        workingOn(4, P.web, "upgrade-deps"),
        resting(5, P.docs, "docs-site"),
      ],
    },
    // The same, its timed wait one minute short of the threshold.
    "scene-alert-early": {
      floor: {
        alertAfterMinutes: 5,
        projects: [
          sceneProject(P.billing, { color: "green", priority: "high" }),
          sceneProject(P.web, { color: "blue" }),
          sceneProject(P.docs, { color: "terracotta", approvals: true }),
        ],
      },
      sessions: [
        waitingOn(1, P.billing, "billing-api", 3, { alerted: true }),
        waitingOn(2, P.web, "web-portal", 4),
        workingOn(3, P.docs, "rebuild-index", 9, { request: true }),
        workingOn(4, P.web, "upgrade-deps"),
        resting(5, P.docs, "docs-site"),
      ],
    },
    // Eight projects, the owner's design target (D-519): every office used.
    "scene-eight": {
      floor: { projects: tenProjects.slice(0, 8) },
      sessions: [
        ...tenProjects.slice(0, 8).flatMap((p, i) => [
          workingOn(10 + 2 * i, p.name, `${p.name}-task`),
          ...(i % 2 === 0 ? [workingOn(11 + 2 * i, p.name, `${p.name}-review`, 11)] : []),
        ]),
        waitingOn(40, P.billing, "billing-api", 8),
        waitingOn(41, P.data, "data-pipeline", 7),
        waitingOn(42, P.search, "search-index", 6),
        waitingOn(43, P.auth, "auth-service", 5),
        waitingOn(44, P.mobile, "mobile-app", 4),
        resting(45, P.web, "web-portal"),
        resting(46, P.ml, "ml-models"),
        arriving(47, P.docs, "docs-site"),
      ],
    },
    // More waiting than the queue holds, an office with more sessions at
    // work than its desks and its chip spot hold (its chip lists the rest),
    // and live sessions in the eighth to tenth projects.
    "scene-overflow": {
      floor: { projects: tenProjects },
      sessions: [
        ...[...Array(11).keys()].map((i) => waitingOn(1 + i, tenProjects[i % 7].name, `waiting-${i + 1}`, 9 - (i % 6))),
        workingOn(20, P.billing, "invoice-export"),
        workingOn(21, P.billing, "payment-retry"),
        workingOn(22, P.billing, "ledger-sync"),
        workingOn(23, P.billing, "refund-flow"),
        workingOn(28, P.billing, "audit-ledger"),
        workingOn(29, P.billing, "tax-report"),
        workingOn(24, P.ml, "train-baseline"),
        workingOn(25, P.status, "uptime-probe"),
        resting(26, P.search, "search-index"),
        resting(27, P.web, "web-portal"),
      ],
    },
  };

    // AD-71, D-120: scene-c without two Working sessions, ten employees and
    // five waiting, for the CPU measurement; written by --home only.
    scenarios["scene-measure"] = {
      homeOnly: true,
      floor: scenarios["scene-c"].floor,
      sessions: scenarios["scene-c"].sessions.filter((s) => !["upgrade-deps", "install-guide"].includes(s.agent.name)),
    };
    return scenarios;
}

/** Writes scenario `name`'s Officina home, user profile, floor, event files
 *  and agents fixture under `dir`, making its relay requests through the
 *  real `officina run`; returns the environment the CLI or the window runs
 *  with, and the sessions as written. */
export function writeSceneHome({ dir, name, scenario, officina, fakeClaude, approveFixture }) {
  const home = join(dir, "home");
  const officinaHome = join(dir, "officina");
  mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
  mkdirSync(join(officinaHome, "events"), { recursive: true });
  // A project whose session makes a relay request lives in a real folder:
  // `officina run` starts there. Its sessions' `cwd` follow it.
  const projectPaths = new Map();
  for (const p of scenario.floor.projects.filter((p) => p.approvals)) {
    const projectDir = join(dir, "work", p.name);
    mkdirSync(projectDir, { recursive: true });
    projectPaths.set(p.name, projectDir.replaceAll("\\", "/"));
  }
  const sessions = scenario.sessions.map((s) => {
    const cwd = projectPaths.get(s.project);
    if (!cwd) return s;
    return { ...s, agent: { ...s.agent, cwd }, events: s.events.map((e) => ({ ...e, cwd })) };
  });
  const floorFile = join(officinaHome, "floor.toml");
  writeFileSync(floorFile, sceneFloorToml(scenario.floor, projectPaths));
  const base = {
    SYSTEMROOT: process.env.SYSTEMROOT ?? "",
    USERPROFILE: home,
    HOME: home,
    OFFICINA_HOME: officinaHome,
    OFFICINA_FLOOR: floorFile,
  };
  const requesters = sessions.filter((s) => s.request);
  if (requesters.length > 0) {
    approveFixture(officinaHome, "approve");
    for (const s of requesters) {
      const run = spawnSync(officina, ["run", "restart", "service=web01", "--wait", "1"], {
        env: { ...base, CLAUDE_CODE_SESSION_ID: sid(s.n) },
        cwd: projectPaths.get(s.project),
        encoding: "utf8",
      });
      if (run.status !== 126) throw new Error(`${name}: the request of ${s.agent.name} did not wait: ${run.status} ${run.stderr}`);
    }
  }
  const byDay = new Map();
  for (const event of sessions.flatMap((s) => s.events)) {
    const day = utcDay(event.ts);
    byDay.set(day, (byDay.get(day) ?? "") + JSON.stringify(event) + "\n");
  }
  for (const [day, content] of byDay) writeFileSync(join(officinaHome, "events", `${day}.jsonl`), content);
  const fixture = join(dir, "agents.json");
  writeFileSync(fixture, JSON.stringify(sessions.map((s) => s.agent)));
  const env = { ...base, OFFICINA_CLAUDE_BIN: fakeClaude, FAKE_CLAUDE_FIXTURE: fixture, FAKE_CLAUDE_MODE: "ok" };
  return { env, sessions, officinaHome, fixture };
}

/** Fails when the printed board differs from what the helpers meant: each
 *  session's column, no other live card, the floor's projects in file
 *  order, and one pending request per requesting session. */
export function checkSceneIntents(name, board, sessions, scenario) {
  const live = [...board.columns.needs_you, ...board.columns.your_turn, ...board.columns.working];
  const problems = [];
  for (const s of sessions) {
    const card = live.find((c) => c.session_id === sid(s.n));
    if (!card) {
      problems.push(`${s.agent.name}: no live card`);
      continue;
    }
    const placed = { column: card.column, zone: card.scene?.zone };
    for (const [key, expected] of Object.entries(s.intent)) {
      if (placed[key] !== expected) problems.push(`${s.agent.name}: ${key} ${placed[key]}, meant ${expected}`);
    }
  }
  if (live.length !== sessions.length) problems.push(`${live.length} live cards for ${sessions.length} sessions`);
  const floorNames = board.floor_projects.map((p) => p.name).join(",");
  const meant = scenario.floor.projects.map((p) => p.name).join(",");
  if (floorNames !== meant) problems.push(`floor projects ${floorNames}, meant ${meant}`);
  const requested = sessions.filter((s) => s.request).map((s) => sid(s.n)).sort();
  const pending = board.approvals.filter((a) => a.valid).map((a) => a.session_id).sort();
  if (requested.join(",") !== pending.join(",")) problems.push(`pending requests of ${pending}, meant ${requested}`);
  if (problems.length > 0) throw new Error(`${name}: the board differs from the scenario's intents:\n  ${problems.join("\n  ")}`);
}

