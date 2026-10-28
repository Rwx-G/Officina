# Story 3.8: Hook fields in the contract canary

**Status:** review

## Story

As the author whose Claude Code updates itself without asking (E11),
I want the canary to check the hook payload fields Officina reads, per Claude Code version, next to the agents fields,
so that a field a new version stops sending shows up in the window and in doctor instead of silently degrading the registry.

## Acceptance Criteria

1. `officina-core::contract::HOOK_FIELDS` maps each buffered event to the payload fields Officina reads that are verified always present for it: E2 for four events, E17 for `Notification` (D-90); a field no research note shows observed does not enter it. Every buffered event has an entry (AD-56, D-44). Proof: host unit test `crates/officina-host/src/doctor.rs::tests::every_buffered_event_has_hook_fields`, iterating the buffered-events constant (D-18, or `HOOK_EVENTS` until story 2.4 renames it); review of each entry against E2 and E17.
2. The hook writes `absent` on event line version 1: the `HOOK_FIELDS` entries of the event missing from the payload, omitted when none is (AD-45 second bullet); v0.2's dispatch is untouched (D-44); the hook's own cost stays within the AD-17 budget. Proof: core `crates/officina-core/src/events.rs::tests::{absent_lists_the_missing_hook_fields, absent_is_omitted_when_empty}`; CLI `crates/officina/tests/hook.rs::payload_missing_a_field_records_absent` (iterating `HOOK_FIELDS`: each field removed in turn from a complete redacted payload of its event); `crates/officina/tests/perf.rs::hook_latency_p95`.
3. At ingestion (AD-56, D-71): lines carrying `absent` whose `ts` follows the current version's first sighting update that version's record in `state/contract.json` with `hook_missing` (`Event.field`); `hook_not_observed` lists the buffered events not seen since that sighting; `first_seen_at`, `hook_missing` and `hook_not_observed` default for old files. Proof: host `crates/officina-host/tests/registry_ingest.rs::{absent_after_the_first_sighting_updates_hook_missing, absent_before_the_first_sighting_is_ignored, hook_not_observed_shrinks_as_events_arrive, a_v0_2_contract_file_still_loads}`.
4. `ContractView.hook_missing` (AD-54, D-71, P-13: no second schema bump) reaches the board; the header's contract chip names the missing fields; doctor prints `hook_missing` and `hook_not_observed` for the running version (AD-56). Proof: core `crates/officina-core/src/board.rs::tests::hook_missing_reaches_the_contract_view`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; Playwright `app/e2e/board.spec.ts > the contract chip names missing hook fields`; CLI `crates/officina/tests/quota_doctor.rs::doctor_reports_hook_fields_missing_and_not_observed`.
5. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; the README's canary paragraph says hook fields are checked too and names `HOOK_FIELDS` without listing it (derived-not-transcribed rule). Proof: review.

## Tasks

- [x] `HOOK_FIELDS` in `crates/officina-core/src/contract.rs` from E2 and E17; coverage test against the buffered-events constant (AC 1)
- [x] `absent` in `events::{EventLine, reduce_payload}` (AC 2)
- [x] `ContractRecord` fields with serde defaults; ingestion updates per version (AC 3)
- [x] `ContractView.hook_missing`, header chip, doctor lines; regenerate TypeScript (AC 4)
- [x] Fixture with a missing field in `app/scripts/e2e-fixtures.mjs`; Playwright case (AC 4)
- [x] Update `CHANGELOG.md` and the README (AC 5)

## Dev Notes

- Requirements: FR-14. Decisions: AD-12, AD-45 (second bullet), AD-54, AD-56. Facts: E2, E11 (silent self-update), E17 (`Notification` in a background session in `default` mode asked to use Write). Epic decisions: D-18 (Epic 2), P-13, D-44, D-71, D-90.
- `Notification` is verified (D-90 supersedes P-12): no workstation capture is left to do. E2's print-mode capture did not cover it; E17's background-session method is how a later version is re-checked.
- The hook only lists names it reads from a constant; it never parses more of the payload than it does today, so the AD-17 latency budget is untouched. Keep the check a lookup per field.
- "First sighting" is `first_seen_at` (D-71): lines older than it describe another version and are ignored, so an upgrade does not inherit the previous version's gaps.
- `ContractView` carries `hook_missing` only, like `missing` on the agents side; `hook_not_observed` is a doctor line (D-71).
- The README and doctor name the constant, never its content (AD-12, derived-not-transcribed rule).
- After this story slice A is complete: memory, ingestion, sessions, subjects, filing, proposals, batch confirmation and the canary.

## Testing

- Core unit: `crates/officina-core/src/{events,contract,board}.rs`.
- Host unit and integration: `crates/officina-host/src/doctor.rs`, `crates/officina-host/tests/registry_ingest.rs`.
- CLI integration: `crates/officina/tests/{hook,quota_doctor}.rs`; ignored release test `perf.rs`.
- Bindings: `crates/officina-core/tests/typescript_bindings.rs`. Playwright: `app/e2e/board.spec.ts`.

## Dev Agent Record

### Debug Log

- **Decisions where the story is silent:**
  - AC 1: `HOOK_FIELDS` is a slice of (event, fields), with `hook_fields(event)` and `absent_hook_fields(event, payload)` beside it. Each entry holds the fields `reduce_payload` reads that E2 (for every event but `Notification`) or E17 (for `Notification`) shows on every capture; fields Claude Code sends that Officina does not read stay out, and so does `hook_event_name`, whose absence makes a payload no hook call at all. The coverage test also refuses an entry for an event the hook does not buffer. Review against the notes: each entry's fields appear in the verified lists of E2 (the base fields of `SessionStart` and the fields each later event adds) or of E17.
  - AC 2: `absent` lists missing keys (`payload.get(field)` is `None`), whatever `no_excerpts` says: it describes the payload, not what is kept. `to_bounded_line` never drops it; it is a few short names.
  - AC 3: ingestion folds every event line it reads into `IngestReport.hook_sightings` (`contract::HookSightings`: the latest `ts` per event and per `Event.field` absent), so a pass costs a map of names, never a copy of its lines. `collector::record_hook_fields(path, version, sightings)` folds them into that version's record; the window's collector keeps sightings until it knows the running version and that version has a record, so lines ingested at start, before `claude --version` answered, are not lost. "Follows the first sighting" reads as `ts >= first_seen_at`. A field once missing stays in `hook_missing` for that version: it was verified always present, so one absence is the break.
  - AC 3, D-71: `first_seen_at` is an `Option` read through `first_sighting()` (falls back to `checked_at`), `hook_not_observed` defaults to every buffered event, so a v0.2 file reads as "nothing learnt yet". The agents check keeps a record's hook fields when it rewrites it and stamps a new version's first sighting with its own time.
  - AC 4: `board --json` now runs `refresh_contract` (one `claude --version` spawn): D-71 has it rewrite the agents fields of `state/contract.json`, which needs the version, and the board can then show the version's `hook_missing`. Doctor prints a "hook fields" line after `claude agents --json`: `[FAIL]` with the missing `Event.field` when any is, like the agents side, and "not observed yet" with the events no line of the version carried; "not checked yet" when the version has no record. The header shows "hook fields missing: ..." in place of "contract held"; an agents break keeps its own chip.
  - AC 4, fixture: `contract-hook-missing` comes from the real path: `board --json` starts the record, `officina hook` records a `Stop` lacking a field, the `registry_fixture` example's new `hook_fields` step runs the window's ingestion and fold, and `board --json` prints the field.
- **Adversarial pass** (an upgrade inheriting the old version's gaps, a line ingested before the version is known, a record not written yet, a v0.2 contract file, an agents re-check rewriting the record, an event the hook does not buffer, `no_excerpts`, the bounded line, an old reader of the new field): two real misses, both now tested: the window's path from ingestion to the record had no end-to-end case (`crates/officina/tests/watch.rs::a_missing_hook_field_reaches_the_board`), and nothing proved an agents re-check keeps the hook fields it rewrites around (`quota_doctor.rs::doctor_reports_hook_fields_missing_and_not_observed` now rewrites the record with a new sample in between). Mutation checks: not wiring the pass's sightings to the collector turns the watch case red; resetting the hook fields in the agents check turns the doctor case red.

### Full gate at the end of story 3.8

Every job of `.github/workflows/ci.yml`, run locally with `--locked`, covering stories 3.7 and 3.8, on 1afb629: `cargo fmt --check`; frontend install (frozen lockfile) and build; clippy `--workspace --all-targets --features officina-core/ts` clean, in the `core-linux` feature set, and on the host without `window-authority`; `cargo test --workspace --features officina-core/ts` 860 passed, 0 failed, 11 ignored (TypeScript drift green); `perf.rs` alone in release 5 passed (hook own cost 9.3 ms, budget 150 ms; gate own cost 3.9 ms, budget 15 ms; in-process p95 4.3 ms); the lexer differential 2 passed; `approvals_stress` passed; the crash step 3 passed (helper built first, the confirm case included); "officina holds no authority": both markers absent from `officina.exe`, present in `officina-app.exe`; "Registry dependencies" ok; "No bundled roots" ok; `pnpm check`, `lint`, `test` 47 passed; fixtures rebuilt from the real CLI (19 scenarios, `triage` and `contract-hook-missing` new), Playwright 83 passed, and the board, subjects and triage specs 159 passed with `--repeat-each=3`; `cargo deny --locked check` ok; `pnpm audit` 1 low, ignored with its reason (unchanged). The `core-linux` job replayed in a `rust:1-bookworm` container on 1afb629 with Rust 1.99.0: fmt, clippy clean, 749 passed, 0 failed, 9 ignored. Node 25.4 locally against CI's 22.

### Completion Notes

- AC 1: `crates/officina-host/src/doctor.rs::tests::every_buffered_event_has_hook_fields`, iterating `BUFFERED_EVENTS`; the review above.
- AC 2: `crates/officina-core/src/events.rs::tests::{absent_lists_the_missing_hook_fields, absent_is_omitted_when_empty}`; `crates/officina/tests/hook.rs::payload_missing_a_field_records_absent`, iterating `HOOK_FIELDS`; `crates/officina/tests/perf.rs::hook_latency_p95` (own cost 9.3 ms against 150 ms in the full gate).
- AC 3: `crates/officina-host/tests/registry_ingest.rs::{absent_after_the_first_sighting_updates_hook_missing, absent_before_the_first_sighting_is_ignored, hook_not_observed_shrinks_as_events_arrive, a_v0_2_contract_file_still_loads}`; core `crates/officina-core/src/contract.rs::tests::{sightings_after_the_first_sighting_update_the_record, absent_fields_follow_the_entry_of_the_event}`.
- AC 4: `crates/officina-core/src/board.rs::tests::hook_missing_reaches_the_contract_view`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; `app/e2e/board.spec.ts > the contract chip names missing hook fields`; `crates/officina/tests/quota_doctor.rs::doctor_reports_hook_fields_missing_and_not_observed`; `crates/officina/tests/watch.rs::a_missing_hook_field_reaches_the_board`.
- AC 5: `CHANGELOG.md` `[Unreleased]` Added and Changed; README "How it reads your sessions" names `HOOK_FIELDS` and where it lives, without its content.

## File List

- `crates/officina-core/src/{contract,events,board}.rs`, `crates/officina-core/src/registry/fold.rs` (test literal)
- `crates/officina-host/src/{collector,doctor,watch}.rs`, `crates/officina-host/src/registry/ingest.rs`, `crates/officina-host/examples/registry_fixture.rs`, `crates/officina-host/tests/registry_ingest.rs`
- `crates/officina/src/main.rs`, `crates/officina/tests/{hook,quota_doctor,watch}.rs`
- `app/src/lib/generated/ContractView.ts`, `app/src/components/Header.svelte`, `app/scripts/e2e-fixtures.mjs`, `app/e2e/{fixtures.ts,board.spec.ts}`
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.8-hook-fields-canary.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `Notification` verified by E17, the workstation gate removed and the background-session method named for re-checks (D-90); fixture script path
- 2026-10-28 implemented: `HOOK_FIELDS` and `absent` on event line version 1, the hook sightings folded per version into `state/contract.json`, `ContractView.hook_missing`, the header chip, the doctor line, the fixture through the real path; two adversarial tests added. Status review
