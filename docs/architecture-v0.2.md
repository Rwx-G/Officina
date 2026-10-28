---
title: "Architecture addendum: Officina v0.2 - The Relay"
author: Rwx-G
date: 2026-10-27
status: approved 2026-10-27 (owner delegated design decisions; dependencies of section 2 accepted, HTTP ones only if AD-36 is built)
inputs:
  - prd/prd-v0.2.md
  - (requirement challenge, not published)
  - architecture-spine.md
  - research-notes.md
---

# Architecture addendum: Officina v0.2

AD-1 to AD-20 stay in force; this addendum adds AD-21 onward. **Gate** marks a fact to verify on the workstation before the dependent story closes. The relay is a guardrail and a record, not a boundary (PRD section 1): each decision stops mistakes and injected instructions on the obvious path, none claims to stop a process running as the author.

## 1. Decisions

### AD-21: Module layout and the window's authority

- `officina-core::relay` (pure): `catalogue` (schema, validation, canonical form, hash, diff), `params`, `argv`, `shell` (lexer), `gate` (decision), `classes`, `journal` and `approvals` (types), `view` (payloads). `regex` and `sha2` are pure and live here; time enters as `now_ms`.
- `officina-host::relay`: `home` (paths, reads), `journal` (locked append, read), `run` (execution), `gate` (hook glue), `authority`; `platform/windows.rs` gains the job object and `GetSystemDirectoryW`.
- `officina`: `hook` dispatches `PreToolUse`; new `run` and `relay --json` (read-only, also feeds e2e fixtures).
What only the author may decide (catalogue approval, approval decisions, stop and resume) lives in `officina-host::relay::authority`, compiled only under the host feature `window-authority`. `officina-app` enables it; the `officina` binary does not, so the exe a session can call does not contain that code. CLI integration tests enable it through a dev-dependency to play the window. Trade-off: one feature flag; in exchange "no CLI command can approve" is a property of the binary, not a promise.

### AD-22: Floor schema 2

```toml
schema = 2
guarded_programs = ["ansible*", "ssh", "scp", "sftp", "rsync", "wsl"]   # optional, replaces the default

[[project]]
name = "supervision"
path = "Supervision"
autonomy = "guarded"                 # "free" | "guarded", default "guarded"
actions = ["check", "apply"]

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"                       # "free" | "after-check" | "human" | "auto"
runner = "wsl"                       # "native" (default) | "wsl"
distro = "Ubuntu"                    # wsl only, optional
program = "/home/user01/venv/bin/ansible-playbook"
env = { ANSIBLE_CONFIG = "/home/user01/ansible.cfg" }   # literals only
args = ["--check", "--diff", "-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]

[action.params.playbook]
values = ["alerting.yml", "node-exporter.yml"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"
```

- `runner` replaces the PRD's "`program` is a path or `wsl`": `program` is always absolute, Windows for `native`, POSIX for `wsl`.
- `env` answers PRD open question 2 (virtualenv `PATH`, `ANSIBLE_CONFIG`).
- An `args` item holds at most one placeholder, with optional literal prefix and suffix (`--limit={group}`); it always yields exactly one argument.
- **Schema 1 still loads, every project `free`.** Upgrading Officina must not start denying `ssh` in repositories the author never opted in. The new keys in a schema 1 file are an error naming `schema = 2`. Trade-off: the `guarded` default applies only to files rewritten for v0.2; the schema bump is the opt-in.

### AD-23: Two-phase parse

Phase 1 parses projects strictly (as v0.1) and `action` as `Vec<toml::Spanned<toml::Value>>`: a malformed project still invalidates the floor, nothing about actions can. Phase 2 converts each action alone (`deny_unknown_fields`) and validates it; a failure becomes `ActionIssue { name, line, message }` and the action is dropped. Rules, each a typed error with a test:

- name `^[a-z][a-z0-9-]{0,31}$`, unique; `program` absolute for its runner, no placeholder; `distro` `^[A-Za-z0-9._-]{1,64}$`;
- every placeholder names a declared parameter and every parameter is used (an unused one could be smuggled later);
- a parameter has exactly one of `values` (each passing AD-30) or `pattern` (at most 256 chars, compiled `^(?:...)$`, 1 MiB size limit);
- `after-check` names, in `check`, a valid `free` action with the same parameter names;
- `timeout_s` 1..=3600, default 540; `max_output_kib` 1..=1024, default 64;
- `auto` is reported "reserved for v0.5"; `kind = "http"` is reported "not in this build" until AD-36.

A project naming an unknown or dropped action gets an issue and keeps its other actions. Issues go to `RelayView.issues` and doctor, never to the board's `floor_error` (FR-4). The gate runs phase 1 only and never compiles a regex.

**Gate:** `toml::Spanned` inside arrays of tables in toml 0.9 is proved by the first floor story; fallback is `toml::Value` with lines located by the action's `name`.

### AD-24: Canonical catalogue, hash, approval

The hash covers meaning, not text. `CanonicalCatalogue` (core) holds: `format: 1`; the normalised floor path; `guarded_programs` with defaults applied, sorted, deduplicated; projects by name (resolved path, autonomy, sorted actions); valid actions by name with every default explicit, parameters by name, `values` sorted. Excluded: colours, floor `name`, `config_dir`, `no_excerpts`, comments, invalid actions. Hash: SHA-256 of `serde_json::to_vec` of it, lowercase hex. A golden test pins the hash of a fixture floor; changing the form fails it until `format` is bumped, which invalidates approvals on purpose.

`<home>/relay/approved.json` (atomic write, window only) holds `approved_at_ms`, `hash` and the full `canonical` value. The copy gives the window its diff (core `catalogue::diff`: actions and projects added, removed, changed per field; `guarded_programs` changes) and gives the gate a snapshot a floor edit cannot loosen.

`approve_catalogue(hash)` recomputes the current hash and writes only if it equals the one displayed: a floor edited between display and click is not approved.

**Relay floor resolution** (gate and run): `OFFICINA_FLOOR`, else the floor path in `approved.json`, else `<home>/floor.toml`. A different floor fails the hash check. Doctor warns when the window's `--floor` and the relay's floor differ.

**FR-4 moves to Slice A.** Without it a session can add `program = "C:/Windows/System32/cmd.exe"` and run it next call: catalogue approval is the only control on what the relay runs. Slice A ships it as a banner with the diff and an Approve button.

### AD-25: Relay files under the Officina home

| Path | Writer | Content |
|---|---|---|
| `relay/approved.json` | window | AD-24 |
| `relay/stopped` | window | `{"since_ms":...}`, present means stopped |
| `relay/approvals/<id>.request.json` | `officina run` | AD-33 |
| `relay/approvals/<id>.approved.json`, `.rejected.json` | window | `decided_at_ms`, `request_key` |
| `relay/approvals/<id>.consumed.json` | `officina run`, by exclusive create | AD-33 |
| `journal/YYYY-MM-DD.jsonl`, `journal/.lock` | run, gate, window | AD-32 |

"Window only" is held by AD-21 and by the gate denying commands that name these paths. It is not held against a process writing the files directly: accepted, PRD section 1.

### AD-26: Hook dispatch and gate decision

`officina hook` parses stdin to `serde_json::Value` once and branches on `hook_event_name`: `PreToolUse` goes to the gate and is never appended to the event buffer (volume, and `tool_input` can carry secrets); every other event takes the v0.1 path unchanged.

Gate, first match wins:

1. `tool_name != "Bash"` or no `tool_input.command`: no decision.
2. Normalise `cwd` (`C:\x`, `C:/x`, Git Bash `/c/x` all give `c:/x`). Find the project by longest prefix in the current floor (phase 1) and in the approved snapshot; guarded if either says so. A session setting its repository `free` does not lift containment until the author approves.
3. Not guarded or no project: no decision.
4. Deny when: the lexer fails; a guarded program is in command position; a command-position word is dynamic (expansion or substitution); the text contains `OFFICINA_` (no redirection of home or floor by environment); the text names the relay or journal directory (either slash, case-insensitive).
5. Otherwise no decision. `officina run` enforces the catalogue itself (AD-29), so the PRD's "single `officina run` without operators" rule is dropped: it bought nothing and would deny `officina run ... | head`.

Deny: the verified JSON shape on stdout, exit 0. The reason names the program and project, lists the project's actions as `officina run <name> <param>=<...>`, points to `officina run --list`, cut at 2 KiB. Each deny is journaled.

Doctor's block gains `PreToolUse` with `"matcher": "Bash"`; `HOOK_EVENTS` becomes (event, matcher) pairs and `missing_hook_events` checks the matcher. `Bash` only: adding `Write|Edit` would put about 40 ms (E9) on every file edit of every session to guard files that are not a boundary anyway.

### AD-27: The command lexer

`core::relay::shell::lex`: linear, no regex, recursion depth 3, input over 256 KiB is an error.

- **Quoting:** `'...'` literal; `"..."` with `\` escaping `"`, `\`, `$`, backtick; `\` outside quotes escapes one character; unterminated quote is an error.
- **Separators** (next word in command position): newline, `;`, `&`, `&&`, `|`, `||`, `|&`, `(`, `)`, `{`, `}`, `!`, and reserved words (`if then else elif do while until case function time` and their closers).
- **Redirections** (`<`, `>`, `>>`, `<&`, `>&`, `&>`, `<>`, `<<<`, fd digits) and their targets are not words. **Heredoc** bodies are consumed to the terminator and treated as data, except for interpreters.
- **Dynamic text:** `$(`, backtick, `<(`, `>(` anywhere, double quotes included, mark a substitution whose content is lexed recursively; `$name`, `${` mark an expansion.
- **Command position:** first word after `NAME=value` prefixes, skipping wrappers and their options: `env` (with `-u`), `sudo`, `doas`, `nohup`, `nice`, `time`, `command`, `exec`, `builtin`, `stdbuf`, `timeout <d>`, `xargs`, `watch`, `setsid`; `find -exec|-execdir|-ok` puts the next word in command position.
- **Program name:** last segment after `/` or `\`, lowercased, `.exe` stripped (`/usr/bin/ssh`, `C:\Windows\System32\OpenSSH\ssh.exe`, `SSH.EXE` all give `ssh`). A list entry ending in `*` is a prefix.
- **Interpreters:** `bash sh zsh dash ksh -c <s>` and `eval`: the string is lexed recursively. `cmd /c`, `powershell`, `pwsh`, `python -c`, `node -e`, `perl -e`, `ruby -e`: the string is scanned for a guarded name as a whole word. An interpreter reading stdin (after `|`, from a heredoc or `<`): the whole text is word-scanned.

`wsl` in the default list covers every `wsl.exe -- ansible-playbook` route.

**What a string match cannot catch:** scripts in files (`bash deploy.sh`, `make apply`); a guarded program called by another tool (`git` over SSH, `rsync -e`, Python importing ansible); renamed binaries; aliases or functions from an earlier call; `cd` from a free repository into a guarded one; anything outside the Bash tool.

Behaviour is held by `tests/vectors/gate.json` (hand-written, redacted, formatter-exempt): `command`, `guarded_programs?`, `cwd_guarded`, `expect`, `matched?`. A coverage test iterates the core constants (default guarded programs, separators, wrappers, interpreters) and fails when one appears in no case.

### AD-28: Gate budget and failure mode

In-process work (payload parse, two small reads, phase 1 parse, lexing) under 3 ms p95 for a 64 KiB command and a 50-action floor; end to end, spawn cost over an empty process plus at most 5 ms (AD-17 harness). No regex, no hash, no child, no lock except the journal append on a deny.

**Fail open:** an internal error or panic gives no decision and exit 0, like every hook path (AD-16). The gate is containment; `officina run` is the authority and fails closed. Gate errors are journaled as `gate_error` and shown in the Relay view and doctor.

*Amendment, 2026-10-28 (story 2.17, D-452):* the gate fails closed where it guards. A panic while deciding falls back to locating the call alone: a call in a guarded project, or one the fallback cannot locate, is denied with a reason that says the gate failed; a call outside every guarded project still gets no decision. The hook still exits 0 and never 2. Failures the process cannot catch (it does not start, it outlives its `timeout`, it dies) are Claude Code's to handle: doctor prints the `PreToolUse` entry with `onFailure: "block"` (Claude Code 2.1.295 and later), fails on an installed entry without it, and fails on an older Claude Code.

### AD-29: `officina run`

`officina run <action> [name=value ...] [--wait <s>]` and `officina run --list` (read-only: the project's approved actions and allowed values). Tokens split at the first `=`; a token without `=`, a duplicate or no action is a usage error.

Order, failing closed, each refusal journaled with a reason code:

1. stop flag (AD-34);
2. current canonical hash equals the approved one, else "catalogue changed, approval pending in Officina";
3. project of the process cwd (longest prefix, approved catalogue);
4. action in the project's `actions`;
5. parameters: all declared present, none extra, each passing AD-30;
6. class (AD-33); a `human` approval is consumed last, just before the spawn;
7. build the vector, journal `run_started`, spawn (AD-31), journal `run_finished`.

Exit codes follow shell conventions a session already reads: the program's code passed through (`std::process::exit`, full Windows code); 126 refused; 124 timeout; 127 could not start; 64 usage; never 2. A refusal prints one stderr line `officina: refused: <reason>`.

Session id from `CLAUDE_CODE_SESSION_ID`: **gate passed (E13)**, Bash children receive it. When absent (a terminal outside Claude Code) the journal records `null` and cards say "session unknown".

### AD-30: Parameter values and argument vectors

Value rules (core; also applied to every `values` entry at validation): 1 to 256 chars; refused when it starts with `-`, contains a control character, a bidi or invisible formatting character (U+200B-200F, U+202A-202E, U+2066-2069, U+FEFF), or has a `.` or `..` segment when split on `/` and `\`. Then `values` membership or anchored `pattern`.

- `native`: `Command::new(program)`, cwd the project folder, `CREATE_NO_WINDOW`, `env` added, stdin null.
- `wsl`: program `<GetSystemDirectoryW>\wsl.exe` (never `PATH` or `%SystemRoot%`, which a session can set), args `[-d <distro>] --cd <project folder> --exec /usr/bin/env K=V ... timeout -k 10 <timeout_s> <program> <args...>`. `--exec` is mandatory: without it WSL hands the line to the Linux login shell and `;` in a value would run.
- `wsl.exe` rebuilds the Linux argv from the Windows command line. **Gate passed (E13):** spaces, double quotes, `$`, `*`, `;`, backslashes, a trailing backslash and accented letters all arrive intact through `--exec`, so `wsl` values follow the same rules as `native` ones, with no extra charset. The vector set keeps a case per character class so a WSL change shows up.

Vectors for both runners in `tests/vectors/argv.json`: action, params, expected vector or refusal.

### AD-31: Process tree, timeout, output

- **Job object.** Before any spawn, `officina run` creates a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and assigns itself (nested jobs, Windows 8+). Children inherit it with no race between spawn and assignment; if `officina run` dies, including when the Bash tool kills it, the tree dies too. On our own timeout: journal `run_finished` (`timeout`), print the stderr line, then `TerminateJobObject(job, 124)`, which ends the tree and `officina run` with 124. Unsafe stays in `platform/windows.rs` with `// SAFETY:` comments. Non-Windows kills the child only (Linux is v1).
- **WSL.** **Gate passed (E13):** killing `wsl.exe` ends the Linux process it started. `timeout -k 10` stays inside as a second line, for a WSL release that changes this.
- **Wait:** `try_wait` every 50 ms against the deadline (AD-3).
- **Output:** two reader threads copy stdout and stderr through as bytes arrive, sharing a `max_output_kib` budget; past it they keep draining (a full pipe stalls the program) and a last line says `[officina: output cut at N KiB, M bytes dropped]`. Each stream is SHA-256 hashed over all its bytes.
- **Bash tool timeout** (2 min default, 10 max): default `timeout_s` 540 fits the cap; doctor warns above it; the CLAUDE.md paragraph doctor prints tells sessions to pass 600000 ms for relay calls.

### AD-32: Journal

`<home>/journal/YYYY-MM-DD.jsonl` (UTC, AD-9), lines `{v:1, ts, kind, ...}`:

| kind | fields |
|---|---|
| `run_started` | `run_id`, `project`, `session_id`, `action`, `class`, `params`, `argv`, `catalogue_hash`, `approval_id?`, `tree?` |
| `run_finished` | `run_id`, `outcome` (`exited`, `timeout`, `spawn_failed`), `exit_code?`, `duration_ms`, byte counts and SHA-256 per stream |
| `refused` | `project?`, `session_id`, `action`, `params?`, `reason`, `detail` |
| `gate_denied`, `gate_error` | `project`, `session_id`, `matched?`, `command_excerpt` (280 chars, dropped when excerpts are off) |
| `catalogue_approved` | `hash`, `previous_hash?` |
| `approval_requested`, `_given`, `_rejected` | `approval_id`, `request_key` |
| `relay_stopped`, `relay_resumed` | none |

Two lines per run keep a run cut short visible: a `run_started` without `run_finished` past its timeout plus 60 s reads "abandoned". No output is stored.

Append: build the line, take an exclusive lock on `journal/.lock` with `std::fs::File::try_lock` (stable since Rust 1.89, no crate), retry every 5 ms up to 2 s, one `write_all` through `append_line`, unlock. A separate lock file means readers never meet a locked range. Lock timeout: `run` refuses before spawning; a gate deny still stands. Readers take two days and count unparsable lines (AD-10). The window deletes journal files older than 90 days, hourly.

### AD-33: Classes

**after-check.** Core `classes::check_satisfied` takes journal lines, project, check action, canonical params, catalogue hash, tree and `now_ms`: true when a `run_started` paired with `run_finished(exited, 0)` matches all of them within 30 minutes. Two days of journal are read, so a check at 23:50 UTC counts at 00:10.

The **tree fingerprint** closes the gap where a session checks, edits the playbook, then applies unchecked code. For check actions and `after-check` actions, `run` records SHA-256 of `git rev-parse HEAD`, `git diff HEAD --binary` and untracked paths with sizes, run in the project folder. Outside git the field is absent and only parameters bind; doctor says so. Trade-off: two or three `git` spawns per check or apply, negligible next to a playbook.

**human.** Request key = SHA-256 of canonical `(project, action, params, catalogue_hash)`.

1. A rejection for this key younger than 15 minutes: refused "rejected in Officina; do not retry".
2. Consume: `fs::rename` an `<id>.approved.json` for this key decided under 15 minutes ago to `<id>.consumed.json`. The first rename wins, a concurrent `run` gets `NotFound`: single use without a lock.
3. Else reuse a pending request for this key younger than 60 minutes, or `create_new` `<id>.request.json` (`id` = 16 hex of the key, `-`, `now_ms`) with `request_key`, `created_at_ms`, `project`, `action`, `params`, `argv` (exactly what will run), `catalogue_hash`, `session_id`.
4. **Wait** for a decision, stat'ing every 250 ms, for `--wait` seconds (default 110, fits the Bash tool's 2 minutes; max 540), checking the stop flag each tick. Approved: consume and run. Rejected: refused. Timeout: refused "approval still pending; run the same command again to keep waiting".

The wait makes journey 3 one click: the call is still open when the author approves, so the action starts within 250 ms instead of the author typing "go". Pending requests leave the board after 60 minutes; approval files older than 24 hours are deleted by the window; the journal keeps history.

*Amendment, 2026-10-28 (story 2.15):* step 2's rename is not a single-use claim on Windows, where two callers that opened `<id>.approved.json` at once both rename it (story 2.8's Debug Log, caught by `approvals_stress.rs`). Consume claims instead: `create_new` on `<id>.consumed.json` (one caller only), the decision written into it, then the approved name removed; a crash in between leaves both names, which reads as used. The first claim wins, a concurrent `run` finds the claim taken: single use without a lock, as step 2 intends.

*Amendment, 2026-10-28 (story 2.16, D-175):* the window's record of its decisions (`relay/decisions.json`, D-173) holds each request's verdict and request key, and `officina run` obeys it: an approval is used only when the record holds it as approved for the same request key (`approval_not_given` otherwise), only the catalogue the record names runs (`catalogue_unconfirmed`), the relay is stopped while the flag is there or the record holds it stopped (AD-34), and a record missing or unreadable once the relay holds a decision refuses everything (`decision_record_unreadable`) until the author acknowledges it in the Relay view, which starts a new record from the state found. Actions start from the environment Windows builds from the registry for the user, never the caller's (AD-30); the tree fingerprint lists submodules' tracked files, hashes the ignore files that are not in the tree, and refuses a folder under a `.git` that `git` cannot read as a work tree.

### AD-34: Stop flag

`relay/stopped` present means stopped; `metadata` failing with anything but `NotFound` also means stopped. Read at step 1 of every `run` and every wait tick. Written and deleted only by `set_relay_stopped`, journaled. It stops the relay, not the gate.

### AD-35: Window

**Board.** `Board` gains `approvals: Vec<ApprovalView>`; `waiting_count` counts them, so title, overlay and Needs you include approvals. `BOARD_SCHEMA` becomes 2. `ApprovalView { id, project, action, description, argv, session_id, session_name, requested_at_ms, expires_at_ms }`, built in core; `session_name` resolved from the cards.

**Relay payload.** Separate `RelayView`, event `relay://updated`, command `get_relay`: `stopped_since_ms`, `approved_hash`, `catalogue`, `pending: Option<{hash, diff}>`, `issues`, `recent` (last 50 entries, runs paired), `journal_corrupt_lines`, `gate_errors_24h`. Separate so the journal tail does not churn the board contract.

**Collector.** The AD-3 thread also stats the floor file (reloaded on change, so board projects follow too), the `relay/` tree (fingerprint like `sessions_fingerprint`) and today's journal. Both payloads are emitted only when their bytes change (AD-5). Authority commands send `CollectorMsg::RelayChanged` for an immediate recompute.

**Commands**, added to `build.rs` and `capabilities/default.json` (AD-19): `get_relay`, `approve_catalogue(hash)`, `decide_approval(id, approve)`, `set_relay_stopped(stopped)`. Thin wrappers over `authority`; `decide_approval` refuses unknown, decided or expired ids and requests whose catalogue hash is not the approved one. No plugin added. New types derive `ts_rs::TS` and join the AD-13 diff check; `ipc.ts` gains four functions, `lib/relay.svelte.ts` holds the relay store.

**Layout.**
- Needs you lists approval cards first, oldest first: amber border, project chip, action and description, argv as one monospace chip per argument (a crafted value cannot pass for two), session name or "session unknown", age, expiry, Approve and Reject. No key approves: a stray `1` must not run a playbook.
- Header: a Board/Relay toggle and a status chip, "relay stopped" (terracotta) or "catalogue change" (amber). The Board view shows a banner for either state; Slice A's banner carries the diff and Approve.
- Relay view: stop toggle; pending change (diff per action, Approve); issues with lines; catalogue table; recent activity (time, project, action, params, outcome, duration). All text rendered as text (`svelte/no-at-html-tags`).

### AD-36: HTTP actions, designed, built last, cuttable

HTTP is 2 to 5 % of the pain (owner) for a TLS stack, a credential binding and a URL builder. Build it as v0.2.x after a week of Slices A and B at work, or drop it. Design:

- `kind = "http"`, GET or HEAD, `url` template parsed in core: scheme `https` (`http` only without a credential) and authority literal; a placeholder fills one whole path segment or query value, percent-encoded outside `A-Za-z0-9-._~`.
- `ureq` (synchronous, AD-3) with rustls and the platform verifier, so the Windows store and an enterprise CA are trusted with no bundled roots; no redirect; timeout `min(timeout_s, 60)`; body read to the cap then dropped; no proxy.
- `[[credential]] name, hosts, header, scheme`; an action names `credential` and its host must be in `hosts`. Secret read with `CredReadW` on `officina:<name>`, blob decoded as UTF-16LE (what `cmdkey /generic` stores), never logged; doctor prints a `cmdkey` line with placeholders.

### AD-37: Test strategy

| Layer | How |
|---|---|
| core | lexer, gate, value rules, argv, two-phase parse, canonical hash (golden), diff, class decisions on synthetic journals; vector sets `gate.json`, `argv.json` with coverage tests iterating the constants |
| fake tool | new test-only bin `officina-fake-tool` (fake-claude pattern): `FAKE_TOOL_LOG` appends argv, cwd and env keys (launch counter); `FAKE_TOOL_MODE` = `exit:<n>`, `bytes:<n>`, `sleep:<s>`, `tree` (spawns a grandchild writing a heartbeat every 100 ms) |
| run | exit passthrough; 124 with the heartbeat stopping; killing `officina run` stops the grandchild; output cap and hashes; 127; 126 per refusal code; every `argv.json` injection refused with zero launches |
| gate | recorded `PreToolUse` payloads (verified field set, redacted) through `officina hook`: stdout and exit 0; v0.1 events still appended; `PreToolUse` never appended |
| flows | catalogue approval via `authority`; delegate (`check` then `apply`); after-check refused without check and after a tree change; approval round trip (run waits, `decide_approval`, runs once, under 2 s; the next call creates a new request); rejection blocks retries; containment; stop during a wait |
| stress, perf | 16 processes x 200 journal lines over 4096 bytes, zero corrupt; two threads consuming one approval, exactly one wins; AD-28 budget; `#[ignore]`, release |
| zero write | `quota_doctor.rs` fingerprint extended to `run`, `run --list`, `relay --json`, a gate deny |
| frontend | Vitest: approval card argv chips, bidi text inert, buttons call `decide_approval`. Playwright: `mockIPC` extended to `get_relay` and the authority commands, fixtures from `officina board --json` and `officina relay --json` |

Manual, recorded in research notes: WSL `ansible-playbook` check and apply on a lab group; the AD-30 and AD-31 gates; `CLAUDE_CODE_SESSION_ID` in Bash children; the `cwd` form of real `PreToolUse` payloads; a deny in an interactive bypass session (E12 was background).

## 2. Dependencies for approval

| Crate | Where | Licence | Why |
|---|---|---|---|
| `regex` | core | MIT/Apache-2.0 | linear-time anchored patterns |
| `sha2` | core | MIT/Apache-2.0 | catalogue hash, request keys, output and tree hashes |
| `windows-sys` features `Win32_System_JobObjects`, `Win32_System_SystemInformation` | host | existing crate | job object, system directory |
| `ureq` (rustls, platform verifier), `windows-sys` `Win32_Security_Credentials` | host, AD-36 only | MIT/Apache-2.0 | HTTP actions, if built |

Not added: `fs2`/`fd-lock` (std `File::lock`), `hex`, `shlex`/`shell-words` (they split, they do not classify), `tokio`, `reqwest`, `url`, `webpki-roots` (licence not in `deny.toml`). No Node dependency.

## 3. PRD amendments

1. **6.1.1.** "Slice A: FR-1 to FR-4, FR-6 to FR-8 (`free`, `after-check`), FR-13, FR-14; the catalogue change is a banner with an Approve button." "FR-5 and FR-12 follow as v0.2.x if a week of use shows the need."
2. **FR-1.** Append: "A schema 1 floor still loads, every project `free`; the new keys require `schema = 2`."
3. **FR-2.** Replace the `program` clause with "`runner` (`native` or `wsl`, optional `distro`), `program` (absolute for that runner), `env` (literals)"; "fills exactly one argument" with "fills part of exactly one argument, one placeholder per argument"; "default 600" with "default 540".
4. **FR-3.** Replace the last sentence with: "Values starting with `-`, containing a control, bidi or invisible formatting character, or having a `.` or `..` path segment are refused. `wsl` values are limited to `[A-Za-z0-9._,:@/+=-]` until WSL argument handling is verified."
5. **FR-4.** Append: "The hash covers the validated catalogue, not the text. A repository is guarded if the approved catalogue or the current file says so."
6. **FR-6.** Replace the bullets with: "In a guarded repository the gate denies a guarded program in command position (also through wrappers, `bash -c`, `eval`, interpreter one-liners), an unparsable command, a dynamic command name, and any mention of `OFFICINA_` or the relay's files. Otherwise no decision: `officina run` enforces the catalogue." Append the limits of AD-27. Default list: `ansible*`, `ssh`, `scp`, `sftp`, `rsync`, `wsl`.
7. **FR-7.** "Exit with the program's code, 126 on refusal, 124 on timeout, 127 when the program cannot start, 64 on usage errors. `officina run --list` prints the project's actions."
8. **FR-8.** human: "runs if a matching approval (action, parameters, project, catalogue) was given under 15 minutes ago and is unused, and consumes it; otherwise creates or reuses a pending request and waits up to 110 s (`--wait`, max 540), running as soon as it is approved. A rejection refuses the same request for 15 minutes. Pending requests expire after 60 minutes." after-check, append: "and, in a git repository, the working tree has not changed since that check."
9. **FR-9, FR-11.** Append: "No command line command approves, rejects, approves a catalogue, stops or resumes." FR-6's third bullet goes.
10. **FR-13.** "Two lines per run (started, finished)"; append "Kept 90 days. Gate denials are journaled."
11. **Section 7, Journal coverage.** "Every run leaves one start line and at most one end line; every refusal and decision exactly one line."
12. **Section 8, question 2.** "Answered by `runner = "wsl"`, `env` and absolute `program`; WSL argument handling is an architecture gate (AD-30)."

## 4. Risks

| # | Risk | Mitigation |
|---|---|---|
| R14 | WSL argv handling and kill behaviour unverified | AD-30 and AD-31 gates; doctor checks `/usr/bin/timeout` and `/usr/bin/env` in the distro |
| R15 | gate false positives stall guarded sessions | reason lists the actions; vector set; `guarded_programs` per floor |
| R16 | ansible ignores `ansible.cfg` in a world-writable `/mnt/c` folder | `env = { ANSIBLE_CONFIG = ... }` |
