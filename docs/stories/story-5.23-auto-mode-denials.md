# Story 5.23: Auto-mode denials in the journal and the Relay view (Slice D, v0.5.x)

**Status:** Review

## Story

As the author who may run his sessions in Claude Code's auto mode,
I want each action auto mode refuses recorded with its reason, and shown as a neutral count where I already look,
so that I see how often the classifier stops my sessions and why, without Officina ever asking Claude Code to retry anything (AD-181, the research note's tier 2).

## Acceptance Criteria

1. **The hook** (AD-181, D-580; hooks reference). `officina hook` on a `PermissionDenied` payload appends one journal line `auto_mode_denied` (session id, tool name when present, `reason` cut to 200 characters on a character boundary), the way the gate appends `gate_denied`; it prints nothing (never `retry`), exits 0, and never exits 2. A payload that does not parse, or has no session id, appends nothing. Proof: core `crates/officina-core/src/relay/journal.rs::tests::{auto_mode_denied_round_trips, the_reason_is_cut_to_200_characters}`; CLI `crates/officina/tests/hook.rs::{permission_denied_appends_one_line_and_prints_nothing, a_malformed_permission_denied_appends_nothing}`; `crates/officina/tests/perf.rs`'s hook own-cost case covers the branch (release, ignored run).
2. **Registration** (D-530, D-534, AD-16). Doctor's merged hooks block registers `PermissionDenied` with Officina's hook command, in the gate entry's form, `timeout` 5 and no `onFailure`, and `missing_hook_events` reports it when absent; the block and its paste file follow D-530 and D-534 unchanged (foreign entries kept, merging twice changes nothing). Proof: host `crates/officina-host/src/doctor.rs::tests::{permission_denied_is_registered_once, a_missing_permission_denied_is_reported}`, and D-530's existing merge cases, which iterate the registrations rather than list them.
3. **`RelayView`** (P-26: no schema number moves). `RelayView` gains `auto_mode_denials`: per session, the count over the last 24 h and the latest reasons (at most 5, `Untrusted<String>`), and each recent entry may be an `auto_mode_denied` line; `officina relay --json` prints the same. Proof: core `crates/officina-core/src/relay/view.rs::tests::{denials_are_counted_per_session_over_24_h, the_reasons_are_untrusted_and_capped}`; CLI `crates/officina/tests/relay.rs::prints_auto_mode_denials`; `typescript_bindings.rs::committed_bindings_match_the_rust_types`.
4. **Where the author sees it** (D-564; the research note's tier 2: neutral, never amber). The Relay view lists the denials with their reasons as text; `SessionFile.svelte` shows the session's count as neutral text ("N auto-mode denials today") when above 0; nothing enters the title, the taskbar overlay, the alert or the scene's zones; no colour is added. Proof: Playwright `app/e2e/relay-view.spec.ts > auto-mode denials are listed with their reasons as text`, `app/e2e/scene.spec.ts > the file counts a session's auto-mode denials`, each in the light and the dark theme; `> denials never take the attention colour` (D-163's computed-colour check).
5. **Zero write** (D-80). The hook's `PermissionDenied` branch joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
6. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: auto-mode denials recorded and shown; README: what a denial line means, that Officina never retries one, that the hooks block gains `PermissionDenied` once pasted. Proof: review; `crates/officina/tests/readme.rs`.

## Tasks

- [x] Journal kind and the hook's `PermissionDenied` branch (AC 1)
- [x] Doctor's registration (AC 2)
- [x] `RelayView.auto_mode_denials`, `relay --json`, bindings (AC 3)
- [x] Relay view list, session file count (AC 4)
- [x] Zero-write (AC 5)
- [x] Update `CHANGELOG.md` and the README (AC 6)

## Dev Notes

- Requirements: PRD section 9 item 10's `PermissionDenied` sentence (AD-181). Decisions: AD-16 as refined by epic-4-decisions.md D-530 and D-534, AD-32, AD-181. Epic decisions: D-80, D-130, D-163, D-491, D-564, D-578, D-580, D-581.
- Entry (D-581): after story 5.19 closes v0.5; it does not need story 5.22. It depends on no entry question: the payload's `reason` is documented, and SD-3 (what else the payload carries) changes nothing here, since only the session id, the tool name and the reason are kept.
- A denial is useful only once the author runs auto mode (his decision, D-490); before that the hook never fires, and the registration costs nothing.
- The hook appends to the journal as the gate does (`relay/gate.rs`'s `gate_denied`), not to the event buffer: a denial is a relay fact, read by `RelayView`, and the board and the registry ignore it.
- No employer content in fixtures: placeholder reasons and tools.

## Testing

- Core unit: `crates/officina-core/src/relay/{journal,view}.rs`; core integration: `typescript_bindings.rs`.
- Host unit: `crates/officina-host/src/doctor.rs`.
- CLI integration: `crates/officina/tests/{hook,relay,quota_doctor,readme,perf}.rs`.
- Playwright: `app/e2e/{relay-view,scene}.spec.ts`.

## Dev Agent Record

### Debug Log

- Implemented on the lead's parallel track before 5.19 closes v0.5 (D-900), after story 5.22 on `main`.
- AC 4's "N auto-mode denials today" is worded "N auto-mode denials in the last 24 h", the window the count covers (D-910, to confirm).
- With excerpts off a reason keeps its bracketed rule name only (D-911): the story keeps it whole and is silent on excerpts off.
- The briefing's runs section reads the relay view's recent entries: the new `auto_mode_denied` entry is left out there (D-909).
- Gate, Windows, on this story's code rebased on story 5.19's close (`b3b1d47`), every job of `.github/workflows/ci.yml` replayed: fmt; frontend install and build; clippy in both CI feature sets; the spawn tests alone; `perf.rs` in release with CI's budgets, 7 passed (PermissionDenied own cost 0.4 ms, PermissionRequest 0.4 ms, hook and gate within their budgets, in-process p95 3.4 ms); the lexer differential; `approvals_stress`; `registry_crash`; `proposals_crash` and the `officina mcp` kill; officina holds no authority (and no test seam) with its controls; registry dependencies; no bundled roots; e2e fixtures from the real CLI (`relay-denials` among them); `pnpm check`, `lint`, Vitest 352 passed; the mutants job (each mutant caught, control green); `cargo deny`; `pnpm audit`. The machine ran other gates: the gate's fail-fast workspace run stopped at `watch.rs::a_two_second_turn_is_confirmed` (not confirmed in its window, outside this story; 5.21 saw it too) and Playwright lost its preview server once (`ERR_CONNECTION_REFUSED` on one case); rerun, `cargo test --workspace --features officina-core/ts --no-fail-fast` 1,695 passed, 1 failed, 16 ignored (TypeScript drift green), the failure `spawn.rs::a_work_order_timeout_kills_only_claude` (its holder's heartbeat not written within the 2 s deadline, outside this story, also failing with its sibling in an earlier loaded run), which passed three reruns of `spawn.rs` (15 each); Playwright `--repeat-each=3` 1,344 passed, 6 skipped. The `core-linux` job in a `rust:1-bookworm` container (toolchain from `rust-toolchain.toml`, source read-only): fmt and clippy clean, 1,522 passed, 0 failed, 14 ignored, the spawn tests alone passed.

### Completion Notes

Decisions D-907 to D-911 (D-910 to confirm). The tests proving each AC:

1. The hook: `crates/officina-core/src/relay/journal.rs::tests::{auto_mode_denied_round_trips, the_reason_is_cut_to_200_characters}`, `journal.rs::tests::with_excerpts_off_only_the_rule_name_is_kept` (D-911) and the doctests of `AutoModeDenied::{from_payload, without_excerpts}`; `crates/officina/tests/hook.rs::{permission_denied_appends_one_line_and_prints_nothing, a_malformed_permission_denied_appends_nothing, permission_denied_without_excerpts_keeps_the_rule_name_only}`; `crates/officina/tests/perf.rs::permission_denied_is_within_the_hook_budget` (release, ignored run; the three hook timings now share `hook_own_cost_within_budget`).
2. Registration: `crates/officina-host/src/doctor.rs::tests::{permission_denied_is_registered_once, a_missing_permission_denied_is_reported}`, and D-530's merge cases, `partially_registered_hooks_are_reported` now deriving its expectation from `registrations`.
3. `RelayView`: `crates/officina-core/src/relay/view.rs::tests::{denials_are_counted_per_session_over_24_h, the_reasons_are_untrusted_and_capped}` and the every-kind case of `recent`; `crates/officina/tests/relay.rs::prints_auto_mode_denials`; `typescript_bindings.rs::committed_bindings_match_the_rust_types` (`AutoModeDenied.ts`, `SessionDenials.ts`, `RecentEvent.ts`, `RelayView.ts`).
4. Where the author sees it: Playwright `app/e2e/relay-view.spec.ts > auto-mode denials in the {light,dark} theme > {auto-mode denials are listed with their reasons as text, denials never take the attention colour}` (D-163's computed-colour check, on the Relay view's rows and the file's count) and `app/e2e/scene.spec.ts > the file counts a session's auto-mode denials ({light,dark})` (title, waiting count and alert band unchanged against a relay view without denials; no line without denials), on the `relay-denials` fixture from the real hook and `officina relay --json`; Vitest `relay-text.test.ts > auto-mode denials`. No colour or token is added; the count is text, so no stable-size case.
5. Zero write: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` runs the hook's `PermissionDenied` branch.
6. Documentation: `CHANGELOG.md` `[Unreleased]` Added; README "Wiring the hooks" (the block and its paragraph) and "Auto-mode denials"; `crates/officina/tests/readme.rs::readme_shows_every_registration_and_never_a_retry`.

## File List

- `crates/officina-core/src/relay/journal.rs`, `relay/view.rs`, `orders/briefing.rs`
- `crates/officina-host/src/relay/journal.rs`, `doctor.rs`
- `crates/officina/src/main.rs`; `crates/officina/tests/hook.rs`, `relay.rs`, `quota_doctor.rs`, `readme.rs`, `perf.rs`
- `app/src/lib/generated/AutoModeDenied.ts` (new), `SessionDenials.ts` (new), `RecentEvent.ts`, `RelayView.ts`
- `app/src/lib/relay-text.ts`, `relay-text.test.ts`, `relay.test.ts`; `app/src/components/SessionFile.svelte`, `scene/Scene.svelte`; `app/src/App.svelte`
- `app/e2e/relay-view.spec.ts`, `scene.spec.ts`, `fixtures.ts`; `app/scripts/e2e-fixtures.mjs`
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this story

## Change Log

- 2026-10-28 created for Slice D (AD-181; D-578, D-580)
- 2026-10-29 implemented; decisions D-907 to D-911; status review
