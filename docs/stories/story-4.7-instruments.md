# Story 4.7: Instruments: the relay lamp and the quota dials

**Status:** Approved

## Story

As the author glancing at the workshop,
I want the relay's state as a lamp by the door and the two quota windows as dials on the wall, each explained in text on hover and focus,
so that the scene tells me whether the relay waits on me and how much quota is left without my looking up at the header.

## Acceptance Criteria

1. Layout (AD-61 walls, AD-63 tab order, AD-68, D-104): `layoutScene` adds the lamp, only when `input.relay` is not `null`, and the two dials, always, each with its target from `geometry.ts`; tab order after the cabinet: lamp, "5 h" dial, "week" dial; all three in the walls' group. Proof: Vitest `app/src/lib/scene/layout.test.ts > without a relay there is no lamp and no lamp target`, `> the lamp and the dials follow the cabinet in tab order, 5 h first`.
2. Lamp state (FR-4 as amended, D-116): amber when `board.approvals` is not empty or `relay.pending` is set, showing `board.approvals.length` when above 0; else unlit when `relay.stopped_since_ms` is set; else `--color-working`; its name states the state and the count. Proof: Vitest `layout.test.ts > the lamp's state follows approvals, a pending change and a stopped relay, in that precedence`; Playwright `app/e2e/instruments.spec.ts > the lamp is unlit when the relay is stopped`, `> blue when running with nothing pending`, `> amber with the approval count`, `> amber without a count for a catalogue change`.
3. Lamp activation (AD-68, D-116, D-28): Board mode when approvals wait, the Relay view otherwise; nothing in the scene approves. Proof: Playwright `instruments.spec.ts > the lamp opens Board mode when approvals wait`, `> the lamp opens the Relay view otherwise`, `> no key approves from the scene`.
4. Dials (FR-4 as amended, D-115, D-183): "5 h" left of "week" as in the header; each shows its value and, in its name tag and `aria-label`, its window, value and reset time worded at `board.generated_at_ms` through `lib/quota.ts`, which `Gauge.svelte` now calls with `store.nowMs`, adding the week's elapsed share that the dial leaves out; a dial is muted without data or when the board's `five_hour_reset_passed` or `seven_day_reset_passed` says its reset passed; no freshness line. Proof: Vitest `app/src/lib/quota.test.ts > the dial's wording is the gauge's at the same time, without the week's share`, `> the gauge's week wording carries the elapsed share`; `app/src/components/components.test.ts > Gauge shows the explanation of lib/quota`; Playwright `instruments.spec.ts > each dial names its window, value and reset on hover and focus`, `> a board with the reset passed mutes the dial` (review F11); `app/e2e/board.spec.ts > quota gauges show value, reset time, week share and freshness` unchanged.
5. Drawing (AD-61, AD-72, D-119): `components/scene/Instruments.svelte` draws the lamp in its three states and the dials' arcs in layer 1, from tokens, as the mock. Proof: Playwright `instruments.spec.ts > the instruments match their baseline` (`scene-c` with a relay view pushed, one approval pending, clock installed, animations disabled, `maxDiffPixelRatio: SCREENSHOT_MAX_DIFF_RATIO`; the baseline produced by the CI `windows` job's snapshot run and compared once by the owner with `scene-mock.png`) (review F3).
6. Truthful motion with the relay (AD-70, D-98): `e2e-mock.ts` gains `window.__officinaPushRelay(view)`, mirroring `__officinaPushBoard` (store and emit `relay://updated`), and `e2e-globals.d.ts` its type, unless story 2.7 added them; `app/e2e/fixtures.ts` gains `loadRelay(name)` through the fixture index (D-181); pushing the same relay view again changes nothing in the scene, a changed one does. Proof: Playwright `instruments.spec.ts > pushing the same relay view changes nothing in the scene` (review F12).
7. Size and topmost with instruments (AD-70, D-104): story 4.5's size case, with its scroll and tolerance, runs again with a relay view pushed, so the lamp and the dials are checked too. Proof: Playwright `app/e2e/scene.spec.ts > every target is 44 px or more and topmost at its centre` (its relay variant).
8. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: the relay lamp and the quota dials); README "The scene": the lamp's states and where it leads, the dials. Proof: review.

## Tasks

- [ ] `layout.ts` and `text.ts`: lamp and dials, their targets and names (AC 1, 2, 4)
- [ ] `lib/quota.ts` from `Gauge.svelte`; the gauge calls it (AC 4)
- [ ] `Instruments.svelte` in layer 1; activation in `Scene.svelte` through `lib/view.svelte.ts` (AC 3, 5)
- [ ] `__officinaPushRelay` in `e2e-mock.ts` and `e2e-globals.d.ts` if absent; `loadRelay` in `app/e2e/fixtures.ts` (AC 6)
- [ ] `app/e2e/instruments.spec.ts`; the relay variant of the size case; Vitest cases; the instruments baseline from CI (AC 1-7)
- [ ] Update `CHANGELOG.md` and the README (AC 8)

## Dev Notes

- Requirements: FR-4 as amended. Decisions: AD-61 (walls), AD-63, AD-64, AD-68, AD-70, AD-72. Epic decisions: D-28 (Epic 2); P-20, D-98, D-104, D-115, D-116, D-119, D-121, D-181, D-183.
- Seams (D-98): `RelayView` (`stopped_since_ms`, `pending`) comes from story 2.6 and reaches the window through `lib/relay.svelte.ts` and `relay://updated` (2.7); `Board.approvals` from 2.8. Story 2.7's mock answers `get_relay` but defines no push function, so this story owns `__officinaPushRelay` when it is absent. If 2.6 to 2.8 have not merged, this story waits: it is Slice B, and Slice A never reads the relay.
- D-116 departs from AD-68: the Relay view of story 2.9 lists no approvals, which are approved on their cards at the head of Needs you (AD-35, story 2.8 AC 12).
- D-115 and D-183: the scene never reads the clock (AD-64), so the dial's wording takes the board's `generated_at_ms` and its muted state comes from the board's reset flags (story 4.1), which change the board's bytes at the first recompute after a reset; a reset therefore reaches the dial as a payload change. The week's elapsed share is relative to the clock and stays in the header (P-20, against FR-4's "mirroring"). The header keeps freshness and the stale state.
- The lamp's amber marks what waits on the author (frontend rule); blue is `--color-working`, unlit the scene's quiet screen colour.
- The geometry property of story 4.3 already includes the lamp and dial targets; this story only starts drawing and naming them.

## Testing

- Vitest: `app/src/lib/scene/layout.test.ts`, `app/src/lib/quota.test.ts`, `app/src/components/components.test.ts`.
- Playwright: `app/e2e/{instruments,scene,board}.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: baseline from the CI snapshot run with an explicit diff ratio (F3, D-119); dials muted by the board's reset flags and worded without the week's share (F11, D-183); this story owns `__officinaPushRelay` and `loadRelay` (F12)
