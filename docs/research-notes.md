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
  Verified for the triage call in E16 (`--tools ""`, `--strict-mcp-config`,
  `--mcp-config`, `--setting-sources ""`); the orchestrator's use (v0.5) is
  not exercised yet.

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

## E13. Running commands through WSL

- **Verified (WSL 2.7.14, Debian).** `wsl.exe -d <distro> --exec <program>
  <args...>` started with an argument vector (MSVC quoting, as Rust and
  Python build it) delivers every argument intact to the Linux program:
  spaces, double quotes, `$HOME`, `*`, `;`, backslashes, a trailing
  backslash, accented letters. With `--exec` no shell interprets anything.
- **Verified.** Killing the Windows `wsl.exe` process ends the Linux process
  it started: a Job Object around `wsl.exe` is enough to stop a timed-out
  action.
- **Verified.** `CLAUDE_CODE_SESSION_ID` is set in the environment of the
  commands a session runs through its Bash tool, so `officina run` can record
  which session called it.
- **Verified (2026-10-28, WSL 2.7.14, Debian, Claude Code 2.1.312 on the
  workstation; story 2.10, D-6).** Through `officina run`, a `wsl` action of
  a project whose folder holds accented and mixed-case letters and spaces
  (`C:/path/to/Équipe Réseau/Projet Mixte`) receives that folder as given in
  `--cd`, and the Linux program starts in
  `/mnt/c/path/to/Équipe Réseau/Projet Mixte`; a value with accented letters
  and a space (`Fichier été.txt`) reaches it as one argument. In the same
  distro `wsl.exe -d <distro> --exec /usr/bin/test -x <path>` exits 0 for an
  executable, 1 for a missing path or a file that is not executable, and
  `wsl.exe` itself exits -1 when the distro does not exist: doctor's probe
  reads these three cases.
- **Not run yet (story 2.10 AC 10).** A WSL `ansible-playbook` check and
  apply on a lab group through `officina run`, and PRD journey 2 run start to
  finish without the author pasting anything, need the author's lab and
  work sessions; listed in `docs/backlog.md`.

## E14. The claude process id in hooks

- **Verified (2.1.312).** Every hook process receives `CLAUDE_PID`, the id of
  the `claude` process that runs the session, and `CLAUDE_CODE_SESSION_ID`.
  `CLAUDE_PID` equals the `pid` that `claude agents --json` reports for that
  session (checked on a live interactive session).
- Consequence: the hook records `claude_pid` (reserved in event lines since
  schema 1), and the board attaches events to a live card by process as well
  as by session id. Whatever `/clear` does to the session id reported by
  `claude agents`, the events of the cleared session stay on the live card
  instead of surfacing as an "interrupted" ghost. The `/clear` question of
  AD-11 no longer needs an answer.
- Not verifiable here: driving `/clear` in a background session. `claude
  --bg --resume <id>` on a running session starts a copy instead.

## E15. The PreToolUse payload and the working directory

- **Verified (2.1.312, `claude -p` with a logging `PreToolUse` hook).** The
  payload of a `Bash` call carries exactly: `cwd`, `effort`,
  `hook_event_name`, `permission_mode`, `prompt_id`, `session_id`,
  `tool_input` (`command`, `description`), `tool_name`, `tool_use_id`,
  `transcript_path`. `cwd` uses Windows backslashes.
- **Verified.** `cwd` follows the shell: after a call running `cd crates`,
  the next call's `cwd` ends in `\crates`. `CLAUDE_PROJECT_DIR`, in the hook's
  environment, stays the session's project root (forward slashes).
- **Verified (E12).** The deny that holds in every mode is the hook printing
  `{"hookSpecificOutput": {"hookEventName": "PreToolUse",
  "permissionDecision": "deny", "permissionDecisionReason": "..."}}` on
  stdout and exiting 0.
- Consequence: a repository is guarded for a call if the project of
  `CLAUDE_PROJECT_DIR` or the project of `cwd` is guarded, so `cd ..` does
  not leave containment.

## E16. A contained model call for triage

- **Verified (2.1.312, Haiku, from an empty folder).** `claude -p <prompt>
  --model haiku --tools "" --strict-mcp-config --mcp-config
  '{"mcpServers":{}}' --setting-sources "" --output-format json
  --json-schema <schema>` runs and returns the constrained answer in
  `structured_output` (with `result` holding the same JSON as text).
- **Verified.** Asked to run a shell command, that call has no tool: it
  wrote a fake tool call and a fake result in its text, ran nothing (one
  turn, no file created). Model text is untrusted even when it describes
  actions; only the schema-constrained field may be read.
- **Verified.** The prompt can come on stdin (`claude -p` with no prompt
  argument), so untrusted excerpts never reach a command line, and
  `--no-session-persistence` writes no transcript under the config dir (the
  run's session id has no file afterwards). With `--setting-sources ""` the
  user-level hooks do not load either, so the triage run does not record
  itself through `officina hook`.
- **Measured.** One triage proposal on a short excerpt: about 4.5k input
  tokens (system prompt, cached for an hour) and 380 output tokens (260 of
  thinking), USD 0.0011 at API prices.
- **Verified with Rust's argument vector (2026-10-28, 2.1.312, story 3.9
  AC 13).** `examples/triage_probe.rs` ran one real call with the argument
  vector as `Command` quotes it (the empty `--tools` and `--setting-sources`
  values passed as separate arguments, no shell): exit 0 in 3.66 s; the
  answer was schema-valid (`finished`, with a coherent reason); 4,581 input
  tokens (1 % from the 4,500 above, tolerance 20 %), 266 output tokens,
  1,049 micro-USD; no transcript under the config dir for the session id the
  call reported, so `--no-session-persistence` holds. PASS.

## E17. Notification hooks, and what any claude process writes

- **Verified (2.1.312, background session in `default` mode asked to use
  Write).** While the session reads `waiting / blocked / permission prompt`,
  the `Notification` hook fires with `notification_type:
  "permission_prompt"`, `message: "Claude needs your permission"`, and the
  fields `cwd`, `hook_event_name`, `prompt_id`, `scratchpad_dir`,
  `session_id`, `transcript_path`. E2's documented shape holds.
- **Observed.** `~/.claude.json` changes continuously while any `claude`
  process runs (every interactive session updates it), so a write by one
  call cannot be isolated on a live workstation. Officina's guarantee is
  that Officina itself never writes under the Claude config dir or into
  `~/.claude.json`; the `claude` processes it starts (`claude agents`,
  `--version`, the triage call) maintain their own state like any session.
  The zero-write tests run against the fake claude.

## E18. When a hook fails

- **Documented** (hooks reference, https://code.claude.com/docs/en/hooks,
  and changelog 2.1.295, https://code.claude.com/docs/en/changelog, read
  2026-10-28). On most events a hook that fails or times out does not
  stop the action. On `PreToolUse` a timed-out hook "doesn't block the tool
  call", and "don't count on a stalled hook to act as a gate".
- A failure is any of these:
  - the hook cannot start;
  - it exits with a code other than 0 or 2, even after printing JSON that
    allows the call;
  - it reaches its `timeout` (default 600 s for a `command` hook), which
    cancels it and discards its output;
  - its JSON cannot be parsed or fails validation.
- Exit 0 with no output is no decision: the normal permission flow applies.
  Exit 2 blocks a `PreToolUse` call, even with `permissionDecision: "allow"`
  in the JSON.
- `"onFailure"`, on a `command` or `http` hook entry (the handler object,
  next to `type` and `command`, not the matcher group), takes `"continue"`,
  the default, or `"block"`. It requires Claude Code 2.1.295 or later. With
  `"block"` a failure does what exit 2 does on that event, so on
  `PreToolUse` it blocks the call; on `PermissionRequest` it denies. It has
  no effect on `Stop`, `SubagentStop`, `TaskCompleted`, `TeammateIdle` or
  background (`async`) hooks.
- Consequence (D-452): the gate's entry carries `"onFailure": "block"`,
  and the hook itself prints a deny, never exit 2, on its own fault in a
  guarded project. The owner's 2.1.312 honours the field. Not verified on
  the workstation: a crash or timeout of the real hook under Claude Code.

## E19. `--bg`, trust and agent view (v0.5, addendum items 1 to 3)

Story 5.1 AC 2, D-125. **Verified** by the developer agent on 2026-10-29,
Claude Code 2.1.312, Windows 11, no model turn. An empty scratch folder
under the user's temporary folder, outside every trusted folder; every
name of the session-variables entry (E84) and every `OFFICINA_*` name
removed from the child, spawned from Python's `subprocess` with list
arguments and no console.

- `claude --help` exits 0 and still shows `--permission-mode <mode>` with
  `dontAsk` among its choices, `--disallowedTools`, `logs <id|name>` and
  the `--bg` sentence "With --resume <session-id>, continues that session
  in the background under the same ID, or starts a copy and says so when
  the session is already running".
- `CLAUDE_CODE_DISABLE_AGENT_VIEW=1 claude --bg --name probe -- x`: exit 1
  in 0.14 s, stderr `'--bg' is disabled by CLAUDE_CODE_DISABLE_AGENT_VIEW.`,
  nothing started.
- `claude --bg --name probe -- x`: exit 1 in 0.51 s, stderr
  ``Workspace not trusted. Run `claude` in <folder> once and accept the
  trust prompt, then retry.``
- `CLAUDE_CODE_DISABLE_AGENT_VIEW=1 claude agents --json --all`: exit 1,
  empty stdout, stderr `'claude agents --json' is disabled by
  CLAUDE_CODE_DISABLE_AGENT_VIEW.` It lists nothing, so the orchestrator's
  MCP server, which inherits the variable (AD-80), cannot list sessions
  through `claude agents`.
- `claude agents --json --all` without the variable listed the same rows
  before and after, and the folder stayed empty.
- **Trust is not inherited by nesting.** A plain folder under the
  repository's git worktree (in its ignored `target/`) passed the trust
  check without having been trusted itself; a folder inside a git
  repository created under that same worktree failed it, and so did the
  temporary folder. Claude Code's project list holds the main checkout,
  not the worktree, so trust follows the folder's git root, a worktree
  resolving to its main repository (inferred from these three cases, not
  documented). Claude Code recorded no trust entry for any scratch folder.
- **`--bg` does not validate options.** In that trusted folder,
  `claude --bg --zzz-unknown -- x` exited 0 (`Starting background
  service…`) and started a session that ended `failed`; `claude logs` on
  it exited 1 (`Couldn't read logs ... connect ENOENT` on the supervisor's
  control pipe, the supervisor having exited). Removed with `claude stop`
  and `claude rm`. An unknown option after `--bg` is a launch, not a usage
  error: the `WorkOrder` vector must be exact, as AD-79's golden vectors
  hold it.
- Outcome: items 1 to 3 hold on 2.1.312 as the addendum states. Cost:
  none.

## E20. The bare `--` (v0.5 gate 2)

Story 5.1 AC 3, AD-96 row 2, D-125, D-127. **Verified** by the developer
agent on 2026-10-29, 2.1.312. A scratch folder under the worktree's
ignored `target/` (trusted through the main checkout, E19), not a git
repository, whose `.claude/settings.local.json` the agent wrote (the
owner allows it for a scratch folder) with one `UserPromptSubmit` hook: a
Python script writing the payload's `prompt` as JSON (every byte kept),
its `session_id` and the names of its environment to one file per
prompt, and blocking the prompt while a flag file exists, so these runs
cost no model turn. Spawned from Python's `subprocess` with list
arguments (MSVC quoting, as Rust's `Command`), the session-variables
entry's names removed, `OFFICINA_*` kept (none set),
`NoDefaultCurrentDirectoryInExePath=1`. The four cases of addendum
section 1 item 5: (1) a leading dash, double quotes, an escaped quote, a
newline and a tab; (2) `100%`, `^`, `&`, `|`, `<`, `>`, `!`, `$HOME` and
`*.txt`; (3) accented letters, `C:\path\to\dir\` and a trailing
backslash; (4) inner `--`, `--bg` and `--model opus`.

| Form | Cases 1 to 4 |
|---|---|
| `claude --bg --name probe-<n> --model haiku -- <case>`, stdin null, stdout and stderr piped, no console | byte for byte, each |
| `claude --resume <session id> -- <case>`, new console, working directory the session's folder | byte for byte, each, in the same session |
| the same, working directory the `claude` binary's folder | not delivered: trust dialog, each |
| `claude -p --model haiku -- <case>` (item 5, re-run) | byte for byte, each |

- **`--bg`'s stdio.** Exit 0 in 0.58 to 1.36 s; both pipes reached end of
  file at the exit, within 20 ms, in all four cases: no process holds
  them. stdout: `backgrounded · <id> · <name>` then four lines naming
  `claude agents`, `claude attach <id>`, `claude logs <id>` and
  `claude stop <id>`; stderr empty, except `Starting background service…`
  on the call that started the supervisor. The supervisor's parent is
  `WmiPrvSE.exe`: `claude --bg` starts it through WMI, so it inherits no
  handle of the caller and sits in no job of the caller. It exits by
  itself once no background session remains.
- Right after a blocked first prompt the listing read `status: idle`,
  `state: working`; after a blocked resumed prompt, `state: blocked` with
  no `waitingFor`.
- **Resume from the binary's folder.** From a new console whose working
  directory is the `claude` binary's folder, as AD-79's `Answer` vector
  does for a live background session, `claude --resume` first shows
  Claude Code's trust dialog for that folder ("Accessing workspace:
  <binary folder>", "No, exit" selected, "Yes, I trust this folder"). With
  nobody answering, nothing was delivered in 45 s and the session's state
  did not change (the console's text read from its screen buffer, then
  the console killed). That folder is not trusted on this workstation, and
  the agent trusts no folder.
- **Resume from the session's folder.** The console printed `Sending your
  prompt to the background session (<id>)…`, then `Sent your prompt to
  the background session (<id>); opening it…`, and attached; the hook
  received the case with the same `session_id`, and the listing kept one
  row per session (no copy).
- **Item 4, re-run.** `claude -p --resume <unknown id> "--zzz probe"`
  exits 1 with `error: unknown option '--zzz probe'`; with `--` before the
  prompt, exit 1 with `No conversation found with session ID: <id>`.
- **Outcome** (D-127): `--` holds for both vectors. The `WorkOrder`
  vector keeps `--`; the `Answer` vector keeps `--` and needs no
  `AnswerRefusal::LeadingDash`. New fact for AD-79 and story 5.11: the
  `Answer` vector's working directory for a live background session
  cannot be the binary's folder while that folder is untrusted, since the
  resume then stops at the trust dialog; from the session's folder the
  delivery works. D-127 names no branch for this: the choice (the
  session's folder, or the owner trusting the binary's folder once) is
  the lead's and the owner's.
- Every probe session was stopped and removed; `claude agents --json
  --all` listed the owner's same rows before and after. Cost: no model
  turn.

## E21. Resume delivery (v0.5 gate 3)

Story 5.1 AC 4, AD-96 row 3, D-127. **Verified** by the developer agent
on 2026-10-29, 2.1.312, Haiku, in E20's scratch folder with the hook
recording without blocking. The resumes ran in a new console the agent
started (Python, `CREATE_NEW_CONSOLE`) whose working directory was the
session's folder (E20: the binary's folder stops at the trust dialog);
nothing was typed, and the console's text was read from its screen
buffer.

- `claude --bg --name probe-g3 --model haiku -- "Reply with the word
  ready."`: exit 0; `idle` and `done` 5 s later.
- `claude --resume <sessionId> -- "Reply with the word delivered."`: the
  hook received the prompt in that session; the listing read `busy` /
  `working` at 2.1 s and `idle` / `done` at 4.3 s, one row throughout, no
  copy. The resume process stayed attached (alive after 30 s) until the
  agent killed it, which left the session `idle` / `done`. The console
  line, as E20 read it: `Sent your prompt to the background session
  (<id>); opening it…`.
- A second session asked to use `AskUserQuestion` read `waiting` /
  `blocked` / `waitingFor: input needed` 9 s after its start.
  `claude --resume <sessionId> -- "Reply with the word delivered."`, kept
  on screen with `cmd /k`, printed ``That session is running in the
  background (<id>). Your prompt was not sent to it: it is waiting on
  your answer to a question. Run `claude attach <id>` to open it, or
  `claude stop <id>` first to resume it here. Add --fork-session to branch
  off a copy instead.`` and exited 1 within 2 s; the hook received
  nothing; the state stayed `waiting` / `blocked` / `input needed`; no
  copy.
- **Outcome: pass, both halves**, from the session's folder.
  `meeting::delivery` may return `ResumeLive` (D-127); a session waiting
  on a question gets the documented refusal and needs `attach`. Its
  `waitingFor` reads `input needed`, not `permission prompt`.
- Probe sessions stopped and removed; listings before and after equal.
  Cost: three short Haiku turns in background sessions, which report no
  cost; well under USD 0.01 by E16's figures.

## E22. Work order permissions (v0.5 gate 4)

Story 5.1 AC 5, AD-96 row 4, D-126, D-127, D-573.

- **Verified (agent half, 2026-10-29, 2.1.312): a `--bg` session takes
  the supervisor's environment, not the invoker's.** With the supervisor
  already running, started by a `claude --bg` whose environment carried a
  marker name `O5_INVOKER_MARK`, a new `claude --bg` whose environment
  carried only `O5_INVOKER_MARK2` started a session whose
  `UserPromptSubmit` hook saw `O5_INVOKER_MARK` and not `O5_INVOKER_MARK2`
  (E20's hook, prompt blocked, no model turn). A session's environment is
  the one the supervisor started with, from whichever `claude --bg`
  started it; since the supervisor exits once no background session
  remains (E20), the next `--bg` starts it with its own. Not tested:
  whether the session-variables entry's names reach later sessions when
  the invoker that starts the supervisor holds them.
- Besides E84's names, a background session's hook also sees
  `CLAUDE_JOB_DIR`, `CLAUDE_CODE_ALT_SCREEN_FULL_REPAINT`, `COLUMNS`,
  `LINES`, `BROWSER` and `INVOCATION_ID`; it does not see `TERM_PROGRAM`
  or `VSCODE_GIT_IPC_HANDLE`, which the supervisor's invoker held (the two
  names checked).
- The agent's probe sessions ran in the owner's default mode: the
  attached console's footer read `bypass permissions on`, and
  `claude logs` prints the same footer (E23).
- **Outcome so far** (D-127): the branch "a `--bg` session takes the
  supervisor's environment" holds. D-128's environment rule cannot reach
  a work order while a supervisor runs; story 5.7 records it and the
  README states it.
- **Owner half: run 2026-10-30, recorded in E88**: the free project
  passed, the project whose own settings set `acceptEdits` stopped at a
  permission prompt. The procedure it followed: from the repository root,
  in a console of the owner's own: `cargo build -p officina-host --features
  window-authority --example gates_v05`, then
  `target\debug\examples\gates_v05.exe gate4`.
  It asks for one free and one guarded project, starts the exact
  `WorkOrder` vector in each (the floor's `CLAUDE_CONFIG_DIR`,
  `NoDefaultCurrentDirectoryInExePath=1`, agent view on, `OFFICINA_*`
  kept), records `claude agents --json` over 60 s, the probe file and the
  names its Bash lists, and the mode footer from `claude logs`, then asks
  before removing the file and the session. Report:
  `target/gates-v05/report-gate4.md`. It runs again once the owner
  switches his default mode (D-573).
- **Window half: passed with findings, run 2026-10-30, recorded in E88**
  (story 5.7 AC 11, D-1003): one card launched from the Orchestrator view
  with the mouse started in the background and its row bound 1.8 s after
  the launch; the probe's Bash listed Officina's `OFFICINA_*` names. Its
  three findings are fixed by D-1000 to D-1002. The branch above is
  recorded in epic-5-decisions.md D-671 and stated in the README.

## E23. `claude logs` (v0.5 gate 7)

Story 5.1 AC 6, AD-96 row 7, D-126. **Verified** by the developer agent
on 2026-10-29, 2.1.312, on gate 3's first session after its two turns:
working directory the binary's folder (as the `Logs` kind), stdin null,
stdout redirected to a file, no console.

| Environment | Exit | Elapsed | Bytes | ESC bytes |
|---|---|---|---|---|
| agent view on | 0 | 0.89 s | 128,131 | 4,606 |
| `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` | 1 | 0.99 s | 0 | 0 |

- With the variable, stderr `'logs' is disabled by
  CLAUDE_CODE_DISABLE_AGENT_VIEW.`
- The output is the session's rendered screen with its footer
  (`bypass permissions on (shift+tab to cycle)`), so it also tells a
  session's permission mode.
- `claude logs` needs no trusted folder. It fails once the supervisor has
  exited (E19).
- **Outcome: pass** (exit 0 in under 2 s): story 5.10 keeps the `Logs`
  kind; readability after AD-84's stripping is judged there.
- **Readability, agent check (story 5.10, 2026-10-29, 2.1.312).** One
  Haiku background session in a scratch folder under the worktree's
  `target/` (two short turns: three colours, then a shell command exiting
  3), read through `crates/officina-host/examples/logs_probe.rs`, the exact
  `Logs` vector: exit 0 in 0.56 s, 169,204 bytes, 6,549 ESC bytes; all
  6,549 sequences removed, 65,332 bytes kept and 75,523 dropped from the
  start. What remains is the session's rendered screen as text: the
  prompt line, the answer, `Ran 1 shell command` and its summary, the
  `Crunched for 12s` line and the mode footer, repeated once per repaint,
  with box-drawing rules and the logo's block characters; no escape byte
  and no stray control character. Blank padding lines took most of the
  kept bytes, so the meeting now squeezes trailing spaces and runs of
  blank lines (D-806). No error marker matched that output (the shell's
  exit 3 prints none). The session was stopped and removed; nothing left
  behind. Cost: two Haiku turns, well under USD 0.01.
- **Readability, owner check (2026-11-01, 2.1.316): fail as built**
  (E100): the stripped text repeated the whole screen once per repaint.
  The meeting now plays the output on a minimal screen and keeps the
  final one (D-1020); the owner's rerun on one of his background sessions
  is pending (D-1023): `cargo run -p officina-host --example logs_probe --
  <id>`, the id from `claude agents --json`; readable closes story 5.10
  AC 4, unreadable applies D-126.

## E24. The orchestrator's flag set (v0.5 gate 1)

Story 5.1 AC 7, AD-96 row 1, D-126, D-164, D-561.

- **Verified (agent half, 2026-10-29, 2.1.312).** AD-89's exact flag set,
  with a stub stdio server named `officina` (Python, in a scratch folder),
  started by the agent in a new console in an empty, untrusted workdir
  under the temporary folder whose parent holds a marker `CLAUDE.md`, with
  exactly D-561's environment: the allow list's names found in the
  environment (as `registry::triage_call::triage_env` builds them, the
  names taken from the companion's dry run), `NoDefaultCurrentDirectoryInExePath=1`
  and `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` (no `CLAUDE_CONFIG_DIR`: the
  owner's floor sets none). It starts with no missing name reported, and
  Claude Code shows its trust dialog for the workdir first; the stub
  server had not started while the dialog waited. The agent answered
  nothing and killed the console. So far `ORCHESTRATOR_ENV_EXTRA` stays
  empty (D-561).
- **Print mode, items 6 to 8:** as the addendum's section 1 records them
  (2026-10-28); E84 adds that a print-mode MCP server holds
  `CLAUDE_PROJECT_DIR` and the messaging socket and token, but not
  `CLAUDE_PID`.
- **Owner half (2026-10-29, 2.1.312).** Run by the owner with
  `gates_v05 gate1` (report `target/gates-v05/report-gate1.md`): AD-89's
  exact flag set with the stub server named `officina`, started in a new
  console in an empty workdir under the temporary folder whose parent
  holds the marker `CLAUDE.md`, environment exactly D-561's allow list
  plus `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` (no extra name). Against AD-96
  row 1's pass column:
  - Trust dialog: shown once for the workdir; the owner accepted it. The
    session then started with the first prompt as its first turn.
  - `/mcp`: only `officina`. **Pass.**
  - `/context`: only `mcp__officina__*` tools, as the owner read it; the
    tool names were not copied into the report. **Pass**, names not
    recorded.
  - Memory files: the marker `CLAUDE.md` in the workdir's parent was not
    listed and no answer started with its word. Memory files from the
    parent folder do not load with this flag set, so D-164's refusal
    branch for `<home>/orchestrator/` is not needed.
  - `/status` permission mode `dontAsk`: **deviation**, the owner read
    `Manual`. Explained by the agent's follow-up below: the session runs
    in `dontAsk`; `Manual` is the settings default, not the session's
    mode.
  - `/config`: no row "Messages from your other sessions". **Pass.**
  - Read a file, run `hostname`, message another session: no prompt
    appeared and nothing ran. **Pass.**
  - Token test: a second interactive session's `/list-agents` showed
    `officina-orchestrator`; asked to send it a unique token, it reported
    the send refused (`session is not accepting cross session messages`).
    The token did not appear in the orchestrator's transcript viewer and
    no held-message dialog opened. **Pass**; the rerun without
    `CLAUDE_CODE_DISABLE_AGENT_VIEW` was not needed.
  - `claude agents --json`: the orchestrator listed as `interactive`,
    `idle`, its `cwd` the workdir. **Listed**: AD-89's pid fallback is not
    needed for presence.
  - Start again from the window: not part of the companion; story 5.16
    AC 10 (window half).
  - Listings before and after held the owner's same rows; no session
    left.
- **The first turn ran no tool.** The owner answered that the briefing
  tool ran, but the stub's log records `server/discover`, `initialize`,
  `notifications/initialized` and `tools/list`, and no `tools/call`. The
  agent reproduced it (below): the first prompt's API request carries an
  empty tool list, and the model wrote a tool call and its result as
  plain text. **Deviation**: the first briefing is not a tool result.
- **The MCP server's environment.** On top of D-561's allow list and
  `CLAUDE_CODE_DISABLE_AGENT_VIEW`, the stub server held Claude Code's own
  session variables: `AI_AGENT`, `CLAUDECODE`, `CLAUDE_CODE_ENTRYPOINT`,
  `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_MESSAGING_TOKEN`,
  `CLAUDE_CODE_SESSION_ID`, `CLAUDE_PROJECT_DIR`, plus
  `COREPACK_ENABLE_AUTO_PIN` and `SHELL` (names only, values never read).
  Claude Code adds them; Officina cannot remove them from the vector.
  `officina mcp` inherits the orchestrator's messaging socket and token
  and must never use them (R38).
- **Agent follow-up (2026-10-29, 2.1.312, Haiku).** The same vector with
  `--name probe-e24-<x>` and `--model haiku`, started by the agent in a
  new console in the owner's trusted gate workdir, the parent session's
  variables removed, `--settings` extended with `UserPromptSubmit`,
  `PreToolUse` and `Stop` command hooks recording the payload's
  `permission_mode`; the console's screen read from its buffer, and
  `/status` and `/config` typed into that console's input buffer.
  - Effective mode: every hook payload read `dontAsk` (`UserPromptSubmit`,
    `Stop`, and `PreToolUse` for `mcp__officina__briefing`); the
    transcript's `permission-mode` entries and each user entry read
    `dontAsk`; the status bar read `⏵⏵ don't ask on (shift+tab to
    cycle)`.
  - `/status` opens the settings dialog on its Status tab, whose rows are
    version, session name and ID, session kind, peer address, `cwd`,
    login, model, MCP servers, `Setting sources: Command line arguments`
    and auto mode server: no permission mode row. The dialog's Config tab,
    filtered on "permission", shows one row: `Default permission mode
    Manual`.
  - With `"permissions": {"defaultMode": "acceptEdits"}` added to
    `--settings` (prompt blocked by the hook, no model turn), that row read
    `Accept edits` while the hooks and the status bar still read
    `dontAsk`. The row shows the settings default, which with
    `--setting-sources ""` and no `defaultMode` key is the built-in one,
    `Manual` (the permission modes page: `default` is labelled Manual);
    `--permission-mode` overrides it for the session, as the CLI reference
    states. Not run: the vector without `--setting-sources ""`, which
    would load the owner's user hooks.
  - First turn: the request was sent 0.2 s after the prompt, before the
    stub server process had started, with `tools: []`; the model then
    wrote a tool call as text (one run) or nothing useful (another). A
    second prompt typed 8 s later had `mcp__officina__briefing` in its
    request, called it with no prompt, and the stub recorded `tools/call`.
    The MCP page says a request that needs a still-connecting server
    waits inside `ToolSearch` or `WaitForMcpServers`; `--tools ""` removes
    both (inferred, not tested).
  - From the second request on, the tool list sent to the API also held
    `DeferredToolPlaceholder` (deferred, "never call this tool") and a
    server-side `advisor` tool (deferred, model Opus), which `/context`
    did not show to the owner. It runs nothing on the workstation; the
    environment variables page documents
    `CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1` to remove it (not tested).
  - The workdir stayed empty; every probe console was killed with its
    process tree; `claude agents --json --all` listed the owner's same
    rows before and after. Cost: four Haiku requests over two sessions,
    the rest with the prompt blocked by the hook.
- **Outcome.** Containment passes: one MCP server, only Officina's local
  tools, no prompt and nothing run for read, run and message requests,
  inbound messages refused, the session listed, no memory file loaded.
  The `/status` item is a wrong check, not a failure: the session runs in
  `dontAsk`, which the status bar shows; AD-96 row 1 and story 5.16 AC 10
  read the status bar instead. The first turn fails: the first prompt
  reaches the model before the `officina` server connects, so story 5.16
  changes how the briefing is first asked for. `ORCHESTRATOR_ENV_EXTRA`
  stays empty (D-561).
- **Window half (owner, 2026-11-01, 2.1.316): pass** (story 5.16 AC 10,
  D-1022). Run with `gates_v05 gate1-window` on the installed window built
  from `7ce75d2` (report `target/gates-v05/report-gate1-window.md`, not
  committed), the marker `CLAUDE.md` in `<home>/orchestrator/` during the
  run:
  - Start: Claude Code's trust dialog for the workdir appeared in the new
    console and the owner accepted it; Start's answer beside the button
    said it started and to type a first message.
  - `/mcp`: one server, `officina`, connected.
  - `/context`: exactly the tools `McpTool::ALL` names, each
    `mcp__officina__*` and no other; no `advisor` tool (D-692); the marker
    memory file not listed (D-164).
  - The status bar read "don't ask on" (D-690); `/config` had no inbound
    messages row.
  - The typed first message: the briefing tool was called, a tool call
    shown in the console, before the answer (D-691); the board tool named
    the sessions live at that moment (review F4); no answer started with
    the marker's word.
  - `claude agents --json` listed `officina-orchestrator` in the workdir;
    the window's Orchestrator button showed the filled dot and the view
    said Live.
  - A second Start brought the orchestrator's console to the front, the
    window saying it already runs, and no second console opened.
  - `claude agents --json --all` held the owner's same rows before and
    after; the marker was removed.

## E25. Memory files in the review call (v0.5, story 5.13)

Story 5.13 AC 14, D-164, D-126. **Verified** by the developer agent on
2026-10-29, Claude Code 2.1.312, with
`crates/officina-host/examples/review_probe.rs`: one real review of an
`auto` request, the exact `ReviewCall` vector (`-p --model haiku --tools ""
--strict-mcp-config --mcp-config {"mcpServers":{}} --setting-sources ""
--settings {"crossSessionInbound":"refuse"} --no-session-persistence
--output-format json --json-schema <review schema>`, the request on stdin
between nonce markers), run by `relay::review::run_call` in
`<home>/review/empty` with the contained allow list and
`CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1`, the probe's home a fresh folder under
the worktree's ignored `target/`. A marker `CLAUDE.md` in `<home>/review/`,
the folder's parent, asked every reason to start with one capitalised
word.

- The call answered in 6.3 s, schema-valid: decision `escalate`, reason
  "Dry run passed and the prompt asks for this apply, but last_outcomes is
  empty, so no prior run succeeded. Needs human review." No marker word:
  the memory file of the folder's parent did not load with this flag set,
  as E24 found for the orchestrator's.
- `CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1` (D-692) did not keep the call from
  starting or answering.
- **Outcome** (D-164's first branch, D-127): no flag is added to the
  vector, and `workdir_not_empty` stays about the folder itself; nothing
  outside `review/empty` is refused.
- Cost: one Haiku call, USD 0.0015 (`total_cost_usd`).

## E80. The taskbar overlay badge and the attention request (v0.4 gate 2)

Story 4.1 AC 2, AD-178 gate 2, numbered after the ranges Epics 5 to 7
reserve (E19 to E79). Run 1 on 2026-10-28 with the probe of story 4.1
(`cargo run -p officina-app --example taskbar_probe`), which asks the owner
at each step and writes its report; Windows 10.0.26200.9711, Tauri 2.12.5,
100 % display scaling, light taskbar.

| Badge (run 1 design) | Size | Number readable | Alert colour distinct from amber |
|---|---|---|---|
| amber, 3 x 5 font, one pixel per cell | 16 px | with effort | - |
| alert colour, same font | 16 px | with effort | clearly |
| amber | 24 px | with effort | - |
| alert colour | 24 px | with effort | clearly |
| amber | 32 px | not readable | - |
| alert colour | 32 px | not readable | clearly |

- The probe's colour question was worded "Is it clearly different from the
  amber one you just rated?"; the owner answered "not different" about the
  number's legibility, and corrected it: the alert colour was clearly
  different at every size. The question now asks about the colour only.
- Request while another window had focus (`UserAttentionType::Informational`):
  the button flashed until he clicked it (back after 6 s). Request while the
  probe had focus: nothing changed. This half passes, as documented.
- Reading: Windows draws the overlay at the small icon's size, 16 px at
  100 %, so the larger images are scaled down and blur; at 16 px, one-pixel
  strokes are too thin. Outcome: epic-4-decisions.md D-523 (one size, 16 px
  times the display scale; digits at twice the font's cell; a `!` mark on
  the alert badge).

Run 2 on 2026-10-28, the D-523 design, same machine, Windows build, Tauri
version, 100 % scaling and light taskbar (the probe's report says "run 1":
it was written beside another build of the probe):

| Badge (D-523 design) | Size | Readable | Alert colour distinct from amber |
|---|---|---|---|
| waiting, cycling 1, 9 and 9+ | 16 px | at a glance | - |
| alert, `!` | 16 px | at a glance | clearly |

- Request while another window had focus: the button flashed until he
  clicked it (back after 2 s). Request while focused: nothing changed.
- **Pass** at 100 % with a light taskbar: story 4.5 keeps the D-523 badge.
  Not run, as information only, not blocking: a dark taskbar, and 150 %
  scaling (D-520: not the owner's screen).

## E81. What a real wait emits (v0.4 gate 4)

Story 4.1 AC 4, AD-178 gate 4, D-499. Run by the owner on 2026-10-28, in a
scratch folder whose `.claude/settings.local.json` sets
`permissions.defaultMode` to `default` (the owner's own sessions run in
`bypassPermissions`), Claude Code 2.1.312, an interactive session. The
prompt asked Claude to ask a question through `AskUserQuestion`, then to
create a file, which raises a `Write` permission dialog; the owner waited
about 70 s at each dialog before answering.

`slice0_probe wait <session id>`, each line's delay since the previous one:

| Event | Delay |
|---|---|
| `SessionStart` (`startup`) | +0 ms |
| `UserPromptSubmit` | +16015 ms |
| `Notification` `permission_prompt` (the `AskUserQuestion` dialog) | +7997 ms |
| `Notification` `permission_prompt` (the `Write` dialog, after the 70 s wait, the answer and Claude's next step) | +106265 ms |
| `Stop` | +169000 ms |
| `SessionEnd` (`prompt_input_exit`) | +5179 ms |

`slice0_probe notifications` over the whole buffer: 2026-10-28,
`permission_prompt`: 2.

- A question through `AskUserQuestion` emits a blocking `Notification` of
  type `permission_prompt`, not `elicitation_dialog`, seconds after the
  prompt: a question has a known wait start, and escalates by time.
- A permission dialog emits the same type: the type does not tell a
  question from a permission.
- No repeated notification and no `idle_prompt` arrived during a 70 s wait
  at an open dialog.
- The owner's real sessions, in `bypassPermissions`, emitted no
  `Notification` at all over the buffer's day: they raise no dialog.
- Outcome: story 4.3's vectors carry the question as observed
  (`tests/vectors/scene.json`, "a question through AskUserQuestion carries
  a permission_prompt notification ..."); the shape without a blocking
  notification stays as the fallback for a wait whose start Officina did
  not see, which a bypass session's agents `waiting` status alone would
  give. Rule 3 is unchanged. Gate 4 is recorded after story 4.3 was built
  (D-499's order not met, already in the Epic 4 report).

## E82. Idle and walking CPU of the scene (story 4.14 AC 5)

Story 4.14 AC 5, AD-71 as amended, D-120, D-480, D-515. Run on
2026-10-29 at night with the owner's leave, his installed Officina
closed and nothing else heavy running on the machine.

Machine: Intel Core Ultra 9 285K, 24 logical processors (24 cores, no
SMT), 95 GB RAM, NVIDIA GeForce RTX 5090. Windows 11 25H2, build
26200.9711; WebView2 runtime 156.0.4314.50. The window maximised on the
primary screen, 3440 x 1440 at 240 Hz, 100 % scaling (96 DPI); theme
Auto, Windows dark.

Binary: the release window of `main` at `5efc8a0`, built by `pnpm bundle`.
Run 1 used a local build of the same commit with `HALO_BREATHING_MS` at
one hour, D-120's worst-case build, never committed.

**Condition: the session was locked.** The owner was asleep and the
console showed the lock screen; the window was shown, maximised, not
minimised and not in front. A local probe build (a frame counter written
to the window title, never committed) read, in the same state:
`document.visibilityState` "visible", reduced motion off, Scene mode and
the seeded walking value, five `halo-breathe` animations running in the
breathing setup and none in the steady one, and `requestAnimationFrame`
at about 170 frames per second, not the screen's 240. So the WebView is
not throttled under the lock, but it draws about 30 % fewer frames than
on the unlocked screen: a per-frame cost would be up to 240/170 = 1.4
times the figures below.

Method:

- Each run on a new home from `node app/scripts/e2e-fixtures.mjs --home
  <dir> scene-measure`, the window started with the printed environment
  plus `WEBVIEW2_USER_DATA_FOLDER=<dir>\webview`. Without that variable
  the release window keeps its WebView2 profile, and with it the stored
  view, in the installed app's folder under the user's local app data,
  whatever `USERPROFILE` says: a measurement run would rewrite the
  owner's stored mode and walking choice.
- The stored view (Scene; walking on, or off for run 5) was written into
  that profile once by a throwaway build, never committed, and copied
  into each run's profile; the measured binary only read it.
- Window maximised 3 s after the start, 15 s to settle, then 60 samples
  10 s apart; for runs 4 and 5, `node app/scripts/scene-replay.mjs --home
  <dir>` started with the first sample (one crossing change every 10 s,
  60 changes).
- A sample sums the CPU time (`TotalProcessorTime`) of `officina-app` and
  of every `msedgewebview2.exe` below it in the parent chain (7 processes),
  over the sample's wall time: % of one logical core, and that over 24
  for the machine. Mean: total CPU time over total wall time; max: the
  largest 10 s sample. The fake claude processes the collector starts are
  stand-ins for `claude` and are not counted.
- `scene-measure`'s waits are 3 to 9 minutes old when it is written and
  the alert threshold defaults to 10 minutes, so in any 10-minute idle
  run the waits escalate one by one (a walk to the alert room each, the
  band, the taskbar). The idle runs 1 to 3 add `[alert] after_minutes =
  30` to the home's floor file, so nothing changes during them.
- Those waits being past two minutes, the shipped binary shows them
  steady from the start: run 3 is that, rather than D-120's build at 0.
  Run 2 re-dates the five waits to the start (their prompt two minutes
  before), so each breathes its two minutes during the run, then rests.
  Runs 4 and 5 use the home as written: the replay ends the old waits in
  its first minute, before any reaches 10 minutes.

| Run | Setup | Mean, machine | Max, machine | Mean, one core | Max, one core | Gate: under 1 % of the machine |
|---|---|---|---|---|---|---|
| 1 Halos breathing throughout | breathing build, five waits breathing | 0.49 % | 0.80 % | 11.8 % | 19.2 % | pass |
| 2 The real two-minute rule | shipped, five waits from the start | 0.12 % | 0.63 % | 2.9 % | 15.1 % | pass |
| 3 Halos steady | shipped, five waits past two minutes | 0.066 % | 0.15 % | 1.6 % | 3.6 % | pass |
| 4 Replay, walking on | shipped | 1.95 % | 3.39 % | 46.9 % | 81.3 % | **fail** |
| 5 Replay, walking off | shipped | 0.58 % | 0.84 % | 14.0 % | 20.2 % | pass |

- Run 2: 0.48 % of the machine (11.4 % of one core) over the first 100 s,
  while the halos breathe, then 0.048 % (1.2 %).
- Run 5 climbs from 0.13 % to about 0.6 % over its first two minutes: the
  replay's new waits each breathe their two minutes, so about two halos
  breathe at any time. The glides add little.
- Run 4 less run 5, walking's own cost at one walk every 10 s: about
  1.4 % of the machine (33 % of one core). The frame loop stops once
  everyone has arrived, but at this pace a walk is under way most of the
  time.
- The per-core figure is information only (D-515): every run with a
  breathing halo is over 8 % of one core.
- Times 1.4 for the unlocked screen, an upper bound since not every cost
  is per frame: run 1 0.69 %, run 2 0.17 %, run 5 0.82 % on average; run
  5's largest sample 1.18 %. Run 4 fails at 170 frames per second, so its
  verdict holds on the unlocked screen.
- Synology Drive's daemon used about one core during the runs; it is not
  in the measured processes.

Outcome: the walking run fails FR-9's gate, so D-480's fallback ships in
the same change: `DEFAULT_WALKING` is `false`, the Walking toggle stays,
and the README and the default-mode cases follow
(`modes.spec.ts > walking is off by default`). The two-minute halo run
passes: `HALO_BREATHING_MS` stays 120 000 ms. A run on the unlocked
240 Hz screen would put the passes in the AC's own condition; it cannot
reverse run 4.

## E86. The walking runs again after story 4.16

Story 4.16 AC 6, D-515, D-553. E82's runs 4 (replay, walking on) and 5
(replay, walking off) run again on 2026-10-29 at night, by E82's method
unchanged (fresh `scene-measure` home per run, the same stored views copied
into each run's WebView2 profile, maximised, 15 s to settle, 60 samples
10 s apart from the replay's start, `officina-app` and its 7 WebView2
processes counted). Same machine, screen, WebView2 runtime
(156.0.4314.50) and theme as E82; the owner's installed Officina closed,
no build, test or container running; Synology Drive's daemon used up to
about one core, outside the measured processes.

Binary: the release window of `main` at `d33d0c2`, built by `pnpm bundle`;
its code is story 4.16's commit `0effd0d` (depth bands, the 13 ms frame
floor, resting frames not redrawn). Run 4 was run twice, its first result
being within a thousandth of the gate.

**Condition: the session was locked again**, the window shown, maximised,
not minimised, not in front. E82's probe build read
`requestAnimationFrame` at about 170 frames per second (129 to 171 over
30 s). With the 13 ms floor the mover draws every third refresh at 170 Hz
(about 57 frames a second) and every fourth at 240 Hz (60): the lock now
changes the frames drawn by about 5 %, not 30 %, and the unlocked screen
adds empty callbacks, so it would not lower these figures.

| Run | Mean, machine | Max, machine | Mean, one core | Max, one core | Gate: under 1 % of the machine |
|---|---|---|---|---|---|
| 4 Replay, walking on | 0.9985 % | 1.63 % | 24.0 % | 39.1 % | at the threshold |
| 4 again | 1.007 % | 1.57 % | 24.2 % | 37.6 % | **fail** |
| 5 Replay, walking off | 0.51 % | 0.70 % | 12.1 % | 16.9 % | pass |

- Against E82: run 4 from 1.95 % to about 1.00 % of the machine (46.9 %
  to 24 % of one core), run 5 from 0.58 % to 0.51 %.
- Walking's own cost, run 4 less run 5 at one walk every 10 s: about
  0.50 % of the machine (12 % of one core), from E82's 1.4 %, a cut of
  about 64 %.
- Over time, both run 4s: 0.84 % in the first two minutes, then 1.01 to
  1.06 % from minute 2 to 10, once about two halos breathe at a time (run
  5: 0.34 % then 0.55 %). The whole-run mean is at 1 % only because the
  first two minutes are lighter.
- The per-core figure is information only (D-515).

Outcome: run 4 is not under 1 % of the machine: one run at 0.9985 %, its
repeat at 1.007 %, and the steady part of both over 1 %. D-553's condition
is not met, so walking stays off by default (`DEFAULT_WALKING` `false`);
the toggle stays. Story 4.16 halved the walking run, which now sits on the
gate: about 0.5 % of the machine for the walks and about 0.5 % for the
rest of the scene (the breathing halos, the replay's changes).

Owner decision, 2026-10-29 (epic-4-decisions.md D-700): given this
result on a deliberately stressful replay, walking is on by default; the
toggle turns it off. The 1 % gate is unchanged.

## E87. Reading the scene in two seconds (story 4.14 AC 9)

Story 4.14 AC 9 (PRD section 7): on five fixtures with someone waiting,
the Needs you count and the oldest waiting name read in under 2 s in Scene
mode. Run by the owner on 2026-10-29.

Method. Screenshots of the window at 1920 x 1032 CSS px (the owner's Full
HD screen, maximised, D-520), Scene mode, light theme (the owner's
choice), settled, rendered from `2a2fb94` (story 4.17) by the e2e build
with each fixture's real `officina board --json` output. A test page
showed the five floors in a random order, each for 2 s, then hid it and
asked two questions: how many sessions need you (a number), and which
project has waited longest (one of four names, the expected one among
them). Its introduction named both questions before the first floor.

Expected answers, from the lead's answer sheet, which takes them from the
fixtures:

| Fixture | Needs you | Oldest waiting | Where the scene shows it |
|---|---|---|---|
| `scene-subagents` | 1 | web-portal | register row 1; the decision office's figure with badge 1 |
| `scene-b` | 2 | billing-api | register row 1; the decision office's figure with badge 1 |
| `scene-c` | 6 | billing-api | register row 1; the decision office's figure with badge 1 |
| `scene-alert` | 3 | web-portal | register row 1 (red); the alert room's figure with badge 1; the alert band |
| `scene-eight` | 5 | billing-api | register row 1; the decision office's figure with badge 1 |

Result line: `OFC-AC9 l sub:1+- b:2+- c:6++ al:3++ 8p:5++` (theme, then
per fixture the count given, count right, oldest right).

- Count: 5 of 5 right.
- Oldest waiting name: 3 of 5 right (`scene-c`, `scene-alert`,
  `scene-eight`); missed on `scene-subagents` and `scene-b`.
- The owner's explanation: `scene-subagents` and `scene-b` were the first
  two floors shown, and he did not yet know what to look at: he took in the
  whole scene; after two rounds of questions he knew and looked for what
  was asked, and read the three remaining floors right. So 3 of 3 once the
  questions were known; the two misses are a learning effect of the test's
  order, not a reading of those floors.

Outcome: **pass, with that caveat** (epic-4-decisions.md D-701): the
count reads at a glance on every floor, and the oldest name on every
floor shown once the reader knew the questions. Pending: the first two
floors run again in a random order, the result to be added here. The
scene is unchanged by this run.

## E83. Sub-agent signals (v0.4 gate 1)

Story 4.1 AC 1, AD-176, AD-178 row 1 as amended by D-518, D-507. Claude
Code 2.1.312 on Windows 11, the owner's real sessions with the hooks
block that registers `SubagentStart` and `SubagentStop` pasted, read from
Officina's own event buffer with `cargo run -p officina-host --example
slice0_probe -- subagents` (read-only; it prints counts and times, never
an id, a type, a prompt or a path).

Two readings on 2026-10-29:

| Figure | Lead's run | Second run, later the same day |
|---|---|---|
| `SubagentStart` lines | 8 | 20 |
| `SubagentStop` lines | 9 | 18 |
| Lines without `agent_id` | 0 | 0 |
| Starts naming a session that sent `UserPromptSubmit` | 8 of 8 | 20 of 20 |
| Started agent ids; with a later stop | 2; 2 | 7; 4 |
| Agent ids stopped without any start | 1 | 1 |
| Agent ids started more than once; most starts, most stops for one id | 1; 7, 7 | 2; 8, 8 |
| Starts of an id already running (no stop between) | not printed | 0 |
| Stop to the same id's next start | not printed | 13 gaps: shortest 119 ms, median 121 s, longest 648 s |
| Most sub-agents running at once under one session | not printed | 3 |
| Ids still running at the reading | not printed | 3, started 12, 115 and 142 s before |

The second run's pairing figures come from `print_pairing`, added to the
probe for this entry. Its three ids without a stop were sub-agents still
running in the lead's own session when it was read: an id with no stop
after its last start is, by the probe's reading, still running or a lost
stop, and these three had started minutes before.

What the cases show:

- **Observed.** Every start carries the lead's `session_id`: the session
  that sent the prompts, not an id of the sub-agent's own. Sub-agents run
  side by side under one lead (three at once). A background sub-agent
  outlives the lead's turn and, resumed by messages, produces a new
  `SubagentStart` and `SubagentStop` pair under the same `agent_id` each
  time it resumes (up to eight pairs for one id), never a second start
  while it runs. The time between a stop and the same id's next start
  ranges from about a tenth of a second to minutes. One id was stopped
  without any start, the documented shape of a prompt suggestion or a
  `/btw` side question.
- **Not observed by name.** No `/btw` was run on purpose, so the start-less
  stop is not tied to one; no agent team was used, so the teammate's
  per-message shape (D-507) was not seen; no `claude --bg` session and no
  nested sub-agent were identified as such. The docs' description of the
  teammate (a start per message handled) stays documented, not verified.
- No field of the contract was absent on these lines.

**Pass.** Every start names the lead's session and pairs by `agent_id`
with a later stop once the sub-agent ends; the stop without a start is
there. Story 4.12 ships (D-476), folding the shapes above: a repeated pair
is the same sub-agent reappearing, a start while it runs is the same
sub-agent, a stop without a start is ignored, and a stop followed by a
start a fraction of a second later must not make its figure flicker
(epic-4-decisions.md D-547, D-548).

## E84. The session variables (AD-80)

Story 5.1 AC 1, D-125, D-560 (numbered when written; story 5.2's
`SESSION_ENV_REMOVED` cites this entry). **Verified** by the developer
agent on 2026-10-29, Claude Code 2.1.312, Windows 11. Names only, never a
value.

Procedure:

1. The names the agent's own Bash child holds (an interactive session
   started from VS Code's terminal, its Bash tool running Git Bash) that
   neither the user nor the machine environment block holds, both blocks
   read from the registry keys behind
   `[Environment]::GetEnvironmentVariables('User')` and `('Machine')`
   (PowerShell is refused inside the agent's worktree).
2. From an empty scratch folder, a print-mode `claude -p --model haiku
   --strict-mcp-config --mcp-config <stub> --setting-sources ""
   --permission-mode dontAsk --allowedTools "Bash(python -I *)"` started
   with only the logon environment (the two blocks and the names Windows
   sets at logon), whose stub stdio server logged its environment's names
   and whose one Bash call listed its own. Any name beyond that launch
   environment was added by Claude Code or, in the Bash child, by Git
   Bash. The server held every launch name as well.

Claude Code's names, the union (A: the agent's Bash child; B: the
print-mode Bash child; M: the print-mode MCP server):

| Name | A | B | M |
|---|---|---|---|
| `AI_AGENT` | yes | yes | yes |
| `CLAUDECODE` | yes | yes | yes |
| `CLAUDE_CODE_BRIDGE_SESSION_ID` | yes | - | - |
| `CLAUDE_CODE_CHILD_SESSION` | yes | yes | - |
| `CLAUDE_CODE_DISABLE_TERMINAL_TITLE` | yes | - | - |
| `CLAUDE_CODE_ENTRYPOINT` | yes | yes | yes |
| `CLAUDE_CODE_EXECPATH` | yes | yes | - |
| `CLAUDE_CODE_MESSAGING_SOCKET` | yes | yes | yes |
| `CLAUDE_CODE_MESSAGING_TOKEN` | yes | yes | yes |
| `CLAUDE_CODE_SESSION_ATTENDED` | yes | yes | - |
| `CLAUDE_CODE_SESSION_ID` | yes | yes | yes |
| `CLAUDE_CODE_SSE_PORT` | yes | - | - |
| `CLAUDE_EFFORT` | yes | yes | - |
| `CLAUDE_PID` | yes | yes | - |
| `CLAUDE_PROJECT_DIR` | - | - | yes |
| `COREPACK_ENABLE_AUTO_PIN` | yes | yes | yes |
| `GIT_EDITOR` | yes | yes | - |
| `NoDefaultCurrentDirectoryInExePath` | yes | yes | yes |
| `SHELL` | yes | yes | yes |

- The `CLAUDE*` names and `AI_AGENT` are the session's identity, project
  and process, entrypoint and nesting markers, and messaging socket and
  token. The last four rows are defaults Claude Code gives every child
  rather than identity; `NoDefaultCurrentDirectoryInExePath` is set by
  every Officina vector anyway (AD-80). Names only the agent's session
  holds (`..._BRIDGE_SESSION_ID`, `..._DISABLE_TERMINAL_TITLE`,
  `..._SSE_PORT`) depend on how that session was started, most likely
  from the IDE (not checked).
- A print-mode MCP server holds no `CLAUDE_PID`, where E11's server did.
- A background session's hook holds more Claude Code names (E22).
- What Git Bash and the shell add to a Bash child: `_`, `ACLOCAL_PATH`,
  `CONFIG_SITE`, `DISPLAY`, `EXEPATH`, `HOME`, `HOSTNAME`, `INFOPATH`,
  `LANG`, `MANPATH`, `MINGW_CHOST`, `MINGW_PACKAGE_PREFIX`,
  `MINGW_PREFIX`, `MSYSTEM`, `MSYSTEM_CARCH`, `MSYSTEM_CHOST`,
  `MSYSTEM_PREFIX`, `ORIGINAL_PATH`, `ORIGINAL_TEMP`, `ORIGINAL_TMP`,
  `PKG_CONFIG_PATH`, `PKG_CONFIG_SYSTEM_INCLUDE_PATH`,
  `PKG_CONFIG_SYSTEM_LIBRARY_PATH`, `PLINK_PROTOCOL`, `PWD`, `SHLVL`,
  `SSH_ASKPASS`, `TERM`, `TMPDIR`. A process started with
  `ORIGINAL_PATH` set gets a Bash tool whose profile rebuilds `PATH` from
  it (E85).
- What the terminal host (VS Code) adds: `COLORTERM`, `TERM_PROGRAM`,
  `TERM_PROGRAM_VERSION`, `GIT_ASKPASS`, `VSCODE_GIT_ASKPASS_EXTRA_ARGS`,
  `VSCODE_GIT_ASKPASS_MAIN`, `VSCODE_GIT_ASKPASS_NODE`,
  `VSCODE_GIT_IPC_HANDLE`, `VSCODE_PYTHON_AUTOACTIVATE_GUARD`,
  `CHROME_CRASHPAD_PIPE_NAME`, `FPS_BROWSER_APP_PROFILE_STRING`,
  `FPS_BROWSER_USER_PROFILE_STRING` and five `EFC_<n>_<n>` names.
- What Windows sets at logon, in neither block: the profile, application
  data and program folders, the public folder, the computer, user domain
  and logon server names, and the session name.
- Cost: one print-mode Haiku call, USD 0.0019.

## E85. Auto mode and `officina run` (v0.5 gate 8)

Story 5.1 AC 8, AD-180, D-572, D-573. **Verified** by the developer agent
on 2026-10-29, 2.1.312, in print mode. A scratch Officina home under the
temporary folder, with a floor holding one guarded project (a scratch
git repository under the worktree's ignored `target/`) and two actions:
`check` (class `free`, `git --version`) and `apply` (class `human`,
`check = "check"`, the same program), approved with
`examples/approve_fixture.rs`. `officina.exe` was a debug build of the
branch, first on the child's `PATH`, and `OFFICINA_HOME` named the
scratch home for the child only. The child started from the logon
environment: from a Git Bash environment, the Bash tool's profile rebuilt
`PATH` from `ORIGINAL_PATH` and the first run found no `officina`
(exit 127; the classifier had let both calls start). Command:
`claude -p --permission-mode auto --model haiku --output-format
stream-json --verbose --no-session-persistence [--settings <case>] --
<request>`. The init message read `model: claude-haiku-5-5`,
`permissionMode: auto`: auto mode accepts Haiku. Each tool result carries
`tool_result_meta.permission_decision` (`decision`, `source`,
`reason_type`), from which the verdicts below are read.

| Case | Request | Calls | Decision (`reason_type`) | Result |
|---|---|---|---|---|
| a, no rule (two runs) | explicit: `officina run check`, then `officina run apply --wait 5` | `check` | accept (`classifier`) | ran, exit 0 |
| | | `apply --wait 5` | accept (`classifier`) | ran: exit 126 `approval_pending`, a request file under `relay/approvals/` |
| b, `--settings {"permissions":{"allow":["Bash(officina run *)"]}}` | the same | both | accept (`subcommandResults`) | the same: the rule decided, the classifier was not asked |
| a, the session's own choice | "This repository's routine actions go through Officina's relay; `officina run --list` lists them. Run the repository's dry run, then apply the change (give the apply `--wait 5`) ..." | `--list`, `check`, `apply --wait 5` twice | accept (`classifier`), each | the same; the retry reused the one request |
| control | explicit: `git push --force origin main` | that | accept (`classifier`) | ran, failed on the missing branch |

- The control shows that a command the user asks for in so many words
  passes the classifier; the run where the session chose the calls
  itself, as a session following doctor's paragraph does, is the faithful
  case, and it passed as well.
- `claude auto-mode config` (exit 0): the owner's user settings hold no
  `autoMode` key, so it equals `claude auto-mode defaults` (17 `allow`,
  72 `soft_deny`, 1 `hard_deny`, 21 `environment` entries), none naming
  Officina. Given `--settings` with an `autoMode.allow` sentence, it lists
  that sentence: a flag source reaches the classifier's configuration.
- The relay's gate hook did not run: the owner's user settings register
  `officina hook` on seven events, not on `PreToolUse`. The verdicts above
  do not depend on it.
- `/permissions` has no print-mode form; case b's `reason_type` shows the
  narrow rule kept and deciding in auto mode.
- **Outcome: pass in case a** (D-573): doctor's block adds nothing for
  `officina run`; case c was not needed. Switching modes stays the
  owner's decision (D-490), and gate 4 runs again once he switches.
- **Owner half (interactive confirmation): pending, optional.** From the
  repository root, in a console of the owner's own: `cargo build -p
  officina`, `cargo build -p officina-host --features window-authority
  --example gates_v05`, then `target\debug\examples\gates_v05.exe gate8`.
  It prepares a fresh scratch home, opens `claude --permission-mode auto`
  consoles for cases a and b, and records the footer, both calls, the
  request file and `/permissions`. Report:
  `target/gates-v05/report-gate8.md`.
- Cost: six print-mode Haiku runs, USD 0.020 in all (each run's
  `total_cost_usd`).

## E88. Work orders in a project's own permission mode (v0.5 gate 4, owner half)

Story 5.24, D-980 to D-987; the owner half of E22. **Verified** by the
owner on 2026-10-30 with `gates_v05 gate4`, Claude Code 2.1.314, the
exact `WorkOrder` vector of AD-79 (no `--permission-mode`), each project a
trusted git repository of his. Workstation paths, probe names and session
ids are left out.

| Project | Its own settings | `claude --bg` | `claude agents --json` over 60 s | Probe file | Footer in `claude logs` |
|---|---|---|---|---|---|
| a free project | none (the owner's user default) | exit 0 in 1.4 s, stderr "Starting background service…" | `busy`, `working`, then `idle`, `done` at about 20 s | written | `bypass permissions on` |
| a guarded project | `acceptEdits` | exit 0 in 0.8 s, stderr empty | `busy`, `working`, then from 15 s `waiting`, `blocked`, `waitingFor: "permission prompt"` to the end | not written | `accept edits on` |

- **The project's own mode wins.** With no `--permission-mode`, a
  background session takes its project settings' `defaultMode`, else the
  author's: the second project's work order stopped at its first Bash
  call. AD-96 row 4 passed in the first project and failed in the
  second. Both probe sessions were stopped and removed by the owner.
- **The invoker's environment reached the session that started the
  service.** In the first run, whose `claude --bg` started the background
  service (its stderr says so), the probe's Bash listed
  `GATES_V05_INVOKER_MARK`, a name set on that invocation only: E22's
  branch seen from the other side, since a supervisor started by an
  invocation keeps that invocation's environment. In the second run the
  service was already running and the probe wrote no file, so which
  environment that session got is not known from this run. Besides the
  invoker's names, the Bash listed the session names of E84 and E22
  (`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_JOB_DIR` and the like).
- **Inferred, not verified**: the background service is per configuration
  directory, so a supervisor started with the floor's `CLAUDE_CONFIG_DIR`
  serves only sessions of that directory. Not tested here.
- **Outcome** (D-127 as D-980 amends it): work orders keep the project's
  own mode by default, a blocked one waits in Needs you, and the author
  may switch a project's work orders to Officina's default, auto mode
  (D-982 to D-986). The environment that a service a work order started
  hands to the author's own later background sessions is accepted and
  stated in the README (D-987).

### Window half (story 5.7 AC 11)

**Passed, with three findings** (D-1000 to D-1003). **Verified** by the
owner on 2026-10-30 with Claude Code 2.1.314 and the installed window built from
`b60690f`, started with `OFFICINA_FLOOR` naming a test floor: one `free`
project on a trusted scratch git repository and one template,
`probe-note`, whose prompt asks for `officina-probe.txt` listing the
environment variable names the Bash tool sees, then `git status`.
Workstation paths and ids are left out.

- **Approval.** The catalogue banner did not appear: a floor of free
  projects and templates, no action, was not counted as using the relay,
  so nothing offered it for approval (finding 1, D-1000). Once a harmless
  `free` action (`git --version`) was added to the floor, the header's
  "catalogue change" chip appeared, the owner approved the catalogue in
  Board with the mouse, and the chip went.
- **Composing.** In the Orchestrator view, Create on the subject picker
  seemed to do nothing, while the card `probe-note` stood just below
  (finding 2, D-1001).
- **Launch and binding.** The card read "started in the background". The
  journal held `work_order_launched` with outcome `started`. The registry's
  `work_order` row: outcome `started`, exit 0, `printed` holding
  `backgrounded · <id> · probe-note-1`, `bg_id` and `session_id` set,
  `bound_at_ms` 1.8 s after `launched_at_ms`, well within the 10 minutes.
- **Where the session worked.** The session ended `done`, but Claude made
  a git worktree itself, `<scratch>/.claude/worktrees/officina-probe` on
  the branch `worktree-officina-probe` (locked), ran there (its `claude
  agents` folder was that worktree) and wrote the probe file there, not in
  the project's working tree; the worktree stayed after the session
  (finding 3, D-1002). The binding held, since the folder lies under the
  project's. Inferred, not verified: the session chose to use Claude
  Code's own worktree tool, since the work order's vector passes no
  worktree flag; whether every work order does so is not known from one
  run.
- **Environment.** The probe's Bash listed Officina's environment names,
  `OFFICINA_FLOOR` among them: a session kind keeps the author's
  `OFFICINA_*` settings (README, "How Officina starts `claude`"), as D-987
  expects.
- **Outcome.** AD-96 row 4's window half holds: one pointer click on a
  core-issued card started the session, the row bound within seconds, and
  `printed` names it. Story 5.7 AC 11 closes (D-1003).

## E89. Terms and account (v0.6 gate 7)

Story 6.1 AC 1; `epic-6-decisions.md` D-601, D-602, D-603, D-615.
**Stated by the owner** on 2026-10-28, recorded on 2026-11-01. Not
verified on the workstation: it is a statement of terms and intent.

- **The account.** Workers use a Claude subscription account (Pro or Max
  style, not the API), whatever its origin: the owner's personal account,
  his professional account, or one the company provides. Every origin must
  work, so no code reads, stores or branches on the plan or the origin.
  No API key path is required; `api-key` stays an optional kind, never the
  default.
- **The credential.** `claude setup-token` is acceptable and the
  recommended path for unattended workers; `/login`, the owner's method
  today (about a month per login), must be supported too (D-603).
- **Docker Desktop's terms.** The company has 65 employees (the owner's
  answer) and its annual revenue is assumed under USD 10 million, for the
  owner to correct; Docker's Desktop license page makes Docker Desktop free
  under 250 employees and USD 10 million. No licence blocker.
- **Outcome against AD-126 row 7.** Pass on the owner's statement. Whether
  a subscription credential works in a worker is the gate 3 entry's and the
  login entry's to show; P-52's stop applies only if both fail and no API
  key is available (D-601).

## E90. The work network (v0.6 gate 0)

Story 6.1 AC 2; D-611, D-613, D-615. Recorded on 2026-11-01.

- **Stated by the owner** (2026-10-28): the work network has an HTTPS
  proxy configured automatically, a PAC file likely. Officina supports it
  as a standing assumption: `upstream_proxy = "system"` by default,
  resolved from Windows' settings, and a corporate root from `ca_file`.
- **Verified, this workstation's network** (addendum section 1 item 4,
  2026-10-28, repeated on 2026-11-01 with the companion probe's `network`
  subcommand): direct, no proxy variable, Windows `ProxyEnable` 0, no
  `ProxyServer` or `AutoConfigURL`; as uid 1000, `curl -v
  https://api.anthropic.com` from the probe base answered `HTTP/2 404` with
  the public chain (issuer Google Trust Services).
- **The work network's own procedure** is the work network entry (story
  6.1 AC 14: W1 to W4), due before story 6.20 closes Slice A. W1 and W2
  were not run by the agent: its worktree isolation refuses PowerShell, so
  the owner's first run of W1 is also the one-liner's first run.
- **Outcome against AD-126 row 0.** Recorded; with E89 it opens Slice A
  (D-613). No code changes either way (D-203).

## E91. The musl build (v0.6 gate 9)

Story 6.1 AC 3; AD-100, D-202. **Verified** by the agent on 2026-11-01:
Windows 11 Pro 10.0.26200, Rust 1.99.1 (the pinned toolchain), Docker
Desktop 4.95.0 (242831), engine 29.8.2, kernel
`6.18.33.2-microsoft-standard-WSL2`.

- `rustup target add x86_64-unknown-linux-musl` installed the target
  locally (the repository's `rust-toolchain.toml` changes in story 6.5).
- `cargo build --release -p officina-core --target x86_64-unknown-linux-musl`
  with `CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld`: built.
- A scratch std-only program outside the repository (resolve a name, open
  a CONNECT tunnel) built the same way into a 468 KiB static ELF. **No
  linker flavour flag was needed**: with the linker named `rust-lld`, rustc
  picks the `ld.lld` flavour for the Linux target itself, so
  `.cargo/config.toml` needs only `linker = "rust-lld"`.
- Copied into a `FROM scratch` probe image, run as uid 65534 with
  `--read-only --cap-drop ALL --security-opt no-new-privileges:true` on a
  probe `--internal` network beside a stub CONNECT proxy attached to that
  network and to a probe bridge: it resolved the stub's name through the
  engine's mounted resolver file, an outside name did not resolve
  ("Try again"), the CONNECT to the allowed host answered `200 Connection
  established` and the stub logged `ALLOWED`; an off-list host answered
  `403` and the stub logged `REFUSED`.
- **Outcome against AD-126 row 9 (first half).** Pass; AD-100's fallback is
  not needed. The second half (the real `officina-gateway:local`) closes
  story 6.8.
- **Second half** (story 6.8 AC 7, D-202). **Verified** by the agent on
  2026-11-01, same versions, driven by `crates/officina-host/examples/battery_ci.rs`
  on Docker Desktop with the window's resolution of `docker.exe`.
  `officina-gateway:local` built by Build images' own code (`FROM
  scratch`, the static program, `USER 65534:65534`) on the `desktop-linux`
  builder (`docker` driver), then run with AD-102's `Gateway` vector as
  core builds it (read-only root, every capability dropped, no new
  privileges, `--init`, 0.5 CPU, 128 MiB, 136 processes, the local log
  driver) on a test egress bridge, connected to a test internal network. A
  probe client on the internal network, through `-x
  http://<gateway>:3128`: `https://api.anthropic.com/` answered `404`
  (reached), `https://example.com/` got `CONNECT tunnel failed, response
  403`. The gateway's lines: `started` with its digest and no upstream,
  `allowed` for `api.anthropic.com:443`, `refused` for `example.com:443`
  with cause `not_allowed`.
- **Outcome against AD-126 row 9 (second half).** Pass. Every test object
  (`officina-test-*`, labelled `officina-test=1`) and the built images were
  removed afterwards and the listings checked empty.

## E92. Images, options, mounts, copies (v0.6 gate 1)

Story 6.1 AC 4; AD-99, AD-102, D-202, D-203. **Verified** by the agent on
2026-11-01, same versions as E91, Claude Code 2.1.316 on the host. The
owner's half (a git-ignored folder of one of his projects in the synced
tree, and the junction question there) is open; it runs with
`workshop_gate_probe mount`.

- **The probe base**, drafted to AD-99 in a scratch folder:
  `debian:trixie-slim` by index digest, the packages sorted with
  `--no-install-recommends`, an empty `ca/` folder copied and
  `update-ca-certificates` run before any HTTPS download, uv 0.13.6 checked
  with `sha256sum -c` against the release's published checksum, the
  documented installer (`install.sh <version>`) in a build stage, user
  `worker` uid 1000, setuid and setgid bits removed. Built with
  `HTTPS_PROXY` and `https_proxy` build arguments naming a logging stub
  that allowed every host: 853 MB.
- **The installer's hosts**: `claude.ai` (the script), then
  `downloads.claude.ai` only (the `latest` pointer, `manifest.json`, the
  bootstrap binary, then the pinned version's download by `claude
  install <version>`). The build stage's `claude` also opened one
  connection to `api.anthropic.com` after installing (builds use the
  engine's default network, so this changes nothing; a real build stage can
  set `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`). uv's download went to
  `github.com` and `release-assets.githubusercontent.com`. The Debian
  packages came over Debian's HTTP mirror, outside `HTTPS_PROXY`.
- **The installer's checksum.** `install.sh` checks the bootstrap binary's
  SHA-256 against `manifest.json` from the same host over TLS and stops on
  a mismatch; the installed `/opt/officina/bin/claude` hashed equal to the
  `linux-x64` checksum of the 2.1.316 release manifest. A version can be
  pinned (`install.sh 2.1.316` installed 2.1.316). AD-99's release-manifest
  path is not needed (D-203).
- **The image.** As uid 1000: `/work/.officina/{in,out,home,config,mounts}`
  owned by `worker`; no setuid or setgid file; `claude` root-owned 0755;
  `claude --help` lists `--permission-prompts`; image `ENV` names `PATH`,
  `SSL_CERT_FILE`, `REQUESTS_CA_BUNDLE`, `NODE_EXTRA_CA_CERTS`,
  `UV_NATIVE_TLS` (plus `HOME` and `HOSTNAME` from the engine).
- **A project image** `FROM` the probe base with the default builder: no
  pull (the builder resolved the local image), and the inherited
  `officina.base` label was present on the result.
- **The builder.** `docker buildx ls`: the current builder is
  `desktop-linux`, driver `docker`, so builds see local images; story 6.8
  passes `--builder` naming the engine's `docker`-driver builder (D-203).
- **AD-102's `Worker` options by hand** (`--user 1000:1000 --read-only
  --cap-drop ALL --security-opt no-new-privileges:true --init --stop-timeout
  3 --pids-limit 1024 --memory 2g --memory-swap 2g --cpus 2 --log-driver
  local --log-opt max-size=32m --log-opt max-file=3`, a named volume at
  `/work`, a tmpfs `/tmp`) were accepted and read back by `inspect`. `docker
  cp` of a folder into the created container's volume arrived owned by
  uid 0, mode 0755, readable by uid 1000; writes to `/`, `/usr`,
  `/opt/officina/bin` and `/etc` failed with "Read-only file system", `/tmp`
  took them; `docker start` returned at once and the container kept running
  after the CLI exited; `docker cp` out of the stopped container returned
  every file, the accented name intact.
- **The tar stream** (D-218, review F16): `docker cp <container>:/work/.officina/out -`
  from a probe container whose `out` held a file with a 125-byte name, an
  accented name and `tip` is saved as `tests/vectors/fixtures/docker-cp-out.tar`.
  It is ustar with **PAX `x` records** before the long and the non-ASCII
  names (the ustar name field then holds a cut or ASCII-stripped name and
  the PAX `path` the real one); the directory entry comes first; uid 1000,
  modes 0755 and 0644. Story 6.13's `workshop::tar` must honour `path` from
  `x` records.
- **CLI plugins** (review F40), by watching every descendant of
  `docker.exe`: `docker create` and `docker run` started no child.
  **`docker build` started `docker-buildx.exe` and `git.exe`** (buildx reads
  the build context's git state for provenance); with `BUILDX_GIT_INFO=0`
  in the environment `git.exe` no longer started. Story 6.8's build
  environment sets it, since a project's own context is a git work tree.
  The Docker CLI's `config.json` was not read: the workstation's secret
  fence refuses it, so the `proxies`, hooks and plugin-directory entries
  stay doctor's to name (D-228).
- **Junctions** (D-236), on a scratch folder of the agent's, not in the
  synced tree: a junction inside a read-only mounted folder, pointing at a
  folder outside it, appeared in the container as an entry that does not
  resolve ("No such file or directory"): not followed. The owner's run in
  the synced tree decides D-236's branch.
- **Outcome against AD-126 row 1 (first half).** Pass for the agent's part:
  no copy fallback, no `--build-context`, no release-manifest path. The
  synced-tree mount is the owner's (open). The second half (the real images
  and the real `Worker` vector) closes story 6.8.
- **Second half, the agent's part** (story 6.8 AC 7, D-202). **Verified**
  by the agent on 2026-11-01, same versions, Claude Code 2.1.316 on the
  host, through `crates/officina-host/examples/battery_ci.rs --worker-probe`
  on Docker Desktop (two runs, the second on the final Dockerfile).
  `officina-gateway:local`, `officina-workshop:local` and one probe project
  image (`FROM officina-workshop:local`, a scratch Dockerfile of two
  lines) built by Build images' own code, in that order, with
  `BUILDX_GIT_INFO=0`; the browser base is Slice B's. `docker image
  inspect` of the base shows AD-99's labels (`officina.base`,
  `officina.version`, the Dockerfile, entry and gateway SHA-256,
  `officina.claude.version` 2.1.316, `officina.ca.sha256` `none`, the OCI
  set). The measured programs (entry point and Claude Code, streamed out of
  a container never started) were equal on the base and the project image,
  and the measured entry point equal to the shipped program. `find / -xdev
  -perm /6000 -type f` in the base found nothing (the Dockerfile's removal
  step now fails the build if it cannot run). `/opt/officina/bin/claude
  --version` in the base printed `2.1.316 (Claude Code)`, the host's. The
  base run with `workshop.json`'s `Worker` vector (its options, limits,
  tmpfs, the work volume and one read-only bind mount of a scratch folder
  of the agent's at `/work/.officina/mounts/0`, a dummy credential name),
  its command replaced by a probe: `uid=1000(worker) gid=1000(worker)`; a
  write to `/` refused ("Read-only file system"); the mount read; a write
  in the mount refused; a write in `/work` succeeded.
- **The owner's half** (story 6.1 AC 4, D-202). **Verified** by the owner
  on 2026-11-01, same versions, with `workshop_gate_probe mount` from his
  own console: a git-ignored folder of one of his projects inside the
  synced tree, bind-mounted read-only at `/work/.officina/mounts/0` with
  AD-102's `Worker` options on the probe base: 89 files seen, 89 read,
  none failed; a rewrite and a create both refused ("Read-only file
  system"); a junction he made inside the mount, pointing at a scratch
  folder outside it, was **not followed** ("No such file or directory"),
  as in the agent's scratch run. D-236's branch: doctor does not count
  reparse points inside a mount; the refusal of a mount or memory source
  reached through a reparse point stays. The probe's objects were removed.
  A run of `battery_ci.rs --worker-probe --mount-dir <the ignored
  folder>` would repeat it on the real base; the probe base was drafted
  to the same AD-99 rules, so it is not required.
- **Outcome against AD-126 row 1.** Pass, both halves: the agent's on the
  real images, the owner's on his synced tree. Every test object and the
  built images were removed afterwards.

## E93. No route out (v0.6 gate 2, first half)

Story 6.1 AC 5; AD-104, D-202, D-203. **Verified** by the agent on
2026-11-01, same versions as E91. Layout: a probe `--internal` network, a
probe bridge, a stub CONNECT proxy attached to both allowing
`api.anthropic.com` alone, a probe worker (uid 1000, AD-102's fixed
options) on the internal network only, `HTTPS_PROXY` naming the stub. 28
attempts, a Python script in the probe:

| Attempt | Outcome |
|---|---|
| TCP to three public IPv4 addresses on 443 | "Network is unreachable" |
| an IPv6 socket to a public address | "Network is unreachable" |
| UDP DNS to a public resolver | "Network is unreachable" |
| resolving `example.org`, `api.anthropic.com`, `host.docker.internal`, `gateway.docker.internal`, `http.docker.internal` | "Temporary failure in name resolution" |
| an ICMP datagram socket | "Network is unreachable" |
| Docker Desktop's host address (what `host.docker.internal` resolves to on a plain bridge) on 22, 80, 2375, 2376 | "Network is unreachable" |
| the internal network's own gateway address on 22, 80, 2375, 2376 | "Connection refused" |
| the stub's ports 22, 80, 3129, 8080 | "Connection refused" |
| a second probe run's container on its own internal network | "Network is unreachable" |
| engine socket paths, `/mnt/wsl`, `/mnt/host` | absent |

- **The allowed host through `HTTPS_PROXY`**: from the probe base, `curl
  https://api.anthropic.com/` through the stub answered 404 with the
  certificate verified; `example.org` through it got the stub's 403.
- **One nuance for the battery** (AD-117's host family): the internal
  network's gateway address answers at layer 3 ("Connection refused", not
  "unreachable"): the engine VM holds that address on the bridge even for an
  internal network, and nothing listened on the four ports. The battery's
  attempts on "the battery networks' gateways" are therefore judged on the
  connection, not on reachability.
- **Outcome against AD-126 row 2 (first half).** Pass: every attempt failed
  and the allowed host was reached through the proxy. Outside names do not
  resolve, so the `--dns 0.0.0.0` branch is not taken (D-203). The second
  half (the real battery on Docker Desktop) closes story 6.10; the CONNECT
  through the work network's proxy closes story 6.25.
- **Second half** (story 6.10 AC 7; AD-117, D-202). **Verified** by the
  agent on 2026-11-02 on Docker Desktop 4.95.0 (engine 29.8.2, kernel
  6.18.33.2-microsoft-standard-WSL2), Claude Code 2.1.316 in the base,
  driven by `crates/officina-host/examples/battery_ci.rs` with the
  window's resolution of `docker.exe`, a floor with `auth =
  "oauth-token"` and the dummy credential: Build images' own code built
  the gateway and base images, then `workshop::battery::run` played the
  real battery on the base: the decoy on its bridge and its probe (the
  host family's control), phase 1 behind gateway A, phase 2 behind gateway
  B. **Pass**: 45 attempts, 45 `refused`, none `escaped`, `inconclusive` or
  `not_applicable` (Docker Desktop's names resolved for the decoy and were
  unreachable from the worker), every gateway-family refusal matched in
  order in the gateways' logs, gateway B exited 3; clock skew 251 ms (a
  diagnostic). No owner project image was built in this run, since none
  declares a Dockerfile on the agent's floor: the battery on each owner
  project image runs with the owner's floor (`battery_ci.rs --floor
  <owner floor>`), recorded here when run. Every object carried
  `officina.managed=1` and the battery's id, and was removed; the images
  were removed at the end.
- **Cleanup** (gates 9, 1 and 2): every probe container, network, volume
  and image carried `officina-probe=1`; after removal, the four label
  listings were empty. The pinned Debian base pulled by digest stays in the
  engine's store as a build cache layer, as D-201 allows.

## E94. A worker authenticates with a setup token (v0.6 gate 3, first half)

Story 6.1 AC 6; AD-126 row 3, D-202, D-203, D-1118. **Verified** by the
owner on 2026-11-01 with `workshop_gate_probe token` from his own console,
the token from `claude setup-token` set in that console only (the report
holds no token); Docker Desktop 4.95.0, engine 29.8.2, Claude Code 2.1.316
in the probe base. A worker by hand with AD-102's options on the probe
internal network, behind the stub CONNECT proxy allowing
`api.anthropic.com:443` alone: `system/init` with permission mode
`bypassPermissions`, no `api_retry`, `result` subtype `success`, 2 turns,
0.0267 USD, exit 0, one commit past the base. The stub allowed
`api.anthropic.com:443` and refused nothing. `claude --help` in the image
lists `--permission-prompts`.

- **Outcome against AD-126 row 3 (first half).** Pass. `CLAUDE_CODE_HOSTS`
  stays `api.anthropic.com` alone (D-203; D-1118 confirmed). The window
  half closes story 6.19.

## E95. Hooks from a read-only mount (v0.6 gate 4, first half)

Story 6.1 AC 7; AD-126 row 4, D-202. **Verified** by the owner on
2026-11-01, same run, a scratch repository whose git-ignored `.claude`
was mounted read-only with a logging hook on six events: the stream
carried `hook_started` and `hook_response` lines (exit 0 each) for
`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse` and `Stop`
(10 lines), and the hooks log showed `SessionEnd` too, which the stream
does not report. A hook line's keys: `exit_code`, `hook_event`, `hook_id`,
`hook_name`, `outcome`, `output`, `session_id`, `stderr`, `stdout`,
`subtype`, `type`, `uuid`. Result `success`, 2 turns, 0.0192 USD, exit 0.
The source kinds of the work machine's instructions (`/memory`,
`/hooks`) were not answered.

- **Outcome against AD-126 row 4 (first half).** Pass. FR-9 needs no
  amendment from it; the second half (one window run per owner project
  with hooks) closes story 6.20.

## E96. A missing hook program (v0.6 gate 5)

Story 6.1 AC 8; AD-126 row 5, D-203. **Verified** by the owner on
2026-11-01, same run: a `PreToolUse` hook whose program does not exist
reported `hook_response` with exit code 127, and the run still
**succeeded**: the tool call went ahead (`result` `success`, exit 0, one
commit). A missing hook program fails open.

- **Outcome against AD-126 row 5.** Pass for the fields (the hook lines
  carry `exit_code`, so `hook_failed` is built, D-203); the consequence is
  D-1133: the workshop never relies on a missing guard to block; the
  inventory's `hook_program_missing` refusal before the launch (story 6.9)
  is the guard.

## E97. The budget cap (v0.6 gate 6)

Story 6.1 AC 9; AD-126 row 6, D-1109. **Verified** by the owner on
2026-11-01, same run: `--max-budget-usd 0.01` on a multi-step prompt
stopped with `result` subtype `error_max_budget_usd`, `terminal_reason`
`budget_exhausted`, exit 1, after 1 turn, having spent 0.0177 USD: the cap
is checked after a turn, so a run can overspend it by up to one turn.

- **Outcome against AD-126 row 6.** Pass: the budget stops a run, so
  `max_usd` and `--max-budget-usd` stay (D-203); it is a soft cap (D-1134).

## E98. Where a setup token is revoked (v0.6 question 10)

Story 6.1 AC 11. Asked of the owner on 2026-11-01: not found by the owner,
who did not look further. The README's revoke sentence (story 6.24) says
that this place is not known.

## E99. The login check (v0.6, D-610)

Story 6.1 AC 15; D-605 to D-610. **Verified** by the owner on 2026-11-01
with `workshop_gate_probe login` from his own console (no credential in
the report), same versions as E94; step 6 is the agent's (D-1101).

1. `claude auth login` in the probe base as uid 1000, `CLAUDE_CONFIG_DIR`
   on a probe volume: the URL and pasted-code flow completed, "Login
   successful"; files written, mode 600: `.claude.json`,
   `.credentials.json` and a backup of `.claude.json`.
2. `claude auth status` on that volume, with the bridge network and with
   `--network none`: exit 0, logged in, `authMethod` `claude.ai`.
3. A worker given only `.credentials.json` copied into a fresh config
   dir, behind the stub: `success`, 0.0592 USD, exit 0. `.claude.json` is
   not needed (D-606 copies `.credentials.json` alone).
4. The run's copy of `.credentials.json` hashed the same as the stored file
   afterwards, and a second run from the stored file authenticated: one
   session does not rotate the stored login (D-606's attended-use branch
   is not taken).
5. After `claude auth logout` on the first volume (which revokes that
   login), a run from a saved copy gave `system/api_retry` with
   `authentication_failed`, then `result` with `is_error` true,
   `terminal_reason` `api_error`, exit 1, cost 0. **`claude auth status` on
   the revoked copy still said logged in**, with and without the network:
   it reads the local file only, and cannot see a revoked or expired login.
7. Hosts through the stub: allowed `api.anthropic.com:443` and
   `platform.claude.com:443`; refused `mcp-proxy.anthropic.com:443`, with
   no effect on the run.

- **Outcome.** Pass (steps 1 and 3). The login kind is built (D-610). A
  login run needs `platform.claude.com` beside `api.anthropic.com`, which
  `LOGIN_HOSTS` already adds for the login kind alone (D-606); the token
  kinds keep `api.anthropic.com` alone (E94); `mcp-proxy.anthropic.com` is
  not needed. The auth probe cannot find a revoked or expired login
  (D-1135).

## E100. `claude logs` repaints and a stopped background service (v0.5 gate 7)

Story 5.10 AC 4, D-126, D-806, D-1020, D-1021, D-1023.

- **Owner readability check (2026-11-01, 2.1.316): fail as built.**
  `target\debug\examples\logs_probe.exe <bg id>` on a live background
  session: `claude logs` exit 0 in 711 ms, 111,535 bytes, 3,770 ESC
  bytes; the stripping removed the 3,770 sequences, kept 65,529 bytes and
  dropped 25,970 from the start. The text repeated the whole screen about
  thirty times, each repaint (the banner, the status bar "bypass
  permissions on", spinner lines such as "Determining… (3s · ↓ 124
  tokens)", the prompt line, the box-drawn table of the answer) after the
  one before. Everything that mattered was there (the prompt, "Ran 1
  shell command", the answer and its table, "Brewed for 7s · done 21:06"),
  buried among the frames.
- **Why (agent capture, 2026-11-01, 2.1.316).** One Haiku background
  session (`claude --bg --name logs-fixture --model haiku`, a one-line
  prompt asking for a markdown table, no tool call) in the owner's trusted
  probe scratch folder (a scratch folder under `target/` was refused as
  untrusted, and trusting it would have written Claude Code's
  configuration), read with `claude logs <id>` once done, then stopped and
  removed: exit 0, 65,535 bytes, empty stderr. Its sequences: EL 1,380,
  SGR 851, home (`CSI H`) 30, CUP 30 (all `CSI 47;3H`, the input row), ECH
  30 and CUF 30 (the effort indicator placed at column 181), ED 2; no OSC,
  no scroll region, no alternate screen; 1,440 line feeds, each after a
  carriage return. So thirty frames, each starting at home, every row
  ended by EL, the cursor parked on the input row at the end. The screen
  is 200 columns by 50 rows (full-width rules of 200 `─`; the input row is
  47 with three rows below), and the rule above the input row is followed
  by `❯ ` with no line break, which relies on the terminal's deferred
  wrap. Nothing in the output gives the screen's size. The capture is
  committed as `crates/officina-core/tests/data/logs/claude-2.1.316-repaints.ansi`,
  its one home-relative path redacted, marked binary in `.gitattributes`
  so its carriage returns stay. Cost: one Haiku turn, well under USD 0.01.
- **After D-1020 (agent).** The capture renders to its last frame once:
  the banner, the prompt, the table whole, `✻ Baked for 2s · done 9:10
  PM`, the effort indicator, the two rules and the mode footer; the first
  frames' spinner is gone (`screen::tests::the_capture_renders_its_final_screen_once`).
  With no width known, the input row's `❯` stays at the end of the rule
  above it (D-1020). The owner's rerun on one of his sessions is pending
  (D-1023).
- **A stopped background service (owner, 2026-11-01, 2.1.316).** After a
  reboot, `claude logs` of a finished session failed with `connect ENOENT
  \\.\pipe\cc-daemon-<id>-control`, the background service's control
  pipe, while that service was not running; the probe printed "no logs
  section" with the error. Starting a background session prints "Starting
  background service…" first (agent, same day). The meeting names that
  case (D-1021).

## E7. Configuration directory

- **Documented.** `CLAUDE_CONFIG_DIR` redirects settings, session history and
  credentials; an absolute path is required. The sessions directory Officina
  watches is `<config dir>/sessions`.
