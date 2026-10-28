# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

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
- Registry (v0.3, in progress): a local SQLite database at
  `~/.officina/registry/registry.db` that only the window writes and every
  command line reads read-only. It is never opened on a network folder; in
  a folder a sync client manages it opens and `officina doctor` names the
  folder to exclude. At each open the window checks its integrity: a
  damaged database is set aside under `registry/quarantine/` and replaced
  by the newest valid daily copy (`registry/backup/`, two kept), and a
  database written by an older version is copied, then migrated.
  `officina doctor` reports its location, version, integrity, copies,
  quarantine and last restore, and never creates it.
- Session records (v0.3, in progress): the window records each session
  once, whichever of `claude agents` and the hook sees it first, with each
  fact's latest value, its source (`agents`, `hook`, `rules`) and its time.
  A refresh of `claude agents` writes only what changed and never stores
  `waitingFor`. A session gone from `claude agents` without a `SessionEnd`,
  also while the window was closed, is `interrupted`; a `SessionEnd`
  always wins over it, and a session listed again or sent a newer prompt
  is no longer ended. `/clear` and `/resume` in the same process link the
  session to its successor, and a run never seen in `claude agents` that
  ended with `other` is recorded as non-interactive.
- Subjects (v0.3, in progress): a new session joins its project's subject
  when that project has exactly one active subject, whichever source sees
  it first; otherwise it stays unfiled, and no rule creates a subject. The
  window's writer creates, renames, merges, archives (with the routine box)
  and reopens subjects and files or unfiles sessions, each refusal with its
  reason and each change recorded as the author's with its time. A session
  alive again wakes its `to_resume` or archived subject. A subject keeps
  only its latest excerpts, and archiving clears them from the database
  and its log.
- `officina subjects --json`: the registry's subjects and sessions, joined
  to the live board, the unfiled sessions per project and the ingestion's
  lag, read-only and with the window closed; exit 3 with the reason when
  the registry cannot be read.
- Board cards carry their subject (`subject`, in the board contract's
  schema 3), from the registry, read-only for `officina board --json`; the
  window shows it on each card.
- Subjects view in the window (v0.3, in progress), beside Board and
  Relay: the active then `to_resume` subjects with their sessions' live
  state, and for a subject with no live session its last prompt and the
  end of its last answer; the unfiled sessions by project, non-interactive
  runs counted and collapsed; the archived subjects on demand. Create a
  subject (from selected sessions, which give it their project, or in a
  project the registry knows), rename, file, unfile, merge, archive with
  the "routine, could have run alone" box and reopen, each in one gesture;
  a refused action says which rule refused it and changes nothing. Notes
  name the folder to exclude from a sync client, the copy a damaged
  registry was restored from (for 7 days), or why the registry cannot be
  read. Digit keys focus cards in the Board view only.
- What needs a verdict (v0.3, in progress): one row per `active` subject
  whose sessions have all ended, never one per session, anchored on its
  latest session that saw a prompt; one per ended, unfiled, interactive
  session that saw a prompt and was not replaced by `/clear` or
  `/resume`, with the subject to file it under when its project has only
  one open; and "empty subject: archive?" for an `active` subject none of
  whose sessions saw a prompt. Each row carries one proposal, `finished`,
  `to_resume` or `unsure`, with its source (`state`, `rules` or `model`)
  and a reason code, computed again on every view and never stored.
  `officina subjects --json` prints the rows and their count.
- Triage panel (v0.3, in progress), at the head of the Subjects view: each
  row shows its proposal with its source, its reason in words and the
  excerpts it rests on; an unfiled row proposes its project's only open
  subject, lets you pick another or name a new one, and is filed in the
  same gesture; an empty subject asks to be archived. Arrows move between
  rows, `f` and `t` set the verdict, `Enter` confirms the row in focus and
  waits for the view that removes it, and a held key repeats nothing; an
  `unsure` proposal waits for `f` or `t`. **Confirm all** takes a pointer
  click only, says how many rows it confirms and how many it leaves out
  (without a verdict, without a subject, being confirmed). Confirming
  `finished` archives the subject, with the routine box, and clears its
  excerpts; `to_resume` sets the status. A row whose proposal changed
  since it was shown is refused with the reason and stays. A neutral "N
  to sort" chip in the header opens the panel; it never turns amber, and
  rows to sort count neither in the window title nor in the taskbar
  overlay.
- Each confirmation records the proposal it answered (verdict, source,
  reason code, call id) and keeps a copy of the excerpts it was decided
  on, which expires when those excerpts were due to, under the same
  30-day bound; a batch is one transaction, answered per item.
- Hook fields in the contract canary (v0.3, in progress): the hook notes,
  in an optional `absent` field of event line version 1, the fields of
  `HOOK_FIELDS` its payload lacked; the window records them per Claude
  Code version in `state/contract.json` (`hook_missing`, and
  `hook_not_observed` for the events not seen yet), counting only lines
  after the version's first sighting. The header's contract chip names a
  missing field (`hook_missing` in the board's contract view), and
  `officina doctor` prints both lists.
- Triage model, off (v0.3, in progress): a contained `claude -p` call,
  never made unless you ask for a dry run. It runs with the light model,
  no tool, no MCP server, no settings and no session persistence, from an
  empty folder, without the variables a session or Officina sets, with the
  floor's config dir; the excerpts go on stdin only, as one line of escaped
  JSON between markers carrying a random nonce, and the answer is read from
  the schema-constrained field alone: a verdict and a reason of at most 200
  characters, or the call counts as invalid, failed or timed out, never as
  a proposal. A `.cmd` or `.bat` `claude`, `no_excerpts` or a work folder
  that is not empty refuses the call before anything starts, spending
  nothing. Calls run one at a time on their own thread, killed with what
  they started after 60 s, at most one every 5 s, 60 an hour and 300 a UTC
  day, counted from the registry so the budget holds across restarts; each
  is kept 90 days.
- Dry run: **Evaluate the model** in the Triage panel sends the disabled
  model your confirmations of a default proposal whose excerpts are still
  kept, newest first, at most 50, one case per call, and shows how many it
  got right beside the default, how often it said finished when you said
  to resume, how many cases got no answer, and what it cost; its progress
  shows while it runs, and nothing it answers becomes a proposal. The panel
  also shows the calls and their cost today and over 30 days, and each
  refusal with its cause. `officina doctor` reports the model's state, its
  calls and cost, and the refusal while `claude` resolves to a shim.

### Changed

- `officina board --json` reads the `claude` version (v0.3, in progress),
  so it keeps the running version's contract record current and shows its
  missing hook fields.
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
- The canonical catalogue is format 3: it covers credentials, HTTP actions
  and `forwarded_values`, so a catalogue approved before shows as changed
  once and is approved again.
- The board contract (`officina board --json`) is schema 2: it carries the
  pending approval requests, and `waiting_count` counts the approvable ones
  beside the sessions that wait.
- Event files are deleted only once the registry has ingested them and both
  kept daily copies hold them complete, by the window's start and hourly
  pass; `officina board --json` no longer deletes anything, and reads the
  registry read-only to report its state. With the window closed or the
  registry unavailable, event files are kept and nothing expires.
- The window ingests every event file into the registry before its first
  board and whenever a hook writes, keeps each session's latest prompt and
  the head and tail of its latest answer for 28 days less an hour from the
  event, and clears them at once when `no_excerpts` is turned on. The board
  contract is schema 3: `diagnostics.registry` says when the registry
  cannot be used, and the Board view shows why in a banner.
- The hook also records the last 280 characters of a long answer
  (`last_message_tail`, an optional field of event line version 1).
- The window deletes approval files older than 24 hours, kept check
  outputs older than 7 days and journal files older than 90 days on its
  startup and hourly pass; `officina run` and the gate never delete.
- An HTTP run's `run_finished` line says when its body broke off, and the
  relay view gives each recent entry its tone (`ok`, `failed`, `refused`),
  computed by the rule `officina run` exits by; the window colours rows from
  it instead of judging exit codes and statuses itself.
- Built with Rust 1.99.0 (was 1.94.0; `rust-version` follows) and linted
  with ESLint 10. Every cargo command of CI and the release takes
  `--locked`, the dependency audit also runs weekly and covers the
  frontend's dev dependencies, and `cargo deny` bans `webpki-roots`.
- The README's Privacy section says what reaches the network: the requests
  of the HTTP actions an approved catalogue declares, run with `officina
  run`, and nothing else; it said Officina opened no connection at all.

### Security

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

### Fixed

- The window says when it cannot write `relay/known-projects.json`, the
  record a broken floor falls back to: one `known_projects_unwritable` gate
  error in the journal, listed in the Relay view and counted by
  `officina doctor`, where the failure used to pass silently.
- A `claude` call that timed out now ends with every process it started
  (a Windows job object), instead of leaving them running and holding the
  board's reader threads.
