# Story 3.10: Measured error, auto-archive gate, routine candidates

**Status:** review

## Story

As the author who trusts automation only as far as it has been measured,
I want to see how often each source's "finished" was wrong, an auto-archive toggle that exists only while `state` proposals have never been flipped over enough cases, and the subjects I marked routine listed for later,
so that nothing is archived without me until the evidence says it is safe, and the routine work I spotted is not forgotten.

## Acceptance Criteria

1. Rates in core (FR-11 as amended by amendment 9, AD-50, D-59, D-77): `registry::rates` gives, per proposal source, the flipped count among that source's last 50 confirmed `finished` proposals with the count it rests on, the flips among all its last 50, the `unsure` count apart, and says so when a source has no `finished` proposal; auto confirmations count once 7 days old, as flipped if the subject was reopened within those 7 days, by the author or by a session alive again (FR-12 as amended by amendment 10). Counts, rates and thresholds travel in the view (AD-5). Proof: the `rates` section of `tests/vectors/triage.json`; core `crates/officina-core/src/registry/rates.rs::tests::{last_50_per_source, no_finished_proposal_is_stated, auto_confirmation_counts_after_7_days, author_reopen_within_7_days_is_a_flip, alive_again_within_7_days_is_a_flip, unsure_is_counted_apart}`.
2. Gates in core (AD-50, D-69): the auto-archive gate (`state` source, at least 50 cases, below 2 %, which with 50 cases means none flipped), the model enable gate (latest dry run under 7 days old, at least 30 cases, `model_correct > default_correct`) and the model self-disable test (over its last 50 confirmed proposals, at least 20, right less often than `to_resume` would have been) each return a `GateState` with its reason. The model gates are wired by story 3.11. Proof: the `rates` cases; `triage_vectors.rs::every_value_has_a_case` now iterates `GateState::ALL`.
3. Auto-archive (FR-12, AD-50, D-62, D-70, R18): `set_auto_archive` is refused unless the gate holds; while on, the collector confirms, after each view build, the rows of filed subjects whose proposal is `state` `finished` with `auto = 1`, archiving without the routine mark; unfiled rows, empty-subject rows and model proposals are never auto-archived; when the gate breaks the toggle turns off with source `rules` and its reason. Proof: host `crates/officina-host/tests/registry_write.rs::{auto_archive_is_refused_while_the_gate_does_not_hold, auto_archive_confirms_state_finished_rows_of_filed_subjects_only, auto_archive_never_takes_a_model_unfiled_or_empty_subject_row, gate_break_turns_auto_archive_off_with_its_reason}`; CLI-crate collector test `crates/officina/tests/watch.rs::auto_archive_runs_after_the_view_build`.
4. A wrong auto-archive is seen (D-77, FR-12 amendment 10): a background session of an auto-archived subject re-instructed within 7 days reopens the subject and the `state` source counts one flip. Proof: CLI-crate collector test `watch.rs::a_re_instructed_auto_archived_session_counts_as_a_flip` (copied fake with a sidecar listing the session `done`, then `working`, D-86).
5. Evidence in the window (FR-11, journey 4, PRD section 7 Evidence): per source, the flipped `finished` proposals with the count they rest on, the flips among the last 50, the `unsure` count, and "no finished proposal yet" when so. Proof: Playwright `app/e2e/evidence.spec.ts > per source, flipped finished proposals with the count they rest on`, `> a source with no finished proposal says so`.
6. Auto-archive toggle in the window (FR-12, AD-55): offered only while the gate holds; otherwise disabled with the gate's reason; recent auto-archives (7 days) are listed with a reopen action; `set_auto_archive` is declared, allowed, wrapped and mocked. Proof: `evidence.spec.ts > the auto-archive toggle explains why it is disabled`, `> the toggle sends set_auto_archive while the gate holds`, `> recent auto-archives can be reopened`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
7. Routine candidates (FR-13, P-10): subjects archived with the routine box, merged sources excepted (D-55), are listed in the view as candidates for the v0.7 catalogue and shown in the Subjects view. Proof: core `crates/officina-core/src/registry/view.rs::tests::routine_candidates_exclude_merged_sources`; Playwright `app/e2e/subjects.spec.ts > routine subjects are listed as candidates`; CLI `crates/officina/tests/subjects.rs::routine_candidates_are_printed`.
8. Fixtures (D-72): an `evidence` scenario from `registry_fixture` seeds confirmations at explicit `now_ms` (flipped and kept proposals per source, empty-subject confirmations, auto-archives older and younger than 7 days, one reopened within them). Proof: CI fixture step and the Playwright cases.
9. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": what the error rates count, when auto-archive can be turned on and why it may turn itself off, that any process able to start `claude --bg` in a repository feeds the `state` source (a wrong auto-archive is reopened when its session comes back, and counted), routine candidates. Proof: review.

## Tasks

- [x] `crates/officina-core/src/registry/rates.rs`: per-source counts, auto rule with both reopen paths, `GateState` reasons for the three gates (AC 1, 2)
- [x] `rates` section of the vector set; coverage of `GateState::ALL` (AC 1, 2)
- [x] Auto-archive in the collector and writer; `set_auto_archive` with `setting` provenance (AC 3, 6)
- [x] Collector test of a re-instructed auto-archived session (AC 4)
- [x] View fields: rates, gates with reasons, recent auto-archives, routine candidates; regenerate TypeScript (AC 1, 5-7)
- [x] `TriageEvidence` per-source part and the toggle; routine list in `SubjectsView` (AC 5-7)
- [x] `evidence` scenario; Playwright cases (AC 5-8)
- [x] Update `CHANGELOG.md` and the README (AC 9)

## Dev Notes

- Requirements: FR-11 as amended (amendment 9), FR-12 as amended (amendment 10), FR-13, journey 4, PRD section 7 Evidence. Decisions: AD-5, AD-50, AD-54, AD-55. Risks: R18 (auto-archive ignores model proposals), R20. Epic decisions: P-10, D-55, D-58, D-59, D-62, D-69, D-70, D-72, D-76, D-77, D-86.
- `rules` proposes `finished` only for an empty subject (D-76); its false-`finished` rate otherwise says "no finished proposal yet", which AC 1 and AC 5 show.
- Auto-archives count once 7 days old (AD-50): without that, the rate would freeze as soon as automation runs, since the author confirms nothing by hand any more.
- With 50 cases, below 2 % means none flipped (amendment 10): one flip closes the gate until 50 newer unflipped cases exist.
- A reopen stamps `reopened_at_ms` on the subject's latest confirmation, whether the author reopens (D-58) or a session comes alive again (D-77); the second is how a re-instructed background session turns a wrong auto-archive into a counted flip.
- All three gates live in `rates` (AD-38) so the model gates of story 3.11 read the same counts the panel shows.

## Testing

- Core unit: `crates/officina-core/src/registry/{rates,view}.rs`.
- Core integration: `crates/officina-core/tests/triage_vectors.rs`.
- Host integration: `crates/officina-host/tests/registry_write.rs`.
- CLI integration and collector: `crates/officina/tests/{subjects,watch}.rs`.
- App crate unit: `app/src-tauri/src/main.rs`. Playwright: `app/e2e/{evidence,subjects}.spec.ts`.

## Dev Agent Record

### Debug Log

- **Decisions where the story is silent:**
  - AC 1: a confirmation flips when the author's verdict is not the definite one proposed; `unsure` counts apart and is never a flip (D-59). Only an auto confirmation counts a reopen: it counts once 7 days old, as kept, or as soon as its subject is reopened within those 7 days, as a flip, so a wrong auto-archive closes the gate at once rather than a week later. "Last 50" is by confirmation time, then id, among the confirmations that count yet; the `finished` count rests on the source's `finished` proposals only, the overall count on all of them.
  - AC 2: "below 2 %" is `flipped * 100 < 2 * cases`, which with the 50 cases the window holds means none flipped (amendment 10). The self-disable test reports `holds` below 20 cases (too few to judge keeps the model on); `GateState` carries one variant per reason across the three gates, and each `Gate` carries the cases it read and the cases it needs, which the panel words.
  - AC 3: the setting's value is `{"on", "reason"}`: the author's change has no reason, a rules change carries the gate state that broke it (D-69). The pass runs in `publish_both`, right after the view is emitted, and emits a second view when it archived or turned itself off, so the window never waits a tick to see it. It checks the gate first, so a gate broken while the window was closed turns the switch off before anything is archived. `auto_archivable` in core decides the rows (a filed subject's row, `state`, `finished`), so unfiled rows, empty subjects and model proposals are excluded by construction (R18); the model's case is a built row while the model stays off (story 3.11).
  - AC 4: the copied fake's sidecar fixture lists the session `done`, then `working`; nothing else is needed, since D-77's wake already stamps the subject's latest confirmation, which is the auto one.
  - AC 6: the toggle can always be turned off; on, only while the gate holds. The recent list shows the auto-archives of the last 7 days, with **Reopen** while the subject is still archived and "reopened" once it was.
  - AC 7: routine candidates are archived subjects with the routine box and no `merged_into`, the latest archived first; listed in the Subjects view above the subjects.
  - AC 8: `registry_fixture` gains `auto_archive` and `auto_archive_pass` steps; the scenario names subjects as `SUBJECT(<name>)`, resolved by the script to their creation-order id, so 55 created subjects need no counting by hand. The fixture checks its own claims (50 cases with one flip for `state`, a `rules` source with empty subjects and flips, `model` with no `finished`, the switch off by rules with its reason, two recent auto-archives, one reopened).
- **Adversarial pass** (a gate broken while the window was closed, a flip racing an enable, a reopen exactly at the 7-day edge, a session alive again after the 7 days, an auto-archive of a subject with a live session, a model or unfiled row, the switch's DOM state after a refusal): one real miss, now tested: the checkbox moved with the click and stayed ticked when the backend refused the change (`app/e2e/evidence.spec.ts > a refused toggle stays as the registry holds it and says why`). The 7-day edge is in `rates.rs::tests::author_reopen_within_7_days_is_a_flip` and the vectors; the closed-window case is the gate-first order of `Writer::auto_archive`, held by `registry_write.rs::gate_break_turns_auto_archive_off_with_its_reason`.

### Full gate at the end of story 3.10

Every job of `.github/workflows/ci.yml`, run locally with `--locked`, covering stories 3.9 and 3.10, on 10f24a4: `cargo fmt --check`; frontend install (frozen lockfile) and build; clippy `--workspace --all-targets --features officina-core/ts` clean, in the `core-linux` feature set, and on the host without `window-authority`; `cargo test --workspace --features officina-core/ts` 933 passed, 0 failed, 11 ignored (TypeScript drift green); `perf.rs` alone in release 5 passed (hook own cost 7.1 ms, budget 150 ms; gate own cost 3.1 ms, budget 15 ms; in-process p95 3.4 ms); the lexer differential 2 passed; `approvals_stress` passed; the crash step 3 passed (helper built first); "officina holds no authority": both markers absent from `officina.exe`, present in `officina-app.exe`; "Registry dependencies" ok; "No bundled roots" ok; `pnpm check`, `lint`, `test` 47 passed; fixtures rebuilt from the real CLI (20 scenarios, `evidence` new), Playwright 94 passed, and the evidence, subjects, triage and board specs 192 passed with `--repeat-each=3`; `cargo deny --locked check` ok; `pnpm audit` 1 low, ignored with its reason (unchanged). The `core-linux` job replayed in a `rust:1-bookworm` container on 10f24a4 with Rust 1.99.0: fmt, clippy clean, 821 passed, 0 failed, 9 ignored. Node 25.4 locally against CI's 22. The triage probe (story 3.9 AC 13) was not run: a real call is the owner's.

### Completion Notes

- AC 1: the `rates` section of `tests/vectors/triage.json`; core `crates/officina-core/src/registry/rates.rs::tests::{last_50_per_source, no_finished_proposal_is_stated, auto_confirmation_counts_after_7_days, author_reopen_within_7_days_is_a_flip, alive_again_within_7_days_is_a_flip, unsure_is_counted_apart}`.
- AC 2: the `rates` cases; `crates/officina-core/tests/triage_vectors.rs::every_value_has_a_case` iterates `GateState::ALL`; `rates.rs::tests::model_gates_read_the_dry_run_and_the_model_rows`.
- AC 3: `crates/officina-host/tests/registry_write.rs::auto_archive::{auto_archive_is_refused_while_the_gate_does_not_hold, auto_archive_confirms_state_finished_rows_of_filed_subjects_only, auto_archive_never_takes_a_model_unfiled_or_empty_subject_row, gate_break_turns_auto_archive_off_with_its_reason}`; `crates/officina/tests/watch.rs::auto_archive_runs_after_the_view_build`; core `verdict.rs::tests::only_state_finished_subject_rows_are_auto_archivable`.
- AC 4: `watch.rs::a_re_instructed_auto_archived_session_counts_as_a_flip`.
- AC 5: `app/e2e/evidence.spec.ts > per source, flipped finished proposals with the count they rest on`, `> a source with no finished proposal says so`.
- AC 6: `evidence.spec.ts > the auto-archive toggle explains why it is disabled`, `> the toggle sends set_auto_archive while the gate holds`, `> recent auto-archives can be reopened`, `> a refused toggle stays as the registry holds it and says why`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
- AC 7: core `crates/officina-core/src/registry/view.rs::tests::routine_candidates_exclude_merged_sources`; `app/e2e/subjects.spec.ts > routine subjects are listed as candidates`; `crates/officina/tests/subjects.rs::routine_candidates_are_printed`.
- AC 8: the `evidence` scenario of `app/scripts/e2e-fixtures.mjs` and its checks; the Playwright cases above.
- AC 9: `CHANGELOG.md` `[Unreleased]`; README "Measured error and auto-archive" and "Routine candidates".

## File List

- `crates/officina-core/src/registry/{rates,verdict,view,mod}.rs`, `crates/officina-core/tests/triage_vectors.rs`, `tests/vectors/triage.json`
- `crates/officina-host/src/registry/{read,write,window}.rs`, `crates/officina-host/src/watch.rs`, `crates/officina-host/examples/registry_fixture.rs`, `crates/officina-host/tests/registry_write.rs`
- `crates/officina/tests/{watch,subjects}.rs`
- `app/src-tauri/{build.rs,capabilities/default.json,src/main.rs}`, `app/src/lib/generated/` (regenerated), `app/src/lib/{ipc.ts,registry.svelte.ts,registry-text.ts}`, `app/src/e2e-mock.ts`, `app/src/components/{TriageEvidence,SubjectsView}.svelte`, `app/src/components/components.test.ts`, `app/scripts/e2e-fixtures.mjs`, `app/e2e/{evidence,subjects}.spec.ts`
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.10-measured-error-and-auto-archive.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: a session alive again within 7 days counts as a flip of an auto-archive, with a collector test (D-77); empty-subject rows are a `rules` source of `finished` and are never auto-archived (D-76); README line on `claude --bg` feeding the `state` source
- 2026-10-28 implemented: the rates and the three gates in core with their vectors, auto-archive under its gate in the writer and the collector, the view's rates, gates, recent auto-archives and routine candidates, the panel's evidence and toggle, the routine list, the evidence fixture; one adversarial test added. Status review
