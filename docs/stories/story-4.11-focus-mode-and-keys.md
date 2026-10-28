# Story 4.11: Focus mode, and the keys in every mode

**Status:** Review

## Story

As the author who needs to concentrate,
I want one key to hide the scene and the board and leave only what waits for me, still numbered so the same digits act, and the same key to bring back what I had,
so that nothing moves and nothing distracts me while I work through the waiting sessions.

## Acceptance Criteria

1. Focus mode (FR-7, AD-66, D-113): `ViewState` gains `focus`; `ModeSwitch.svelte` offers Board, Scene and Focus; `f` turns Focus on, and off back to the base it came from; `m` from Focus switches the base and leaves Focus; Focus survives a reload; the walking toggle is hidden in Focus. Proof: Playwright `app/e2e/modes.spec.ts > f turns Focus on and off back to the previous mode`, `> the mode switch offers Board, Scene and Focus`, `> m from Focus switches the base and leaves Focus`, `> Focus survives a reload`; Vitest `app/src/lib/view.test.ts > a stored view without focus reads as not focused`.
2. The list (AD-66, D-114): `components/FocusList.svelte` shows, when `board.approvals` is not empty, one amber row "N approvals waiting" that switches to Board mode; then the Needs you sessions (number, amber border, and an "alert" pill with its cause for an alerted one) and the Your turn sessions (amber outline) as rows of at least 44 px whose activation is the board card's, with its outcome in the row; then the Working count as text. No scene, no halo, no transition, no frame loop. Proof: Playwright `modes.spec.ts > Focus lists Needs you then Your turn, with Working as a count`, `> a Focus row brings its session to front`, `> Focus shows the approvals waiting and opens them on the board`, `> Focus marks alerted sessions, a last known one included`, `> Focus runs no animation` (`document.getAnimations()` empty), `> Focus rows are 44 px or more`; Vitest `app/src/components/components.test.ts > FocusList renders session text as text`.
3. Digits in every mode (AD-66, amending AD-55): keys 1 to 9 act on the nth Needs you session in Board, Scene and Focus. Proof: Playwright `modes.spec.ts > keys 1 to 9 act on the nth waiting session in Board, Scene and Focus`.
4. Keys are view-scoped (FR-7, D-113): `m`, `f` and digits do nothing in the Subjects and Relay views; triage's `f` in the Subjects view is untouched. Proof: Playwright `modes.spec.ts > m, f and digits do nothing outside the Sessions view`, `> f in the Subjects view still sets the triage verdict`.
5. A mode change closes the session file (D-113), and leaving Scene stops the mover. Proof: Playwright `modes.spec.ts > a mode change closes the session file`, `> leaving Scene mid-walk leaves no frame loop running` (`data-moving` absent and no `requestAnimationFrame` callback pending, checked with the installed clock).
6. The alert band stays in Focus (D-473): story 4.5's band case covers Focus mode. Proof: `alert.spec.ts > the band shows in every view and mode`, extended here.
7. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: Focus mode, `f`; Changed: digits act in every mode); README "The scene": Focus, `f`, digits in every mode, keys only in the Sessions view. Proof: review.

## Tasks

- [x] `lib/view.svelte.ts`: `focus`, derived mode, `f`; `ModeSwitch.svelte` third radio (AC 1)
- [x] `FocusList.svelte` with `lib/outcome.ts` (AC 2)
- [x] `App.svelte` key handling by view and mode; the mover stopped on leaving Scene (AC 3-5)
- [x] `modes.spec.ts` and Vitest cases (AC 1-5)
- [x] Update `CHANGELOG.md` and the README (AC 7)

## Dev Notes

- Requirements: FR-7, FR-3 (focus mode has no halo), J3. Decisions: AD-55 as amended, AD-66. Epic decisions: D-28 (Epic 2); D-113, D-114, D-121, D-473, D-494, D-497.
- Dependencies: story 4.8's file panel (AC 5); story 4.10's mover (AC 5); story 4.3's alert state (AC 2); story 4.5's band (AC 6).
- An unknown session that keeps its alert (D-494) keeps its "alert" pill in Focus, with "last known" beside its cause.
- Focus rows are not `.card.needs_you`, so story 4.4's ring never applies to them.
- `store.needsYou` stays `columns.needs_you`: approvals are not in it, so the digit mapping is the board's; alerted sessions keep their place in it.
- Persistence cases start from `openBoard` or `openScene`, whose init script writes the view state only when the key is absent (story 4.7).

## Testing

- Vitest: `app/src/lib/view.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/modes.spec.ts`.

## Dev Agent Record

### Debug Log

- `ViewState.focus` is a flag over `base` (D-532): `mode` is derived, `setMode` is the one writer, `setBase` and `m` leave Focus, `f` toggles it. A stored view without `focus`, or with a value that is not a boolean, reads as not focused, its other fields kept.
- The header keeps the base's layout in Focus (one line over Scene) and the Walking toggle's room, unseen and unreachable, so `f` moves nothing in it (D-517). The mode switch's arrows cycle the three radios and wrap.
- Leaving Scene unmounts `Scene.svelte`, whose `onDestroy` destroys the mover, which cancels its pending frame; the file panel's state lives in `Scene.svelte`, so a mode change closes it. No new code was needed for AC 5; its two cases prove it, the frame loop through a wrapper around the installed clock's `requestAnimationFrame` that counts the callbacks asked for and neither run nor cancelled.
- Digits already acted in both modes (story 4.8); `FocusList` numbers its Needs you rows from `columns.needs_you`, the list `store.needsYou` is, so a digit and a row number always name the same session.
- AC 4's triage case lives in `triage.spec.ts`, beside triage's key cases (D-532).

### Completion Notes

- AC 1: Playwright `app/e2e/modes.spec.ts > f turns Focus on and off back to the previous mode` (the Walking toggle hidden in Focus, a held key), `> the mode switch offers Board, Scene and Focus`, `> m from Focus switches the base and leaves Focus`, `> Focus survives a reload`; Vitest `app/src/lib/view.test.ts > a stored view without focus reads as not focused`, `> f turns Focus on over the base and off back to it; m leaves Focus for the other base`.
- AC 2: Playwright `modes.spec.ts > Focus lists Needs you then Your turn, with Working as a count`, `> a Focus row brings its session to front`, `> Focus shows the approvals waiting and opens them on the board`, `> Focus marks alerted sessions, a last known one included`, `> Focus runs no animation`, `> Focus rows are 44 px or more`; Vitest `app/src/components/components.test.ts > FocusList renders session text as text`.
- AC 3: Playwright `modes.spec.ts > keys 1 to 9 act on the nth waiting session in Board, Scene and Focus`.
- AC 4: Playwright `modes.spec.ts > m, f and digits do nothing outside the Sessions view`; `app/e2e/triage.spec.ts > f in the Subjects view still sets the triage verdict`.
- AC 5: Playwright `modes.spec.ts > a mode change closes the session file`, `> leaving Scene mid-walk leaves no frame loop running`.
- AC 6: Playwright `app/e2e/alert.spec.ts > the band shows in every view and mode` (Focus added).
- AC 7: `CHANGELOG.md` `[Unreleased]` Added: Focus mode and `f`; Changed: digits in every mode, keys in the Sessions view only. README "Board, Scene and Focus".

## File List

- `app/src/lib/view.svelte.ts`
- `app/src/lib/view.test.ts`
- `app/src/components/ModeSwitch.svelte`
- `app/src/components/Header.svelte`
- `app/src/components/WalkingSwitch.svelte`
- `app/src/components/FocusList.svelte` (new)
- `app/src/components/components.test.ts`
- `app/src/App.svelte`
- `app/e2e/modes.spec.ts`
- `app/e2e/alert.spec.ts`
- `app/e2e/triage.spec.ts`
- `CHANGELOG.md`
- `README.md`
- `docs/stories/epic-4-decisions.md`
- `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created as story 4.6
- 2026-10-28 reworked after review (F26, F37)
- 2026-10-28 renumbered 4.10 for the scene v2 (D-460): alerted rows, the walking toggle hidden in Focus, the mover stopped on leaving Scene
- 2026-10-28 renumbered 4.11 after the plan challenge (D-497, D-498): story numbers updated; a last known alert keeps its pill
- 2026-10-28 implemented: Focus over the base (D-532), `FocusList`, `f`, the keys scoped to the Sessions view; every AC proved, moved to Review
