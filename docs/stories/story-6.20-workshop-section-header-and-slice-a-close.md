# Story 6.20: The Workshop section, the live count and Stop all, and Slice A's close

**Status:** Approved

## Story

As the author who builds images, plays the battery and stops the workshop from the window,
I want one Workshop section showing images, inventories, batteries, runs, the stop flag and per-template results, a quiet live-run count on the Orchestrator view switch with Stop all in the header while anything runs, and Slice A closed on my own projects' hooks and one real Stop all,
so that the workshop is set up and watched from one place, and Slice A is usable alone with evidence that my guards run inside it.

## Acceptance Criteria

1. **The Workshop section** (AD-122, FR-17). `WorkshopSection.svelte` in the Orchestrator view holds `ImageTable.svelte` (states, labels compared, inventories per project, Build images), `BatteryStatus.svelte` (per image, its last outcome, age and validity, Run escape battery), `RunList.svelte` (live, kept and orphan runs; Stop all; remove), `ResumeDialog.svelte` (each kept run to collect or remove, collect by default; resume the relay, checked by default), `TemplateTable.svelte` (story 6.16's table, notices, the week's share and environment failures) and `CredentialPanel.svelte` (the floor's credential kind and state with its action, D-607: for `login`, Log in for the workshop, which creates the login volume and then shows the login command with a Copy button, and Check login, which runs the probe; for the token kinds, the `setup-token` step and the floor's `cmdkey` line with Copy; and the network: the proxy source and each entry's route, D-611), fed by `get_workshop` and `workshop://updated`. Log in for the workshop and Check login act on a pointer click only (each starts a container or creates a volume); the commands they call are story 6.26's. Build images, Run escape battery and Resume act on a pointer click only; Stop all and remove are keyboard-reachable. Proof: Vitest `app/src/components/workshop.test.ts > {build images ignores detail 0, run escape battery ignores detail 0, resume ignores detail 0, log in and check login ignore detail 0, the login command renders as text with a copy button, each credential state shows its action, stop all is keyboard reachable, resume defaults to collect and to resuming the relay}`; Playwright `app/e2e/workshop.spec.ts > {the section shows each image state, a stale battery is named, the template table shows no change and failed apart}`.
2. **The header** (AD-122 as D-618 reads it after v0.4 and v0.5 as built, AD-55, FR-17, D-620; to confirm). The live-run count from `Board.workshop_live` sits on the Orchestrator view switch, after `epic-5-decisions.md` D-565's presence mark and proposal count, as a neutral number in a fixed slot sized for `max_workers`' upper bound, its accessible name and tooltip "N in the workshop"; Stop all takes a `data-stable-slot` in the header's status slot, present while any run is live and keyboard-reachable, so its appearance moves nothing before it (D-517). Neither uses `--color-attention`, appears in the window title or the overlay; both hold in Board and Scene modes and in both themes. Proof: Playwright `workshop.spec.ts > {the view switch counts live runs, the count never uses the attention colour, stop all appears only while a run is live}` (colour cases in both themes); `app/e2e/stable-size.spec.ts` gains `CASES` for the count and Stop all (the gauges' and buttons' boxes unchanged as each appears); `scene.spec.ts > eight projects fit a maximised Full HD window with the alert band` stays green with both shown; Vitest `components.test.ts > the window title never counts workshop runs`.
3. **Stop all on the workstation** (PRD section 7 Stop, AD-126 manual list; owner). With two runs live on Docker Desktop, Stop all from the header: every worker and gateway stopped within 10 s, the relay stopped, the author's editor sessions untouched; after a window restart the workshop still reads stopped; Resume collects one run and removes the other. Proof: the research notes' Slice A entry (Slice A checks, D-201) with the measured time.
4. **Gate 4, second half** (AD-126 row 4, D-202, D-203; owner). One run from the window per owner project that declares hooks, each with its project image: `workshop_finished.hooks` and the review panel show every configured event's hooks ran; any hook that did not run, or a source the mounts cannot carry that the project relies on, is recorded as a finding against FR-9 before this story closes. Proof: the research notes' gate 4 entry, second half.
5. **Slice A usable alone** (PRD 6.1.1). On one owner project with its toolchain image: Build images, a fresh passing battery, one launch from a card, a package reviewed with any sensitive group approved, Import creating `officina/<name>`; recorded with the run's id. Done once with a `setup-token` floor and, unless the login entry left the login kind unbuilt (D-610), once with a `login` floor through Log in for the workshop. Proof: the research notes' Slice A entry.
6. **The work network entry before the close** (D-613). Slice A closes only once the work network entry is recorded (story 6.1 AC 14) and, if it found an unauthenticated proxy, story 6.25's CONNECT through it is recorded too; an authenticating proxy is recorded as the work floor's finding, with FR-4 reopened with the owner, and does not hold the close for other floors. Proof: the research notes' work network entry and the Slice A entry's pointer to it.
7. **Documentation** (D-226). README "The workshop (v0.6, in progress)" covers Slice A end to end: setup (a Claude subscription account of any origin; `setup-token` recommended for unattended workers, `/login` through Log in for the workshop; the floor keys, `upstream_proxy = "system"` and how to obtain `ca_file`; Build images, battery), launch, review, Import, Stop all and Resume; `CHANGELOG.md`. Proof: review; `crates/officina/tests/readme.rs` stays green.

## Tasks

- [ ] `WorkshopSection.svelte` and its parts, through the `ipc.ts` wrappers and `e2e-mock.ts` answers the backend stories own (6.8, 6.10, 6.17, 6.18, 6.26; review F33), `CredentialPanel.svelte` among them (AC 1)
- [ ] The count on the Orchestrator view switch, Stop all in a stable slot of `Header.svelte`, the `stable-size.spec.ts` cases (AC 2)
- [ ] Vitest and Playwright cases (AC 1, 2)
- [ ] Workstation: Stop all once, gate 4's second half, Slice A end to end with each credential kind built, with the owner (AC 3 to 5); the work network entry checked recorded (AC 6)
- [ ] README, `CHANGELOG.md` (AC 7)

## Dev Notes

- Requirements: FR-14 (Stop all in the header), FR-15 (Run escape battery from the section), FR-17, FR-9 (gate 4), PRD 6.1.1 (Slice A usable alone), section 7 Stop and Hooks. Decisions: AD-55, AD-122, AD-126. Facts: the gate 4 entry. Epic decisions: D-28 (Epic 2); D-163 (Epic 5); P-52, D-200, D-202, D-203, D-226, D-249, D-605, D-607, D-610, D-611, D-613.
- Seams (D-200, read through `main` as built, D-620): `Header.svelte` (one line in Scene mode, its status slot after the fixed controls, `epic-4-decisions.md` D-517, D-526, D-533), the view switch with `epic-5-decisions.md` D-565's Orchestrator button, `OrchestratorView.svelte`, `app.css` and `src/tokens.test.ts` (`PAIRS`) for any new colour, `app/e2e/stable-size.spec.ts` (`CASES`), `ipc.ts`, `e2e-mock.ts`, the overlay and the window title.
- Why the count is not a header chip (D-618, to confirm): the status slot is 230 px and eight projects with the alert band fit the owner's screen by a few pixels (D-526, D-520); D-565 moved the orchestrator's two chips to the view switch for the same reason.
- Waits for: stories 6.16 to 6.19 and 6.26 merged. Closes on gate 4's second half, the Slice A entry and the work network entry (AC 3 to 6), which block no other story's code (D-202). Gate 3's window half closes story 6.19, the first story with a workshop Launch (review F5).
- Section 7's Hooks row is read from gate 4's second half; its "zero run continued past a hook exit of 126 or 127" holds by `hook_failed` (story 6.12) unless the gate 5 entry removed it, in which case the README says so.
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
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
- 2026-10-29 the owner's answers: `CredentialPanel.svelte` (Log in for the workshop, Check login, the token steps, the routes); Slice A run once per credential kind built; the work network entry recorded before the close (D-605, D-607, D-610, D-611, D-613)
- 2026-10-29 aligned with v0.4 and v0.5 as built: the live count on the Orchestrator view switch and Stop all in a stable slot, with `stable-size.spec.ts` cases and the eight-projects fit; seams by files (D-618, D-620)
