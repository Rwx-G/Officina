# Story 2.5: `officina run` and `run --list`

**Status:** Review

## Story

As a session working in a guarded repository,
I want `officina run <action> key=value ...` to check everything, run the approved command as an argument vector with a timeout and an output cap, and answer with exit codes a shell already understands,
so that I can do the author's routine actions myself and every run is journaled.

## Acceptance Criteria

1. `officina run <action> [name=value ...]` and `officina run --list` are parsed by hand; tokens split at the first `=`; a token without `=`, a duplicate name or a missing action is a usage error, exit 64 (AD-29). Proof: CLI tests `crates/officina/tests/run.rs::{token_without_equals_is_usage, duplicate_parameter_is_usage, missing_action_is_usage}`.
2. Checks run in the order of AD-29, failing closed: stop flag, current catalogue hash equals the approved one, project of the process cwd (longest prefix, approved catalogue, `free` or `guarded` per D-21), action allowed for the project, parameters (all declared present, none extra, each passing AD-30 and D-5), class. When several fail, the first is reported. Proof: `run.rs::{checks_run_in_the_documented_order, free_project_listing_the_action_is_served}`.
3. Every refusal exits 126 with one stderr line `officina: refused: <reason>` and leaves exactly one `refused` journal line with its reason code (AD-29, AD-32, PRD amendment 11). Proof: `run.rs::every_refusal_code_exits_126_with_one_journal_line`, iterating the core refusal-reason enum's `ALL` constant through a scenario builder whose `match` is exhaustive, so a new code without a scenario does not compile.
4. The stop flag is read at step 1: `relay/stopped` present means stopped; a metadata error other than `NotFound` also means stopped (AD-34, FR-11). The writer comes with story 2.7, so the test writes the file by hand. Proof: `run.rs::stop_flag_written_by_hand_refuses` and host unit test `crates/officina-host/src/relay/home.rs::tests::metadata_error_other_than_not_found_is_stopped` on the pure decision function.
5. Class `free` runs. `after-check` and `human` actions are refused with reason code `class_not_available` (D-20). Proof: `run.rs::free_action_runs`, `run.rs::after_check_and_human_are_not_available_yet` (deleted by story 2.8, D-20 gone).
6. The program's exit code passes through `std::process::exit` with its full Windows value; 127 when the program cannot start (journaled `spawn_failed`); never 2 from Officina itself (AD-29). Proof: `run.rs::{exit_code_passes_through, full_windows_exit_code_passes_through, missing_program_exits_127}` (the second `#[cfg(windows)]`).
7. Job object (AD-31): before any spawn `officina run` assigns itself to a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; on its own timeout it journals `run_finished` (`timeout`), prints the stderr line and ends the tree with 124; killing `officina run` stops the grandchild. Unsafe lives in `platform/windows.rs` with `// SAFETY:` comments. Proof: `run.rs::{timeout_exits_124_and_the_tree_stops, killing_run_stops_the_grandchild}` (`#[cfg(windows)]`, fake tool mode `tree`: the heartbeat file stops growing).
8. Output (AD-31): stdout and stderr are copied through as bytes arrive, sharing a `max_output_kib` budget; past it both streams keep draining and a last line says `[officina: output cut at N KiB, M bytes dropped]`; each stream is SHA-256 hashed over all its bytes; byte counts and hashes go to `run_finished`. Proof: `run.rs::{output_is_capped_and_drained, hashes_cover_all_bytes}` (fake tool mode `bytes:<n>`).
9. Closed parameters (PRD section 7): every refusal case of `tests/vectors/argv.json` sent through `officina run` is refused before any process starts. Proof: `run.rs::every_argv_injection_is_refused_before_launch`, iterating the vector file; the fake tool's launch log stays absent.
10. Argument fidelity: every accepted `native` case of `argv.json`, with its program replaced by the fake tool, arrives at the fake tool byte for byte as the vector expects; the same holds for the `wsl` cases through a real distro when one is installed, with the program replaced by `/bin/bash -c 'printf "%s\0" "$@"' argv0`, and is skipped with a printed reason otherwise. Proof: `run.rs::{every_accepted_native_case_arrives_intact, every_accepted_wsl_case_arrives_intact}`.
11. Launch (AD-30, D-6): `native` runs with the project folder as cwd, `CREATE_NO_WINDOW`, the declared `env` added and stdin null; `wsl` takes `wsl.exe` from `GetSystemDirectoryW`, never from `PATH` or `SystemRoot`, and its command removes `WSLENV`. Proof: `run.rs::native_launch_gets_cwd_env_and_null_stdin` (fake tool log), `run.rs::wsl_program_comes_from_the_system_directory` (with `SYSTEMROOT` and `PATH` pointing at a temp dir, the journaled `argv` starts with the system directory's `wsl.exe`), and host unit test `crates/officina-host/src/relay/run.rs::tests::wsl_command_removes_wslenv` (inspects `Command::get_envs`).
12. Timeout margin (D-6): the inner WSL `timeout` is `timeout_s + 5`, so on a hung `wsl` action the job kill fires first and the journal says `timeout`, not `exited 124`. Proof: the `argv.json` cases of story 2.1 AC 9 and `run.rs::outer_deadline_precedes_the_inner_timeout` (asserts the launch's inner value against the action's `timeout_s`).
13. Journal (AD-32): `run_started` (`run_id`, project, session id, action, class, params, argv, catalogue hash) before the spawn, `run_finished` after; a journal lock timeout refuses before spawning. Every run leaves one start line and at most one end line (PRD amendment 11). Proof: `run.rs::{run_leaves_one_start_and_one_finish_line, journal_lock_timeout_refuses_before_spawn}`.
14. The session id comes from `CLAUDE_CODE_SESSION_ID` (E13), `null` when absent (AD-29). Proof: `run.rs::session_id_from_env_or_null`.
15. `officina run --list` follows D-21: the cwd project's approved actions with their parameters and allowed values or pattern, unavailable classes marked (D-17), a stopped relay or a pending change stated, exit 0; outside every project it says so and exits 0. Proof: `run.rs::{list_prints_the_projects_approved_actions, list_marks_unavailable_classes, list_outside_every_project, list_reports_stop_and_pending_change}` (`list_marks_unavailable_classes` deleted by story 2.8).
16. Zero write: `run` and `run --list` join the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.

## Tasks

- [x] Core: refusal-reason enum with `ALL`, class decision for `free`, run plan (checks in order) as pure functions (AC 2, 3, 5)
- [x] Host `relay::run`: checks, launch build via core `argv`, spawn, `try_wait` every 50 ms (AD-3), reader threads with shared budget and SHA-256, journal lines (AC 2-13)
- [x] `platform/windows.rs`: job object create, assign self, terminate; `GetSystemDirectoryW`; `windows-sys` features of addendum section 2 (AC 7, 11)
- [x] `officina run` and `run --list` in `crates/officina/src/main.rs`; update `USAGE` (AC 1, 15)
- [x] `crates/officina/tests/run.rs` using `officina-fake-tool` (story 2.4); extend the zero-write test (AC 1-16)
- [x] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (`officina run`, exit codes) (D-34)

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

- **Decisions where the documents are silent**, each the reading that fails closed or matches the architecture:
  - Refusal codes (`RefusalReason`, a closed enum with the journal's types): `stopped`, `not_approved`, `approved_unreadable`, `floor_unreadable`, `catalogue_changed`, `outside_projects`, `action_not_allowed`, `missing_parameter`, `unknown_parameter`, `invalid_value`, `class_not_available`. AD-29's step 2 and step 5 each split into the cases a session can act on differently.
  - Two refusals cannot leave a journal line and are not in the enum: a journal that stays locked (AC 13, `journal_unavailable`) and no Officina home (`no_home`). Both exit 126 with one stderr line.
  - The stderr line is `officina: refused: <why> (<code>)`, on one line (a quoted TOML error is folded), and never echoes a refused value, only parameter names.
  - Two approved projects with one path: the first by name that lists the action serves it.
  - The output budget is shared first come, first served; the cut line goes last on stdout, after a newline when stdout did not end with one. After the program exits, its streams get 500 ms to close; a process it left holding them ends with the tree.
  - `spawn_failed` carries the start error in `detail`. A failure of `try_wait` itself (never seen) journals `exited` without a code and exits 1.
  - `--wait` is a usage error until story 2.8 adds it; `run --list` exits 0 whatever it says.
  - `CreateJobObjectW` is declared in `platform/windows.rs`: windows-sys gates it behind `Win32_Security` only for the security-attributes pointer type, which Officina passes null, and the addendum does not list that feature.
  - If `officina run` cannot put itself in the job, the child is put in it right after it starts (best effort, the AD-31 race accepted in that case only).
  - The collector's `claude` call uses the same job helper without kill-on-close: a timeout terminates the job, but a process `claude` leaves running after it answered (a background-session supervisor) is not Officina's to end. What `claude` starts in the instant before the assignment escapes it.
  - The fake tool gains an `orphan` mode beyond AD-37's list, and the fake claude a `tree` mode, for the two adversarial and backlog tests below.
  - A command-line argument that is not Unicode is a usage error (64) instead of the panic `std::env::args` raised; `officina hook` still exits 0 whatever follows it.
  - The tests turn each `argv.json` action into TOML with a small emitter (JSON string escapes are TOML's), so the `officina` crate takes no `toml` dev-dependency.
- **Found by the tests, fixed before commit.** With `officina run` in its own kill-on-close job, a plain exit let the job end the exiting process a second time and replace the program's exit code with 0 (`exit_code_passes_through`, `full_windows_exit_code_passes_through`, `missing_program_exits_127` all read 0). `officina run` now ends its tree with the program's code (`TerminateJobObject(job, code)`), as the timeout path does with 124. A refusal quoting a TOML parse error printed six stderr lines (`every_refusal_code_exits_126_with_one_journal_line`); details are now folded to one line in core.
- **Adversarial pass.** Tried: a program that exits at once leaving a process holding its output, eight concurrent runs on one journal, a run killed mid-way (abandoned: a start line, no end), a stop flag that is a folder or unreadable, a value carrying a shell command, a NUL (Windows cannot carry it on a command line), every refusal of the vector set, `PATH` and `SYSTEMROOT` pointing at a planted `wsl.exe`, and output past the budget with both streams. The first two are new tests (`a_process_left_behind_neither_holds_nor_outlives_the_run`, `concurrent_runs_each_journal_one_start_and_one_end`), the value one a core test (`a_refusal_never_echoes_the_value`); the others are AC tests. A mutation that removed the collector's job terminate made `a_timed_out_claude_stops_with_every_process_it_started` fail.
- **WSL leg (AC 10).** Ran on the workstation against the first installed distro (Debian): every accepted `wsl` case of `argv.json` arrived byte for byte at `/bin/bash -c 'printf "%s\0" "$@"'`. Hosted runners without a distro print why they skip.

### Completion Notes

- AC 1: `crates/officina/tests/run.rs::{token_without_equals_is_usage, duplicate_parameter_is_usage, missing_action_is_usage}`; host `relay/run.rs::tests::arguments_split_at_the_first_equals`.
- AC 2: `run.rs::{checks_run_in_the_documented_order, free_project_listing_the_action_is_served}`; core `relay/run.rs::tests::checks_fail_in_the_documented_order`.
- AC 3: `run.rs::every_refusal_code_exits_126_with_one_journal_line`, iterating `RefusalReason::ALL` through `scenario`, whose `match` is exhaustive.
- AC 4: `run.rs::stop_flag_written_by_hand_refuses`; host `relay/home.rs::tests::metadata_error_other_than_not_found_is_stopped` on `home::is_stopped`.
- AC 5: `run.rs::{free_action_runs, after_check_and_human_are_not_available_yet}`; core `classes.rs::tests::only_free_runs_in_this_build` (the second deleted and the third renamed `every_class_but_auto_runs` by story 2.8, itself deleted by story 2.15 with the unavailable-class texts).
- AC 6: `run.rs::{exit_code_passes_through, full_windows_exit_code_passes_through, missing_program_exits_127}`.
- AC 7: `run.rs::{timeout_exits_124_and_the_tree_stops, killing_run_stops_the_grandchild}`.
- AC 8: `run.rs::{output_is_capped_and_drained, hashes_cover_all_bytes}`.
- AC 9: `run.rs::every_argv_injection_is_refused_before_launch`.
- AC 10: `run.rs::{every_accepted_native_case_arrives_intact, every_accepted_wsl_case_arrives_intact}`.
- AC 11: `run.rs::{native_launch_gets_cwd_env_and_null_stdin, wsl_program_comes_from_the_system_directory}`; host `relay/run.rs::tests::wsl_command_removes_wslenv`.
- AC 12: `run.rs::outer_deadline_precedes_the_inner_timeout`.
- AC 13: `run.rs::{run_leaves_one_start_and_one_finish_line, journal_lock_timeout_refuses_before_spawn}`.
- AC 14: `run.rs::session_id_from_env_or_null`.
- AC 15: `run.rs::{list_prints_the_projects_approved_actions, list_marks_unavailable_classes, list_outside_every_project, list_reports_stop_and_pending_change}`; core `relay/run.rs::tests::list_states_what_blocks_and_marks_unavailable_classes` (`list_marks_unavailable_classes` deleted by story 2.8; the core test renamed `list_states_what_blocks_and_explains_human_actions`).
- AC 16: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` now runs `run` and `run --list`.
- Backlog Low item (a timed-out `claude` killed only as the direct child) closed: `crates/officina/tests/board.rs::a_timed_out_claude_stops_with_every_process_it_started`.
- Gates at the end of the story: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings` and with the CI feature set; `cargo test --workspace --all-features` 319 passed, 0 failed, 5 ignored (`run.rs` 31); `cargo test --release -p officina --test perf -- --ignored` 5 passed; `cargo deny check` clean (only the existing duplicate-version warnings); the shipped `officina.exe` built alone holds no authority marker and imports no `user32.dll`; each intermediate commit clippy-clean. No board type changed.

## File List

- `crates/officina-core/src/relay/{mod,run,classes}.rs`
- `crates/officina-host/Cargo.toml` (windows-sys features `Win32_System_JobObjects`, `Win32_System_SystemInformation`)
- `crates/officina-host/src/platform/windows.rs`
- `crates/officina-host/src/relay/{mod,home,run}.rs`
- `crates/officina-host/src/claude.rs`
- `crates/officina/src/main.rs`
- `crates/officina/src/bin/{officina-fake-tool,officina-fake-claude}.rs`
- `crates/officina/tests/{run,board,quota_doctor}.rs`
- `README.md`, `CHANGELOG.md`, `docs/backlog.md`

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: fake tool moved to story 2.4; D-6 (`WSLENV`, inner timeout margin), D-21 (free projects served, `--list`), argument fidelity through the fake tool and WSL, stop flag written by hand
- 2026-10-28 implemented: the checks as pure functions, `officina run` and `run --list` with the job object, output budget and hashes, the journal lines; the collector's `claude` call ends its tree on timeout (backlog item closed); adversarial pass; status Review
