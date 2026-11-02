# Story 7.20: The two-week measure and the close decisions

**Status:** Approved

## Story

As the author who let the governor run because the shadow said it could,
I want its exit criteria and its value read from my own journal over two weeks with launches on, and each starting value the readings contradict changed on that evidence,
so that I know whether the reserve was never entered, no launch ran on stale quota and the antechamber never went over its ceiling, and whether the governor returns more attention than it costs (PRD section 7, Slice B rows; framing's exit criteria).

## Entry conditions

Implemented only once story 7.13's Slice B decision entry records **go** (D-331; the conditions are listed in story 7.14), after story 7.19. The measure starts the first day the governor is on with at least one standing order eligible, runs 14 consecutive days, alongside v0.8, and is read at the latest at v1's close (PRD section 6.1.1, P-62).

## Acceptance Criteria

1. **The table in code** (PRD section 7, D-331's pattern). `governor::evidence::slice_b_table(lines, rows, days, from_ms, to_ms, offset)` computes each Slice B row from the journal and the registry: Reserve (launches whose recorded projection plus reserve plus allowance exceeded 100, or with a scoped weekly row at or above 100 minus the reserve; readings at or above 100 minus the reserve while an autonomous run was live or within 10 minutes after its end without that run stopped within 60 s, from `quota_observed`, `usage_probe`, `workshop_started` and `workshop_finished`; at each weekly reset, the last observation before it and that week's autonomous points); Fresh quota (launches on an observation older than 30 minutes, lacking the weekly window, past a reset, or older than the previous autonomous run's end; with the probe, launches without a probe anchored under 2 minutes before); Ceiling (launches, `package_arrived` lines not held, and `package_released` lines whose count before the change was at or above the ceiling; D-351, review F12); Weekly cap (from this week's autonomous points as D-343 computes them, never from a worker's reported cost; review F1); Gesture (every governor-launched `workshop_started` matched to a `governor_launch` line on an order of the approved snapshot of that time, and no other launch without a click); Window (launches outside the window, autonomous runs live 60 s past the window's end or the reset their deadline was cut to); Use (over the first 10 days of the window with the governor on, due launches of eligible orders that happened before the window's end that day, and autonomous runs that reached a verdict); Throughput (autonomous packages imported per week, merged beside them; the reported cost shown beside as the worker's estimate, P-64); Autonomous acceptance; Review time (launches with spent plus pending plus estimate above the budget; actual autonomous review minutes per day); Held (expiries). `crates/officina-host/examples/governor_metrics.rs` prints it. Proof: core `crates/officina-core/src/governor/evidence.rs::tests::{the_reserve_row_reads_each_weekly_reset, a_reading_after_a_run_end_counts_10_minutes, a_launch_on_a_31_minute_observation_fails_fresh_quota, a_release_at_the_ceiling_fails_ceiling, use_counts_due_launches_inside_the_window_only, throughput_counts_imports_per_week, an_arrival_at_the_ceiling_fails_ceiling, a_release_into_the_last_seat_passes_ceiling, a_forged_reported_cost_moves_no_weekly_cap_figure}`, each row's case seeded from lines the host tests wrote.
2. **The readings** (PRD section 7). After 14 consecutive days with the governor on and at least one order eligible, the agent runs the example and records the table in a research notes entry: the framing's exit criteria (Reserve, Fresh quota and Ceiling rows) each with its target, and every other Slice B row. One regime is built (D-1204), so every row applies. The Stop row cites the v0.7 gate 5 entry's second half. Proof: the research notes entry.
3. **The close decisions** (PRD section 7, section 8 questions 9 and 14, D-331's rule for constants). Each recorded in the same entry with its reading: Throughput under 1 import in each of the two weeks: the owner decides whether the governor stays on; Autonomous acceptance under 60 %: `acceptance_runs` goes up by the owner's floor edit; Review time over budget on more than 2 of 10 days: the default review estimate goes up; any expiry: read against the ceiling and the cadences; question 9: a template whose acceptance differs by more than 30 points between two projects with 5 settled runs each is named; question 14: each starting value the shadow or the measure contradicts is named with its reading. A code constant that changes does so in the same commit as its vector cases, with the fact added to `epic-7-decisions.md`; a floor key changes only by the owner's edit and approval (PRD section 5: thresholds do not learn). Proof: the research notes entry; in the change case, the vector cases.
4. **QA, the measure** (D-342). `docs/qa/epic-7-report.md` gains a section with the table, the close decisions, and any finding moved to `docs/backlog.md`. Proof: review of the report.

## Tasks

- [ ] `evidence::slice_b_table`; the example's Slice B table (AC 1)
- [ ] 14 days of use with the governor on; the research notes entry (AC 2)
- [ ] The close decisions with the owner; constant changes with their cases (AC 3)
- [ ] The report's measure section (AC 4)

## Dev Notes

- Reworked 2026-11-02 (D-1204, D-1216, D-1218): one regime, so no reading per USD; the measure runs on the personal floor's machine; it runs alongside v0.8 or after it, whichever order the owner keeps (D-1218), and is read at the latest at v1's close.
- Requirements: PRD section 7's Slice B table and its closing paragraph, section 6.1.1's exception, section 8 questions 9 and 14. Decisions: AD-143 (the lines it reads), AD-148's manual row. Epic decisions: P-16 (Epic 3), P-25 (Epic 5); P-62, P-64, P-65, D-304, D-331, D-342, D-343, D-351.
- Dependencies: story 7.19 done; the measure period on a build of `main` holding Epic 7, on the home PC (D-1216).
- `slice_b_table` is the executable form of section 7's "How measured": the numbers in the notes and the report come from it, never from a hand count. Every `governor_launch` line carries its whole input (AD-143), which is what lets the Reserve and Fresh quota rows be read per launch.
- This story does not hold v0.8 (P-62): v0.8 starts when story 7.19 closes.
- No employer content in the notes: placeholder orders and projects.

## Testing

- Core unit: `crates/officina-core/src/governor/evidence.rs`.
- Manual: the research notes entry (AC 2, 3), the report's section (AC 4).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the Ceiling row reads arrivals and releases with counts before the change (F12); the Weekly cap and per-USD rows read trusted figures only (F1, P-64)
- 2026-11-02 reworked against v0.4 to v0.6 as built: one regime, no USD reading (D-1204); the home PC (D-1216); either order with v0.8 (D-1218)
