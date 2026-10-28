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
  own, or 126 refused, 124 timed out, 127 not started, 64 usage. `human`
  actions are refused as not available yet.
- `officina run --list`: the actions of the current folder's project with
  what each parameter accepts, the check an `after-check` action waits for,
  and whether the relay is stopped or a catalogue change awaits approval.
- `after-check` actions: one runs only after its check succeeded in the same
  project with the same parameters, under the same approved catalogue and on
  the same code, in the last 30 minutes, the journal of the day before
  included; otherwise `officina run` refuses with `check_required` and names
  the check. In a git work tree the code is bound by a fingerprint of `HEAD`,
  the diff to it, the untracked files git does not ignore and the tracked
  files flagged to hide their edits (by content up to 1 MiB, by size beyond).
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
  architecture.
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
  WSL action's tools and program probed inside its distro), and warnings for
  a timeout that can outlive the Bash tool's cap, an `after-check` action
  whose check its project does not list, a project with `after-check`
  actions outside a git work tree, unreadable journal lines and gate errors
  of the last 24 hours. For each guarded project it prints the `CLAUDE.md`
  paragraph to paste, generated from the approved catalogue.
- The window follows floor edits without a restart: project names and
  colours, a moved config dir (sessions, quota, attach) and the relay's
  state, which it also rebuilds when the relay's files or today's journal
  change.

### Changed

- `officina hook` also answers `PreToolUse` for the shell tools, as the
  relay gate;
  every other event is recorded as before, and `PreToolUse` payloads are
  never written to the event buffer.
- `officina doctor` requires the `PreToolUse` hook, its matcher naming the
  shell tools only, once the relay guards a project, and prints it in the
  block to paste; workstations without one keep the five v0.1 hooks and pay
  nothing per shell tool call. A program of the catalogue that does not resolve, or a relay
  floor that cannot be read, also makes it exit 1.
- Floor files accept schema 2, where a project without `autonomy` is
  guarded. A schema 1 floor still loads with every project `free`, and the
  relay's keys in it are an error naming `schema = 2`.

### Security

- The relay is a guardrail against mistakes and injected instructions, and
  a record; it is not a security boundary: a session runs as the author and
  could edit Officina's files. The isolated workshop (v0.6) is the
  boundary.
- `officina.exe`, which sessions call, contains no code that approves a
  catalogue or stops the relay; only the window does. CI builds the shipped
  binary alone and searches it, with a positive control on the window.
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
- The gate gets stricter, never looser, when the approved catalogue or the
  floor cannot be read, and a repository stays guarded when either says so,
  so editing the floor or `cd ..` does not lift containment.
- The tree fingerprint runs `git` with external diff tools, textconv
  drivers, fsmonitor and the caller's `GIT_` variables out of the way, so a
  repository's configuration runs nothing inside a relay run.

### Fixed

- A `claude` call that timed out now ends with every process it started
  (a Windows job object), instead of leaving them running and holding the
  board's reader threads.
