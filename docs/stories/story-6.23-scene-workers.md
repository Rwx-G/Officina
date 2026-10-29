# Story 6.23: Workshop workers in the scene

**Status:** Approved

## Story

As the author who reads the floor at a glance in the scene,
I want a live workshop worker to sit at a desk of its project's office with a neutral glass outline, and a package awaiting my review to queue at the decision office like any other wait,
so that isolated work is visible where my other sessions are, and what waits on me is placed the same way everywhere.

## Acceptance Criteria

1. **Scene state in core** (`architecture-v0.4.md` AD-167 as D-619 extends it; FR-17; `epic-4-decisions.md` D-509). `officina-core::scene` gives `workshop` cards a `SceneState` inside `build_board`, replacing story 6.18's `None`: Working (`launching` to `collecting`) is zone `office`; Needs you (`review`, `quarantined`) is zone `decision`, queued after the approvals, `waiting_since_ms` the time the run entered that state, `alert` `priority` when the project's priority is high and never by waiting time, as an approval-only wait (AD-167 rule 3); To resume and Done get `None`; a stale board (rule 1) gives `unknown` and keeps the last known alert (D-494). No `Board` field is added (P-53); the frontend reads `Card.scene` and never re-implements the rules (AD-13). Proof: cases for each workshop state in `tests/vectors/scene.json` (formatter-exempt) through its coverage test, which still iterates `Zone::ALL` and `AlertCause::ALL`; core `crates/officina-core/src/scene.rs::tests::{a_live_worker_is_in_the_office, a_review_package_queues_in_the_decision_office, a_review_package_never_escalates_by_time, a_high_priority_project_sends_it_to_the_alert_room, stopped_and_done_runs_are_not_drawn}`.
2. **Desks and the queue** (`architecture-v0.4.md` AD-62, D-102, D-104; D-619). A live worker takes the lowest free desk of its project's office under AD-62's rules, counted as a session of its project (the "+n" chip beyond four desks, More offices beyond the slots); a package awaiting review stands in the decision office's queue after the approvals, its desk kept while it is away as AD-62 keeps a waiting session's. Walking between the desk and the queue follows `epic-4-decisions.md` D-531 (walking off: fade where it stands). Proof: Vitest `app/src/lib/scene/layout.test.ts > {a live worker takes its project's lowest free desk, a fifth worker joins the office's chip, a review package queues after the approvals, its desk is kept while it queues}`; AD-70's geometry property over the `scene-eight` scenario with workers added.
3. **Drawing** (AD-122, AD-55, AD-70, D-620). A worker's figure carries a neutral glass outline: a token `--color-glass` in `app/src/app.css` with its light value, its value in both dark blocks (`[data-theme="dark"]` and the media query, which `tokens.test.ts` keeps identical) and a pair in `PAIRS` at the non-text target of 3 against the office floor; never `--color-attention`. Its marks are a session's (the queue's ticket while it waits). Reduced motion and AD-70's truthful-motion test hold unchanged. Proof: Playwright `app/e2e/scene.spec.ts > {a workshop worker has the glass outline, the glass outline never uses the attention colour, the truthful motion case holds with workers}` in both themes (computed colours against `--color-attention` resolved on the root, D-163); `src/tokens.test.ts` (the new pair in `PAIRS`, both dark blocks equal).
4. **Fixtures and baselines** (`epic-4-decisions.md` D-101 as D-461 reads it, D-119 as D-508 amends it, D-181). The scene scenarios gain workshop runs from story 6.18's fixtures (`app/scripts/e2e-fixtures.mjs`, kind `workshop`); the screenshot baselines that change are produced by D-119's `workflow_dispatch` path and committed after the owner sees them, his comparison written in the Completion Notes with its date and verdict (D-508). Proof: Playwright screenshot cases of `scene.spec.ts` with the new baselines, light and dark; the owner's look recorded in the Completion Notes.
5. **Documentation** (D-226). README: the scene's workshop workers (at a desk while they work, in the decision office's queue while their package waits); `docs/assets/scene.png` and `scene-dark.png` regenerated if a worker is in the settled scene they show; `CHANGELOG.md`. Proof: review.

## Tasks

- [ ] `core/scene.rs` rules for `workshop` cards; `scene.json` cases (AC 1)
- [ ] Desk and queue placement in `lib/scene/layout.ts` (AC 2)
- [ ] `--color-glass` with its dark values and `PAIRS` entry; drawing in `lib/scene/objects.ts` (AC 3)
- [ ] Fixtures, baselines through `workflow_dispatch`, the owner's look (AC 4); README, `CHANGELOG.md` (AC 5)

## Dev Notes

- Requirements: FR-17 Slice B (scene) as D-619 reads it after v0.4. Decisions: AD-13, AD-55, AD-62, AD-65, AD-70, AD-122, `architecture-v0.4.md` AD-167, AD-170, AD-171. Epic decisions: D-101, D-102, D-104, D-119, D-181, D-461, D-494, D-508, D-509, D-531 (Epic 4); D-163, D-564 (Epic 5); P-53, D-200, D-226, D-619, D-620.
- Seams (D-200, read through `main` as built, `epic-5-decisions.md` D-563): `crates/officina-core/src/scene.rs` and `tests/vectors/scene.json`; `app/src/lib/scene/layout.ts` (`SceneInput { board }`, `Marks`, targets), `places.ts`, `objects.ts`, `mover.ts`, `memory.ts`; `app/src/app.css` and `src/tokens.test.ts` (`PAIRS`); `app/scripts/e2e-fixtures.mjs` and the scene scenarios; `app/e2e/scene.spec.ts`.
- Waits for: story 6.18; Slice A.
- Why the queue and not a halo at the desk (D-619, `epic-4-decisions.md` D-509): v0.4 places every wait in the decision office, so a package that waits on the author queues there; a halo at the desk would be the one wait drawn elsewhere.
- Why no escalation by time: a package blocks no session and waits for the author's review at his pace, as an approval-only wait (AD-167 rule 3); the project's priority still sends it to the alert room.
- A baseline changes only with a visual change the owner has seen (D-119, D-508).

## Testing

- CI, no Docker: core `crates/officina-core/src/scene.rs` with `scene.json`; Vitest `app/src/lib/scene/layout.test.ts`, `src/tokens.test.ts`; Playwright `app/e2e/scene.spec.ts` (screenshots on the `windows` job, both themes).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-29 reworked against v0.4 as built (`epic-4-decisions.md` D-509): the scene state in core, a live worker at its project's office desk, a waiting package in the decision office's queue, the glass token with its dark values and `PAIRS`, seams by files (D-619, D-620)
