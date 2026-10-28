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
- Floor schema 2, the relay's catalogue: projects declare `autonomy`
  (`free` or `guarded`) and `actions`; `[[action]]` entries declare a closed
  command (`runner` `native` or `wsl`, an absolute `program`, literal `env`,
  `args` with at most one placeholder each, parameters limited to `values` or
  an anchored `pattern`, `class`, `timeout_s`, `max_output_kib`). An invalid
  action is skipped with its line and never invalidates the floor for the
  board. A schema 1 floor still loads, every project `free`.
- Relay catalogue approval, the core of it: the catalogue is reduced to its
  meaning and hashed, the approved copy is kept in `relay/approved.json`,
  and an approval is refused when the floor changed since it was displayed.
  Only the window build contains the code that approves; CI checks that the
  shipped `officina.exe` does not.
- Relay gate decision, the core of it: a command lexer reads a `Bash` command
  the way bash splits it and finds every program it would start, through
  quotes, wrappers, `bash -c`, interpreter one-liners and command
  substitutions; in a guarded repository the gate refuses a guarded program
  (remote shells, Ansible, WSL and the Windows distro launchers), a command
  name built at run time, a PowerShell encoded command, a redirection of
  Officina's home, and any mention of an `OFFICINA_` setting or of Officina's
  own files. Outside a guarded repository it decides nothing. The documented
  limits of a text match are recorded in the architecture.
- Relay gate in the hook: `officina hook` answers Claude Code's `PreToolUse`
  for `Bash` with a deny that holds in every permission mode, bypass
  included, and lists the actions the session may run instead; otherwise it
  prints nothing. A repository is guarded when the current floor or the
  approved catalogue says so, from the session's root or the shell's current
  folder, so neither editing the floor nor `cd ..` lifts containment; an
  unreadable approval or floor makes the gate stricter, never looser.
  `PreToolUse` payloads are never written to the event buffer.
- Relay journal (`~/.officina/journal/YYYY-MM-DD.jsonl`): every gate deny,
  gate error and catalogue approval leaves one line, written under a lock
  file so lines never interleave; command excerpts follow `no_excerpts`.
- `officina doctor` requires and prints the `PreToolUse` hook, matcher `Bash`
  only, once the relay guards a project; workstations without one keep the
  five v0.1 hooks and pay nothing per `Bash` call.
- `officina run <action> name=value ...`: runs an action of the approved
  catalogue from its project's folder, `free` or `guarded`, after checking,
  in order, that the relay is not stopped, that the catalogue still is the
  approved one, the project, the action and every parameter; the program
  starts as an argument vector (WSL actions through the system's `wsl.exe`),
  with its timeout and an output budget, and it and every process it starts
  end together. Exit codes are the program's own, or 126 refused, 124 timed
  out, 127 not started, 64 usage. Each run journals a start and an end line
  (output byte counts and SHA-256, never the output), each refusal one line.
  `human` actions are refused as not available yet.
- `after-check` actions: `officina run` runs one only after its check
  succeeded with the same parameters, under the same approved catalogue and
  on the same code, in the last 30 minutes, the journal of the day before
  included; otherwise it refuses with `check_required` and names the check.
  In a git work tree the code is bound by a fingerprint of `HEAD`, the diff
  to it, the untracked files git does not ignore and the tracked files
  flagged to hide their edits (by content up to 1 MiB, by size beyond);
  `git` runs with external diff, textconv, fsmonitor and the caller's `GIT_`
  variables out of the way. `officina run --list` says which check an
  action waits for, and run start lines record the tree and the timeout.
- `officina run --list`: the actions of the current folder's project with
  what each parameter accepts, and whether the relay is stopped or a
  catalogue change awaits approval.
- Catalogue approval in the window: an amber "catalogue change" chip and a
  banner with the change per action and per field (what an added action
  runs, a changed parameter by name), the projects and guarded programs that
  changed and the actions the floor skips with their lines; **Approve this
  catalogue** approves exactly what is shown, on a pointer click only, and
  says so when the floor changed again in between.
- Stop and resume the relay from the window's header: while stopped,
  `officina run` refuses every action and a terracotta "relay stopped" chip
  and a board banner say so; the gate keeps guarding. Each change is one
  journal line.
- The window follows floor edits without a restart: project names and
  colours, a moved config dir (sessions, quota, attach) and the relay view,
  which it also rebuilds when the relay's files or today's journal change;
  it keeps `relay/known-projects.json` current while the relay is in use.
- `officina relay --json`: the relay's state once, read only and the same
  contract the window reads: the stop flag, the approved catalogue, a
  pending change with its diff per action and field, the floor's catalogue
  issues with their lines, the last 50 journal entries with each run's start
  and end paired (abandoned when no end came 60 s past its timeout), the
  unreadable journal lines and the gate errors of the last 24 hours. Exit 0,
  or 1 when the Officina home cannot be read.

### Changed

- The gate's command lexer reads a 64 KiB command in about 1 ms (it took
  about 4 ms): fewer allocations per word and each command analysed as soon
  as it ends.

### Fixed

- A `claude` call that timed out now ends with every process it started
  (a Windows job object), instead of leaving them running and holding the
  board's reader threads.
