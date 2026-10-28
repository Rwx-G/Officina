# Story 5.9: The composer and the work order card in the window

**Status:** Approved

## Story

As the author delegating a routine subject,
I want to compose a work order from my templates with pickers only, see exactly what will run, and launch it with one pointer click,
so that a dependency bump or a lint sweep starts in one gesture, and nothing I did not choose from a list can slip in (J2, journey 2).

## Acceptance Criteria

1. **The composer** (AD-93, FR-4). `Composer.svelte`, in the Orchestrator view, offers a template picker (approved templates of `OrchestratorView`, with their descriptions), a project picker (the template's projects) and one `<select>` per parameter holding exactly that parameter's list; there is no free-text input for a value anywhere; the subject picker reuses `SubjectPicker` (AD-55) with the card's options and a new-name field. Each choice calls `issue_card` or `edit_card` and shows the returned card or the typed refusal. Proof: Vitest `app/src/components/components.test.ts > Composer offers one select per parameter with its list`, `> Composer has no text input for a value`; Playwright `app/e2e/orchestrator.spec.ts > composing sends issue_card with the chosen template, project and values`, `> changing a value sends edit_card and shows the new card`, `> a refusal is shown on the composer`.
2. **The card** (AD-93, AD-35, FR-4). `WorkOrderCard.svelte` shows the template and its description, the project, each value as a chip, the full rendered prompt as text in a `<pre>`, the argument vector as one monospace chip per argument, the working directory, the subject it will be filed under, and the quota gauges; it never takes the attention colour. Proof: Vitest `components.test.ts > WorkOrderCard shows one chip per argument`, `> WorkOrderCard renders the prompt as text`; Playwright `orchestrator.spec.ts > the card shows the prompt, the argument chips, the folder and the subject`, `> the card and its chips never take the attention colour` (in the light and the dark theme; computed colours of the card, its chips and its border compared, in the real browser, with the value of `--color-attention` resolved on the document root; jsdom resolves no `var()`, D-163, review F22).
3. **One gesture** (AD-78, FR-4, FR-13 as amended by amendment 8, D-28). Launch acts on a pointer click only (`event.detail > 0`) and sends the card's id and prompt SHA-256 through `launch_work_order`; Enter and Space on the focused button do nothing; while a launch is in flight the card shows it and the button is disabled, its label switching between "Launch" and "Launching" through `StableLabel` so the button keeps its box (D-564). Proof: `app/e2e/stable-size.spec.ts`, a `CASES` entry "work order Launch" on an orchestrator scene (a `SCENES` entry fed by story 5.8's `orchestrator-day` fixture); Vitest `components.test.ts > Launch ignores a click with detail 0`, `> Enter and Space on Launch do nothing`; Playwright `orchestrator.spec.ts > Launch sends the card id and the prompt hash`, `> a keyboard press never launches`.
4. **Outcomes** (FR-4, AD-81). The card shows the launch outcome: started (and, once bound, the session); `untrusted_workspace` with the instruction to trust the folder once in `claude`; failed with the exit code and the error as text; a launch refusal with its reason (superseded, catalogue changed, and the others of `LaunchRefusal`, through an exhaustive `Record`). Nothing retries; the author composes again. Proof: Playwright `orchestrator.spec.ts > a started launch shows its session once bound`, `> workspace not trusted is explained, not retried`, `> a failed launch shows its exit code and error as text`; Vitest `components.test.ts > every launch refusal has a wording` (iterating the generated `LaunchRefusal` values; `pnpm check` fails on a missing entry).
5. **Stores** (AD-93). `lib/orchestrator.svelte.ts` holds the cards in flight beside the view; a card whose id the backend no longer knows is dropped from the screen with the refusal shown. Proof: Vitest `app/src/lib/orchestrator.test.ts > a superseded card is replaced by the new one`, `> an unknown card is dropped with its refusal`.
6. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: the work order composer; README "The orchestrator (v0.5, in progress)": composing and launching, what the card shows, pointer-only Launch, and that the session runs as the author in the project folder under its own settings. Proof: review.

## Tasks

- [ ] `Composer.svelte`, `SubjectPicker` reuse (AC 1)
- [ ] `WorkOrderCard.svelte` (AC 2-4)
- [ ] `lib/orchestrator.svelte.ts` cards in flight; `ipc.ts` functions of story 5.7 wired (AC 5)
- [ ] Vitest and Playwright cases; mock answers for issue, edit and launch, including refusals and each outcome (AC 1-5)
- [ ] Update `CHANGELOG.md` and the README (AC 6)

## Dev Notes

- Requirements: FR-4, FR-13's Launch clause (amendment 8), J2, J5, journey 2. Decisions: AD-35 (chips), AD-55 (`SubjectPicker`), AD-78, AD-93. Epic decisions: D-28 (Epic 2); D-124, D-149, D-163.
- Dependencies: stories 5.7 (commands and cards) and 5.8 (view and store).
- v0.4 as built (D-563, D-564): no colour is added (the card reuses the board card's surface and the chips the approval card's), so the dark theme holds by construction and the attention check runs in both themes; `Gauge.svelte` keeps its Board width (D-533's compact width belongs to the one-line header); `SubjectPicker.svelte` is reused as story 3.5 built it.
- The card is core's (AD-78): the frontend renders it and never recomputes a prompt, a hash or a vector. Edits go back to core, which issues a new card.
- Launch is not keyboard-reachable by design (FR-13 as amended); Start in the orchestrator's room is (story 5.18). Do not add `tabindex` tricks to make Launch reachable.
- Quota gauges reuse `Gauge.svelte` with the card's windows.
- The mock answers `launch_work_order` with each outcome by scenario, so the Playwright cases need no real spawn; the real path is covered by the injection battery (5.7).

## Testing

- Vitest: `app/src/components/components.test.ts`, `app/src/lib/orchestrator.test.ts`.
- Playwright: `app/e2e/orchestrator.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the attention-token check moves to a Playwright computed-style case (F22)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: Launch keeps its box through `StableLabel` with a stable-size case, the attention check in both themes, no new colour (D-564)
