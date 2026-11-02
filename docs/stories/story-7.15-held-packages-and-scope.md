# Story 7.15: Held packages, the Needs you rule, the order's scope and the self-stop writer

**Status:** Approved

## Story

As the author whose antechamber has three seats and who said which paths a routine order may touch,
I want an autonomous package to wait outside Needs you while my antechamber is full, to enter it one seat at a time as room frees, and to be quarantined when it changes a path outside its order's scope, all before the governor can be turned on,
so that the governor never adds to a full antechamber, never hands me work outside what I allowed, and never interrupts me while it waits (FR-8, FR-12's Scope, J2).

## Entry conditions

Implemented only once story 7.13's Slice B decision entry records **go** (D-331; the conditions are listed in story 7.14), and after story 7.14. It precedes story 7.16, which offers mode on: no build offers on before scope and holding exist (D-345, review F5).

## Acceptance Criteria

1. **The scope travels with the run** (D-333 as amended by review F34). `RunPlan` (`workshop::launch`, story 6.6) gains `scope: Option<Vec<String>>` (`serde(default)`), set only for a governor run whose order sets `paths`, covered by `plan_sha256` and ignored by the entry point. Collection reads the scope from the registry's `workshop_run.plan_json`, never from the copy the container saw. An image built before this story is refused at launch by AD-99's image check (its `officina.version` label differs, `image_mismatch`), so no older entry point reads the new plan. Proof: core `crates/officina-core/src/workshop/launch.rs::tests::{the_scope_travels_in_the_plan_and_its_hash, a_clicked_plan_has_no_scope}`; the entry point's Linux test `crates/officina-container/tests/entry_linux.rs::a_plan_with_a_scope_runs_unchanged`; CLI `crates/officina/tests/governor.rs::{collection_reads_the_scope_from_the_registry, a_container_edit_of_its_plan_copy_changes_nothing}`; the image check's case for an older version label in `tests/vectors/workshop.json`.
2. **`out_of_scope`** (FR-12 as amended by amendment 8, AD-139, D-307, D-333). AD-111 step 6 checks both sides of every added, changed, deleted and renamed path against the run's scope with story 7.2's anchored, case-sensitive matcher; a path outside gives `QuarantineCause::OutOfScope` (no migration, D-310); the run settles refused and suspends its order (story 7.6). Proof: CLI `governor.rs::{a_package_outside_its_scope_is_quarantined_out_of_scope, a_rename_out_of_scope_counts_both_sides, a_case_difference_is_out_of_scope, a_scope_changed_in_the_catalogue_after_launch_does_not_apply}` (fake engine, real git, crafted packages); a case in the quarantine section of `tests/vectors/workshop.json`.
3. **Held, and every arrival journaled** (AD-136, FR-8, D-334, D-351; review F12). An autonomous run reaching state `review` while the Needs you count, this package excluded, is at or above the ceiling gets `held_at_ms`. Every autonomous package reaching `review` journals `package_arrived` (run, the count before it, excluding it, and `held`), which replaces AD-143's `package_held`. A package of a clicked run is never held and journals nothing new. Proof: CLI frozen-clock `governor.rs::{an_autonomous_package_at_the_ceiling_is_held, an_autonomous_package_below_the_ceiling_arrives_with_its_count, a_clicked_package_at_the_ceiling_is_not_held}`; core `relay/journal.rs::tests::package_arrived_round_trips`.
4. **The Needs you rule** (AD-136, AD-35 as amended, D-334). `view::workshop_cards` puts a run in Needs you only when it is in `review` and not held, or quarantined and clicked; held packages and autonomous quarantines are absent from `waiting_count`, the title, the overlay and the scene's waiting halo; a `no_change` run is listed in the Governor section, never held. Proof: core `crates/officina-core/src/board.rs::tests::{a_held_package_is_not_in_needs_you, an_autonomous_quarantine_is_not_in_needs_you, a_clicked_quarantine_still_is}`; cases in `tests/vectors/classification.json` under its coverage test; Vitest `app/src/lib/scene/layout.test.ts::a_held_package_draws_no_halo`.
5. **The self-stop writer** (AD-140, FR-14, D-335, D-345). `setting.governor_self_stop` (source `rules`, the `SelfStopCause` of story 7.4, its numbers) is written by one writer, journaled `governor_self_stopped`, and holds `decide` with `self_stopped`; `GovernorView` and D-1209's header slot, reading "governor stopped" in the terracotta of "relay stopped" (AD-35) with the cause in its tooltip and accessible name, show it; this story writes `held_expired`, story 7.16 the other causes and the rules that clear them. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::{a_self_stop_is_written_with_its_cause_and_numbers, a_self_stop_holds_the_decision}`; core `relay/journal.rs::tests::governor_self_stopped_round_trips`; Playwright `app/e2e/governor.spec.ts::a_self_stop_shows_the_terracotta_slot_in_both_themes`.
6. **Release and expiry, whatever the mode** (AD-136, FR-8 as amended by amendment 6, D-334, D-351). On every recompute, with the governor in any mode, while the count is below the ceiling, the oldest held packages are released one per free seat (`released_at_ms`, `package_released` with the count before the release); a package held for `HELD_EXPIRY_MS` (7 days) is discarded `expired` (v0.6's `DiscardReason::Expired`), journaled `package_expired` (run, `held_at_ms`), and self-stops the governor with `held_expired` through AC 5's writer. Proof: CLI frozen-clock `governor.rs::{held_packages_are_released_oldest_first_one_per_seat, a_package_is_released_with_the_governor_off, a_package_held_7_days_expires_and_self_stops, a_release_into_the_last_seat_reads_below_the_ceiling}`; core `relay/journal.rs::tests::{package_released_round_trips, package_expired_round_trips}`.
7. **The hold mutant** (D-338 as amended by review F10). Core feature `mutant-governor-skip-hold` (the Needs you rule admits an autonomous package at the ceiling), forwarded `officina` to `officina-host` to `officina-core` and covered by `MUTANTS_ENABLED`; the `mutants` job runs `cargo test -p officina --test governor --features mutant-governor-skip-hold --no-run` (must compile), then the run, which must fail with `an_autonomous_package_at_the_ceiling_is_held`; a control run passes. Proof: the CI job `mutants`.
8. **`Board.governor`** (AD-144, FR-17, P-63, D-330). `Board` gains `governor: Option<GovernorSummary { mode, state, held, self_stopped }>`, built by the host from the last step as an input of `build_board`, under story 7.14's `BOARD_SCHEMA`; `officina mcp`'s `board` tool carries it. Proof: core `board.rs::tests::the_governor_summary_counts_held_packages`; CLI `crates/officina/tests/mcp.rs::the_board_tool_carries_the_governor_summary`.
9. **In the window** (AD-145, FR-17). `HeldList.svelte` lists held packages oldest first, then autonomous runs ended without a package, `no_change` or quarantined, each with its cause, and Discard where it applies (`discard_package` serves held and autonomous quarantined packages); the held count shows in the section and the briefing only, never in the header, the title, the overlay or the scene (D-1209). Proof: Playwright `app/e2e/governor.spec.ts::{held_packages_list_oldest_first, the_held_count_is_not_in_the_header_or_the_title, a_held_package_can_be_discarded}`.
10. **Briefing** (AD-144, FR-17). Held packages, autonomous runs and their outcomes since the cursor, and a self-stop. Proof: core `crates/officina-core/src/orders/briefing.rs::tests::held_packages_and_autonomous_outcomes_since_the_cursor`.
11. **Doctor** (AD-147 Slice B, D-341). Held packages and their age. Proof: host `crates/officina-host/src/governor/doctor.rs::tests::held_packages_are_reported_with_their_age`.
12. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added. README "The governor (v0.7, in progress)": held packages, the ceiling, the 7-day expiry and its self-stop, `paths` as an allow-list matched from the project root, `out_of_scope`. Proof: review.
13. **The header slot** (D-1209, D-1210). One `data-stable-slot` of the header's status slot, sized for its longest label through `StableLabel`, holds "governor stopped" while a self-stop holds (this story) and "autonomy on" while the mode is on (story 7.16); nothing otherwise. Proof: a `stable-size.spec.ts` case for the slot in each state; `scene.spec.ts > eight projects fit a maximised Full HD window with the alert band` with the slot shown; D-163's computed-colour check in both themes.
14. **Reports are held like packages** (D-1219). A role run's report reaching `review` follows the same Needs you rule, hold, release and expiry as an autonomous package. Proof: CLI frozen-clock `crates/officina/tests/governor.rs::a_report_at_the_ceiling_is_held`.
15. **Journal writes** (D-1207). `package_arrived`, `package_released`, `package_expired` and `governor_self_stopped` are recorded. Proof: the round-trip tests of AC 3, AC 5 and AC 6; a spill case in `governor.rs`.

## Tasks

- [ ] `RunPlan.scope`; collection reads it from the registry (AC 1)
- [ ] Scope check at AD-111 step 6, `QuarantineCause::OutOfScope` (AC 2)
- [ ] Held at collection; `JournalKind::PackageArrived` (AC 3)
- [ ] The Needs you rule in `view::workshop_cards`; classification and scene cases (AC 4)
- [ ] The self-stop writer, the header slot's stopped state; `JournalKind::GovernorSelfStopped` (AC 5)
- [ ] Release and expiry on every recompute (AC 6)
- [ ] `mutant-governor-skip-hold` (AC 7)
- [ ] `Board.governor`; MCP case (AC 8)
- [ ] `HeldList.svelte`; briefing; doctor (AC 9 to 11)
- [ ] `CHANGELOG.md`, README (AC 12)
- [ ] The header slot; reports held; `record` for the lines (AC 13 to 15)

## Dev Notes

- Reworked 2026-11-02 (D-1207, D-1209, D-1210, D-1219): the header gets one slot for exceptions instead of the self-stop and held chips; a helper's report waits like a package. `Header.svelte` on `main` holds the status slot after the fixed controls (D-517) and shows exceptions only (D-679).
- Requirements: FR-8 as amended (amendment 6), FR-9's release rule, FR-12's Scope as amended, FR-14's self-stop record, FR-17, PRD section 7's Ceiling row. Decisions: AD-35, AD-55, AD-99, AD-111, AD-113 (syntax), AD-114, AD-136, AD-139, AD-140, AD-143, AD-144, AD-145. Epic decisions: D-148, D-163 (Epic 5); D-210, D-213 (Epic 6); P-60 (amendment 6's reading), P-63, P-65, D-300, D-307, D-310, D-330, D-333, D-334, D-335, D-338, D-340, D-341, D-345, D-351.
- Seams (D-300): AD-111 step 6 and the collection states (6.13), `RunPlan` (6.6), the image check (6.8), `view::workshop_cards` and `classification.json` (6.18), `discard_package` and `DiscardReason::Expired` (6.16), the scene's waiting halo for workers (6.23), `Header.svelte` (6.20).
- Waits for: story 7.14. Story 7.16 waits for this one (D-345); its tests and this story's seed `setting.governor_mode`.
- Holding moves review in time (FR-8); the review budget (FR-9, counted at launch) and suspension bound it. Releases depend on the ceiling only.
- Why releases run whatever the mode (D-334): a package held when the author turns the governor off would otherwise strand and expire, discarding work the quota already paid for.
- Why every arrival is journaled (D-351): the Ceiling row counts packages entering Needs you at or above the ceiling, which only held packages would otherwise record; the counts are taken before the change, so a release into the last seat reads below the ceiling.
- `RunPlan.scope` is author text from the approved catalogue; the worker may read it in `plan.json`, which tells it nothing it could not read in the floor, and nothing it writes there is read back.
- No employer content: placeholder paths and projects in the crafted packages.

## Testing

- Core unit and integration: `crates/officina-core/src/{workshop/launch,board,orders/briefing,relay/journal}.rs`, `crates/officina-core/tests/{workshop_vectors,classification_vectors}.rs`.
- Container: `crates/officina-container/tests/entry_linux.rs` on `core-linux`.
- Host unit: `crates/officina-host/src/governor/{writers,doctor}.rs`. CLI integration: `crates/officina/tests/{governor,mcp}.rs`.
- Frontend: Vitest `app/src/lib/scene/layout.test.ts`, Playwright `app/e2e/governor.spec.ts`. CI: `mutants`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: renumbered before mode on, which now waits for this story, and the self-stop writer moved here with `held_expired` (F5); `package_arrived` for every autonomous arrival, counts before the change (F12); `mutant-governor-skip-hold` (F10); collection reads the scope from the registry and older images are refused (F34); `RunPlan` tested in `workshop/launch.rs` (F30)
- 2026-11-02 reworked against v0.4 to v0.6 as built: one header slot instead of the chips (D-1209, AC 5, AC 9, AC 13); reports held like packages (D-1219, AC 14); `record` for the lines (D-1207, AC 15)
