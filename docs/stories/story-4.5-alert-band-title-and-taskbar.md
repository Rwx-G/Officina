# Story 4.5: The alert band, the window title, and the taskbar badge

**Status:** In progress (code and tests done; AC 10 waits for gate 2, story 4.1, which picks the badge's size)

## Story

As the author whose window often sits behind my editor,
I want a really blocked session on an important project, or one that has waited too long, to show as a band in the window, in the window's title and as a red badge on the taskbar button that flashes once, with the waiting count on that badge all day,
so that I know when to look without a toast, a sound or a theme change, from the first week of v0.4 and before any drawing.

## Acceptance Criteria

1. The band (FR-10, AD-175, D-473, D-494): `components/AlertBand.svelte` shows, while `board.alert_count > 0`, under the header and above every other banner, in every view and mode that exists (Board, Subjects, Relay; stories 4.7 and 4.11 add Scene and Focus to the same case), with `role="status"`: an "Alert" pill and "{name} ({project}) is blocked: {waiting reason}, {cause}, and N more.", the first alerted session being the first in board order, cause "high-priority project" or "waiting N min" worded from `waiting_since_ms` on the board store's tick; while any alerted card's `scene.zone` is `unknown`, the text ends with "Last known state: data unknown."; no dismiss; gone when nobody is in alert. Proof: Playwright `app/e2e/alert.spec.ts > the band names the first alerted session, its reason and cause`, `> the band shows in every view and mode`, `> the band counts the other alerted sessions`, `> the band leaves with the last alert`; Vitest `app/src/components/components.test.ts > AlertBand renders session text as text`, `> AlertBand words both causes`, `> AlertBand marks a last known alert on unknown data` (a fixture board whose alerted card is rewritten to `zone: "unknown"` with its alert kept, as core builds it on a stale board).
2. Alert tokens (AD-67 as amended, D-117): the prototype v2's light `--color-alert` and `--color-on-alert` join `app.css`, its only copy; the contrast test's pairs gain text on the alert colour (4.5) and the alert colour on the background (3), the alert colour on amber staying decor. Proof: Vitest `app/src/tokens.test.ts > every role pair meets its contrast target in the light theme`.
3. The theme never changes (FR-8): with an alert, every `--color-*` token resolved on `:root` equals its value without one. Proof: Playwright `alert.spec.ts > an alert changes no theme token` (story 4.7 adds the `--scene-*` tokens, story 4.13 the dark run).
4. Time escalation end to end (FR-10, AD-168, D-467, D-496): on `scene-alert`, with the threshold of its floor, two boards built by the real CLI at times either side of the threshold are pushed: the band appears with the second, from the board alone; the approval-only wait of the scenario's normal project never reaches the band. Proof: Playwright `alert.spec.ts > a wait that reaches the threshold raises the band`, `> an approval-only wait on a normal project raises no band`; story 4.3's host test covers the recompute without spawn.
5. The overlay badge (FR-11, AD-174, D-475): `app/src-tauri/src/taskbar.rs`'s `overlay_badge(count, alert, size)` (story 4.1) is used at the size gate 2 recorded; `WindowEvents::board` sets no overlay when `waiting_count` is 0, else the badge with the count ("9+" above nine), amber, or the alert colour while `alert_count > 0`, unknown cards that keep their alert included. Proof: Rust unit `app/src-tauri/src/taskbar.rs::tests::{no_overlay_without_waiting, the_badge_turns_to_the_alert_colour, a_last_known_alert_keeps_the_alert_colour}` (the function choosing the overlay from a board is pure and tested without a window).
6. Colours derived (`derived-not-transcribed`, AD-174): a Rust test reads `app/src/app.css` and asserts the overlay's amber, dark text, alert and light text colours equal the light `--color-attention`, `--color-attention-text`, `--color-alert` and `--color-on-alert` values. Proof: `taskbar.rs::tests::overlay_colours_equal_the_light_tokens`.
7. The title (AD-174 as amended, D-494): `Officina (N waiting)`; `Officina (N waiting, M in alert)` while `alert_count > 0`; `Officina (N waiting, M in alert, data unknown)` while an alerted card's zone is `unknown`. Proof: `taskbar.rs::tests::{the_title_counts_alerts, the_title_marks_a_last_known_alert}` (the title builder moves from `main.rs` into `taskbar.rs`).
8. The flash (FR-11, AD-174, D-475, D-494): `WindowEvents` keeps the alerted card keys of the last emitted board; `alerts_to_announce(previous, board)` is true when the board holds an alerted key the previous set did not; then the app calls `request_user_attention(Some(UserAttentionType::Informational))` once. A known key never flashes again, including one that stays alerted while unknown; the first board after start with alerts flashes once. Proof: `taskbar.rs::tests::{a_new_alerted_key_is_announced, a_known_key_is_not_announced_again, a_key_that_leaves_and_returns_is_announced_again, the_first_board_with_alerts_is_announced, a_key_kept_alerted_while_unknown_is_not_announced}`; manual: gate 2's entry and story 4.14's check on the real app.
9. No new dependency and no capability (AD-174): the calls are Rust; `app/src-tauri/capabilities/default.json` is unchanged. Proof: review of the diff; `cargo deny check` in CI.
10. Gate 2's size (D-479): this story closes only after story 4.1's gate 2 entry; if the count was illegible at every size, the overlay keeps today's dot, recoloured on alert, and AC 5 and the tests follow that fallback. Proof: the research-notes entry referenced in Completion Notes.
11. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: alert by priority and threshold, the alert band, the taskbar badge with the waiting count and its flash on a new alert, the title's alert count); README: the alert, the two floor keys (story 4.2), "no toast", the last known alert kept on stale data. Proof: review; `crates/officina/tests/readme.rs` stays green.

## Tasks

- [x] `--color-alert` and `--color-on-alert` in `app.css`; contrast pairs (AC 2)
- [x] `AlertBand.svelte` in `App.svelte` above the banners; wording function with the unknown mark (AC 1, 3)
- [x] `app/src-tauri/src/taskbar.rs`: overlay choice from a board, title, `alerts_to_announce`, colour test; `main.rs` wiring (AC 5-9)
- [x] `app/e2e/alert.spec.ts`; Vitest cases (AC 1, 3, 4)
- [ ] Check story 4.1's gate 2 entry before closing (AC 10) (open: the owner's gate 2)
- [x] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-8, FR-10, FR-11; J5. Decisions: AD-19 as amended, AD-67, AD-168, AD-174, AD-175. Epic decisions: D-101, D-117, D-465, D-467, D-473, D-475, D-479, D-494, D-496, D-497.
- Dependencies: story 4.3 (`Card.scene.alert`, `alert_count`, the unknown carry-over); story 4.2's `scene-alert`; story 4.1's gate 2 before closing. Nothing of the scene: this story ships the alert before any drawing (D-497); the alert room and its beacon are story 4.9.
- Verified (AD-174): `set_overlay_icon` is Windows only and already used by `main.rs`; `request_user_attention` with `Informational` flashes the taskbar button until the window is focused and does nothing when it is focused; `set_badge_count` is unsupported on Windows. All three are on the Rust `WebviewWindow`, so no capability is needed; the JavaScript forms would need `core:window:allow-set-overlay-icon` and `core:window:allow-request-user-attention`, which are not added.
- The overlay is drawn in code, like today's `waiting_dot`: a bitmap digit font of a few constants, no asset and no image crate. Keep the drawing in a pure function so the tests compare pixels without a window. The overlay image cannot carry a "data unknown" mark; the title carries it, and Windows shows the title on the taskbar button (D-494).
- The flash is an OS attention request that ends on focus; it is not tied to the reduced-motion setting (D-475). A second alert while the first still flashes is a new key and calls again: harmless, Windows keeps one flash.
- The band lives outside the scene paths, so it may word a relative time from the store's tick; the scene never does. Its "data unknown" sentence reads `card.scene.zone`, which core sets; the frontend never infers staleness itself (AD-13).
- The prototype's taskbar mock (the "attention" fill and the 6 s flash) is a mock of the OS behaviour; the real flash lasts until focus.

## Testing

- Rust unit: `app/src-tauri/src/taskbar.rs`.
- Vitest: `app/src/components/components.test.ts`, `app/src/tokens.test.ts`.
- Playwright: `app/e2e/alert.spec.ts`.
- Manual: gate 2 (story 4.1), the real app in story 4.14.

## Dev Agent Record

### Debug Log

- The band's wording lives in `lib/alert.ts` (pure, tested through the component): the first alerted card in board order (columns in display order, so an alerted Needs you comes before an alerted approval-only wait of Working), what it waits on (`waiting_for`, else "a relay approval" for an approval-only wait, else "waiting for you"), and its cause, "high-priority project" or "waiting N min" from `scene.waiting_since_ms` at the store's 30 s tick. The words of a cause come from an exhaustive `Record<AlertCause, string>`, so a new cause fails `pnpm check`.
- `taskbar::overlay_for` also shows the badge, in the alert colour, when `waiting_count` is 0 and a card keeps its last known alert (an alerted card whose badge became unknown sits in Working, which `waiting_count` does not count): D-494 says stale or unknown data never clears an alert, and an overlay that vanished would. The badge then carries the larger of the two counts. The title says "N waiting" only when N is above 0.
- `WindowEvents` keeps the alerted keys of the last board it emitted; `alerts_to_announce` is the pure decision. Boards are emitted only on change (AD-5), so a key that stays alerted is never announced twice.
- The overlay's colours are constants of `taskbar.rs`, tested against `app.css`'s light `:root` block; today's rim is `--color-text`. The size stays 16 px (today's dot) until gate 2 picks one, a one-constant change in `main.rs`.
- No capability: the calls are the Rust `WebviewWindow` methods; `capabilities/default.json` is unchanged (the existing `every_declared_command_is_allowed` test still holds it against `build.rs`).
- Adversarial pass: a waiting count of 0 with a kept alert (handled, above); a key that leaves and returns (announced again); a key kept alerted while unknown (not announced); markup in a session name (text); a fixture with both causes; the band leaving on a board without alerts; theme tokens compared before and after an alert.

### Completion Notes

- AC 1: Playwright `alert.spec.ts > {the band names the first alerted session, its reason and cause, the band shows in every view and mode, the band counts the other alerted sessions, the band leaves with the last alert}`; Vitest `components.test.ts > AlertBand > {AlertBand renders session text as text, AlertBand words both causes, AlertBand marks a last known alert on unknown data, AlertBand is absent without an alert}`. Scene and Focus join the "every view and mode" case in stories 4.7 and 4.11.
- AC 2: Vitest `tokens.test.ts > every role pair meets its contrast target in the light theme`.
- AC 3: Playwright `alert.spec.ts > an alert changes no theme token` (story 4.7 adds the `--scene-*` tokens, story 4.13 the dark run).
- AC 4: Playwright `alert.spec.ts > {a wait that reaches the threshold raises the band, an approval-only wait on a normal project raises no band}` on `scene-alert-early` and `scene-alert`; the recompute without spawn is story 4.3's `watch_escalation.rs`.
- AC 5: `taskbar.rs::tests::{no_overlay_without_waiting, the_badge_turns_to_the_alert_colour, a_last_known_alert_keeps_the_alert_colour}`.
- AC 6: `taskbar.rs::tests::overlay_colours_equal_the_light_tokens`.
- AC 7: `taskbar.rs::tests::{the_title_counts_alerts, the_title_marks_a_last_known_alert}`.
- AC 8: `taskbar.rs::tests::{a_new_alerted_key_is_announced, a_known_key_is_not_announced_again, a_key_that_leaves_and_returns_is_announced_again, the_first_board_with_alerts_is_announced, a_key_kept_alerted_while_unknown_is_not_announced}`; the flash on the real window is gate 2 and story 4.14 (owner).
- AC 9: the diff: no dependency, no capability.
- AC 10: open until story 4.1's gate 2 entry (owner).
- AC 11: `CHANGELOG.md` Added; README "The scene (v0.4, in progress)", "The alert"; `readme.rs` green.

## File List

- `app/src-tauri/src/taskbar.rs`, `app/src-tauri/src/main.rs`, `app/src-tauri/examples/taskbar_probe.rs`
- `app/src/lib/alert.ts` (new), `app/src/components/AlertBand.svelte` (new), `app/src/App.svelte`, `app/src/app.css`
- `app/src/components/components.test.ts`, `app/src/tokens.test.ts`, `app/e2e/alert.spec.ts` (new), `app/e2e/fixture-index.ts`, `app/playwright.config.ts`
- `README.md`, `CHANGELOG.md`

## Change Log

- 2026-10-28 created as story 4.8 with the scene v2 rework (D-460)
- 2026-10-28 renumbered 4.5 and narrowed after the plan challenge (D-494, D-496, D-497): the band, the title and the taskbar ship right after story 4.3, needing core only; the alert tokens move here; the alert room and the beacon move to story 4.9; the last known alert on unknown data is marked in the band and the title; the approval-only wait raises no band by time
- 2026-10-28 implemented: the alert band, the alert tokens, the taskbar badge's alert colour, the title's alert count and the flash on a new alert. Status in progress until gate 2 picks the badge's size
