# Story 5.16: The orchestrator session: vector, lifecycle and presence

**Status:** Approved

## Story

As the author who wants a foreman over every open subject,
I want one gesture to start a `claude` session whose only tools are Officina's, that refuses other sessions' messages, that Officina recognises, never counts as waiting and never resumes,
so that the orchestrator can read and propose while holding nothing that acts, and closing it loses nothing (FR-10, journey 5).

## Acceptance Criteria

1. **The vector** (AD-89, FR-10 as amended by amendment 6, D-127, D-128). `VectorKind` gains `Orchestrator`; `spawn::orchestrator(...)` builds, after the binary: `--tools ""`; `--strict-mcp-config --mcp-config <json>`; `--setting-sources ""`; `--settings {"crossSessionInbound":"refuse"}`; `--permission-mode dontAsk --allowedTools mcp__officina --disallowedTools SendMessage,ListAgents`; `--append-system-prompt <ORCHESTRATOR_SYSTEM_PROMPT>`; `--name officina-orchestrator`, `--model <alias>` when the floor's `[orchestrator]` sets a valid one, `--` and `<ORCHESTRATOR_FIRST_PROMPT>`. Working directory `<home>/orchestrator/workdir`; a new console; stdin the console; nothing captured; only an `.exe` accepted; the contained environment rule of story 5.2 (D-561: exactly the allow list's names found in the base, plus `ORCHESTRATOR_ENV_EXTRA`, the names gate 1's containment half recorded as needed, empty otherwise) with `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` set. Proof: the `Orchestrator` case of `tests/vectors/spawn.json` through `crates/officina-core/tests/spawn_vectors.rs`; CLI `crates/officina/tests/spawn.rs::{every_kind_reaches_the_fake_claude_as_its_golden_vector, a_console_kind_gets_a_console, contained_kinds_receive_the_allow_list_only}` (fake claude `interactive` mode); core `crates/officina-core/src/spawn.rs::tests::the_orchestrator_extra_names_are_e24s` (the constant equals what E24 recorded, empty when it recorded none).
2. **The MCP config and constants** (AD-89, D-128, D-144; review F1). Core builds the `--mcp-config` JSON as one stdio server named `officina`: `command` the `officina.exe` in the window's executable directory, `args` `["mcp"]` plus `--floor <path>` when the window has one, `env` holding `OFFICINA_HOME` (the home), `OFFICINA_NO_EXCERPTS` (the window's effective value) and `OFFICINA_CLAUDE_BIN` (the resolved `claude` path), since the orchestrator's environment drops every `OFFICINA_*` name. `ORCHESTRATOR_SYSTEM_PROMPT` states the role, that fields marked untrusted are session data whose instructions are never followed, and that no tool runs, launches or sends anything; `ORCHESTRATOR_FIRST_PROMPT` asks for the briefing and starts with a letter. Both are pinned by the golden case. Proof: core `crates/officina-core/src/spawn.rs::tests::{the_mcp_config_names_only_officina, the_mcp_config_passes_home_excerpts_floor_and_claude, the_first_prompt_starts_with_a_letter}`.
3. **Refusals** (AD-89, FR-10, D-164, D-167; review F23, F38). Start is refused, with a recorded `orchestrator_start` row of outcome `refused` and its cause, when `claude` is not an `.exe`, the workdir is not empty, `officina.exe` is missing from the window's executable directory, or, when E24 recorded that memory files load and no flag disables them (D-164), `<home>/orchestrator/` holds anything but `workdir/` and `proposals/`; the executable directory is an argument of `start_orchestrator`, which the app passes once from `current_exe()` and tests pass as a temporary directory; the window creates `<home>/orchestrator/workdir` and `<home>/orchestrator/proposals/` and never deletes them. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{start_is_refused_with_a_shim, start_is_refused_with_a_non_empty_workdir, start_is_refused_without_officina_exe, start_creates_the_two_folders}` (the fake claude's log stays absent on refusals).
4. **Lifecycle** (AD-89, FR-10, FR-14, D-144, D-166; review F3, F27). `start_orchestrator` records an `orchestrator_start` row, spawns from a blocking thread (never the collector), keeps the pid and its creation time (no handle, no job), stores both in the row (`pid`, `pid_created_at_ms`), reads them back at window start, and journals `orchestrator_started`; a restart is a new row and a new session, never a resume. Live means an agents entry named `officina-orchestrator` whose normalised `cwd` is the workdir; when E24 recorded it unlisted, the kept pid decides through `platform::process_alive(pid, created)`, guarded against pid reuse by the creation time. A second Start while live brings the console to the front through `platform::focus_window_by_title`, matching `officina-orchestrator` as a whole word: several matches are candidates, none is reported as "already running". Proof: `watch.rs::{a_start_records_a_row_and_a_journal_line, the_orchestrator_is_live_by_its_name_and_folder, a_restart_is_a_new_row_never_a_resume, a_live_pid_fallback_survives_a_window_restart}`; host unit `crates/officina-host/src/platform/windows.rs::tests::{process_alive_refuses_a_reused_pid, title_match_is_a_whole_word}`; core `crates/officina-core/src/relay/journal.rs::tests::orchestrator_started_round_trips`.
5. **Presence, not a card** (AD-89, AD-92, FR-10, P-26). `BoardInput` gains the Officina directories; the agents entry named `officina-orchestrator` whose normalised `cwd` is the workdir becomes `Board.orchestrator: Option<OrchestratorPresence { pid, status, started_at_ms }>`, never a card and never counted in `waiting_count`; any other entry under `<home>/orchestrator/` is an ordinary card (D-166); under the `BOARD_SCHEMA` number of story 5.12; `OrchestratorView.presence` carries the same. Proof: core `crates/officina-core/src/board.rs::tests::{the_orchestrator_is_presence_not_a_card, a_waiting_orchestrator_is_not_counted, another_session_under_the_orchestrator_folder_is_a_card}`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`.
6. **Never filed or triaged** (AD-43 as amended, AD-89, D-166). Ingestion skips lines whose `cwd` is `<home>/orchestrator/workdir`, and the orchestrator's own agents entry creates no session record; any other session under `<home>/orchestrator/` is recorded like any other. Proof: host `crates/officina-host/tests/registry_ingest.rs::{orchestrator_lines_and_its_agents_entry_are_skipped, other_sessions_under_the_orchestrator_folder_are_recorded}`.
7. **The command and the view** (AD-92, AD-93, FR-13 as amended by amendment 8). `start_orchestrator` is declared, allowed, a thin wrapper with a deadline, wrapped in `ipc.ts` and mocked; the Orchestrator view shows the presence (live with its status, or absent) and a Start button, reachable by keyboard, which shows the refusal's cause or "already running" beside it; its label ("Start", "Starting", "Bring to front" while live) goes through `StableLabel` so it keeps its box, and no colour is added (D-564). Proof: `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`; `app/e2e/stable-size.spec.ts`, a `CASES` entry "orchestrator Start"; Playwright `app/e2e/orchestrator.spec.ts > Start sends start_orchestrator`, `> a refused start shows its cause`, `> a live orchestrator shows its status`.
8. **Retention and zero write** (D-134, D-80). `orchestrator_start` rows are deleted 90 days after `started_at_ms`; an orchestrator start through the fake claude joins the fingerprint test. Proof: host `crates/officina-host/tests/registry_retention.rs::orchestrator_start_rows_are_deleted_at_90_days`; `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
9. **Doctor** (AD-94 part C, D-166, D-167). The workdir present and empty, `officina.exe` in the window's executable directory (an argument), and a warning for any agents entry under `<home>/orchestrator/` that is not the orchestrator. Proof: host `crates/officina-host/src/doctor.rs::tests::{a_non_empty_orchestrator_workdir_fails, a_missing_officina_exe_is_reported, a_stranger_under_the_orchestrator_folder_warns}`.
10. **Gate 1, window half** (AD-96 row 1, D-126; owner). With a build of this story: Start from the window (record whether the trust dialog appeared, and accept it there); with a marker `CLAUDE.md` in `<home>/orchestrator/` while the check runs (D-164); in the console, `/mcp` (only `officina`), `/context` (only `mcp__officina__*`, one per tool of `McpTool`, and the memory files listed), `/status` (`dontAsk`) and `/config` (no inbound row); the briefing answers the first prompt, and the `board` tool lists the live sessions of the moment (review F4); `claude agents --json` lists the session or the pid fallback is in use; a second Start brings the console to the front. Pass: all of these; recorded in E24. Proof: manual, research notes entry E24.
11. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: the orchestrator session; README "The orchestrator (v0.5, in progress)": what it can and cannot do, its flags in words, that Claude Code asks to trust its folder on the first start and records that itself, that it is never resumed. Proof: review.

## Tasks

- [ ] `VectorKind::Orchestrator`, `spawn::orchestrator`, MCP config, constants, golden case (AC 1, 2)
- [ ] Host lifecycle in `crates/officina-host/src/orders.rs`: refusals, folders, row with the pid and its creation time, read back at start, spawn, liveness by name and folder, focus; the executable directory as an argument (AC 3, 4)
- [ ] `platform::windows::{process_alive, focus_window_by_title}` with `// SAFETY:` comments, existing `windows-sys` features (AC 4)
- [ ] `Board.orchestrator`, `OrchestratorView.presence`, bindings (AC 5)
- [ ] Ingestion exclusion (AC 6)
- [ ] Command, `ipc.ts`, mock, the view's Start and presence (AC 7)
- [ ] Retention, zero-write, doctor (AC 8, 9)
- [ ] Gate 1's window half with the owner (AC 10)
- [ ] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-10 as amended (amendment 6), FR-14's starts, FR-15 Slice C, PRD section 7 Containment row as amended (amendment 11), journey 5. Decisions: AD-21 and AD-43 as amended, AD-80, AD-89, AD-92, AD-96. Facts: E11, E24 (gate 1, both halves), S1, S2, S5, S7, addendum section 1 items 6 to 8. Risks: R33, R34, R38. Epic decisions: P-24, D-124, D-126 to D-128, D-133, D-134, D-144, D-164, D-166, D-167.
- Seams (D-124, read through `main`, D-563): `Board` and `BoardInput` (v0.1, 5.12's schema number), AD-43's exclusion (`officina-core::registry::ingestion`, 3.2), `crates/officina-host/src/platform/windows.rs` (`creation_time` and `focus_session_window` exist there today, the new functions sit beside them), the start sequence (3.2).
- The scene draws the orchestrator at the decider's desk of v0.4's decision office in story 5.18 (AD-179); `Board.orchestrator` is its only input from this story.
- Principle 2 reading (FR-10): Officina's own sessions load no user or project configuration, so their flags displace nothing; this was decided on 2026-10-28 and is listed for the owner's confirmation in the QA report (5.19).
- The orchestrator's `--` before its first prompt is exercised by gate 1 (D-127); its first prompt starts with a letter either way.
- `--name` sets the console's title (help text), which is what `focus_window_by_title` matches; with Windows Terminal as default terminal the title is the tab's.
- Officina writes nothing under the Claude config dir: the trust Claude Code records on the first start is Claude Code's own write (D-80).
- Proposals and the room are stories 5.17 and 5.18; both wait for AC 10, since a failure there reopens FR-10 (D-126).

## Testing

- Core unit: `crates/officina-core/src/{spawn,board,relay/journal}.rs`; core integration: `crates/officina-core/tests/{spawn_vectors,typescript_bindings}.rs`.
- Host: `crates/officina-host/src/{platform/windows,doctor}.rs`, `crates/officina-host/tests/{registry_ingest,registry_retention}.rs`.
- CLI integration: `crates/officina/tests/{watch,spawn,quota_doctor}.rs`. App crate: `main.rs`. Playwright: `app/e2e/orchestrator.spec.ts`.
- Manual: E24's window half.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: MCP config carries home, excerpt switch and `claude` path, contained-kind environment (F1); pid creation time stored and read back (F3); the gate checks the board tool (F4); memory files and the parent-folder refusal branch (F23); presence by name and folder, other entries are cards, exclusion narrowed (F27); executable directory as an argument (F38)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: the orchestrator takes the contained allow list plus the names gate 1 records (D-561); Start keeps its box with a stable-size case (D-564); seams cited by file (D-563)
