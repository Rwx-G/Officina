# Story 6.20: The Workshop section, the header chip and Stop all, and Slice A's close

**Status:** Approved

## Story

As the author who builds images, plays the battery and stops the workshop from the window,
I want one Workshop section showing images, inventories, batteries, runs, the stop flag and per-template results, a quiet header chip with Stop all while anything runs, and Slice A closed on my own projects' hooks and one real Stop all,
so that the workshop is set up and watched from one place, and Slice A is usable alone with evidence that my guards run inside it.

## Acceptance Criteria

1. **The Workshop section** (AD-122, FR-17). `WorkshopSection.svelte` in the Orchestrator view holds `ImageTable.svelte` (states, labels compared, inventories per project, Build images), `BatteryStatus.svelte` (per image, its last outcome, age and validity, Run escape battery), `RunList.svelte` (live, kept and orphan runs; Stop all; remove), `ResumeDialog.svelte` (each kept run to collect or remove, collect by default; resume the relay, checked by default) and `TemplateTable.svelte` (story 6.16's table, notices, the week's share and environment failures), fed by `get_workshop` and `workshop://updated`. Build images, Run escape battery and Resume act on a pointer click only; Stop all and remove are keyboard-reachable. Proof: Vitest `app/src/components/workshop.test.ts > {build images ignores detail 0, run escape battery ignores detail 0, resume ignores detail 0, stop all is keyboard reachable, resume defaults to collect and to resuming the relay}`; Playwright `app/e2e/workshop.spec.ts > {the section shows each image state, a stale battery is named, the template table shows no change and failed apart}`.
2. **The header** (AD-122, AD-55, FR-17). `Header.svelte` shows a neutral chip "N in the workshop" from `Board.workshop_live`, never `--color-attention`, never in the window title or the overlay, and Stop all while any run is live, keyboard-reachable. Proof: Playwright `workshop.spec.ts > {the chip counts live runs, the chip never uses the attention colour, stop all appears only while a run is live}`; Vitest `components.test.ts > the window title never counts workshop runs`.
3. **Stop all on the workstation** (PRD section 7 Stop, AD-126 manual list; owner). With two runs live on Docker Desktop, Stop all from the header: every worker and gateway stopped within 10 s, the relay stopped, the author's editor sessions untouched; after a window restart the workshop still reads stopped; Resume collects one run and removes the other. Proof: research notes entry E37 (Slice A checks, D-201) with the measured time.
4. **Gate 4, second half** (AD-126 row 4, D-202, D-203; owner). One run from the window per owner project that declares hooks, each with its project image: `workshop_finished.hooks` and the review panel show every configured event's hooks ran; any hook that did not run, or a source the mounts cannot carry that the project relies on, is recorded as a finding against FR-9 before this story closes. Proof: research notes entry E32, second half.
5. **Slice A usable alone** (PRD 6.1.1). On one owner project with its toolchain image: Build images, a fresh passing battery, one launch from a card, a package reviewed with any sensitive group approved, Import creating `officina/<name>`; recorded with the run's id. Proof: research notes entry E37.
6. **Documentation** (D-226). README "The workshop (v0.6, in progress)" covers Slice A end to end: setup (token, floor keys, Build images, battery), launch, review, Import, Stop all and Resume; `CHANGELOG.md`. Proof: review; `crates/officina/tests/readme.rs` stays green.

## Tasks

- [ ] `WorkshopSection.svelte` and its parts, through the `ipc.ts` wrappers and `e2e-mock.ts` answers the backend stories own (6.8, 6.10, 6.17, 6.18; review F33) (AC 1)
- [ ] `Header.svelte` chip and Stop all (AC 2)
- [ ] Vitest and Playwright cases (AC 1, 2)
- [ ] Workstation: Stop all once, gate 4's second half, Slice A end to end, with the owner (AC 3 to 5)
- [ ] README, `CHANGELOG.md` (AC 6)

## Dev Notes

- Requirements: FR-14 (Stop all in the header), FR-15 (Run escape battery from the section), FR-17, FR-9 (gate 4), PRD 6.1.1 (Slice A usable alone), section 7 Stop and Hooks. Decisions: AD-55, AD-122, AD-126. Facts: E32. Epic decisions: D-28 (Epic 2); D-163 (Epic 5); P-52, D-200, D-202, D-203, D-226, D-249.
- Seams (D-200): `Header.svelte` and `OrchestratorView.svelte` (5.8, 5.18), `ipc.ts`, `e2e-mock.ts`, the overlay and the window title (1.12, 3.5).
- Waits for: stories 6.16 to 6.19 merged. Closes on gate 4's second half and E37 (AC 3 to 5), which block no other story's code (D-202). Gate 3's window half closes story 6.19, the first story with a workshop Launch (review F5).
- Section 7's Hooks row is read from gate 4's second half; its "zero run continued past a hook exit of 126 or 127" holds by `hook_failed` (story 6.12) unless E33 removed it, in which case the README says so.
- No employer content in the research notes: "an owner project", placeholder paths.

## Testing

- CI, no Docker: Vitest `app/src/components/{workshop,components}.test.ts`; Playwright `app/e2e/workshop.spec.ts`.
- Workstation, Docker Desktop (manual): Stop all once, gate 4's second half, Slice A end to end.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `ipc.ts` wrappers owned by the backend stories (F33); gate 3's window half placed in 6.19, owner halves block no code (F5)
