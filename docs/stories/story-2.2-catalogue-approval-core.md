# Story 2.2: Canonical catalogue, hash, diff, relay home and the window's authority

**Status:** Approved

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

- [ ] `relay::catalogue`: `CanonicalCatalogue`, `canonical(floor, floor_path)`, `hash`, `diff` (AC 1-3)
- [ ] Golden fixture floor and `crates/officina-core/tests/catalogue_hash.rs` (AC 1, 2)
- [ ] `crates/officina-host/src/relay/{mod,home}.rs`: AD-25 paths, `approved.json` states, relay floor resolution (AC 4, 5)
- [ ] Feature `window-authority` in `crates/officina-host/Cargo.toml`; `relay/authority.rs` behind `#[cfg(feature = "window-authority")]`; the marker in the authority error type (AC 6, 7)
- [ ] `app/src-tauri/Cargo.toml` enables the feature; `crates/officina/Cargo.toml` dev-dependency on `officina-host` with the feature (AC 8)
- [ ] CI step "officina holds no authority" with the positive control in `.github/workflows/ci.yml` (AC 7)
- [ ] Update `CHANGELOG.md` `[Unreleased]` (D-34)

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

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: D-8 (relay floor hashed), D-9 (marker used, positive control, fresh build), D-15 (known-projects file)
