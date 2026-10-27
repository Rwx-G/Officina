# Story 2.2: Canonical catalogue, hash, diff, relay home and the window's authority

**Status:** Review

## Story

As the author who alone decides what the relay may run,
I want the catalogue reduced to its meaning, hashed, diffed against the approved copy, and approvable only by code the session's executable does not contain,
so that a floor edited by a session cannot run anything until I approve the change in the window.

## Acceptance Criteria

1. `CanonicalCatalogue` holds exactly the content AD-24 lists (format, normalised floor path, `guarded_programs` with defaults applied, sorted and deduplicated, projects by name, valid actions by name with every default explicit, parameters by name, `values` sorted) and none of the excluded fields. The hash is SHA-256 of `serde_json::to_vec` of it, lowercase hex. Proof: core test `crates/officina-core/tests/catalogue_hash.rs::golden_fixture_hash_is_pinned` on `crates/officina-core/tests/data/relay/golden-floor.toml`; changing the canonical form fails it until `format` is bumped.
2. The hash covers meaning, not text: comments, key order, action order, `values` order, colours, floor `name`, `config_dir`, `no_excerpts` and invalid actions leave it unchanged; a change of any canonical field changes it. Proof: `catalogue_hash.rs::{formatting_and_excluded_fields_keep_the_hash, every_canonical_field_changes_the_hash}`; the second test walks the serialized canonical value and mutates each leaf in turn, so a field added later is covered without editing the test.
3. `catalogue::diff(approved, current)` reports actions and projects added, removed and changed per field, and `guarded_programs` changes (AD-24). Proof: `crates/officina-core/src/relay/catalogue.rs::tests::{diff_reports_added_and_removed, diff_reports_changed_fields, diff_reports_guarded_programs, identical_catalogues_have_an_empty_diff}`.
4. `officina-host::relay::home` owns the AD-25 paths (with `relay/known-projects.json` of D-15) and reads `relay/approved.json` (`approved_at_ms`, `hash`, `canonical`) into one of three states: missing, unreadable or corrupt (reported, never a crash), valid. Proof: host tests `crates/officina-host/tests/relay_home.rs::{missing_approved_is_missing, corrupt_approved_is_reported_as_unreadable, approved_round_trips}`.
5. Relay floor resolution is `OFFICINA_FLOOR`, else the floor path in `approved.json`, else `<home>/floor.toml` (AD-24). Proof: `relay_home.rs::floor_resolution_order` (resolution takes its environment as an argument, so the test sets nothing process-wide).
6. Host feature `window-authority` compiles `officina-host::relay::authority`; `authority::approve_catalogue(home, hash)` resolves the floor with AC 5 (D-8), recomputes the current hash and writes `approved.json` atomically (AD-10) only when it equals the displayed one; otherwise it returns a typed error and writes nothing (AD-21, AD-24). On success it also writes `relay/known-projects.json` (D-15). Proof: `crates/officina-host/tests/authority.rs::{approves_the_displayed_hash, edited_floor_between_display_and_click_is_not_approved, invalid_actions_do_not_block_approval, approval_hashes_the_relay_floor_not_another, approval_records_known_projects}`, declared with `required-features = ["window-authority"]`.
7. The `officina` binary a session can call contains no authority code (AD-21, P-5, D-9): the CI step builds `cargo build --release -p officina` itself, checks with `cargo tree -p officina -e normal,features` that `window-authority` is absent, and searches the fresh `officina.exe` for the authority marker, which the authority's error type uses; as a positive control the same search on `officina-app.exe` (built with the feature) must find it. Proof: CI `windows` job step "officina holds no authority"; it fails when the marker is in `officina.exe` or missing from `officina-app.exe`.
8. `officina-app` enables `window-authority`; `officina` enables it only through a dev-dependency so CLI tests can play the window (AD-21). Proof: `cargo clippy --workspace --all-targets` and `cargo test --workspace` in CI.

## Tasks

- [x] `relay::catalogue`: `CanonicalCatalogue`, `canonical(floor, floor_path)`, `hash`, `diff` (AC 1-3)
- [x] Golden fixture floor and `crates/officina-core/tests/catalogue_hash.rs` (AC 1, 2)
- [x] `crates/officina-host/src/relay/{mod,home}.rs`: AD-25 paths, `approved.json` states, relay floor resolution (AC 4, 5)
- [x] Feature `window-authority` in `crates/officina-host/Cargo.toml`; `relay/authority.rs` behind `#[cfg(feature = "window-authority")]`; the marker in the authority error type (AC 6, 7)
- [x] `app/src-tauri/Cargo.toml` enables the feature; `crates/officina/Cargo.toml` dev-dependency on `officina-host` with the feature (AC 8)
- [x] CI step "officina holds no authority" with the positive control in `.github/workflows/ci.yml` (AC 7)
- [x] Update `CHANGELOG.md` `[Unreleased]` (D-34)

## Dev Notes

- Requirements: FR-4 as amended (amendment 5), moved to Slice A by AD-24 and amendment 1. Decisions: AD-21, AD-24, AD-25, AD-10 (atomic write). Epic decisions: P-5, D-8, D-9, D-15 (known-projects file). Non-goal reminder: the relay is a guardrail, not a boundary (PRD section 1, AD-25 last line).
- Cargo unifies features across packages built together: `cargo build --workspace` would link the authority into `officina.exe`. The release workflow builds `-p officina` alone (`tauri.bundle.conf.json` and `release.yml`); AC 7 keeps that true. A `cargo test --release -p officina` leaves an exe with the authority in `target/release`, hence the fresh build inside the CI step.
- With `lto = "thin"` and `strip = true`, an unreferenced constant disappears even from a binary that links the module; that is why the marker sits in a type the authority functions return, and why the positive control exists (D-9).
- Check once, before relying on it, that `cargo tree -e normal,features` does not report dev-dependency features; record the observed output in Completion Notes.
- `approve_catalogue` journals `catalogue_approved`: the journal appender lands in story 2.4, which adds that line. This story writes `approved.json` and `known-projects.json` only.
- D-8: the window shows and approves the relay's floor, which is also the floor the gate and `run` check. The board still follows the window's `--floor` (AD-7); doctor warns when the two differ (D-30).
- The canonical floor path is normalised with `quota::normalise_path` (lowercase, forward slashes). Windows opens it case-insensitively; Linux (v1) will need the original spelling kept beside it. Note it, do not solve it now.

## Testing

- Core unit: `crates/officina-core/src/relay/catalogue.rs`.
- Core integration: `crates/officina-core/tests/catalogue_hash.rs`.
- Host integration: `crates/officina-host/tests/{relay_home,authority}.rs`.
- CI: the "officina holds no authority" step.

## Dev Agent Record

### Debug Log

- **The positive control needs the window to call the authority.** With `lto = "thin"` and `strip = true`, an authority function nothing calls, and its error's `Display`, are removed from a binary that links the module, so `officina-app.exe` would lack the marker until story 2.7 wires the banner. This story therefore adds the bare Tauri command `approve_catalogue(hash)` (thin wrapper, run off the main thread, declared in `build.rs` and allowed in `capabilities/default.json`) so the shipped window links the authority now. Story 2.7 adds `CollectorMsg::RelayChanged` and the banner that calls it. No frontend code calls the command yet.
- The marker is the first word of every `AuthorityError` message, and the CI step reads it from `AUTHORITY_MARKER` in the source with `sed`, so the workflow does not restate it.
- The CI step captures `cargo tree` before grepping it: under `bash -eo pipefail` a failing `cargo tree` inside an `if` would otherwise pass silently; it also requires an `officina-host feature` line, so an empty tree fails.
- `relay/known-projects.json` is written before `relay/approved.json`. If the approval write then fails, the record names a hash with no `approved.json` behind it, which D-15 reads as a broken snapshot (every floor project guarded): a partial failure can only tighten the gate. Written the other way round, a failed record would leave stale project paths for the fallback.
- `RelayHome` carries the `OFFICINA_FLOOR` value, so `approve_catalogue(home, hash)` keeps the story's signature and tests resolve the floor without setting anything process-wide (AC 5).
- `relay/approved.json` is read with `deny_unknown_fields` (Officina is its only writer, AD-8) and its hash is not recomputed on read: the gate computes no hash (AD-28). A snapshot of an older `format` parses and differs from the current hash, which is the intended "approval pending".
- The golden floor declares `guarded_programs`, so story 2.3's extension of the default list (D-11) does not move the pinned hash; `catalogue.rs::tests::{defaults_are_explicit_after_validation, diff_reports_guarded_programs}` cover the default list.
- `every_canonical_field_changes_the_hash` mutates each leaf of the serialized form: a suffix or another string of the catalogue for strings (so the closed enums get another valid variant; the golden floor uses at least two of class, runner and autonomy), plus one for numbers, a value for nulls and empty containers. A leaf with no mutation that still deserializes fails the test. Map keys (projects, actions, parameters, env) are renamed too.
- `diff` works field by field over the serialized entries; `catalogue.rs::tests::diff_covers_every_canonical_field` fails when a top-level canonical field has no slot in `CatalogueDiff`.
- `OFFICINA_FLOOR` is used as given. A relative value would resolve against each process's working directory and give the window and the gate different floor paths; not solved, it is a misconfiguration doctor can report (story 2.10).
- The canonical floor path is lowercase (Dev Notes): Windows opens it whatever the case; Linux (v1) will need the original spelling. Noted on `CanonicalCatalogue::floor_path`.

### Completion Notes

- AC 1: `crates/officina-core/tests/catalogue_hash.rs::golden_fixture_hash_is_pinned` on `crates/officina-core/tests/data/relay/golden-floor.toml` (pins `(CANONICAL_FORMAT, hash)`), plus `hash_is_sha256_of_the_serialized_form_in_lowercase_hex` (recomputes the digest with `sha2` directly).
- AC 2: `catalogue_hash.rs::{formatting_and_excluded_fields_keep_the_hash, every_canonical_field_changes_the_hash}`; the first compares the golden floor with `golden-floor-variant.toml` (comments, key, action and values order, colours, name, `config_dir`, `no_excerpts`, defaults written out, an invalid action).
- AC 3: `crates/officina-core/src/relay/catalogue.rs::tests::{diff_reports_added_and_removed, diff_reports_changed_fields, diff_reports_guarded_programs, identical_catalogues_have_an_empty_diff}`.
- AC 4: `crates/officina-host/tests/relay_home.rs::{missing_approved_is_missing, corrupt_approved_is_reported_as_unreadable, approved_round_trips}`, plus `paths_follow_the_relay_file_table`.
- AC 5: `relay_home.rs::floor_resolution_order`.
- AC 6: `crates/officina-host/tests/authority.rs::{approves_the_displayed_hash, edited_floor_between_display_and_click_is_not_approved, invalid_actions_do_not_block_approval, approval_hashes_the_relay_floor_not_another, approval_records_known_projects}`, declared with `required-features = ["window-authority"]`.
- AC 7: CI `windows` job step "officina holds no authority". Run locally with the job's shell (`bash --noprofile --norc -eo pipefail`) after `cargo test --release -p officina --test perf --no-run`: exit 0; the marker is found 0 times in the fresh `officina.exe` and once in `officina-app.exe`.
- AC 7, `cargo tree` check (Dev Notes): `cargo tree -p officina -e normal,features` lists `officina-host feature "default"` and no `window-authority`; the control `cargo tree -p officina -e features`, which includes dev edges, lists `officina-host feature "window-authority"`. The check therefore sees exactly the shipped feature set.
- AC 8: `cargo clippy --workspace --all-targets --features officina-core/ts -- -D warnings` and `cargo test --workspace --features officina-core/ts` pass with the app enabling the feature and `officina` enabling it through its dev-dependency.
- Gates at the end of the story: fmt, clippy and the workspace tests all green; `cargo deny check` passes.

## File List

- `crates/officina-core/src/relay/mod.rs`
- `crates/officina-core/src/relay/catalogue.rs`
- `crates/officina-core/tests/catalogue_hash.rs`
- `crates/officina-core/tests/data/relay/golden-floor.toml`
- `crates/officina-core/tests/data/relay/golden-floor-variant.toml`
- `crates/officina-host/Cargo.toml`
- `crates/officina-host/src/lib.rs`
- `crates/officina-host/src/paths.rs`
- `crates/officina-host/src/relay/mod.rs`
- `crates/officina-host/src/relay/home.rs`
- `crates/officina-host/src/relay/authority.rs`
- `crates/officina-host/tests/relay_home.rs`
- `crates/officina-host/tests/authority.rs`
- `crates/officina/Cargo.toml`
- `app/src-tauri/Cargo.toml`
- `app/src-tauri/build.rs`
- `app/src-tauri/capabilities/default.json`
- `app/src-tauri/src/main.rs`
- `.github/workflows/ci.yml`
- `README.md`
- `CHANGELOG.md`

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: D-8 (relay floor hashed), D-9 (marker used, positive control, fresh build), D-15 (known-projects file)
- 2026-10-28 implemented: canonical form, hash and diff, relay home, window authority with the `approve_catalogue` command, CI authority check with positive control; status Review
