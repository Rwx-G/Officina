---
title: "PRD: Officina v0.1 - The Board"
author: Rwx-G
date: 2026-10-27
status: approved (revised after review-challenger.md and architecture-spine.md section 5)
inputs:
  - (framing, not published)
---

# PRD: Officina v0.1 - The Board

## 0. Document Purpose

This PRD scopes Officina v0.1, the first minor version of the roadmap set by the framing document (2026-10-27). It is the contract for Epic 1. Later versions (v0.2 relay, v0.3 registry and triage, and so on) get their own PRD; this one names them only to keep v0.1 from blocking them.

The framing's architecture principles are inputs, not open questions. Where step 0 produced a fact that changes one of them, section 3 says so and cites `docs/research-notes.md`.

## 1. Vision

A developer runs five to ten Claude Code sessions in parallel, across repositories, in editor terminals. v0.1 gives one always-open window that answers three questions in under two seconds, without touching any session: **which sessions need me, which are working, which are finished**. One click brings the right session to the foreground.

v0.1 is read-only. It launches nothing, writes nothing into Claude Code's configuration, and keeps working (or degrades to "unknown") whatever Claude Code does.

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Triage at a glance.** When I come back from a meeting or finish a review, I want to see which sessions are blocked on me, so I unblock them before anything else.
- **J2 - Find the terminal.** When I know which subject I want to continue, I want one click to land me in that session, so I stop cycling through tabs.
- **J3 - Know my margin.** When I decide whether to start a new subject, I want the 5-hour and weekly quota with their reset times and their freshness, so I do not start something I cannot finish.
- **J4 - Trust the board.** When the board says "finished", I want that to be true, so I can stop checking. When it does not know, it must say "unknown".

### 2.2 Non-Users (v0.1)

Teams, shared dashboards, remote observers. Officina is single-user, single-workstation. There is no network surface.

### 2.3 Key User Journeys

1. **Morning.** The author opens Officina. Sessions left from yesterday appear: background ones with their final state, interactive ones still running. Two are amber ("needs you"): one waits on a permission prompt, one asks a question. The header pill reads "2 waiting".
2. **Unblock.** The author clicks the first amber card. The editor window holding that terminal comes to the foreground; for a background session, a terminal tab opens attached to it. The card turns blue ("working") within seconds of the answer.
3. **Margin check.** Before starting a new subject the author reads the gauges: "5 h: 62 %, resets at 14:20", "Week: 41 %, 57 % of the week elapsed", "updated 3 min ago".
4. **Setup on a new workstation.** The author runs `officina doctor`. It reports what is green, what is missing, and prints the exact hooks block and statusline snippet to paste. Nothing is written for them.

## 3. Glossary

- **Session** - one Claude Code process as listed by `claude agents --json` (interactive or background), identified by `sessionId`.
- **Floor** - one workspace: a set of projects described by one `officina.toml`, optionally with its own `CLAUDE_CONFIG_DIR`. v0.1 shows one floor at a time.
- **Project** - a directory sessions run in, mapped by `officina.toml` or derived from the session's `cwd`.
- **Column** - the board's four groups: *Needs you* (a decision is blocking), *Your turn* (an interactive session ended its turn), *Working*, *To resume / Done*. An *unknown* badge never counts as done.
- **Card key** - A live interactive session is keyed on `pid` and `startedAt`, a background session on `id`, an ended interactive session on `sessionId`. Hook events attach by `session_id`; the latest `claude agents` snapshot maps each live pid to its current `sessionId` (verified before the classification story; if it does not hold, the hook records the `claude` process id and `SessionStart` re-points).
- **Event buffer** - append-only JSON Lines file under `~/.officina/events/` written by `officina hook`.
- **Quota file** - `~/.officina/quota/<account key>.json`, one per Claude config dir (rate limits are per account), written by any statusline running on that account through `officina quota write`. Last writer wins. Schema versioned.
- **Contract** - the set of fields Officina reads from Claude Code's documented surfaces. The canary checks it.

## 4. Features

Facts from step 0 (recorded in `docs/research-notes.md`) that shape these requirements:

- `claude agents --json [--all]` returns, per session: `id` (background), `cwd`, `kind` (`interactive`|`background`), `startedAt` (ms), `sessionId`, `name`, `pid` and `status` (`busy`|`waiting`|`idle`) while the process lives, `state` (`working`|`blocked`|`done`|`failed`|`stopped`) and `waitingFor` when waiting. Verified on 2.1.311: `busy`, `idle`, `waiting`, `blocked`, `done`, `failed`, and `waitingFor` = `permission prompt` and `input needed` (E1). An interactive session that ends its turn, even with a question, reads `idle`.
- Hook payloads verified on 2.1.311: `SessionStart` (`source`), `UserPromptSubmit` (`prompt`, `prompt_id`), `Stop` (`last_assistant_message`), `SessionEnd` (`reason`); `Notification` carries `notification_type` and `message` (documented).
- Statusline JSON carries `rate_limits.five_hour` and `rate_limits.seven_day` with `used_percentage` and `resets_at` (epoch seconds), only on subscription accounts and after the first API response.

### 4.1 Collection

#### FR-1: Session list from `claude agents`
Officina runs `claude agents --json --all` and parses every documented field. When the floor declares a `config_dir`, the call runs with `CLAUDE_CONFIG_DIR` set to it. Unknown fields are ignored; a missing field is "unknown" for that session, never a default. The `claude` binary is resolved from `PATH` under its Windows names (`claude.exe`, then `claude.cmd`), overridable by `OFFICINA_CLAUDE_BIN`. A failed or timed-out call (5 s) keeps the previous snapshot and marks the board stale with the error, it never empties the board.

#### FR-2: Event-driven refresh
Officina refreshes when the event buffer grows, when a quota file changes, or when the modification time of the Claude sessions directory (`<config dir>/sessions`) changes; it never reads that directory's files. A safety poll runs every 15 s. Nothing re-renders when the computed board is unchanged.

#### FR-3: Hook command
`officina hook` reads one hook payload on stdin, appends one JSON line to the event buffer, and exits 0 in every case (malformed input, full disk, missing directory included). It keeps only: event name, `session_id`, `cwd`, timestamp, `source`, `reason`, `notification_type`, `message`, and the first 280 characters of `prompt` and `last_assistant_message`. Excerpts can be disabled with `OFFICINA_NO_EXCERPTS=1`. It opens no socket and spawns no process. It is a console-subsystem binary with no GUI dependency, and its latency is measured as Claude Code pays it: from a real hook invocation. Lines are appended with a single write so concurrent hooks never interleave.

#### FR-4: Event buffer retention
One file per UTC day; readers read the current and previous day's files; files older than 24 h are deleted by the application, never by the hook. No real hook payload is ever committed: test vectors are written by hand. A reader tolerates a truncated last line.

#### FR-5: Quota contract
`officina quota write` reads a statusline JSON payload on stdin and writes `~/.officina/quota/<account key>.json` atomically (temp file plus rename), the account key being derived from `CLAUDE_CONFIG_DIR` or the default config dir: `schema`, `account_key`, `session_id`, `written_at`, `five_hour` and `seven_day` (`used_percentage`, `resets_at`), each window absent when the payload lacks it. It prints nothing on stdout, so it sits inside an existing statusline without changing its output. One statusline per account is enough; doctor recommends wiring it in a personal, uncommitted settings file (`.claude/settings.local.json` or user settings), never in a repository's committed statusline. `--tee` copies stdin to stdout unchanged so the command can sit in front of an existing statusline. Data older than 30 min is shown as stale; a window whose `resets_at` has passed shows "reset, no new data".

### 4.2 Board

#### FR-6: Classification
A live `status` is authoritative; hook events never override it. Each session lands in exactly one column, by this precedence:
1. *Needs you*: `status == waiting` or `state == blocked`, with `waitingFor` shown. Only when `status` is absent does a `Notification` of type `permission_prompt` or `elicitation_dialog` count, and only if newer than the session's last `UserPromptSubmit` and last `Stop`.
2. *Working*: `status == busy` or `state == working`.
3. *Your turn*: `kind == interactive` and `status == idle`. Amber outline, no fill: it waits on the author but nothing is blocked.
4. *To resume / Done*: background `state == done` goes to Done; `failed`, `stopped`, and interactive sessions whose process is gone go to To resume with an "interrupted" or "ended" badge. Only the last 24 h are listed. Interactive sessions whose process is gone are reconstructed from the event buffer: `SessionEnd` with `prompt_input_exit` or `logout` gives 'ended', no `SessionEnd` gives 'interrupted', `clear` and `resume` are superseded and `other` (non-interactive runs) is not listed.
5. Otherwise *Unknown*: shown in *Working* with an "unknown" badge, never in Done.

The classification is a pure function with a vector set of cases (`tests/vectors/classification.json`) replayed in Rust and in the frontend if the frontend ever re-implements it.

#### FR-7: Card content
Name (session name, else project name), project chip with the project colour, kind, `waitingFor` when present, last prompt and last assistant message excerpts (from the event buffer, when known), time since the last event, time since start. Text from events and transcripts is rendered as plain text, never as HTML.

#### FR-8: Header
"N waiting" pill (amber, only when N > 0), the two quota gauges with value, reset time, explanation and freshness, a stale banner when the session list could not be refreshed, and the contract status ("contract held" or the missing fields).

#### FR-9: Floor configuration
An optional `officina.toml` declares projects: `name`, `path` (relative to the file), `color` (one of the palette tokens), and an optional `config_dir`. Officina finds it through `--floor <path>` or `OFFICINA_FLOOR`, then `~/.officina/floor.toml`. Without it, projects are derived from each session's `cwd` (last path segment). Sessions outside every declared project go to an "other" project; they are never hidden.

### 4.3 Focus

#### FR-10: Bring a session to the foreground
Clicking a card:
- background session: spawns `claude attach <id>` directly as a new console process (`CREATE_NEW_CONSOLE`) in the session's `cwd`, with the floor's `CLAUDE_CONFIG_DIR`; on Windows 11 the default terminal setting decides where it opens. The card also offers to copy the attach command.
- interactive session: walks the parent chain of `pid` up to the first process that owns visible top-level windows (editor or terminal). Among them it tries the folder name of the session's `cwd`, then each parent folder name up to the user home, and picks the first name that matches exactly one window title as a whole word; with exactly one window it takes it. With no match or several matches it lists the candidates on the card instead of guessing. Selecting the terminal tab inside an editor is out of scope.

The attach process receives an argument vector built from the session id (validated against `^[A-Za-z0-9_-]{1,64}$`) and nothing else; `cwd` is passed as the working directory, never on a command line; no text from prompts or messages ever reaches a command line. The outcome (window focused, attach opened, candidates listed, failed with reason) is shown on the card. The card offers "wrong window"; the focus metric counts first clicks not followed by "wrong window" or a second click on the same card within 10 s.

### 4.4 Setup and safety

#### FR-11: Doctor
`officina doctor` checks and prints, without writing anything under the Claude config dir: `claude` found and its version; `claude agents --json` parses and carries the contract fields; the Officina hooks are present in user settings (read-only comparison) or else the exact block to paste; at least one quota file fresher than 30 min or else the statusline snippet to add; `~/.officina` writable; floor file valid. Exit code 0 when everything required is green, 1 otherwise.

#### FR-12: Contract canary
At startup and whenever the resolved `claude` binary changes, Officina checks the fields it depends on against a live `claude agents --json` sample and the last hook payloads, and stores the result per version. The header shows "contract held" or the missing fields.

#### FR-13: No writes under the Claude config dir
Officina never creates, modifies or deletes a file under the Claude config dir. An end-to-end test fingerprints that tree (paths, sizes, contents) before and after a full run.

#### FR-14: Headless board
`officina board --json` prints the computed board once and exits. It is the scripting surface and the backbone of the end-to-end tests.

## 5. Non-Goals (Explicit)

- Launching, stopping or answering sessions (v0.5 onward). v0.1 only opens an attach for background sessions, which is a terminal action of the author.
- Reading transcripts. Excerpts come from hook payloads only.
- The isometric scene, rooms and focus mode (v0.4). v0.1 is a board with the final palette and type.
- Registry, subjects, triage verdicts (v0.3).
- Any network listener, telemetry or update check.
- Multiple floors displayed at once.

## 6. MVP Scope

### 6.1 In Scope
FR-1 to FR-14; Windows 10/11 x64; two binaries (`officina.exe` console CLI with no GUI dependency, `officina-app.exe` board window); a portable build and an installer built by CI; README with logo, screenshots and an animated demo.

### 6.1.1 Delivery order
The owner wants v0.1 usable the next working day, and the complete v0.1. Stories are ordered so the first slice is usable alone:
- **Slice A (usable tomorrow):** FR-1, FR-3, FR-6, FR-7, FR-14, FR-10, the board window, and doctor printing the hooks block. Projects from `cwd` only.
- **Slice B (same epic, after A):** FR-2 event-driven refresh, FR-5 quota, FR-8 gauges, FR-9 floor file, FR-11 full doctor, FR-12 canary, FR-13 fingerprint test, installer, README assets.
The 10-day exit criteria apply to the full v0.1.

### 6.2 Out of Scope for MVP
Linux and macOS builds (code stays portable; `cargo test` runs on Linux in CI from day one); dark theme (v0.4); desktop notifications (candidate for v0.1.x once the board is trusted).

## 7. Success Metrics

| Metric | Target | How measured |
|---|---|---|
| Hook latency | p95 < 30 ms | CI: 200 spawns of the release binary with a 200 KB payload, p95 < 30 ms. Workstation: added latency per event through a real `claude -p --settings` run, recorded in research notes |
| Writes under Claude config dir | 0 | e2e fingerprint test, and doctor on the real workstation |
| Quota freshness | always displayed | UI test: gauges always carry an age or "no data" |
| Focus success | > 90 % without a second click | click outcome counters over 10 working days |
| Idle CPU | < 1 % | measured on the workstation with 8 sessions |
| Daily use | 10 working days | author's log |

## 8. Open Questions

- Does a statusline run for `--bg` sessions? If not, quota freshness depends on one interactive session being displayed. Not blocking: freshness is shown either way, and one interactive session per account is enough now that the file is per account.
- Which of the user or project statusline wins when both exist? Documented precedence suggests project; to verify on the workstation (step 0 item 2).
- Focus inside an editor with several terminals in one window lands on the window, not the tab. Measured by FR-10's counters before any extension work.

## 9. Assumptions Index

- A1: `claude agents --json` stays a documented scripting surface ("for scripting; does not require a TTY").
- A2: hooks from user settings and project settings merge (documented) and `officina hook` added to user settings runs alongside each project's own hooks.
- A3: one personal statusline per account can call one extra command without breaking its output.
- A4: VS Code window titles contain the workspace folder name (default `window.title`). If the author changed it, FR-10 lists candidates instead of focusing.
