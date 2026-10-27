# Story 2.5: `officina run` and `run --list`

**Status:** Approved

## Story

As a session working in a guarded repository,
I want `officina run <action> key=value ...` to check everything, run the approved command as an argument vector with a timeout and an output cap, and answer with exit codes a shell already understands,
so that I can do the author's routine actions myself and every run is journaled.

## Acceptance Criteria

1. `officina run <action> [name=value ...]` and `officina run --list` are parsed by hand; tokens split at the first `=`; a token without `=`, a duplicate name or a missing action is a usage error, exit 64 (AD-29). Proof: CLI tests `crates/officina/tests/run.rs::{token_without_equals_is_usage, duplicate_parameter_is_usage, missing_action_is_usage}`.
2. Checks run in the order of AD-29, failing closed: stop flag, current catalogue hash equals the approved one, project of the process cwd (longest prefix, approved catalogue, `free` or `guarded` per D-21), action allowed for the project, parameters (all declared present, none extra, each passing AD-30 and D-5), class. When several fail, the first is reported. Proof: `run.rs::{checks_run_in_the_documented_order, free_project_listing_the_action_is_served}`.
3. Every refusal exits 126 with one stderr line `officina: refused: <reason>` and leaves exactly one `refused` journal line with its reason code (AD-29, AD-32, PRD amendment 11). Proof: `run.rs::every_refusal_code_exits_126_with_one_journal_line`, iterating the core refusal-reason enum's `ALL` constant through a scenario builder whose `match` is exhaustive, so a new code without a scenario does not compile.
4. The stop flag is read at step 1: `relay/stopped` present means stopped; a metadata error other than `NotFound` also means stopped (AD-34, FR-11). The writer comes with story 2.7, so the test writes the file by hand. Proof: `run.rs::stop_flag_written_by_hand_refuses` and host unit test `crates/officina-host/src/relay/home.rs::tests::metadata_error_other_than_not_found_is_stopped` on the pure decision function.
5. Class `free` runs. `after-check` and `human` actions are refused with reason code `class_not_available` (D-20). Proof: `run.rs::free_action_runs`, `run.rs::after_check_and_human_are_not_available_yet`.
6. The program's exit code passes through `std::process::exit` with its full Windows value; 127 when the program cannot start (journaled `spawn_failed`); never 2 from Officina itself (AD-29). Proof: `run.rs::{exit_code_passes_through, full_windows_exit_code_passes_through, missing_program_exits_127}` (the second `#[cfg(windows)]`).
7. Job object (AD-31): before any spawn `officina run` assigns itself to a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; on its own timeout it journals `run_finished` (`timeout`), prints the stderr line and ends the tree with 124; killing `officina run` stops the grandchild. Unsafe lives in `platform/windows.rs` with `// SAFETY:` comments. Proof: `run.rs::{timeout_exits_124_and_the_tree_stops, killing_run_stops_the_grandchild}` (`#[cfg(windows)]`, fake tool mode `tree`: the heartbeat file stops growing).
8. Output (AD-31): stdout and stderr are copied through as bytes arrive, sharing a `max_output_kib` budget; past it both streams keep draining and a last line says `[officina: output cut at N KiB, M bytes dropped]`; each stream is SHA-256 hashed over all its bytes; byte counts and hashes go to `run_finished`. Proof: `run.rs::{output_is_capped_and_drained, hashes_cover_all_bytes}` (fake tool mode `bytes:<n>`).
9. Closed parameters (PRD section 7): every refusal case of `tests/vectors/argv.json` sent through `officina run` is refused before any process starts. Proof: `run.rs::every_argv_injection_is_refused_before_launch`, iterating the vector file; the fake tool's launch log stays absent.
10. Argument fidelity: every accepted `native` case of `argv.json`, with its program replaced by the fake tool, arrives at the fake tool byte for byte as the vector expects; the same holds for the `wsl` cases through a real distro when one is installed, with the program replaced by `/bin/bash -c 'printf "%s\0" "$@"' argv0`, and is skipped with a printed reason otherwise. Proof: `run.rs::{every_accepted_native_case_arrives_intact, every_accepted_wsl_case_arrives_intact}`.
11. Launch (AD-30, D-6): `native` runs with the project folder as cwd, `CREATE_NO_WINDOW`, the declared `env` added and stdin null; `wsl` takes `wsl.exe` from `GetSystemDirectoryW`, never from `PATH` or `SystemRoot`, and its command removes `WSLENV`. Proof: `run.rs::native_launch_gets_cwd_env_and_null_stdin` (fake tool log), `run.rs::wsl_program_comes_from_the_system_directory` (with `SYSTEMROOT` and `PATH` pointing at a temp dir, the journaled `argv` starts with the system directory's `wsl.exe`), and host unit test `crates/officina-host/src/relay/run.rs::tests::wsl_command_removes_wslenv` (inspects `Command::get_envs`).
12. Timeout margin (D-6): the inner WSL `timeout` is `timeout_s + 5`, so on a hung `wsl` action the job kill fires first and the journal says `timeout`, not `exited 124`. Proof: the `argv.json` cases of story 2.1 AC 9 and `run.rs::outer_deadline_precedes_the_inner_timeout` (asserts the launch's inner value against the action's `timeout_s`).
13. Journal (AD-32): `run_started` (`run_id`, project, session id, action, class, params, argv, catalogue hash) before the spawn, `run_finished` after; a journal lock timeout refuses before spawning. Every run leaves one start line and at most one end line (PRD amendment 11). Proof: `run.rs::{run_leaves_one_start_and_one_finish_line, journal_lock_timeout_refuses_before_spawn}`.
14. The session id comes from `CLAUDE_CODE_SESSION_ID` (E13), `null` when absent (AD-29). Proof: `run.rs::session_id_from_env_or_null`.
15. `officina run --list` follows D-21: the cwd project's approved actions with their parameters and allowed values or pattern, unavailable classes marked (D-17), a stopped relay or a pending change stated, exit 0; outside every project it says so and exits 0. Proof: `run.rs::{list_prints_the_projects_approved_actions, list_marks_unavailable_classes, list_outside_every_project, list_reports_stop_and_pending_change}`.
16. Zero write: `run` and `run --list` join the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.

## Tasks

- [ ] Core: refusal-reason enum with `ALL`, class decision for `free`, run plan (checks in order) as pure functions (AC 2, 3, 5)
- [ ] Host `relay::run`: checks, launch build via core `argv`, spawn, `try_wait` every 50 ms (AD-3), reader threads with shared budget and SHA-256, journal lines (AC 2-13)
- [ ] `platform/windows.rs`: job object create, assign self, terminate; `GetSystemDirectoryW`; `windows-sys` features of addendum section 2 (AC 7, 11)
- [ ] `officina run` and `run --list` in `crates/officina/src/main.rs`; update `USAGE` (AC 1, 15)
- [ ] `crates/officina/tests/run.rs` using `officina-fake-tool` (story 2.4); extend the zero-write test (AC 1-16)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (`officina run`, exit codes) (D-34)

## Dev Notes

- Requirements: FR-7 as amended (amendment 7), FR-8 `free`, FR-11 (read side), FR-13. Decisions: AD-29, AD-30, AD-31, AD-32, AD-34 (read side; the writer is story 2.7). Facts: E13 (session id in Bash children; killing `wsl.exe` ends the Linux process). Epic decisions: D-5, D-6, D-17, D-20, D-21.
- `officina run` is the authority and fails closed; the gate is containment and fails open on internal faults only (AD-28, D-15). Do not share their error handling.
- Exit codes: 126 refused, 124 timeout, 127 could not start, 64 usage, the program's own code otherwise. A program can itself exit 126 or 124; the stderr line and the journal disambiguate, not the code.
- `class_not_available` is a deliberate, tested intermediate state (D-20), removed by stories 2.6 and 2.8; it is not a placeholder.
- Tests approve the catalogue through `officina_host::relay::authority::approve_catalogue` (dev-dependency feature, story 2.2), never by writing `approved.json` by hand, so the flow matches the window's.
- The test harness clears the environment and sets `PATH` empty: floors in tests point `program` at `env!("CARGO_BIN_EXE_officina-fake-tool")`.
- AC 10's WSL leg needs an installed distro; hosted runners usually have none, so it runs on the workstation. Detect with `wsl.exe -l -q` from the system directory, and print why when skipping.
- `timeout_s` is at least 1 (AD-23): the timeout tests take one to two seconds; keep them few.
- Non-Windows builds kill the child only (Linux is v1, AD-31); the tree tests are `#[cfg(windows)]`.

## Testing

- Core unit: `crates/officina-core/src/relay/classes.rs` (class decision), refusal enum.
- Host unit: `crates/officina-host/src/relay/{home,run}.rs`.
- CLI integration: `crates/officina/tests/{run,quota_doctor}.rs` with `officina-fake-tool`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: fake tool moved to story 2.4; D-6 (`WSLENV`, inner timeout margin), D-21 (free projects served, `--list`), argument fidelity through the fake tool and WSL, stop flag written by hand
