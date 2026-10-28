# Story 7.17: The stop shortcut

**Status:** Approved

## Story

As the author working in my editor while the governor runs routine work,
I want one system-wide chord that stops autonomy from anywhere, even with the window minimized or behind a full-screen editor, and nothing else,
so that I can stop every autonomous run in seconds without reaching for the window, while my clicked runs and the relay go on (FR-16, J5).

## Entry conditions

Implemented only once story 7.13's Slice B decision entry records **go** (D-331; the conditions are listed in story 7.14), after story 7.16 (Stop autonomy), and only when E66's first half passed (D-303). Before its registration code, one run of the `windows` CI job registers a free chord on the hosted runner and records whether the session allows it (D-355, review F39). If E66 failed, this story stays unimplemented: Stop autonomy stays a button, Stop all a command, and doctor says the shortcut is not available.

## Acceptance Criteria

1. **The key map** (AD-141, D-336). `windows-sys` gains feature `Win32_UI_Input_KeyboardAndMouse` (approved with the addendum). Story 7.2's normal form maps to `RegisterHotKey`'s modifiers plus `MOD_NOREPEAT` and to a virtual key through `windows-sys`'s constants, never retyped numbers. Proof: host unit, Windows only, `crates/officina-host/src/platform/windows.rs::tests::every_chord_key_maps_to_its_virtual_key_constant` (iterates the grammar's key set from story 7.2's constant).
2. **Registration** (AD-141, FR-16 as amended by amendment 10). Under `window-authority`, a thread named `hotkey` calls `RegisterHotKey(NULL, id, modifiers | MOD_NOREPEAT, vk)` with the chord of the approved snapshot only, then loops on `GetMessageW`; `WM_HOTKEY` sends `AuthorRequest::StopAutonomy { by: shortcut }`, which carries no deadline and is never dropped; a chord change in an approved catalogue, or shutdown, posts `WM_QUIT` with `PostThreadMessageW`, the thread unregisters and ends, and a new thread registers the new chord; a pending floor change re-registers nothing. Proof: host integration, Windows only, `crates/officina-host/tests/governor_hotkey.rs::{a_free_chord_registers_and_a_posted_hotkey_yields_stop_autonomy, a_chord_change_unregisters_then_registers, shutdown_unregisters, a_pending_floor_change_keeps_the_approved_chord}` (`WM_HOTKEY` posted to the thread with `PostThreadMessageW`). When the hosted runner's session refuses registration (D-355), these tests carry `#[ignore]` with that reason, the key-map test of AC 1 still runs on every build, and story 7.19 runs the ignored ones on the workstation with `--ignored`.
3. **Failure is shown** (AD-141, D-336). A chord another program holds fails to register: `GovernorView` and doctor say so with the chord, and the floor can name another. Proof: `governor_hotkey.rs::a_chord_held_by_the_test_fails_and_is_reported` (the test registers the chord first); host `crates/officina-host/src/governor/doctor.rs::tests::an_unregistered_shortcut_warns_with_its_chord`.
4. **It stops autonomy only** (FR-16, FR-14, D-335). The shortcut runs story 7.16's Stop autonomy with `by: shortcut`: mode off, every live autonomous run stopped, clicked runs and the relay untouched; `governor_mode` records `by: shortcut`. Proof: CLI-crate collector test `crates/officina/tests/watch.rs::{a_shortcut_stop_is_never_dropped, a_shortcut_stop_leaves_clicked_runs_and_the_relay}`.
5. **Gate 5, second half** (AD-149 row 5, D-302, PRD section 7's Stop row). On a build of this story, with one autonomous run and one clicked run live (real workshop runs, or a dev build whose `OFFICINA_DOCKER_BIN` names `officina-fake-docker`, D-224), the owner presses the chord with the window minimized, then behind a full-screen editor: the autonomous run is stopped within 10 s each time, the clicked run and the relay go on, and holding the chord stops once. Proof: manual, research notes entry E66 (second half).
6. **In the window** (AD-144, AD-145). `GovernorView` carries the chord and whether it is registered; the Governor section shows it. Proof: core `crates/officina-core/src/governor/view.rs::tests::the_shortcut_state_is_shown`; Playwright `app/e2e/governor.spec.ts::the_shortcut_and_its_registration_are_shown`.
7. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: the stop shortcut. README "The governor (v0.7, in progress)": the default chord, that it stops autonomy only, what happens when another program holds it, that with the window closed `officina stop --all` remains, and that this piece is Windows-only until v1's Linux build. Proof: review.

## Tasks

- [ ] One `windows` CI run registering a free chord on the hosted runner; record the result (D-355)
- [ ] `windows-sys` feature; key map (AC 1)
- [ ] `platform::windows::hotkey`: thread, loop, lifecycle; `AuthorRequest::StopAutonomy` (AC 2, 4)
- [ ] Failure reporting in the view and doctor (AC 3, 6)
- [ ] Gate 5's second half with the owner (AC 5)
- [ ] `CHANGELOG.md`, README (AC 7)

## Dev Notes

- Requirements: FR-16 as amended (amendment 10), FR-14's Stop autonomy, PRD section 5 (the Linux counterpart is a v1 input), section 7's Stop row. Decisions: AD-3 as amended (thread `hotkey`), AD-141; spine section 3 "Rejected for v0.1: Global hotkey" as amended. Facts: addendum section 1, item 7; E66. Risks: R74. Epic decisions: D-135 (Epic 5); P-65, D-300, D-302, D-303, D-335, D-336, D-340, D-341, D-355.
- Seams (D-300): `platform::windows` (1.7, 6.3), the collector's `AuthorRequest` (3.5), the app's start and shutdown sequence.
- Waits for: story 7.16; E66's first half (story 7.13).
- Every `unsafe` block sits in `platform/windows.rs` with its `// SAFETY:` comment (officina-rust rule). No plugin and no crate: two functions of an existing crate.
- The chord only reduces, so it needs no pointer click (D-28 applies to what starts or widens).
- `stop_shortcut` was validated from story 7.2 and is used from here (P-63).

## Testing

- Host unit and integration, Windows only: `crates/officina-host/src/platform/windows.rs`, `crates/officina-host/tests/governor_hotkey.rs`; `crates/officina-host/src/governor/doctor.rs`.
- CLI integration: `crates/officina/tests/watch.rs`. Core unit: `crates/officina-core/src/governor/view.rs`.
- Frontend: Playwright `app/e2e/governor.spec.ts`.
- Manual: research notes entry E66 (second half).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: registration checked once on the hosted runner, its tests ignored there and run on the workstation if the session refuses it (F39); gate 5 is E66 (F7, superseded by the owner's range)
