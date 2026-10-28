# Story 2.7: Window, slice A: catalogue approval and stop

**Status:** Review

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

- [x] `watch.rs`: floor stamp and reload with watched-path rebuild, relay tree fingerprint, journal stamp, relay callback and diff before emit, `known-projects.json` upkeep, `CollectorMsg::RelayChanged` (AC 1-4)
- [x] `authority::set_relay_stopped` with journal lines (AC 5)
- [x] `app/src-tauri/src/main.rs`: `get_relay`, `approve_catalogue`, `set_relay_stopped`, `relay://updated` emit; `build.rs`, `capabilities/default.json` (AC 4, 5, 7)
- [x] `ipc.ts`, `lib/relay.svelte.ts`, `e2e-mock.ts`, `e2e-globals.d.ts` (AC 8)
- [x] Banner with the diff and Approve; header chips and stop toggle; stopped banner (AC 9-12)
- [x] `crates/officina-host/examples/approve_fixture.rs` (approve the relay floor of a given home; set or clear the stop flag); relay scenarios in `scripts/e2e-fixtures.mjs`; `app/e2e/relay.spec.ts` (AC 9-13)
- [x] CLI tests `run.rs`, `gate.rs` for the stop flow (AC 6)
- [x] Update `CHANGELOG.md` `[Unreleased]` and the README relay section (approval, stop) (D-34)

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

- **Decisions where the documents are silent**, each the reading that fails closed or matches the architecture:
  - `PendingChange` gains `current`, the canonical catalogue the Approve button would approve: `CatalogueDiff` lists added actions by name only, and the author must see what an added action runs (AD-24: the author approves meaning). The banner shows each added action's class, runner, program and arguments as chips, `env`, parameters and limits; a changed map field (`params`, `env`) is split by key so one added value is not lost in the whole map.
  - The banner also lists the actions the floor skips as invalid (`RelayView.issues`), with their lines, so the author knows what is not part of the approval; the Relay view of story 2.9 keeps the full list.
  - The stop toggle, the chips and the relay banners appear only while the relay is in use (an approved hash, a pending change, a stop flag or a broken snapshot): a v0.1 workstation sees nothing new, and the board e2e cases and README assets are unchanged.
  - A refused approval says "changed again" when the next relay view's pending hash differs from the hash the author approved; the store compares hashes rather than parsing the error text. Any other refusal shows the backend's message.
  - `with_authority` sends `RelayChanged` after every authority call, refused ones included: a refusal usually means the floor moved.
  - Floor reload: the default floor (`floor.toml` in the home) counts when it exists, as at startup (created later it is picked up, deleted it leaves no floor); a floor named by `--floor` or `OFFICINA_FLOOR` that disappears is a board error. `Collector::from_parts` now takes the environment's paths and applies the floor's `config_dir` itself, keeping the base for reloads; `Collector` carries the relay home (`OFFICINA_FLOOR` applied in `Collector::new`).
  - The collector also stats the relay floor (AD-24, D-8), which can differ from the window's floor, and re-resolves it when the relay tree changes; a relay floor edit rebuilds the relay view, not the board (AD-7).
  - The relay view is rebuilt every 15 s besides its triggers (`RELAY_POLL`), so a run turns "abandoned" and old gate errors leave the 24 h count without a file change.
  - `relay/known-projects.json` upkeep (D-15): written only while the relay is in use (a valid snapshot, a record already there, or a floor that declares an action or a guarded project), because a record makes a broken floor guard every known project, which a workstation without the relay must not get. The record holds the paths of the valid floor and the valid snapshot, and keeps its recorded hash when the snapshot is gone, so the gate keeps reading that state as broken. The "in use" rule is one core function, `catalogue::relay_in_use`, also used for a first pending change.
  - The e2e mock synthesises only what the authority would change for `set_relay_stopped` (the flag and its time); approval views come from fixtures (`__officinaRelayApproved`). `__officinaPushRelay` is defined here, as the Epic 4 stories expect, with `loadRelay` in `e2e/fixtures.ts`.
  - The fixtures run `cargo run -p officina-host --example approve_fixture --features window-authority` (AC 13's command) from `e2e-fixtures.mjs`, so CI's existing step "e2e fixtures from the real CLI" covers them without a workflow change. Each scenario has its own temporary home, so hashes differ between scenarios and the e2e cases read them from the fixtures.
- **Interrupted twice by the coordinator**, each in its own commit: the core-linux CI failure (story 2.6's record) and `perf.rs::gate_overhead_over_empty_process` on the hosted Windows runner, whose baseline is now the same binary answering another tool than `Bash` (recorded in story 2.4, commit `test(relay): measure the gate's own cost against the same binary`).
- **Adversarial pass.** Tried: a floor edited between display and click (refused, new diff shown), a double click and Enter, Space and number keys on the focused Approve button, markup in a description, an invalid action's name and an issue message, approving with the window on another floor than the relay's, stopping twice and concurrently, stopping with the journal locked, a floor moving `config_dir`, a default floor created and deleted, the relay floor differing from the window's, `approved.json` deleted under a running window, a v0.1 floor, and a relay file the view does not show. Two real misses became tests: after a reload moved `config_dir`, the window's focus and attach commands kept the startup config dir and would have attached with the wrong account (`watch.rs::floor_edit_rebuilds_watched_paths` now asserts the shared paths; the window builds its focus context per call from them); and the "relay in use" rule was written twice, on the canonical form and on the raw floor (now one core function with a doctest). The header's Resume button took the pin button's dark pressed style (equal specificity, later rule), seen on a screenshot and fixed.
- **Not run here:** the window itself (Tauri) was not started; its commands are thin wrappers proved by the host and collector tests, and its declarations by `main.rs::tests::every_declared_command_is_allowed`. The release search for the authority marker was not rerun locally; `cargo tree -p officina -e normal,features` still shows no `window-authority`.

### Completion Notes

- AC 1: `crates/officina/tests/watch.rs::{floor_edit_reloads_board_projects, floor_edit_rebuilds_watched_paths, relay_tree_change_recomputes_the_relay_view, journal_append_recomputes_the_relay_view}`; host `watch.rs::tests::{relay_changes_rebuild_the_relay_view_alone_and_time_does_too, floor_change_rebuilds_everything, relay_tree_changes_change_its_fingerprint}`.
- AC 2: `watch.rs::relay_view_is_emitted_only_on_change`; `relay_tree_change_recomputes_the_relay_view` reads `WatchHandle::relay`, the cache `get_relay` answers.
- AC 3: `watch.rs::known_projects_follow_the_floor` (floor edit, snapshot gone, and no record for a v0.1 floor).
- AC 4: host `authority.rs` approval tests (story 2.2); `watch.rs::relay_changed_message_recomputes_now` (a change no stat sees, shown within 2 s of the message).
- AC 5: `crates/officina-host/tests/authority.rs::{stop_writes_the_flag_and_journals, resume_deletes_the_flag_and_journals, stopping_twice_journals_once}`, plus `a_locked_journal_stops_nothing`.
- AC 6: `crates/officina/tests/run.rs::{stop_through_the_authority_refuses, resume_lets_runs_through}`, `crates/officina/tests/gate.rs::stop_does_not_change_gate_decisions`.
- AC 7: `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` (build.rs list, capability file and `generate_handler!` list, each read from its file).
- AC 8: the Playwright cases below run against the mock; Vitest `app/src/lib/relay.test.ts` covers the store.
- AC 9: `app/e2e/relay.spec.ts > a pending catalogue change shows the diff and approves the displayed hash`, `> the header chip marks a pending change`.
- AC 10: `relay.spec.ts > a refused approval says the catalogue changed again`.
- AC 11: `relay.spec.ts > the stop toggle calls set_relay_stopped`, `> a stopped relay shows the terracotta chip and the board banner`.
- AC 12: `relay.spec.ts > catalogue text renders as text, never as HTML`, Vitest `relay.test.ts > RelayBanners > renders catalogue text as text, never as markup`, and the `svelte/no-at-html-tags` lint.
- AC 13: `app/scripts/e2e-fixtures.mjs` scenarios `relay-clean`, `relay-pending`, `relay-pending-again`, `relay-pending-approved`, `relay-stopped`; `crates/officina-host/Cargo.toml` declares the example with `required-features = ["window-authority"]`.
- Beyond the ACs: `relay.spec.ts > no key approves`, `> a workstation without the relay shows no relay control`; Vitest `relay.test.ts > RelayBanners > approves on a pointer click only`.
- Gates at the end of the story: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings` and with CI's feature sets; `cargo test --workspace --all-features` 386 passed, 0 failed, 5 ignored; TS drift test green; `cargo test --release -p officina --test perf -- --ignored` 5 passed; `cargo deny check` ok (existing duplicate-version warnings only); `pnpm -C app check`, `lint`, `test` (20 passed); fixtures rebuilt from the real CLI and `pnpm -C app e2e` 28 passed. The two Rust commits before the UI one were built and tested alone in a scratch worktree.

## File List

- `crates/officina-core/src/relay/{catalogue,view}.rs`, `app/src/lib/generated/PendingChange.ts`
- `crates/officina-host/src/{collector,watch}.rs`, `crates/officina-host/src/relay/authority.rs`
- `crates/officina-host/Cargo.toml`, `crates/officina-host/examples/approve_fixture.rs`
- `crates/officina-host/tests/authority.rs`
- `crates/officina/tests/{watch,run,gate,quota_doctor}.rs`
- `app/src-tauri/src/main.rs`, `app/src-tauri/build.rs`, `app/src-tauri/capabilities/default.json`
- `app/src/{App.svelte,app.css,e2e-mock.ts,e2e-globals.d.ts}`, `app/src/components/{Header,RelayBanners}.svelte`, `app/src/lib/{ipc.ts,relay.svelte.ts,relay.test.ts}`
- `app/e2e/{fixtures.ts,relay.spec.ts}`, `app/scripts/e2e-fixtures.mjs`
- `README.md`, `CHANGELOG.md`

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: stop moved here into Slice A (D-26), fixtures approved through the `approve_fixture` example (D-25), relay floor approved (D-8), `known-projects.json` upkeep (D-15), watched paths rebuilt on reload, journal-append recompute test
- 2026-10-28 implemented: stop and resume in the authority, the collector's floor reload and relay view, the window's commands, the approval banner, chips and stop toggle, fixtures through `approve_fixture`; adversarial pass (focus after a config dir move, one "in use" rule); status Review
