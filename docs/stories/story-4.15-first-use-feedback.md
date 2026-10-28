# Story 4.15: The owner's first-use feedback

**Status:** Review

## Story

As the author who has just used Officina v0.4 on his real sessions,
I want sober controls, a window that uses my wide screens, a header that stays the same everywhere, a way to move an ended card where it belongs (done, or the trash for a ten-second test), a minimum window size and smoother transitions, and the archive bug fixed,
so that Officina reads as the professional tool it is and keeps my board true to what I decided.

The owner's feedback of 2026-10-28, after using the installed build of `44d0f92`: "Already pretty good", then nine items, numbered below as he gave them (item 9 came after the first eight).

## Acceptance Criteria

1. Sober controls (item 1, D-539): buttons, chips, toggles, inputs and segmented controls have a small radius from one token family of `app.css` (`--radius-control` 4 px, `--radius-surface` 6 px, `--radius-pill` for count badges only, `--radius-round` for dots and discs); no component declares a radius of its own; D-516 and D-517 hold. Proof: Vitest `tokens.test.ts > radii > the detector takes tokens and refuses literals`, `> no component rounds a corner with anything but the radius tokens`, `> controls and surfaces keep small corners`; Playwright `stable-size.spec.ts` (every case).
2. Wide screens (item 2, D-543): from the minimum window size up to 3440 x 1392, no card stretches (480 px at most), a board column lays cards side by side when it has the room, the Subjects and Relay views stop at a reading width, centred, and the session file docks beside the scene when the window leaves room; the scale rules of AD-61 and AD-169 stand; eight projects still fit a maximised Full HD window. Proof: Playwright `wide.spec.ts > the board uses the width without stretching a card at 1920 x 1032` and `at 3440 x 1392`, `> views to read stop at a reading width at ...` (both), `> the session file docks beside the scene when there is room at ...` (both), each attaching its screenshot; `scene.spec.ts > eight projects fit a maximised Full HD window` and `> ... with the alert band`.
3. Moving an ended card (item 3, D-536 to D-538, D-542): an ended card can be dragged onto Done (the author's verdict, recorded with source `author`, never inferred), back onto To resume, or onto a Discard target for a session that was never real work; a discarded session leaves To resume, the board and the scene, is listed with the archived subjects and restored from there, and leaves the registry consistent; each drag has a keyboard and menu equivalent (Mark done, Back to To resume, Discard) with focus handling and Undo; a verdict covers what the session did before it, and a live card is never moved. Proof: Rust `confirm.rs::tests::settling_a_session_writes_only_its_verdict`, `board.rs::tests::the_author_marks_done_or_discards_an_ended_session`, `> the_author_verdict_wins_over_the_archived_subject`, `> a_verdict_older_than_the_last_event_and_a_live_card_are_ignored`, `> no_settlement_leaves_the_card_where_its_sources_place_it`, `verdict.rs::tests::a_board_verdict_answers_an_unfiled_row_for_the_end_it_followed`, `registry_read.rs::session_record_round_trips_with_each_source` (the `settled` fact), `crates/officina/tests/watch.rs::author_verdicts_move_an_ended_session_on_the_board`; the `settle-after` fixture's self-check against the real CLI; Vitest `settle.test.ts` (six cases); Playwright `settle.spec.ts > dragging a card to Done marks it done, and Undo takes it back`, `> dragging a card to the discard target discards it; Escape cancels a drag`, `> a live card has no menu and does not drag`, `> the card menu does it from the keyboard, the focus on Undo, then back on the card`, `> a card done by its archived subject goes back by reopening the subject`, `> a refused verdict says why and moves nothing`, `> a discarded session is listed with the archived subjects and restored from there`.
4. To resume and Done in the header (item 4, D-541): fixed controls under D-517, in every view and mode, opening Board mode on their column. Proof: Playwright `scene.spec.ts > the header counts To resume and Done and opens Board mode on the column`, `> an empty board has no target and the header's counts say zero`, `stable-size.spec.ts > ended counts keeps its box and moves nothing after it`; Vitest `components.test.ts > EndedCounts counts each column, words the Done window in its tooltip, and opens each column`.
5. Minimum window size (item 5, D-546): `tauri.conf.json`'s main window has `minWidth` 1140 and `minHeight` 820, derived from the layouts; the bundle config overrides no window. Proof: Playwright `min-size.spec.ts > the minimum window holds the header on two lines and the base floor at MIN_SCALE`, which reads the numbers from the file.
6. The scene's shadow (item 6, D-544): removed, in both themes. Proof: the after screenshots (Completion Notes); review.
7. The archive bug (item 7, D-535): archiving a subject moves its ended session out of To resume, to Done, and reopening it brings the session back. Proof: Rust `board.rs::tests::an_archived_subject_takes_its_ended_session_out_of_to_resume` (written first, failing on the code before the fix), `watch.rs::author_verdicts_move_an_ended_session_on_the_board`; the `settle-before` fixture's self-check; Playwright `settle.spec.ts > an archived subject's ended session leaves To resume for Done`.
8. Transitions (item 8, D-545): views and modes, cards, panels and menus, the alert band and the chips fade or slide, 160 ms, opacity and transform only, none under reduced motion, none continuous; FR-9's existing cases stay green; the scene's figure motion rules stand. Proof: Playwright `motion.spec.ts > every transition is short and moves opacity or transform only, and none outlives itself`, `> under reduced motion no transition takes any time`, `halo.spec.ts > board halos breathe without style recalculation`, `modes.spec.ts > Focus runs no animation`, `walk.spec.ts`.
9. One header (item 9, D-540): the quota gauges, the buttons and the "contract held" chip have identical boxes in Board, Scene, Focus, Subjects, Relay and Triage, at 1920 x 1032 and 3440 x 1392; D-533's Scene-only compact gauges are reconciled into one form. Proof: Playwright `header.spec.ts > the header is the same in every view and mode at 1920 x 1032` and `at 3440 x 1392`, `modes.spec.ts > the walking toggle shows in every view and mode`, `scene.spec.ts > eight projects fit a maximised Full HD window with the alert band`.
10. Documentation: the story, the decisions (D-535 to D-546), README, `CHANGELOG.md` `[Unreleased]`, `sprint-status.yaml`, the Epic 4 report's 4.15 section. Proof: review.

## Tasks

- [x] Item 7 first: a failing board test, the root cause, the board reading the registry's settlements (AC 7)
- [x] The `settled` fact, the `Settle` action, `list_discarded`, the Tauri commands, the triage rule, the fixtures (AC 3)
- [x] Radius tokens and every component on them (AC 1)
- [x] One header, the counts in it, the line under the scene removed (AC 4, 9)
- [x] Drag, card menu, note with Undo, discard target, discarded list (AC 3)
- [x] Shadow removed (AC 6)
- [x] Wide screens (AC 2)
- [x] Minimum window size (AC 5)
- [x] Transitions (AC 8)
- [x] README, CHANGELOG, README assets, decisions, report, sprint status (AC 10)

## Dev Notes

- Requirements: PRD v0.4 FR-6 (keyboard), FR-7 (modes), FR-9 (under 1 % of the machine, D-515); PRD v0.3 FR-6, FR-7, FR-9 (subjects, verdicts). Decisions: D-516, D-517, D-526, D-533 and D-535 to D-546.
- The board's To resume column comes from live sources (AD-11); the registry gave cards their subject's name and nothing else, which is item 7's root cause. The settlement keeps that split: the board still classifies from Claude Code's facts, and the author's verdicts, from the registry, only move an ended card or take it off.
- No migration: `session_fact.fact` has no `CHECK` (D-88). Epic 5's migration 2 stays free.
- No new dependency: `svelte/transition` and `svelte/animate` ship with Svelte.

## Testing

- Rust: `crates/officina-core/src/{board.rs, registry/confirm.rs, registry/verdict.rs}`, `crates/officina-host/tests/registry_read.rs`, `crates/officina/tests/watch.rs`, `tests/vectors/scene.json`.
- Vitest: `app/src/lib/settle.test.ts`, `app/src/tokens.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/{settle,header,wide,min-size,motion}.spec.ts` (new), `{scene,modes,stable-size,halo,walk}.spec.ts`.
- Fixtures: `settle-before` and `settle-after` in `app/scripts/e2e-fixtures.mjs`, written by the window's writer and printed by the real CLI, each with its self-check.

## Dev Agent Record

### Debug Log

- Item 7 reproduced in Rust before the fix: `an_archived_subject_takes_its_ended_session_out_of_to_resume` builds a board from three ended sessions and a settlement map; without the board reading it, the archived one stays in To resume. The real-path proofs are the watch test (the collector thread archives, the next board moves the card, reopen brings it back) and the `settle-before` fixture, whose self-check runs on `officina board --json`.
- The discarded list's excerpt is often empty: archiving a subject clears its excerpts (FR-4), so a session discarded after its subject was archived shows its name or its id; the watch test asserts it.
- The header with To resume and Done added overflowed the one line at 1920 px with the relay in use (the status chips wrapped, the header grew 19 px, eight projects with the alert band scrolled by 13 px). Measured item by item: gauges 150 px, buttons' padding 12 px, "Pin", two-digit room for the counts, "N done" with the window in the tooltip; the status slot is then 197 px for its 175 px of chips.
- The header spec measured the Triage case 73 px off: the panel takes the focus and scrolls the page; the spec measures from the top.
- The colour literal detector read `role="menu"` and `aria-haspopup="menu"` as the deprecated system colour `Menu`; it now skips class names and ARIA attributes, with a detector case.
- `pnpm check` caught a spread of a `NodeList` in `header.spec.ts` that Chromium runs but the node typecheck refuses; fixed in its own commit.
- Transitions: Svelte 5 runs them as Web Animations, which the global `prefers-reduced-motion` CSS rule does not reach, hence `lib/motion.ts` reading the setting at each run. A card fading out is still in the DOM, inert: the focus after an undo skips it. Specs that measure a settled page (`halo.spec.ts`'s recalculation count, the file's position, Focus's animation count, the README assets) wait for the transitions to end; `openBoard` and `openScene` do so for every spec.
- `walk.spec.ts > the frame loop stops after arrival` failed once, then 1 run in 30: the arrived figure's halo writes its delay when its animation starts (D-522), sometimes after the observer began; nothing in this story touches the halo. The case now ignores that write, which is the halo's, not the loop's; 40 repeats pass.
- Drag uses pointer capture, and the drop target is found under the pointer with the dragged card's own elements skipped, rather than with `pointer-events: none`, which pointer capture does not need and which would risk losing the captured events.
- The minimum width: 1124 px (floor plus the window's scrollbar room) still scrolled by 14 px, the scene area keeping its own scrollbar room; 1140 holds both. The board's column minimum went from 210 to 200 px for five columns at that width.

### Completion Notes

- AC 1: `tokens.test.ts > radii > the detector takes tokens and refuses literals`, `> no component rounds a corner with anything but the radius tokens`, `> controls and surfaces keep small corners`; `stable-size.spec.ts` (every case).
- AC 2: `wide.spec.ts` (six cases, screenshots attached); `scene.spec.ts > eight projects fit a maximised Full HD window` (with and without the alert band).
- AC 3: `confirm.rs::tests::settling_a_session_writes_only_its_verdict`; `board.rs::tests::{the_author_marks_done_or_discards_an_ended_session, the_author_verdict_wins_over_the_archived_subject, a_verdict_older_than_the_last_event_and_a_live_card_are_ignored, no_settlement_leaves_the_card_where_its_sources_place_it}`; `verdict.rs::tests::a_board_verdict_answers_an_unfiled_row_for_the_end_it_followed`; `registry_read.rs::session_record_round_trips_with_each_source`; `watch.rs::author_verdicts_move_an_ended_session_on_the_board`; `settle.test.ts` (six cases); `settle.spec.ts` (seven cases after the item 7 case); the `settle-after` fixture's checks.
- AC 4: `scene.spec.ts > the header counts To resume and Done and opens Board mode on the column`, `> an empty board has no target and the header's counts say zero`; `stable-size.spec.ts > ended counts keeps its box and moves nothing after it`; `components.test.ts > EndedCounts ...`.
- AC 5: `min-size.spec.ts > the minimum window holds the header on two lines and the base floor at MIN_SCALE`.
- AC 6: screenshots before and after, 1920 x 1032 and 3440 x 1392, light and dark, Board and Scene (not committed; their paths are in the lead's report).
- AC 7: `board.rs::tests::an_archived_subject_takes_its_ended_session_out_of_to_resume`; `watch.rs::author_verdicts_move_an_ended_session_on_the_board`; `settle.spec.ts > an archived subject's ended session leaves To resume for Done`; the `settle-before` fixture's checks.
- AC 8: `motion.spec.ts` (two cases); `halo.spec.ts > board halos breathe without style recalculation`; `modes.spec.ts > Focus runs no animation`; `walk.spec.ts`.
- AC 9: `header.spec.ts` (two cases); `modes.spec.ts > the walking toggle shows in every view and mode`; `scene.spec.ts > eight projects fit a maximised Full HD window with the alert band`.
- AC 10: this file, `epic-4-decisions.md` D-535 to D-546, README ("Moving an ended card", the header, wide screens, the minimum size, the transitions), `CHANGELOG.md`, `docs/qa/epic-4-report.md` "Story 4.15", `sprint-status.yaml`. The README assets were rendered again from this build.
- Not verified on the real window (the owner is away; the mandate is not to open it): the drag in WebView2, the minimum size as Windows applies it, the transitions' feel. Each is covered in Playwright's Chromium; the owner's next use confirms them.

## File List

- `crates/officina-core/src/board.rs`
- `crates/officina-core/src/registry/{types.rs, fold.rs, confirm.rs, verdict.rs, view.rs}`
- `crates/officina-core/tests/typescript_bindings.rs`
- `crates/officina-host/src/{collector.rs, focus.rs, watch.rs}`
- `crates/officina-host/src/registry/{commands.rs, read.rs, window.rs, write.rs}`
- `crates/officina-host/tests/registry_read.rs`
- `crates/officina/tests/watch.rs`
- `tests/vectors/scene.json` (gate 4's case, recorded with story 4.3)
- `app/src-tauri/{build.rs, capabilities/default.json, src/main.rs, tauri.conf.json}`
- `app/src/app.css`, `app/src/App.svelte`, `app/src/tokens.test.ts`, `app/src/e2e-mock.ts`, `app/src/e2e-globals.d.ts`
- `app/src/lib/{ipc.ts, board.svelte.ts, registry.svelte.ts, settle.ts, settle.test.ts, drag.svelte.ts, motion.ts}` (`settle*`, `drag*`, `motion*` new)
- `app/src/lib/generated/{Card.ts, Settled.ts, Settlement.ts, DiscardedSession.ts}`
- `app/src/lib/scene/{geometry.ts, geometry.test.ts, furniture.ts, testing.ts}`, `app/src/lib/board.test.ts`
- `app/src/components/{Header, Gauge, ModeSwitch, ThemeSwitch, WalkingSwitch, EndedCounts, BoardColumn, SessionCard, CardMenu, SettleNote, DiscardTarget, ArchivedList, AlertBand, FocusList, SubjectsView, RelayView, TriagePanel, SessionFile}.svelte` and every component whose corner radius moved to the tokens; `components.test.ts`
- `app/src/components/scene/{Scene.svelte, scene.test.ts}`; `SceneStrip.svelte` removed (now `EndedCounts.svelte`)
- `app/scripts/e2e-fixtures.mjs`
- `app/e2e/{settle,header,wide,min-size,motion}.spec.ts` (new), `app/e2e/{fixtures.ts, scene.spec.ts, modes.spec.ts, stable-size.spec.ts, halo.spec.ts, walk.spec.ts}`
- `docs/assets/{board.png, demo.gif, scene.png, scene-dark.png, scene.gif}`
- `README.md`, `CHANGELOG.md`
- `docs/stories/epic-4-decisions.md`, `docs/stories/sprint-status.yaml`, `docs/qa/epic-4-report.md`

## Change Log

- 2026-10-28 created from the owner's first-use feedback on the installed build of `44d0f92` (items 1 to 8, then item 9), implemented and documented the same day; decisions D-535 to D-546 taken under delegation. Status review
