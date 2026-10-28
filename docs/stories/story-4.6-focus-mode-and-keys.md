# Story 4.6: Focus mode, and the keys in every mode

**Status:** Approved

## Story

As the author who needs to concentrate,
I want one key to hide the scene and the board and leave only what waits for me, still numbered so the same digits act, and the same key to bring back what I had,
so that nothing moves and nothing distracts me while I work through the waiting sessions.

## Acceptance Criteria

1. Focus mode (FR-7 as amended, AD-66, D-113): `ViewState` gains `focus`; `ModeSwitch.svelte` offers Board, Scene and Focus; `f` turns Focus on, and off back to the base it came from; `m` from Focus switches the base and leaves Focus; Focus survives a reload. Proof: Playwright `app/e2e/modes.spec.ts > f turns Focus on and off back to the previous mode`, `> the mode switch offers Board, Scene and Focus`, `> m from Focus switches the base and leaves Focus`, `> Focus survives a reload`; Vitest `app/src/lib/view.test.ts > a stored view without focus reads as not focused`.
2. The list (AD-66, D-114): `components/FocusList.svelte` shows, when `board.approvals` is not empty, one amber row "N approvals waiting" that switches to Board mode; then the Needs you sessions (number, amber border) and the Your turn sessions (amber outline) as rows of at least 44 px whose activation is the board card's, with its outcome in the row; then the Working count as text. No scene, no halo, no transition. Proof: Playwright `modes.spec.ts > Focus lists Needs you then Your turn, with Working as a count`, `> a Focus row brings its session to front`, `> Focus shows the approvals waiting and opens them on the board`, `> Focus runs no animation` (`document.getAnimations()` empty), `> Focus rows are 44 px or more` (review F26); Vitest `app/src/components/components.test.ts > FocusList renders session text as text`.
3. Digits in every mode (AD-66, amending AD-55): keys 1 to 9 act on the nth Needs you session in Board, Scene and Focus. Proof: Playwright `modes.spec.ts > keys 1 to 9 act on the nth waiting session in Board, Scene and Focus`.
4. Keys are view-scoped (FR-7 as amended, AD-66, D-113): `m`, `f` and digits do nothing in the Subjects and Relay views; triage's `f` in the Subjects view is untouched. Proof: Playwright `modes.spec.ts > m, f and digits do nothing outside the Sessions view`, `> f in the Subjects view still sets the triage verdict`.
5. A mode change closes the session file (D-113). Proof: Playwright `modes.spec.ts > a mode change closes the session file`.
6. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: Focus mode, `f`; Changed: digits act in every mode); README "The scene": Focus, `f`, digits in every mode, keys only in the Sessions view. Proof: review.

## Tasks

- [ ] `lib/view.svelte.ts`: `focus`, derived mode, `f`; `ModeSwitch.svelte` third radio (AC 1)
- [ ] `FocusList.svelte` with `lib/outcome.ts` (AC 2)
- [ ] `App.svelte` key handling by view and mode (AC 3-5)
- [ ] `modes.spec.ts` and Vitest cases (AC 1-5)
- [ ] Update `CHANGELOG.md` and the README (AC 6)

## Dev Notes

- Requirements: FR-7 as amended (Focus, keys), FR-3 as amended (focus mode has no halo), J3. Decisions: AD-55 as amended, AD-66. Epic decisions: D-28 (Epic 2); D-98, D-113, D-114, D-121.
- Dependencies: AC 5 needs story 4.5's file panel.
- Seams (D-98): `Board.approvals` comes from story 2.8; Approve and Reject stay pointer-only on their cards in Board mode (D-28), so Focus only counts them and sends the author there. Triage's `f` (story 3.7, AD-55) lives in the Subjects view; the key handler dispatches by view first, so the two never meet. If 2.8 or 3.7 has not merged, the approvals row or the triage case waits.
- Focus rows are not `.card.needs_you`, so story 4.2's ring never applies to them; AC 2's animation check holds by construction and guards a regression.
- `store.needsYou` stays `columns.needs_you`: approvals are not in it (story 2.8's note), so the digit mapping is the board's.
- The Working count includes the Working column's unknown sessions, as the board's count does.
- Persistence cases (AC 1) start from `openBoard` or from `openScene`, whose init script writes the view state only when the key is absent (story 4.4), so a reload keeps what the keys changed.

## Testing

- Vitest: `app/src/lib/view.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/modes.spec.ts` (with v0.2's approvals fixture and v0.3's subjects fixtures for AC 2 and 4).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: Focus rows checked at 44 px (F26); persistence cases and `openScene`'s init script (F37); dependency on 4.5's file panel stated
