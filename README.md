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
- **Relay journal** (`journal/`, once you use the relay): one line per run start and end, refusal, gate refusal or fault, approval, stop and resume, with the session id, the action, its parameters and argument vector, the first 280 characters of a refused command unless excerpts are off, and never a program's output.
- **Relay requests and dry runs** (`relay/approvals/`, `relay/outputs/`): each `human` action's request with its parameters and command, kept 24 hours, and the output of each check run, at most 256 KiB, kept 7 days so an approval card can show it.

Officina opens no TCP port and does not read transcripts. Its only network connections are the requests of the HTTP actions an approved catalogue declares, each to the URL it declares, when a session runs one with `officina run`; the hook, the board, `officina relay --json` and the window open none.

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

The relay lets a session run your routine actions itself, repository by repository, instead of asking you to run a command and paste the result: a dry run of a playbook on a lab group, a service status, a known change applied after its check. You declare the actions in the floor file and approve them in the window; sessions call them with `officina run`; a gate in the hook refuses the raw commands they replace; every run is journaled.

### What it is, and what it is not

A session runs as you. It could, in principle, edit Officina's files or call your tools directly. The relay is a guardrail against mistakes and injected instructions, and a record of what was done on your behalf; it is **not a security boundary**. The boundary comes with the isolated workshop (v0.6), where the worker does not hold your identity.

### Free and guarded repositories

Each project of the floor file is `free` or `guarded`:

- **free**: Officina only shows it, as in v0.1. A free project that lists actions can still run them through `officina run`.
- **guarded**, the default for a schema 2 project: the gate refuses a session's direct calls to the programs that reach other machines (remote shells and copies, Ansible, WSL and the Windows distro launchers), and the session runs the actions you approved instead.

Folders outside every declared project are never touched, and a v0.1 floor (`schema = 1`) keeps every project free. The default list of guarded programs lives in the code (`DEFAULT_GUARDED_PROGRAMS`); `officina doctor` prints the list in effect. A floor can replace it with its own `guarded_programs`, a list of program names where a trailing `*` matches a prefix.

### Declaring the catalogue

The floor file declares the actions, and each project the actions it may use:

```toml
schema = 2

[[project]]
name = "infra"
path = "Infra"
autonomy = "guarded"
actions = ["check", "apply"]

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
values = ["site.yml", "monitoring.yml"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"      # anchored: the whole value must match

[[action]]
name = "apply"
description = "Apply a playbook on one lab group, after its check"
class = "after-check"
check = "check"                      # a free action with the same parameters
runner = "wsl"
distro = "Ubuntu"
program = "/home/user01/venv/bin/ansible-playbook"
env = { ANSIBLE_CONFIG = "/home/user01/ansible.cfg" }
args = ["-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]

[action.params.playbook]
values = ["site.yml", "monitoring.yml"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"
```

- An `args` item holds at most one `{placeholder}`, with an optional literal prefix and suffix, and always becomes exactly one argument: no local shell ever reads it. The program may still read a value further: ssh hands the arguments after its destination to the remote host's shell as one command line, and Ansible templates extra vars and module arguments and expands `~` and `$VAR` in paths. Give such parameters a `values` list, or a pattern without whitespace, shell characters or template characters (`{`, `%`). The catalogue reports these values as notices (`NoticeKind`, one line of explanation each) on the banner, in `officina relay --json` and in `officina doctor`, as warnings. A `pattern` on such a parameter that admits one of the characters of `FORWARDED_PROBE_CHARS` (probed with short values built around them) drops the action with the issue `forwarded_pattern`, unless the action sets `forwarded_values = "accepted"`: your word that whatever the pattern admits may reach that remote shell or template. A `values` list is always accepted. The probe finds the common broad patterns; it is not a proof, a `values` list is.
- A parameter declares `values` or a `pattern`, never both. Whatever it declares, a value is refused when it starts with `-`, holds a control, bidirectional or invisible formatting character, or has a `.` or `..` path segment; on the `native` runner also when it starts with `/` or `@`.
- An invalid action is skipped and reported with its line; it never invalidates the floor, so the board keeps working.

The three classes: a `free` action runs; an `after-check` action runs only after its `check` succeeded (see below); a `human` action runs once per approval you give in the window, and may name a `check` too, its dry run, which must succeed first and whose output the approval card shows:

```toml
[[action]]
name = "apply-shared"
description = "Apply a playbook to a shared group, after its dry run and your click"
class = "human"
check = "check"                      # optional: its dry run, a free action with the same parameters
runner = "wsl"
program = "/home/user01/venv/bin/ansible-playbook"
args = ["-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]
```

A dry run is only as harmless as the program makes it. For Ansible, `--check` does not stop a task that sets `check_mode: false`, nor one that delegates (`delegate_to`, `local_action`, `run_once`): a check action is harmless only if no task it reaches does either.

### Approving it in the window

The relay runs only a catalogue you approved in the window. Approval records a hash of the catalogue's meaning (projects, actions, parameters, guarded programs), not of its text: comments, key order, colours or an invalid action do not change it, any change of meaning does. Only `officina-app.exe` contains the code that approves; `officina.exe`, which sessions can call, does not, and CI checks the shipped binary for it.

When the floor's catalogue differs from the approved one (yours or a session's edit), the header shows an amber "catalogue change" chip and the board a banner with the change: each action added (with the program and arguments it would run), removed or changed field by field, the projects and guarded programs that changed, and the actions the floor skips as invalid, with their lines. **Approve this catalogue** approves exactly what the banner shows, and only on a mouse click: no key approves. If the floor changed again in between, nothing is approved and the banner shows the new change. The window approves the relay's floor (`OFFICINA_FLOOR`, else the approved one, else `~/.officina/floor.toml`), whose path the banner names, even when it shows another floor's board.

**Stop relay**, in the header, makes `officina run` refuse every action until you press **Resume relay**; a terracotta "relay stopped" chip and a banner say so meanwhile. Stopping does not touch the gate, which keeps guarding your projects. Each stop and resume is one journal line.

### Setting it up with `officina doctor`

Run `officina doctor` after each floor edit. Once the floor uses the relay, it checks it end to end:

- the relay floor, and whether it differs from the floor doctor itself reads: the window approves the relay's floor, so a window started with another `--floor` shows one floor's board and approves another;
- the catalogue's issues with their lines, the approval state (approved, change pending, nothing approved yet), the projects with their actions, and the guarded programs in effect;
- that each action's program resolves: a `native` program is an existing file; for a WSL action the system's own `wsl.exe` exists and, in the action's distro, `/usr/bin/env`, `/usr/bin/timeout` and the program are executable;
- for each project with an action that binds its files (one naming a check, or a `human` action), whether its files bind: the `git` it found and how many files it binds, or why it cannot (no `git` where Git for Windows records it, a tree past the fingerprint's limits, a file it cannot read), which fails, since those actions then refuse;
- the decisions the window did not make (see the Relay view below), which fail until you acknowledge them;
- warnings: an action whose timeout, plus the longest wait of a `human` action, can outlive the Bash tool's 10-minute cap; an action naming a check its project does not list (it could never run); a project with actions that bind files outside a git work tree (only the parameters bind); unreadable journal lines and gate errors of the last 24 hours.

A missing `PreToolUse` registration while a project is guarded, a program that does not resolve, files that cannot be bound, an unacknowledged anomaly and a relay floor that cannot be read are failures (exit 1); everything else warns. The gate runs inside `officina hook` as a `PreToolUse` hook whose matcher names every shell tool of `SHELL_TOOLS` and every file tool of `FILE_TOOLS`, the constants in the relay's core: once the floor guards a project, doctor adds that registration to the block to paste, and names the tools a matcher you merged earlier leaves out.

For each guarded project, doctor also prints a paragraph to paste into that repository's `CLAUDE.md`, generated from the approved catalogue: use `officina run` for the listed actions, `officina run --list` to see them and their values, the Bash tool's longest timeout for relay calls, read a refusal's reason before retrying, and run commands with the tools the gate reads. With nothing approved it says the catalogue awaits your approval and lists no action. Officina never writes it: paste it again after approving a change.

### The gate

In a guarded repository the gate reads the command of every call to a shell tool it lexes (`SHELL_TOOLS` gives each tool its rule) and refuses one that would start a guarded program, even through quotes, wrappers, `bash -c`, an interpreter one-liner or a command substitution, along with a command name built at run time, a PowerShell encoded command, a redirection of Officina's home, an `officina` call with `PATH`, a `GIT_` variable or another variable of `OFFICINA_ENVIRONMENT_NAMES` reassigned, and any mention of an `OFFICINA_` setting or of Officina's own files. A refusal holds in every permission mode, bypass included: the session sees why, and the actions it may run instead.

Once the floor guards a project, a file tool (`FILE_TOOLS`: the tools that write a file, Write and Edit among them) aimed anywhere under Officina's home is refused from every project, guarded, free or undeclared: the approvals, the stop flag, the decision record, the journal and the home's floor are written by Officina only. Edit Officina's files yourself, outside a session.

A shell tool whose language Officina does not read is refused whole in a guarded project instead: that is the PowerShell tool, which Claude Code turns on by default on Windows for claude.ai accounts. Sessions there keep the Bash tool, and the `CLAUDE.md` paragraph doctor prints tells them to use it. Free projects and undeclared folders keep every tool.

A repository counts as guarded when the current floor or the approved catalogue says so, so a session cannot lift containment by editing the floor, and when it is the session's root or the shell's current folder, so `cd ..` does not leave it. Those folders, and a file tool's target, are resolved by an opened handle before the gate compares them, so an 8.3 short name, a junction, a symbolic link or a `subst` drive leading into a guarded project or into Officina's home counts as the folder it leads to. A guarded project's own path is resolved the same way, once per set of paths, so a floor that declares it through an alias guards the real folder too; a free project's path is compared as written. If the approved catalogue or the floor cannot be read, the gate gets stricter, not looser. Each refusal, and each such fault, is one journal line, with the first 280 characters of the command unless excerpts are off.

The gate is a text match, so it has limits: a guarded program hidden in a script file, started by another tool, renamed, or aliased by an earlier command escapes it. The architecture lists what it cannot catch ([AD-27](docs/architecture-v0.2.md)), and the gate's vector set (`tests/vectors/gate.json`) keeps each such case marked as a limit, so an improvement shows up as a changed case.

### Running actions

Sessions run the approved actions themselves, from anywhere inside the project's folder:

```bash
officina run --list                                   # this project's actions and what each parameter accepts
officina run check group=lab-web playbook=site.yml
```

`officina run` checks, in this order, that the relay is not stopped, that the floor still holds the approved catalogue, that the folder is in an approved project (free or guarded), that the project lists the action, and that every parameter is declared, given once and accepted. It then starts the program as an argument vector, never through a shell, in the project's folder, with the action's `env`; a WSL action goes through the system's own `wsl.exe` with `--exec`. The program finds other programs through the `PATH` Windows builds from the registry for you (the machine's, then yours), never the caller's: the variables of `SYSTEM_ENV` come from there, or are removed, before the action's `env` applies. Output streams through as it arrives, up to `max_output_kib`, after which a last line says how much was dropped. At `timeout_s` the program and every process it started are stopped, and they also stop if `officina run` itself is killed. Each run leaves a start line and an end line in the journal (exit code, duration, byte counts and SHA-256 of each stream, never the output); each refusal leaves one line.

| Exit code | Meaning |
|---|---|
| the program's own | it ran (any code, full Windows values included) |
| 126 | refused: one stderr line `officina: refused: <why> (<code>)` |
| 124 | stopped at its timeout |
| 127 | the program could not start |
| 64 | usage error |

A program can exit 126 or 124 itself: the stderr line and the journal tell them apart.

An `after-check` action names a `free` action with the same parameters as its `check`, and runs only after that check succeeded in the same project with the same parameters, under the same approved catalogue, on the same code, in the last 30 minutes; otherwise it is refused with `check_required` and the check to run first.

A `human` action asks you first. Its call writes a request and waits for your decision, 110 seconds by default (`--wait <s>`, at most 540): approved, it runs at once; rejected, it is refused, and the same request stays refused for 15 minutes; with no decision in time, the call is refused with "approval still pending; run the same command again to keep waiting", and calling again waits on the same request. An approval runs once, within 15 minutes; a request expires after 60. A `human` action naming a check makes a request only after that check succeeded, and the request is bound to that dry run and the files it ran on; a `human` action naming none is bound to the files as they were when it asked. Just before running, `officina run` reads the files again, and if they changed it refuses, `check_not_satisfied` after a dry run or `tree_changed` otherwise, and leaves your approval unused, so a session cannot edit the playbook while its call waits for your click.

"The same code" and "the same files" mean the same fingerprint of the project's git work tree: `git` lists the tracked files and the untracked ones it does not ignore, and Officina reads every one and hashes its bytes itself, so neither git's stat cache (an edit whose size and time are put back) nor a filter the repository configures can hide a change. `git` is the one Git for Windows records in the registry (machine, then user), never the one on the caller's `PATH`, run with the caller's `GIT_` variables removed and fsmonitor off; the journal records which `git` listed the files. A file over 64 MiB, more than 20,000 files or 512 MiB in all refuse rather than bind less, and so do a file that cannot be read and a git work tree without that `git`. The binding has limits: files git ignores, by your `.gitignore` or the repository's own exclude rules (vault files, local inventories, retry files), are not bound, so a check that writes one does not invalidate itself, and an edit to one does not stop the run; programs, collections and roles outside the work tree are not bound either. Outside a git work tree only the parameters bind, and doctor says so. The refusals of the binding are `TREE_BINDING_REFUSALS`, each leaving an approval unused.

Requests show first in **Needs you**, oldest first, with an amber border: the project, the action and its description, the exact command as one chip per argument, the session that asked (or "session unknown"), the request's age and when it expires, and **Approve** and **Reject**. The session is the one the call reports itself, from its own environment: it is advisory, not proof of which session asked. The command shown is rebuilt from the approved catalogue, never read from the request file, which any program running as you could write; a request that does not rebuild (forged, made under another catalogue, or naming an action no longer approved) is listed as not approvable, with the reason, and has no button. A request with a dry run shows its output under the command. Every card says what its files are bound to: "tracked and unignored files unchanged since the dry run", or "since the request" for an action without one, while the project's fingerprint still matches, the number of files bound beside it; a warning once they changed, since the run will then refuse; and "no file binding: not a git tree" outside git. No key approves or rejects: only a mouse click or a pen tap does, and keys 1 to 9 still go to session cards only. That rule guards against a stray key, not against code running inside the window's page, which could call the approval directly; the page renders every text as text and loads no inline script or style (its Content Security Policy forbids them) so that no such code gets in. The title, the taskbar dot and the waiting pill count the requests that can be approved. Each request and each decision is one journal line; the window deletes approval files after 24 hours and kept dry-run outputs after 7 days, and `officina run` never deletes either.

`officina relay --json` prints the relay's state once, without writing anything: whether it is stopped, the approved catalogue, a pending change with its diff, the floor's catalogue issues with their lines, and the last 50 journal entries with each run's start and end paired. A run with no end line 60 s past its timeout reads "abandoned" (its `officina run` was killed). The window reads the same contract.

### HTTP actions and credentials

An action can also read an HTTP endpoint, a health check or a monitoring API, with `kind = "http"`. It declares a `method` and a `url` instead of a program, and optionally a `credential`:

```toml
[[credential]]
name = "monitoring"
hosts = ["monitoring.example.com"]   # the only hosts its secret is ever sent to
header = "Authorization"
scheme = "Bearer"

[[action]]
name = "health"
description = "Read one check of the monitoring API"
class = "free"
kind = "http"
method = "GET"                       # GET or HEAD
url = "https://monitoring.example.com/api/{check}?verbose=1"
credential = "monitoring"            # optional

[action.params.check]
values = ["health", "ready"]
```

- The URL's scheme is `https`, or `http` for an action without a credential; its host and port are literal, and it carries no user or password. A placeholder fills one whole path segment or one whole query value, never a query key. Its value meets the same value rules as any other, then every byte outside letters, digits and `-._~` is percent-encoded, so a value can add neither a segment nor a parameter. The rules and their cases are the vector set `tests/vectors/url.json`.
- The request follows no redirect and uses no proxy (proxy variables are ignored); it waits at most `timeout_s`, capped at `MAX_HTTP_TIMEOUT_S`; the body is read up to `max_output_kib`, then the connection is dropped and a last line says so; `HEAD` reads no body. TLS trusts the Windows certificate store, an enterprise CA included: no root set is bundled.
- `officina run` prints `HTTP <status> <reason>`, then the body. It exits 0 for a 2xx status, 1 for any other with one stderr line `officina: http <status>`, 124 at the timeout, 127 when no response came (name resolution, connection or TLS failure).
- The journal records the method, the URL with its values and the credential's name instead of an argument vector, then the status, the body's byte count and its SHA-256.

A credential's secret is never in the floor. It lives in the Windows Credential Manager as the generic credential `officina:<name>`: `officina doctor` checks each declared credential and, for a missing one, prints the `cmdkey` line that stores it, `cmdkey /generic:officina:<name> /user:<user> /pass`, without the secret, so `cmdkey` asks for it and it stays out of your shell's history. A missing credential that an approved action uses fails doctor; an unused one warns.

The header `<header>: <scheme> <secret>` is added only when the URL's host is one of the credential's `hosts`: the same name, any case, any port, no wildcard. The header cannot be one that frames the request (`FORBIDDEN_CREDENTIAL_HEADERS`). `officina run` reads the secret after every other check and just before the request; for a `human` action, before your approval is used, so a missing credential leaves the approval for the next call. The secret goes into that header and nowhere else: not the output, the journal, an approval file or the Relay view.

Adding credentials and HTTP actions, then `forwarded_values`, changed what an approval covers: after each upgrade, the catalogue shows as changed once and is approved again.

### The Relay view

Once the relay is in use, the header gains a **Board** / **Relay** switch; the chips and **Stop relay** stay in the header in both views. The Relay view reads the same contract as `officina relay --json` and shows, in order:

- whether the relay runs or is stopped, with its own **Stop relay** button, the relay floor, and why that floor or the approved catalogue cannot be read, when one cannot;
- the decisions this window did not make, when there are any: an approval file for a request you never decided, a run journaled with such an approval, an approved catalogue you did not approve, and a stop flag gone while you hold the relay stopped. The window keeps a record of its own decisions (`relay/decisions.json`) from the first time it opens, and `officina run` leaves unused an approval that record does not name, refusing `approval_not_given`; any program running as you can write a relay file, so these stop and show the cheap forgeries, not a session that also rewrites the record. Each stays listed, and fails `officina doctor`, until you **Acknowledge** it with a mouse click;
- a pending catalogue change with its diff and **Approve this catalogue**, as on the board's banner;
- the floor's issues with their lines;
- the approved catalogue: each action's class, the projects that list it, what it starts (or, for an HTTP action, its method and URL template), its parameters and limits; then the credentials, with the hosts they go to, the actions that send them and whether the Credential Manager holds them; then the projects and the guarded programs. The window checks the credentials when the relay or its floor changes and at most once a minute otherwise; `officina relay --json` never asks the store and reports them unchecked;
- the recent activity, newest first: time, project, action, parameters, outcome (an exit code, a timeout, a refusal and its reason, a gate deny with the command's excerpt, a gate error) and duration, and "abandoned" for a run whose end never came;
- the number of unreadable journal lines and of gate errors in the last 24 hours.

Approval requests are not listed there: you decide them on their cards in Needs you. Everything the view shows is rendered as text, never as markup.

The journal keeps 90 days of history (`JOURNAL_RETENTION_DAYS`): the window deletes older day files when it starts and then every hour. `officina run` and the gate never delete a journal file.

### What works today

Usable today: declare a catalogue, approve it in the window, let sessions run its `free` and `after-check` actions with `officina run` and its `human` actions after your click in Needs you, contained by the gate, journaled, stoppable from the window, audited in the Relay view, and checked by `officina doctor`, which prints each guarded project's `CLAUDE.md` paragraph; HTTP read actions with a credential from the Windows Credential Manager, sent only to the hosts it is bound to.

## Command line

| Command | Purpose |
|---|---|
| `officina doctor` | check the setup, the relay included, and print what to paste; exit 1 when something required is missing |
| `officina board --json` | print the board once (exit 3 when the session list could not be refreshed) |
| `officina hook` | record one hook payload from stdin, or answer the relay gate for a shell tool's call (run by Claude Code) |
| `officina quota write [--tee]` | record the rate limits from a statusline payload (run by your statusline) |
| `officina run <action> [name=value ...]` | run an approved relay action from its project (run by sessions) |
| `officina run --list` | list the actions of the current folder's project |
| `officina relay --json` | print the relay's state once: catalogue, pending change, recent activity |
| `officina version` | print the version |

## Roadmap

Each minor version is one PRD and is useful alone.

| Version | Goal |
|---|---|
| **v0.1 The board** | this release: who needs you, quotas, one click to the right session |
| v0.2 The relay | in progress: a closed catalogue of actions sessions run on their own, gated and journaled, so they stop asking you to run commands and paste results |
| v0.3 The registry and triage | subjects that survive restarts, finished sessions sorted into history or to resume |
| v0.4 The scene | the isometric workshop: rooms, focus mode, the attention halo |
| v0.5 The orchestrator | a restricted session that sees every subject and proposes work orders, launched in one gesture |
| v0.6 The isolated workshop | autonomous tasks in a container that holds no key, behind an allow-list gateway |
| v0.7 The governor | spends the perishable 5-hour margin on routine tasks, never your weekly quota or your attention |
| v1 | installer, update, Linux build, documentation |

Design documents: [PRD v0.1](docs/prd/prd-v0.1.md), [PRD v0.2](docs/prd/prd-v0.2.md), [architecture](docs/architecture-spine.md) and its [v0.2 addendum](docs/architecture-v0.2.md), [research notes](docs/research-notes.md), [stories](docs/stories/).

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
