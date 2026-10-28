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

## E7. Configuration directory

- **Documented.** `CLAUDE_CONFIG_DIR` redirects settings, session history and
  credentials; an absolute path is required. The sessions directory Officina
  watches is `<config dir>/sessions`.
