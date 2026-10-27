# Story 3.8: Hook fields in the contract canary

**Status:** Approved

## Story

As the author whose Claude Code updates itself without asking (E11),
I want the canary to check the hook payload fields Officina reads, per Claude Code version, next to the agents fields,
so that a field a new version stops sending shows up in the window and in doctor instead of silently degrading the registry.

## Acceptance Criteria

1. **Gate (P-12):** a real `Notification` payload is captured on the workstation (logging hook through `claude -p --settings`, as E2 did) and its fields recorded in `docs/research-notes.md` as an addition to E2, before this story closes; a field not observed there does not enter `HOOK_FIELDS`. Proof: manual, research notes entry with the Claude Code version.
2. `officina-core::contract::HOOK_FIELDS` maps each buffered event to the payload fields Officina reads that E2 (with AC 1) shows always present for it; every buffered event has an entry (AD-56, D-44). Proof: host unit test `crates/officina-host/src/doctor.rs::tests::every_buffered_event_has_hook_fields`, iterating the buffered-events constant (D-18, or `HOOK_EVENTS` until story 2.4 renames it).
3. The hook writes `absent` on event line version 1: the `HOOK_FIELDS` entries of the event missing from the payload, omitted when none is (AD-45 second bullet); v0.2's dispatch is untouched (D-44); the hook's own cost stays within the AD-17 budget. Proof: core `crates/officina-core/src/events.rs::tests::{absent_lists_the_missing_hook_fields, absent_is_omitted_when_empty}`; CLI `crates/officina/tests/hook.rs::payload_missing_a_field_records_absent` (iterating `HOOK_FIELDS`: each field removed in turn from a complete redacted payload); `crates/officina/tests/perf.rs::hook_latency_p95`.
4. At ingestion (AD-56, D-71): lines carrying `absent` whose `ts` follows the current version's first sighting update that version's record in `state/contract.json` with `hook_missing` (`Event.field`); `hook_not_observed` lists the buffered events not seen since that sighting; `first_seen_at`, `hook_missing` and `hook_not_observed` default for old files. Proof: host `crates/officina-host/tests/registry_ingest.rs::{absent_after_the_first_sighting_updates_hook_missing, absent_before_the_first_sighting_is_ignored, hook_not_observed_shrinks_as_events_arrive, a_v0_2_contract_file_still_loads}`.
5. `ContractView.hook_missing` (AD-54, D-71, P-13: no second schema bump) reaches the board; the header's contract chip names the missing fields; doctor prints `hook_missing` and `hook_not_observed` for the running version (AD-56). Proof: core `crates/officina-core/src/board.rs::tests::hook_missing_reaches_the_contract_view`; `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; Playwright `app/e2e/board.spec.ts > the contract chip names missing hook fields`; CLI `crates/officina/tests/quota_doctor.rs::doctor_reports_hook_fields_missing_and_not_observed`.
6. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; the README's canary paragraph says hook fields are checked too and names `HOOK_FIELDS` without listing it (derived-not-transcribed rule). Proof: review.

## Tasks

- [ ] Workstation capture of a `Notification` payload; research notes (AC 1)
- [ ] `HOOK_FIELDS` in `crates/officina-core/src/contract.rs`; coverage test against the buffered-events constant (AC 2)
- [ ] `absent` in `events::{EventLine, reduce_payload}` (AC 3)
- [ ] `ContractRecord` fields with serde defaults; ingestion updates per version (AC 4)
- [ ] `ContractView.hook_missing`, header chip, doctor lines; regenerate TypeScript (AC 5)
- [ ] Fixture with a missing field in `scripts/e2e-fixtures.mjs`; Playwright case (AC 5)
- [ ] Update `CHANGELOG.md` and the README (AC 6)

## Dev Notes

- Requirements: FR-14. Decisions: AD-12, AD-45 (second bullet), AD-54, AD-56. Facts: E2, E11 (silent self-update). Epic decisions: D-18 (Epic 2), P-12, P-13, D-44, D-71.
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
- Manual: research notes (AC 1).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
