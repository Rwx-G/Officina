# Story 1.8: Doctor, slice A

**Status:** Approved

## Story

As the author setting up a workstation,
I want `officina doctor` to tell me what works, what is missing, and print the exact hooks block to paste,
so that I can wire Officina in minutes without it ever writing into my Claude Code configuration.

## Acceptance Criteria

1. Doctor reports whether `claude` is found and its version. Proof: CLI tests `crates/officina/tests/doctor.rs::{reports_claude_version, missing_claude_is_red_and_exits_1}`.
2. Doctor runs `claude agents --json`, reports parse success and the contract result of `core::contract` (FR-11, AD-12). Proof: CLI tests `doctor.rs::{agents_fixture_holds_contract, fixture_missing_an_always_field_is_red, garbage_output_is_red}`.
3. Doctor compares, read-only, the hooks in user settings (`<config dir>/settings.json`) with the expected block; when absent or different it prints the block of AD-16 (absolute `current_exe()` path, forward slashes, quoted, every event of the hook event constant, timeout, synchronous). Proof: CLI tests `doctor.rs::{no_settings_prints_block_and_exits_1, block_printed_is_valid_json, settings_containing_the_printed_block_is_green}` (the last test pastes the printed block into a temp settings file and reruns doctor, so the comparison and the printer cannot drift).
4. Doctor checks that the Officina home is writable by creating and removing a probe file under it, never under the Claude config dir. Proof: CLI test `doctor.rs::home_under_a_file_is_red`.
5. Doctor reports whether a hook event was seen in the last 24 h (spine section 3 item 10). Proof: CLI tests `doctor.rs::{recent_event_reports_hook_seen, no_event_reports_hook_not_seen}`.
6. Doctor prints the focus success rate over the last 10 working days using the metric of story 1.7. Proof: CLI test `doctor.rs::prints_focus_rate_from_seeded_log`.
7. Exit code is 0 when every required check is green, 1 otherwise (FR-11). In slice A the required checks are those of AC 1-4; AC 5-6 are informative. Proof: CLI tests `doctor.rs::{all_green_exits_0, any_required_red_exits_1, informative_red_still_exits_0}`.
8. Doctor writes nothing under the Claude config dir. Proof: each `doctor.rs` test asserts the harness `fingerprint` of the temp config dir is unchanged (story 1.11 adds the cross-command test).

## Tasks

- [ ] `officina_host::doctor` with a check list returning typed results (name, required, status, detail) (AC 1-7)
- [ ] Hooks block builder in host from the core hook event constant and `current_exe()`; read-only comparison with user settings JSON (AC 3)
- [ ] Probe write under Officina home (AC 4)
- [ ] Hook-seen and focus-rate checks (AC 5, 6)
- [ ] `officina doctor` subcommand, human-readable output, exit code (AC 7)
- [ ] CLI tests with fingerprint assertions (AC 1-8)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Requirements: FR-11 (slice A part), journey 4. Decisions: AD-12 (contract), AD-16 (hooks block shape), spine section 3 item 10. Facts: E2 (hook merge across settings levels, exit codes, `timeout`).
- Officina never writes Claude Code configuration (project non-negotiable). Doctor prints; the author pastes.
- Quota freshness, statusline snippet and floor validation are added by stories 1.9 and 1.10; contract display at runtime by story 1.11.
- "Working day" for the focus rate is not defined by the PRD or the spine. Use calendar days with at least one click and state that choice in the doc comment; it is listed as an open point in the epic summary.
- The comparison must tolerate other hooks in the same settings file (E2: hooks merge): green means "Officina's handlers are present for every event", not "the hooks section equals the block".

## Testing

- CLI integration: `crates/officina/tests/doctor.rs`.
- Host unit tests for the block builder and comparison in `crates/officina-host/src/doctor.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
