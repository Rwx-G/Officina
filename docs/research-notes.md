---
title: Officina research notes
author: Rwx-G
---

# Research notes

Facts about Claude Code that Officina depends on. Each entry says how it is
known: **verified** (observed on a workstation, with the version), or
**documented** (read in the official documentation, with the page). An
assumption that is neither is listed as such and is never used to decide.

Step 0 of the framing, run on 2026-10-27 against Claude Code 2.1.311 on
Windows 11.

## E1. `claude agents --json`

- **Verified (2.1.311).** `claude agents --json` prints active sessions,
  interactive and background, as a JSON array, without a TTY. `--all` adds
  completed background sessions.
- **Verified.** Fields observed: `pid`, `id` (background only), `cwd`, `kind`
  (`interactive`, `background`), `startedAt` (Unix ms), `sessionId`, `name`,
  `status` (only while the process lives), `state` (background).
- **Verified values.** `status`: `busy`, `idle`. `state`: `done`, `failed`.
  A background session that answered and waits for its next instruction reads
  `status: idle, state: done`; after `claude stop` it keeps `state: done` and
  loses `pid` and `status`.
- **Documented** (agent-view page). `status`: `busy`, `waiting`, `idle`.
  `state`: `working`, `blocked`, `done`, `failed`, `stopped`. `waitingFor` is
  present when `status` is `waiting`: `permission prompt`, `input needed`,
  `sandbox request`, `worker request`, `dialog open`. "A session that finished
  its turn and is waiting for your next instruction reads done, not blocked."
- **Verified (2.1.311).** A background session started with
  `--permission-mode default` and asked to run a shell command reads
  `status: waiting, state: blocked, waitingFor: "permission prompt"`. One
  asked to use `AskUserQuestion` reads
  `status: waiting, state: blocked, waitingFor: "input needed"`. Both held
  steadily over 30 s.
- **Verified (2.1.311, in daily use).** An interactive session in a VS Code
  terminal answering a question through a dialog reads
  `status: waiting, waitingFor: "input needed"` (no `state`): interactive
  sessions do report waits, not only background ones.
- **Inferred, not verified.** An interactive session that ends its turn with a
  plain-text question reads `status: idle`, like any finished turn: nothing
  distinguishes "asked something" from "finished". Officina must therefore
  treat interactive `idle` as "your turn", never as "done".
- Consequence: the framing's step 0 item 1 is answered. `state` is not present
  on interactive sessions; their signal is `status`.

## E2. Hook payloads

- **Verified (2.1.311)**, through `claude -p --settings <hooks>`:
  - `SessionStart`: `session_id`, `transcript_path`, `cwd`, `hook_event_name`,
    `source` (`startup` observed).
  - `UserPromptSubmit`: adds `permission_mode`, `prompt`, `prompt_id`.
  - `Stop`: adds `last_assistant_message`, `stop_hook_active`,
    `background_tasks`, `session_crons`, `effort`.
  - `SessionEnd`: adds `reason` (`other` observed for `-p`).
- **Documented** (hooks page). `Notification` carries `message`, `title`,
  `notification_type`, among them `permission_prompt`, `idle_prompt`,
  `elicitation_dialog`, `agent_needs_input`, `agent_completed`.
  `SessionStart.source`: `startup`, `resume`, `clear`, `compact`, `fork`.
  `SessionEnd.reason`: `clear`, `resume`, `logout`, `prompt_input_exit`,
  `other`.
- **Documented.** Hooks from several settings levels merge rather than
  replace; an identical handler defined twice runs once. Exit code 0 is
  success, 2 blocks, any other code is a non-blocking error. Command hooks
  accept `async: true` and `timeout`. Answers step 0 item 4 on paper; the
  merge of `--settings` hooks with project hooks is not stated explicitly.
- **Verified.** Hooks from `--settings` ran alongside the probe directory's
  (empty) project settings.

## E3. Statusline

- **Documented** (statusline page). Input JSON carries
  `rate_limits.five_hour` and `rate_limits.seven_day`, each with
  `used_percentage` (0-100) and `resets_at` (Unix seconds), on subscription
  accounts only and after the first API response.
- **Not documented.** Whether a statusline runs for `--bg` sessions. Open.
- **Not verified.** Which statusline wins when user and project settings both
  define one (step 0 item 2). The general precedence puts project above user.

## E4. Background sessions

- **Verified.** `claude --bg --name <name> "<prompt>"` returns immediately and
  prints the id for `attach`, `logs`, `stop`. `claude stop <id>` and
  `claude rm <id>` work. A directory must have been trusted once
  interactively before `--bg` runs in it.
- **Documented.** Background sessions are run by a separate supervisor process
  and need no open terminal. Survival when the supervisor itself dies is
  documented for macOS and Linux only.

## E5. Flags for restricted sessions

- **Documented.** `--tools` restricts built-in tools (`""` disables all),
  `--strict-mcp-config` keeps only `--mcp-config` servers, `--setting-sources`
  selects `user`, `project`, `local`, `--settings` overrides given keys only.
  Not exercised yet; needed from v0.3 (triage session) and v0.5
  (orchestrator).

## E6. Long-lived token

- **Documented.** `claude setup-token` prints a one-year OAuth token after a
  browser authorization flow and does not store it; it is consumed through
  `CLAUDE_CODE_OAUTH_TOKEN`. It must be generated on a machine with a browser
  and injected into a container. Terms of use for scripted sessions remain to
  check (framing open point, before v0.6).

## E8. Process chain of an interactive session in VS Code

- **Verified (Windows 11, VS Code).** Two interactive sessions open in two
  different VS Code windows (different repositories) both walk up
  `claude.exe` -> `bash.exe` x3 -> `Code.exe` (no window) -> `Code.exe`
  (main process). The main process reports a single `MainWindowTitle`, the
  title of one of its windows, not of the session's window: the session in
  repository A reported the window of repository B.
- Consequence: the focus action must enumerate every top-level window of the
  main editor process and match the title against the session's folder name.
  "First window of the ancestor" is wrong in the normal multi-window case.
- **Verified.** The native install puts `claude.exe` in `~/.local/bin`.

## E9. Hook cost and idle cost (2026-10-27, release build)

- **Verified (workstation).** 200 sequential `officina hook` runs with a
  200 KB payload: p95 19.5 ms. An executable that does nothing (the test
  fake answering `--version`) has a p95 of 21.6 ms on the same machine: the
  hook's own cost is lost in the noise of Windows process creation.
- **Verified (GitHub hosted Windows runner).** The same hook measures 82 ms
  at p95: hosted runners start processes several times slower. CI therefore
  gates the hook's cost over an empty process, not the absolute figure.
- **Verified (workstation).** 16 processes x 500 concurrent hook runs
  produced 8000 parseable lines, none interleaved.
- **Verified (workstation).** The board window with two live sessions:
  35 MB working set, 0.013 % of the machine's CPU over 30 s idle.
- **Verified (workstation, Claude Code 2.1.311, 6 real `claude -p` turns,
  timings from `--debug-file`).** Through Claude Code's own shell, `officina
  hook` takes 38-50 ms on `UserPromptSubmit`, 39-52 ms on `Stop` and
  71-107 ms on `SessionStart` (first spawn of the turn). In the same turns
  the project's Python hooks took 87-118 ms (`UserPromptSubmit`) and
  122-187 ms (`SessionStart`), and a hook that fetches git 1.1-1.4 s.
- **Verified (debug log).** Claude Code runs the hooks of one event in
  parallel: the event waits for its slowest hook. Next to an existing Python
  hook, `officina hook` adds no wait at all; alone, it adds about 40 ms per
  event. The PRD's 30 ms target holds for the binary (E9, first bullet), not
  for the shell hop Claude Code adds on Windows.

## E10. What changes on disk when a session changes state

- **Verified (2.1.311, stat only, contents never read).** In the Claude
  sessions directory, each live session has a `<pid>.json` (and a `.key`
  file). The session's `.json` is rewritten on every state transition
  (`busy` to `idle`, to `waiting`), while the directory's own modification
  time changes only when a session appears or ends.
- **Verified.** One `claude agents --json --all` call takes about 210 ms
  and 200 ms of CPU on the workstation: polling it every second would cost a
  fifth of a core.
- **Verified (release build, end to end).** With the collector stat'ing each
  session `.json` every 250 ms and spawning `claude agents` on change, a
  session entering a permission prompt reaches the window title 329 ms after
  its file was rewritten. The window process used no measurable CPU over 30 s
  idle.

## E11. What Claude Code gives a stdio MCP server

- **Verified (Claude Code 2.1.312, a spy server started through
  `--mcp-config`).** The first request is `server/discover`, carrying
  protocol `2026-07-28` in `_meta`. A server that answers it with JSON-RPC
  error `-32601` (method not found) gets the classic handshake next:
  `initialize` with `protocolVersion: "2025-11-25"`, then
  `notifications/initialized`, `tools/list`, `tools/call`. Tool calls carry
  `_meta.claudecode/toolUseId` and a `progressToken`.
- **Verified.** The server runs in the session's project folder and its
  environment carries `CLAUDE_CODE_SESSION_ID` (the session id the board
  already knows), `CLAUDE_PROJECT_DIR` and `CLAUDE_PID`. It also carries
  Claude Code's internal messaging socket and token: a server must never log
  its environment wholesale.
- **Verified.** Claude Code updated itself from 2.1.311 to 2.1.312 during the
  day, without action from the author: the contract canary exists for this.

## E12. PreToolUse decisions and permission modes

- **Verified (2.1.312).** A `PreToolUse` hook on `Bash` answering
  `permissionDecision: "deny"` blocks the command in every mode tried,
  `bypassPermissions` included, and the session reports the hook's reason.
- **Verified, background sessions.** `permissionDecision: "ask"` raises a
  real permission prompt (`status: waiting, state: blocked, waitingFor:
  "permission prompt"`). In `default` mode the prompt held for the 60 s
  observed and the command did not run. In `bypassPermissions` mode the
  prompt resolved by itself after 8 to 22 s and the command ran, twice out of
  two. What resolved it is unknown.
- Consequence: a human approval gate cannot rely on `ask`. It must deny, keep
  the request in Officina, and let the same call through once the author
  approved it in the window.

## E7. Configuration directory

- **Documented.** `CLAUDE_CONFIG_DIR` redirects settings, session history and
  credentials; an absolute path is required. The sessions directory Officina
  watches is `<config dir>/sessions`.
