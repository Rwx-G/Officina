# Story 7.10: The decision

**Status:** Approved

## Story

As the author who lets deterministic code, not a model, choose when routine work runs,
I want one pure function that reads numbers, counts, my verdicts and my approved catalogue, checks every condition in a fixed order, and returns hold with its causes, probe, or would-launch,
so that one line always tells me why nothing runs or why something would, every condition is proved to block a launch on its own, and no text anyone wrote can reach the choice (FR-11, J4, principle 8).

## Acceptance Criteria

1. **Types** (AD-137, D-325, D-326). `governor::decide(input: &GovernorInput, now_ms: i64, local_offset_min: Option<i32>) -> Decision`, pure. `GovernorInput` is serializable and every field is a number, a count, a closed-enum value or author text from the approved catalogue (AD-137's table, with D-316's latest booking per order, D-329's readiness, D-344's `inputs: InputsState`, and the last autonomous run's end and the 5-hour windows used counting shadow bookings, D-349; new fields take `#[serde(default)]`, so a journaled input replays after story 7.14 adds the last refusal time). No field holds a worker's reported cost (P-64): allowances and this week's points arrive as story 7.9 computes them from trusted figures. `GovernorMode` and `SelfStopCause` come from story 7.4 (D-310, review F8). `HoldCause` is a `closed_enum!` seeded from AD-137's table as D-303 and D-304 build it (a cause whose condition is not built is not a variant), with D-344's `inputs_unknown` (global, step 2) and `readiness_unknown` (per order, step 9); `HoldCause::family()` is an exhaustive `match` naming each cause's family (review F33); `HoldNumbers` has one variant per cause; `HoldReport` holds the causes in order, the per-order causes and `next_change_ms`; `LaunchRecord` holds AD-137's fields and `input`, the canonical JSON of the `GovernorInput`. `Decision` is `Hold(HoldReport)`, `Probe` or `WouldLaunch(LaunchRecord)`: it has no `Launch` variant. Proof: core `crates/officina-core/src/governor/decide.rs::tests::{the_input_serializes_canonically, a_launch_record_carries_its_input, every_hold_cause_has_a_family}`; a `compile_fail` doctest naming `Decision::Launch`.
2. **Conditions in order** (AD-137, FR-11 as amended by amendment 7, D-325). Steps 1 to 12 in AD-137's order; every failing condition is recorded with its numbers and the first is shown; a condition that cannot be evaluated because a missing-data cause holds is not recorded; global causes come first, then the per-order causes of the first due order in pick order, each due order's own causes travelling in `orders[]`; an approved floor whose `[workshop] auth` is `api-key` holds `governor_unavailable` at step 3 (D-1204); without the probe, step 11 adds `one_per_window` and step 12 does not exist. **Fail-closed inputs** (D-344, review F4): an unknown registry, journal ledger, board (stale, or `claude agents` not yet answered since window start) or readiness cache gives `inputs_unknown` at step 2 with numbers naming each missing input, and an order whose project has no cached readiness facts gives `readiness_unknown`; the conditions those inputs feed are not recorded (D-325). Proof: the `decide` section of `tests/vectors/governor.json` through `crates/officina-core/tests/governor_vectors.rs::{decide_cases_match, every_hold_cause_is_first_in_a_case, every_decision_kind_has_a_case}` (the second iterates `HoldCause::ALL` and requires each variant as the first cause of at least one case, `inputs_unknown` once per kind of missing input and `readiness_unknown` included).
3. **The probe step** (AD-137 step 12, FR-11). With the probe available: when steps 1 to 10 leave a candidate and the quota conditions pass on fresh data or cannot be evaluated (`quota_unknown`, `quota_stale`), and no probe observation anchored in the last `PROBE_FRESH_MS` (2 minutes) is among those used, the decision is `Probe` if the budget and any back-off allow, else a hold with `probe_wait` beside the quota cause; a quota condition failing on fresh data holds without probing; `probe_failed` holds while the last probe failed structurally and the pause has not begun; `WouldLaunch` rests on a probe anchored in the last 2 minutes. Proof: `decide` cases for each branch, including a stale file with a passing probe 119 s old (would launch) and 121 s old (probe).
4. **The pick and the deadline** (AD-137, FR-11, FR-12 step 4). Among candidates, the oldest last launch, never launched first, ties by name. The record's deadline is `now_ms` plus the least of the template's `max_minutes`, the time to the 5-hour reset and the time to the window's end, with which bound set it. Proof: `decide` cases for the pick's three rules and each bound.
5. **The next change** (AD-137). `next_change_ms`: the earliest instant at which a time-based input flips a condition (window start, end and ending threshold, a cadence coming due, an observation's freshness ending, a reset, the perishable start, the probe budget or a back-off clearing). Proof: `decide` cases, one per source, asserting `next_change_ms`.
6. **Each condition blocks alone** (AD-148 blocking property, D-325). In the test, `make_fail(cause, &mut GovernorInput)` is an exhaustive `match` over `HoldCause`; for every `decide` case expecting `WouldLaunch` and every cause, `decide` on the modified input must hold with that cause present. A new variant fails the test's compilation until it gets its failing input. Proof: `governor_vectors.rs::each_cause_blocks_every_launch_case`.
7. **Readiness per order** (D-329, D-344). `workshop::readiness_without_card(&WorkshopReadinessFacts, project) -> Readiness`: the first side-effect-free refusal of AD-107's order among the facts the `workshop` thread caches with `WorkshopView`, `Ready`, or `Unknown` when the facts lack the project, which is never a pass; checks needing a card are not run; readiness reads the floor's `CredentialState`, `missing`, `needs_login` and `needs_new_credential` being refusals and `unchecked` and `ok` passing (D-1213). Proof: core `crates/officina-core/src/workshop/view.rs::tests::{readiness_without_card_keeps_ad_107_order, checks_needing_a_card_are_not_run, a_project_without_facts_is_unknown}`.
8. **No text reaches the decision** (FR-12's principle 8 paragraph, D-305). Story 7.2's source scan, with its `crate::` allow-list, passes on `decide.rs`; `GovernorInput`'s author text is limited to names from the approved catalogue, and its numbers come from trusted sources only (P-64). Proof: `crates/officina-core/tests/governor_sources.rs::{no_governor_source_names_untrusted_text, no_governor_source_reaches_outside_the_allow_list}`; the forged-cost cases of story 7.9.
9. **Mutants** (FR-12, AD-148 mutation row, D-148, D-338 as amended by review F10). Core features `mutant-governor-skip-freshness`, `mutant-governor-skip-ceiling` and `mutant-governor-skip-reserve`, each skipping one check at its own site, forwarded `officina` to `officina-host` to `officina-core`, covered by `MUTANTS_ENABLED` and the app's compile-time assertion. The `mutants` job gains, per feature, `cargo test -p officina-core --test governor_vectors --features <feature> --no-run` (must compile), then the run, which must fail with its named case: `decide/fallback_stale_file_holds_quota_stale` (freshness; a fallback case, since with the probe a stale observation gives `Probe` either way), `decide/ceiling_at_the_count_holds` (ceiling), `decide/projection_plus_reserve_over_100_holds` (reserve); then a control run without the features passes. Stories 7.14 to 7.16 add the launch, hold and live-guard cases. Proof: the CI job `mutants`.
10. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added. README "The governor (v0.7, in progress)": the families of hold causes, each named as `HoldCause::family()` names it, naming `HoldCause` without listing its members; that the decision reads numbers, counts, verdicts and the approved catalogue, never text, and holds on any input it cannot read. Proof: `crates/officina/tests/readme.rs::readme_names_every_hold_cause_family` (iterates the families; review F33); the prose is reviewed.
11. **No five-hour window, and an API-key floor** (D-1217, D-1204). A `decide` case whose fresh observations carry the weekly window alone expects `no_active_window` first; a case on an approved floor of kind `api-key` expects `governor_unavailable` first, its numbers naming the kind. Proof: `decide/weekly_window_alone_holds_no_active_window`, `decide/an_api_key_floor_is_unavailable` through `governor_vectors.rs::decide_cases_match`; a credential state case per refusing state in `workshop/view.rs`'s tests.

## Tasks

- [ ] `crates/officina-core/src/governor/decide.rs`: types, `decide`, the order of conditions, the probe step, the pick, the deadline, `next_change_ms` (AC 1 to 5)
- [ ] `decide` section; coverage tests; `make_fail` and the blocking property (AC 2, 6)
- [ ] `InputsState`, `inputs_unknown`, `readiness_unknown` (AC 1, 2)
- [ ] `workshop::readiness_without_card` (AC 7)
- [ ] Mutant features, forwarding, CI targets (AC 9)
- [ ] `CHANGELOG.md`, README (AC 10)

## Dev Notes

- Reworked 2026-11-02 (D-1201, D-1204, D-1213, D-1217): one regime, so step 11 never holds `usd_budget`; readiness reads v0.6's credential state (D-607, D-1135) and `hook_program_missing` stays the guard a missing hook program needs (D-1133); the mutants job and the lone-crate step are steps of the local gate. GitHub CI is off (D-1201): every CI job named here is that job replayed by the local full gate.
- Requirements: FR-11 as amended (amendment 7), FR-12's principle 8 paragraph, J4, PRD section 7's Fresh quota, Reserve and Ceiling rows (their vector half). Decisions: AD-107, AD-127, AD-133 to AD-137, AD-148. Risks: R71, R73. Epic decisions: D-148 (Epic 5); P-60 (AD-137 over FR-11's step list), P-64, P-65, D-300, D-303, D-304, D-305, D-310, D-316, D-324, D-325, D-326, D-329, D-338, D-340, D-344, D-349.
- Seams (D-300): v0.6's `LaunchRefusal` and the readiness facts behind `WorkshopView` (6.11, 6.18), `Board.stale` and `BoardInput.agents` (1.4).
- Waits for: stories 7.2, 7.3, 7.4 (the mode types), 7.6, 7.7, 7.8 and 7.9, which supply the functions each step calls (without the probe, D-303, story 7.8 is not built and its steps are not variants).
- The decision has no `Launch` in Slice A at all: shadow cannot launch by type (D-326). Story 7.14 adds the variant behind `governor-launch`, and builds the grant inside `decide`.
- A missing-data cause stands for the conditions it hides (D-325), so `make_fail` for `five_hour_room` must supply fresh data; the blocking property is what proves each cause is reachable.
- `inputs_unknown` makes AD-137's "each age errs toward holding" executable (D-344): an unreadable registry would otherwise read as no live run, no package awaiting and no suspension, and a board before `claude agents` answers as an empty antechamber.
- `governor.json` is formatter-exempt (D-324): the cases are hand-written inputs and expectations, and `input` lines of the journal (story 7.11) replay through the same function.
- No employer content: placeholder orders, templates and projects.

## Testing

- Core unit and integration: `crates/officina-core/src/{governor/decide,workshop/view}.rs`, `crates/officina-core/tests/{governor_vectors,governor_sources}.rs`; doctests.
- CI: job `mutants`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `inputs_unknown` and `readiness_unknown` hold on unknown registry, journal, board or readiness (F4); the mode types come from story 7.4 (F8); each mutant names its case, freshness on a fallback case (F10); bookings feed `no_post_run_observation` and `one_per_window` (F15); no reported cost in the input (F1, P-64); `HoldCause::family()` and the README family test (F33)
- 2026-11-02 reworked against v0.4 to v0.6 as built: one regime, an api-key floor unavailable (D-1204, AC 2, AC 11); readiness reads the credential state (D-1213, AC 7); the weekly-window-only case (D-1217, AC 11); local gate (D-1201)
