---
title: "PRD: Officina v0.2 - The Relay"
author: Rwx-G
date: 2026-10-27
status: approved 2026-10-27, with the amendments of ../architecture-v0.2.md section 3, which take precedence over the text below (amendment 4's WSL charset is lifted by E13)
inputs:
  - (framing, not published)
  - (requirement challenge, not published)
  - prd/prd-v0.1.md
  - architecture-spine.md
  - research-notes.md
---

# PRD: Officina v0.2 - The Relay

## 0. Document Purpose

Scopes v0.2, Epic 2. v0.1 and its spine (AD-1 to AD-20) hold unless this PRD
says otherwise. A first draft built a relay of HTTP probes; the requirement
challenge asked how much of the measured pain HTTP could answer, and the owner
answered 2 to 5 %. This version is rewritten around what the owner actually
needs: **delegating targeted actions, from start to finish, per repository**.

## 1. Vision

The author ran hundreds of playbooks and SSH commands that sessions handed
them, by choice: to stay at the centre of every subject. That no longer
scales. v0.2 lets the author decide, repository by repository, what a session
may do on its own:

- a **free** repository runs as today in bypass mode; Officina only shows it;
- a **guarded** repository may run the actions of a catalogue the author
  approved, some freely, some only after a dry run, some only after the
  author clicks "approve" in the window.

Everything a guarded session runs through the relay is journaled. Anything
outside the catalogue is refused with a reason the session can act on.

**What this is not.** A session runs as the author. It could, in principle,
edit Officina's files or call the tools directly. The relay is a guardrail
against mistakes and injected instructions, and a record of what was done;
it is not a security boundary. The boundary is the isolated workshop (v0.6),
where the worker does not hold the author's identity.

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Delegate a subject.** When a subject is routine (check a playbook,
  read service states, apply a known change on a lab group), I hand it to a
  session and it runs from start to finish without me.
- **J2 - Keep the risky steps.** When an action touches a shared system, I
  approve it in one click, from the board, with the exact command in front of
  me.
- **J3 - Know what was done.** I can see every action run on my behalf: when,
  by which session, with which parameters, and its outcome.

### 2.2 Key User Journeys

1. **Declare.** In the workspace's floor file the author marks the
   `supervision` repository `guarded` and declares actions: `check` (an
   `ansible-playbook --check --diff` on a playbook from a list, limited to an
   inventory group from a list, class `free`), `apply` (the same without
   `--check`, class `after-check`), `apply-sensitive` (two named playbooks,
   class `human`), and `status` (an SSH `systemctl status` on a host of a
   group, class `free`). The window shows "catalogue changed: approve?"; one
   click approves it.
2. **Delegate.** In that repository's session the author writes "deploy the new
   alert rule on the lab group". The session runs `officina run check ...`,
   reads the diff, runs `officina run apply ...` with the same parameters,
   checks with `officina run status ...`, and reports. The author did nothing.
3. **Approve.** The session needs `firewall.yml`. Its `officina run
   apply-sensitive ...` is refused: "approval requested in Officina". The board
   shows the request in Needs you with the exact command. The author clicks
   Approve; the session retries and the action runs once.
4. **Contain.** The session tries a raw `ansible-playbook` or `ssh` command in
   the guarded repository: refused, with the list of actions it may use
   instead.
5. **Stop.** One click on "Stop the relay": every relay call is refused until
   the author resumes.

## 3. Glossary

- **Free / guarded repository** - a project of the floor file with
  `autonomy = "free"` (Officina does not interfere) or `"guarded"` (the gate
  applies). Default for a declared project: `guarded`. Undeclared folders are
  not touched.
- **Action** - a named command template of the catalogue: a program, fixed
  arguments, and closed parameters. Never a shell line.
- **Class** - how an action is allowed: `free` (runs), `after-check` (runs if
  its declared check action ran successfully with the same parameters in the
  last 30 minutes), `human` (runs once per approval given in the window).
  `auto` is reserved for the orchestrator (v0.5) and refused until then.
- **Gate** - `officina hook` answering Claude Code's `PreToolUse` event for
  `Bash` in guarded repositories.
- **Approval** - a pending request for a `human` action, shown in the window,
  approved or rejected by a click; an approval is single-use and expires.
- **Journal** - append-only JSON Lines of runs, refusals, approvals and stops.

## 4. Features

### 4.1 Catalogue

#### FR-1: Projects and autonomy
The floor file's `[[project]]` entries gain `autonomy` (`free` or `guarded`,
default `guarded`) and `actions` (names of catalogue actions the project may
use).

#### FR-2: Actions
`[[action]]` entries declare `name`, `description`, `class`, `program` (an
absolute path, or `wsl` for the default WSL distribution with an optional
`distro`), `args` (a list; each item a literal or a `{param}` placeholder that
fills exactly one argument), `params`, `timeout_s` (default 600, max 3600),
`max_output_kib` (default 64), and for `after-check` the name of its `check`
action. An action of kind `http` declares a GET or HEAD `url` instead of a
program (secondary type, see FR-12).

#### FR-3: Closed parameters
Each parameter declares exactly one of `values` (a list) or `pattern` (a
regular expression, anchored, linear-time engine, max 64 characters of
input). Values starting with `-`, containing a NUL, or equal to `.` or `..`
are refused whatever the pattern says, so a value can never become an option
or walk a path.

#### FR-4: Catalogue approval
The relay runs only the catalogue the author approved: the window shows a
changed catalogue (its diff by action) and approves it with a click, which
records its hash. Until then every relay call is refused with "catalogue
changed, approval pending in Officina". Invalid actions are reported with
their line and skipped; they never invalidate the floor for the board.

#### FR-5: Secrets
Credentials for HTTP actions live in Windows Credential Manager under
`officina:<name>`, each bound in the catalogue to the hosts it may be sent to.
Command actions use the author's own tools and keys as today (ssh-agent,
ansible vault): v0.2 adds no credential to them.

### 4.2 The gate

#### FR-6: Gate decisions
For a `Bash` call in a guarded repository (the session's `cwd` inside a
`guarded` project), `officina hook` decides:
- a single `officina run <action> key=value ...` call, without shell
  operators, for an action the project may use: no decision (the call runs,
  and `officina run` enforces the class itself);
- any command matching the floor's `guarded_programs` (default:
  `ansible`, `ansible-playbook`, `ssh`, `scp`, `wsl`): **deny**, with the list of
  actions the session may use;
- any attempt to call `officina` approval, resume or catalogue commands:
  **deny**;
- anything else: no decision (Claude Code's own permissions apply).
Free repositories and undeclared folders: no decision, ever. The gate never
exits 2 and never asks (E12).

#### FR-7: Run an action
`officina run <action> key=value ...` validates the catalogue approval, the
project's right to the action, the parameters and the class; then runs the
program with an argument vector (no shell), in the project folder, with the
timeout and an output cap, and prints the output and the exit code. Exit 0 on
success, the program's code on failure, 1 on refusal with the reason on
stderr, 64 on usage errors.

#### FR-8: Classes
- `free`: runs.
- `after-check`: runs if the journal holds a successful run of its check
  action with the same parameters, by the same project, in the last 30
  minutes; otherwise refused with "run `<check>` first".
- `human`: runs if a matching approval (same action, same parameters, same
  project) is approved, unused and less than 15 minutes old, and consumes it;
  otherwise creates a pending approval (or reuses the pending one) and is
  refused with "approval requested in Officina".
- `auto`: refused with "auto approval needs the orchestrator (v0.5)".

### 4.3 Approvals and the window

#### FR-9: Approvals in Needs you
A pending approval appears in Needs you as its own card: project, action,
the exact argument vector, the session that asked, age. Approve and Reject
buttons. Rejected and expired approvals leave the column. Approving is
possible only from the window.

#### FR-10: Relay panel
The window shows the relay: approved catalogue (actions, class, projects),
the pending catalogue diff with its Approve button, the last runs and
refusals, and the stop toggle.

#### FR-11: Stop
A stop flag in the Officina home, set and cleared only from the window,
makes every relay call refuse ("relay stopped"). Read on every call; an
unreadable flag counts as set.

### 4.4 Secondary: HTTP read actions

#### FR-12: HTTP actions
An action with `url` runs a GET or HEAD: placeholders only in path segments
or query values (never scheme, host or port), no redirect followed, system
TLS store, response capped, credential from FR-5 sent only to its bound host.

### 4.5 Journal and setup

#### FR-13: Journal
One JSON line per run (time, project, session id from the hook or
`CLAUDE_CODE_SESSION_ID`, action, parameters, argument vector or URL, exit
code or HTTP status, duration, output size, SHA-256 of the output), refusal,
catalogue approval, approval given or rejected, stop and resume. Outputs are
not stored. One file per UTC day, appended under a file lock. No hash chain in
v0.2: without an anchor out of the session's reach it would prove nothing.

#### FR-14: Doctor
Doctor validates the catalogue, lists guarded projects, checks that the
`PreToolUse` gate is registered (printing the block to paste otherwise), that
WSL and each action's program resolve, and that HTTP credentials exist in the
Credential Manager. It prints, per guarded project, the `CLAUDE.md` paragraph
that tells sessions to use `officina run` and lists their actions.

## 5. Non-Goals (Explicit)

- Isolation: the session still runs as the author (v0.6).
- The orchestrator's `auto` approvals (v0.5).
- MCP server for actions: `officina run` through Bash covers v0.2; `officina
  mcp` stays reserved for the orchestrator.
- SSH through dedicated diagnostic accounts (needs the security officer's
  approval; actions use the author's SSH today).
- Hash-chained journal (deferred to v0.6, when writer and attacker differ).

## 6. MVP Scope

### 6.1.1 Delivery order
- **Slice A:** FR-1 to FR-3, FR-6 to FR-8 (`free` and `after-check`), FR-13,
  FR-14: delegation from the floor file, gated, journaled.
- **Slice B:** FR-4 catalogue approval, FR-9 approvals and `human`, FR-10,
  FR-11, FR-5 and FR-12 HTTP actions.

## 7. Success Metrics

| Metric | Target | How measured |
|---|---|---|
| Delegation | one guarded subject run start to finish without the author pasting anything | author's log, at work |
| Containment | 100 % of raw guarded programs refused in guarded repositories | e2e on the gate with recorded payloads |
| Closed parameters | 100 % of injection attempts refused before any process starts | e2e with a fake program counting launches |
| Journal coverage | every run, refusal and approval leaves exactly one line | e2e |
| Approval round trip | approve click to the retried action running in under 2 s | e2e |

## 8. Open Questions

- Whether a project-level `defaultMode` override takes guarded repositories
  out of the user-level bypass. Not required: the gate denies in any mode (E12).
- Exact WSL invocation for ansible environments (virtualenvs, vault password
  files): the catalogue's `args` must be enough; to confirm on the work
  workstation.
