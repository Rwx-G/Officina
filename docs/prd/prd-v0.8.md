---
title: "PRD: Officina v0.8 - The Workstation and Production"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions), with the amendments of ../architecture-v0.8.md applied; Slice A conditional on story 2.8's two-week reading; gates 0-3 open
inputs:
  - (framing, not published)
  - (requirement challenge, not published)
  - prd/prd-v0.6.md (cited as "v0.6")
  - (v0.6 requirement challenge, not published)
  - architecture-v0.6.md (cited as "v0.6 addendum")
  - prd/prd-v0.7.md (cited as "v0.7")
  - (v0.7 requirement challenge, not published)
  - prd/prd-v0.1.md
  - prd/prd-v0.2.md
  - prd/prd-v0.3.md
  - prd/prd-v0.4.md
  - prd/prd-v0.5.md
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - architecture-v0.4.md
  - architecture-v0.5.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - stories/epic-3-decisions.md
  - stories/epic-4-decisions.md
  - stories/epic-5-decisions.md
  - stories/story-2.8-approvals-and-class-human.md
  - https://code.claude.com/docs/en/chrome
  - https://code.claude.com/docs/en/computer-use
  - https://code.claude.com/docs/en/desktop
  - https://code.claude.com/docs/en/cli-reference
  - https://code.claude.com/docs/en/agent-view
  - https://code.claude.com/docs/en/env-vars
  - https://code.claude.com/docs/en/hooks
  - https://code.claude.com/docs/en/whats-new/2026-w28
  - https://code.claude.com/docs/en/whats-new/2026-w36
---

# PRD: Officina v0.8 - The Workstation and Production

## 0. Document Purpose

Scopes v0.8, Epic 8. v0.1 to v0.7 hold. The framing's roadmap row: browser
and commands for workers, intervention orders with a dry run. Its exit
criteria are the targets: zero write outside autonomy without an approved
order; a browser task carried out without mouse or keyboard. Behind both
stands the framing's fifth need: act on the rest of the workstation (browser,
WSL, administered servers) in the background, without taking the author's
mouse or keyboard (framing section 1).

v0.6 and v0.7 hand v0.8 what they deferred: relay actions, server identities
and the framing's `iterative` profile in the workshop; commands and the
workstation's browser for workers; intervention orders with a dry run (v0.6
section 5, v0.7 section 5). Each is taken here, built, folded into story 2.8,
or deferred with its reason (section 5).

Revised after a requirement challenge: the intervention order's core moved
into story 2.8 (D-170), so v0.8 starts from it; per-project scope waits on two
weeks of that story at work and on the owner's count of production writes;
workers get read actions only, with their dry runs judged by target; the
binding claim is bounded to what the fingerprint covers; and the browser
slice is deferred, the exit criterion's browser half being read on v0.6
FR-16.

**What v0.8 starts from.** Ansible and SSH run from WSL are about 95 % of the
roughly 450 commands of the measured mailbox session; HTTP reads are 2 to 5 %
(owner, 2026-10-27). v0.2 delegates them per repository through catalogue
actions with closed parameters. Story 2.8, amended by D-170, builds the
framing's intervention order inside the approval flow: a `human` action may
name a `check`; its request needs a matching successful check under the
after-check rule; the check's output is kept and shown on the approval card;
the request records the check's run id and the tree fingerprint; and the
fingerprint is checked again before the approval is consumed. Scope is
declared by splitting actions on their parameter rules (`apply-lab`,
`after-check`, its `group` limited to lab groups; `apply-prod`, `human` with
its check), which the README shows. Section 4.1 states what that gives and
what it does not.

**What v0.8 adds.**
- **Slice A, conditional:** a fail-closed `effect` default and per-project
  scope in guarded projects (FR-1 to FR-3), built only if story 2.8's first
  two weeks at work show a write that reached production through a
  non-`human` action, or the author asks for per-project scope; and the
  binding of ignored roles and collections (FR-4), built only if gate 1 finds
  any.
- **Slice B:** read actions for workshop workers, run on the host from the
  author's tree (FR-5 to FR-8).
- **The browser half of the exit criterion** is read on v0.6 FR-16's headless
  browser on a project's own server; dashboard reads go to AD-36's HTTP
  actions. The workstation browser slice is deferred with the owner's
  confirmation (section 5's backlog note).

**First step.** Before any v0.8 story, the owner counts the production writes
of the measured session (gate 0). How many there are, and over how many
playbooks and groups, decides whether action pairs suffice for scope.

**Null option.** With story 2.8 as amended and the README's action split,
production writes are behind a click with their dry run on the card, bound to
the tracked tree. v0.8's Slice A buys only the fail-closed default (a write
the author forgot to class `human` cannot reach production) and scope
declared per project instead of per action. Section 7's first table decides
whether that is worth building.

**Dependencies.** v0.8 consumes no governor code. Slice A needs story 2.8.
Slice B needs v0.6's workshop and FR-1 and FR-2's declarations, which it
carries if Slice A is not built. Workers get no write at all, so v0.7's
premise that "a worker holds no relay and no key either way" (v0.7 section 1)
narrows to "no key and read actions only", and no rule for governor-launched
runs is needed. Roadmap: section 8, question 6.

Facts about Claude Code rest on `docs/research-notes.md` (E-numbers), on
earlier PRDs' documented surfaces (cited "v0.6 S2"), or on the pages listed at
the head of section 4 (S-numbers). What none settles is in section 8 with the
check that settles it; the gating ones run first (FR-0). This PRD is written
before v0.7's architecture addendum and while v0.6's is a draft: an FR naming
a v0.6 or v0.7 FR follows that FR as its addendum amends it.

## 1. Vision

The author wants to hand a subject to an employee from start to finish, keep
production writes in his own hands, and see what he approves.

**Production.** A production write waits for the author's click with its
dry run's output on the card, and runs in the session's waiting call bound to
the tracked tree the dry run ran on (story 2.8, D-170). The author declares
where sessions may write alone by how he writes his catalogue; if that proves
too loose in use, v0.8 adds a scope judged on the target ("autonomy is judged
on the target, not on the repository's status", framing section 6) and a
default that treats an undeclared action as a write.

**Commands for workers.** A workshop worker may ask Officina to run, on the
host, the read actions its template lists, and the dry runs of writes inside
the scope. It holds no key: the relay runs the author's own tree under his
identity and hands back the output. It never runs anything the worker wrote,
and it never writes for a worker.

**The browser.** A worker's browser task runs in its container's headless
browser, against the project's own server, while the author keeps his mouse
and keyboard (v0.6 FR-16). Driving the workstation's own browser waits for a
task that needs it (section 5).

**Where the boundary ends.** For workers, the relay is a boundary: a worker
holds no key and has no route to a server (v0.6 FR-3, FR-4), so the relay is
its only way to reach one, and it runs only reads and in-scope dry runs from
the author's catalogue and tree. For host sessions, the relay is a guardrail
(v0.2 section 1): a session runs as the author and can bypass the gate in the
ways AD-27 lists. For host sessions a dry run can write (check-mode
exceptions, delegation). The order binds what runs; it does not make the dry
run harmless. Free repositories are outside by the author's choice.

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Production stays mine.** No write reaches production through
  Officina without my click on a card that shows its dry run.
- **J2 - Approve with the evidence in front of me.** The command and the dry
  run's output on one card; one click; the session goes on with the output,
  and I paste nothing.
- **J3 - What I approved is what runs, as far as it can be checked.** A
  changed tracked file, parameter or catalogue, or too much time, voids the
  approval; the card says what the check does not cover.
- **J4 - Diagnose without lending my key.** A worker reads servers through
  Officina and hands back a report and a branch; it never holds my key, never
  runs its own code on my machine and never writes through the relay.
- **J5 - A browser task without my hands.** A worker checks a web interface
  in its own headless browser while I keep working.

### 2.2 Key User Journeys

The work floor; the guarded project `supervision` of v0.2's journeys;
inventory groups `lab`, `lab-web` and `prod-web`; hosts `lab-web01` and
`web01`, `web02` under `example.org`.

1. **Count (gate 0).** Before any story, the author goes through the measured
   session and counts its production writes: `ansible-playbook` runs without
   `--check` on a non-lab limit, and SSH commands that changed state; and how
   many playbooks and groups they touched. He records the count in the
   research notes.
2. **An approval with its dry run (story 2.8, the baseline).** `apply-prod`
   is `human` and names `check`. The session runs `officina run check
   playbook=alerting.yml group=prod-web`, reads the diff, then `officina run
   apply-prod ...` with the same values. Needs you shows the argument vector,
   the dry run's output and "tracked and unignored files unchanged since the
   dry run". The author clicks Approve; the waiting call runs the playbook and
   the session reads its output. Had the session edited a template in
   between, the call would refuse `check_not_satisfied`.
3. **Per-project scope (Slice A, if built).** Two weeks in, the Production
   writes row shows a `free` action that reached a production host. The author
   gives `supervision` a scope (lab groups, lab host names, `firewall.yml`
   kept on order) and marks its actions' effects. Now `apply` on `prod-web`,
   whatever its class, takes story 2.8's path with its check, and an action
   without `effect` counts as a write.
4. **A worker diagnosis (Slice B).** The author launches the workshop
   template `investigate-service` (`service = nginx`, `group = lab-web`). The
   worker runs `officina-relay status host=lab-web01.lab.example.org
   service=nginx` and `officina-relay journal-tail ...`; the host runs both
   from the author's tree and returns the output. Its `check` on `lab-web`
   runs, being in scope; its `check` on `prod-web` is refused
   `order_required`; any write is refused `worker_write`. It writes a report
   and a playbook fix on its branch. The author imports the branch and applies
   it from his own session.
5. **A browser task (v0.6 FR-16).** A workshop template for a personal Svelte
   project starts the project's server in the container and walks its main
   pages with the project's Playwright, reporting console errors. The author
   keeps typing; nothing prompts.

### 2.3 Candidates (proposed, to confirm by the owner)

1. **Scope** (work floor, Slice A, only if triggered). For each guarded
   infrastructure project: lab groups and lab host names in scope; every
   production group out; the framing's two sensitive playbooks (`firewall.yml`,
   `admin-users.yml`) kept on order everywhere. Until then, the same intent
   is written as action pairs (section 4.1).
2. **Worker templates** (work floor, Slice B). `investigate-<service>`: read
   service states, recent logs and a lab `--check`, then write a report and a
   proposed fix; `lab-drift`: a weekly `--check` over the lab groups,
   reported as a package. The second could later be a v0.7 standing order:
   it reads only.
3. **Browser tasks.** (a) Personal floor: a UI walkthrough of a personal
   Svelte project on its own server, in the workshop's headless browser (v0.6
   FR-16, v0.6 gate 8). (b) Work floor: reading a lab monitoring dashboard
   after a lab change, as an AD-36 HTTP action with a credential bound to its
   host.

## 3. Glossary

- **Intervention order** - the framing's term for a `human` approval of an
  action that names a `check`: the command, the dry run's output and the
  author's click (story 2.8, D-170).
- **Dry run** - the `check` an `after-check` or `human` action names: its
  `--check --diff` form, or a read of the state it would change.
- **Effect** - what an action does to its target: `read` or `write`, declared
  by the author (FR-1).
- **Target parameter** - the parameter of a write action whose value says
  where it writes: an inventory group, a host (FR-1).
- **Autonomous scope** - per guarded project, the target values and host
  names where writes run under their class (FR-2). The framing's `cibles`.
- **Kept on order** - parameter values that take story 2.8's path even
  inside the scope (FR-2). The framing's `sur-accord`.
- **In scope**, **out of scope** - a call's standing under FR-3 or FR-7.
- **Targets action** - an optional read action listing, one per line on its
  stdout, the host names behind a write's target (FR-1, FR-3).
- **Bound paths** - project paths hashed by content into the tree
  fingerprint whatever git ignores (FR-4).
- **Worker relay request** - a workshop worker's request for a catalogue
  action, run on the host from the author's tree (FR-6, FR-7).

## 4. Features

Documented surfaces this PRD rests on, beyond E1 to E17 and earlier PRDs'
S-numbers. Each becomes an E-entry once a check that uses it has run. Pages
are under `https://code.claude.com/docs/en/`, read on 2026-10-28. Numbers are
local to this PRD. S1, S4, S5 and S6 back the deferred browser slice
(section 5's backlog note); S2 and S3 back the non-goals.

- **S1** (`chrome`) Claude Code "integrates with the Claude in Chrome browser
  extension" from the CLI or the VS Code extension. It "works with Google
  Chrome and Microsoft Edge" and sets up the connection in "other
  Chromium-based browsers, including Brave, Arc, Vivaldi, and Opera"; it
  "isn't supported in Windows Subsystem for Linux (WSL)". Claude "opens new
  tabs for browser tasks and shares your browser's login state, so it can
  access any site you're already signed into. Browser actions run in a
  visible Chrome window in real time." At a login page or CAPTCHA "it pauses
  and asks you to handle it manually". Requirements: extension 1.0.36 or
  later, a direct Anthropic plan, and "signing in with `/login`. If you
  authenticate with an API key or a long-lived token from `claude
  setup-token`, Claude Code keeps Chrome integration off, even when you pass
  `--chrome`." The first launch shows "a one-time dialog". "If more than one
  browser is connected, you choose which one Claude uses." "Site-level
  permissions are inherited from the Chrome extension"; in auto mode, when the
  classifier approves a browser call, "the extension skips its own per-site
  check for that call". A project's settings cannot turn Chrome on. On
  Windows: "if another process is using the same named pipe, restart Claude
  Code. Close any other Claude Code sessions that might be using Chrome." The
  connection needs network access to `bridge.claudeusercontent.com`. Tools are
  listed under the `claude-in-chrome` MCP server.
- **S2** (`computer-use`) Computer use in the CLI "is a research preview on
  macOS", Pro or Max only, and "requires an interactive session, so it is not
  available in non-interactive mode with the `-p` flag". It runs "on your
  actual desktop": "other visible apps are hidden", "Only one session at a
  time can use your computer". "On Windows, use computer use in Desktop
  instead."
- **S3** (`desktop`, `whats-new/2026-w28`, `whats-new/2026-w36`) In the
  Desktop app, computer use is "a research preview on macOS and Windows";
  background computer use is described for macOS only. The Desktop app's
  Browser pane "uses a clean browser profile, separate from your personal
  browser"; it is a Desktop app feature. Officina hosts the `claude` CLI, not
  the Desktop app.
- **S4** (`cli-reference`, `agent-view`) `--chrome` enables and `--no-chrome`
  disables Chrome integration "for this session". The flags that carry to a
  backgrounded session are `--mcp-config`, `--strict-mcp-config`,
  `--settings`, `--setting-sources`, `--add-dir`, `--plugin-dir`,
  `--fallback-model` and `--allow-dangerously-skip-permissions`; `--chrome` is
  not among them.
- **S5** (`env-vars`) `CLAUDE_CODE_ENABLE_CFC` "Set to `1` to start a CLI
  session with Chrome integration on, or `0` to start with it off"; ignored
  as `1` in project and local settings.
- **S6** (`hooks`) "MCP tools follow the naming pattern
  `mcp__<server>__<tool>`"; "To match every tool from a server, append `.*`
  to the server prefix". The part read does not give an MCP call's
  `tool_input` shape, nor say that a `PreToolUse` deny on an MCP tool holds in
  `bypassPermissions` (E12 verified it for `Bash`).

### 4.0 Slice 0: checks

#### FR-0: Gates first
The section 8 questions marked **gate** are run on the workstation, or
answered by the owner, and recorded as E-entries.
- Before any v0.8 story: gate 0's count.
- Before any Slice A story: Slice A's trigger (section 7's first table), the
  rest of gate 0, and gate 1. FR-4 only after gate 1 finds ignored roles or
  collections.
- Before any Slice B story: gates 1, 2 and 3.

A failed gate changes the FR it names first, as E12 reshaped v0.2's
approvals; section 8 says how for each.

### 4.1 What story 2.8 already gives

The framing's intervention order is "the exact command, the targeted hosts,
the dry run's output" (framing section 6). Story 2.8, as D-170 amends it,
builds it in the approval flow, and v0.8 builds nothing beside it:
- **The command:** the argument vector, rebuilt from the approved catalogue
  (D-27), one chip per argument (AD-35).
- **The targeted hosts:** the limit's value, as an argument. Resolved host
  names come only with Slice A's targets action (FR-3).
- **The dry run's output:** the request needs a matching successful check
  (project, check action, mapped parameters, catalogue hash, tree
  fingerprint, 30 minutes) or `run` refuses `check_required`; the check's
  output (stdout then stderr, 256 KiB, a cut marker past it) is kept 7 days
  and shown on the card as inert text.
- **The binding:** the request key's inputs include the check's run id and
  the tree fingerprint; just before consuming an approval, `run` recomputes
  the fingerprint and refuses `check_not_satisfied` on a difference, leaving
  the approval unconsumed. Approvals keep AD-33's 15 minutes and single use.
- **The click:** pointer only (D-28); no CLI, no `officina mcp` tool, no
  model approves.

**Who runs it.** The session's waiting `officina run` runs the approved
action and goes on with its output (AD-33). The framing has Officina prefill
the author's terminal for him to press Enter. That is not built: the output
would reach his terminal and not the session, which is the mailbox the
measured pain is about, and the window's pointer click is the gesture v0.2
verified and journals (E12, D-28). Listed for confirmation (question 5).

**What the binding does not cover.** The card reads "tracked and unignored
files unchanged since the dry run" (D-170), never "same code". The
fingerprint (AD-33, D-22) does not cover: files git ignores (roles and
collections installed by `ansible-galaxy`, vault password files, local
`host_vars`), collections and roles outside the tree, Ansible configuration
outside the tree (`ANSIBLE_CONFIG`, the home directory), the interpreter's
packages behind the action's `program`, dynamic inventory output, the state of
the hosts between the dry run and the run, vault content and variables
outside the tree, and an inventory's address mapping (`ansible_host`). Time
bounds the state of the hosts; FR-4 adds ignored roles and collections if
gate 1 finds them; question 4 measures whether re-running the dry run before
consumption would be worth it.

**Scope today.** The README shows scope as action pairs: `apply-lab`
(`after-check`, `group` limited by `values` or `pattern` to lab groups,
`playbook` a list without `firewall.yml` and `admin-users.yml`) and
`apply-prod` (`human`, naming `check`). Gives up the fail-closed default: a
write action the author forgot to class `human` stays able to reach
production. Slice A recovers it if use shows the need.

### 4.2 Effect and scope (Slice A, conditional)

FR-1 to FR-3 are built only if section 7's first table shows a write that
reached production through a non-`human` action, or the author asks for
per-project scope. FR-1 and FR-2's declarations are also built with Slice B
if Slice A is not, for its own use (FR-7).

#### FR-1: Effect and target
Schema 2 gains action keys; the floor schema number does not change (AD-75's
reasoning). Every key below belongs to the canonical catalogue (AD-24): the
canonical format goes up once when v0.8's keys land, and again when Slice A's
enforcement lands if that is later, so the author approves the change that
starts FR-3 on a banner listing the writes it sends to story 2.8's path
(D-131's rule), which voids the approval on purpose, and the banner shows the
diff.

```toml
[[action]]
name = "apply"
class = "after-check"
check = "check"
effect = "write"            # "read" | "write"
target = "group"            # write only: the parameter that says where it writes
targets = "list-hosts"      # write only, optional
runner = "wsl"
program = "/path/to/venv/bin/ansible-playbook"
args = ["--diff", "-i", "inventory/hosts", "--limit", "{group}", "playbooks/{playbook}"]

[[action]]
name = "list-hosts"
class = "free"
effect = "read"
runner = "wsl"
program = "/path/to/venv/bin/ansible"
args = ["{group}", "-i", "inventory/hosts", "--list-hosts"]
```

Phase 2 rules (AD-23), each an issue kind with a failing fixture:
- `effect` absent reads as `write` and is reported (`effect_undeclared`): a
  forgotten key never lets a write through unnoticed.
- On a read action, `target` and `targets` are issues.
- `target` names one of the action's parameters. A write without it is
  reported (`target_undeclared`), and every call of it is out of scope.
- When `program` ends in `ansible-playbook` and the `target` placeholder is
  not the argument after `--limit` or `-l`, nor inside `--limit=`, a warning
  (`target_not_limit`): the runner cannot know every tool, so it does not
  refuse.
- `targets` names a valid `read` action whose parameter names are a subset of
  the write's, the target parameter included.
- An HTTP action (AD-36) is GET or HEAD: `effect = "write"` on one is an
  issue.

Officina never infers an effect: a `read` that writes is the author's
declaration, and the catalogue diff labels every change from `write` to
`read` (FR-9). The README says beside `effect`: an Ansible `--check` action is
a read only if no task it can reach sets `check_mode: false` or delegates.

On an HTTP action, `effect` absent reads `read`. `effect_undeclared`,
`target_undeclared` and `target_not_limit` are notices: the action stays
valid.

#### FR-2: The autonomous scope
```toml
[[project]]
name = "supervision"
path = "Supervision"
autonomy = "guarded"
actions = ["check", "list-hosts", "apply", "status"]

[project.scope]
targets = { pattern = "lab(-[a-z0-9-]{1,30})?" }        # or { values = [...] }
hosts = { pattern = "[a-z0-9-]{1,40}\\.lab\\.example\\.org" }
on_order = { playbook = ["firewall.yml", "admin-users.yml"] }
```

- Scope binds the relay in guarded projects. Without `[project.scope]` a
  guarded project's scope is empty and every write takes story 2.8's path:
  the framing's default profile is `strict` (framing section 6).
- In a free project an out-of-scope write runs under its class, and doctor
  lists it as "free project, scope not enforced": there the gate makes no
  decision (v0.2 FR-6), so refusing would only push sessions off the relay,
  the one place their runs are journaled.
- `targets` matches the target parameter's value, whatever it names; `hosts`
  matches the names a targets action lists. Each is `values` or `pattern`,
  under AD-30's value rules and AD-23's regular expression limits.
- `on_order` maps a parameter name to values that always take story 2.8's
  path; the name must be a parameter of an action the project uses.

#### FR-3: The decision for a write
At AD-29's step 6, before the class, in a guarded project:
- A read runs under its class, as today.
- A write is **in scope** when:
  1. its target parameter is declared and its value matches `targets`;
  2. no parameter value is kept on order;
  3. when it declares a targets action, that action runs first through the
     relay (journaled like any run; one stderr line to the session: "targets:
     N hosts, in scope", or the first name out of scope), and every name it
     lists matches `hosts`, with at least one name. Names are read from
     **stdout only**: one per line after trimming spaces, empty lines
     skipped, after exactly one first line `hosts (N):` whose N equals the
     count of names; any other stdout line, a name failing the host name
     rule, a non-zero exit or a timeout fails the resolution, which counts as
     out of scope. Stderr (Ansible's `[WARNING]` lines) is kept, shown, and
     never parsed.
- In scope, the class applies exactly as in v0.2 and v0.5.
- Out of scope, whatever the class, the call takes story 2.8's `human` path
  with its check (D-170). A write that names no `check` is refused
  `no_dry_run`. An `auto` action is reviewed by its model only in scope (v0.5
  FR-7): no model weighs an out-of-scope write.
- When targets were resolved, the request also records the SHA-256 of the
  resolved names, sorted among the request key's inputs; the card shows the
  cause ("target `prod-web` not in scope", "`playbook = firewall.yml` kept on
  order"), the names and the kept stderr; and `run` resolves the targets
  again just before consuming the approval: a different set of names, or a
  failed resolution, refuses `targets_changed` and leaves the approval
  unconsumed (D-170's pattern). The
  targets action's stdout and stderr are kept as D-170 keeps a check's.

`run_started` gains `scope` (`read`, `in`, `out`) and, when resolved, the
targets SHA-256. Doctor names every write judged on its target value alone
("scope judged on names only").

The resolution reads inventory names. An inventory that maps a name in scope
to another address passes it, a dynamic inventory can change between the
resolution and the run, and delegation reaches hosts outside the limit
(section 4.1). In a guarded repository a session can edit the inventory: a
guardrail, as v0.2 section 1 says. A worker cannot edit the author's tree
(FR-7).

With Slice A, a write of any class may name a `check`, used only out of
scope. An `auto` write out of scope escalates before any review call.

#### FR-4: Bound paths (only if gate 1 finds ignored roles or collections)
```toml
[[project]]
name = "supervision"
bind = ["roles", "collections"]
```

`bind` (canonical catalogue, at most 8 paths, relative to the project folder,
no `..` segment) names folders whose files are hashed by content into the
project's tree fingerprint (AD-33, D-22), whatever git ignores, with
D-22's content rule (1 MiB by content, size beyond). The after-check rule and
story 2.8's binding then cover them. Past 10,000 files or 256 MiB in all, the
fingerprint is marked `bind_too_large` and a request that needs it is refused
with that cause; doctor shows each bound path's size. The card's line becomes
"tracked, unignored and bound files unchanged since the dry run".

The walk never follows a link; a link binds by its target text and a missing
bound path binds its absence.

### 4.3 Commands for workers (Slice B)

#### FR-5: Relay actions of a workshop template
```toml
[template.workshop]
base = ["main"]
max_minutes = 60
relay_actions = ["status", "journal-tail", "check"]
```

`relay_actions` (canonical catalogue, at most 16) lists the actions a worker
of this template may request. Phase 2: each names a valid approved action that
every project of the template lists (v0.2 FR-1) and that declares `effect =
"read"` (FR-1); a write, or an action whose effect is undeclared, is an issue.
The card shows the list beside the allow-list (v0.6 FR-6).

A relay action's class is `free` or `after-check`: a worker never creates an
approval request.

#### FR-6: The worker's channel
The workshop image (v0.6 addendum AD-99) carries `officina-relay`, a program
of the `officina-container` crate (AD-100): `officina-relay <action>
[name=value ...]` and `--list`. It hands one request to the window and waits
for its response up to the action's `timeout_s` and the run's deadline,
printing the output and exiting with `officina run`'s codes (AD-29: the
program's code, 126 refused, 124 timeout, 127 could not start). The window,
which already follows the run (v0.6 FR-8), collects requests and returns
responses through the engine. A mailbox container per run, on the run's
internal network only, attached to the window by its standard streams,
carries requests and responses; `officina-relay` is `officina-entry` by
another name and reads the mailbox's address from the plan. With no answer
within 10 s it prints `no_answer` and exits 126., within these requirements
(gate 2):
- no TCP port on the host (framing principle 6), no route from the run's
  networks to the host, no host path the worker can write (v0.6 FR-3); the
  gateway does not change;
- one request at a time per run, at most one per 2 s and 120 per run, each at
  most 16 KiB; a response at most the action's `max_output_kib`;
- a request is untrusted bytes, parsed in core with AD-29's token rules;
  anything malformed, oversized or over the rate is refused and journaled; 20
  refusals in one run stop it (`relay_abuse`), as the gateway's limit does
  (v0.6 FR-4);
- with the window closed nobody answers: the call times out and is refused.

#### FR-7: What the host runs for a worker
The window decides in core with AD-29's order, the run standing in for the
session:
1. stop flags: the relay's and the workshop's (AD-34, v0.6 FR-14);
2. the current catalogue hash equals the approved one and the run card's: a
   catalogue changed since launch refuses;
3. the action is in the approved catalogue and the run's project lists it;
4. any write is refused `worker_write`;
5. the action is in its template's `relay_actions` as approved at launch,
   and its parameters pass AD-30, a value that is an absolute path refused;
6. a **dry run** (an action that is the `check` of some write action of the
   project) is judged by that write's target rule against the project's scope
   (FR-2, FR-3 items 1 to 3, the targets action included): out of scope it is
   refused `order_required`. A dry run whose write declares no target is out
   of scope. An action that is the check of several writes is in scope only
   if every one of them would be;
7. the relay's runner (AD-30, AD-31) runs the action in the **host** project
   folder, from the author's tree, as a session's `officina run` would; never
   a path in the worker's clone, never its bundle. `run_started` and
   `run_finished` carry `workshop_run` in place of `session_id`.

A run the governor launched (v0.7 FR-12) is a worker like any other: it reads
only.

**Why reads and in-scope dry runs only.** A dry run can write: a task with
`check_mode: false` (or `no`) runs for real under `--check`, and
`delegate_to`, `local_action` and `run_once` reach hosts other than the limit.
A worker reads text that others can shape (a log line, a page), so a dry run
it may aim at production is a path from untrusted text to production with no
gesture; judged by target, it stays on hosts where the author accepts
breakage. Writes are left out entirely: "the author's tree" is his working
copy now, uncommitted edits included, and a worker applying it to the lab
would push someone else's change in progress at a moment the worker picks.
Gate 1 counts the check-mode exceptions and delegations each guarded project
holds; doctor names each playbook with a non-zero count beside the dry-run
actions listed for workers that can reach it.

**Why only the author's tree.** An Ansible playbook runs code on the control
node, not only on its targets: lookups such as `pipe`, `delegate_to:
localhost`, `local_action`, and plugins shipped in the repository
(`library/`, `filter_plugins/`, `callback_plugins/`), in check mode too.
Running a worker's playbook on the host would run the worker's code as the
author, in his WSL, beside his keys, and the boundary would be gone. So a
worker observes servers and proposes changes on its branch; the author
imports them and applies them from his own session. The framing's `iterative`
profile for workers (apply what it changed, observe, correct) needs a control
node inside the boundary with an identity scoped to its targets: the
framing's write relay with a restricted account, after v1 (section 5;
question 8).

**Identity.** Until the diagnostic accounts the framing names exist (question
7), a worker's reads run under the author's own SSH identity, bounded only by
the catalogue's argument vector. When they exist, the author points read
actions at them in the catalogue (`args`, `env`, AD-22), with no Officina
change.

**Exposure.** A relay output enters the worker's context and can leave
through `api.anthropic.com` (v0.6 section 1). Do not list for a worker a read
whose output you would not paste into a model. The README says so beside
`relay_actions`.

The package (v0.6 FR-10) gains the run's relay calls: action, parameters,
outcome and exit code, never the output. The review panel lists them; an
`order_required` refusal reads "the worker asked for a dry run out of scope:
run it from a session".

Scope binds a worker's dry run in free and guarded projects alike. A worker's
successful dry run counts as a check for a session's after-check or human
request on the same values and tree. `relay_abuse` ends the run and is not an
environment failure.

#### FR-8: The battery's relay family
The escape battery (v0.6 FR-15) gains a relay family, played through
`officina-relay` against a fake tool that counts launches (AD-37): an action
outside `relay_actions`; a write of each class (`worker_write`); a dry run on
an out-of-scope target, on a kept-on-order value and with a targets
resolution that fails (`order_required`); a parameter starting with `-` or
with a `..` segment; a malformed and an oversized request; a flood past the
rate; a request after the deadline; a request while the relay is stopped.
Pass: every attempt refused and journaled, no launch of a requested action;
on the engine every program of the battery's catalogue is a sentinel that
does not exist, and the failing targets resolution case, which launches its
targets action once, runs in the host family. A host test asserts
that the runner's working directory and every path argument of a worker's
request lie in the host project folder, never under the run folder.

### 4.4 Window and doctor

#### FR-9: In the window
- **Approval card** (story 2.8): with Slice A, the out-of-scope cause, the
  resolved names and the targets action's stderr; with FR-4, the bound-files
  line.
- **Catalogue banner and diff** (AD-24, AD-35): a "widens autonomy" label on
  a scope `targets` or `hosts` value added or a pattern changed, an
  `on_order` value removed, an action's `effect` changed to `read`, a write's
  `target` or targets action removed, a `bind` path removed, an action added
  to `relay_actions`; an "approval now required" label on the writes whose
  out-of-scope calls the change sends to story 2.8's path.
- **Relay view:** per project, its scope, kept values and bound paths; per
  action, its effect and whether an out-of-scope call has a dry run
  (`no_dry_run` named); worker relay calls.
- **Workshop:** relay actions on the card; relay calls in the review panel
  (FR-7).
- **Briefing** (v0.5 FR-1): worker relay refusals since the cursor.
  `officina mcp` serves them through `briefing` and `relay_activity`; no tool
  is added, and no kept output is served.
- Every text from a session, a run or a worker stays `Untrusted<T>` (AD-74).

#### FR-10: Doctor
- Kept outputs (story 2.8, FR-3): count and size, and a warning when the
  Officina home lies under a synced or backed-up folder, since they may hold
  what `--diff` shows (whichever v0.8 slice ships first).
- Slice A: actions without `effect`; writes without a target, a check or a
  targets action; `target_not_limit` warnings; guarded projects without a
  scope (every write takes story 2.8's path); free projects with a scope
  ("scope not enforced"); bound paths and their sizes.
- Slice B: `officina-relay` in each workshop image's inventory (AD-118); each
  template's relay actions; that worker reads run under the author's identity
  until question 7 is answered; gate 3's outcome as the README records it;
  the files under the project, ignored ones included, in which a line-based
  scan finds a `check_mode` false or no, `delegate_to`, `local_action` or
  `run_once`, with counts, beside the dry-run actions listed for workers.

## 5. Non-Goals (Explicit)

- Rebuilding in v0.8 what story 2.8 builds: the dry run on the card, its
  kept output, the binding to the check run and the tracked tree (D-170).
- Officina deciding what is in scope or inferring an effect: both are the
  author's declarations, approved with the catalogue.
- Scope enforced in free projects (FR-2).
- Any write for a worker, in scope or not (`worker_write`); intervention
  orders for workers; an approval created or given by a model, the
  orchestrator, an `auto` review or the governor.
- The framing's `iterative` profile for workers, in which a worker applies
  its own changes: worker code would run on the author's control node (FR-7).
  It needs a control node inside the boundary holding a target-scoped
  identity, which is the framing's write relay with a restricted account,
  after v1.
- Running an approved action in the author's terminal (section 4.1;
  question 5).
- Re-running a dry run before consumption and comparing outputs (question
  4).
- Binding what section 4.1 lists as not covered, beyond FR-4's bound paths.
- Creating, requesting or managing any server account. The diagnostic
  accounts are the owner's request to his security officer (question 7).
- A relay path for host sessions other than `officina run`; an MCP server for
  relay actions (v0.2 section 5 holds).
- Computer use. In the CLI it runs on macOS only, interactively, takes the
  screen and hides other apps (S2); on Windows it is a Desktop app feature
  (S3), and Officina hosts the CLI.
- The Desktop app's Browser pane (S3): a clean profile, but a Desktop feature.
- A browser tool for workshop workers beyond v0.6 FR-16's headless Chromium,
  driven by the project's own Playwright code. A Playwright MCP server would
  be a new third-party dependency for a need no candidate names.
- Any write method for HTTP actions.
- **Backlog note: the workstation's browser, deferred with the owner's
  confirmation (question 9), not dropped.** Deferred items:
  - browser work orders: a host template's `browser = true` giving a
    background work order Claude in Chrome's tools (`--chrome`, S4;
    `CLAUDE_CODE_ENABLE_CFC`, S5; or an interactive console), at most one
    live (S1's named-pipe conflicts);
  - the worker browser: a Chromium-based profile kept for workers, the
    extension installed only there, signed into only what the author lends,
    site permissions limited to those sites (S1);
  - their gates: Chrome in a background session; profile isolation; focus
    and keystrokes while a task runs; site prompts and per-site checks under
    bypass; two sessions on Chrome at once on Windows; the employer's
    agreement and `/login` on the floor (S1);
  - doctor's read of the native messaging host keys (S1) and its `windows-sys`
    `Win32_System_Registry` use.
  Revived when the owner names a browser task that needs a signed-in site or
  a LAN host that neither the workshop nor an HTTP action can reach, and, on
  the work floor, records the employer's written agreement. If revived, every
  relay write from a browser work order is refused (`browser_write`), as
  FR-7 does for workers. Dropped outright, not deferred: gating the browser
  calls of the author's own sessions in guarded projects, which no candidate
  needs and which would rest on undocumented tool inputs (S6).
- The installer, updates, `doctor` for a fresh machine, documentation and the
  Linux build (v1).
- After v1: voice, macOS, the write relay with a restricted account, extra
  outbound connectors.

## 6. MVP Scope

### 6.1.1 Delivery order
- **Slice 0:** FR-0. Gate 0's count before any v0.8 story; gate 1 before
  Slice A or B; gates 2 and 3 before Slice B.
- **Slice A, conditional:** FR-1 to FR-3 and their parts of FR-9 and FR-10,
  built only when section 7's first table triggers them; FR-4 only when gate
  1 finds ignored roles or collections under a guarded project, whatever the
  trigger. Needs story 2.8. Until then the measured pain is served by story
  2.8 and the README's action pairs.
- **Slice B, commands for workers:** FR-5 to FR-8, FR-1 and FR-2's
  declarations if Slice A is not built, and their parts of FR-9 and FR-10.
  Needs v0.6's Slice A. Usable alone: a workshop worker reads servers and
  runs lab dry runs through Officina, holding no key.
- **The exit criterion's browser half:** no slice. It is read on v0.6 FR-16
  (v0.6 gate 8 recorded) with one confirmed task (section 7).

What any slice defers is done before the next version starts (P-7), Slice A's
conditional parts excepted: they are built when their trigger holds, at the
latest before v1's close, or recorded as not needed at v1's close. This is an
explicit exception to P-7, listed for the owner's confirmation with question
6. v0.8 consumes no v0.7 code.

### 6.1.2 Dependencies for the owner's approval

No crate, image, Node package, Windows feature or third-party program is
added. `officina-relay` is a binary of the existing `officina-container`
crate; the relay family reuses the fake tool (AD-37).

## 7. Success Metrics

Read over story 2.8's first two weeks at work. This table decides Slice A:

| Metric | Target | How measured |
|---|---|---|
| Production writes | every production change over the 10 working days, listed from the servers' own records (an Ansible callback log or the hosts' change logs, whichever exists), matched to an approved `human` request or explained; one that reached production through a non-`human` action triggers Slice A | server-side records, journal |
| Hand writes | production writes the author made on a guarded project's targets outside Officina, in his log with why; each names the gap (a missing action, a missing check, an approval too slow) | author's log |
| Use | fewer than 3 approvals of `human` actions naming a check in 10 working days, and fewer than 3 hand writes: production writes are rare, the fold into story 2.8 stands, and Slice A is not built unless the author asks for per-project scope | journal, author's log |
| Decision time | median from the card shown to Approve or Reject under 3 minutes; above it, the card is revisited (output folding, a summary of changed tasks) before any v0.8 story | `approval_requested` to `approval_given` or `approval_rejected` |
| Void | `check_required` and `check_not_satisfied` refusals and expired approvals under 20 % of requests; above it, the causes are read and the values changed | journal |
| Mailbox | 3 production subjects carried from start to finish by approvals with their dry run, with no output pasted by the author | author's log, journal |

Slice A, if built:

| Metric | Target | How measured |
|---|---|---|
| Relay containment | zero `run_started` of an out-of-scope write in a guarded project without a consumed approval whose request named its check and whose bindings held. This checks the relay against its own verdicts, nothing more; the Production writes row checks the servers | journal audit; e2e; mutants `mutant-scope-skip`, `mutant-targets-skip` each turn the scope vectors red |
| Production writes | the row above, continued over Slice A's first two weeks | as above |
| Friction | `no_dry_run` refusals, `effect_undeclared` and `target_not_limit` warnings, and the time per targets resolution (p50, p95), per week; an action refused more than 3 times in a week gets a catalogue fix | journal, doctor |
| Bound paths (FR-4) | a change to a bound ignored file between the dry run and the approval refuses `check_not_satisfied` | e2e |

Slice B:

| Metric | Target | How measured |
|---|---|---|
| Worker containment | the battery's relay family all refused and journaled with zero launches; zero worker write ran; zero worker dry run ran out of scope; zero worker request ran from a path under its run folder | battery (FR-8), journal audit, host test |
| Use | over 10 working days: at least 5 workshop runs used relay reads, from at least one template the owner confirms; fewer, the close decides whether Slice B stays | journal |
| Value | packages of runs with relay calls imported over decided, at least 50 %; under it, workers' diagnoses cost more review than they return, and the close decides | v0.6 FR-12 verdicts |
| Round trip | from the window's receipt of a request without a targets resolution to the action's start, plus gate 2's in-container hop; requests with a resolution are reported apart under 2 s at p95 | journal times |

Exit criteria:

| Metric | Target | How measured |
|---|---|---|
| No write outside autonomy | over the two weeks of the first table, and of Slice A if built: every production change matched to an approval or explained; zero worker write | Production writes rows, Worker containment |
| Browser task | one workshop run of a confirmed template (section 2.3, 3a) uses v0.6 FR-16's headless Chromium on the project's own server and completes with no author input during the run: no attach, no prompt, no approval | v0.6 gate 8 recorded; journal; board states |

The v0.8 close reads the first table (or its reading at story 2.8's close),
Slice B's Use and Value rows, and the two exit criteria (P-16).

## 8. Open Questions

0. **Owner gate, first.** (a) Before any v0.8 story: in the measured mailbox
   session, how many of the roughly 450 commands were writes on production
   (`ansible-playbook` without `--check` on a non-lab limit, and SSH commands
   that changed state), and how many distinct playbooks and groups did they
   touch? The owner records the count in the research notes. Over 10 write
   actions per project to split into pairs, Slice A's per-project scope is
   worth building without waiting for the trigger. (b) Before Slice A, if
   triggered: for each guarded project at work, which target values and host
   names are in scope, which values stay on order, and which actions are
   reads.
1. **Gate (FR-1 to FR-4, FR-7), before Slice A or B.** On the work floor,
   through the catalogue's WSL runner (E13), per guarded project:
   - does `ansible <group> -i <inventory> --list-hosts` print on stdout only
     the `hosts (N):` header and one name per line, for a group, an empty
     group and an unknown one, and what does it print on stderr (exit codes,
     times)?
   - how large is a real production `--check --diff` output, and does it
     carry escape sequences with the author's `ansible.cfg`?
   - the count of `check_mode: (false|no)`, `delegate_to`, `local_action` and
     `run_once` in the playbooks a check action can name;
   - the ignored paths under the tree that Ansible reads (roles, collections,
     vault files, `host_vars`), whether the inventory is static or dynamic,
     and `ansible-config dump --only-changed` run through the runner.
   Consequences: the listing format fixes FR-3's reading, or targets fall
   back to the parameter value and doctor says "scope judged on names only";
   ignored roles or collections bring FR-4; the counts go to doctor and the
   README; the card states that the names were read from the inventory and
   are read again just before the run.
2. **Gate (FR-6), before Slice B.** Does the mechanism the architecture
   chooses meet FR-6's requirements on Docker Desktop: a request and its
   response between a running worker and the window, with no host port, no
   writable host path and no change to the gateway, at p95 under 2 s? If none
   does, Slice B is not built and the close says so.
3. **Gate (FR-7), before Slice B.** Does an action run by the window's
   process, not by a session's Bash, reach the same WSL distribution,
   `ANSIBLE_CONFIG` and SSH agent as `officina run` from a session? Does the
   agent ask for confirmation on each use (`ssh-add -c`, the framing's standing
   advice), and where does that prompt appear? If it prompts, every worker
   read takes the author's hand, and question 7's dedicated key becomes a
   precondition of Slice B. On the workstation, `SSH_AUTH_SOCK` is unset in a
   `wsl.exe --exec` child: an agent started by a shell profile is invisible to
   both paths unless the action's `env` names its socket (addendum section
   1).
4. **Drift.** Over story 2.8's two weeks, run each approved action's dry run
   again a few minutes later on unchanged lab targets: how often is the output
   byte-identical after escape stripping? Above 95 %, a later version may
   re-run the dry run before consumption and refuse on a difference; below,
   the output carries volatile text and the comparison would refuse good
   approvals.
5. **Owner, section 4.1.** The framing has an order prefill the author's
   terminal for him to press Enter. Story 2.8 runs the approved action in the
   session's waiting call, so the output reaches the session. Confirm the
   departure.
6. **Owner, roadmap order.** The dry run card and the tree binding are folded
   into story 2.8 now (D-170). v0.8 Slice B (reads only) follows v0.6. v0.7
   keeps its place and closes on its own evidence. The rest of Slice A waits
   for story 2.8's two-week reading, under section 6.1.1's exception to P-7.
7. **Owner, diagnostic accounts.** The framing's read-only diagnostic
   accounts (forced command to a probe dispatcher, sudo limited to reads) are
   not yet requested from the security officer. v0.8 does not wait on them:
   worker reads run under the author's identity, bounded by the catalogue.
   Recommendation: request them now, worded as the framing does; meanwhile, a
   dedicated key for relay read actions, without `ssh-add -c` and restricted
   on the lab hosts by `authorized_keys` options, keeps worker reads from
   needing the author's hand (question 3). The same key on production hosts
   is the diagnostic account by another name and needs the same approval.
8. **Owner, `iterative`.** v0.8 builds the observing half of the framing's
   `iterative` profile for workers (reads and in-scope dry runs) and leaves
   applying a worker's own changes to the after-v1 write relay with a
   restricted account (FR-7). Confirm.
9. **Owner, the workstation's browser.** Confirm the deferral of section 5's
   backlog note, the exit criterion's browser half read on v0.6 FR-16, and
   dashboard reads as AD-36 HTTP actions. Revival needs a named task that
   neither the workshop nor an HTTP action can serve, and the employer's
   written agreement on the work floor.
10. **Owner, defaults.** If Slice A is built, a missing `effect` reads as
    `write` and a guarded project's missing scope is empty, so on the first
    v0.8 approval every write v0.2 ran freely on any target in a guarded
    project takes story 2.8's path until the author declares his scope. The
    banner lists them. Confirm the fail-closed defaults.
11. The values 3 approvals and 3 hand writes in 10 working days, 10 write
    actions per project, 3 minutes of decision time, 20 % void, 3 production
    subjects, 8 bound paths, 10,000 files and 256 MiB bound, 16 relay actions
    per template, one request per 2 s and 120 per run, 16 KiB per request, 20
    refusals per run, 5 runs with relay reads and 50 % imported are starting
    values, measured over story 2.8's two weeks and Slice B's use.
