# Story 4.8: Alert band, beacon, and the taskbar badge

**Status:** Approved

## Story

As the author whose window often sits behind my editor,
I want a really blocked session on an important project, or one that has waited too long, to show as a band in the window, a lit beacon in the alert room and a red badge on the taskbar button that flashes once, with the waiting count on that badge all day,
so that I know when to look without a toast, a sound or a theme change.

## Acceptance Criteria

1. The band (FR-10, AD-175, D-473): `components/AlertBand.svelte` shows, while `board.alert_count > 0`, under the header and above every other banner, in every view and mode (Board, Scene, Focus, Subjects, Relay), with `role="status"`: an "Alert" pill and "{name} ({project}) is blocked: {waiting reason}, {cause}, and N more.", the first alerted session being the first in board order, cause "high-priority project" or "waiting N min" worded from `waiting_since_ms` on the board store's tick; no dismiss; gone when nobody is in alert. Proof: Playwright `app/e2e/alert.spec.ts > the band names the first alerted session, its reason and cause`, `> the band shows in every view and mode`, `> the band counts the other alerted sessions`, `> the band leaves with the last alert`; Vitest `app/src/components/components.test.ts > AlertBand renders session text as text`, `> AlertBand words both causes`.
2. The theme never changes (FR-8): with an alert, every `--color-*` and `--scene-*` token resolved on `:root` equals its value without one, in both themes. Proof: Playwright `alert.spec.ts > an alert changes no theme token`.
3. The alert room (AD-175, D-474): while the room holds someone, its floor and hazard outline, the status board's mark and its plaque take the alert tokens; the beacon is lit, its glow uses story 4.4's `breathe` keyed on the first alerted card's `(key, waiting_since_ms)` (two minutes, shared phase, then steady; steady under reduced motion), and the beam is static; an office of a high-priority project carries a "high priority" pill. Proof: Playwright `alert.spec.ts > the alert room lights its beacon while occupied`, `> the beacon glow animates opacity only and stops after two minutes` (clock and `last_event_at_ms` rewritten as D-101), `> under reduced motion the beacon is steady`, `> a high-priority office carries its pill`; Vitest `app/src/lib/scene/layout.test.ts > the alert room's state follows its occupants`.
4. Time escalation end to end (FR-10, AD-168, D-467): on `scene-alert`, with the threshold of its floor, the session whose wait crosses the threshold walks from the decision office to the alert room and the band appears, from the board alone. Proof: Playwright `alert.spec.ts > a wait that reaches the threshold moves to the alert room` (two pushed boards built by the real CLI at times either side of the threshold); story 4.3's host test covers the recompute without spawn.
5. The overlay badge (FR-11, AD-174, D-475): `app/src-tauri/src/taskbar.rs`'s `overlay_badge(count, alert, size)` (story 4.1) is used at the size gate 2 recorded; `WindowEvents::board` sets no overlay when `waiting_count` is 0, else the badge with the count ("9+" above nine), amber, or the alert colour while `alert_count > 0`. Proof: Rust unit `app/src-tauri/src/taskbar.rs::tests::{no_overlay_without_waiting, the_badge_turns_to_the_alert_colour}` (the function choosing the overlay from a board is pure and tested without a window).
6. Colours derived (`derived-not-transcribed`, AD-174): a Rust test reads `app/src/app.css` and asserts the overlay's amber, dark text, alert and light text colours equal the light `--color-attention`, `--color-attention-text`, `--color-alert` and `--color-on-alert` values. Proof: `taskbar.rs::tests::overlay_colours_equal_the_light_tokens`.
7. The title (AD-174): `Officina (N waiting)`, or `Officina (N waiting, M in alert)` while `alert_count > 0`. Proof: `taskbar.rs::tests::the_title_counts_alerts` (the title builder moves from `main.rs` into `taskbar.rs`).
8. The flash (FR-11, AD-174, D-475): `WindowEvents` keeps the alerted card keys of the last emitted board; `alerts_to_announce(previous, board)` is true when the board holds an alerted key the previous set did not; then the app calls `request_user_attention(Some(UserAttentionType::Informational))` once. A known key never flashes again; the first board after start with alerts flashes once. Proof: `taskbar.rs::tests::{a_new_alerted_key_is_announced, a_known_key_is_not_announced_again, a_key_that_leaves_and_returns_is_announced_again, the_first_board_with_alerts_is_announced}`; manual: gate 2's entry and 4.12's check on the real app.
9. No new dependency and no capability (AD-174): the calls are Rust; `app/src-tauri/capabilities/default.json` is unchanged. Proof: review of the diff; `cargo deny check` in CI.
10. Gate 2's size (D-479): this story closes only after story 4.1's gate 2 entry; if the count was illegible at every size, the overlay keeps today's dot, recoloured on alert, and AC 5 and the tests follow that fallback. Proof: the research-notes entry referenced in Completion Notes.
11. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: alert by priority and threshold, the alert band, the taskbar badge with the waiting count and its flash on a new alert); README: the alert, the two floor keys (story 4.2), "no toast". Proof: review.

## Tasks

- [ ] `AlertBand.svelte` in `App.svelte` above the banners; wording function (AC 1, 2)
- [ ] Alert room, beacon and pill states in the layout and the drawing (AC 3)
- [ ] `app/src-tauri/src/taskbar.rs`: overlay choice from a board, title, `alerts_to_announce`, colour test; `main.rs` wiring (AC 5-9)
- [ ] `app/e2e/alert.spec.ts` (AC 1-4)
- [ ] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-8, FR-10, FR-11. Decisions: AD-19 as amended, AD-65, AD-168, AD-174, AD-175. Epic decisions: D-101, D-465, D-467, D-473 to D-475, D-479.
- Dependencies: stories 4.3 (`Card.scene.alert`, `alert_count`), 4.4 (`breathe`), 4.6 (the drawn scene); story 4.1's gate 2 before closing.
- Verified (AD-174): `set_overlay_icon` is Windows only and already used by `main.rs`; `request_user_attention` with `Informational` flashes the taskbar button until the window is focused and does nothing when it is focused; `set_badge_count` is unsupported on Windows. All three are on the Rust `WebviewWindow`, so no capability is needed; the JavaScript forms would need `core:window:allow-set-overlay-icon` and `core:window:allow-request-user-attention`, which are not added.
- The overlay is drawn in code, like today's `waiting_dot`: a bitmap digit font of a few constants, no asset and no image crate. Keep the drawing in a pure function so the tests compare pixels without a window.
- The flash is an OS attention request that ends on focus; it is not tied to the reduced-motion setting (D-475). A second alert while the first still flashes is a new key and calls again: harmless, Windows keeps one flash.
- The band lives outside the scene paths, so it may word a relative time from the store's tick; the scene never does.
- The prototype's taskbar mock (the "attention" fill and the 6 s flash) is a mock of the OS behaviour; the real flash lasts until focus.

## Testing

- Rust unit: `app/src-tauri/src/taskbar.rs`.
- Vitest: `app/src/components/components.test.ts`, `app/src/lib/scene/layout.test.ts`.
- Playwright: `app/e2e/alert.spec.ts`.
- Manual: gate 2 (story 4.1), the real app in 4.12.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created with the scene v2 rework (D-460)
