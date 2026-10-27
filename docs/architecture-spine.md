---
title: "Architecture spine: Officina v0.1 - The Board"
author: Rwx-G
date: 2026-10-27
status: approved 2026-10-27 (owner delegated design decisions; dependency list of section 4 accepted)
inputs:
  - (framing, not published)
  - prd/prd-v0.1.md
  - (requirement challenge, not published)
  - research-notes.md
---

# Architecture spine: Officina v0.1

Stories cite decisions as `AD-n`. A decision changes only through an edit of this file with the new fact that motivates it. Facts tagged E1-E8 come from `docs/research-notes.md`; anything marked "unverified" is a gate, not a premise.

## 1. Shape in one paragraph

Three Rust crates and one Svelte frontend. `officina-core` is pure: parsing, classification, board, formats, matching, argument building, no IO. `officina-host` does every IO and every platform call, and owns the collector loop. `officina` (console exe) and `officina-app` (GUI exe) are thin shells over `officina-host`. The GUI links the host crate and never shells out to the CLI. The board is computed once, in Rust, and the same serialized `Board` feeds `officina board --json`, the Tauri event, the Tauri command and the Playwright fixtures.

## 2. Decisions

### AD-1: Crate layout and boundaries

| Crate | Kind | Owns | May depend on |
|---|---|---|---|
| `crates/officina-core` | lib | types and pure functions: agent entry parsing, event line schema, quota schema, floor schema, account key, classification, board build, contract check, title matching, attach argv building, UTC date math | serde, serde_json, toml, thiserror, ts-rs (optional feature `ts`) |
| `crates/officina-host` (new) | lib | paths, `claude` resolution and invocation, event buffer append/read/retention, atomic write, quota read/write, state store, floor loading, collector loop, doctor checks, `platform::windows` (process tree, window enumeration, focus, console spawn) | officina-core, thiserror, windows-sys (cfg windows) |
| `crates/officina` | bin `officina.exe` (console subsystem) | argument parsing, subcommands `hook`, `quota write`, `doctor`, `board --json`; second bin `officina-fake-claude` for tests only | officina-host, anyhow |
| `app/src-tauri` | bin `officina-app.exe` (windows subsystem) | Tauri wiring: commands, events, window, single instance, taskbar overlay | officina-host, tauri, tauri-plugin-single-instance |

Purity of `officina-core` is enforced, not hoped for: `crates/officina-core/clippy.toml` lists `disallowed-methods` for `std::fs::*`, `std::process::Command::new`, `std::time::SystemTime::now`, `std::env::var*`; CI runs clippy with `-D warnings`. Time enters core as a `now_ms: i64` argument. Trade-off: one more crate in the workspace; in exchange every rule that decides what the author sees runs on Linux CI without mocks.

`unsafe_code` stays `deny` at workspace level. The only exception is `officina-host/src/platform/windows/` with a module-level `#![allow(unsafe_code)]` and a `// SAFETY:` comment on each block. Non-Windows builds compile `platform::other`, which returns `FocusOutcome::Unsupported`.

### AD-2: The GUI reuses the host crate, it does not shell out

`officina-app` calls `officina_host::collector` in-process. Trade-off: the collector code ships twice (in both exes, a few hundred KiB); shelling out would add a process spawn per refresh, a second JSON boundary and a console-window flash risk for nothing.

### AD-3: Threading model

- One dedicated `std::thread` named `collector` owns all collector state (last agents snapshot, event offsets, quota, contract record, last emitted board bytes). No tokio in Officina code.
- It loops on `mpsc::Receiver<CollectorMsg>::recv_timeout(500 ms)`. Messages: `Refresh` (manual), `Visibility(bool)`, `Shutdown`.
- `claude agents` runs synchronously inside that thread, with a 5 s deadline enforced by a `try_wait` loop at 50 ms and `kill` on expiry. While it runs, other triggers only set dirty flags. At most one `claude` child at a time.
- The latest board lives in `Arc<RwLock<Option<Board>>>` shared with the Tauri commands.
- Tauri commands that block (`focus_card`) are `async fn` and wrap the call in `tauri::async_runtime::spawn_blocking`; nothing runs on the main thread.

Trade-off: a slow `claude agents` delays quota updates by up to 5 s; acceptable, and it removes every locking question.

### AD-4: Collector triggers and refresh plan

Every 500 ms the collector stats a fixed set of paths (4 `metadata` calls, no file watcher crate):

| Watched | Signal | Action |
|---|---|---|
| today's event file | length grew | read new lines incrementally; recompute; schedule agents refresh (debounce 250 ms) and a follow-up refresh at +1.5 s |
| `<config dir>/sessions` | mtime changed | schedule agents refresh; the directory is never listed or read (FR-2) |
| `<officina home>/quota/<account key>.json` | mtime changed | re-read quota; recompute; no spawn |
| resolved `claude` binary | mtime changed, or startup | run `claude --version`; on a new version run the canary (AD-12) |
| safety timer | 15 s while the window is visible, 60 s while minimized or hidden | agents refresh |

Spawns are rate-limited to one per second. The +1.5 s follow-up exists because a `Stop` hook can fire before `claude agents` reports `idle` (race not measured; the follow-up costs one spawn per event and closes it).

Trade-off of stat polling over `notify`: up to 500 ms extra latency, and no dependency, no ReadDirectoryChangesW edge cases on synced or redirected folders.

### AD-5: Compute, diff, emit

After each recompute: `board = core::build_board(&input, now_ms)`; serialize with `serde_json::to_vec`; compare with the last emitted bytes; emit only when different. The GUI emits Tauri event `board://updated` with the `Board` payload; command `get_board` returns the cached board for the first paint. `Board` carries absolute timestamps only; relative times ("3 min ago") are rendered by the frontend on a 30 s tick, so the backend never re-emits for clock motion. Thresholds the frontend needs (`quota.stale_after_ms`, `done_window_ms`) travel inside the board, so no constant is duplicated in TypeScript.

### AD-6: Resolving and running `claude`

Resolution order: `OFFICINA_CLAUDE_BIN`; `PATH` search for `claude.exe` then `claude.cmd`; `%USERPROFILE%\.local\bin\claude.exe` (E8, covers a GUI launched from the Start menu with a stale PATH). Every spawn from either exe sets `CREATE_NO_WINDOW` (`std::os::windows::process::CommandExt::creation_flags`, no unsafe), otherwise the GUI flashes a console every 15 s. `CLAUDE_CONFIG_DIR` is set on the child when the floor declares `config_dir`. stdout capped at 8 MiB, stderr kept (first 500 chars) for the stale banner.

### AD-7: Paths and overrides

| Item | Default | Override |
|---|---|---|
| Officina home | `%USERPROFILE%\.officina` (`$HOME` elsewhere) | `OFFICINA_HOME` |
| Claude config dir | `CLAUDE_CONFIG_DIR`, else `<home>\.claude` | floor `config_dir` |
| Floor file | `--floor <path>`, `OFFICINA_FLOOR`, `<officina home>\floor.toml` | |
| `claude` | AD-6 | `OFFICINA_CLAUDE_BIN` |

No `dirs` crate: two environment variables cover Windows and Linux. All four overrides exist so tests never touch the real home.

### AD-8: File formats (all versioned, all under the Officina home)

Each schema is a Rust type in `officina-core` with `#[serde(deny_unknown_fields)]` where Officina is the only writer (floor file, state) and permissive parsing where another program writes (agents output, hook payloads). The version field is checked; an unknown major version is "unknown", never a crash.

**Event buffer line** - `events/YYYY-MM-DD.jsonl`, UTC date (AD-9), one JSON object per line:

```json
{"v":1,"ts":1761555600123,"event":"Notification","session_id":"<UUID>","cwd":"C:/path/to/repo","notification_type":"permission_prompt","message":"..."}
```

Fields: `v`, `ts` (ms, taken by the hook on receipt), `event`, `session_id`, `cwd`, and when present `source`, `reason`, `notification_type`, `message`, `prompt`, `last_message`. `prompt`, `last_message` and `message` are cut to 280 chars on a char boundary; `OFFICINA_NO_EXCERPTS=1` drops `prompt` and `last_message`. If the serialized line exceeds 4096 bytes, excerpts are dropped and `"truncated":true` is set. Absent fields are omitted, never `null`.

**Quota file** - `quota/<account key>.json`:

```json
{"schema":1,"account_key":"default","session_id":"<UUID>","written_at":1761555600,"five_hour":{"used_percentage":62.0,"resets_at":1761570000},"seven_day":{"used_percentage":41.0,"resets_at":1761900000}}
```

Seconds, like the statusline payload. Account key: `default` when no config dir override applies, else `cfg-` plus the FNV-1a 64 hex of the normalized absolute path (lowercase, forward slashes, no trailing slash). FNV is ten lines in core and needs no crate; it is an identifier, not a security boundary. The public contract is the command `officina quota write`, not the hash, so no other program computes it.

**Floor file** - `officina.toml`:

```toml
schema = 1
name = "work"
config_dir = "C:/path/to/claude-config"   # optional, absolute

[[project]]
name = "backend"
path = "Backend"          # relative to this file
color = "green"           # a ProjectColor token
```

`config_dir` is a floor key, not a project key (one floor, one account). `ProjectColor` is an enum in core (`green`, `blue`, `terracotta` today); doctor reports unknown keys and unknown tokens with line numbers from the toml error.

**State** - `state/contract.json` (`{"schema":1,"versions":{"2.1.311":{"checked_at":...,"held":true,"missing":[],"not_observed":["waitingFor"]}}}`), written atomically. `state/focus.jsonl`, append-only: `{"v":1,"ts":...,"card":"<fnv of card key>","kind":"interactive","outcome":"focused"}` and `{"v":1,"ts":...,"card":"...","outcome":"wrong_window"}`. No path or text in state files. Focus lines older than 30 days are dropped by the retention pass.

### AD-9: Event files per UTC day, retention by the app

FR-4 says "local day". Local dates need a timezone crate or Win32 calls in core; UTC dates need fifteen lines of civil-date arithmetic. Readers read today's and yesterday's files, so the 24 h window is exact either way. The retention pass (app startup and hourly) deletes event files dated before yesterday (UTC). The hook never deletes. Trade-off: a file boundary at 01:00 or 02:00 local, invisible to the user.

### AD-10: Appends and atomic writes on Windows

**Hook append.** `OpenOptions::new().append(true).create(true)`; on Windows Rust opens with `FILE_APPEND_DATA` without `FILE_WRITE_DATA`, so every write lands at the current end of file whatever other writers do. The line (including `\n`) is built in memory and written with one `write_all` call on a line capped at 4096 bytes. On a local NTFS volume a single `WriteFile` with append access is, as far as I know, not interleaved with another writer's, but Microsoft does not document it as a guarantee. Therefore: (1) a stress test (AD-17) runs 16 processes x 500 appends on the CI runner and asserts zero corrupt lines; (2) the reader skips any unparsable line, counts it, and surfaces the count in `board.diagnostics`, so corruption is visible, never silent. No file lock: `LockFileEx` would put a wait inside every hook. Trade-off: correctness rests on a measured behaviour plus a loud fallback, not on a documented guarantee.

On `NotFound` the hook creates `events/` and retries once. Any other error: exit 0, nothing printed.

**Atomic write** (quota, contract): write `<name>.<pid>.tmp` in the same directory, `sync_all`, then `std::fs::rename`, which replaces the target on Windows (`MoveFileExW` with replace semantics). Rust opens files with `FILE_SHARE_DELETE`, so Officina's own readers never block the rename; a third-party handle (antivirus, indexer) can, so the rename retries three times at 10 ms, then gives up and deletes the temp file. `quota write` skips the write when windows are identical and the file is younger than 60 s, to spare the disk on every statusline tick.

### AD-11: Classification and card keys

`core::classify(entry: Option<&AgentEntry>, events: &SessionEvents, now_ms) -> Placement { column, badge, waiting_for }` is pure. The precedence is FR-6, verbatim in code order. Additions this spine fixes:

- **Card key.** Live interactive: `pid:<pid>:<startedAt>` (pid reuse within 24 h is real on Windows; `startedAt` disambiguates). Background: `bg:<id>`. Ended interactive: `sid:<sessionId>`.
- **Events attach by `session_id`.** The current `sessionId` of a live card is taken from the latest agents snapshot, which is authoritative for pid -> sessionId. Excerpts come from the events of that sessionId. **Gate, unverified:** before the classification story, run `/clear` in an interactive session and record in research notes whether `claude agents --json` shows the new `sessionId` for the same pid. If it does not, the hook line gains an optional `claude_pid` (first ancestor named `claude.exe`, via the toolhelp snapshot) and re-pointing uses `SessionStart` with `source` in `clear|resume|fork` matching `claude_pid` and `cwd`. The field is reserved in schema v1 so either outcome is non-breaking.
- **Ended interactive sessions** are not in `claude agents` output, so they are reconstructed from the event buffer: a `session_id` absent from the live list and from the background list, whose events in the last 24 h include a `UserPromptSubmit`. Last event `SessionEnd` with `reason` in `clear|resume` -> superseded, not shown. `SessionEnd` with `prompt_input_exit|logout` -> To resume, badge `ended`. `SessionEnd` with `other` -> not shown (this is what `claude -p` scripts produce, E2). No `SessionEnd` -> To resume, badge `interrupted`. This keeps `board --json` stateless: no extra file, same answer after a restart.
- **Done window.** Background `done` is listed when its last event, else `startedAt`, is within 24 h.
- **Ordering.** Needs you: oldest wait first. Your turn: oldest first. Working: most recent start first. To resume and Done: most recent first.

### AD-12: Contract canary

`core::contract` holds one constant per surface: fields always present on an agents entry (`sessionId`, `cwd`, `kind`, `startedAt`), fields present conditionally (`pid`, `status` when live; `id`, `state` when background; `waitingFor` when waiting), and hook fields per event. The check returns `missing` (an always-present field absent from any entry) and `not_observed` (a conditional field never seen yet). `held = missing.is_empty()`. Triggered on startup and when the `claude` binary mtime changes (AD-4), stored per version (AD-8). Doctor prints the same result. Documentation names the constant; it never lists the fields.

### AD-13: Board JSON contract and TypeScript types

The `Board` type in `officina-core` is the contract. TypeScript types are generated with `ts-rs` behind the optional `ts` feature into `app/src/lib/generated/`, committed, excluded from Prettier and ESLint formatting rules, and CI regenerates them and fails on `git diff --exit-code`. Trade-off: one optional dependency; hand-written types would be a transcription with no guard.

Top level (field list owned by the Rust type; this outline is orientation only): `schema`, `generated_at_ms`, `floor`, `stale` (`since_ms`, `error`) or absent, `waiting_count`, `columns` (`needs_you`, `your_turn`, `working`, `to_resume`, `done`, each an array of `Card`), `quota` (windows, `written_at_ms`, `stale_after_ms`), `contract`, `diagnostics`. A `Card` carries `key`, ids, `kind`, `name`, `project { name, color }`, `badge`, `waiting_for`, `last_prompt`, `last_message`, `last_event_at_ms`, `started_at_ms`, and `attach_available`.

The frontend never re-implements classification, so the vector set (AD-14) is replayed in Rust only. Project colours: TypeScript maps `Record<ProjectColor, string>` to CSS variables, so a new Rust variant breaks `pnpm check`.

`officina board --json` prints the board and exits 0 when fresh, 3 when stale (board still printed), 2 on a usage error.

### AD-14: Classification vector set

`tests/vectors/classification.json`, hand-written, redacted, exempt from every formatter:

```json
{
  "schema": 1,
  "cases": [
    {
      "name": "interactive waiting on a permission prompt",
      "now_ms": 1761555600000,
      "agents": [{"pid": 4100, "cwd": "C:/work/repo-a", "kind": "interactive", "startedAt": 1761550000000, "sessionId": "00000000-0000-0000-0000-000000000001", "name": "repo-a", "status": "waiting", "waitingFor": "permission prompt"}],
      "events": [],
      "expect": [{"key": "pid:4100:1761550000000", "column": "needs_you", "badge": null, "waiting_for": "permission prompt"}]
    }
  ]
}
```

`agents` holds raw `claude agents` objects (the parser is exercised too); `events` holds raw event lines. A Rust test replays every case through `build_board`. A second test refuses a stale set: it iterates the `Column`, `Badge`, documented `status`, `state` and `SessionEnd.reason` enums and fails when a value appears in no case. Agent samples are built from the verified shapes of E1, never pasted from a real workstation.

### AD-15: Focus on Windows

All in `officina-host/src/platform/windows/`, crate `windows-sys`, features `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`, `Win32_System_Diagnostics_ToolHelp`, `Win32_System_Threading`. Unsafe surface: one `CreateToolhelp32Snapshot` walk, one `EnumWindows` callback, `GetWindowTextW`, `GetWindowThreadProcessId`, `IsWindowVisible`, `GetWindow(GW_OWNER)`, `IsIconic`, `ShowWindow`, `SetForegroundWindow`, `FlashWindowEx`. Every pure part (candidate names, title match, ranking) lives in core and is unit tested.

**Interactive session.**
1. Snapshot processes; walk `pid` -> parent, stopping at 16 hops or a cycle; keep ancestors whose creation time precedes the child's (pid-reuse guard via `GetProcessTimes`).
2. Enumerate top-level windows: visible, unowned, non-empty title. Keep those owned by an ancestor. Take the nearest ancestor that owns at least one (E8: the `Code.exe` main process).
3. Candidate names from `cwd`: last segment, then each parent segment up to but excluding the user home and drive root. For each name in order, match titles case-insensitively where the name is bounded by start, end, or a non-alphanumeric char. The first name with exactly one matching window wins. This covers a session started in a subfolder of the opened workspace, which a last-segment-only rule misses. A single window owned by the ancestor wins outright.
4. Focus: `ShowWindow(SW_RESTORE)` if iconic, then `SetForegroundWindow`. The click comes from Officina's own foreground window, which satisfies the foreground-lock rule; `AllowSetForegroundWindow` and the `AttachThreadInput` trick are not used. If Windows refuses, `FlashWindowEx` on the target and outcome `refused`.
5. Outcomes: `focused`, `candidates(list of titles)`, `no_window`, `refused`, `attach_opened`, `failed(reason)`.

**Background session.** See PRD amendment 1: spawn `claude.exe attach <id>` directly with `CREATE_NEW_CONSOLE`, `current_dir(cwd)` and the floor's `CLAUDE_CONFIG_DIR`. `id` must match `^[A-Za-z0-9_-]{1,64}$`; `cwd` must be an existing absolute directory; otherwise `failed`. The child is not waited on and not tracked: Officina owns no session process, and the attach client is the author's terminal. On Windows 11 with Windows Terminal as default terminal, the console lands in Windows Terminal through the default-terminal handoff. The card also offers "copy attach command" (`navigator.clipboard`, no plugin) for the cases where the author prefers an editor terminal.

**Metric.** Each click appends an outcome line (AD-8); "wrong window" appends a line. Doctor prints the rate over 10 working days: first clicks with outcome `focused`/`attach_opened`, not followed within 10 s by "wrong window" or a second click on the same card.

**Manual verification** (no automation can judge "right window"): a checklist in the story, run before closing it: two VS Code windows on two repos, one session each, plus one session in a subfolder, plus one minimized window, plus one window on another virtual desktop, plus Windows Terminal standalone. Each click result is recorded in research notes.

### AD-16: Hook and CLI binary

Hand-parsed arguments (no `clap`): five subcommands do not justify a crate in the hook's image. `officina hook` reads stdin to end, parses a struct with only the kept fields, appends, exits 0 on every path including panic (`std::panic::catch_unwind` around the body, `panic = "abort"` not set for this binary). No allocation of a runtime, no thread, no socket, no child. `officina quota write [--tee]`: `--tee` copies stdin to stdout unchanged, so the existing statusline becomes `officina.exe quota write --tee | <existing command>`, a one-line edit in `settings.local.json`. Without `--tee` it prints nothing (FR-5). Doctor prints the hooks block with the absolute path of `current_exe()`, forward slashes, quoted, five events, `timeout: 5`, synchronous (ordering of lines matters more than the few milliseconds `async` would hide, and `async` on Windows is unverified).

### AD-17: Test strategy

| Layer | What | How |
|---|---|---|
| core unit | classification, board build, parsers, account key, title match, attach argv validation, UTC dates, truncation on char boundary | `#[cfg(test)]` in each module; vector replay (AD-14); runs on Linux and Windows |
| host integration | event append/read with truncated last line, retention, atomic write retry, quota write skip rule, floor loading, collector `snapshot_once` | `tempfile` dirs, `OFFICINA_HOME` and `CLAUDE_CONFIG_DIR` set to temp dirs |
| CLI integration | every subcommand end to end | `crates/officina/tests/`, `env!("CARGO_BIN_EXE_officina")`, `OFFICINA_CLAUDE_BIN = CARGO_BIN_EXE_officina-fake-claude`; HOME, USERPROFILE, OFFICINA_HOME, CLAUDE_CONFIG_DIR all pointed at temp dirs |
| fake claude | `src/bin/officina-fake-claude.rs` in the `officina` crate | prints the file named by `FAKE_CLAUDE_FIXTURE` for `agents --json`; `FAKE_CLAUDE_MODE` = `ok`, `garbage`, `exit1`, `hang`; answers `--version` from `FAKE_CLAUDE_VERSION`; appends argv and `CLAUDE_CONFIG_DIR` to `FAKE_CLAUDE_LOG` so tests assert the env reached the child |
| hook latency | 200 sequential spawns of the release `officina.exe hook` with a 200 KB payload, p95 < 30 ms; plus the same through `bash -c` when Git Bash is present, reported, not gated | `#[ignore]` test run by CI with `--release -- --ignored` |
| append stress | 16 processes x 500 hook invocations, then parse every line | same harness |
| zero write | fingerprint (relative path, size, full content bytes; no hash crate needed at test scale) of a temp `CLAUDE_CONFIG_DIR` seeded with a settings file and a sessions dir, before and after `doctor`, `board --json`, a collector run of 5 s, `hook`, `quota write` | CLI integration test |
| frontend unit | `Card`, `Column`, `Header`, `Gauge`, relative time, no-HTML rendering of excerpts (inject `<img onerror>` text, assert it renders as text) | Vitest + @testing-library/svelte + jsdom |
| e2e | board layout, keyboard navigation, stale banner, quota "no data" and "reset, no new data", focus outcome display | Playwright on `vite build --mode e2e` served by `vite preview`; `src/e2e-mock.ts` (imported only when `import.meta.env.MODE === 'e2e'`) installs `mockIPC` from `@tauri-apps/api/mocks` and serves boards injected by `page.addInitScript`; `window.__officinaPushBoard` simulates the event |
| e2e fixtures | real `officina board --json` output | `pnpm e2e:fixtures` runs the CLI against the fake claude and hand-written event files into `app/e2e/.fixtures/` (git-ignored, regenerated each run) |
| README assets | screenshots and GIF from the same scenarios | `pnpm readme-assets`: Playwright with `page.clock.install` frozen time, PNG frames, GIF via `gifenc`; output `docs/assets/`, committed, run by hand, not a CI gate |
| lint | `@html` forbidden | `svelte/no-at-html-tags` set to error |

Not automatable: real window focus (AD-15 checklist), real hook latency inside Claude Code (manual: 20 runs of `claude -p --settings` with the hook vs a no-op hook, delta per event recorded in research notes), idle CPU with 8 real sessions (Task Manager over 10 minutes, recorded), WebView2 rendering differences from Playwright Chromium (eyeball once per release).

### AD-18: CI on GitHub Actions

Workflow `ci.yml` on push and pull request:

- `core-linux` (ubuntu-latest): `cargo fmt --check`; `cargo clippy -p officina-core -p officina-host -p officina --all-targets -- -D warnings`; `cargo test` on the same three crates. The app crate is excluded (WebKitGTK system packages are not worth it before v1).
- `windows` (windows-latest): fmt, clippy on the workspace, `cargo test --workspace`, `cargo test --release -p officina -- --ignored` (latency, stress), ts-rs regeneration plus `git diff --exit-code app/src/lib/generated`, `pnpm install --frozen-lockfile`, `pnpm check`, `pnpm lint`, `pnpm test`, `pnpm e2e:fixtures`, `pnpm build --mode e2e`, `pnpm exec playwright install chromium`, `pnpm e2e`. Upload Playwright report on failure.
- `audit` (ubuntu-latest): `cargo deny check` (licenses, advisories, bans, sources) and `pnpm audit --prod`.

Workflow `release.yml` on tag `v*` (windows-latest): `pnpm tauri build` (NSIS), `cargo build --release -p officina`, zip `officina.exe`, `officina-app.exe`, `README.md`, `LICENSE` into `officina-<version>-windows-x64-portable.zip`, `gh release create` with both artifacts and SHA-256 sums. No third-party release action. Binaries are unsigned in v0.1 (risk R7).

`deny.toml` allows Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, CC0-1.0, and MPL-2.0 only as an exception for crates pulled by Tauri (html parsing in `tauri-utils`; unmodified, file-level copyleft, compatible with Apache-2.0 distribution).

### AD-19: Tauri surface

- Commands, declared in `build.rs` through the app manifest so the capability file gates them: `get_board`, `focus_card(key)`, `report_wrong_window(key)`, `refresh`. Window always-on-top uses the core permission `core:window:allow-set-always-on-top`. No shell, fs, http or opener plugin.
- Event: `board://updated`.
- `tauri-plugin-single-instance`: a second launch focuses the existing window. Two collectors would double the spawns and race the retention pass.
- CSP stays as scaffolded; fonts embedded via `@fontsource`.
- Window title becomes `Officina (2 waiting)` and the taskbar gets an amber overlay dot (`set_overlay_icon`) when `waiting_count > 0`: the one signal visible with the window behind the editor. **Unverified:** the exact Tauri 2.12 API name; the story checks it first and drops to the title change only if absent.

### AD-20: Frontend structure

`app/src/lib/generated/` (ts-rs output), `lib/ipc.ts` (the only file importing `@tauri-apps/api`), `lib/board.svelte.ts` (runes store: current board, focus outcomes keyed by card for 10 s), `lib/time.ts`, components `Header`, `Gauge`, `Column`, `Card`, `StaleBanner`. Tokens from framing section 8 as CSS custom properties in `app.css`. Amber only for waiting.

## 3. Concept improvements for v0.1

Kept, because each saves attention for its cost:

1. **Taskbar overlay and window title count** (AD-19). The board is useless behind a maximized editor; this is the cheapest glance.
2. **Keyboard**: `1`-`9` focus the nth Needs-you card, arrows move, `Enter` focuses, `w` reports wrong window, `r` refreshes. Answering "who needs me" without the mouse.
3. **Single instance** (AD-19).
4. **Follow-up refresh after events** (AD-4): without it a card can sit on stale "busy" until the safety poll.
5. **Collapsed Done** beyond 5 cards, with a count; Needs you never collapses. Thirty background sessions must not push the amber cards off screen.
6. **Visibility-adaptive safety poll** (AD-4): idle CPU target.
7. **`quota write --tee`** (AD-16): one-line statusline wiring.
8. **Copy attach command** on background cards (AD-15).
9. **Always-on-top toggle** in the header, slice B: one permission, no code beyond a button.
10. **Doctor reports "hook seen in the last 24 h" and the focus rate**: a silent hook failure becomes visible.

Rejected for v0.1:

- **Tray icon**: competes with the always-open window and adds a close-to-tray lifecycle. The overlay covers the need.
- **Desktop notifications**: interrupts the attention the product protects; PRD already defers.
- **Global hotkey**: plugin plus conflicts; `Alt+Tab` works until the scene exists.
- **Window position memory plugin**: one more plugin; v0.1.x if the owner asks after a week.
- **Autostart**: a Startup-folder shortcut documented in the README costs zero code.
- **Dismissing To resume cards**: needs persistent UI state; the 24 h window bounds the column.

## 4. Dependency list for approval

Rust (existing: serde, serde_json, toml, tauri, tauri-build):

| Crate | Where | Licence | Why |
|---|---|---|---|
| `thiserror` | core, host | MIT/Apache-2.0 | typed errors in libraries, per the Rust rule |
| `anyhow` | officina, officina-app | MIT/Apache-2.0 | error context at the binary boundary |
| `windows-sys` | host, cfg(windows) | MIT/Apache-2.0 | process tree, window enumeration and focus; raw bindings, no runtime |
| `ts-rs` | core, optional feature `ts` | MIT | generated TypeScript for the board contract (AD-13) |
| `tauri-plugin-single-instance` | app | MIT/Apache-2.0 | one collector per workstation |
| `tempfile` | dev only | MIT/Apache-2.0 | isolated temp homes in tests |

Explicitly not added: `clap`, `notify`, `chrono`/`time`/`jiff`, `dirs`, `tokio` as a direct dependency, `sha2`, `log`/`tracing`.

Node (existing kept): devDependencies `gifenc` (MIT, GIF encoder for README assets) and `pngjs` (MIT, decode Playwright PNG frames). No runtime dependency added.

Tools in CI only: `cargo-deny` (MIT/Apache-2.0) installed by the workflow.

## 5. PRD amendments

1. **FR-10, background bullet.** Replace with: "background session: spawns `claude attach <id>` directly as a new console process (`CREATE_NEW_CONSOLE`) in the session's `cwd`, with the floor's `CLAUDE_CONFIG_DIR`; on Windows 11 the default terminal setting decides where it opens. The card also offers to copy the attach command." And replace the paragraph on `wt.exe` arguments with: "The attach process receives an argument vector built from the session id (validated against `^[A-Za-z0-9_-]{1,64}$`) and nothing else; `cwd` is passed as the working directory, never on a command line; no text from prompts or messages reaches a process." Reason: an existing Windows Terminal window does not take the caller's environment, so a floor with `config_dir` would attach to the wrong account (I'm fairly sure, not verified), and `wt.exe` re-parses its arguments (`;`). A direct spawn has neither problem. Gives up: a tab inside the current Windows Terminal window.
2. **FR-10, interactive bullet.** Replace "picks the one whose title contains the folder name of the session's `cwd`" with "tries the folder name of the session's `cwd`, then each parent folder name up to the user home, and picks the first name that matches exactly one window title as a whole word".
3. **FR-4.** Replace "One file per local day" with "One file per UTC day; readers read the current and previous day's files".
4. **Glossary, Card key.** Replace with: "A live interactive session is keyed on `pid` and `startedAt`, a background session on `id`, an ended interactive session on `sessionId`. Hook events attach by `session_id`; the latest `claude agents` snapshot maps each live pid to its current `sessionId` (verified before the classification story; if it does not hold, the hook records the `claude` process id and `SessionStart` re-points)."
5. **FR-6 rule 4.** Append: "Interactive sessions whose process is gone are reconstructed from the event buffer: `SessionEnd` with `prompt_input_exit` or `logout` gives 'ended', no `SessionEnd` gives 'interrupted', `clear` and `resume` are superseded and `other` (non-interactive runs) is not listed."
6. **FR-12.** Replace "On each new Claude Code version seen" with "At startup and whenever the resolved `claude` binary changes".
7. **Section 7, hook latency row.** Replace the "How measured" cell with: "CI: 200 spawns of the release binary with a 200 KB payload, p95 < 30 ms. Workstation: added latency per event through a real `claude -p --settings` run, recorded in research notes".
8. **FR-5.** Append: "`--tee` copies stdin to stdout unchanged so the command can sit in front of an existing statusline."

## 6. Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | `claude agents` costs a few hundred ms of CPU per spawn (guess; native Bun binary); 8 sessions and frequent events could break the < 1 % idle target | rate limit 1/s, 60 s poll when hidden, measure in slice A on the workstation; if over, raise the safety poll and rely on events |
| R2 | Console flash from GUI spawns | `CREATE_NO_WINDOW` on every spawn (AD-6), checked in the manual run |
| R3 | Concurrent append interleaving undocumented | stress test plus counted unparsable lines (AD-10) |
| R4 | `sessionId` after `/clear` unknown | gate before the classification story (AD-11), schema slot reserved |
| R5 | Window title customised, subfolder cwd, cloaked windows on other desktops | ancestor-name matching, candidates listed, wrong-window counter |
| R6 | `SetForegroundWindow` refused | flash plus `refused` outcome, never a silent no-op |
| R7 | Unsigned exes: SmartScreen on install, Defender scanning `officina.exe` on first hook calls | documented in README; first-run latency excluded from p95 by a warm-up of 5 calls, and the workstation measurement records the cold case |
| R8 | Repository inside a SynologyDrive folder: `target/` and `node_modules/` sync churn and file locks break builds | set `CARGO_TARGET_DIR` outside the synced tree on the workstation, or exclude both folders from sync; noted in the contributing section |
| R9 | `claude` update changes the contract | canary per version, missing fields shown, classification degrades to "unknown" |
| R10 | Quota stale when no interactive statusline runs (bg statusline unknown, E3) | freshness always shown; doctor names the account with no fresh file |
| R11 | Prompt excerpts on disk | 24 h retention, 280 chars, `OFFICINA_NO_EXCERPTS`, no real payload in `tests/` |
| R12 | `claude agents` spawned every 15 s might trigger Claude Code's own background work (update checks) | unverified; observe the config dir and process list during the slice A run; if seen, set `DISABLE_AUTOUPDATER=1` on Officina's children only |
| R13 | Tauri overlay API name differs in 2.12 | AD-19 fallback to the title count |
