# Story 2.4: Gate in the hook, journal, doctor block

**Status:** Approved

## Story

As the author whose guarded repositories run in bypass mode,
I want `officina hook` to answer `PreToolUse` for `Bash` with a deny Claude Code honours in every mode, and every deny written to a journal,
so that a raw guarded command is refused with the list of actions the session may use, and I can see each refusal afterwards.

## Acceptance Criteria

1. `officina hook` parses stdin to `serde_json::Value` once and branches on `hook_event_name`: `PreToolUse` goes to the gate and is never appended to the event buffer; every other event takes the v0.1 path unchanged (AD-26). Proof: CLI tests `crates/officina/tests/gate.rs::{pre_tool_use_is_never_appended, v0_1_events_are_still_appended}`; `crates/officina/tests/hook.rs::appends_one_line_per_event_silently` iterates the buffered-events constant (D-18), not the registrations.
2. On a deny the hook prints the shape verified in E15 (`hookSpecificOutput` with `hookEventName`, `permissionDecision: "deny"`, `permissionDecisionReason`) on stdout and exits 0; on no decision it prints nothing and exits 0; it never exits 2 and never asks (FR-6, E12, E15). Proof: `gate.rs::{guarded_raw_command_is_denied_with_the_verified_shape, run_call_gets_no_decision, free_project_gets_no_decision, undeclared_folder_gets_no_decision, non_bash_tool_gets_no_decision}`, driven by hand-written, redacted payloads under `crates/officina/tests/data/pretooluse/` carrying exactly E15's field set (backslash `cwd`).
3. Containment (PRD section 7): every program of the default guarded list, in command position, from a guarded project, is denied through the real binary. Proof: `gate.rs::every_default_guarded_program_is_denied`, iterating the core constant.
4. Project scope (D-16): a call is guarded if the project of `CLAUDE_PROJECT_DIR` or the project of the payload `cwd` is guarded, so `cd ..` out of a guarded repository does not leave containment, and a `cwd` inside a guarded repository is guarded whatever the session's root. Proof: `gate.rs::{cd_out_of_a_guarded_project_stays_guarded, cwd_inside_a_guarded_project_is_guarded_from_a_free_root, no_project_dir_falls_back_to_cwd}`.
5. Union of sources (D-14): the effective `guarded_programs` is the union of the current floor's list (phase 1, defaults applied) and the approved snapshot's list; a project is guarded if either says so. Proof: `gate.rs::{emptying_guarded_programs_does_not_lift_containment, dropping_a_program_from_the_floor_does_not_lift_it, setting_free_in_the_floor_does_not_lift_containment}`.
6. Broken snapshot (D-15): with `approved.json` unreadable or corrupt, or missing while `relay/known-projects.json` records an approved hash, every project of the current floor is guarded with the default list plus the floor's list; with the floor failing phase 1 too, guarded programs are denied when `CLAUDE_PROJECT_DIR` or `cwd` lies inside a project path of `known-projects.json`, else no decision. Each case journals `gate_error` with its cause. Proof: `gate.rs::{corrupt_approved_json_guards_every_floor_project, missing_approved_after_an_approval_guards_every_floor_project, broken_floor_and_snapshot_fall_back_to_known_projects, nothing_readable_gives_no_decision}`.
7. Fail open on internal faults only: a panic or an IO error inside the gate gives no decision, exit 0, and a `gate_error` line when the journal is reachable (AD-28). Proof: unit tests `crates/officina-host/src/relay/gate.rs::tests::{panic_gives_no_decision_and_records_gate_error, io_error_gives_no_decision}` through an injected decision closure; no CLI test uses a file the session can write to provoke fail-open.
8. The deny reason follows D-17: actions from the approved snapshot, or the floor entry's action names with "catalogue awaits approval", unavailable classes marked; `protected_dirs` per D-13 from the resolved home. Proof: `gate.rs::{reason_lists_approved_actions, reason_without_approval_names_floor_actions, relay_dir_in_a_command_is_denied}`.
9. Journal append per AD-32: core line types (`v`, `ts`, `kind` and the fields of the AD-32 table); host `relay::journal::append` builds the line, takes `journal/.lock` with `File::try_lock`, retries every 5 ms up to 2 s, writes once through `files::append_line`, unlocks; `read_recent` reads two UTC days and counts unparsable lines. Proof: host tests `crates/officina-host/tests/relay_journal.rs::{append_writes_one_line_under_the_lock_file, lock_held_past_2_s_times_out, reader_spans_two_days_and_counts_corrupt, unknown_kind_is_not_corrupt}`.
10. Each deny leaves exactly one `gate_denied` line with project, session id, matched program and `command_excerpt` (280 characters), the excerpt dropped when `OFFICINA_NO_EXCERPTS` or the floor's `no_excerpts` is set; a `gate_error` takes the same lock (P-4); a held lock does not change the decision (AD-32). Proof: `gate.rs::{deny_is_journaled_once, excerpt_dropped_when_excerpts_are_off, deny_stands_when_the_journal_lock_is_held}`.
11. `authority::approve_catalogue` journals exactly one `catalogue_approved` line with `hash` and `previous_hash` (AD-32). Proof: `crates/officina-host/tests/authority.rs::approval_is_journaled_once_with_previous_hash`.
12. Test-only binary `officina-fake-tool` (AD-37), created here and used from this story on: `FAKE_TOOL_LOG` appends argv, cwd and environment keys (launch counter); `FAKE_TOOL_MODE` = `exit:<n>`, `bytes:<n>`, `sleep:<s>`, `tree` (spawns a grandchild writing a heartbeat every 100 ms). It is never shipped (`test = false`, explicit file list in `release.yml`). Proof: `crates/officina/tests/fake_tool.rs::{logs_argv_cwd_and_env_keys, each_mode_behaves}`.
13. The gate runs phase 1 only and spawns nothing: a floor whose action pattern would not compile gets the same decision as without it, and the fake tool's launch log stays absent while the floor points every action at it. Proof: `gate.rs::{invalid_pattern_does_not_affect_the_decision, gate_spawns_nothing}`.
14. Doctor (D-18): `HOOK_EVENTS` splits into the buffered-events constant and (event, matcher) registrations; `PreToolUse` with `"matcher": "Bash"` is required, and printed in the block, only when the relay floor has at least one guarded project; `missing_hook_events` checks the matcher. Proof: unit tests `crates/officina-host/src/doctor.rs::tests::{block_includes_pre_tool_use_only_with_a_guarded_project, pre_tool_use_without_the_matcher_is_missing}` and CLI tests `crates/officina/tests/quota_doctor.rs::{doctor_passes_once_the_block_is_merged, doctor_with_a_guarded_project_requires_pre_tool_use}` (paste-back of the printed block).
15. Gate budget (D-19): own cost (gate p95 minus empty-process p95, 200 runs, 64 KiB command, 50-action floor) under 5 ms on every machine; the in-process p95 under 3 ms asserted only when `OFFICINA_PERF_WORKSTATION=1`, printed otherwise. Proof: `#[ignore]` tests `crates/officina/tests/perf.rs::{gate_overhead_over_empty_process, gate_in_process_p95}`, run by the CI release step.
16. Journal stress: 16 processes x 200 lines over 4096 bytes each, zero corrupt lines (AD-37). Proof: `#[ignore]` test `perf.rs::journal_appends_never_interleave`.
17. Zero write: a gate deny joins the fingerprint test of the Claude config dir. Proof: `quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
18. A deny inside an interactive session in bypass mode (E12 and E15 were `claude -p`) is recorded in `docs/research-notes.md`. Proof: manual, per AD-37.

## Tasks

- [ ] `crates/officina-core/src/relay/journal.rs`: line types per AD-32 (AC 9)
- [ ] `crates/officina-host/src/relay/journal.rs`: locked append, two-day reader with corrupt count (AC 9)
- [ ] `crates/officina-host/src/relay/gate.rs`: payload to decision, `CLAUDE_PROJECT_DIR` and `cwd`, relay floor resolution (story 2.2), phase 1 parse, snapshot states, union of sources, `known-projects.json` fallback, protected dirs, injected-closure guard, journal of denies and errors (AC 2-8, 10, 13)
- [ ] `crates/officina/src/main.rs` hook: one parse, branch on `hook_event_name` (AC 1)
- [ ] `approve_catalogue` journals `catalogue_approved` (AC 11)
- [ ] `crates/officina/src/bin/officina-fake-tool.rs` (`test = false`, `doc = false`) and `fake_tool.rs` (AC 12)
- [ ] `doctor.rs`: buffered-events constant, (event, matcher) registrations, conditional `PreToolUse`; update `hook.rs` to the buffered constant (AC 1, 14)
- [ ] Payload fixtures and `crates/officina/tests/gate.rs`; perf and stress tests in `perf.rs`; extend the zero-write test (AC 2-8, 10, 13, 15-17)
- [ ] Manual check recorded in research notes (AC 18)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (gate, journal) (D-34)

## Dev Notes

- Requirements: FR-6 as amended, FR-13 (gate denials journaled, amendment 10), PRD section 7 containment row. Decisions: AD-16 (hook shape, exit 0 on every path), AD-26, AD-28, AD-32. Facts: E9 (about 40 ms per event through Claude Code's shell, hence `Bash` only), E12 (deny holds in bypass, `ask` does not), E15 (payload fields, `cwd` follows `cd`, `CLAUDE_PROJECT_DIR` stays the root, deny shape). Epic decisions: P-4, D-13 to D-19.
- Why D-14 to D-16: the gate must never become looser through anything a session can write without the author's click. The floor, `approved.json` and the shell's `cwd` are all within reach of a session; the union and the "broken means stricter" rule keep each of them from lifting containment alone. A session that corrupts every file and the known-projects record together gets no decision: accepted, the relay is not a boundary (PRD section 1).
- `CLAUDE_PROJECT_DIR` is read from the hook's environment (E15), not the payload; normalise both it and `cwd` with `gate::normalise_cwd`.
- `known-projects.json` is written by the window (story 2.2 on approval, story 2.7 on every valid read); the gate only reads it, and only when the floor and the snapshot both fail.
- The floor's `no_excerpts` does not reach the v0.1 hook (backlog, Medium). The gate reads the floor anyway, so it honours both switches for `command_excerpt`; the v0.1 event path is unchanged here.
- Panics: the existing `catch_unwind` around the hook body stays; the gate's guard returns "no decision" and records `gate_error` when it can.
- The gate must not resolve `claude`, start the collector or touch `user32.dll` (story 1.3 note on hook imports).
- Keep `missing_hook_events` tolerant of other hooks in the same settings file (E2: hooks merge). D-18 keeps the 40 ms per `Bash` call (E9) off workstations with no guarded project.

## Testing

- Host unit: `crates/officina-host/src/{doctor,relay/gate}.rs`.
- Host integration: `crates/officina-host/tests/{relay_journal,authority}.rs`.
- CLI integration: `crates/officina/tests/{gate,hook,fake_tool,quota_doctor}.rs`.
- Ignored release tests: `crates/officina/tests/perf.rs`, CI `windows` job.
- Manual: research notes (AC 18).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: D-14 (union of sources), D-15 (broken snapshot is stricter, known-projects fallback), D-16 (`CLAUDE_PROJECT_DIR` or `cwd`, E15), D-18 (conditional `PreToolUse`, buffered constant), D-19 (relative budget); fake tool moved here from story 2.5
