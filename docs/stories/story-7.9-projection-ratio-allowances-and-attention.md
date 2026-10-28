# Story 7.9: Projection, measured points, allowances, the weekly cap and the attention inputs

**Status:** Approved

## Story

As the author who keeps a quarter of my week for myself and can only review so much in a day,
I want the governor to project my weekly use from the pace of my working days, to know from my own account's report what one run of a template takes from my windows, to cap its own share of the week, and to count the review time it creates,
so that it never plans to enter my reserve, never takes more than its share whatever the projection's error or anything a worker prints, and never launches work I have no time to review (FR-6, FR-7, FR-8, FR-9, J1, J2).

## Acceptance Criteria

1. **Integer arithmetic** (AD-135, FR-6 as amended by amendment 5, D-322 as amended by review F6 and F16). Points are `i64` thousandths, money `i64` micro-USD, cents converted exactly; the ratio is thousandths of a point per USD, a sample being `ceil(rise_milli * 1_000_000 / cap_micro_usd)`; projections, allowances, points and ratio samples all round up. A probe's whole-point reading is an interval (story 7.8) whose upper bound conditions read. Proof: the `projection` section of `tests/vectors/governor.json`, with every comparison of AD-137's step 11 at its exact boundary (100.000 passes, 100.001 holds), file values 64.9 and 69.9, whole-point probe values, and the ratio product at its largest rise, through `crates/officina-core/tests/governor_vectors.rs::projection_cases_match`.
2. **Projection** (AD-135, FR-6). `budget::projection(used_milli, &Share)`: `used` over story 7.3's `listed_share`, counting every use of the account. Proof: `projection` cases including the requirement challenge's uneven week (a Wednesday reset, weekdays only) and a projection just after a reset (the one-listed-day minimum).
3. **Brackets and samples** (AD-135, D-320, D-343; review F1, F6, F9). Core `budget::bracket(&Bracket) -> BracketReading` per window: valid with its rise bound (the second observation's upper bound minus the first's lower bound), or not with its cause (`no_pre_probe`, `no_post_probe`, `reset_crossed` when the window's `resets_at` differs between the two observations); a valid bracket is a clean sample unless `other_working`, `restart`, `rise_too_small` (under 2 points) or `cost_unbounded` (no enforced cap, or a cap of 0, which never divides) holds. The ratio in use per window is the largest of the last 5 clean samples of the last 90 days, absent before any. The worker's reported cost is never an input: the bracket's types carry no cost field. Proof: `projection` cases for each cause, for a cap of 0, and for a run whose evidence carries a reported cost of 0 and of a large value with the same reading; coverage over the sample causes' `ALL` in `governor_vectors.rs::every_sample_cause_has_a_case`.
4. **Brackets in the host** (AD-134, D-320, D-346, D-348; review F9). While the governor is shadow or on: a bracket's first observation is the newest probe observation anchored at or before the worker's start (`workshop_started`) and at most 5 minutes before it, which for an autonomous launch is the decision probe it rested on; a probe is also asked when a launch begins (`asked_by: bracket`) when the budget allows, the launch never waiting for it; the second is a probe asked at `workshop_finished`, anchored within 15 minutes after it; the collector keeps, per live run, whether another card of the floor was in Working, from each board; `ratio_sample` (run, the cap in micro-USD, both anchors, per window the rise bounds, valid, clean, cause; never the reported cost) is journaled per bracket, and the ledger (D-348) keeps the samples of 90 days. Proof: CLI frozen-clock `crates/officina/tests/governor.rs::{a_clicked_run_is_bracketed_in_shadow, a_decision_probe_serves_as_the_first_observation, a_probe_ending_after_the_worker_started_is_not_a_first_observation, a_60_minute_run_gets_its_second_observation_within_the_budget, another_working_card_makes_a_sample_unclean, a_reset_inside_the_bracket_gives_reset_crossed, a_restart_makes_a_live_sample_unclean, no_bracket_while_the_governor_is_off}`; core `crates/officina-core/src/relay/journal.rs::tests::{ratio_sample_round_trips, ratio_sample_carries_no_reported_cost}`.
5. **Points and allowances from trusted figures** (P-64, D-343, D-321; review F1, F38). A finished run's points per window: its valid bracket's rise bound; a finished autonomous run without a valid bracket counts the allowance it was launched with. A template's allowance per window: the largest rise bound of its last 5 clean samples, clicked or autonomous; before any clean sample, `max(START_ALLOWANCE, max_usd * ratio)`, the second term only when both exist, `START_ALLOWANCE` being, per window, the larger of the addendum's starting value (3 weekly points, 10 five-hour points) and E62's measured rise bound. Each allowance carries its source (`measured`, `max_usd`, `starting`). This weekly window's autonomous points: finished autonomous runs' points, live runs' launch allowances, and in shadow each booking's allowance. Proof: `projection` cases per source, for each component of the weekly points, and for the forged-cost cases of AC 3 (a forged low or high reported cost changes no point and no allowance).
6. **The four quota conditions** (AD-135, AD-137 step 11). `budget::weekly_projection`, `scoped_weekly` (rows classed by kind, whatever the template's model), `autonomous_week_cap` (`autonomous_week_max_pct`) and `five_hour_room` (`five_hour_max_pct`) each return pass or their typed numbers, reading the upper bound of every reading. Proof: `projection` cases at each boundary.
7. **The other regime** (D-304, D-343, FR-7; review F1). When E62 recorded `api-key`, this story builds `budget::usd_budget` instead of AC 2 to 6: every autonomous run launched in this local week, finished or live, counts its enforced cap (`max_usd`) as spent, plus this template's cap, at most `usd_per_week`, the week starting at local midnight of the first listed day; the reported cost is never read. An order whose template has no enforced cap is never a candidate (`MaxUsdRequired`, story 7.2). Proof, in that branch only: `usd` cases in the `projection` section, a forged reported cost among them.
8. **Attention** (AD-136, FR-8, FR-9, D-323 as amended by review F25). `attention::review(...)` from the ledger's `review_opened` and verdict lines: a package's minutes (rounded up, at most 30, 1 for a Discard without Receive, 30 for an Import without Receive), spent today since local midnight for autonomous packages decided by the author, a template's estimate (median of its last 5 author verdicts with the upper middle for an even count, 10 until 3 exist), pending (autonomous packages in Needs you or held, live autonomous runs, and in shadow today's bookings); `expired` and `not_collected` count neither in spent nor in the estimate. `attention::crossings(&[CountSample], &WindowState)` gives the `ceiling_crossed` lines: one per crossing inside the window and one at each window start. Proof: the `attention` section through `governor_vectors.rs::attention_cases_match`, with an `expired` package that moves nothing, an Import without Receive, and spent today across a daylight saving change.
9. **Doctor** (AD-147, D-341; review F31). The regime built; the ratios in use per window with their sample counts; each template's allowance with its source; the starting allowance when none; FR-18's "gate 3's recorded outcome" read through those sources. Proof: host `crates/officina-host/src/governor/doctor.rs::tests::{the_regime_and_ratio_sources_are_reported, allowances_report_their_source}`.
10. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added. README "The governor (v0.7, in progress)": the projection over listed days, the reserve, the weekly autonomous cap that holds whatever the projection's error, that points and allowances come from the account's own report around each run and a worker's reported cost is shown, never trusted, the API-key budget counted at each run's cap, the review budget. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/governor/budget.rs`: arithmetic, projection, brackets, samples, ratio, points, allowances, weekly points, conditions; `projection` section (AC 1 to 3, 5, 6)
- [ ] Brackets in the host, the per-run Working flag, the ledger's samples; `JournalKind::RatioSample` (AC 4)
- [ ] `usd_budget` in the api-key branch only (AC 7)
- [ ] `crates/officina-core/src/governor/attention.rs`; `attention` section (AC 8)
- [ ] Doctor; `CHANGELOG.md`, README (AC 9, 10)

## Dev Notes

- Requirements: FR-6 as amended (amendment 5), FR-7, FR-8 (the ceiling computed as an input), FR-9 (the budget computed in shadow), FR-18, PRD section 7's Reserve and Weekly cap rows (read in story 7.20). Decisions: AD-109 (stream lines are display only), AD-129, AD-134, AD-135, AD-136, AD-137. Facts: E61 (floor or round of whole points), E62 (the first measured rise, the stream's events). Risks: R71, R72. Epic decisions: D-222 (Epic 6); P-61, P-64, P-65, D-300, D-304, D-310, D-320, D-321, D-322, D-323, D-340, D-341, D-343, D-346, D-348.
- Seams (D-300): the workshop launch and `workshop_finished` (6.11, 6.13), `review_opened` and the verdict lines (6.16, 6.18), `Board.waiting_count` and the Working column (1.4, 6.18).
- Waits for: stories 7.3, 7.4 (`GovernorMode`, D-310), 7.7 and 7.8 (an `oauth-token` floor); on an `api-key` floor, stories 7.3, 7.4 and E62 only (D-304).
- Why no reported cost (P-64): any process in a container can print a `result` line (v0.6 AD-109), so a forged low cost would shrink the weekly cap's count and the allowances, and a forged high one would lower a ratio. The account's own report before and after a run is the trusted measure; other use during the run only raises it, which errs toward caution. The ratio divides by the enforced cap, so it is a lower bound, and it only ever raises a starting allowance.
- Why the first observation may be 5 minutes old (D-320, D-346): a launch rests on a probe of the last 2 minutes, which the 2-minute spacing would refuse to repeat at launch; a longer gap before the worker starts only widens the rise. A probe ending after the worker started is never a first observation.
- `ceiling_crossed` lines are written by story 7.11's step from this function; the share under the ceiling is a fold over them.
- No employer content: placeholder templates and caps.

## Testing

- Core unit and integration: `crates/officina-core/src/{governor/budget,governor/attention,relay/journal}.rs`, `crates/officina-core/tests/governor_vectors.rs`.
- Host unit: `crates/officina-host/src/governor/doctor.rs`. CLI integration: `crates/officina/tests/governor.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the worker's reported cost gates nothing; points and allowances come from bracket rise bounds, the ratio from the enforced cap, `usd_budget` from caps (F1, P-64); the ratio's unit, upward rounding, `reset_crossed` and `cost_unbounded` (F6); the decision probe serves as the first observation within 5 minutes, and long runs keep their second within the hourly cap of 8 (F9); `GovernorMode` from story 7.4 (F8); automatic discards excluded from the review budget, an Import without Receive counts 30 (F25); the regime and sources in doctor (F31); "per window" wording (F38)
