# Story 7.8: The usage probe

**Status:** Approved

## Story

As the author whose statusline goes quiet when I stop typing,
I want Officina to ask Claude Code for my plan's usage through a contained `/usage` call it reads only as structured rows, never as text, within a budget that cannot degrade my own `/usage`,
so that the governor decides on numbers anchored minutes ago, backs off when the endpoint is rate limited, and falls back to the file when the probe breaks (FR-5).

## Acceptance Criteria

1. **The vector and its environment** (AD-134, FR-5 as amended by amendment 4, D-128, D-318). `VectorKind::UsageProbe` joins with its golden case in `tests/vectors/spawn.json`: AD-134's arguments, `/usage` a constant of core and the only prompt the kind carries, working directory `<home>/usage/empty`, no console, a kill-on-close job, stdin null, stdout capped at 256 KiB, 30 s, only an `.exe` accepted. Environment: D-128's contained-kind rule (`SESSION_ENV_REMOVED`, every `OFFICINA_*` name and `CLAUDE_CODE_DISABLE_AGENT_VIEW` removed), plus `CLAUDE_CODE_OAUTH_TOKEN` and `ANTHROPIC_API_KEY` removed, names compared ignoring ASCII case on Windows; the floor's `CLAUDE_CONFIG_DIR` and `NoDefaultCurrentDirectoryInExePath=1` set; `--bare` never passed. `UsageProbe` is a launch kind (`VectorKind::is_launch()`, D-318 as amended by review F21), so `officina.exe`, built without `window-authority`, cannot spawn it. The golden case is compared once with E60's recorded vector. Proof: the `UsageProbe` case through `crates/officina/tests/spawn.rs::{every_kind_reaches_the_fake_claude_as_its_golden_vector, contained_kinds_drop_the_officina_prefix, the_usage_probe_drops_both_credential_names}` (a seeded base environment with both names in mixed case; the fake claude logs names only, D-129); D-128's without-feature host unit test, which iterates `VectorKind::ALL`, refuses `UsageProbe` in the lone-package step `cargo test -p officina-host --lib spawn`.
2. **The folder** (AD-134, D-318). The window creates `<home>/usage/empty`; a probe whose folder holds anything is refused before any process starts. Proof: `spawn.rs::a_usage_probe_in_a_non_empty_folder_is_refused_before_spawning`.
3. **Reading** (AD-134, D-319). `governor::probe::read(stdout, exit_code, timed_out) -> ProbeOutcome`, permissive line parsing, outcomes in order: structural failure (`ProbeFailure`, a `closed_enum!`: non-zero exit, deadline, output cap, no `result` line, a model turn as amendment 4 defines it, no `usage_report`, `rate_limits` null with no degraded marker, `limits` empty or null, no `session` row, no `weekly_all` row, a duplicated `session` or `weekly_all` row, more than 8 `weekly_scoped` rows (`TooManyScopedRows`, D-352, review F28), a percent outside 0 to 100, an unparsable `resets_at`); back-off; observation anchored at the probe's end. A whole-point `percent` p becomes the interval [p * 1000, p * 1000 + 999] thousandths, or [p * 1000 - 500, p * 1000 + 499] once E61 shows the server rounds (D-322, review F16); conditions read the upper bound. Rows are classed by `kind`, never by a label; a `session` row with `resets_at` null means no active 5-hour window; any other kind is counted and ignored. `dates` parses ISO 8601 with `Z` or a numeric offset and optional fractional seconds. Proof: the `probe` section of `tests/vectors/governor.json`, lines built from the addendum's section 1 shape with synthetic values, through `crates/officina-core/tests/governor_vectors.rs::{probe_cases_match, every_probe_failure_has_a_case}` (with a whole-point reading's interval asserted); core `crates/officina-core/src/dates.rs::tests::iso_8601_with_z_offset_and_fractions`.
4. **Degraded markers and the text** (AD-134, D-319, R79). `USAGE_DEGRADED_MARKERS` is seeded from S3's two phrases and E60's observed ones; the report's text is compared case-insensitively against each entry and dropped, never parsed for a number, stored, journaled or shown. Proof: `governor_vectors.rs::every_degraded_marker_backs_off_in_any_case` (iterates the constant, case variants derived programmatically); CLI `crates/officina/tests/governor.rs::the_report_text_never_leaves_memory`, which iterates every `usage` variant of the fake claude, seeds a marker in its stdout and its stderr, and after each searches every file under the test home, every journal line, every registry row and every IPC payload (D-319 as amended by review F19).
5. **The canary** (AD-12 as amended, D-319). `USAGE_FIELDS` become the contract canary's `usage` surface in `state/contract.json`, checked on each probe; a missing field is a structural failure and the canary records it. Proof: core `crates/officina-core/src/contract.rs::tests::the_usage_surface_is_derived_from_usage_fields`; CLI `governor.rs::a_probe_missing_a_field_is_recorded_by_the_canary`.
6. **Budget, back-off, pause** (AD-134 as D-346 amends it, FR-5, D-303, D-319; review F9). `probe::budget(lines, now_ms, offset)` counts the ledger's `usage_probe` lines for every purpose alike: the last probe at least 2 minutes ago, fewer than 8 in the last 3,600 s, fewer than 60 since local midnight, or E60's lower numbers. The budget is checked when a job is dispatched and again when a queued job is about to spawn; a job refused at spawn time spawns nothing. A back-off stops probes for `BACKOFF_MS` (60 minutes) and never counts toward the pause. Three structural failures in a row with one `claude` stamp (path, size, modification time, D-66) pause the probe until the stamp changes. All three are folds over lines, so they survive a restart. Proof: budget cases across a local midnight and across a daylight saving change in the `probe` section; CLI frozen-clock `governor.rs::{the_probe_budget_holds_across_a_restart, a_queued_job_refused_at_spawn_time_spawns_nothing, a_back_off_lasts_60_minutes_and_never_pauses, three_structural_failures_pause_until_the_stamp_changes, a_pause_survives_a_restart}`.
7. **The worker** (AD-127, AD-3 as amended). Thread `governor-probe` (`window-authority`) runs one probe at a time through `spawn::execute`; a job asked while one runs waits for it; the outcome returns as an `AuthorRequest` without deadline, never dropped; the board never waits. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{the_board_keeps_its_pace_while_a_probe_hangs, a_second_probe_job_waits_for_the_first}`.
8. **The ledger** (D-348, review F14). `officina-host::governor::ledger` holds the journal folds the governor reads beyond the relay's two days, starting with this story's: the probe lines since the last clean probe and those of today. The window seeds it once at start from up to 90 days of journal files and updates it with each line it appends; the CLI's read half seeds its own. Stories 7.9, 7.11 and 7.14 extend it. Proof: host unit `crates/officina-host/src/governor/ledger.rs::tests::{a_seeded_ledger_equals_a_full_re_read_after_a_restart, an_appended_line_updates_the_fold, ninety_days_bound_the_seed}`.
9. **Journal** (AD-143). `usage_probe`: outcome, failure or back-off cause, `asked_by` (`decision`, `live_guard`, `bracket`), duration, exit code, `total_cost_usd`, `num_turns`, the stamp's fingerprint, `backoff_until_ms`, `anchored_ms` and the values when an observation resulted; never text. Proof: core `crates/officina-core/src/relay/journal.rs::tests::{usage_probe_round_trips, usage_probe_carries_no_text_field}`.
10. **The fake** (D-129, D-339). `officina-fake-claude` gains mode `usage` with variants clean, degraded (its text built from an entry of `USAGE_DEGRADED_MARKERS`), null `rate_limits`, empty `limits`, missing row, model turn, exit 1, hang and oversized, each printing lines in the addendum's shape with synthetic values. Proof: `spawn.rs::each_usage_variant_prints_its_shape`.
11. **Ingestion exclusion** (D-318, D-166's rule). AD-43's exclusion gains exactly `<home>/usage/empty`. Proof: host `crates/officina-host/tests/registry_ingest.rs::{usage_workdir_lines_are_skipped, other_lines_under_the_usage_folder_are_recorded}`.
12. **Zero write** (D-80). A probe through the fake claude joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
13. **Doctor** (AD-147, D-341). The probe's availability, last outcome, back-off, pause with its stamp, and today's count; without the probe, that the scoped weekly reserve is not watched (review F32); `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` named when set in doctor's environment (a warning); `<home>/usage/empty` present and empty (fails otherwise). Doctor runs no probe. Proof: host `crates/officina-host/src/governor/doctor.rs::tests::{the_probe_state_is_read_from_the_journal, nonessential_traffic_off_warns, a_non_empty_usage_folder_fails, without_the_probe_the_scoped_reserve_is_named_unwatched}`.
14. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: the usage probe, its budget, never its text. README "The governor (v0.7, in progress)": what the probe is, that it loads no user or project configuration, that it answers for the floor's own login, its budget and back-off, that without it the governor acts only while an interactive session keeps the statusline fresh and the scoped weekly reserve is not watched, naming `ProbeFailure` and `USAGE_DEGRADED_MARKERS` without listing them. Proof: review.

## Tasks

- [ ] `VectorKind::UsageProbe`, golden case, environment rule, `<home>/usage/empty` (AC 1, 2)
- [ ] `crates/officina-core/src/governor/probe.rs`: `read`, `ProbeFailure`, `USAGE_FIELDS`, `USAGE_DEGRADED_MARKERS`, `budget`; ISO 8601 in `dates`; the `probe` section (AC 3, 4, 6)
- [ ] Canary `usage` surface (AC 5)
- [ ] `crates/officina-host/src/governor/probe.rs`: thread `governor-probe`, `AuthorRequest` variants, dispatch and spawn-time budget checks (AC 6, 7)
- [ ] `crates/officina-host/src/governor/ledger.rs` (AC 8)
- [ ] `JournalKind::UsageProbe` (AC 9)
- [ ] Fake claude `usage` mode (AC 10)
- [ ] Ingestion exclusion; zero write; doctor (AC 11 to 13)
- [ ] `CHANGELOG.md`, README (AC 14)

## Dev Notes

- Requirements: FR-5 as amended (amendment 4), FR-18's probe items, PRD section 7's Probe row, section 8 question 1. Decisions: AD-3, AD-12, AD-43, AD-50, AD-52, AD-79, AD-80, AD-127, AD-134, AD-143. Facts: addendum section 1, items 1 to 5; S2, S3, S6; E60, E61. Risks: R66 to R70, R79, R80. Epic decisions: D-66, D-80, D-86 (Epic 3); D-128, D-129, D-166 (Epic 5); P-60 (FR-5's live probe against amendment 9), P-65, D-300, D-303, D-304, D-318, D-319, D-322, D-339, D-340, D-341, D-346, D-348, D-352.
- Seams (D-300): the spawn module, `VectorKind` and `SESSION_ENV_REMOVED` (5.2), the fake claude (D-129), the contract canary (1.11), AD-43's exclusion (3.2, 5.13, 5.16), the journal reader (2.5), `quota_doctor.rs` (D-80).
- Waits for: story 7.1 (E60, E61) and story 7.7 (the observation type). Built only on an `oauth-token` floor, and only when E60 and E61 pass (D-303, D-304); otherwise this story stays unimplemented and story 7.10 builds `one_per_window` as the only path.
- The kind removes both credential names because a `setup-token` token lacks the profile scope the report needs (addendum section 1, documented): the probe answers for the floor's own interactive login.
- Who asks for probes is story 7.10's decision, story 7.9's brackets and story 7.16's live guard; this story builds the call, its reading, its budget and its thread.
- A marker can only turn numbers into a back-off, so a wrong match errs toward caution (AD-134).
- Why 8 probes an hour (D-346): the live guard alone takes 6 an hour during a run, and a long run's second bracket observation and the next decision probe need room; E60's burst hours exercised 11.
- Why the ledger lands here (D-348): the pause fold reaches back to the last clean probe and the daily count to local midnight, beyond the relay's two-day reader; the step runs at every recompute, so the folds are kept, never re-read.
- No employer content in fixtures: synthetic percents, resets and scope labels.

## Testing

- Core unit and integration: `crates/officina-core/src/{governor/probe,dates,contract,relay/journal}.rs`, `crates/officina-core/tests/{governor_vectors,spawn_vectors}.rs`.
- Host: `crates/officina-host/src/{governor/probe,governor/ledger,governor/doctor}.rs`, `crates/officina-host/tests/registry_ingest.rs`.
- CLI integration: `crates/officina/tests/{spawn,governor,watch,quota_doctor}.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the probe is a launch kind (F21); whole-point readings as intervals (F16); at most 8 scoped rows (F28); the R79 search over every variant and both streams (F19); hourly cap 8 with dispatch and spawn-time checks (F9); the ledger (F14); the unwatched scoped reserve named without the probe (F32)
