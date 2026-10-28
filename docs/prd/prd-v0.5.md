---
title: "PRD: Officina v0.5 - The Orchestrator"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions), with the amendments of ../architecture-v0.5.md section 4 applied
inputs:
  - (framing, not published)
  - (requirement challenge, not published)
  - prd/prd-v0.4.md
  - (v0.4 requirement challenge, not published)
  - prd/prd-v0.1.md
  - prd/prd-v0.2.md
  - prd/prd-v0.3.md
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - stories/epic-3-decisions.md
  - https://code.claude.com/docs/en/cli-reference
  - https://code.claude.com/docs/en/sessions
  - https://code.claude.com/docs/en/agent-view
  - https://code.claude.com/docs/en/permission-modes
  - https://code.claude.com/docs/en/cross-session-messaging
  - https://code.claude.com/docs/en/env-vars
  - https://code.claude.com/docs/en/channels
  - https://code.claude.com/docs/en/channels-reference
---

# PRD: Officina v0.5 - The Orchestrator

## 0. Document Purpose

Scopes v0.5, Epic 5. v0.1 to v0.4 hold. Framing principles 7 ("the registry
is the memory") and 8 ("no LLM launches anything") are tested hardest here:
for the first time Officina launches sessions, and for the first time it hosts
a model session that sees every subject. The owner's delegation model adds the
relay class `auto`, reserved since v0.2 (FR-8 there). `officina hook` does not
change. Revised after a requirement challenge: template parameters take closed
lists only; the launch path has one validating constructor, one spawn module
and a mutation test; the deterministic briefing, work orders and meeting room
come first, the `auto` review second in blind shadow against a rule baseline,
the orchestrator session last; `auto` no longer stops when quota data ages;
verdict proposals are cut; metrics count use as well as safety.

Facts about Claude Code rest on `docs/research-notes.md` (E-numbers) or on
the documented pages listed at the head of section 4 (S-numbers). What
neither settles is in section 8 with the check that settles it; the gating
ones run first (FR-0).

## 1. Vision

The author juggles subjects, not terminals. v0.5 gives the workshop a
foreman's desk. Officina's own code briefs the author on what is open, what
waits and what changed. Routine subjects start as work orders: one click of
the author on a template the author wrote and values from closed lists, shown
exactly as they will run. When an employee waits on a real decision, the
author receives it in the meeting room, sees its commits, its diff, what
blocks and what it asks, and answers a background session in writing.

Guarded actions the author classes `auto` are reviewed by a contained model
call that can only approve, reject or escalate a command the catalogue already
fixed. It starts blind, in shadow, measured against a plain rule, and
approves nothing until the author's own decisions say it may.

Last comes the foreman itself: a `claude` session whose only tools are
Officina's, which reads what Officina records and writes proposals, recaps
and work orders. It keeps nothing of its own; close it, start it again, and it
picks up from the registry. Nothing it writes can act.

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Start the day in one place.** I open Officina's briefing and get what
  is open, what waits for me and what changed since I last read it, without a
  terminal per subject.
- **J2 - Delegate in one click.** A routine agentic subject (a dependency
  bump, a lint sweep) starts from a template I wrote, composed by me or
  proposed by the orchestrator, in one click.
- **J3 - Approve only what is doubtful.** In guarded repositories, actions I
  class `auto` run once the review approves them, after it has earned that on
  my own decisions; whatever it doubts, and whatever it cannot review, comes
  to me.
- **J4 - Answer where I look.** I receive a waiting employee and read what it
  did (commits, diff), what blocks and what it asks; I answer a background
  session in writing, and an editor session in its own terminal, one click
  away.
- **J5 - Trust by construction.** No model text launches anything without the
  author's click on values from closed lists.

### 2.2 Key User Journeys

1. **Briefing.** The author opens the Orchestrator view: four subjects open,
   two sessions waiting, one approval pending; since the last reading the
   alert-rules check passed and the certificate renewal failed twice. Once the
   orchestrator exists (Slice C), it reads the same briefing, writes a recap of
   the certificate subject and proposes a `dependency-bump` work order for
   `backend`.
2. **Launch.** The work order card shows template `dependency-bump` ("bump
   {package} to its latest {level} release, run the tests, fix what breaks,
   commit"), project `backend`, `package = serde_json`, `level = minor`, the
   rendered prompt and the argument vector. One click: a background session
   appears in the `backend` workshop, filed under "dependency upkeep".
3. **Auto.** Auto approval is on, after its shadow period. A session in
   `supervision` runs `officina run apply ...`, an `auto` action with a check.
   The review sees the request, the subject and the passed check, and
   approves; the playbook runs through WSL, journaled. At night a session
   retries `apply` on a flapping host: the seventh request in the hour lands in
   Needs you with "hourly cap reached".
4. **Meeting.** A background employee sits in Your turn with a design
   question. The author clicks Receive: the question (end of its last answer),
   two commits since it started, two runs that succeeded, one refused, the
   diff. The author writes "keep the old alias, take the second option"; the
   session receives it as its next turn and turns busy, and until then the
   text stays in the panel and on the clipboard. For an editor session,
   Receive shows the same with a Focus button; the author answers in that
   terminal.
5. **Restart.** The author closes the orchestrator's terminal and starts it
   again: the same proposals are pending; the briefing covers everything since
   the previous start.

## 3. Glossary

- **Briefing** - a summary built by Officina's code from the registry, the
  board and the relay: what is open, what waits, what changed since a cursor.
- **Template** - an author-written work order model in the floor file:
  projects, prompt with placeholders, parameters with closed lists; approved
  with the catalogue.
- **Work order** - a template, a project and values validated against the
  template's lists: the only input of the launch function.
- **Card** - a work order as core issued it for display: id, template,
  project, values, SHA-256 of the rendered prompt. Any edit supersedes it.
- **Meeting room** - a side panel, not a drawn room, where the author
  receives one session: recap, changes, errors and, for background and ended
  sessions, a written answer.
- **Auto review** - a contained model call (E16), not the orchestrator
  session, answering approve, reject or escalate for one pending `auto` relay
  request.
- **Rule baseline** - the deterministic rule each shadow review is compared
  with (FR-9).
- **Orchestrator** - the one `claude` session Officina starts with only
  `officina mcp` tools (FR-10). Officina's own session, never a project's.
- **`officina mcp`** - the stdio MCP server giving the orchestrator read tools
  and proposal tools, nothing else.
- **Proposal** - untrusted output of the orchestrator, a recap or a work
  order; it acts on nothing until the author accepts it.
- **Untrusted text** - text from a model, an excerpt, a hook payload or a tool
  output; core type `Untrusted<T>`, displayable, never parsed into a command.

## 4. Features

Documented surfaces this PRD rests on, beyond E1 to E17. Each becomes an
E-entry once the check of section 8 that uses it has run. Pages are under
`https://code.claude.com/docs/en/`.

- **S1** (`cli-reference`) `--tools ""` removes every built-in tool and leaves
  MCP tools; `--strict-mcp-config` keeps only the `--mcp-config` servers (E5,
  E16); `--append-system-prompt` and an initial prompt work interactively.
- **S2** (`permission-modes`) `dontAsk` denies every call that would prompt;
  calls matching allow rules run.
- **S3** (`sessions`; gate, section 8 question 3) `claude --resume <session>
  "<prompt>"` on a running background session sends the prompt as its next
  turn, then attaches (2.1.285+). It sends nothing and exits 1 with piped or
  redirected input or output, or with flags that configure the session; it
  does not send a prompt starting with `/` or `!`, nor any prompt while the
  session waits on an answer to a question. On an ended session it loads the
  transcript, and not every launch flag is restored.
- **S4** (`agent-view`) A reply does not answer a permission prompt or
  dialog; only attaching does. `claude --resume <id> --bg "<prompt>"`
  continues in place or starts a copy and prints a `note:` line (2.1.257+).
  `--bg` is read as a flag only before a bare `--` (2.1.299). `claude --bg`
  checks workspace trust and fails with `Workspace not trusted` where no
  dialog can appear; it takes `--name` and `--model`. `claude logs <id>`
  prints a background session's recent output.
- **S5** (`cross-session-messaging`, `env-vars`) Each session binds an inbox,
  a named pipe on Windows. A script posts after an auth line carrying the
  session's `CLAUDE_CODE_MESSAGING_TOKEN`, exported to that session's hooks
  and Bash commands (E11 saw it in an MCP server's environment too). A
  delivered message is framed as another session's and cannot approve
  anything. `crossSessionInbound` (`accept`, `hold`, `refuse`) set through
  `--settings` overrides user settings. The line format after the auth line
  is not documented.
- **S6** (`channels`, `channels-reference`) A channel is an MCP server pushing
  `notifications/claude/channel`; it registers only when named in
  `--channels` (allowlisted plugins) or `--dangerously-load-development-channels`
  (interactive only, after a confirmation dialog). Research preview.
- **S7** (`env-vars`) `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` turns off `--bg`,
  `/background` and agent view for that process.

### 4.0 Slice 0: checks

#### FR-0: Gates first
The section 8 questions marked **gate** are run on the workstation and
recorded as E-entries before the stories that depend on them are written. A
failed gate changes the FR it names first, as E12 reshaped v0.2's approvals.

### 4.1 Briefing and work orders

#### FR-1: Briefing
Core builds the briefing from the registry, the board and the relay: active
and to-resume subjects with their sessions' states, Needs you and Your turn
sessions, pending approvals, triage rows, failed and refused runs, launched
work orders and their sessions' states, proposals awaiting the author (once
Slice C lands), and what changed since a cursor: the author's previous reading
of the Orchestrator view (the window records when it was last closed), or the
previous orchestrator start for the `briefing` tool (FR-11). At most 32 KiB,
oldest items dropped first with a count. The Orchestrator view shows it next to
the work order composer (FR-4); no model is involved.

#### FR-2: Templates
The floor file gains `[[template]]`: `name`, `description`, `projects` (the
catalogue projects it may run in), `prompt` with `{param}` placeholders,
`params`, and an optional `model` from a closed list. A template parameter has
`values` only; `pattern` is refused for templates. A value is at most 64
characters of `[A-Za-z0-9._:/@+-]`, with no space, and passes AD-30's rules. A
rendered prompt holds only author text and these tokens, at most 8,000
characters. Templates belong to the canonical catalogue (AD-24): a new or
changed template is usable only after the author approves the catalogue in the
window. Later schema changes of `[[template]]` (base branch, budget,
capabilities in v0.6 and v0.7) bump the hash format and void earlier
approvals: accepted. Templates are schema 2 keys; the floor schema number
does not change. A template's prompt starts with author text that is not
`/`, `!` or `-`, and its longest rendering is checked at validation. Values
pass AD-30's runner-independent rules; the native runner's refusal of a
leading `/` or `@` applies to argument vectors only, so `@scope/pkg` is a
valid value. `class = "auto"` may name a `check`, and `auto_max_per_hour` is
an action key. The floor gains `[orchestrator] model` and `[auto_review]
model`; the latter belongs to the approved catalogue.

#### FR-3: The launch path
Principle 8 holds as follows. The host's `launch` takes only core's
`WorkOrder`. `WorkOrder` has one constructor,
`WorkOrder::validate(&ApprovedTemplate, &ProjectName, &[(ParamName, &str)])`,
which accepts only values from the template's closed lists. `Untrusted<T>` and
a proposal convert to text only through `display()`, which returns a type
`validate` does not accept (`compile_fail` doctests). A proposal's values
reach a `WorkOrder` only through the same validation and click as the
author's own.

Core builds vectors; the host spawns. Every `claude` spawn (orchestrator, work
order, answer, attach, triage call, review call) goes through one host module,
whose vector kinds are an enum with a golden test each. The work order vector:
`claude --bg --name <template>-<n> [--model <m>] -- <rendered prompt>` (S4),
working directory the project folder of the approved catalogue, the floor's
`CLAUDE_CONFIG_DIR`, `NoDefaultCurrentDirectoryInExePath=1`,
`CREATE_NO_WINDOW`; refused when `claude` resolves to `.cmd` (AD-51). No
`--settings` and no permission mode: the session starts as any new session in
that folder would (S4), under the project's own settings and, when guarded,
the relay gate. Whether that session edits and runs commands without a prompt
is a gate (section 8, question 4). Officina owns no session process: `claude
--bg` returns once the supervisor holds the session (E4). The environment
drops the variables Claude Code exports to a session's children (session
identity, messaging socket and token) and keeps agent view on.

#### FR-4: One gesture
Core issues a card for every display and every edit. The card shows the
template and its description, the project, each value as a chip, the full
rendered prompt, the argument vector as one chip per argument (AD-35), the
subject it will be filed under (the project's single open subject or the
proposal's, choosable, or new), the quota gauges, and, for a proposal, its
rationale under a "proposal" label. The author may change values within their
lists; each change issues a new card. Launch acts on a pointer click only
(D-28) and sends the card id and the prompt hash; core refuses an unknown or
superseded card, a hash mismatch, or a catalogue changed since the card was
issued. The author composes work orders in the Orchestrator view without any
proposal.

The work order's name is unique (`<template>-<n>`); the session is bound by
that name in `claude agents`, and the id `--bg` printed is recorded and
checked against it; the session is filed under the chosen subject when first
seen; the journal gets `work_order_launched`
(template, project, values, argument vector, prompt SHA-256, catalogue hash,
outcome). A failure (`Workspace not trusted`, S4; any non-zero exit) shows on
the card and is never retried by itself.

### 4.2 The meeting room

#### FR-5: Receive
A Needs you or Your turn card, an employee in the scene, or a subject to
resume offers Receive. The meeting opens as a side panel in Board and Scene
modes; in the scene the received employee is marked at its own desk, one state
change under v0.4 FR-3. It shows, as text:
- decision expected: `waitingFor`, the latest `Notification` message, the end
  of the last answer;
- done and blocking: this session's relay runs (succeeded, failed with exit
  code, refused) and pending approvals, from the journal;
- the last prompt, and the latest recap proposal under its label (Slice C);
- changes: the commits since the session's `startedAt` (`git log`, D-22's
  flags, read-only), each with its files, and the working tree against
  `HEAD`; one diff covers both, capped at 256 KiB, with a jump from each file
  to its hunks and from each failed run to its journal entry;
- for a background session, the recent output of `claude logs <id>` (S4),
  display only, escape sequences stripped, capped, with a jump to the first
  line matching a fixed list of error markers.

No transcript is read (framing principle 3).

#### FR-6: Answer
For a background session or an ended one, the panel offers a composer. The
answer is the author's text, never prefilled from session or proposal text:
1 to 4,000 characters, no control character but newline and tab, no bidi or
invisible formatting character (AD-30), not starting with `/` or `!` (S3:
Claude Code would not send it). It is sent with a button or `Ctrl+Enter`,
copied to the clipboard at the same time, and kept in the panel until
delivery is confirmed or the author discards it.

| Session | Delivery |
|---|---|
| background, live, not waiting on a question or dialog | new console running `claude --resume <sessionId> -- <answer>` (S3, gate): the answer is its next turn, then the console attaches. Confirmed when the session turns busy within 30 s, else shown "not confirmed". The console keeps agent view on: with it off Claude Code sends nothing (sessions page). Its working directory is the `claude` binary's folder. |
| background, waiting on a question, a permission prompt or a dialog | Claude Code sends no prompt then (S3, S4): the answer stays copied and the attach opens (v0.1 FR-10) |
| ended (to resume) | new console in the session's folder running `claude --resume <sessionId> -- <answer>` (S3); confirmed as above |
| interactive, live | no composer: Focus only (below) |

For a live interactive session the panel shows Focus (v0.1 FR-10) and no
composer; Officina prefills and copies nothing, and the author answers in
that terminal. No documented surface delivers text to it as the author's
prompt (section 8, question 6).

The vector holds the validated session id and the answer after `--` (section
8, question 2), goes through FR-3's spawn module, refuses `.cmd` and sets
`NoDefaultCurrentDirectoryInExePath=1`. The answer sits on a process command
line, visible to local process listings while that console runs: accepted.
The journal gets `meeting_answer` (session, delivery, SHA-256 of the answer,
outcome), never the text.

### 4.3 The relay class `auto`

#### FR-7: Auto review
An `auto` action is requested and waited on like a `human` one (AD-33); when
it names a `check`, the `after-check` rule applies first. For each pending
`auto` request the window runs one auto review.

The review is a contained call, not the orchestrator session: AD-51's flags
and prompt rules (stdin, nonce-delimited data, schema, validation), the
floor's review model from a closed list (default `haiku`, E16). The
orchestrator session may be closed, busy with the author, or carry other
subjects' text; a contained call per request is schedulable, budgeted and
sees one request. Gives up: the review sees the subject's excerpts and the
journal, not a conversation.

Input: the request as core rebuilds it from the approved catalogue (D-27:
action, description, project, values, argument vector), the subject's name,
the requesting session's excerpts as untrusted data, the last 10 outcomes of
that action in that project, and the check's status. Answer: `decision`
(`approve`, `reject`, `escalate`) and a `reason` of up to 200 characters.
- `approve`, while auto approval is on (FR-9): the window records the
  approval as given by the review (authority, AD-21); `officina run` consumes
  it and runs.
- `reject`, `escalate`, or any decision while auto approval is off: the
  request becomes a Needs you card as for `human`; the session keeps waiting
  for the author.

The review never produces values or arguments: one call reviews one request
and can only answer for it. This is not a launch: the session asked, the
author's catalogue fixed the command, and the review can only let it through
or hand it to the author. `auto` sits between `free` (runs) and `human`
(author click). Reviews run one at a time within the budget (FR-8), so a queue
can outlast `officina run`'s 110 s wait (AD-33); the session is then told to
run the same command again, as for a `human` request still pending: accepted.
In live mode the request shows as reviewing, not waiting, until the review
answers; in shadow mode it is a Needs you card at once. Both contained calls
refuse messages from other sessions (`crossSessionInbound` from
`--settings`).

#### FR-8: Fail closed
While auto approval is off every `auto` request escalates, its review still
running in shadow when the causes below allow (FR-9). On or off, the request
escalates without any model call, the cause on the card and in the journal,
when: `claude` resolves to `.cmd`; the request has no session id; excerpts are
off (`no_excerpts`), so on such a floor `auto` always escalates; the latest
known 5-hour use is at or above 90 % (stale data still counts as known; no
data at all does not block); review approvals of that action, in shadow and
live modes alike, reached its `auto_max_per_hour` (default 6) in that project
within the hour; the review
budget is spent (one call per 5 s, 60 per hour, 300 per UTC day, counted as
AD-52). It also escalates when the call fails, times out (60 s) or answers
outside the schema. The approved command spends no Claude quota; the 90 %
guard only keeps the review's own calls off a nearly spent window. Whether
the orchestrator session is open plays no part. A stopped relay refuses as
today (AD-34). While the window is closed nobody reviews: the request waits
and expires like a `human` one. The session never reads the review's reason;
its refusal text is v0.2's. A malformed `[auto_review]` table escalates every
`auto` request.

#### FR-9: Blind shadow, rule baseline, on by evidence
Auto approval ships off. While off, each `auto` request is reviewed when FR-8
allows and the review is recorded, hidden until the author decides; the card
then shows it. Rejecting an `auto` request asks for a reason, "should not run"
or "not now"; only "should not run" against a review `approve` counts as a
disagreement.

Each shadow review is also compared with the rule baseline: "the check passed
within 30 minutes (for an action that names one) and the last outcome of that
action in that project succeeded". The Relay view shows how often review and
rule differ.

The toggle that lets reviews approve is offered only when, over the last 30
days, at least 20 shadow reviews met an author decision with no disagreement.
The Relay view states that 20 clean cases are weak evidence: by the rule of
three they bound the false-approve rate only to about 15 %. When the gate
holds and review and rule differ in under 5 % of the shadow cases, the window
also offers to make `auto` that rule and drop the model call. While auto
approval is on, the Relay view lists the auto-approved runs of the last 7
days, each with "should not have run"; one mark turns auto approval off,
recorded with its reason (D-69), and turning it on again needs the gate
again. Counts and rates show in the Relay view.

### 4.4 The orchestrator

#### FR-10: A session with only Officina's tools
The window starts the orchestrator on one gesture: an interactive `claude` in
a new console (`CREATE_NEW_CONSOLE`, as AD-15's attach), spawned through
FR-3's module, in the empty folder `<officina home>/orchestrator/workdir`,
with a vector fixed by core (golden test):
- `--tools ""` (S1);
- `--strict-mcp-config --mcp-config` naming `officina mcp` only (E16);
- `--setting-sources ""`: no user or project settings, hooks or servers (E16);
- `--settings` inline holding `crossSessionInbound = "refuse"` and nothing
  else: no other session's text reaches it (S5);
- `--permission-mode dontAsk` with `--allowedTools mcp__officina`: anything
  else is denied and nothing prompts (S2);
- `--disallowedTools SendMessage,ListAgents`: `dontAsk` still runs calls that
  need no approval (permission-modes page), and both tools are in the default
  set;
- `--append-system-prompt` with a constant compiled into the binary;
- `--name officina-orchestrator`, the floor's `orchestrator.model` when set
  (closed list), and a constant first prompt asking for the briefing.

Environment: AD-51's removals, the floor's `CLAUDE_CONFIG_DIR`,
`NoDefaultCurrentDirectoryInExePath=1`, and `CLAUDE_CODE_DISABLE_AGENT_VIEW=1`
(S7), so it cannot be moved to the background, where the supervisor keeps
`--settings` and `--mcp-config` but not `--tools` or `--allowedTools`
(agent-view page). Refused when `claude` resolves to `.cmd` (AD-51) or the
workdir is not empty.

Principle 2 governs the sessions that do the author's work. Officina's own
sessions (triage call, review, orchestrator) load no user or project
configuration, so their flags displace nothing. This reading was decided on
2026-10-28 under the owner's delegation of design decisions and is listed for
his confirmation. Officina writes no file under the Claude config dir (D-80).

The orchestrator is never resumed: a restart is a new session (S3: resume
does not restore every flag). It is recognised in `claude agents` by its
folder; if section 8 question 1 shows it unlisted, the window keeps the pid
from the spawn (no handle held, no job) for liveness and focus. It is drawn
in its own room (FR-13), never counted as waiting, and recorded in the
registry as Officina's, never filed or triaged. A second Start brings the live
one to the front. The MCP server is the `officina.exe` beside the window,
given the Officina home and the floor explicitly. On the first start Claude
Code asks, in the orchestrator's console, to trust its folder.

#### FR-11: `officina mcp`
A stdio MCP server in `officina.exe`, which holds no authority code (AD-21,
P-5): JSON-RPC by hand, answering `server/discover` with `-32601` and serving
the classic handshake (E11). It serves only when `CLAUDE_PROJECT_DIR` is the
orchestrator workdir (E11); elsewhere it exits with an error. The check
prevents accidents, not forgery: any process running as the author can set
that variable or write proposal files, as AD-25 accepts for approval files;
proposals stay untrusted and act only through a click. It never logs its
environment (E11, S5). It reads the registry read-only (AD-38, D-82) and the
views behind `board --json`, `subjects --json` and `relay --json`; with
`no_excerpts` it serves no excerpt.
- Read tools: `briefing` (FR-1, cursor at the previous orchestrator start),
  `subjects`, `subject`, `board`, `relay_activity`, `templates`, `proposals`
  (its own, with their status).
- Proposal tools: `propose_recap`, `propose_work_order`.

No tool runs, launches, approves, sends text to a session, reads a file or
reaches the network. Inputs are bounded in size and count; session text in
outputs is marked untrusted. Proposal tools check only the rate limits; every
other rule is checked at ingestion, so a file written directly meets the same
rules.

#### FR-12: Proposals
`officina mcp` writes each proposal as one file under
`<officina home>/orchestrator/proposals/` (temporary name, then rename, unique
id); the window ingests it into the registry (single writer, AD-38) and
validates it: the subject, session, template and project it names exist, and
text fields are within their bounds. Kinds:
- recap: session or subject; done, blocking and decision expected, up to 500
  characters each;
- work order: template, project, values, subject to file under, rationale up
  to 500. A value outside its template's list, such as a sentence, makes the
  proposal invalid at ingestion.

Every text field is `Untrusted<String>`: rendered as text under a "proposal"
label, never parsed, never part of an argument vector, a prompt or a
decision. An invalid proposal is kept, shown as invalid with its reason, and
reported back by `proposals`. Limits: 30 per hour and 200 pending, past which
the tool refuses with a reason. Work order proposals expire after 24 hours; a
valid one opens as a card (FR-4).

#### FR-13: The orchestrator's room
Under v0.4's rules (its FR-3 on motion, FR-6 on keyboard, tokens) the scene gains the
orchestrator's room in the eighth cell of the scene's grid; workshops keep
seven slots and 'More workshops' takes the seventh from the eighth project:
the orchestrator when its session is live
(state cue as for employees), or an empty chair with Start in its file, and a
neutral plaque counting pending proposals. Board mode gets a header chip
(orchestrator live or absent) and the proposals list in the Orchestrator
view. Proposals are neutral like "N to sort" (AD-55): not amber, not in the
title or the overlay. Start in the room is keyboard-reachable; Launch on a
work order is not.

#### FR-14: Restart without loss
The orchestrator keeps nothing Officina needs. Proposals and their statuses,
orchestrator starts and briefing cursors, reviews and decisions live in the
registry or the relay files. Closing and restarting the orchestrator, killing
`officina mcp` mid-write, or restarting the window loses no written proposal
and duplicates none (unique file ids, idempotent ingestion).

### 4.5 Setup

#### FR-15: Doctor
Doctor checks, as each slice lands: the `claude` version (below 2.1.285 it
warns that answers to background sessions fall back to copy and attach, S3),
the `.cmd` refusal, the templates and agent view turned off in the
environment or user settings (warn) (Slice A); the `auto` actions, the
review's state, counts and agreement with the rule (Slice B); the orchestrator
workdir, present and empty (Slice C).

## 5. Non-Goals (Explicit)

- Isolation, clones, base branches, budgets, review bundles (v0.6): a work
  order runs as the author in the project folder, like any session.
- A per-project start command (framing principle 1): work orders use
  `claude --bg` in the project folder, and the project's own settings apply.
- Launches without a gesture, pre-approved permanent templates, the attention
  budget, the governor (v0.7).
- Intervention orders and production writes outside the catalogue (v0.8).
- Orchestrator verdict proposals: its view of a subject goes in a recap.
- Orchestrator access to files, Bash, the web, other MCP servers or other
  sessions. The framing's "workspace read" is left out: a Read tool would
  carry any file's text into the one session that writes proposals.
- Typing into terminals, channels (S6) and posting to a session's inbox (S5):
  section 8, question 6.
- A drawn meeting room (after v1): the meeting is a side panel, and the scene
  marks the received employee at its desk.
- Reading transcripts; voice (after v1); several orchestrators; several floors
  at once.

## 6. MVP Scope

### 6.1.1 Delivery order
- Slice 0: FR-0, the gates of section 8.
- Slice A: FR-1 to FR-6 and FR-15's Slice A part: briefing view, templates,
  one-gesture launch, meeting room.
- Slice B: FR-7 to FR-9 and FR-15's Slice B part: `auto` review in blind
  shadow, rule baseline, toggle gated by data. Built now, not after a week of
  `human` approvals: blind shadow approves nothing, so it is itself how the
  evidence is gathered; only the toggle waits for data.
- Slice C: FR-10 to FR-14 and FR-15's Slice C part: orchestrator session,
  `officina mcp`, proposals, orchestrator room, restart. Model proposals
  arrive last.

What any slice defers is done before v0.6 starts (P-7).

## 7. Success Metrics

| Metric | Target | How measured |
|---|---|---|
| Gesture | zero launches without a pointer click | e2e injection battery through card launches (unknown card, superseded card, hash mismatch), proposal files, `officina mcp` and seeded excerpts (argument fragments, option strings, `--settings`, `/` and `!` prompts, bidi, oversized text, unknown templates, a sentence as a value, refused at ingestion, other values outside their lists, "launch now" text); a fake claude counts `--bg` spawns: 0 before the click, exactly 1 after, its vector equal to the golden rebuild; `compile_fail` doctests; a mutation of the launch path that accepts unvalidated text turns a test red |
| Restart | zero proposal lost or duplicated | e2e with a scripted MCP client: `officina mcp` killed mid-write, window restart, orchestrator restart |
| Containment | the orchestrator holds only `officina mcp` tools; no other session's message reaches it | golden vector; print-mode check recorded (addendum section 1, item 7); interactive check recorded (section 8, question 1) |
| Auto fail-closed | escalation under each FR-8 cause, 100 %; every auto-approved run's vector equals the catalogue rebuild; no shadow review shown before the author decides | e2e with fake claude review modes: approve, reject, escalate, invalid, timeout |
| Delivery check | an answer reaches a live background session as its next turn | gate recorded (section 8, question 3); e2e on the console spawn's vector |
| Zero write | Officina writes nothing under the Claude config dir | fingerprint test extended to `officina mcp`, a launch, an answer, a review (D-80) |
| Use | 10 working days with at least one work order launched or one meeting answer sent per day | journal |
| Auto | `human` plus `auto` clicks per day, in the week before Slice B ships and in the last week of shadow | Relay view, from the journal |
| Delivery share | share of meeting answers delivered as a turn versus copied | journal `meeting_answer` lines |

The v0.5 close reads Use, Auto and Delivery share (P-16).

## 8. Open Questions

1. **Gate (FR-10).** Does the flag set hold as a whole: only
   `mcp__officina__*` tools listed, `ListAgents` and `SendMessage` absent,
   every Officina tool called without a prompt under `dontAsk` with
   `--allowedTools mcp__officina`, `crossSessionInbound` from `--settings` in
   force alongside `--setting-sources ""`, and the session still listed by
   `claude agents --json` with `CLAUDE_CODE_DISABLE_AGENT_VIEW=1`? Check: start
   it, run `/mcp`, `/context` (tool list), `/status` and `/config` (the
   inbound row is absent while `--settings` sets the key; `/status` shows
   nothing for a refusing session), ask it to read a file, run a command and
   have another session message it a unique token, which never reaches its
   transcript; record. If it is not listed, the window keeps the
   pid from the spawn (no handle held, no job) and uses it for liveness and
   focus.
2. **Gate (FR-3, FR-6, FR-10).** Does `claude` take a bare `--` before the
   prompt in interactive mode, with `--bg` and with `--resume`, for a prompt
   holding newlines, quotes and a leading `-`? S4 implies it for `--bg` only.
   Check in a scratch folder. If not, the vectors drop `--`, and FR-2 and FR-6
   refuse a leading `-`. Print mode is verified (addendum section 1, items 4
   and 5); `--bg` and interactive resume remain.
3. **Gate (FR-6).** Does S3 hold on the workstation: `claude --resume
   <sessionId> -- <answer>` from a new console on a live idle background
   session sends the answer as its next turn and attaches, and sends nothing
   while the session waits on a question? E14 saw `--bg --resume` on a running
   session start a copy, so resume on live sessions has surprised once. Check
   on a scratch background session, with `claude agents --json` before and
   after. If S3 does not hold, every background answer is copied with attach.
4. **Gate (FR-3, J2).** Does a `claude --bg` started with FR-3's vector, once
   in a free project and once in a guarded one, edit a file and run a
   non-guarded command without prompting? Nothing in E1 to E17 says whether a
   settings `defaultMode` reaches a `--bg` session. If not, J2 is limited to
   tasks that need no tool permission, and the PRD says so; no permission mode
   is added to the vector (principle 2).
5. Answered: `claude --bg --resume` on a running session starts a copy
   (`claude --help`, 2.1.312). FR-6 does not use it.
6. Text to a live interactive session. No documented surface delivers it as
   the author's prompt: the inbox (S5) has an undocumented line format, needs
   on Windows a per-session token that only that session's children receive,
   and frames the text as another session's, which cannot approve anything;
   channels (S6) need the session started with a channel flag. Focus only
   until Claude Code documents an author-input surface; recheck at each new
   version the canary sees.
7. Does `claude logs <id>` print plain text without a TTY, and how fast?
   Check; if not, FR-5 drops it.
8. The values 6 per hour, 90 % of the 5-hour window, 20 shadow cases and a 5 %
   rule difference are starting values, measured over the shadow period and
   two weeks of use.
