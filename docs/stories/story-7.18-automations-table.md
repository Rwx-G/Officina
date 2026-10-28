# Story 7.18: The Automations table

**Status:** Approved

## Story

As the author who now has several things that act without me (the triage model, auto-archive, auto approval, the governor, each standing order),
I want one read-only table, in the Governor section and the briefing, that says for each what state it is in, when it last changed and the cause recorded with that change,
so that I can ask in one place what runs without me and why something stopped, without any automation changing how it decides (FR-15).

## Entry conditions

Implemented only once story 7.13's Slice B decision entry records **go** (D-331; the conditions are listed in story 7.14), and after story 7.16, whose `governor_mode` and `governor_self_stopped` lines the table reads.

## Acceptance Criteria

1. **The table in core** (AD-146, FR-15, D-337). `governor::automations::table(&[SettingRow], &[JournalLine]) -> Vec<AutomationRow>`, pure and read-only, from records each automation already writes: `setting` rows with source `rules` of the triage model and auto-archive (D-69); the latest `auto_approval_set` line (v0.5); the `automation_changed` lines of standing orders; the `governor_mode` and `governor_self_stopped` lines. `AutomationKind` (`triage_model`, `auto_archive`, `auto_approval`, `governor`, `standing_order`) is a `closed_enum!`; each row gives the current state, the last change, its source and its cause as recorded, or "never changed" when no record exists. Proof: the `automations` section of `tests/vectors/governor.json`, a case per kind and one per kind without a record, through `crates/officina-core/tests/governor_vectors.rs::{automations_cases_match, every_automation_kind_has_a_case}` (the second iterates `AutomationKind::ALL`).
2. **Nothing else changes** (FR-15, D-337). No shared state type, no new rule, no write: the table reads; v0.3's and v0.5's automations keep their code and their tests unchanged. Proof: `automations` is a pure function with no host half (story 7.2's source scan and AD-1's purity lint cover it); the existing v0.3 and v0.5 test suites pass unchanged; review of the diff.
3. **Where it shows** (AD-144, AD-145, FR-17). `GovernorView` gains the rows (schema per P-63); `AutomationsTable.svelte` in the Governor section; the briefing (AD-83) gains an "Automations" section of the rows changed since the cursor, served by `officina mcp`'s `briefing`. Proof: core `crates/officina-core/src/orders/briefing.rs::tests::automations_changed_since_the_cursor`; Playwright `app/e2e/governor.spec.ts::the_automations_table_lists_each_kind_with_its_last_change`.
4. **Untrusted text stays out** (AD-74, D-305). Causes are enum names and numbers; no row carries a session's, a model's or a worker's text. Proof: story 7.2's source scan over `governor/automations.rs`; a vector case whose `setting` row carries a reason text the table does not copy.
5. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added. README "The governor (v0.7, in progress)": the table, what it reads, that it changes nothing. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/governor/automations.rs`, `AutomationKind`, the `automations` section (AC 1, 4)
- [ ] View rows, `AutomationsTable.svelte`, the briefing section (AC 3)
- [ ] `CHANGELOG.md`, README (AC 5)

## Dev Notes

- Requirements: FR-15, FR-17's Automations table. Decisions: AD-1, AD-50, AD-83, AD-88, AD-144, AD-145, AD-146. Epic decisions: D-69 (Epic 3); D-162 (Epic 5); P-63, D-300, D-305, D-337, D-340.
- Seams (D-300): the triage model and auto-archive setting rows (3.10, 3.11), `auto_approval_set` (5.14), the briefing (5.8).
- Waits for: story 7.16.
- The framing suspends task templates only; this table lets the author ask, it re-decides nothing (FR-15, the requirement challenge's Medium finding).

## Testing

- Core unit and integration: `crates/officina-core/src/{governor/automations,orders/briefing}.rs`, `crates/officina-core/tests/governor_vectors.rs`.
- Frontend: Playwright `app/e2e/governor.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
