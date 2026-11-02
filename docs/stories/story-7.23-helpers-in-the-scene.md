# Story 7.23: Helpers in the scene

**Status:** Approved

## Story

As the author who watches the workshop as a scene,
I want a live critic or research helper to walk in to the orchestrator's desk, stand there while it runs and walk out when it ends, told from a sub-agent by where it stands and what it is called,
so that I see at a glance that the orchestrator's question is being worked on, from the run's real state only (PRD section 9 item 3, AD-184, D-493).

## Acceptance Criteria

Implemented after story 7.22 (role runs exist). It changes the scene only; no launch path.

1. **The place comes from core** (AD-184 as D-1221 reads it on v0.4 and v0.6 as built, `epic-6-decisions.md` D-619). v0.4 replaced AD-68's separate scene inputs with `SceneInput { board }` and core's `Card.scene`, and v0.6 gives a workshop card in Working a desk of its project's office (D-619). A role run's workshop card (`role` set, states `launching` to `collecting`) instead gets core `SceneState` zone `decision` at a helper-run place, is not counted among its project's sessions for desks, and is never waiting; the layout reads it from the board, a pure function as before. The name `helper` is not reused: on `main` it names sub-agents (`layout.ts`'s `helpers`, `helperKey`, `TargetKind` `"helper"`, story 4.12), so the new target kind is `"helper_run"`. Proof: core `crates/officina-core/src/scene.rs::tests::{a_role_run_card_is_placed_at_a_helper_run_place, a_role_run_takes_no_desk_of_its_project}`; Vitest `app/src/lib/scene/layout.test.ts > a helper run is laid out from its card's scene state`.
2. **Places** (AD-184, D-1219, D-1046, D-510). `places.ts` gains two helper-run places in the decision office beside the decider's desk (`DECIDER`), one per role, outside `WAITING_ZONES` and off the orchestrator's staff chain (D-568), then the office's group for any further run; no helper-run place is beside an employee's desk. Proof: Vitest `places.test.ts > helper-run places are beside the decider's desk, outside the waiting zones and off the staff chain`; `geometry.test.ts`'s target properties at full occupancy with both helper runs and the orchestrator present (every target clear of every other, of the plaques and of the decider's screens); `layout.test.ts > no helper-run place is beside an employee's desk`.
3. **The figure** (AD-184, v0.4 AD-171). A rig scaled 0.68, without a cap, in `--orch-tunic`; static once placed (no bubble, no blink); never counted as waiting. Proof: Vitest `rig.test.ts > a helper run's rig is the small capless rig in the orchestrator's tunic`; `objects.test.ts > a helper run draws no mark`.
4. **Walking** (AD-184, v0.4 AD-170, AD-172). It walks in from the entrance along the walk graph when the run appears and out when it ends, like any arrival; with walking off it appears and fades as figures do. Proof: Vitest `mover.test.ts > a helper run walks in from the entrance and out at its end`; Playwright `app/e2e/walk.spec.ts > a helper run walks to the orchestrator's desk and leaves` on fixtures with an explicit clock.
5. **Identity** (D-510, prd-v0.4.md FR-6). Its accessible name and name tag are "critic helper" or "research helper" with the run's name as context; a sub-agent's stay "<type>, sub-agent of <lead>" (`helperWords`); the tunic is a third cue, never the only one. Its target opens the run's card. Proof: Vitest `text.test.ts > a helper run is named by its role, a sub-agent by its type and lead`; Playwright `app/e2e/scene.spec.ts > a helper run's target names its role and opens its card ({light,dark})`.
6. **The eight-projects fit and the stable boxes** (D-564, D-1210). `scene.spec.ts > eight projects fit a maximised Full HD window with the alert band` stays green with both helper runs present; no new colour token (the tunic is v0.4's); a `SCENES` entry in `stable-size.spec.ts` with helper runs. Proof: those cases.
7. **No pixel baseline** (D-1201). GitHub CI is off, so no `workflow_dispatch` baseline run: the story's proofs are the layout, geometry and Playwright cases above; refreshing a baseline is the owner's choice. Proof: review of the File List (no baseline image added).
8. **Vector** (AD-178's `scene.json`). `tests/vectors/scene.json` gains cases with live role run cards (one, two, three, and one beside a worker of the same project at its desk), replayed by core and the frontend. Proof: `crates/officina-core/tests/scene_vectors.rs`; the frontend's replay of the same file.
9. **Documentation** (D-340). README's scene section: the helper figures, how they differ from sub-agents. `CHANGELOG.md` `[Unreleased]` under Added. Proof: review.

## Tasks

- [ ] Core `SceneState` for role run cards; places, rig, mover, text (AC 1 to 5)
- [ ] Geometry, layout and Playwright cases; `scene.json` cases (AC 2, 6, 8)
- [ ] README, `CHANGELOG.md` (AC 9)

## Dev Notes

- Requirements: PRD section 9 item 3 (FR-17's Scene, amended). Decisions: AD-68, AD-170, AD-171, AD-172, AD-178, AD-184. Epic decisions: D-503, D-510, D-1046 (Epic 4); D-564, D-567, D-568 (Epic 5); D-493, D-1201, D-1210, D-1219, D-1221.
- Seams (D-1202): `crates/officina-core/src/scene.rs` (the workshop cards' `SceneState` of story 6.23, D-619), `app/src/lib/scene/{places,layout,rig,mover,objects,text,geometry}.ts` and their tests, `app/src/components/scene/`, `app/e2e/{scene,walk,stable-size}.spec.ts`, `tests/vectors/scene.json`; v0.5's decision office with the orchestrator at the decider's desk (story 5.18).
- Waits for: story 7.22 and v0.6's scene workers (story 6.23).
- Why a new name (D-1221): v0.4 names sub-agents "helpers" in the scene's code; reusing it would mix the two in every type and target, and the owner's prototype tells them apart by place and name.
- No employer content in fixtures.

## Testing

- Vitest: `app/src/lib/scene/{places,layout,rig,mover,objects,text,geometry}.test.ts`.
- Playwright: `app/e2e/{scene,walk,stable-size}.spec.ts`.
- Core: `crates/officina-core/src/scene.rs`, `crates/officina-core/tests/scene_vectors.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-11-02 created from D-493, D-1219 and D-1221 (AD-184 on v0.4's decision office as built)
