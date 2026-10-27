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
