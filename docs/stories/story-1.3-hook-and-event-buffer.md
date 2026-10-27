# Story 1.3: Hook command and event buffer

**Status:** Approved

## Story

As the author running Claude Code sessions,
I want `officina hook` to record each hook event in a local buffer without ever slowing or breaking a session,
so that the board knows what happened in every session since it started.

## Acceptance Criteria

1. `officina hook` reads one payload on stdin, appends exactly one line to `<officina home>/events/<UTC date>.jsonl`, prints nothing on stdout or stderr and exits 0 (FR-3, AD-16). Proof: CLI integration test `crates/officina/tests/hook.rs::appends_one_line_per_event` driven by hand-written payloads, one per event name of the hook event constant.
2. It exits 0 and prints nothing for empty stdin, malformed JSON, a non-object JSON, an `OFFICINA_HOME` that is a file, and an unwritable events path. Proof: CLI tests `hook.rs::{empty_stdin_exits_0, malformed_json_exits_0, non_object_exits_0, home_is_a_file_exits_0}`.
3. A missing `events/` directory is created and the append retried once (AD-10). Proof: CLI test `hook.rs::creates_events_dir_on_first_event`.
4. A panic inside the hook body still exits 0 (AD-16). Proof: unit test `crates/officina/src/hook.rs::tests::guard_turns_panic_into_success` on the `catch_unwind` wrapper with a panicking closure.
5. `OFFICINA_NO_EXCERPTS=1` drops `prompt` and `last_message` from the stored line. Proof: CLI test `hook.rs::no_excerpts_env_drops_excerpts`.
6. The hook spawns no process: with `OFFICINA_CLAUDE_BIN` pointing at the fake claude and `FAKE_CLAUDE_LOG` set, the log is never created. Proof: CLI test `hook.rs::hook_spawns_no_claude`. The absence of sockets is held by the module having no `std::net` import, checked in review.
7. The hook never deletes: an event file dated three days earlier survives a hook run. Proof: CLI test `hook.rs::hook_never_deletes_old_files`.
8. `officina_host::events` reads the current and previous UTC day files, tolerates a truncated last line, and reports the skipped-line count. Proof: host integration tests `crates/officina-host/tests/event_buffer.rs::{reads_today_and_yesterday, truncated_last_line_is_skipped_and_counted, older_files_are_ignored}`.
9. Incremental reading returns new complete lines from a byte offset and never consumes a partial last line, which is re-read once completed. Proof: host test `event_buffer.rs::partial_line_is_left_for_the_next_read`.
10. The retention function deletes event files dated before yesterday (UTC) and leaves today, yesterday and unrelated files (AD-9). Proof: host test `event_buffer.rs::retention_keeps_today_and_yesterday_only`.
11. Hook latency: 200 sequential spawns of the release binary with a 200 KB payload, after 5 warm-up calls, p95 < 30 ms; the same run through `bash -c` is reported when Git Bash exists, not gated (AD-17, PRD section 7, R7). Proof: `#[ignore]` test `crates/officina/tests/perf.rs::hook_latency_p95`, run by CI with `--release -- --ignored`.
12. Concurrent appends: 16 processes x 500 hook invocations produce 8000 parseable lines and zero skipped lines (AD-10, R3). Proof: `#[ignore]` test `perf.rs::concurrent_appends_never_interleave`.
13. Workstation latency through a real `claude -p --settings` run (20 runs with the hook vs a no-op hook) is recorded in `docs/research-notes.md`. Proof: manual, per AD-17 "not automatable".
14. Unknown subcommands and arguments exit 2 with usage on stderr (AD-16 hand-parsed arguments). Proof: CLI test `crates/officina/tests/cli.rs::unknown_subcommand_exits_2`.

## Tasks

- [ ] Hand-written argument dispatch in `crates/officina/src/main.rs`; usage text; exit 2 path (AC 14)
- [ ] `crates/officina/src/hook.rs`: read stdin to end, reduce with core, append via host, `catch_unwind` guard (AC 1, 2, 4, 5)
- [ ] Add the hook event names as one constant in core (used here and by doctor in story 1.8) (AC 1)
- [ ] `officina_host::events::append`: `OpenOptions::append(true).create(true)`, single `write_all` of the line plus `\n`, create-dir-and-retry on `NotFound` (AC 1, 3)
- [ ] `officina_host::events::{read_window, read_from, prune}` (AC 8, 9, 10)
- [ ] Hand-written payload files under `crates/officina/tests/data/hook/` (redacted, no real payload, FR-4) (AC 1)
- [ ] CLI tests `hook.rs`, `cli.rs`; host tests `event_buffer.rs` (AC 1-10, 14)
- [ ] `perf.rs` with the two ignored tests; add the `cargo test --release -p officina -- --ignored` step to `ci.yml` (AC 11, 12)
- [ ] Run the workstation measurement and record it in research notes (AC 13)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Behaviour: FR-3, FR-4. Append semantics and fallback: AD-10. Binary shape: AD-16. Retention by the app only: AD-9. Latency and stress harness: AD-17. Payload shapes: E2.
- Hook vs payload field names: the payload's `last_assistant_message` is stored as `last_message` (AD-8). `ts` is taken by the hook on receipt.
- Keep the hook path free of everything else in the host crate: no collector, no `claude` resolution. `officina-host` links `windows-sys` for focus (story 1.7); check with `dumpbin /imports` on the release `officina.exe` that the hook binary does not import `user32.dll`, which would add load time to every hook. Record the result in Completion Notes.
- Retention is implemented here and scheduled by the GUI only (startup and hourly, story 1.12). Neither the hook nor `board --json` deletes anything.
- A reader must never treat a missing file as an error: no events yet is normal.

## Testing

- Unit: `crates/officina/src/hook.rs` (guard).
- Host integration: `crates/officina-host/tests/event_buffer.rs`.
- CLI integration: `crates/officina/tests/{hook,cli}.rs`.
- Ignored release tests: `crates/officina/tests/perf.rs`, CI `windows` job.
- Manual: workstation latency, research notes.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
