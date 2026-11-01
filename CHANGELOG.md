# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- A work order whose session worked in another folder than its project's,
  such as a git worktree Claude made under `.claude/worktrees/`, names
  that folder on its card and in the history, so you know where its
  changes are: on that worktree's branch, not in the project's working
  tree. Merging and removing the worktree stays yours until v0.7's merged
  detection.

- A project's own permission mode for work orders, or Officina's default
  (story 5.24): a work order runs in the permission mode its project's
  settings set, else your default, as gate 4 found it does, and one that
  stops at a permission prompt waits in Needs you like any session. The
  card's Permissions line says which mode applies; its tooltip, shown on
  hover or when Tab reaches its button, offers "Use Officina's default for
  this project", which runs that project's work orders in Claude Code's
  auto mode (`--permission-mode auto`), never `bypassPermissions`, on a
  pointer click only; "Back to the project's mode" undoes it. The choice
  is Officina's own state (`relay/work-order-permissions.json`, written by
  the window only), never Claude Code's settings nor the floor, held per
  project and folder, journaled as `work_order_permission_set`, and named
  by `officina doctor`; a card issued before a switch is refused
  `permission_changed` at Launch. The README also states that background
  sessions you start while a service a work order started runs get
  Officina's environment.

- Auto-mode denials (v0.5.x, story 5.23): when Claude Code's auto mode
  refuses a tool call, its `PermissionDenied` hook makes `officina hook`
  journal `auto_mode_denied` with the session, the tool and the
  classifier's reason (200 characters at most, its rule name only with
  excerpts off), never the call's input,
  and print nothing, so Officina never asks for a retry. The Relay view
  lists each denial with its reason as text; a session's file in the scene
  counts its denials of the last 24 hours in neutral text, never in the
  title, the taskbar, the alert or the attention colour; `officina relay
  --json` prints the same. Doctor's hooks block registers
  `PermissionDenied`: paste it again.

- The permission router's hook path (v0.5.x, story 5.22; internal,
  nothing changes for you yet): `officina hook` on a `PermissionRequest`
  gives Claude Code its own prompt at once unless a window that routes
  prompts runs, which none does until the routing story; with one, it
  would write the request, wait for a decision only the window's authority
  writes, bound to the session and the input's hash and used once, and
  print only `allow` or `deny`, never anything that changes Claude Code's
  settings, the call or the session. The window keeps a liveness marker,
  `window/alive.json`, refreshed every 5 s and removed when it closes.
  Doctor does not register `PermissionRequest` yet.

- The orchestrator (v0.5): work orders started from templates you wrote
  and approved, a meeting where you receive a session and answer it in
  writing, an `auto` class reviewed in blind shadow before it may approve,
  and a restricted orchestrator session that reads what Officina records
  and proposes recaps, work orders and answers, none of which acts before
  your click. The entries below follow that order.
- Templates in the floor file (v0.5, story 5.4): `[[template]]` declares a
  prompt with placeholders, the catalogue projects it may run in, a closed
  `values` list per parameter (never a `pattern`) and an optional model.
  Each rule (name, description, projects, braces, placeholders, characters,
  the first character after leading whitespace, the longest rendering,
  values, model) is an `IssueKind`; a failing template is skipped and
  reported with its line and table, never invalidating the floor.
  Templates belong to the approved catalogue: the Relay view lists them,
  the change banner shows them added, removed or changed. `[orchestrator]`
  and `[auto_review]` tables each take an optional `model`, a failing
  table reported; `[auto_review]` belongs to the approved catalogue (its
  model defaults to haiku), `[orchestrator]` does not. `class = "auto"`
  validates and must name its `check` (`auto_without_check` otherwise), so
  an unattended apply always runs after its dry run; `auto_max_per_hour`
  (1 to 60, default 6) applies to `auto` actions only; an HTTP action
  cannot be `auto`. `officina doctor` names each issue's table (`template
  bump`, `action apply`), counts valid templates, and lists templates,
  credentials and the review model in a pending change.
- Work orders (v0.5, stories 5.5 to 5.7 and 5.9): a template is read from
  the approved snapshot only while the floor still hashes to it
  (`TemplateUnavailable`), and `WorkOrder::validate` is the only way to an
  order, refusing a project the template does not list, a missing,
  repeated or undeclared parameter, a value that is not byte for byte one
  of its list's, and a rendering over the prompt limit or starting with
  `/`, `!` or `-` (`WorkOrderRefusal`); the order keeps the template's own
  strings. Its vector, `claude --bg --name <template>-<seq> [--model
  <alias>] -- <prompt>`, runs in the project's folder as declared, the
  prompt one argument after `--`, as a session kind, in no job, its
  deadline ending `claude` alone; `tests/vectors/orders.json` and a
  `work_order` case of `tests/vectors/spawn.json` pin it.
- Cards and the one click (v0.5, stories 5.7 and 5.9): in the Orchestrator
  view, the composer offers the approved templates with their
  descriptions, the template's projects and one list per parameter holding
  exactly its values, with no field to type a value, and a subject picker
  with the project's open subjects and a new name. Each choice issues or
  edits a card through core: the rendered prompt, its SHA-256, the
  argument vector exactly as it would run under a name reserved at issue,
  each value and argument as a chip, the folder, the subject and the quota
  gauges, never in the attention colour; an edit issues a new card under
  the same name and supersedes the old. **Launch** acts on a pointer click
  only: the collector checks the card in a fixed order (`LaunchRefusal`),
  consumes it, records the row and hands the card's vector, byte for byte,
  to the click's own thread, which runs `claude --bg` and sends the outcome
  back (`LaunchOutcome`, an untrusted folder its own), recorded however
  late and journaled once as `work_order_launched`. The card then shows the
  outcome. Nothing retries: a double click launches once, and a failed
  launch burns its card. Commands `issue_card`, `edit_card` and
  `launch_work_order`.
- Work order records (v0.5, story 5.6): a launch is recorded `launching`
  under its unique name `<template>-<seq>` (a name the registry holds is
  refused `name_taken`), then its outcome, with what `claude --bg` printed
  (4 KiB) and its error (500 characters) kept as untrusted text; a row left
  `launching` two minutes reads `unknown`. A `started` order binds, within
  ten minutes, to the background session listed under its name in its
  project folder, and that session is filed under the order's subject with
  source `author`, ahead of the filing rules; a new subject is created at
  `started`, never at the click, or the open subject of that name is used.
  Rows are kept 90 days, their texts cleared with the excerpts.
- The Orchestrator view and its briefing (v0.5, story 5.8): a fourth view
  in the header's switch, with a neutral presence mark. The briefing,
  built by Officina's own code from the board, the relay view and the
  registry, lists the open subjects with their sessions' states, the Needs
  you and Your turn sessions, the pending approvals, the rows to sort, the
  failed and refused runs and the launched work orders, and what changed
  since you last read it (`ChangeKind`). It is at most 32 KiB: the oldest
  items go first and each section says how many. Leaving the view or
  closing the window records the reading. The view also lists the approved
  templates, those awaiting approval and the work order history with each
  order's outcome, binding, "unbound" and "printed id differs" flags and
  its session's state. Commands `get_orchestrator` and
  `mark_briefing_read`, event `orchestrator://updated`.
- The meeting (v0.5, story 5.10): Receive on a Needs you or Your turn card,
  a background session idle in Done, the session file or a subject to
  resume opens a side panel in the file's place, which resizes nothing:
  what the session expects (waiting for, Claude Code's latest
  notification, the end of its last answer, your last prompt), its relay
  runs since it started and its pending approvals, the commits since it
  started with one diff to the working tree and its untracked files, and
  for a background session the end of `claude logs` with its escape
  sequences stripped; then how an answer would reach it (`Delivery`). The
  scene rings the received employee in the working colour. `git` runs
  read-only, in the folder `claude agents` reported only, running nothing
  a repository configures; no transcript is read.
- Written answers from the meeting (v0.5, story 5.11): a composer, never
  prefilled, sends with Send or Ctrl+Enter and copies the text on the same
  gesture. A session whose answer goes as its next turn gets it through
  `claude --resume` in its own console, in the folder `claude agents`
  reported for it, confirmed when it turns busy within 30 s; otherwise the
  text is copied, with `claude attach` opened while the session waits on a
  question, and an interactive session gets Focus. The panel keeps the
  text until it is confirmed or discarded; the journal keeps its SHA-256
  (`meeting_answer`), never the text. `officina doctor` warns when
  `claude` is too old to send a resumed prompt.
- The `auto` class runs (v0.5, story 5.12): `officina run` requests and
  waits for an `auto` action's approval exactly as for a `human` one; just
  before an approval is used, the check is evaluated again on the files as
  they are then and refuses `check_not_satisfied`, the approval left
  unused, when it no longer holds. Approval files and `approval_given`
  lines say who approved (`by`); rejecting an `auto` request asks "should
  not run" or "not now", kept in the rejection file and its line. One
  decision per request: a `.decided` marker created first refuses a second
  decider `already_decided`. The approval card and the session file show
  the class; `run --list` shows every action's class.
- The review of `auto` requests, in blind shadow (v0.5, story 5.13): each
  new `auto` request is prechecked and either goes to you at once with its
  cause (`EscalationCause`: the setup, the request, the limits) or is
  reviewed by a contained call on a `review` thread of its own, paced 5 s
  apart from each call's start, the cap and budget checked again right
  before each call. The call sees the catalogue's rebuild of the request,
  the subject, your last prompt, the last ten runs and the check's status,
  never the session's answers, and can only answer approve, reject or
  escalate. With auto approval off the card says so at once, and the
  review's answer stays hidden until you decide and writes no approval.
  Each review is an `auto_review` row with the rule baseline and your
  decision copied onto it, journaled `auto_review` and `auto_escalated`
  (never the reason), kept 90 days, its reason cleared with the excerpts.
  The triage call also turns the advisor tool off
  (`CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1`).
- Letting reviews approve (v0.5, story 5.14): the Relay view's Auto
  approval section counts the shadow reviews met by your decision over 30
  days, the disagreements and how often review and rule differ, and states
  the 3/n bound on wrong approvals. Turning auto approval on is offered
  only while the gate holds (20 cases, no disagreement), on a pointer
  click, and the writer refuses it when the gate broke since display. Live
  mode: a request is `reviewing` (no buttons, not waiting) until its review
  answers; an approve is recorded by the window's authority (`by =
  "review"`, the review's id), the hourly cap checked once more, your
  decision winning. Rule mode, offered while review and rule differ in
  under 5 % of the cases: no call, a true baseline approves `by = "rule"`,
  a false one escalates `rule_baseline_false`. "Should not have run" on an
  auto-approved run of the last 7 days turns approval off and the mode
  back to review, and counts as a disagreement for 30 days. Each change is
  journaled `auto_approval_set`; an approval the authority refuses
  escalates `approval_refused`. `officina relay --json` prints the section
  and each decision's review without any reason.
- Permission modes in doctor (v0.5, story 5.20): the hook records each
  session's `permission_mode` on its event lines (cut to 32 characters;
  the canary watches it on `UserPromptSubmit`), and doctor prints a
  "permission mode" line counting the sessions of the last 24 h in each
  mode, interactive or background, an undocumented value named unknown.
  `officina doctor --permissions` prints the user settings' whole
  `permissions` key with `defaultMode` set to `"auto"` and the whole
  `autoMode` key (Claude Code's defaults and two placeholders when there is
  none), every entry kept, each also written to `doctor/`; switching is
  your decision and no settings file is written. Gate 8 passed without a
  rule, so nothing is added for the relay. Plain doctor reads neither key
  and removes the two files.
- `officina mcp` (v0.5, stories 5.15 and 5.21): the orchestrator's MCP
  server on stdio, newline-delimited JSON-RPC answered by hand, a line over
  1 MiB discarded and refused. It serves only in
  `<officina home>/orchestrator/workdir` (else one line, exit 1). Read
  tools, read-only with the window open or closed: the briefing (cursor:
  the previous orchestrator start), the subjects, one subject, the board,
  the relay's recent activity, the approved templates and the proposals
  of the last seven days. Proposal tools (`propose_recap`,
  `propose_work_order`, `propose_answer`) write one file each under
  `orchestrator/proposals/`, bounded and limited per hour and in pending
  count (`PROPOSALS_PER_HOUR`, `PROPOSALS_PENDING_MAX`), and validate
  nothing else. Every answer starts with a note and marks every session
  text `{"untrusted": "..."}`; no excerpt with `no_excerpts`; answers at
  most 64 KiB. No tool runs, launches, approves, sends text, reads a path
  or touches a permission prompt; its `claude agents` child receives none
  of the orchestrator's session variables.
- The orchestrator session (v0.5, story 5.16): **Start** in the
  Orchestrator view opens `claude` in a console of its own, in
  `<officina home>/orchestrator/workdir`, with no built-in tool, Officina's
  MCP server only (run from the `officina.exe` beside the window), no
  settings source, inbound messages refused, `dontAsk` with Officina's
  tools allowed and both messaging tools denied, its role appended and the
  name `officina-orchestrator`; the contained allow list's variables only,
  agent view and the advisor tool off. No first prompt: the role asks for
  the briefing before the first answer. Refused, with the cause shown and
  recorded, for a `claude` that is not `.exe`, a non-empty workdir, a
  missing `officina.exe` or an unwritable registry. Each Start is an
  `orchestrator_start` row (pid and creation time kept, read back at window
  start, deleted after 90 days) and an `orchestrator_started` journal line;
  never a resume. The agents entry named `officina-orchestrator` in its
  workdir is `Board.orchestrator` and the view's presence, never a card; a
  second Start while it runs brings its console to the front by title. Its
  lines and its agents entry are never filed.
- The orchestrator's proposals in the window (v0.5, story 5.17): the
  window reads `orchestrator/proposals/` (a name that is no proposal's is
  deleted and counted, a file read at most `PROPOSAL_FILE_MAX_BYTES` plus a
  byte, the hourly and pending limits held again by deletion, a temporary
  file never read and deleted after a day), validates each against the
  approved catalogue and the registry (`orders::proposal::validate`),
  stores it once by its id and deletes it. A proposal is `pending`,
  `invalid` with its reason (`ProposalInvalidReason`), `accepted`,
  `dismissed`, `expired` (a work order proposal a day after its receipt)
  or `superseded` (a newer recap on the same target); recaps do not
  expire. The Orchestrator view lists them under a "proposal" label, texts
  as text, with Open as card for a valid work order proposal (the same
  validation and the same click as your own cards; a proposed new
  subject's name is a suggestion beside the picker, never the default) and
  Dismiss; launching such a card accepts the proposal. The briefing lists
  the pending ones and counts those received. Rows go after 90 days, their
  texts with the excerpts. The meeting shows the latest pending recap for
  the received session or its subject after its runs, under the same
  label, never copied into the answer composer.
- Answer proposals (v0.5, story 5.21): the window stores a proposed answer
  `pending` only for a background session live and idle in its turn whose
  answer goes as its next turn, its text passing the composer's own rules
  and its rationale given; otherwise `invalid` with its reason. It shows
  in that session's meeting beside your composer, under the "proposal"
  label and as text, never copied into it; **Send the proposed answer**
  acts on a pointer click only and sends the proposal's id, the window
  sending the text it stored, copied to the clipboard on the same click and
  confirmed as your own answers are; nothing else ever sends it. It
  expires on the first board where its session moved on, a newer one for
  the same session supersedes it, and a sent one is `accepted`. The
  Orchestrator view lists them with their session and an Open meeting
  button; `meeting_answer` lines carry `source` (`author` or `proposal`)
  and the proposal's id, never the text.
- The orchestrator in the scene's decision office (v0.5, story 5.18):
  while `claude agents` lists it, it sits at the decider's desk in its own
  tunic (`--orch-tunic`, both themes), at its desk while busy, facing the
  room otherwise, never ringed, numbered, counted or grouped; the plaque
  reads "Orchestrator" while it is live and "Decisions: you" otherwise. Its
  screens show the pending proposals and the Needs you count, the
  whiteboard the latest pending proposals' labels, as text, redrawn only
  when they change. Its button opens the Orchestrator view; the empty
  chair's opens the orchestrator's file with Start, reachable from the
  keyboard. A work order bound while it is live is briefed once at the
  reception counter: the employee and the orchestrator meet there for
  `BRIEFING_DWELL_MS`, then go to their places, never with walking off,
  reduced motion or a hidden window. The view switch's Orchestrator button
  carries the pending count beside its presence mark, in its name and
  tooltip too; neither reaches the title, the taskbar badge or the
  attention colour.
- `officina doctor`'s Orchestrator section (v0.5, story 5.19): the
  orchestrator's checks, from the work orders' setup to the proposals,
  print together, last, in one stable order (`ORCHESTRATOR_SECTION`), each
  once; a template's issue and the templates awaiting approval join them.
  A non-`.exe` `claude`, a non-empty workdir and a missing `officina.exe`
  fail; the others warn or inform, `review_config_invalid` included, since
  every `auto` request then comes to you.
- The use metrics (v0.5, story 5.19): `orders::metrics::per_day` counts,
  per UTC day of the relay journal, the work orders started, the answers
  sent and confirmed (yours and the proposals' apart) and your decisions
  beside the review's and the rule's; `cargo run -p officina-host
  --example use_metrics` prints them from the journal as `officina relay
  --json` reads it, with the Use and Delivery share figures of PRD
  section 7.

- Sub-agents in the scene (v0.4, story 4.12): each running sub-agent of a
  session stands beside its desk, a smaller figure without a cap, from its
  `SubagentStart` to its `SubagentStop`, three at most, wherever its lead
  stands; it fades in and out, and a resumed sub-agent is the same figure
  coming back. The session's file lists every running sub-agent by type
  and its name counts them; `board --json` cards carry `subagents`.
- Moving an ended card (v0.4, story 4.15): a To resume or Done card can be
  dragged onto Done, back onto To resume, or onto a Discard target shown
  while it is dragged, for a session that was never real work; its menu
  does the same from the keyboard. Each is the author's verdict, recorded
  in the registry and covering only what the session did before it; a note
  offers Undo. Discarded sessions leave the board and the scene and are
  listed, with Restore, beside the archived subjects.
- The window's minimum size, 1140 x 820: the header on two lines and the
  scene's base floor whole at its smallest scale (story 4.15).
- Short fades and slides (about 160 ms) when views, modes, cards, menus,
  panels, the alert band and the header's chips come and go; none under
  reduced motion (story 4.15).

- Scene mode (v0.4, in progress): the Sessions view draws the company,
  furnished, scaled to the window, every live session an employee placed
  at once at its place (reception, the decision office's queue with its
  register, its project's office, the lounge, the alert room, More
  offices), its halo breathing two minutes while it waits; nothing in it
  moves or reads the clock between payloads. `m` and a header radio switch
  between Board, still the default, and Scene; the choice is kept. In Scene
  mode the header keeps to one line, so eight projects fit a maximised
  Full HD window. The scene's light materials, figure and people colours
  join the tokens.
- Walking in the scene (v0.4, in progress), on by default: an employee
  whose place changes walks there along the corridors, with a walk cycle,
  and sits down at a seat; its halo appears when it arrives. A new session
  fades in at the entrance and walks in; an ended one walks out and fades.
  A "Walking" toggle in the header, kept for the next start, turns it off:
  employees then glide to their places. Opening the scene, a window brought
  back from behind another, and the system's reduced motion place everyone
  at once; nothing runs once everyone has arrived. Walking sits at the
  scene's CPU budget of 1 % of the machine on a deliberately harsh replay,
  a cost the owner accepted.
- Focus mode (v0.4, in progress): a third mode of the Sessions view, and
  `f`, hide the board and the scene and list only what waits for you: the
  approvals waiting (opened in Board mode), the Needs you sessions,
  numbered for the digit keys and marked when in alert, the Your turn
  sessions, and the Working count. `f` again brings back the mode you had;
  the choice is kept for the next start. Nothing in Focus moves.
- Acting from the scene (v0.4, in progress): every employee is a button
  with its name on hover and keyboard focus. It opens the session's file,
  a side panel that resizes nothing: project and priority, number, place,
  state and what it waits on, since when (or "since unknown"), the alert's
  cause ("last known" on unknown data), subject, last prompt and answer,
  a pending relay request with "Open in Board", and the board card's
  actions with the same outcomes, Bring to front focused. Escape closes it
  and returns to the employee. Tab walks the rooms in order, arrows move
  within a room, digits 1 to 9 bring Needs you sessions to front as on the
  board; the register, an office's chip, a full room and More offices list
  their sessions, each opening its file. A line in the scene's empty
  corner counts the sessions to resume and done, each opening Board mode.
- The alert (v0.4, in progress): a band under the header, in every view,
  names the first session in alert, its project, what it waits on and why
  (a high-priority project, or waiting N minutes), and how many more; no
  dismiss, no toast, no theme change. The taskbar badge turns to the alert
  colour with an exclamation mark instead of the count, so the alert does
  not rest on colour alone, and the button flashes once per session newly
  in alert; the title
  adds the alert count, and "data unknown" while an alert is the last one
  known. `--color-alert` and `--color-on-alert` join the tokens.
- The alert room (v0.4, in progress): while someone stands in it, its floor,
  hazard outline, partitions, status board and plaque take the alert
  colour, the beacon is lit with a static beam, and its glow breathes for
  two minutes then rests (steady under reduced motion); a halo in the room
  takes the alert colour. An office whose project is marked high priority
  carries a "high priority" pill above its plaque.
- Floor keys of the scene (v0.4, in progress): `[[project]] priority`
  (`normal` by default, or `high`) and `[alert] after_minutes` (1 to 1440,
  10 by default), valid in either schema. They authorise nothing, so they
  stay out of the relay's canonical catalogue; a bad value keeps the
  default and `officina doctor` warns with the key and its line. The
  project colour `plum` joins the palette, so a project without a colour
  may get it.
- Dark theme (v0.4, in progress): the whole window, board and scene alike,
  follows the system's dark or light theme, and a header control cycles
  Auto, Light and Dark, kept for the next start and applied before the
  window draws. The header logo follows the theme. An alert never changes
  it. The colours the error banner, the broken-contract text, the column
  washes and the session file's shadow hard-coded are tokens now. The
  "relay stopped" chip takes the error banner's deep red, so its label
  stays legible in both themes.
- `officina hook`: records Claude Code hook events into a local, append-only
  buffer, always exits 0, p95 around 20 ms.
- `officina quota write [--tee]`: records the account's rate limits from a
  statusline payload, one file per Claude config dir.
- `officina board --json`: the board computed once, for scripting.
- `officina doctor`: checks the setup and prints the hooks block and the
  statusline wiring to paste; never writes to the Claude config dir.
- Board window (`officina-app`): Needs you, Your turn, Working, To resume and
  Done columns, quota gauges with reset times and freshness, stale and
  diagnostics banners, keyboard shortcuts, pin on top, single instance.
- Focus: one click brings an interactive session's editor window to the
  front, or opens `claude attach` for a background session, whose command
  can also be copied, floor config dir included; ambiguous windows are
  listed, never guessed.
- Optional floor file (`officina.toml`): project names, colours, a
  dedicated Claude config dir, and `no_excerpts`, which the hook honours too.
- Relay catalogue in the floor file (schema 2): projects declare `autonomy`
  (`free` or `guarded`) and the `actions` they may use; `[[action]]`
  entries declare a closed command (`runner` `native` or `wsl` with an
  optional `distro`, an absolute `program`, literal `env`, `args` with at
  most one placeholder each, parameters limited to `values` or an anchored
  `pattern`, a `class`, `timeout_s`, `max_output_kib`). An invalid action is
  skipped with its line and never invalidates the floor for the board.
- Catalogue approval in the window: the catalogue is reduced to its meaning
  and hashed; an amber "catalogue change" chip and a banner show the change
  per action and per field (what an added action runs, a changed parameter
  by name), the projects and guarded programs that changed and the actions
  the floor skips with their lines; **Approve this catalogue** approves
  exactly what is shown, on a pointer click only, and says so when the floor
  changed again in between.
- `officina run <action> name=value ...`: runs an action of the approved
  catalogue from its project's folder, `free` or `guarded`, after checking,
  in order, that the relay is not stopped, that the catalogue still is the
  approved one, the project, the action and every parameter; the program
  starts as an argument vector with its timeout and an output budget, and it
  and every process it starts end together. Exit codes are the program's
  own, or 126 refused, 124 timed out, 127 not started, 64 usage.
- `officina run --list`: the actions of the current folder's project with
  what each parameter accepts, the check an action waits for, how a `human`
  action waits for its approval, and whether the relay is stopped or a
  catalogue change awaits approval.
- `human` actions: `officina run` writes a request and waits for the
  author's decision in the window (`--wait <s>`, 110 by default, at most
  540); approved, the action runs at once, once; rejected, the same request
  is refused for 15 minutes; with no decision in time the call says to run
  the same command again, which waits on the same request. Approvals are
  used within 15 minutes, requests expire after 60. Each request and each
  decision is one journal line, and the run that uses an approval carries
  its id.
- Approval cards in Needs you, first and oldest first: the project, the
  action, the exact command as one chip per argument, the session that
  asked, the age and expiry, and Approve and Reject on a pointer click only.
  The title, the taskbar dot and the waiting pill count them.
- Dry runs: a `human` action may name a `check`; its request is made only
  after that check succeeded on the same code, records the check run and
  the tree, and its card shows the check's kept output with "tracked and
  unignored files unchanged since the dry run" while the fingerprint
  matches. Every check run keeps its output (256 KiB at most) for 7 days.
- `after-check` actions: one runs only after its check succeeded in the same
  project with the same parameters, under the same approved catalogue and on
  the same code, in the last 30 minutes, the journal of the day before
  included; otherwise `officina run` refuses with `check_required` and names
  the check. In a git work tree the code is bound by the tree fingerprint
  (see Security): the content of the tracked and unignored files, read and
  hashed by Officina.
- Relay gate in `officina hook`: in a guarded repository, Claude Code's
  `PreToolUse` for a shell tool gets a deny that holds in every permission mode,
  bypass included, for a guarded program (remote shells and copies, Ansible,
  WSL and the Windows distro launchers), even through quotes, wrappers,
  `bash -c`, interpreter one-liners and command substitutions, for a command
  name built at run time, a PowerShell encoded command, a redirection of
  Officina's home, and any mention of an `OFFICINA_` setting or of
  Officina's own files; the reason lists the actions the session may run
  instead. Elsewhere it prints nothing. It costs a few milliseconds per
  shell tool call. Its limits as a text match are recorded in the
  architecture, and a test checks its lexer against Git Bash's own parse of
  every command of its vector set and of variants built from its constants.
- Stop and resume the relay from the window's header: while stopped,
  `officina run` refuses every action and a terracotta "relay stopped" chip
  and a board banner say so; the gate keeps guarding.
- Relay journal (`~/.officina/journal/YYYY-MM-DD.jsonl`): every run start
  and end (exit code, duration, byte counts and SHA-256 of each stream, never
  the output), refusal, gate deny or fault, catalogue approval, stop and
  resume leaves one line, written under a lock file so lines never
  interleave; command excerpts follow `no_excerpts`.
- `officina relay --json`: the relay's state once, read only and the same
  contract the window reads: the stop flag, the approved catalogue, a
  pending change with its diff, the floor's catalogue issues with their
  lines, the last 50 journal entries with each run's start and end paired
  (abandoned when no end came 60 s past its timeout), the unreadable journal
  lines and the gate errors of the last 24 hours. Exit 0, or 1 when the
  Officina home cannot be read.
- `officina doctor` checks the relay once a floor uses it: the relay floor
  and whether it differs from the floor doctor reads, the catalogue's issues
  with their lines, the approval state, the projects with their actions and
  the guarded programs in effect, whether each action's program resolves (a
  WSL action's tools and program probed inside its distro), whether each
  project's files bind, the decisions the window did not make, and warnings
  for a timeout that can outlive the Bash tool's cap, an action naming a
  check its project does not list, a project with actions that bind files
  outside a git work tree, unreadable journal lines and gate errors of the
  last 24 hours. For each guarded project it prints the `CLAUDE.md`
  paragraph to paste, generated from the approved catalogue.
- HTTP read actions (`kind = "http"`, `GET` or `HEAD`) on a URL template
  whose placeholders fill a whole path segment or query value,
  percent-encoded; no redirect, no proxy, a bounded wait and a body cut at
  `max_output_kib`; TLS trusts the Windows certificate store. Exit 0 for a
  2xx status, 1 for any other, 124 on timeout, 127 without a response; the
  journal records the method, the URL, the status and the body's hash.
- Credentials (`[[credential]]`: name, hosts, header, scheme) whose secret
  lives in the Windows Credential Manager as `officina:<name>`, read just
  before the request and sent only to the hosts bound to it; `officina
  doctor` checks each one and prints the `cmdkey` line that stores a missing
  one, and the Relay view lists them with their presence.
- Relay view in the window, behind a Board/Relay switch in the header once
  the relay is in use: the stop toggle, a pending catalogue change with
  Approve, the floor's issues with their lines, the approved catalogue
  (class, projects, command, parameters and limits per action), the recent
  activity (time, project, action, parameters, outcome, duration, abandoned
  runs marked), and the unreadable journal lines and gate errors of the last
  24 hours, all rendered as text.
- The window follows floor edits without a restart: project names and
  colours, a moved config dir (sessions, quota, attach) and the relay's
  state, which it also rebuilds when the relay's files or today's journal
  change.
- Registry (v0.3): a local SQLite database at
  `~/.officina/registry/registry.db` that only the window writes and every
  command line reads read-only, with the window open or closed. It is never
  opened on a network folder; in a folder a sync client manages it opens
  and `officina doctor` names the folder to exclude. At each open the
  window checks its integrity: a damaged database is set aside under
  `registry/quarantine/` and replaced by the newest valid daily copy
  (`registry/backup/`, two kept), and a database written by an older
  version is copied, then migrated. `officina doctor` prints the registry,
  the canary's hook fields and the triage model as one section in a stable
  order, and never creates the registry.
- Session records (v0.3): the window records each session once, whichever
  of `claude agents` and the hook sees it first, with each fact's latest
  value, its source (`agents`, `hook`, `rules`) and its time. A refresh of
  `claude agents` writes only what changed and never stores `waitingFor`. A
  session gone from `claude agents` without a `SessionEnd`, also while the
  window was closed, is `interrupted`; a `SessionEnd` always wins over it,
  and a session listed again or sent a newer prompt is no longer ended.
  `/clear` and `/resume` in the same process link the session to its
  successor, and a run never seen in `claude agents` that ended with
  `other` is recorded as non-interactive.
- Subjects (v0.3): a new session joins its project's subject when that
  project has exactly one active subject, whichever source sees it first;
  otherwise it stays unfiled, and no rule creates a subject. The window's
  writer creates, renames, merges, archives (with the routine box) and
  reopens subjects and files or unfiles sessions, each refusal with its
  reason and each change recorded as the author's with its time. A session
  alive again wakes its `to_resume` or archived subject. A subject keeps
  only its latest excerpts, and archiving clears them from the database and
  its log.
- Subjects view in the window (v0.3), beside Board and Relay: the active
  then `to_resume` subjects with their sessions' live state, and for a
  subject with no live session its last prompt and the end of its last
  answer; the unfiled sessions by project, non-interactive runs counted and
  collapsed; the archived subjects on demand. Create a subject (from
  selected sessions, which give it their project, or in a project the
  registry knows), rename, file, unfile, merge, archive with the "routine,
  could have run alone" box and reopen, each in one gesture; a refused
  action says which rule refused it and changes nothing. Notes name the
  folder to exclude from a sync client, the copy a damaged registry was
  restored from (for 7 days), or why the registry cannot be read. Digit
  keys focus cards in the Board view only. Board cards carry their subject
  (`subject`, in the board contract's schema 3).
- `officina subjects --json` (v0.3): the Subjects view printed once,
  read-only and with the window closed: subjects and sessions joined to the
  live board, the unfiled sessions per project, the rows that need a
  verdict, the measured error, the gates, the model's state and the
  ingestion's lag; exit 3 with the reason when the registry cannot be read.
- What needs a verdict (v0.3): one row per `active` subject whose sessions
  have all ended, never one per session, anchored on its latest session
  that saw a prompt; one per ended, unfiled, interactive session that saw a
  prompt and was not replaced by `/clear` or `/resume`, with the subject to
  file it under when its project has only one open; and "empty subject:
  archive?" for an `active` subject none of whose sessions saw a prompt.
  Each row carries one proposal, `finished`, `to_resume` or `unsure`, with
  its source (`state`, `rules` or `model`) and a reason code, computed again
  on every view and never stored.
- Triage panel (v0.3), at the head of the Subjects view: each row shows its
  proposal with its source, its reason in words and the excerpts it rests
  on; an unfiled row proposes its project's only open subject, lets you
  pick another or name a new one, and is filed in the same gesture; an
  empty subject asks to be archived. Arrows move between rows, `f` and `t`
  set the verdict, `Enter` confirms the row in focus and waits for the view
  that removes it, and a held key repeats nothing; an `unsure` proposal
  waits for `f` or `t`. **Confirm all** takes a pointer click only, says how
  many rows it confirms and how many it leaves out (without a verdict,
  without a subject, being confirmed). Confirming `finished` archives the
  subject, with the routine box, and clears its excerpts; `to_resume` sets
  the status. A row whose proposal changed since it was shown is refused
  with the reason and stays. A neutral "N to sort" chip in the header opens
  the panel; it never turns amber, and rows to sort count neither in the
  window title nor in the taskbar overlay.
- Confirmations (v0.3): each records the proposal it answered (verdict,
  source, reason code, call id) and keeps a copy of the excerpts it was
  decided on, which expires when those excerpts were due to, under the same
  30-day bound; a batch is one transaction, answered per item.
- Measured error (v0.3): per proposal source (`state`, `rules`, `model`),
  the Triage panel shows how many of its last 50 confirmed `finished`
  proposals you flipped, with the count it rests on, how many of its last
  50 proposals you flipped, and its `unsure` apart, or says "no finished
  proposal yet". An auto-archive counts once 7 days old, as flipped when its
  subject was reopened within them, by you or by one of its sessions coming
  back.
- Auto-archive (v0.3), off until you turn it on: offered only once Claude
  Code's `state` has proposed `finished` at least 50 times with none of the
  last 50 flipped; while on, the window confirms `finished`, right after
  each view, the rows of filed subjects whose background session Claude
  Code reports done, without the routine mark; an unfiled session, an empty
  subject or a model proposal is never archived this way. When the gate
  breaks it turns itself off and says why. The subjects it archived over
  the last 7 days are listed with **Reopen**.
- Routine candidates (v0.3): the subjects you archived as "routine, could
  have run alone" (a merged one excepted) are listed in the Subjects view
  and in `officina subjects --json`, as candidates for a later catalogue.
- Triage model (v0.3), off by default: a contained `claude -p` call to the
  light model, with no tool, no MCP server, no settings and no session
  persistence, from an empty folder, without the variables a session or
  Officina sets, with the floor's config dir; the excerpts go on stdin
  only, and the answer is read from the schema-constrained field alone (see
  Security). A `.cmd` or `.bat` `claude`, `no_excerpts` or a work folder
  that is not empty refuses the call before anything starts, spending
  nothing. Calls run one at a time on their own thread, killed with what
  they started after 60 s, at most one every 5 s, 60 an hour and 300 a UTC
  day, counted from the registry so the budget holds across restarts; each
  is kept 90 days. The panel and `officina doctor` show the calls, tokens
  and cost today and over 30 days, and each refusal with its cause; doctor
  warns while `claude` resolves to a shim.
- Dry run (v0.3): **Evaluate the model** in the Triage panel sends the model
  your confirmations of a default proposal whose excerpts are still kept,
  newest first, at most 50, one case per call, and shows how many it got
  right beside the default, how often it said finished when you said to
  resume, how many cases got no answer, and what it cost; its progress
  shows while it runs, and nothing it answers becomes a proposal. It runs
  while the model is off or paused.
- Model proposals (v0.3): the model can be turned on (`set_triage_model`)
  only while the latest dry run is under 7 days old, ran at least 30 cases
  and beat the default, and while it can be called (no shim, no
  `no_excerpts`); the switch says why when it is disabled. Turned on, each
  row falling to the default is sent, one case per call within the shared
  budget, and shows "model pending", then the model's proposal with its
  reason as plain text under a "model" label. A call that gives no answer
  leaves the default with why; a refused row waits for new inputs. Three
  failed calls in a row pause the model until it is turned on again or
  `claude` changes; the panel and `officina doctor` say since when. After
  each confirmation of one of its proposals, a model right less often than
  `to_resume` over 20 to 50 of them turns itself off, and only a newer
  winning dry run lets it on again.
- Hook fields in the contract canary (v0.3): the hook notes, in an optional
  `absent` field of event line version 1, the fields of `HOOK_FIELDS` its
  payload lacked; the window records them per Claude Code version in
  `state/contract.json` (`hook_missing`, and `hook_not_observed` for the
  events not seen yet), counting only lines after the version's first
  sighting. The header's contract chip names a missing field
  (`hook_missing` in the board's contract view), and `officina doctor`
  prints both lists, in its registry section, as a warning.

### Changed

- The session file in Scene mode stands under the header as a panel of its
  own: a gap under the header, its right edge on the header's last control,
  four bordered rounded edges and a whole shadow, instead of running into
  the header and the window's edges (story 4.19). Over the scene it covers
  the register whole.
- The session file shows the last exchange as a conversation: your last
  prompt in a bubble on the right, Claude's last answer on the left, each
  labelled, the text exactly as written (story 4.19). It comes after the
  file's buttons, so Bring to front stays in view.
- The board JSON contract is schema 5 (v0.5, story 5.12): each approval
  carries its `class` (`human`, `auto`) and its `state` (`pending`,
  `reviewing`); only a pending request counts as waiting and makes a session
  wait in the scene. The refusal code `class_not_available` is gone.
- Attach refuses a `claude` that is not the native `claude.exe` (v0.5, story
  5.2): through a `.cmd` or `.bat` shim, or any other file type, a click on
  a background card fails with a message pointing at the card's copy of
  the attach command. Every `claude` Officina starts now comes from one
  spawn module (`claude agents`, `--version`, attach and the triage call),
  each refusing such a binary before anything starts, and
  `officina doctor` fails with one line naming every use that is refused.
- The triage call also passes `--settings {"crossSessionInbound":"refuse"}`,
  so no other session can deliver text into it mid-call; its argument
  vector is pinned in `tests/vectors/spawn.json`.
- The catalogue's canonical format goes up by one (v0.5, story 5.4):
  templates, the review model and `auto_max_per_hour` join what the author
  approves. The first start of v0.5 therefore asks for one catalogue
  approval even with an unchanged floor; until then `officina run` refuses
  as "catalogue changed, approval pending" and the gate keeps guarding the
  projects of the last approval. `officina relay --json` is schema 2: each
  issue carries its `table`, the catalogue its `templates` and
  `auto_review`.
- Registry schema version 2 (v0.5, story 5.6), copied to
  `registry/backup/pre-v2-<date>.db` before migrating: the tables of work
  orders, the orchestrator's proposals and starts, and the reviews of
  `auto` requests, written whole in one migration, and `setting` rebuilt
  for its three new keys. Every row of version 1 is kept.
- The header no longer shows "contract held" (v0.5, story 5.8): it shows
  the contract only when it is broken or lacks hook fields, as before. The
  view switch's buttons are 8 px narrower (4 px a side), their height kept,
  so the four views and the status chips stay on one line beside eight
  projects and the alert band on a Full HD screen.
- Sober corners: controls round 4 px and surfaces 6 px, from one radius
  family in `app.css`; pills are kept for count badges (story 4.15).
- One header in every view and mode: the same quota gauges (150 px wide,
  two lines), buttons and chips at the same places; the Walking toggle is
  shown everywhere; "Pin on top" reads "Pin". To resume and Done are
  counted in the header, opening Board mode on their column, instead of
  under the scene (story 4.15).
- Wide screens: a board column shows cards side by side when it has the
  room; the Subjects and Relay views keep to a reading width, centred; the
  session file docks beside the scene when the window leaves room for it
  (story 4.15).
- No shadow under the scene's floor (story 4.15).
- The scene reads in one glance (story 4.17): the register is fixed-size
  overlay UI over the scene's top-right corner, its heading counting
  everyone who waits ("Needs you: 6"), one row per number key, an
  approval-only wait marked by an amber diamond and the word "approval";
  it no longer shrinks with the floor at eight projects. A desk is named
  by its letter on a square tag, so a digit on a round badge is always a
  board number. The decision office's extra waits are a "+n" on its
  plaque, which hangs at the office's back.

- `officina doctor` prints complete blocks to paste, never fragments to
  merge by hand. For the hooks, the whole `"hooks"` key of the user
  settings with Officina's registrations merged in: every hook already
  there kept as it is, Officina's added once, a partial gate entry
  replaced rather than doubled; replace the file's key with it. For the
  statusline, the whole `"statusLine"` value, your command teed and its
  other fields kept, for the user settings and for each project's
  `.claude/settings.local.json` (the whole file when it does not exist).
  Only that key is printed, never another key of a settings file, and a
  file doctor cannot read gets no block, only the reason. Still read-only.
  Each block is also written, its lines whole, to a file under the
  Officina home's `doctor/` folder that its line names, since a terminal
  can cut a long line when it is copied; each run replaces those files,
  and no Claude Code configuration file is written.
- The window's first view is named **Sessions** (it was Board): Board is now
  its default mode, beside Scene and Focus. Digit keys act in every mode;
  `m`, `f` and the digits act in the Sessions view only. The quota
  gauges keep one width whatever their text, the end of a line too long for
  it in a tooltip, so their 30-second tick never moves the header.
- The Needs you halo (v0.4) breathes for two minutes from the start of the
  wait, every halo in one phase shared with the clock, then stays steady; a
  wait already older at first sight is steady at once. It animates opacity
  only, so it costs no style work per frame, keeps its phase and its end
  when the board is shown again, and stops under the system's reduced
  motion setting.
- `officina board --json` (v0.4) carries each live card's place in the
  scene (`scene`: its zone, when its wait began, its alert and whether it
  waits on a relay request alone), the alert count and the time the next
  wait reaches the threshold. A session is in alert when its project is
  high priority or, for a known Needs you wait, when it has waited the
  threshold; a relay request alone never escalates by time. The window
  rebuilds the board when that time comes, without starting `claude`, and
  a failed session list keeps the last known alerts instead of clearing
  them.
- The board contract (`officina board --json`, v0.4) is schema 4: it lists
  the floor's projects in file order (`floor_projects`, each with its colour
  and priority, as a card inside it carries them), carries the alert
  threshold in ms (`alert_after_ms`), each project's `priority`, and each
  card's answer tail (`last_message_tail`), from the same `Stop` line as its
  head.
- `officina board --json` reads the `claude` version (v0.3), so it keeps
  the running version's contract record current and shows its missing hook
  fields.
- `officina hook` also answers `PreToolUse` for the shell tools, as the
  relay gate;
  every other event is recorded as before, and `PreToolUse` payloads are
  never written to the event buffer.
- `officina doctor` requires the `PreToolUse` hook, its matcher naming the
  shell tools and the file tools the gate decides, once the relay guards a
  project, and prints it in the block to paste; workstations without one
  keep the five v0.1 hooks and pay nothing per tool call. A program of the
  catalogue that does not resolve, files that cannot be bound, an
  unacknowledged anomaly or a relay floor that cannot be read also makes it
  exit 1.
- Floor files accept schema 2, where a project without `autonomy` is
  guarded. A schema 1 floor still loads with every project `free`, and the
  relay's keys in it are an error naming `schema = 2`.
- Approval journal lines name their project and action, and a request says
  whether its action has a dry run; lines written before still read.
- The canonical catalogue is format 4: it covers credentials, HTTP actions,
  `forwarded_values`, templates, the review model and `auto_max_per_hour`,
  so a catalogue approved before shows as changed once and is approved
  again.
- The board contract (`officina board --json`) is schema 2: it carries the
  pending approval requests, and `waiting_count` counts the approvable ones
  beside the sessions that wait.
- Event files (v0.3) are deleted only once the registry has ingested them
  and both kept daily copies hold them complete, by the window's start and
  hourly pass; `officina board --json` no longer deletes anything, and reads
  the registry read-only to report its state. With the window closed or the
  registry unavailable, event files are kept and nothing expires.
- The window (v0.3) ingests every event file into the registry before its
  first board and whenever a hook writes, keeps each session's latest prompt
  and the head and tail of its latest answer for 28 days less an hour from
  the event, and clears them at once when `no_excerpts` is turned on. The
  board contract is schema 3: `diagnostics.registry` says when the registry
  cannot be used, and the Board view shows why in a banner.
- The hook (v0.3) also records the last 280 characters of a long answer
  (`last_message_tail`, an optional field of event line version 1).
- The window deletes approval files older than 24 hours, kept check
  outputs older than 7 days and journal files older than 90 days on its
  startup and hourly pass; `officina run` and the gate never delete.
- An HTTP run's `run_finished` line says when its body broke off, and the
  relay view gives each recent entry its tone (`ok`, `failed`, `refused`),
  computed by the rule `officina run` exits by; the window colours rows from
  it instead of judging exit codes and statuses itself.
- Built with Rust 1.99.1 (was 1.94.0; `rust-version` follows) and linted
  with ESLint 10. Every cargo command of CI and the release takes
  `--locked`, the dependency audit also runs weekly and covers the
  frontend's dev dependencies, and `cargo deny` bans `webpki-roots`.
- The README's Privacy section says what reaches the network: the requests
  of the HTTP actions an approved catalogue declares, run with `officina
  run`, and nothing else; it said Officina opened no connection at all.
- Walking costs a fraction of what it did (v0.4, story 4.16): the scene's
  furniture and figures are drawn in eight depth bands, so a walk repaints
  the band it walks in rather than the whole floor, a figure at rest is not
  redrawn, and the walk draws at most one frame per 13 ms, so a 240 Hz
  screen draws 60 frames a second during a walk and a 60 Hz one every
  frame. Profiled in Chromium, a walk's main-thread time per second fell by
  about four fifths. Measured again on the release, walking's own CPU cost
  fell from about 1.4 % to 0.5 % of the machine, and the scene with walking
  on took 1 % of it, at the scene's budget; the owner then turned walking
  back on by default.
- `officina doctor` (v0.4) prints `SubagentStart` and `SubagentStop` in the
  hooks block and reports them missing until the block is pasted again: to
  paste again for the scene to show sub-agents. The hook records them with
  the sub-agent's `agent_id` and `agent_type` (cut to 64 characters,
  optional fields of event line version 1), never its answer or transcript
  paths; they fold into the lead's card only, never its own state or
  activity time, and the registry ignores them.
- The taskbar overlay (v0.4) carries the waiting count on its amber disc,
  "9+" above nine, drawn in code at the small icon's size for the display
  scale (16 px at 100 %, where Windows draws it); `cargo run -p
  officina-app --example taskbar_probe` shows the badges and the attention
  request, asks what the owner sees and writes the taskbar check's report.

### Security

- The review of an `auto` request reads only a session the request is
  bound to (v0.5, story 5.19's security audit, High): `officina run`
  records itself and its ancestors with their start stamps in the request,
  and the window reviews it, or the rule decides it, only when the session
  it names is live in `claude agents` with a process, at the start stamp
  it has now, among those ancestors, and is an interactive session or a
  background session a work order you launched is bound to. Otherwise the
  request is yours at once, with the cause `session_unbound`, its row, its
  journal line and no call, and it is no shadow-gate case. A session can
  no longer claim another session's prompt with
  `CLAUDE_CODE_SESSION_ID`, nor have a `claude --bg` it started speak for
  you.
- A shell command in a guarded project may not name the event buffer
  (`<home>/events`) or the orchestrator's folder (`<home>/orchestrator`),
  beside the relay and the journal (`PROTECTED_DIRS`): a session cannot
  append a forged prompt line the review would read, nor drop a proposal
  there.
- A session's folder is marked untrusted in the orchestrator's tools,
  like its name and excerpts.
- Proposal files dated ahead of the clock no longer count toward the
  proposal tools' hourly limit, so files another program writes there
  cannot stop the orchestrator's tools.
- The meeting's recap and proposed answer, and the Proposals block, name
  the orchestrator's proposals folder as their origin, not the
  orchestrator: any program running as you can write a file there.
- The console launcher (`start_in_new_console`) is held to the spawn
  lint, like `std::process::Command::new`.
- The injection battery (v0.5, story 5.7): `tests/vectors/injection.json`
  replays hostile inputs at the card launch, each tagged with its attack
  family (PRD section 7's Gesture row, declared once in the harness's
  `FAMILIES`), through the window's own functions, the fake claude
  counting `--bg` spawns: none before the click, exactly one after, its
  argument vector equal to the card's and to the catalogue's rebuild. A
  coverage test fails when a family, a section or a refusal has no case.
- The mutation job (CI `mutants`): core features named `mutant-*` each
  skip one check of the launch path; with each one on, the battery must
  still compile and fail on the case that guards it, and a control run
  passes. The window refuses to
  compile with any of them (`MUTANTS_ENABLED`), release builds included.
- Every `claude` Officina starts as a session (`claude agents`,
  `--version`, attach) drops the variables Claude Code gives a session's
  children (`SESSION_ENV_REMOVED`) and `CLAUDE_CODE_DISABLE_AGENT_VIEW`, so
  a window started from inside a session never hands its children that
  session's identity or inbox; the author's `OFFICINA_*` settings are
  kept. Officina's own contained calls (the triage call) receive only the
  contained allow list of the environment. Story 5.2.
- Officina's `git` calls set `GIT_OPTIONAL_LOCKS=0` beside
  `--no-optional-locks`, so a read never writes the index back, and a
  lint (`crates/officina-host/clippy.toml`) refuses
  `std::process::Command::new` outside the three modules that start
  processes (`spawn`, `relay::run`, `git`).
- Session and model text is a distinct type in core (v0.5, story 5.3):
  what the board, the Subjects view, the relay's recent entries and a
  focus show from a session or the triage model can be shown and measured
  but never converted to a string argument, so a later change cannot pass
  it to a process by accident. No behaviour change: the window and the
  `--json` outputs carry the same bytes.
- The meeting's `git` finds a filter driver however the repository
  configures it before diffing the working tree (v0.5, story 5.19's
  security audit): a filter set in a file the repository's configuration
  includes, or in its `config.worktree`, escaped the lookup, so a session
  could have the window run a clean filter program of its choosing on
  Receive. The lookup now follows includes and reads every scope, and a
  filter the repository sets, there or in its own config, limits the diff
  to the commits.
- The gate fails closed where it guards. Doctor prints its `PreToolUse`
  entry with `"onFailure": "block"`, so Claude Code (2.1.295 and later)
  blocks the call when the hook cannot start, times out or crashes, where
  it used to let the call through. Doctor fails on an installed gate entry
  without the field and on an older Claude Code: paste the block again. A
  fault inside the gate now refuses a call in a guarded repository, and a
  call it cannot locate, with a reason that says so. A call outside every
  guarded repository still gets no decision.
- The relay is a guardrail against mistakes and injected instructions, and
  a record; it is not a security boundary: a session runs as the author and
  could edit Officina's files. The isolated workshop (v0.6) is the
  boundary.
- `officina.exe`, which sessions call, contains no code that approves a
  catalogue or stops the relay; only the window does. CI builds the shipped
  binary alone and searches it, with a positive control on the window, and
  the release workflow searches the very files it packages.
- `officina run` fails closed: anything unreadable, unapproved or changed is
  a refusal. It never starts a shell: a parameter value fills exactly one
  argument, and a value starting with `-`, holding a control, bidirectional
  or invisible formatting character, or having a `.` or `..` path segment
  is refused (on the `native` runner also one starting with `/` or `@`); a
  `.bat` or `.cmd` program is refused. WSL actions start the system
  directory's `wsl.exe`, never one from `PATH`, with `--exec` and without
  `WSLENV`.
- The relay gate covers every Claude Code tool that runs a shell command,
  not only Bash: a Monitor command is read like a Bash one, and the
  PowerShell tool, which Claude Code turns on by default on Windows, is
  refused whole in a guarded project, where sessions keep the Bash tool.
  The `PreToolUse` matcher `officina doctor` prints and requires names
  every one of them, and doctor names the tools an earlier matcher leaves
  out.
- An approval card never shows a request file's own command: it rebuilds the
  command from the approved catalogue, and a request that does not rebuild
  (forged, of another catalogue, or of an action no longer approved) cannot
  be approved. No key approves or rejects. An approval is used once, even by
  two calls at the same instant.
- The gate gets stricter, never looser, when the approved catalogue or the
  floor cannot be read, and a repository stays guarded when either says so,
  so editing the floor or `cd ..` does not lift containment.
- Catalogue notices: a closed argument vector does not close what the
  program does with a value. An ssh action's placeholder after the
  destination reaches the remote shell, and an Ansible action's placeholder
  in extra vars or module arguments is templated; the banner, `officina
  relay --json` and `officina doctor` (as warnings) say so with one line
  each, without blocking the action or changing its approval. The README no
  longer says that no shell reads a value.
- A credential's secret goes into its one request header and nowhere else:
  not the output, the journal, an approval file or the Relay view; a
  secret holding a line break is refused, a header that frames the request
  and a URL with a user or password are catalogue issues, and a credential
  over plain `http` is refused. `officina.exe` bundles no root certificate
  set; CI checks that `webpki-roots` stays out of its graph.
- The tree fingerprint runs `git` with external diff tools, textconv
  drivers, fsmonitor and `GIT_` variables out of the way, in the
  environment Windows builds from the registry rather than the caller's,
  so neither a repository's configuration nor a session's variables run
  anything or move a rule inside a relay run.
- The tree fingerprint no longer trusts the caller's `PATH` or git's stat
  cache: `git` is the one Git for Windows records in the registry, only
  lists the tracked files, a submodule's included, and the unignored ones,
  and Officina hashes each file's bytes itself, with the content of the
  repository's `info/exclude` and the global excludes file, so a shim on
  `PATH`, a restored modification time, a clean filter, an edit inside a
  submodule or a file added with the rule that ignores it cannot hide a
  change. A tree past 64 MiB per file, 20,000 files or 512 MiB, an
  unreadable file, a folder under a `.git` that `git` cannot read as a
  work tree (a broken configuration, `core.bare`), and a git work tree
  without that `git` refuse (`tree_too_large`, `tree_unreadable`,
  `git_unavailable`); the journal names the `git` that listed the files.
- Every `human` action in a git work tree is bound to its files, dry run or
  not: an edit while its call waits for the author makes the run refuse
  `tree_changed` and leaves the approval unused. The card says what the
  files are bound to, or "no file binding: not a git tree".
- `officina run` starts actions with the environment Windows builds from
  the registry for the user, `ComSpec` the system's `cmd.exe` and `PATHEXT`
  the machine's, then the action's `env`: nothing of the caller's
  environment reaches the program, so a variable a session sets for one
  call (`PATH`, `PYTHONPATH`, `NODE_OPTIONS`, `KUBECONFIG`) cannot change
  what an approved action loads, and WSL actions get no `WSLENV`. The gate
  also denies an `officina` call with `PATH`, a `GIT_` variable or another
  variable of `OFFICINA_ENVIRONMENT_NAMES` reassigned.
- Once a project is guarded, the file tools (Write, Edit and the others of
  `FILE_TOOLS`) cannot write under Officina's home from any project, and the
  hook resolves the session's folders, a file tool's target and the
  guarded projects' own paths by handle, so an 8.3 name, a junction, a
  link or a `subst` drive cannot lead around the gate, on either side.
- The window records its own decisions (each request's verdict and request
  key, the catalogue it approved, whether it holds the relay stopped) and
  lists, in the Relay view, an approval file, a run, an approved catalogue
  or a missing stop flag it cannot account for, and the record itself gone
  or unreadable; `officina doctor` fails until the author acknowledges
  each. `officina run` obeys the record: it leaves unused an approval the
  record does not hold as approved for that request (`approval_not_given`),
  runs only the catalogue the record names (`catalogue_unconfirmed`), stays
  stopped while the record holds it stopped, and refuses everything while
  the record is missing or unreadable (`decision_record_unreadable`), until
  the author's acknowledgement starts a new one.
- A `pattern` that lets a value forwarded to a remote shell or a template
  hold whitespace, shell or template characters drops the action with the
  issue `forwarded_pattern`, unless it sets `forwarded_values = "accepted"`;
  the probe also tries values as short and as long as the pattern accepts,
  so a pattern that needs a long value (`.{5,}`) is caught.
  `forwarded_values` is part of the canonical catalogue (see Changed).
- A request file's dry-run id names a kept output only when it is a run
  id, so a forged request cannot make the window read another file.
- The window's Content Security Policy allows no inline style; Vite inlines
  no asset as a `data:` URI, which the font policy refused.
- The registry's writer, like the relay's authority, exists only in the
  window (v0.3): CI searches `officina.exe` for the writer's marker and
  finds none, with a positive control on the window. Every command line
  opens the registry read-only and never creates it.
- The triage model's text never gains any power (v0.3): the excerpts reach
  the call on stdin only, as one line of escaped JSON between markers
  carrying a random nonce, presented as data never to follow, one case per
  call so an excerpt cannot steer another row; a subject's name is never
  sent. Only the schema-constrained answer is read, a verdict and a reason
  of 1 to 200 characters without control, bidirectional or invisible
  formatting characters; anything else is a failed, invalid or timed-out
  call. The reason is untrusted text: rules never read it, the window
  renders it as text, it lives only in `model_answer` (deleted with the
  excerpts it answered and on confirmation) and a confirmation keeps the
  model's reason code and call id only. A model proposal is only ever a
  proposal: the author confirms it, auto-archive never takes it, and the
  model pauses after three failed calls and turns itself off when it does
  worse than the default.
- The README's Privacy section says what leaves the machine with the
  triage model (v0.3): while it is on, or a dry run runs, `claude -p` sends
  each judged row's prompt and answer excerpts and how its session ended
  to the Anthropic API under your account; nothing while it is off; and
  `no_excerpts` or keeping it off prevents it. A test keeps the sentence
  from disappearing. The registry section states that the registry is not
  a security boundary against code running as you, as the relay's does.
- Officina runs no `.cmd` or `.bat` shim of `claude` for any call (v0.3):
  `claude agents` and `claude --version` refuse it as the triage call
  does, since `cmd.exe` runs whatever the script says; doctor and the
  board say why. Install the native `claude.exe` or set
  `OFFICINA_CLAUDE_BIN` to it.
- An event line dated in the future, by a wrong clock or a forged line, is
  recorded at the time it is read, and a line dated before its file's day
  at the start of that day (v0.3): no excerpt outlives the 30-day bound
  and newer observations still replace it.
- Text reaching the registry and the triage model is held to the hook's
  own bounds when it is read (v0.3), whatever wrote the event files: a line
  longer than the hook writes is one corrupt line, an excerpt is stored cut
  to 280 characters, and a triage call carries each text cut to its bound,
  so what one call costs is bounded too.
- The triage call's environment is an allow list (v0.3), no longer the
  window's minus a few names: what a process needs to start, the proxy and
  certificate variables, and what `claude` authenticates with. A variable
  that maps the `haiku` alias or a default model to another one, an IDE's
  or a session's variables, and the Bedrock and Vertex switches no longer
  reach it.
- **Confirm all** leaves out the rows the triage model proposed
  `finished` (v0.3), counted apart: each is confirmed on its own row, so
  one click never archives a subject, and clears its excerpts, on text a
  session read.
- `no_excerpts` hides every excerpt as soon as it is on (v0.3): the
  Subjects view, the Triage panel and `officina subjects --json` show none,
  and the window runs the pass that clears them at once when its floor
  turns it on, instead of at the next hourly pass. `officina doctor
  --floor <file>` warns when that file sets `no_excerpts` the hook cannot
  see, since the hook reads only `OFFICINA_FLOOR` or
  `~/.officina/floor.toml`.
- A damaged registry is restored from the copy whose name carries the
  newest UTC day (v0.3), never from the newest modification time, which any
  process can set; a file of another name under `registry/backup/` is never
  restored.
- Officina itself writes nothing under the Claude config dir nor into
  `~/.claude.json` (v0.3): the zero-write fingerprint now covers the
  registry's commands and the window's own model call; the `claude`
  processes it starts keep their own state.

### Fixed

- A floor whose catalogue holds only templates (free projects, no action)
  was never offered for approval: the window showed no catalogue banner,
  its templates could never be approved, and no work order could run from
  it (gate 4's window run). A template now puts the relay in use like an
  action or a guarded project: the banner and the header chip offer the
  catalogue, and `officina doctor` prints its relay lines. Such a floor
  still guards nothing, its free projects stay free, and the window keeps
  no known-projects record for it before its approval.

- The composer's Create, which names a new subject, seemed to do nothing:
  the card changed below, often out of view. A status line under the
  subject picker now names the card and where it files its session, the
  card is scrolled into view after each choice, and a subject picked or a
  proposal opened gives it the focus.

- Doctor kept Officina's hook and statusline entries that named another
  `officina.exe` (a copy placed elsewhere earlier, an older install) as
  registered, so sessions kept running that copy and a block pasted after
  an install mixed both paths. It now fails the hooks line, and warns on
  the quota line even while the quota is fresh, naming the other path;
  the hooks block points every Officina entry at the running
  `officina.exe`, keeping each entry's other fields and every foreign hook,
  never two Officina entries for one event, and the statusline value
  points its `quota write` stage at it, its `--tee` and the rest of the
  pipeline kept. A bare `officina` found through `PATH` is left alone. The
  README states where the installer puts the executables,
  `%LOCALAPPDATA%\Officina\`, and to run that `officina.exe`'s doctor
  (D-981).
- A `claude --version` the window could not read, as one cut by its
  5-second deadline on a loaded machine, left the version unknown until
  `claude` changed or the window restarted, so a meeting's answer to a
  live idle session fell back to the copy and `claude attach`: the window
  now asks again with each session list until it reads it (D-880).
- Two queued auto reviews could start their model calls less than 5
  seconds apart on a loaded machine: the spacing was counted from the
  window's tick that submitted the first call, which can precede the call
  by seconds, rather than from the call's own start; it now counts from
  the start the review row records (D-850).
- The relay journal no longer loses a line when its lock stays held past
  2 seconds, as two loaded writers can on a busy machine: a gate refusal or
  fault, a refusal, a run's end, an auto review, a work order's launch
  outcome and the known-projects fault then go to a spill file beside the
  day file, read with it, pruned with it and watched by the window, instead
  of being dropped without a word (D-760).
- A waiting session's ticket could hide behind the alert room's beacon
  when the queue reached the corridor: tickets are drawn above everything
  on the floor (story 4.17).
- Archiving a subject in the Subjects view left its ended session in To
  resume: the board now reads the subject's status, so the session moves
  to Done, tagged "subject archived", and comes back if the subject is
  reopened (story 4.15).

- `officina doctor` no longer prints a statusline that tees Officina's
  `quota write` into itself when the existing statusline already runs it.
- `officina doctor`, when the quota is stale, now looks for the cause: the
  folders of the sessions the hook saw recently whose own
  `.claude/settings.local.json` or `.claude/settings.json` sets a
  statusline that does not run Officina's `quota write`, which wins over the
  user's in sessions opened there. It names them and prints, for each, the
  statusline for its `.claude/settings.local.json`. Read-only.
- The Triage panel's Confirm all button keeps its width: what it leaves out
  is a line beside it ("Left out: ..."), which the button names as its
  description, instead of part of its label.
- A header status chip (the waiting pill, relay stopped, catalogue change,
  to sort, the contract) no longer moves the quota gauges or the buttons:
  the chips sit on one status line under the right-hand buttons, kept even
  when empty, so the header keeps its height as they come and go.
- Toggling a control no longer moves the controls beside it. A button
  whose label follows its state (Stop relay and Resume relay in the header
  and the Relay view, Pin on top and Pinned, Copy attach command and
  Copied) keeps the width of its longest label; Evaluate the model and the
  dry run's progress share one place; the page keeps the scrollbar's room
  in every view, so switching views does not shift the header's buttons. A
  Playwright spec flips every toggle the window shows and fails when one
  changes size or moves what follows it, or when a toggle has no case.
- Smaller fixes from the Epic 3 audits (v0.3): a registry read that fails
  is reported (doctor fails its triage line instead of calling the model
  disabled, the board names the error instead of showing no subject); a
  window command that times out says the next view shows whether the
  change was made, never that nothing was; folder and subject names beside
  other words are isolated, so a bidirectional control in a name cannot
  reorder them; a triage call's usage out of range is read as unknown.
- A dry run no longer shows a progress bar that never moves when the
  thread that makes the triage calls could not start or stopped (v0.3): it
  ends with what ran recorded and says why, and **Evaluate the model** is
  refused until the window restarts.
- Turning the triage model on again sends once more each row it had
  refused (v0.3), on the same inputs: fixing the cause (an empty work
  folder, a native `claude.exe`) no longer leaves those rows on the
  default until their session changes.
- The window says when it cannot write `relay/known-projects.json`, the
  record a broken floor falls back to: one `known_projects_unwritable` gate
  error in the journal, listed in the Relay view and counted by
  `officina doctor`, where the failure used to pass silently.
- A `claude` call that timed out now ends with every process it started
  (a Windows job object), instead of leaving them running and holding the
  board's reader threads.
