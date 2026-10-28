# Story 5.23: Auto-mode denials in the journal and the Relay view (Slice D, v0.5.x)

**Status:** Approved

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

- [ ] Journal kind and the hook's `PermissionDenied` branch (AC 1)
- [ ] Doctor's registration (AC 2)
- [ ] `RelayView.auto_mode_denials`, `relay --json`, bindings (AC 3)
- [ ] Relay view list, session file count (AC 4)
- [ ] Zero-write (AC 5)
- [ ] Update `CHANGELOG.md` and the README (AC 6)

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

### Completion Notes

## File List

## Change Log

- 2026-10-28 created for Slice D (AD-181; D-578, D-580)
