# Story 1.5: Collector and headless board

**Status:** Approved

## Story

As the author (and as every end-to-end test),
I want a collector that runs `claude agents`, reads the event buffer and recomputes the board on the right signals, and a `board --json` command that prints it once,
so that the board stays current without polling hard, and the same board feeds the CLI, the window and the e2e fixtures.

## Acceptance Criteria

1. `claude` is resolved in the order of AD-6 (env override, `PATH` names, native install fallback per E8). Proof: host tests `crates/officina-host/tests/claude_bin.rs::{env_override_wins, path_prefers_exe_over_cmd, falls_back_to_local_bin, none_found_is_an_error}` using temp dirs and an injected env.
2. `claude agents --json --all` runs with the deadline, output caps and stderr excerpt of AD-6 and FR-1; `CLAUDE_CONFIG_DIR` is set on the child only when a config dir override applies. Proof: host tests `claude_bin.rs::{ok_fixture_parses, garbage_is_an_error_with_excerpt, exit1_is_an_error_with_stderr, hang_is_killed_at_the_deadline, config_dir_reaches_the_child}` with the fake claude (the deadline is a parameter so the hang test runs under 2 s).
3. A failed or timed-out call keeps the previous snapshot and marks the board stale with the error; it never empties the board (FR-1). Proof: host test `crates/officina-host/tests/collector.rs::failure_keeps_previous_snapshot_and_marks_stale`.
4. `officina_host::collector::snapshot_once` builds one board from agents, events and the optional floor. Proof: host test `collector.rs::snapshot_once_builds_a_board`.
5. `officina board --json` prints the board and exits 0 when fresh, 3 when stale with the board still printed, 2 on a usage error (AD-13, FR-14). Proof: CLI tests `crates/officina/tests/board.rs::{fresh_board_exits_0, stale_board_is_printed_and_exits_3, bad_flag_exits_2, output_parses_as_board}`.
6. The refresh planner (AD-4) is a pure state machine in the host crate fed with observations and `now_ms`, returning actions: event growth schedules an agents refresh after the debounce plus a follow-up; a sessions-dir mtime change schedules a refresh; a quota mtime change re-reads without spawning; the safety poll follows visibility; spawns are rate-limited. Proof: host unit tests `collector::plan::tests::{event_growth_schedules_debounced_refresh_and_follow_up, sessions_mtime_schedules_refresh, quota_change_never_spawns, safety_poll_follows_visibility, spawns_are_rate_limited}`.
7. The collector thread follows AD-3: named thread, message loop with `Refresh`, `Visibility`, `Shutdown`, one `claude` child at a time, latest board in a shared lock, and it emits only when the serialized board bytes change (AD-5). Proof: host tests `collector.rs::{appending_an_event_emits_a_new_board, unchanged_board_is_not_re_emitted, shutdown_stops_the_thread_within_1s}`.
8. The sessions directory is only ever passed to `fs::metadata` (FR-2). Proof: review of `collector` (structural); story 1.11 fingerprints writes.
9. Slice A workstation run: CPU per `claude agents` spawn (R1) and any Claude Code side activity under the config dir or process list (R12) are observed and recorded in research notes. Proof: manual, per AD-17 (idle CPU is not automatable).

## Tasks

- [ ] `officina_host::claude`: resolution and `run_agents(deadline)` with `try_wait` loop, kill, stdout cap, stderr excerpt, `CREATE_NO_WINDOW` (AC 1, 2)
- [ ] `officina_host::collector::plan` pure planner (AC 6)
- [ ] `officina_host::collector`: thread, messages, shared board, emit callback, `snapshot_once` (AC 3, 4, 7, 8)
- [ ] `officina board --json` subcommand and exit codes (AC 5)
- [ ] Host tests `claude_bin.rs`, `collector.rs`; CLI tests `board.rs` (AC 1-7)
- [ ] Workstation observation for R1 and R12, research notes (AC 9)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Behaviour: FR-1, FR-2, FR-14. Decisions: AD-2 (GUI reuses this in-process), AD-3 (threading), AD-4 (triggers table), AD-5 (diff and emit, absolute timestamps), AD-6 (resolution, spawn flags), AD-13 (exit codes). Facts: E1, E7, E8.
- Slice note: the PRD places FR-2 in slice B; this story builds the AD-4 triggers in slice A because the collector is written once. If time runs short, the safety poll alone satisfies slice A and the event triggers move to a follow-up story.
- `CREATE_NO_WINDOW` cannot be asserted by a test; the console-flash check (R2) is part of the manual run in story 1.6.
- No tokio, no `notify` crate (AD-3, AD-4, spine section 4).
- Event file rollover at UTC midnight: the watched file name changes; the planner must re-stat the new name and keep reading yesterday's tail.
- Stale with no previous snapshot (first call fails): `board --json` prints empty columns plus `stale` with the error and exits 3.

## Testing

- Host unit: `crates/officina-host/src/collector/plan.rs`.
- Host integration: `crates/officina-host/tests/{claude_bin,collector}.rs`.
- CLI integration: `crates/officina/tests/board.rs`.
- Manual: R1 and R12 observation on the workstation.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
