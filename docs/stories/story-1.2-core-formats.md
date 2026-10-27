# Story 1.2: Core formats completion

**Status:** Approved

## Story

As the developer of Officina,
I want every file format and the date and contract primitives in `officina-core` aligned with the spine,
so that the hook, the collector, doctor and the canary all build on one tested definition.

## Acceptance Criteria

1. The account key follows AD-8: `default` when no config dir override applies, otherwise `cfg-` plus the FNV-1a 64 hex of the normalized path. Proof: unit tests `quota::tests::{no_override_gives_default, override_gives_cfg_prefix_and_16_hex, normalisation_ignores_case_separators_and_trailing_slash}`.
2. `QuotaFile.written_at` is in seconds, as AD-8 states; `from_statusline` takes the current time and stores seconds. Proof: unit test `quota::tests::written_at_is_seconds`.
3. `parse_quota_file` returns "unknown" (no value) on another schema version and never panics on arbitrary input. Proof: unit tests `quota::tests::{other_schema_is_unknown, garbage_is_unknown}`.
4. Event line building follows AD-8: `prompt`, `last_message` and `message` cut to `EXCERPT_CHARS` on a char boundary; a serialized line over the 4096-byte cap drops the excerpts and sets `truncated: true`; absent fields are omitted, never `null`; `OFFICINA_NO_EXCERPTS` semantics are a parameter of the reducer, not an env read in core. Proof: unit tests `events::tests::{message_is_cut_like_excerpts, multibyte_cut_stays_on_char_boundary, oversized_line_drops_excerpts_and_sets_truncated, line_never_exceeds_cap, absent_fields_are_omitted, no_excerpts_drops_prompt_and_last_message}`.
5. `claude_pid` is reserved in event line v1 (AD-11): optional, parsed when present, omitted when absent. Proof: unit test `events::tests::claude_pid_round_trips_and_is_optional`.
6. `parse_event_lines` returns the parsed lines and the count of unparsable or foreign-version lines, so the reader can surface it in `board.diagnostics` (AD-10). Proof: unit test `events::tests::skipped_lines_are_counted`.
7. A `FloorFile` type exists per AD-8: `deny_unknown_fields`, schema checked, `ProjectColor` enum, optional floor-level `config_dir`; parse errors carry line and column from the toml error. Proof: unit tests `floor::tests::{parses_minimal_floor, unknown_key_is_an_error_with_line, unknown_color_is_an_error_with_line, other_schema_is_rejected}`.
8. UTC date math (AD-9): civil date from Unix ms, event file name for a timestamp, previous day. Proof: unit tests `dates::tests::{epoch_is_1970_01_01, leap_days, century_rules_2000_and_2100, day_boundary_at_midnight_utc, round_trip_over_a_wide_range}` (the round trip iterates day numbers; no table of dates is transcribed).
9. `core::contract` holds the constants AD-12 describes and a pure `check(entries, events) -> ContractResult { held, missing, not_observed }`. Proof: unit tests `contract::tests::{full_sample_holds, removing_each_always_field_reports_it_missing, unseen_conditional_field_is_not_observed}`, each iterating the constants rather than retyping them.
10. Core stays pure (story 1.1 AC 2) and all previous core tests still pass. Proof: CI gates on `core-linux`.

## Tasks

- [ ] Change `account_key` to take the optional override and return `default` or `cfg-<hex>`; update tests (AC 1)
- [ ] Store seconds in `written_at`; rename the parameter to make the unit explicit (AC 2)
- [ ] Add schema and garbage tests for `parse_quota_file` (AC 3)
- [ ] Extend `reduce_payload` with the `message` cut, the 4096 cap and `truncated`, and a `keep_excerpts: bool` parameter; add the cap as a named constant (AC 4)
- [ ] Add optional `claude_pid` and `truncated` to `EventLine` (AC 4, 5)
- [ ] Return the skipped-line count from `parse_event_lines` (AC 6)
- [ ] Add `floor.rs` with `FloorFile`, `FloorProject`, `ProjectColor` and the error type (`thiserror`) (AC 7)
- [ ] Add `dates.rs` with civil-from-days and days-from-civil (AC 8)
- [ ] Add `contract.rs` with the constants and `check` (AC 9)
- [ ] Run the core gates (AC 10)

## Dev Notes

- Formats: AD-8. UTC days: AD-9. Line cap and corruption visibility: AD-10. Reserved `claude_pid`: AD-11. Contract: AD-12. Verified field shapes: E1, E2.
- Existing state: `quota.rs` currently returns a bare 16-hex key and stores milliseconds; both change here. `events.rs` already has `EXCERPT_CHARS`, `reduce_payload` and a tolerant `parse_event_lines`.
- Gotcha on the account key: if the author sets `CLAUDE_CONFIG_DIR` explicitly to the default `~/.claude`, the statusline writes `cfg-<hex>` while a collector without override reads `default`. Decide in code whether "override" means "env var set" or "path differs from the default"; the second keeps the two writers in agreement. Record the choice in the doc comment of `account_key`.
- `FloorFile` is parsed here but loaded from disk by story 1.10; the schema lives in core so doctor and the collector share it.
- `ProjectColor` variants are the three tokens of AD-8; the TypeScript map that consumes them comes with story 1.4 (ts-rs) and 1.10.
- The contract constants are the only place the field lists live (AD-12). Documentation and tests name the constant.

## Testing

- Unit tests in `crates/officina-core/src/{quota,events,floor,dates,contract}.rs`, run on Linux and Windows CI.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
