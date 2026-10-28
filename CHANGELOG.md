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
  `after-check` and `human` actions are refused as not available yet.
- `officina run --list`: the actions of the current folder's project with
  what each parameter accepts, and whether the relay is stopped or a
  catalogue change awaits approval.

### Changed

- The gate's command lexer reads a 64 KiB command in about 1 ms (it took
  about 4 ms): fewer allocations per word and each command analysed as soon
  as it ends.
