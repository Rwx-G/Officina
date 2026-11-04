# Story 6.19: The workshop card, the board card and the review panel in the window

**Status:** Review

## Story

As the author who launches, receives and decides workshop runs in the window,
I want the card to show everything the run will and will not hold, the board to show the run where it stands, and a review panel that shows the report, facts, commits, diff and sensitive groups as inert text with pointer-only Approve and Import,
so that what I click is what I saw, no worker text can act in the window, and a stray key never imports a branch.

## Acceptance Criteria

1. **The card** (AD-122, FR-6). `WorkOrderCard.svelte` gains, for a workshop card: the workshop chip; a base `<select>` from the closed list (no free text), each option with its tip and, for an imported branch, its parent run; the uncommitted count with "not carried"; the allow-list; the mounts marked read-only; the image tag and short id; minutes and budget; the readiness causes; a Launch acting on a pointer click only and disabled while any cause holds; Run escape battery offered when the cause is `battery_stale`; for `credential_expired`, the action `WorkshopView` names (D-607): "Login needed" with the login command and a Copy button for a `login` floor, or the `setup-token` step and the floor's `cmdkey` line with Copy for a token floor, each copied text rendered as text; for `proxy_unresolved` and `proxy_unsupported`, the entry named and a pointer to doctor. Changing the base issues a new card (`edit_card` with `Base`). Proof: Vitest `app/src/components/workshop.test.ts > {the base select offers the closed list only, launch is disabled while a cause holds, launch ignores detail 0, a stale battery offers the battery, an expired credential shows its action for each kind, injection keyboard cases never call launch}` (the last replays the `workshop_card` cases tagged `keyboard` from `tests/vectors/injection.json`, story 6.11, review F34); Playwright `app/e2e/workshop.spec.ts > {a workshop card shows its facts, changing the base issues a new card}` (fixtures from story 6.18).
2. **The board card** (AD-122, FR-17). `WorkshopCard.svelte` renders the `workshop` kind: the "isolated" chip while Working (neutral, never `--color-attention`), "review" in Needs you, To resume while stopped, "no change" in Done. Receive on a Needs you or Done workshop card opens the review panel. Proof: Playwright `workshop.spec.ts > {each run state sits in its column, receive opens the review panel}`.
3. **The review panel** (AD-122, FR-10, FR-11, FR-12). `ReviewPanel.svelte` takes AD-84's side slot, 640 px wide: the report as text in a `<pre>`; the facts; commits; `DiffView.svelte` (AD-93); one `SensitiveGroup.svelte` per group with its hunks inline and a pointer-only Approve sending the displayed `hunks_sha256`; a pointer-only Import showing the branch and tip, enabled by the view's readiness; Discard with three reason buttons (keyboard-reachable); a quarantined package shows its cause, the revoke advice for `credential_found`, and Discard only; a `no_change` run shows "Nothing to review: the work branch has no commit past <base> @ <commit>" above its facts and report, and no action. Proof: Vitest `workshop.test.ts > {approve and import ignore detail 0, injection keyboard import cases never call import, discard is keyboard reachable, a quarantined package offers discard only, no change shows nothing to review and no action}`; Playwright `app/e2e/review.spec.ts > {a package is approved then imported, a stale approval shows the refusal, a no change run shows nothing to review}`.
4. **Inert text** (AD-122, FR-17, D-130, D-245). Every run text (report tail, commit subjects, paths, requested hosts, hunks, error text) renders as text, never `{@html}` (the lint rule), and every character of the ranges in `params::FORBIDDEN_FORMAT_RANGES` renders as a visible escape (`<U+202E>`) in the review panel and in `SensitiveGroup.svelte`, `hunks_sha256` staying over the raw bytes, so a Trojan Source hunk cannot reorder what the author approves (review F26). Proof: Vitest `workshop.test.ts > {review texts are inert, every format range renders as an escape}` (the first seeds one markup marker per source of `ReviewView`, iterated from the fixture's untrusted fields; the second iterates `FORBIDDEN_FORMAT_RANGES` and asserts each range's first and last character render escaped; `textContent` holding a raw format character is the failure, so the test can fail); Playwright `review.spec.ts > {markup in a report stays text, a seeded RLO does not reorder the rendered line}` (the rendered line's glyph order compared with the raw text's).
5. **The store** (AD-122, D-249). `lib/workshop.svelte.ts` holds the review panel's state and approvals in flight, through the `ipc.ts` wrappers and `e2e-mock.ts` answers the backend stories own (6.11, 6.13, 6.15, 6.16; review F33). Proof: `pnpm check`; the Playwright cases above against the mock.
6. **Never amber for state** (AD-55, D-163). The workshop chip, the "isolated" chip and the "no change" badge never use the attention colour; only a Needs you card does. Proof: Playwright `workshop.spec.ts > chips never use the attention colour` (computed colours compared with `--color-attention` resolved on the document root, in the light and the dark theme, D-620); any new colour is an `app.css` token with its dark value in both dark blocks and a pair in `src/tokens.test.ts`'s `PAIRS` (`epic-5-decisions.md` D-564); a chip whose text changes keeps its box (`StableLabel` or `data-stable-slot`, a `stable-size.spec.ts` case).
7. **Documentation** (D-226). README screenshots or text for the card and the review panel; `CHANGELOG.md`. Proof: review.
8. **Gate 3, window half** (AD-126 row 3, D-202; owner). This is the first story whose window can issue, launch and review a workshop card, so gate 3's window half closes here (review F5): with a build of this story, Build images and Run escape battery through `crates/officina-host/examples/battery_ci.rs` (the window's buttons are story 6.20's), then one launch from the window on a scratch repository whose git-ignored `.claude` holds no hook, a prompt that edits a file, runs `git status` and commits; record the outcome, the commit in the package seen in the review panel, every host in `gateway_summary` and `gateway_refused`, and no dialog or prompt. Proof: the research notes' gate 3 entry, window half.

## Tasks

- [x] `WorkOrderCard.svelte` workshop facts, base select, Launch (AC 1)
- [x] `WorkshopCard.svelte`, board columns (AC 2)
- [x] `ReviewPanel.svelte`, `SensitiveGroup.svelte`, side slot (AC 3)
- [x] `lib/workshop.svelte.ts` (AC 5)
- [x] Format-range escapes in the panel and `SensitiveGroup.svelte` (AC 4)
- [x] Vitest and Playwright cases (AC 1 to 6); README, `CHANGELOG.md` (AC 7)
- [ ] Gate 3's window half with the owner (AC 8)

## Dev Notes

- Requirements: FR-6 (card), FR-10 (panel), FR-11 (approvals), FR-12 (Import, Discard), FR-17, J2, journey 3. Decisions: AD-55, AD-84, AD-93, AD-122. Epic decisions: D-28 (Epic 2); D-130, D-163 (Epic 5); D-200, D-202, D-225, D-226, D-245, D-249.
- Seams (D-200, read through `main` as built, D-620): `WorkOrderCard.svelte` (5.9), the board columns, `MeetingPanel.svelte`'s side slot and `DiffView.svelte` (5.10), `app.css` and `src/tokens.test.ts`, `app/e2e/stable-size.spec.ts`, `ipc.ts`, `e2e-mock.ts`.
- Waits for: story 6.18 merged (fixtures). Closes on gate 3's window half (AC 8), which blocks no other story's code (D-202).
- Approve, Import and Launch act on a pointer click only (D-28): each starts a container or writes the repository; Discard only reduces and stays keyboard-reachable (AD-122).
- jsdom resolves no `var()` and loads no component CSS, so colour checks are Playwright cases (D-163).

## Testing

- CI, no Docker: Vitest `app/src/components/workshop.test.ts`; Playwright `app/e2e/{workshop,review}.spec.ts` with fixtures from `pnpm e2e:fixtures`; `pnpm check`, lint.
- Workstation, Docker Desktop (manual): gate 3's window half (AC 8).

## Dev Agent Record

### Debug Log

- The frontend cannot import a Rust constant, and retyping the format ranges in TypeScript would be a transcription: they reach `inert.ts` through a vector file a core test rebuilds and pins (D-1237).
- No fixture run collects a package, so the review views are typed literals beside the real `card-bump` card and the real workshop board (D-1236).
- Import first had a transitional label; the mock answers at once, so `stable-size.spec.ts` could not see it flip, and a disabled button says the same: Import has one label (D-1239).

### Completion Notes

- AC 1: Vitest `workshop.test.ts > the workshop card > {the base select offers the closed list only, launch is disabled while a cause holds, launch ignores detail 0, a stale battery offers the battery, an expired credential shows its action for each kind, injection keyboard cases never call launch}` (the last replays the `workshop_card` cases tagged `keyboard` of `tests/vectors/injection.json`); Playwright `workshop.spec.ts > {a workshop card shows its facts, changing the base issues a new card}`.
- AC 2: Playwright `workshop.spec.ts > {each run state sits in its column, receive opens the review panel}` on story 6.18's `workshop-runs-board` fixture; Vitest `workshop.test.ts > the board card > receive is offered on Needs you and Done only, and the isolated chip on Working only`.
- AC 3: Vitest `workshop.test.ts > the review panel > {approve and import ignore detail 0, import stays disabled until every group is approved, injection keyboard import cases never call import, discard is keyboard reachable, a quarantined package offers discard only, no change shows nothing to review and no action}`; Playwright `review.spec.ts > {a package is approved then imported, a stale approval shows the refusal, a no change run shows nothing to review}`; the 640 px width in `workshop.spec.ts > receive opens the review panel`.
- AC 4: Vitest `workshop.test.ts > inert text > {review texts are inert, every format range renders as an escape, the approval hash stays the raw bytes'}`, iterating `UNTRUSTED_SOURCES` and `FORMAT_RANGES` (checked to fail with the escape removed); core `format_ranges_vector.rs::the_file_is_the_constant`; Playwright `review.spec.ts > {markup in a report stays text, a seeded RLO does not reorder the rendered line}`. D-1237.
- AC 5: `lib/workshop.svelte.ts` through the `ipc.ts` wrappers and the `e2e-mock.ts` answers of stories 6.11 to 6.16; `pnpm check`. D-1238.
- AC 6: Playwright `workshop.spec.ts > {chips never use the attention colour, the card's workshop chip never uses the attention colour}` in the light and the dark theme; no new colour token; `stable-size.spec.ts` gains the `review` scene and the `review Approve` case. D-1239.
- AC 7: README "The workshop card and the review panel"; `CHANGELOG.md`.
- AC 8: open, the owner's: gate 3's window half with a build of this story.

## File List

- `app/src/components/WorkOrderCard.svelte`, `app/src/components/WorkshopFacts.svelte` (new), `app/src/components/WorkshopCard.svelte` (new), `app/src/components/ReviewPanel.svelte` (new), `app/src/components/SensitiveGroup.svelte` (new), `app/src/components/BoardColumn.svelte`, `app/src/components/OrchestratorView.svelte`, `app/src/App.svelte`
- `app/src/lib/workshop.svelte.ts` (new), `app/src/lib/workshop-text.ts` (new), `app/src/lib/inert.ts` (new), `app/src/lib/orchestrator.svelte.ts`
- `app/src/components/workshop.test.ts` (new), `app/e2e/workshop.spec.ts` (new), `app/e2e/review.spec.ts` (new), `app/e2e/workshop-fixtures.ts` (new), `app/e2e/stable-size.spec.ts`
- `crates/officina-core/tests/format_ranges_vector.rs` (new), `tests/vectors/format_ranges.json` (new)
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-6-decisions.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: gate 3's window half closes here (F5); format characters rendered as escapes, with tests that can fail (F26); `ipc.ts` wrappers owned by the backend stories (F33); keyboard launch cases replayed from the injection battery (F34)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
- 2026-10-29 the owner's answers: the card shows `credential_expired`'s action per kind with Copy, and the proxy refusals (D-607, D-611)
- 2026-10-29 aligned with v0.4 and v0.5 as built: colour cases in both themes, tokens with `PAIRS`, stable boxes, seams by files (D-620)
- 2026-11-04 implemented but AC 8 (D-1236 to D-1239); status review
