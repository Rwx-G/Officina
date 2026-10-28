# Story 2.10: Doctor for the relay, CLAUDE.md paragraph, documentation finalised

**Status:** Review

## Story

As the author setting up guarded repositories on a workstation,
I want `officina doctor` to check the relay end to end and print, per guarded project, the paragraph that tells its sessions how to use `officina run`,
so that I can wire the relay without guessing, and the repository explains v0.2 to a new reader.

## Acceptance Criteria

1. Doctor reports the catalogue: issues with their lines, the approval state (approved, change pending, nothing approved) and the guarded projects with their actions (FR-14, AD-23). Proof: CLI tests `crates/officina/tests/doctor_relay.rs::{issues_are_reported_with_lines, pending_change_is_reported, guarded_projects_are_listed}`.
2. Doctor checks that each action's program resolves: a `native` program is an existing file; for `wsl` actions, `wsl.exe` from the system directory exists, and in the action's distro `/usr/bin/env`, `/usr/bin/timeout` (D-6) and the program are executable (FR-14, R14). The WSL probe takes the `wsl.exe` path as an argument so tests point it at the fake tool. Proof: host unit tests `crates/officina-host/src/relay/doctor.rs::tests::{wsl_probe_reports_each_missing_tool, wsl_probe_uses_exec_and_the_distro}` and CLI test `doctor_relay.rs::missing_native_program_fails`.
3. Doctor warns per D-32: an action's `timeout_s` plus the maximum `--wait` of a `human` action beyond the Bash tool's cap (P-6, AD-31; both maxima are core constants, so the check does not depend on story 2.8), an `after-check` project without a tree binding (D-22), a relay floor that differs from the floor doctor resolves (D-30), unreadable journal lines and gate errors in the last 24 h (AD-28, AD-32). Proof: `doctor_relay.rs::{long_timeout_warns, after_check_without_tree_binding_warns, relay_floor_mismatch_warns, gate_errors_are_reported}`.
4. Levels per D-31: a missing `PreToolUse` registration while a project is guarded (D-18) and a program that does not resolve are failures (exit 1); catalogue issues, a pending change and the warnings of AC 3 are warnings (v0.1 PRD FR-11 exit rule: 1 when any check fails). Proof: `doctor_relay.rs::{missing_gate_with_a_guarded_project_exits_1, warnings_alone_exit_0}`.
5. For each guarded project doctor prints, never writes, a `CLAUDE.md` paragraph: use `officina run <action> name=value ...` for the listed actions, `officina run --list` to see them, pass a 600000 ms Bash timeout for relay calls, and read a refusal's reason before retrying (FR-14, AD-31). The action list is generated from the approved catalogue; with nothing approved the paragraph says the catalogue awaits approval in the window and lists no action (D-33). Proof: `doctor_relay.rs::{claude_md_paragraph_lists_the_projects_actions, paragraph_follows_the_catalogue, paragraph_without_approval_lists_nothing}` (adding an action to the fixture floor and approving it changes the paragraph with no test edit).
6. Doctor writes nothing under the Claude config dir nor in any project folder. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` and `doctor_relay.rs::doctor_writes_nothing_in_project_folders` (fingerprint of the temp project folders).
7. Documentation finalised (D-34): the README sections each story added are made coherent into one relay section (what the relay is and is not: a guardrail and a record, not a security boundary, PRD section 1; free and guarded repositories; the schema 2 floor example with placeholder names and no `guarded_programs` line; setup through `officina doctor`; the gate's limits by pointing at AD-27; what Slice A gives when later stories are not merged yet). It transcribes no list owned by code. Proof: automated test `crates/officina/tests/readme.rs::readme_mentions_every_subcommand`, which takes the subcommands from `officina --help` (the usage constant) and asserts each appears in `README.md`, plus `readme.rs::readme_lists_no_default_guarded_program`, which iterates the core default guarded list and asserts none appears in a README code span; the prose itself is reviewed by hand.
8. `CHANGELOG.md` `[Unreleased]` holds one coherent v0.2 set of entries under Added, Changed and Security, built from the entries each story added. Proof: review.
9. Doctor's Credential Manager check (FR-14) is part of this doctor, specified and built by story 2.11 AC 13 on top of the relay checks of this story, which leave a place for it in the relay section of the output and in the level rules of AC 4 (D-31). Proof: the tests named in story 2.11 AC 13.
10. Workstation runs recorded in `docs/research-notes.md`: a WSL `ansible-playbook` check and apply on a lab group through `officina run`, including `--cd` on a project path with accented or mixed-case letters (D-6), and PRD journey 2 ("Delegation" metric) run start to finish without the author pasting anything (AD-37 manual list, PRD section 7). Proof: manual, results with Claude Code and WSL versions in research notes.

## Tasks

- [x] `crates/officina-host/src/relay/doctor.rs`: catalogue, program resolution, WSL probe, warnings, levels, CLAUDE.md paragraph (AC 1-5)
- [x] Wire the relay checks into `doctor::run` and the `officina doctor` output (AC 1-5, 9)
- [x] `crates/officina/tests/doctor_relay.rs`; extend the zero-write test (AC 1-6)
- [x] `crates/officina/tests/readme.rs`; README relay section made coherent (AC 7)
- [x] CHANGELOG consolidation (AC 8)
- [ ] Workstation runs, research notes (AC 10): the `--cd` part is recorded; the WSL `ansible-playbook` check and apply on a lab group and PRD journey 2 are open, the owner's to run (backlog, High)

## Dev Notes

- Requirements: FR-14 (Slice A per amendment 1), PRD journeys 1 and 2, section 7 "Delegation". Decisions: AD-24, AD-28, AD-31, AD-33, AD-37 (manual list). Risks: R14, R16 (`ANSIBLE_CONFIG` through `env`). Epic decisions: P-6, D-6, D-18, D-22, D-30 to D-34.
- Dependencies: stories 2.1 to 2.7 only (D-34), so this story can run right after 2.7 and complete Slice A. Stories 2.8, 2.9 and 2.11 keep the README and the changelog current themselves and must keep `readme.rs` green.
- FR-14's "HTTP credentials exist in the Credential Manager" is built by story 2.11 (AD-36, P-7), which extends this doctor; AC 9 holds the reference. The README's HTTP part is written by story 2.11 too.
- D-30: doctor cannot see the window's `--floor`; it compares its own resolved floor with the relay floor resolution.
- The WSL probe runs `wsl.exe [-d <distro>] --exec /usr/bin/test -x <path>` per path, with `CREATE_NO_WINDOW` and the AD-6 style deadline; never through a shell.
- Cutting `[0.2.0]` and bumping the workspace version belong to the release of v0.2, after story 1.13 has cut `[0.1.0]`; not in this story.
- No employer content in the README or the paragraph's example: placeholder projects, groups and paths only (officina-project rule).

## Testing

- Host unit: `crates/officina-host/src/relay/doctor.rs`.
- CLI integration: `crates/officina/tests/{doctor_relay,quota_doctor,readme}.rs`.
- Manual: README prose review, workstation runs in research notes.

## Dev Agent Record

### Debug Log

- **Decisions where the documents are silent**, each the reading that fails closed or matches the architecture:
  - The relay checks live in `officina-host::relay::doctor` and run after the v0.1 checks of `doctor::run`, only while the relay is in use: an approved snapshot in any state, a known-projects record, or a relay floor that declares an action or a guarded project (`catalogue::relay_in_use`, the rule of story 2.7). A v0.1 workstation gets no relay line.
  - Output order: relay floor, relative `OFFICINA_FLOOR`, floor mismatch, catalogue, issues, approval, projects, programs and WSL tools, timeouts, after-check listing, tree binding, journal, then one `CLAUDE.md` paragraph per guarded project. Story 2.11 (AC 9, D-31) inserts its Credential Manager check right after the program checks, as a failure for a missing credential.
  - The checks read the current relay floor's valid catalogue (what the window would approve, and what runs once approved); the paragraph reads the approved snapshot (D-33). A relay floor that is missing or fails phase 1 while the relay is in use fails, like the board's floor check: `officina run` refuses everything and the gate guards every known project until it reads.
  - Programs are checked for every valid action. A `native` program must be a file (`Path::is_file`). WSL actions are grouped by distro; per distro `/usr/bin/env`, `/usr/bin/timeout` and each program are probed with `wsl.exe [-d <distro>] --exec /usr/bin/test -x <path>`, stdio null, `CREATE_NO_WINDOW`, `WSLENV` removed as `officina run` removes it, a 20 s deadline per call (a distro's cold start). Exit 0 is executable, 1 missing or not executable; anything else (`wsl.exe` exits -1 for an unknown distro, verified), a timeout or a failed start fails the probe, and the distro's remaining paths take that failure without another call. A probe that cannot answer fails the action: doctor cannot say it resolves.
  - The WSL probe's process runner is injected (`probe_wsl_with`): the host crate's unit tests cannot reach the CLI crate's fake tool, so they record the commands and choose the exit codes; `wsl_probe_on_an_installed_distro` runs the real probe on the workstation's first distro and skips with a reason elsewhere.
  - Timeout warning (D-32, P-6): a run whose `timeout_s`, plus the longest `--wait` (core `classes::WAIT_RANGE_S`) for a `human` action, reaches the Bash tool's cap (core `run::BASH_TOOL_MAX_TIMEOUT_MS`) warns; reaching it counts, since Officina's own start adds to the run. Both maxima are core constants, as the AC asks; story 2.8 reads the wait range from the same constant.
  - Floor mismatch (D-30): doctor's floor is `--floor`, `OFFICINA_FLOOR` or the home's `floor.toml` (AD-7), the default when it has none; it is compared absolute and normalised with the relay floor.
  - Gate errors are counted over the last 24 h by cause, with the last one's detail; unreadable lines are counted over the two files read (today and yesterday, UTC), which the line says.
  - The `CLAUDE.md` paragraph goes to the check's paste block. It is printed for every project the floor or the snapshot guards; it lists the snapshot's actions for the project with their parameters, an `after-check` action's check and a class not available in this build; with no approved entry for the project it says the catalogue awaits approval and lists none. The Bash timeout in it comes from the core constant.
  - The catalogue line prints the guarded programs in effect, so the README can point at doctor instead of transcribing the default list.
  - `doctor_relay.rs` is Windows only, like `run.rs`: the resolution check needs a `native` program that exists, and the fake tool is one only on Windows.
  - CHANGELOG: `[0.1.0]` is story 1.13's cut, so the v0.1 entries stay first and untouched in `[Unreleased]`; the v0.2 entries follow as one set under Added, Changed and Security. The lexer speed-up entry is folded into the gate entry (it never shipped on its own).
  - README: the default guarded list is described by families and pointed at (`DEFAULT_GUARDED_PROGRAMS`, printed by doctor); the gate's limits point at AD-27 and the vector set. `readme_lists_no_default_guarded_program` checks inline code spans (an entry, or its stem for a `*` entry, as a whole span) and refuses a `guarded_programs` line in any fenced example; the runner is written `runner = "wsl"` or "WSL action" so the runner and the program are not confused.
- **Interrupted once by the coordinator** for a CI failure on main, in its own commit: `perf.rs::gate_overhead_over_empty_process` reads its bound from `OFFICINA_GATE_OWN_COST_BUDGET_MS` (default 5 ms, D-19's workstation figure), which `ci.yml` sets to 15 on the hosted runner; D-19 says so.
- **Adversarial pass.** Tried: a program that is a folder, a WSL action with no `wsl.exe` and with no system directory, an unknown distro, a probe that times out, each D-6 tool missing in turn, a relative `--floor` and a relative `OFFICINA_FLOOR`, doctor and the relay on different floors, a snapshot gone with the record naming a hash, a broken relay floor, a description written over several lines, a pending change that adds an action (not in the paragraph until approved), git leaving the index untouched in a repository with a tracked edit and an untracked file, and a v0.1 floor. Two real misses became tests: a relative `OFFICINA_FLOOR` resolves per process (story 2.2's open item: `doctor_relay.rs::a_relative_floor_override_warns`), and a multi-line description broke the pasted list (`relay/doctor.rs::tests::a_multiline_description_stays_on_its_line`). Mutations that read a `test -x` exit 1 as executable, and that built the paragraph from the floor instead of the snapshot, each turned tests red.
- **Workstation run (AC 10).** Through `officina run`, a WSL action of a project whose folder holds accented and mixed-case letters and spaces started in that folder, and a value with accents and a space arrived as one argument; doctor's probe read the real Debian distro. Recorded in `docs/research-notes.md` (E13) with the WSL and Claude Code versions. Not run: the `ansible-playbook` check and apply on a lab group and PRD journey 2, which need the author's lab and work sessions; listed in `docs/backlog.md` (High).

### Completion Notes

- AC 1: `crates/officina/tests/doctor_relay.rs::{issues_are_reported_with_lines, pending_change_is_reported, guarded_projects_are_listed}`, plus `a_workstation_without_the_relay_gets_no_relay_line`.
- AC 2: host `crates/officina-host/src/relay/doctor.rs::tests::{wsl_probe_reports_each_missing_tool, wsl_probe_uses_exec_and_the_distro}`, plus `a_failed_probe_stops_probing_the_distro`, `without_wsl_exe_every_wsl_action_fails`, `probe_runs_report_exit_codes_and_deadlines`, `wsl_probe_on_an_installed_distro`; CLI `doctor_relay.rs::missing_native_program_fails`.
- AC 3: `doctor_relay.rs::{long_timeout_warns, after_check_without_tree_binding_warns, relay_floor_mismatch_warns, gate_errors_are_reported}`, plus `after_check_without_its_check_in_the_project_warns` (story 2.6's known limit) and `a_relative_floor_override_warns`; host `tests::timeouts_reaching_the_bash_cap_warn`.
- AC 4: `doctor_relay.rs::{missing_gate_with_a_guarded_project_exits_1, warnings_alone_exit_0}`.
- AC 5: `doctor_relay.rs::{claude_md_paragraph_lists_the_projects_actions, paragraph_follows_the_catalogue, paragraph_without_approval_lists_nothing}` (the expected calls are read from the approved snapshot); host `tests::{paragraph_without_the_project_approved_lists_no_action, a_multiline_description_stays_on_its_line}`.
- AC 6: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` (doctor now also runs on an approved relay) and `doctor_relay.rs::doctor_writes_nothing_in_project_folders` (a git work tree with a tracked edit and an untracked file, `git` on doctor's `PATH`, the tree check asserted to have run).
- AC 7: `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_lists_no_default_guarded_program}`, both red on the README before the rewrite (`officina version` missing, a `wsl` code span); the prose is for review.
- AC 8: `CHANGELOG.md` `[Unreleased]`, for review.
- AC 9: the place for story 2.11's credential check is after the program checks (Debug Log).
- AC 10: partly; see Debug Log and `docs/backlog.md`.
- Gates at the end of the story: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --features officina-core/ts -- -D warnings` and the Linux job's crate set; `cargo test --workspace --features officina-core/ts` 414 passed, 0 failed, 5 ignored; `cargo deny check` ok. No board type, hook path or app file changed (no TypeScript regeneration, no perf or frontend gate affected).

## File List

- `crates/officina-core/src/relay/{classes,run}.rs` (`WAIT_RANGE_S`, `BASH_TOOL_MAX_TIMEOUT_MS`)
- `crates/officina-host/src/relay/{doctor,home,mod}.rs`, `crates/officina-host/src/doctor.rs`
- `crates/officina/tests/{doctor_relay,readme,quota_doctor}.rs`, `crates/officina/tests/common/mod.rs`
- `README.md`, `CHANGELOG.md`
- `docs/research-notes.md`, `docs/backlog.md`

## Change Log

- 2026-10-27 created
- 2026-10-27 doctor part references the Credential Manager check of story 2.11
- 2026-10-27 reworked after review: documentation incremental with an automated README test (D-34), dependencies limited to stories 2.1 to 2.7, decisions D-30 to D-33, `--cd` workstation check
- 2026-10-28 implemented: doctor's relay checks and the `CLAUDE.md` paragraph, the README relay section and its test, the v0.2 changelog set, the `--cd` workstation run; adversarial pass (relative `OFFICINA_FLOOR`, multi-line descriptions); status Review, AC 10 partly open (lab group and journey 2)
