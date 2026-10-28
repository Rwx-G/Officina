# Story 2.15: Quality, debt and dependencies before v0.3

**Status:** Approved

## Story

As the author who wants no debt carried from one version to the next,
I want the findings of the Epic 2 quality, debt and dependency audits fixed before the registry work starts,
so that v0.3 builds on code whose structure, documentation and dependencies say what is true.

## Acceptance Criteria

1. Host `relay/run.rs` is split by concern (request and wait, consumption, spawn and output capture, journaling of a run, kept outputs), with one shared ending for journal, cut and keep instead of three, and stderr injectable like stdout. No behaviour changes: every existing test passes unchanged. Proof: the full suite; a unit test per extracted module where none existed.
2. Core `relay/shell.rs`: the magic `coproc: u8` becomes a named enum, the duplicated option scanners become one function per wrapper table, and the lexing state is split from the analysis state where the code allows it without changing behaviour. Proof: `tests/vectors/gate.json` replays unchanged; `step_count_grows_linearly` and the perf tests stay green.
3. Core `relay/catalogue.rs`: the duplicate-name check is one helper used by actions, credentials and projects; the name rule's prose lives on one constant used in every message; the three long validators are split into per-key functions. Proof: catalogue unit tests unchanged and green; one test that every name-rule message comes from the constant.
4. `closed_enum!` generates a `code()` returning the wire name of each variant, and the five serde round trips that fall back to an empty string are replaced by it. Proof: core unit test `relay::tests::code_equals_the_serde_name_for_every_variant` iterating `ALL` of each enum built with the macro.
5. Test support is shared: one `Relay` harness, one floor builder and one `ApprovedCatalogue` builder in `crates/officina/tests/common/` (and the host's equivalent), used by every relay test file; `assert_refused` defined once. Proof: the suite; a grep test is not needed.
6. The collector is ready to be v0.3's single writer: no `pub` fields on `Collector`, its cache behind methods, and the `known-projects.json` write error reported in the relay view and journaled as `gate_error`-style diagnostics instead of swallowed. Proof: host test `crates/officina-host/tests/relay_home.rs::a_known_projects_write_error_is_reported`.
7. Outcome tone comes from core: the relay view carries each recent entry's tone (`ok`, `failed`, `refused`) computed in core from exit code, HTTP status and refusal; `relay-text.ts` renders it; the e2e asserts against fixture data, not against the same function. IPC commands map errors one way (a typed error with a code and a message), and the approval card reads only its own approval, not the whole board store. Proof: core `view.rs::tests::tone_for_every_outcome_kind`; Vitest and Playwright green with the new assertions.
8. Debt fixes: the README's Privacy section states that HTTP actions open the connections their catalogue declares and nothing else does, with a `readme.rs` case; every place that says an approval is consumed by rename says it is claimed by exclusive create (host module doc, stress test header, `FileKind::Consumed` doc, story 2.8, and a dated amendment note under AD-33 in `docs/architecture-v0.2.md`); `GateFault::Io` is removed or produced by a real IO path with a test; the dead "class not available" texts are removed (v0.5 adds what `auto` needs); `invalid_pattern_does_not_affect_the_decision` is rewritten so it fails if the gate compiles patterns; `Lexed.trace` is filled only under `cfg(test)` or removed; stories 2.11 and 2.13 tasks are ticked; the four test names renamed by story 2.8 are corrected in stories 2.5 and 2.6. Proof: `readme.rs`; the rewritten gate test fails against a deliberately broken gate (record the check in the Dev Agent Record).
9. Dependencies: `ureq` stays pinned `=3.5.0` with `rustls-no-provider` and `platform-verifier`, and the internal `_ring` feature is replaced by a direct `rustls` dependency with its `ring` feature (same versions as the lock); `webpki-roots` is added to `bans.deny` in `deny.toml` (the CI grep stays); every cargo command in `ci.yml` and `release.yml` takes `--locked`; a weekly scheduled run of the audit job is added; `pnpm audit` covers dev dependencies too, with an allow-list file only if a finding has no fix, each entry with a reason. Proof: CI green; `cargo deny check` green; `cargo tree -i webpki-roots` empty on the host target.
10. Toolchain: `rust-toolchain.toml` moves to the current stable Rust (1.99.1 at the time of the audit, or newer if released) and every new lint is fixed, not allowed. ESLint moves to its supported major with the flat config updated. Proof: every gate of `ci.yml` green locally and in CI.
11. Third-party notices for release artefacts are recorded in `docs/backlog.md` (High, before the first published release): a generated notices file (Rust crates and npm packages with their license texts, the bundled font's OFL) in the release zip and installer. Not built here: no release is published before the owner asks.

## Tasks

- [ ] Refactors (AC 1-3), macro `code()` (AC 4), shared test support (AC 5)
- [ ] Collector seams (AC 6); tone in core, IPC errors, approval card store (AC 7)
- [ ] Debt fixes and documentation (AC 8)
- [ ] Dependencies and CI (AC 9); toolchain and ESLint (AC 10); backlog entry (AC 11)
- [ ] Update `CHANGELOG.md` `[Unreleased]` where users see a change (Privacy wording, toolchain)

## Dev Notes

- Sources (git-ignored): `_bmad-output/planning-artifacts/qa/epic-2-quality.md`, `qa/epic-2-debt.md`, `qa/epic-2-dependencies.md`. Read them for evidence lines. The debt report's finding that `_bmad-output/` is not ignored is wrong: `_bmad-output/.gitignore` ignores everything under it (checked with `git check-ignore`).
- Not taken: loosening the `ureq` pin to `~3.5.0` (unstable features justify the exact pin); a full rewrite of the lexer (the vector set guards it, the targeted refactor is enough).
- Order: run after story 2.14, which changes `run.rs`, `tree.rs`, `gate.rs` and `authority.rs`. Commit each AC on its own; refactors first, then behaviour-visible changes.
- Decisions: D-174 (this story).

## Testing

- The whole suite, the vector sets, the perf and stress tests, Vitest and Playwright; CI on both jobs.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created from the Epic 2 quality, debt and dependency audits (D-174)
