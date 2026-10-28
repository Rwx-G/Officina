# Story 6.23: Workshop workers in the scene

**Status:** Approved

## Story

As the author who reads the floor at a glance in the scene,
I want a live workshop worker to sit at a desk of its project's workshop with a neutral glass outline, and a package awaiting my review to give it the waiting halo,
so that isolated work is visible where my other sessions are, and what waits on me is marked the same way everywhere.

## Acceptance Criteria

1. **Scene input** (AD-122, AD-68, FR-17). `SceneInput.workshop` is nullable and built from the board's `workshop` cards (story 6.18) by the existing scene input builder; no `Board` field changes (P-53). A live run (Working) takes a desk in its project's workshop after the project's own sessions; a run whose package awaits a verdict (Needs you) keeps its desk with the waiting halo (AD-65); To resume and Done runs take no desk. Proof: Vitest `app/src/lib/scene/layout.test.ts > {a live worker sits after the project's sessions, a review worker keeps its desk with the halo, stopped and done runs take no desk, a null workshop input changes nothing}`.
2. **Drawing** (AD-122, AD-55, AD-70). A worker's figure carries a neutral glass outline (a token in `app/src/app.css`, never `--color-attention`); the halo is the existing waiting halo, so reduced motion and AD-70's truthful-motion test hold unchanged. Proof: Playwright `app/e2e/scene.spec.ts > {a workshop worker has the glass outline, the glass outline never uses the attention colour, the truthful motion case holds with workers}` (computed colours against `--color-attention` resolved on the root, D-163); `tokens.test.ts` asserts the new token exists in both themes.
3. **Fixtures and baselines** (D-101, D-119, D-181). The scene scenarios gain workshop runs from story 6.18's fixtures; the screenshot baselines that change are produced by D-119's `workflow_dispatch` path and committed after the owner sees them. Proof: Playwright screenshot cases of `scene.spec.ts` with the new baselines; the owner's look recorded in the Completion Notes.
4. **Documentation** (D-226). README: the scene's workshop workers; `CHANGELOG.md`. Proof: review.

## Tasks

- [ ] Scene input builder, layout placement (AC 1)
- [ ] Glass outline token and drawing (AC 2)
- [ ] Fixtures, baselines through `workflow_dispatch`, the owner's look (AC 3); README, `CHANGELOG.md` (AC 4)

## Dev Notes

- Requirements: FR-17 Slice B (scene). Decisions: AD-55, AD-65, AD-68, AD-70, AD-122. Epic decisions: D-101, D-119, D-181 (Epic 4); D-163 (Epic 5); P-53, D-200, D-226.
- Seams (D-200): `lib/scene/*` (4.3, 5.18), `app/src/app.css` tokens (4.8), the scene fixtures (4.1).
- Waits for: story 6.18; Slice A.
- Desks are a layout fact; Epic 4's and Epic 5's occupancy rules (D-102, D-104, D-146) hold, with workers counted as sessions of their project.
- A baseline changes only with a visual change the owner has seen (D-119).

## Testing

- CI, no Docker: Vitest `app/src/lib/scene/layout.test.ts`, `tokens.test.ts`; Playwright `app/e2e/scene.spec.ts` (screenshots on the `windows` job).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
