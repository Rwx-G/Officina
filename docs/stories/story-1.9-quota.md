# Story 1.9: Quota

**Status:** Approved

## Story

As the author deciding whether to start a new subject,
I want the 5-hour and weekly quota with reset times and freshness in the header,
so that I never start something I cannot finish, and I never trust a number without knowing its age.

## Acceptance Criteria

1. `officina quota write` reads a statusline payload on stdin, writes `quota/<account key>.json` atomically and prints nothing (FR-5, AD-8). Proof: CLI tests `crates/officina/tests/quota.rs::{writes_quota_file_and_prints_nothing, file_parses_with_core_parser}`.
2. `--tee` copies stdin to stdout byte for byte, including invalid JSON and non-UTF-8 bytes, and still writes when the payload is valid (AD-16). Proof: CLI tests `quota.rs::{tee_is_byte_identical_for_valid_payload, tee_is_byte_identical_for_garbage}`.
3. The account key follows `CLAUDE_CONFIG_DIR` as story 1.2 defines it. Proof: CLI tests `quota.rs::{no_config_dir_writes_default, config_dir_writes_cfg_key}`.
4. A window absent from the payload is absent from the file; a payload with neither window writes nothing. Proof: CLI tests `quota.rs::{missing_seven_day_is_omitted, no_rate_limits_writes_nothing}`.
5. The write is skipped when both windows are identical and the file is younger than 60 s (AD-10). Proof: host test `crates/officina-host/tests/quota_store.rs::identical_windows_within_60s_skip_the_write` (file mtime unchanged).
6. The atomic write uses a same-directory temp file, `sync_all`, rename with three retries at 10 ms, then deletes the temp file on failure (AD-10); `quota write` still exits 0 and, with `--tee`, still forwards stdin. Proof: host test `quota_store.rs::rename_blocked_by_foreign_handle_gives_up_and_removes_temp` (Windows: target held open without share-delete), CLI test `quota.rs::write_failure_exits_0_and_tees`.
7. The collector re-reads the quota file on mtime change without spawning `claude` (AD-4). Proof: host test `crates/officina-host/tests/collector.rs::quota_change_updates_board_without_spawn` (fake claude log unchanged).
8. `Board.quota` carries the windows, `written_at_ms` and `stale_after_ms` (AD-5, AD-13), and ts-rs types are regenerated. Proof: core unit test `board::tests::quota_section_from_file`; CI ts-rs diff step.
9. The header shows two gauges with value, reset time, explanation (weekly elapsed share) and freshness, and the states "no data", "stale" beyond the threshold, and "reset, no new data" when `resets_at` has passed (FR-5, FR-8). Proof: Vitest `src/lib/components/Gauge.test.ts > {value and reset time, weekly elapsed share, no data, stale, reset no new data}`; Playwright `app/e2e/quota.spec.ts > each quota state from generated fixtures` with `page.clock` frozen.
10. Every gauge always carries an age or "no data" (PRD section 7 metric). Proof: Playwright `quota.spec.ts > every gauge shows an age or no data in every fixture`, iterating all fixtures.
11. Doctor reports a quota file fresher than the threshold or prints the statusline snippet (`officina.exe quota write --tee | <existing command>`, recommended in a personal uncommitted settings file); quota freshness becomes a required check. Proof: CLI tests `doctor.rs::{fresh_quota_is_green, stale_quota_prints_snippet_and_exits_1}`.
12. Workstation: the statusline wired with `--tee` displays unchanged; whether a statusline runs for `--bg` sessions and which of user or project statusline wins are observed and recorded in research notes (E3, PRD section 8). Proof: manual.

## Tasks

- [ ] `officina_host::atomic::write` (temp, sync, rename retry, cleanup) (AC 6)
- [ ] `officina_host::quota::{write_from_statusline, read}` with the skip rule (AC 1, 3, 4, 5)
- [ ] `officina quota write [--tee]` (AC 1, 2, 6)
- [ ] Collector quota trigger (AC 7)
- [ ] `Board.quota` section, ts-rs regeneration (AC 8)
- [ ] `Gauge` component in `Header`; `e2e:fixtures` scenarios seeding quota files (fresh, stale, reset, absent) (AC 9, 10)
- [ ] Doctor quota check and snippet (AC 11)
- [ ] Workstation observations (AC 12)
- [ ] Update `CHANGELOG.md` `[Unreleased]`

## Dev Notes

- Requirements: FR-5 (as amended, item 8), FR-8, journey 3. Decisions: AD-4, AD-5, AD-8 (format, seconds, account key), AD-10 (atomic write, skip rule), AD-13, AD-16 (`--tee`). Facts: E3. Risk: R10.
- Thresholds travel in the board (AD-5); the frontend never retypes the 30 min value.
- Quota file `written_at` is seconds, `Board.quota.written_at_ms` is milliseconds: convert once in core.
- Weekly elapsed share is computed from `resets_at` minus seven days, in the frontend from absolute values.
- `quota write` runs on every statusline tick: no logging, no stdout without `--tee`.

## Testing

- CLI integration: `crates/officina/tests/{quota,doctor}.rs`.
- Host integration: `crates/officina-host/tests/{quota_store,collector}.rs`.
- Core unit: `board.rs`.
- Vitest: `Gauge.test.ts`. Playwright: `app/e2e/quota.spec.ts`.
- Manual: workstation statusline observations.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
