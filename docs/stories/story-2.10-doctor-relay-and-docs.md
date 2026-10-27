# Story 2.10: Doctor for the relay, CLAUDE.md paragraph, documentation finalised

**Status:** Approved

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

- [ ] `crates/officina-host/src/relay/doctor.rs`: catalogue, program resolution, WSL probe, warnings, levels, CLAUDE.md paragraph (AC 1-5)
- [ ] Wire the relay checks into `doctor::run` and the `officina doctor` output (AC 1-5, 9)
- [ ] `crates/officina/tests/doctor_relay.rs`; extend the zero-write test (AC 1-6)
- [ ] `crates/officina/tests/readme.rs`; README relay section made coherent (AC 7)
- [ ] CHANGELOG consolidation (AC 8)
- [ ] Workstation runs, research notes (AC 10)

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

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 doctor part references the Credential Manager check of story 2.11
- 2026-10-27 reworked after review: documentation incremental with an automated README test (D-34), dependencies limited to stories 2.1 to 2.7, decisions D-30 to D-33, `--cd` workstation check
