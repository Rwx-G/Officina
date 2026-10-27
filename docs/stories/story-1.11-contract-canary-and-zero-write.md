# Story 1.11: Contract canary and zero-write proof

**Status:** Approved

## Story

As the author,
I want Officina to tell me when a Claude Code update breaks a field it relies on, and to prove it never writes into my Claude configuration,
so that I can trust the board after every update and run it without fear.

## Acceptance Criteria

1. At collector startup and whenever the resolved `claude` binary mtime changes, `claude --version` runs; on a version without a stored record, the canary checks the live agents sample and recent event lines with `core::contract` and stores the result in `state/contract.json` per version, atomically (FR-12, AD-4, AD-8, AD-12). Proof: host tests `crates/officina-host/tests/canary.rs::{startup_checks_and_stores_per_version, binary_change_with_new_version_rechecks, same_version_is_not_rechecked, record_is_written_atomically}` using `FAKE_CLAUDE_VERSION`.
2. `Board.contract` carries `held`, `missing` and `not_observed`; ts-rs types regenerated. Proof: core unit test `board::tests::contract_section`; CI ts-rs diff.
3. The header shows "contract held" or the missing fields (FR-8). Proof: Playwright `app/e2e/contract.spec.ts > {held fixture shows contract held, broken fixture lists missing fields}`.
4. A session whose entry lacks an always-present field classifies as unknown, never done (R9). Proof: vector cases added to `tests/vectors/classification.json` and replayed by story 1.4's test.
5. Doctor prints the same contract result as the runtime canary. Proof: CLI test `doctor.rs::contract_output_matches_stored_record`.
6. Nothing is created, modified or deleted under the Claude config dir by any `officina` subcommand or by a 5 s collector run (FR-13, AD-17 zero-write row). The subcommand list is read from the binary's own usage output, so a new subcommand is covered without editing the test. Proof: CLI test `crates/officina/tests/zero_write.rs::every_subcommand_leaves_config_dir_unchanged` and host test `crates/officina-host/tests/zero_write.rs::collector_run_leaves_config_dir_unchanged`, both on a temp config dir seeded with a settings file and a sessions directory.
7. On the workstation, the fingerprint of the real config dir is taken before and after a one-day run of the app, excluding files Claude Code itself writes; any Officina-attributable change is a defect. Proof: manual, recorded in research notes (PRD section 7 "doctor on the real workstation").

## Tasks

- [ ] `officina_host::canary` and the contract state store (AC 1)
- [ ] Hook the canary into the collector startup and the binary mtime trigger (AC 1)
- [ ] `Board.contract`, ts-rs regeneration, header display (AC 2, 3)
- [ ] Classification vectors for missing fields (AC 4)
- [ ] Doctor reads or computes the same result (AC 5)
- [ ] Zero-write CLI and host tests; usage output lists subcommands in a parseable form (AC 6)
- [ ] Workstation run (AC 7)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Requirements: FR-8 (contract status), FR-12 (as amended, item 6), FR-13. Decisions: AD-4 (binary trigger), AD-8 (state file), AD-10 (atomic write), AD-12 (constants and check), AD-17 (zero-write). Risk: R9.
- FR-12 says the canary checks "the last hook payloads", but the buffer stores reduced lines (FR-3, AD-8), not payloads. The canary therefore checks hook fields as they appear in event lines, and with `OFFICINA_NO_EXCERPTS=1` the excerpt fields are excluded from expectation. This is an interpretation, listed as an open point for the owner.
- The manual workstation run cannot isolate Claude Code's own writes from Officina's by fingerprint alone; use Process Monitor filtered on Officina's process names for that day.

## Testing

- Host integration: `crates/officina-host/tests/{canary,zero_write}.rs`.
- CLI integration: `crates/officina/tests/{zero_write,doctor}.rs`.
- Core unit and vectors: `board.rs`, `tests/vectors/classification.json`.
- Playwright: `app/e2e/contract.spec.ts`.
- Manual: workstation fingerprint and Process Monitor.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
