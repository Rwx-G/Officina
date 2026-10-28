<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/logo-dark.png">
    <img src="docs/assets/logo.png" alt="Officina" width="160">
  </picture>
  <h1 align="center">Officina</h1>
  <p align="center"><strong>One window that tells you which Claude Code session needs you</strong></p>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License"></a>
  <img src="https://img.shields.io/badge/version-0.1.0-brightgreen.svg" alt="Version">
  <img src="https://img.shields.io/badge/Rust-2024-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Windows-0078D6.svg" alt="Platform">
</p>

---

You run five to ten Claude Code sessions at once, in editor terminals, across several repositories, and you no longer know which terminal is doing what. Officina is a small desktop board that answers three questions at a glance, without touching any session:

- **Who needs me?** A session waiting on a permission prompt or a question.
- **Whose turn is it?** A session that finished its turn and waits for your next message.
- **What is working, what is finished, what was interrupted?**

One click brings the right session to the front. The 5-hour and weekly quotas sit in the header, with their reset times and how fresh the numbers are.

<p align="center">
  <img src="docs/assets/board.png" alt="The Officina board: Needs you, Your turn, Working, To resume and Done columns, with quota gauges in the header" width="900">
</p>

<p align="center">
  <img src="docs/assets/demo.gif" alt="A session waiting on a permission prompt: one click focuses it, and once answered it moves back to Working" width="900">
</p>

*The sessions shown are fictitious.*

## Why another tool

The scarce resource is your attention, not your quota. Each subject needs a cycle of focus, study, decision and validation, and that cycle does not hold across six subjects at once. Officina's first job is to stop you from cycling through terminal tabs to find the one that is blocked.

## How it reads your sessions

Officina hosts nothing and owns no session process. It only reads documented Claude Code surfaces:

| Source | What it gives |
|---|---|
| `claude agents --json --all` | every live session, its status (`busy`, `waiting`, `idle`), what it waits for, background session state |
| Hook events (`officina hook`) | last prompt, last answer, permission notifications, how a session ended |
| Your statusline (`officina quota write`) | the 5-hour and weekly rate limits of your account |

Missing data is shown as **unknown**, never as finished. Officina never writes to your Claude Code configuration: `officina doctor` prints what to paste, you paste it.

### The columns

| Column | Meaning |
|---|---|
| **Needs you** | A decision blocks the session: a permission prompt, a question, a dialog. Amber, numbered 1 to 9 for the keyboard. |
| **Your turn** | An interactive session finished its turn and waits for your next message. Even when it asked you something in plain text, Claude Code reports it as idle, so it lands here, never in Done. |
| **Working** | Claude is on it. A session whose state Officina cannot tell shows here with an `unknown` badge. |
| **To resume** | Ended, stopped or interrupted in the last 24 hours. |
| **Done** | Background sessions that finished in the last 24 hours. Collapsed beyond five. |

### Bringing a session to the front

- **Interactive session** in an editor or terminal: Officina walks up from the session's process to the window that hosts it. VS Code runs every window from one process, so Officina picks the window whose title contains the session's folder name, then each parent folder. When several windows match, it lists them instead of guessing. It brings the window forward; choosing the terminal tab inside the editor stays with you.
- **Background session**: Officina opens `claude attach <id>` in a new console. The card can also copy that command for you.

If the wrong window comes up, click **Wrong window?**: Officina counts how often its first click is right.

## Install

Officina v0.1 targets Windows 10 and 11 (x64). Linux builds arrive with v1.

1. Download `officina-<version>-windows-x64-portable.zip` or the installer from the [releases](https://github.com/Rwx-G/Officina/releases). The binaries are not signed yet, so SmartScreen will warn on first launch.
2. Put `officina.exe` somewhere on your `PATH`.
3. Run `officina doctor` and follow what it prints.

### Wiring the hooks

`officina doctor` prints a block like this, with the absolute path of your `officina.exe`. Merge it into your **user** settings (`~/.claude/settings.json`), so every project gets it without touching any repository:

```json
{
  "hooks": {
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "\"C:/path/to/officina.exe\" hook", "timeout": 5 }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "\"C:/path/to/officina.exe\" hook", "timeout": 5 }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "\"C:/path/to/officina.exe\" hook", "timeout": 5 }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "\"C:/path/to/officina.exe\" hook", "timeout": 5 }] }],
    "Notification":     [{ "hooks": [{ "type": "command", "command": "\"C:/path/to/officina.exe\" hook", "timeout": 5 }] }]
  }
}
```

Hooks from user settings run alongside each project's own hooks. The hook appends one line to a local file and exits: no network, no server, no port. It always exits 0, so a broken Officina can never block a session. Measured p95: about 20 ms, process start included.

### Wiring the quota (optional)

The quota comes from the statusline, which receives your rate limits from Claude Code. One statusline per account is enough. Put `officina quota write --tee` in front of your existing statusline command, in a **personal** settings file (`~/.claude/settings.json` or a project's uncommitted `.claude/settings.local.json`), never in a committed one:

```json
{ "statusLine": { "type": "command", "command": "\"C:/path/to/officina.exe\" quota write --tee | python ~/.claude/statusline.py" } }
```

`--tee` passes the payload through unchanged, so your statusline keeps its output. Without a statusline, the gauges say "no data" and the rest of the board works.

### Starting the board

Run `officina-app.exe`. A second launch focuses the existing window. Keep it on a second screen, or press **Pin on top**. The window title shows `Officina (2 waiting)` whenever something waits on you.

| Key | Action |
|---|---|
| `1` to `9` | bring the nth *Needs you* session to the front |
| `Tab`, `Enter` | move between cards, open one |
| `r` | refresh now |

## Privacy

Everything stays on your machine, under `~/.officina`:

- **Hook events**: the first 280 characters of each prompt and answer, plus the event type, session id and folder, in daily files deleted after at most 48 hours. Set `OFFICINA_NO_EXCERPTS=1` (or `no_excerpts = true` in the floor file) to keep no prompt or answer text at all. The hook finds the floor file at `~/.officina/floor.toml` or through `OFFICINA_FLOOR`; one passed only with `--floor` does not reach it.
- **Quota**: the two rate-limit windows, nothing else.
- **Focus statistics**: outcome codes and a hash of the card, never a window title or a path.

Officina opens no network connection and no TCP port. It does not read transcripts.

## Floor file (optional)

Without configuration, a session's project is the name of its folder. A floor file names projects, gives them colours, and can point at another Claude config dir:

```toml
# ~/.officina/floor.toml, or pass --floor <path>, or set OFFICINA_FLOOR
schema = 2
name = "work"
# config_dir = "C:/path/to/claude-config"   # optional, absolute

[[project]]
name = "api"
path = "Api"            # relative to this file, or absolute
color = "blue"          # green, blue or terracotta
autonomy = "free"       # "free" or "guarded"; a schema 2 project without it is guarded
```

Sessions outside every declared project go to `other`; none is hidden. A v0.1 floor (`schema = 1`) still loads, with every project `free`; the relay keys below need `schema = 2`.

## Relay (v0.2, in progress)

In a guarded repository, sessions run the actions of a closed catalogue through Officina instead of calling guarded programs (remote shells, Ansible, WSL) themselves. The floor file declares the catalogue:

```toml
schema = 2
# guarded_programs = ["ssh", "ansible*"]   # optional, replaces the default list

[[project]]
name = "supervision"
path = "Supervision"
autonomy = "guarded"
actions = ["check"]

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"                       # "free", "after-check" or "human"
runner = "wsl"                       # "native" (default) or "wsl"
distro = "Ubuntu"                    # wsl only, optional
program = "/home/user01/venv/bin/ansible-playbook"   # absolute for its runner
env = { ANSIBLE_CONFIG = "/home/user01/ansible.cfg" } # literals only
args = ["--check", "--diff", "-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]
# timeout_s = 540, max_output_kib = 64 when not set

[action.params.playbook]
values = ["alerting.yml", "node-exporter.yml"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"      # anchored: the whole value must match
```

- An `args` item holds at most one `{placeholder}`, with an optional literal prefix and suffix, and always becomes exactly one argument: no shell ever reads it.
- A parameter declares `values` or a `pattern`, never both. Whatever it declares, a value is refused when it starts with `-`, holds a control, bidirectional or invisible formatting character, or has a `.` or `..` path segment; on the `native` runner also when it starts with `/` or `@`.
- An invalid action is skipped and reported with its line; it never invalidates the floor, so the board keeps working.

The relay runs only a catalogue you approved in the window. Approval records a hash of the catalogue's meaning (projects, actions, parameters, guarded programs), not of its text: comments, key order, colours or an invalid action do not change it, any change of meaning does. Only `officina-app.exe` contains the code that approves; `officina.exe`, which sessions can call, does not, and CI checks the shipped binary for it.

In a guarded repository the gate reads each `Bash` command and refuses one that would start a guarded program, even through quotes, wrappers, `bash -c`, an interpreter one-liner or a command substitution, along with a command name built at run time, a PowerShell encoded command, a redirection of Officina's home, and any mention of an `OFFICINA_` setting or of Officina's own files. It is a guardrail, not a boundary: a text match cannot catch a guarded program hidden in a script file (`bash deploy.sh`, `make apply`), called by another tool (`git` over SSH, a Python import), renamed, aliased from an earlier command, or reached by a `cd` from a free repository into a guarded one. Those stay the author's to watch; the limits are listed in the architecture (AD-27).

The gate runs inside `officina hook` as a `PreToolUse` hook on `Bash`. Once the floor guards a project, `officina doctor` adds it to the block to paste:

```json
"PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "\"C:/path/to/officina.exe\" hook", "timeout": 5 }] }]
```

A refusal holds in every permission mode, bypass included: the session sees why, and the actions it may run instead. A repository counts as guarded when the current floor or the approved catalogue says so, so a session cannot lift containment by editing the floor, and when it is the session's root or the shell's current folder, so `cd ..` does not leave it. If the approved catalogue or the floor cannot be read, the gate gets stricter, not looser. Each refusal, and each such fault, is one line in `~/.officina/journal/YYYY-MM-DD.jsonl`, with the first 280 characters of the command unless excerpts are off.

Sessions run the approved actions themselves, from anywhere inside the project's folder:

```bash
officina run --list                                   # this project's actions and what each parameter accepts
officina run check group=lab-web playbook=alerting.yml
```

`officina run` checks, in this order, that the relay is not stopped, that the floor still holds the approved catalogue, that the folder is in an approved project (free or guarded), that the project lists the action, and that every parameter is declared, given once and accepted. It then starts the program as an argument vector, never through a shell, in the project's folder, with the action's `env`; a `wsl` action goes through the system's own `wsl.exe` with `--exec`. Output streams through as it arrives, up to `max_output_kib`, after which a last line says how much was dropped. At `timeout_s` the program and every process it started are stopped, and they also stop if `officina run` itself is killed. Each run leaves a start line and an end line in the journal (exit code, duration, byte counts and SHA-256 of each stream, never the output); each refusal leaves one line.

| Exit code | Meaning |
|---|---|
| the program's own | it ran (any code, full Windows values included) |
| 126 | refused: one stderr line `officina: refused: <why> (<code>)` |
| 124 | stopped at its timeout |
| 127 | the program could not start |
| 64 | usage error |

A program can exit 126 or 124 itself: the stderr line and the journal tell them apart.

An `after-check` action names a `free` action with the same parameters as its `check`, and runs only after that check succeeded with the same parameters, under the same approved catalogue, on the same code, in the last 30 minutes; otherwise it is refused with `check_required` and the check to run first:

```toml
[[action]]
name = "apply"
class = "after-check"
check = "check"                      # a free action with the same parameters
runner = "wsl"
program = "/home/user01/venv/bin/ansible-playbook"
args = ["-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]
```

"The same code" means the same git fingerprint of the project's work tree: `HEAD`, the diff to it, the untracked files git does not ignore and the tracked files flagged `assume-unchanged` or `skip-worktree` (whose edits the diff hides), by content up to 1 MiB and by size beyond. Files your `.gitignore` excludes (vault files, local inventories, retry files) are not bound, so a check that writes such a file does not invalidate itself, and an edit to one does not stop the apply. Outside a git work tree only the parameters bind. `git` is the one on the session's `PATH`, run with external diff tools, textconv drivers and fsmonitor turned off.

`human` actions are refused as not available in this build yet.

`officina relay --json` prints the relay's state once, without writing anything: whether it is stopped, the approved catalogue, a pending change with its diff, the floor's catalogue issues with their lines, and the last 50 journal entries with each run's start and end paired. A run with no end line 60 s past its timeout reads "abandoned" (its `officina run` was killed). The window reads the same contract.

Today the catalogue is parsed, validated and hashed, the gate refuses guarded commands and `officina run` runs `free` and `after-check` actions; `human` actions and the approval banner in the window arrive with the rest of v0.2.

## Command line

| Command | Purpose |
|---|---|
| `officina doctor` | check the setup, print what to paste; exit 1 when something required is missing |
| `officina board --json` | print the board once (exit 3 when the session list could not be refreshed) |
| `officina hook` | record one hook payload from stdin, or answer the relay gate for a `Bash` call (run by Claude Code) |
| `officina quota write [--tee]` | record the rate limits from a statusline payload (run by your statusline) |
| `officina run <action> [name=value ...]` | run an approved relay action from its project (run by sessions) |
| `officina run --list` | list the actions of the current folder's project |
| `officina relay --json` | print the relay's state once: catalogue, pending change, recent activity |

## Roadmap

Each minor version is one PRD and is useful alone.

| Version | Goal |
|---|---|
| **v0.1 The board** | this release: who needs you, quotas, one click to the right session |
| v0.2 The relay | a closed catalogue of read-only probes, so sessions stop asking you to run commands and paste results |
| v0.3 The registry and triage | subjects that survive restarts, finished sessions sorted into history or to resume |
| v0.4 The scene | the isometric workshop: rooms, focus mode, the attention halo |
| v0.5 The orchestrator | a restricted session that sees every subject and proposes work orders, launched in one gesture |
| v0.6 The isolated workshop | autonomous tasks in a container that holds no key, behind an allow-list gateway |
| v0.7 The governor | spends the perishable 5-hour margin on routine tasks, never your weekly quota or your attention |
| v1 | installer, update, Linux build, documentation |

Design documents: [PRD v0.1](docs/prd/prd-v0.1.md), [architecture](docs/architecture-spine.md), [research notes](docs/research-notes.md), [stories](docs/stories/).

## Building from source

Requirements: Rust (pinned by `rust-toolchain.toml`), Node 22 or later, `pnpm`, the MSVC build tools and WebView2 (present on Windows 11).

```bash
cd app && pnpm install && cd ..
cargo build --release -p officina          # the CLI
cd app && pnpm bundle                      # the window, the CLI and the NSIS installer
```

Tests:

```bash
cargo test --workspace --features officina-core/ts      # unit, CLI end to end, vector set, TS bindings drift
cargo test --release -p officina --test perf -- --ignored   # hook latency, concurrent appends
cd app && pnpm check && pnpm lint && pnpm test           # types, lint, component tests
pnpm e2e:fixtures && pnpm e2e                            # Playwright on boards produced by the real CLI
```

The CI workflow in `.github/workflows/ci.yml` is the authoritative list of gates.

If the repository lives in a synchronised folder (OneDrive, Synology Drive, Dropbox), exclude `target/` and `app/node_modules/` from the sync, or set `CARGO_TARGET_DIR` outside it: the sync client locks build files and slows every build.

## License

Apache-2.0. See [LICENSE](LICENSE).
