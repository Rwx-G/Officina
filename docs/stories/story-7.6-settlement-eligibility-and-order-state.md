# Story 7.6: Settlement, acceptance, eligibility and the order state machine

**Status:** Approved

## Story

As the author whose review verdicts are the only judgement of a template's work,
I want each workshop run to settle at its verdict, each template's acceptance computed over its last settled runs, and each standing order told eligible and due or why not, turning itself off when the evidence says so,
so that only templates whose packages I import often enough can run alone, a run that correctly finds nothing to do costs its template nothing, and an order that stops earning autonomy stops by itself (FR-2, FR-3, J3).

## Acceptance Criteria

1. **Settlement** (AD-131, FR-2 as amended by amendment 2, D-313). `governor::settle::settle(&RunEvidence, gate_on_merges) -> Settlement` (`Accepted`, `Refused(RefusalKind)`, `NotCounted(NotCountedReason)`, `Unsettled`), each kind a `closed_enum!`: Import accepted at its `decided_at_ms`; Discard `poor_result` or `should_not_have_run`, and `not_needed` of an autonomous run, refused at `decided_at_ms`; a quarantine, any cause, refused at collection whether or not a Discard follows; `not_needed` of a clicked run, `expired`, `not_collected`, `no_change`, `failed` and `unknown` not counted; a stopped run awaiting Resume unsettled; with the gate on merges, an Import unsettled until its merge row is final, then accepted (`merged*`, at `settled_at_ms`) or refused (`not_merged`). Proof: the `settle` section of `tests/vectors/governor.json`, a case per row and per kind, through `crates/officina-core/tests/governor_vectors.rs::{settle_cases_match, every_settlement_kind_has_a_case, every_refusal_and_not_counted_reason_has_a_case}`.
2. **Acceptance** (AD-131). Accepted over settled among a template's last `acceptance_runs` settled runs by settlement time, clicked and autonomous alike, across projects; recomputed at each view, never stored; shown with the count it rests on, the `no_change` count, refusals by kind and FR-1's merged, retouched and not merged counts. Proof: core `crates/officina-core/src/governor/settle.rs::tests::{acceptance_counts_the_last_n_by_settlement_time, a_quarantine_after_an_import_orders_by_its_own_time, no_change_never_moves_acceptance}`; host `crates/officina-host/tests/governor_evidence.rs::a_correction_moves_the_figures_at_the_next_view`.
3. **Eligible and due** (AD-132, FR-3, D-315, D-316). `governor::eligibility::assess(&OrderEvidence, &TemplateAcceptance, &GovernorConfig, now_ms) -> OrderAssessment`: eligible or the reason (`too_few_settled`, `below_threshold`, `suspended`, `paused`, `off`); then due or the reason (`cadence`, `live_run`, `package_awaiting`, `base_unchanged_since_no_change`, `base_unchanged_since_not_needed`, `base_unknown`); the effective cadence (`every_hours`, doubled while the order's last three autonomous runs ended `no_change`, restored by the first other outcome) and the next due instant. The last launch is the latest real autonomous launch in mode on and the latest of that and the latest shadow booking in shadow (the booking supplied as an input by story 7.11). Proof: the `eligibility` section through `governor_vectors.rs::{eligibility_cases_match, every_eligibility_and_due_reason_has_a_case}`, with cases for the doubled cadence and its restoration, a 720-hour order whose booking is 20 days old, and `base_unknown` only after a `no_change`.
4. **Base tips** (AD-132, D-315). The `governor-git` thread (story 7.5) gains a job reading, every 5 minutes, `rev-parse --verify refs/heads/<base>^{commit}` for each approved order's project and base, with `GitRule::Workshop`; a tip that cannot be read is unknown. The base compared is the `base_commit` of the order's own autonomous runs. Proof: host integration `crates/officina-host/tests/governor_merge.rs::{base_tips_are_read_every_5_minutes, a_missing_project_reads_an_unknown_tip}` (real git, D-23).
5. **Transitions** (AD-132, FR-3, D-314). Core `eligibility::transition(&OrderState, &Event, ...) -> Option<Transition>`; the host writes it in the collector step that records the event (a verdict, a collection, a merge outcome, a correction, an expiry, the gate switch), whatever the governor's mode, and checks each template's newest settlement against its orders' latest state change at window start. Threshold suspension when the gate held before the event and fails after it (every order of the template); order suspension on an autonomous `should_not_have_run` or quarantine; pause after three autonomous runs in a row satisfying `view::is_environment_failure` (D-217), runs finished `autonomy_stopped` skipped; D-314's precedence; each transition journaled `automation_changed` (from, to, source, cause, numbers) and stored with source `rules`. Proof: transition cases in the `eligibility` section (coverage over `OrderState::ALL` as from and to states); CLI frozen-clock `crates/officina/tests/governor.rs::{a_settled_run_below_the_threshold_suspends_every_order_of_the_template, a_template_that_never_met_the_gate_suspends_nothing, an_autonomous_quarantine_suspends_its_order, a_clicked_quarantine_suspends_no_order_directly, three_environment_failures_pause_an_order, an_autonomy_stopped_run_neither_counts_nor_breaks_a_streak, a_paused_order_becomes_suspended_never_the_reverse, a_crossing_is_recorded_with_the_governor_off, window_start_recovers_a_missed_crossing}` (autonomous runs seeded as rows of source `governor`); core `crates/officina-core/src/relay/journal.rs::tests::automation_changed_round_trips`.
6. **Renames keep a state** (D-350, R82, review F18). Every state row carries the order's `identity_sha256` (canonical template, project, values and base). When an approved catalogue adds an order whose identity equals that of a `suspended` or `paused` row, present or absent for at most 90 days, the new name inherits that state, cause and `at_ms` with source `rules`, journaled `automation_changed` with the old name in its numbers; the catalogue banner flags the addition as inheriting it (`catalogue::diff` takes the suspended and paused identities from the host). Proof: transition cases in the `eligibility` section; core `crates/officina-core/src/relay/catalogue.rs::tests::diff_flags_an_order_inheriting_a_suspension`; CLI frozen-clock `crates/officina/tests/governor.rs::{a_renamed_suspended_order_stays_suspended, a_renamed_order_absent_91_days_starts_on, an_order_with_new_values_is_a_new_identity}`.
7. **Gate on merges** (AD-131, FR-2, D-69's pattern). `set_gate_on_merges(on | off)`: on is pointer only and refused by the writer unless, when it writes, at least 20 imports of the last 90 days (the retention of `merge_check` rows; D-313, a departure listed in P-61, review F24) have a final merge outcome and author corrections are under 30 % of them; off is always accepted and keyboard-reachable; stored in `setting.gate_on_merges` with source `author`; settlements move at the next view. The command joins `build.rs`, the capabilities, `ipc.ts` and `e2e-mock.ts`. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::{the_gate_on_is_refused_under_20_final_imports, the_gate_on_is_refused_at_30_percent_corrections, the_gate_off_is_always_accepted}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
8. **Slice A acts on nothing** (FR-3, AD-132). No author command turns an order on or off before story 7.16; eligibility is computed and shown. Proof: `main.rs::tests::every_declared_command_is_allowed` lists no `set_standing_order`; review.
9. **Doctor** (AD-147, D-341). For each standing order: eligible and due, or why not, from core's functions on the read-only registry and the approved snapshot. Proof: host `crates/officina-host/src/governor/doctor.rs::tests::each_order_reports_eligibility_and_due_or_why_not`.
10. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added. README "The governor (v0.7, in progress)": a run settles at its verdict; what counts accepted, refused and not counted, `no_change` counting nowhere; eligibility, due, the doubled cadence; suspension and pause, and that only the author's click brings an order back. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/governor/settle.rs` and the `settle` section (AC 1, 2)
- [ ] `crates/officina-core/src/governor/eligibility.rs`, `assess`, `transition`, the `eligibility` section (AC 3, 5)
- [ ] The base-tip job on `governor-git` (AC 4)
- [ ] Transitions in the collector, window-start check, `JournalKind::AutomationChanged` (AC 5)
- [ ] Identity inheritance at approval; the banner's flag (AC 6)
- [ ] `set_gate_on_merges` writer and command (AC 7)
- [ ] `crates/officina-host/tests/governor_evidence.rs`; frozen-clock cases in `crates/officina/tests/governor.rs` (AC 2, 5)
- [ ] Doctor; `CHANGELOG.md`, README (AC 9, 10)

## Dev Notes

- Requirements: FR-2 as amended, FR-3, PRD section 7's Merge detection row (the switch), section 8 questions 7 and 9. Decisions: AD-50 (pause), AD-109 (`ENVIRONMENT_FAILURE_CAUSES`), AD-110 (`no_change`), AD-114, AD-131, AD-132. Risks: R78, R82. Epic decisions: D-69 (Epic 3); P-65, D-135 (Epic 5); D-214, D-217, D-222 (Epic 6); P-60 (FR-2's zero-commit sentence), D-300, D-313, D-314, D-315, D-316, D-340, D-341, D-350.
- Seams (D-300): the verdict paths (6.16), collection and its outcomes (6.13), `view::is_environment_failure` (6.12, D-217), the setting writer of D-69 (3.10, 3.11), `host::git` (6.3), the per-template table of v0.6 (6.18), which keeps its own "imported over decided" beside these figures (D-222).
- Waits for: stories 7.2, 7.4 and 7.5.
- Why a quarantine settles at collection (D-313): it is a host fact, decided before any verdict, and FR-3 suspends "at once" on it.
- Why transitions run outside the governor step (D-314): with the governor off, a template could cross below its threshold unseen, and turning shadow on later would read it as merely below, without the "clicked run after the suspension" rule.
- In Slice A, suspensions can come from clicked evidence (a threshold crossing); pauses and order suspensions need autonomous runs, which tests seed as rows. Nothing turns an order back on before story 7.16.
- Acceptance is per template across projects (PRD section 8, question 9); the view shows the per-project counts so the owner can read that question.

## Testing

- Core unit and integration: `crates/officina-core/src/{governor/settle,governor/eligibility,relay/journal}.rs`, `crates/officina-core/tests/governor_vectors.rs`.
- Host: `crates/officina-host/src/{governor/writers,governor/doctor}.rs`, `crates/officina-host/tests/{governor_evidence,governor_merge}.rs`.
- CLI integration: `crates/officina/tests/governor.rs`. App crate: `app/src-tauri/src/main.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: a renamed order inherits a suspension or pause by identity, and the banner flags it (F18); the gate switch's 90 days listed as a departure (F24)
