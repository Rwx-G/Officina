# Story 2.7: Window, slice A: catalogue approval and stop

**Status:** Approved

## Story

As the author who edits the floor file or lets a session edit it,
I want the window to show that the catalogue changed, what changed per action, an Approve button, and a stop toggle,
so that nothing new runs through the relay until I have seen it, approving takes one click, and I can halt every relay call at once.

## Acceptance Criteria

1. The collector thread also stats the floor file, the `relay/` tree (fingerprint like `sessions_fingerprint`) and today's journal file; a floor change reloads the floor and rebuilds the watched paths (sessions dir and quota file follow a new `config_dir`), so board projects follow too; `RelayView` is recomputed on any of them (AD-35, AD-4). Proof: CLI-crate tests `crates/officina/tests/watch.rs::{floor_edit_reloads_board_projects, floor_edit_rebuilds_watched_paths, relay_tree_change_recomputes_the_relay_view, journal_append_recomputes_the_relay_view}`.
2. `watch::spawn` takes a second callback for the relay view, emitted on `relay://updated` only when its bytes change, and `get_relay` returns the cached view for the first paint (AD-35, AD-5). Proof: `watch.rs::relay_view_is_emitted_only_on_change`.
3. On every valid read of the floor (phase 1) or of the snapshot, the collector rewrites `relay/known-projects.json` when its content changes (D-15). Proof: `watch.rs::known_projects_follow_the_floor`.
4. Tauri command `approve_catalogue(hash)` is a thin wrapper over `authority::approve_catalogue` (relay floor, D-8) run off the main thread, and sends `CollectorMsg::RelayChanged` for an immediate recompute (AD-35, AD-3). Proof: host tests of story 2.2 AC 6 for the logic; `watch.rs::relay_changed_message_recomputes_now`.
5. Stop (FR-11, AD-34, D-26): `authority::set_relay_stopped(stopped)` writes `relay/stopped` (`{"since_ms":...}`) atomically or deletes it and journals `relay_stopped` or `relay_resumed` once; Tauri command `set_relay_stopped` wraps it and sends `RelayChanged`. Proof: host tests `crates/officina-host/tests/authority.rs::{stop_writes_the_flag_and_journals, resume_deletes_the_flag_and_journals, stopping_twice_journals_once}`.
6. Stop flow (AD-37 flows): with the flag set through `set_relay_stopped`, `officina run` refuses "relay stopped"; after resume the same call runs; gate decisions are identical with and without the flag (AD-34). Proof: CLI tests `crates/officina/tests/run.rs::{stop_through_the_authority_refuses, resume_lets_runs_through}` and `crates/officina/tests/gate.rs::stop_does_not_change_gate_decisions`.
7. `get_relay`, `approve_catalogue` and `set_relay_stopped` are declared in `build.rs` and allowed in `capabilities/default.json` (AD-19, AD-35). Proof: `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`, which reads the command list of `build.rs` and the capability file, so a command added to one place only fails.
8. `ipc.ts` gains `getRelay`, `onRelay`, `approveCatalogue` and `setRelayStopped`; `lib/relay.svelte.ts` holds the relay store; `e2e-mock.ts` answers the three commands and records calls (AD-20, AD-35). Proof: the Playwright cases below run against the mock.
9. When a change is pending, the Board view shows a banner with the diff per action and per field and an Approve button that sends the displayed hash; the header shows an amber "catalogue change" chip (AD-35 layout, PRD journey 1). Proof: Playwright `app/e2e/relay.spec.ts > a pending catalogue change shows the diff and approves the displayed hash` and `> the header chip marks a pending change`.
10. An approval refused because the floor changed again says so and keeps the banner with the new diff. Proof: `relay.spec.ts > a refused approval says the catalogue changed again`.
11. The header carries a stop toggle; a stopped relay shows a terracotta "relay stopped" chip and a banner on the Board view; the toggle calls `set_relay_stopped` (AD-35, PRD journey 5). Proof: `relay.spec.ts > the stop toggle calls set_relay_stopped`, `> a stopped relay shows the terracotta chip and the board banner`.
12. Catalogue text (descriptions, values, patterns, issue messages) is rendered as text. Proof: `relay.spec.ts > catalogue text renders as text, never as HTML` (an `<img onerror>` description) and the `svelte/no-at-html-tags` lint.
13. Fixtures come from the real code (D-25): `app/scripts/e2e-fixtures.mjs` builds the scenarios `relay-clean`, `relay-pending` and `relay-stopped` with `officina relay --json` against a hand-written, redacted floor, after approving it with `cargo run -p officina-host --example approve_fixture --features window-authority`; the stopped scenario writes the flag through the same example. Proof: CI step "e2e fixtures from the real CLI" and the Playwright cases loading them; `crates/officina-host/examples/approve_fixture.rs` declares `required-features = ["window-authority"]`.

## Tasks

- [ ] `watch.rs`: floor stamp and reload with watched-path rebuild, relay tree fingerprint, journal stamp, relay callback and diff before emit, `known-projects.json` upkeep, `CollectorMsg::RelayChanged` (AC 1-4)
- [ ] `authority::set_relay_stopped` with journal lines (AC 5)
- [ ] `app/src-tauri/src/main.rs`: `get_relay`, `approve_catalogue`, `set_relay_stopped`, `relay://updated` emit; `build.rs`, `capabilities/default.json` (AC 4, 5, 7)
- [ ] `ipc.ts`, `lib/relay.svelte.ts`, `e2e-mock.ts`, `e2e-globals.d.ts` (AC 8)
- [ ] Banner with the diff and Approve; header chips and stop toggle; stopped banner (AC 9-12)
- [ ] `crates/officina-host/examples/approve_fixture.rs` (approve the relay floor of a given home; set or clear the stop flag); relay scenarios in `scripts/e2e-fixtures.mjs`; `app/e2e/relay.spec.ts` (AC 9-13)
- [ ] CLI tests `run.rs`, `gate.rs` for the stop flow (AC 6)
- [ ] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (approval, stop) (D-34)

## Dev Notes

- Requirements: FR-4 as amended (amendments 1 and 5: a banner with an Approve button in Slice A), FR-11 moved to Slice A (D-26). Decisions: AD-3, AD-4, AD-5, AD-19, AD-20, AD-21, AD-24 (approve only the displayed hash), AD-34, AD-35. PRD journeys 1 and 5. Epic decisions: D-8, D-15, D-25, D-26.
- `watch::spawn` takes one `emit(&Board)` today and fixes `Watched` at thread start; this story adds the relay callback and makes `Watched` rebuildable after a floor reload (review finding G8).
- The banner must show the diff, not the raw canonical JSON: the author approves meaning, which is what the hash covers (AD-24). Per D-8 the window shows the relay floor's diff even if the window runs with another `--floor`; doctor warns about that case (D-30).
- The example is a test and fixture tool: it is not linked into either shipped binary, and it calls the same `authority` functions as the window, so fixtures never transcribe AD-24 in Node and no CLI path approves (AD-21).
- Amber marks what waits on the author (frontend rule): a pending catalogue change does. Terracotta marks a stopped relay. The Board/Relay toggle and the Relay view come with story 2.9.
- No key approves (AD-35): Approve is a button only, with no keyboard shortcut; the same click rule as approval cards applies (D-28, story 2.8).
- After this story Slice A is usable end to end: declare, approve in the window, delegate through `officina run`, contained by the gate, journaled, stoppable. The `CLAUDE.md` paragraph sessions read comes with story 2.10, which depends only on stories 2.1 to 2.7 (D-34).

## Testing

- CLI-crate integration (collector thread): `crates/officina/tests/watch.rs`.
- Host integration: `crates/officina-host/tests/authority.rs`.
- CLI integration: `crates/officina/tests/{run,gate}.rs`.
- App crate unit: `app/src-tauri/src/main.rs` tests.
- Playwright: `app/e2e/relay.spec.ts` on fixtures from `officina relay --json` and the `approve_fixture` example.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: stop moved here into Slice A (D-26), fixtures approved through the `approve_fixture` example (D-25), relay floor approved (D-8), `known-projects.json` upkeep (D-15), watched paths rebuilt on reload, journal-append recompute test
