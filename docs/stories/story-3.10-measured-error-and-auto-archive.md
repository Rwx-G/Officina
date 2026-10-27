# Story 3.10: Measured error, auto-archive gate, routine candidates

**Status:** Approved

## Story

As the author who trusts automation only as far as it has been measured,
I want to see how often each source's "finished" was wrong, an auto-archive toggle that exists only while `state` proposals have never been flipped over enough cases, and the subjects I marked routine listed for later,
so that nothing is archived without me until the evidence says it is safe, and the routine work I spotted is not forgotten.

## Acceptance Criteria

1. Rates in core (FR-11 as amended by amendment 9, AD-50, D-59): `registry::rates` gives, per proposal source, the flipped count among that source's last 50 confirmed `finished` proposals with the count it rests on, the flips among all its last 50, the `unsure` count apart, and says so when a source has no `finished` proposal; auto confirmations count once 7 days old, as flipped if the subject was reopened within those 7 days (FR-12 as amended by amendment 10). Counts, rates and thresholds travel in the view (AD-5). Proof: the `rates` section of `tests/vectors/triage.json`; core `crates/officina-core/src/registry/rates.rs::tests::{last_50_per_source, no_finished_proposal_is_stated, auto_confirmation_counts_after_7_days, reopen_within_7_days_is_a_flip, unsure_is_counted_apart}`.
2. Gates in core (AD-50, D-69): the auto-archive gate (`state` source, at least 50 cases, below 2 %, which with 50 cases means none flipped), the model enable gate (latest dry run under 7 days old, at least 30 cases, `model_correct > default_correct`) and the model self-disable test (over its last 50 confirmed proposals, at least 20, right less often than `to_resume` would have been) each return a `GateState` with its reason. The model gates are wired by story 3.11. Proof: the `rates` cases; `triage_vectors.rs::every_value_has_a_case` now iterates `GateState::ALL`.
3. Auto-archive (FR-12, AD-50, D-62, D-70, R18): `set_auto_archive` is refused unless the gate holds; while on, the collector confirms, after each view build, the rows of filed subjects whose proposal is `state` `finished` with `auto = 1`, archiving without the routine mark; unfiled rows and model proposals are never auto-archived; when the gate breaks the toggle turns off with source `rules` and its reason. Proof: host `crates/officina-host/tests/registry_write.rs::{auto_archive_is_refused_while_the_gate_does_not_hold, auto_archive_confirms_state_finished_rows_of_filed_subjects_only, auto_archive_never_takes_a_model_or_unfiled_row, gate_break_turns_auto_archive_off_with_its_reason}`; CLI-crate collector test `crates/officina/tests/watch.rs::auto_archive_runs_after_the_view_build`.
4. Evidence in the window (FR-11, journey 4, PRD section 7 Evidence): per source, the flipped `finished` proposals with the count they rest on, the flips among the last 50, the `unsure` count, and "no finished proposal yet" when so. Proof: Playwright `app/e2e/evidence.spec.ts > per source, flipped finished proposals with the count they rest on`, `> a source with no finished proposal says so`.
5. Auto-archive toggle in the window (FR-12, AD-55): offered only while the gate holds; otherwise disabled with the gate's reason; recent auto-archives (7 days) are listed with a reopen action; `set_auto_archive` is declared, allowed, wrapped and mocked. Proof: `evidence.spec.ts > the auto-archive toggle explains why it is disabled`, `> the toggle sends set_auto_archive while the gate holds`, `> recent auto-archives can be reopened`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
6. Routine candidates (FR-13, P-10): subjects archived with the routine box, merged sources excepted (D-55), are listed in the view as candidates for the v0.7 catalogue and shown in the Subjects view. Proof: core `crates/officina-core/src/registry/view.rs::tests::routine_candidates_exclude_merged_sources`; Playwright `app/e2e/subjects.spec.ts > routine subjects are listed as candidates`; CLI `crates/officina/tests/subjects.rs::routine_candidates_are_printed`.
7. Fixtures (D-72): an `evidence` scenario from `registry_fixture` seeds confirmations at explicit `now_ms` (flipped and kept proposals per source, auto-archives older and younger than 7 days, one reopened within them). Proof: CI fixture step and the Playwright cases.
8. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": what the error rates count, when auto-archive can be turned on and why it may turn itself off, routine candidates. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/registry/rates.rs`: per-source counts, auto rule, `GateState` reasons for the three gates (AC 1, 2)
- [ ] `rates` section of the vector set; coverage of `GateState::ALL` (AC 1, 2)
- [ ] Auto-archive in the collector and writer; `set_auto_archive` with `setting` provenance (AC 3, 5)
- [ ] View fields: rates, gates with reasons, recent auto-archives, routine candidates; regenerate TypeScript (AC 1, 4-6)
- [ ] `TriageEvidence` per-source part and the toggle; routine list in `SubjectsView` (AC 4-6)
- [ ] `evidence` scenario; Playwright cases (AC 4-7)
- [ ] Update `CHANGELOG.md` and the README (AC 8)

## Dev Notes

- Requirements: FR-11 as amended (amendment 9), FR-12 as amended (amendment 10), FR-13, journey 4, PRD section 7 Evidence. Decisions: AD-5, AD-50, AD-54, AD-55. Risks: R18 (auto-archive ignores model proposals), R20. Epic decisions: P-10, D-55, D-58, D-59, D-62, D-69, D-70, D-72.
- Rules never propose `finished` (FR-8): their false-`finished` rate says "no finished proposal yet" by design, which AC 1 and AC 4 show.
- Auto-archives count once 7 days old (AD-50): without that, the rate would freeze as soon as automation runs, since the author confirms nothing by hand any more.
- With 50 cases, below 2 % means none flipped (amendment 10): one flip closes the gate until 50 newer unflipped cases exist.
- Reopen stamps `reopened_at_ms` on the subject's latest confirmation (D-58), which is how an auto-archive becomes a flip.
- All three gates live in `rates` (AD-38) so the model gates of story 3.11 read the same counts the panel shows.

## Testing

- Core unit: `crates/officina-core/src/registry/{rates,view}.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host integration: `crates/officina-host/tests/registry_write.rs`.
- CLI integration and collector: `crates/officina/tests/{subjects,watch}.rs`.
- App crate unit: `app/src-tauri/src/main.rs`. Playwright: `app/e2e/{evidence,subjects}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
