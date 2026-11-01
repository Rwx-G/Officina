# Story 4.20: The owner's second UI feedback

**Status:** Review (code done; the owner's look at the installed build is open)

## Story

As the author who uses the installed v0.4 every day,
I want the board's notes, the header, Focus, the scene and the session file to read cleanly,
so that the window says what waits for me without a line break, a duplicate name or a raw mark in the way.

The owner's feedback of 2026-11-01, using the installed build:

1. "The undo / discard bar: careful, there is a line break; its width needs revising."
2. "x to sort / x to resume / x done: maybe merge them into a single indicator that opens on click or hover, to reduce the top bar's size."
3. "Anthropic just removed the 5-hour quota, so we can remove ours too."
4. "Focus page: to be reworked when 'Nothing waits for you' (it's just badly placed text, maybe some styling to make it nice)."
5. "In the scene we see the characters with a name/number; on hover we see another name (for example 'Officina 1' and 'Officina-04' representing the same agent). The handling of these two names should be improved."
6. "Agent/human exchange: render the markdown so the messages look nicer."
7. "Board: the waiting spot should not be drawn in 'Decisions' and 'Alert', and a bit more randomised?" (the dashed circles on the decision office's and the alert room's floors).

## Acceptance Criteria

1. One line, one width (D-1040): the settle note and the discard target keep one line and one width from the minimum window (1140 px) up; a long session name is cut with an ellipsis and whole in the note's tooltip; a short note keeps the same box. Proof: Playwright `settle.spec.ts > the settle note and the discard target keep one line and one width at every window width`.
2. One indicator (D-1041, the owner's revision of D-541): To sort, To resume and Done are one fixed-size header control showing the three counts behind icons; its panel lists each count with its action (Triage panel, Board mode on the column); it opens on hover, on keyboard focus and on a click, which keeps it open; Escape closes it and gives the focus back; it keeps its box whatever the counts ("99+" past 99) and its panel moves nothing; it is never amber. Proof: Playwright `header.spec.ts > the sessions indicator` (three cases), `scene.spec.ts > the header counts To resume and Done and opens Board mode on the column`, `triage.spec.ts > the indicator's to-sort row opens the panel`, `stable-size.spec.ts > sessions indicator keeps its box and moves nothing after it`, `header.spec.ts > the header is the same in every view and mode at <screen>`; Vitest `components.test.ts > WorkCounts`.
3. Gauges follow the data (D-1042): a gauge shows only for a window the quota data carries; the weekly gauge always shows and says "no data" without a file; the five-hour gauge, after it, comes and goes moving no other control; an older file with both windows shows both; doctor's quota line names the windows present. Claude Code still documents and, per the owner's screenshot, still sends `five_hour`, so no gauge is deleted. Proof: Rust `quota.rs > a_window_the_statusline_leaves_out_is_absent_from_the_file`, `quota_doctor.rs > doctor_passes_once_the_block_is_merged`; Playwright `board.spec.ts > a quota file without the five-hour window shows the week only and moves no control` (fixture `quota-week-only` through the real `officina quota write`), `> an empty workstation shows empty columns and no quota data`; Vitest `components.test.ts > QuotaGauges`.
4. Focus's empty state (D-1043): centred, a static inline drawing in the theme's tokens, the headline "Nothing waits for you" and a line naming what Focus will show and the key that leaves it, in both themes. Proof: Playwright `modes.spec.ts > Focus with nothing waiting shows a centred empty state, <theme>`, `> Focus's empty state names the scene when Focus hides the scene`, `> Focus with something waiting shows no empty state`.
5. One name (D-1044): a session carries its own name on every surface; its project is context set apart (the name tag, Focus) or behind its dot; a plaque's count reads as a head count. Proof: Playwright `scene.spec.ts > a session carries one name on every surface, its project only as context`; Vitest `text.test.ts > a figure's tag is the session's own name, its project the context unless the name says it`, `scene.test.ts > the name tag is in the button before any hover and ignores the pointer`.
6. Safe markdown (D-1045, the owner's revision of D-706): the exchange renders paragraphs, line breaks, emphasis, strong, inline and fenced code, headings as bold lines, lists, blockquotes and GFM tables (scrolling inside the bubble), from Officina's own parser, every string a text node; HTML shows as written, no anchor, no image, no `on*`, `href` or `src`; the 280-character limits and the missing-side rule hold. Proof: Vitest `markdown.test.ts` (unit, hostile inputs, seeded fuzz, timing bound); Playwright `session-file.spec.ts > the exchange renders its markdown, markup shown as written, no link and no image`, `> a wide table scrolls inside its bubble`, `> a side the card does not carry shows nothing`; `components.test.ts > SessionFile renders session text as text`, unchanged.
7. Waiting places (D-1046): no place is drawn on the decision office's or the alert room's floor; each waiting figure stands off its spot by an offset from its key, the same on every board; no two targets meet, nothing fixed is met, no plaque covers a head, at every offset. Proof: Vitest `furniture.test.ts > no waiting place is drawn on the floor`, `geometry.test.ts` (every property at every offset), `places.test.ts > waiting places`, `layout.test.ts > waiting figures stand off their spots without meeting, each where it stood before`, `mover.test.ts` (the walk ends at the place); Playwright `scene.spec.ts > the waiting places are not drawn and each waiting figure keeps its own place`, `readability.spec.ts`, unchanged.
8. Nothing else moves: D-516, D-517, D-539, D-540, D-545, the dark theme, FR-9, the eight-projects fit at 1920 x 1032 with the alert band and the minimum window are unchanged. Proof: the gate's Playwright `--repeat-each=3` run (`stable-size.spec.ts`, `header.spec.ts`, `themes.spec.ts`, `min-size.spec.ts`, `wide.spec.ts`, `motion.spec.ts`, `readability.spec.ts`), Vitest `tokens.test.ts`.
9. Documentation: this story, D-1040 to D-1046, the quota contract's absent-window sentence in `architecture-spine.md`, `CHANGELOG.md` `[Unreleased]`, the README, `sprint-status.yaml`, the Epic 4 report; before and after screenshots at 1920 x 1032 in both themes for the owner. Proof: review.

## Tasks

- [x] The settle note and the discard target (AC 1)
- [x] `WorkCounts.svelte` in the header, `EndedCounts.svelte` and the to-sort chip removed (AC 2)
- [x] `QuotaGauges.svelte` for the header and the work order card; `QuotaFile::window_names` and doctor's quota line; the `quota-week-only` fixture (AC 3)
- [x] Focus's empty state (AC 4)
- [x] `naming.ts`, the tag's context, Focus's order, the plaque's head count (AC 5)
- [x] `markdown.ts` and `Markdown.svelte` in the session file (AC 6)
- [x] The spot marks removed; `waitingOffsets` and `restPointFor` in `places.ts`, used by the layout and the mover (AC 7)
- [x] The gate (AC 8)
- [x] Documentation and screenshots (AC 9)

## Dev Notes

- Requirements: FR-5 (the file), FR-7 (Focus), FR-8 (the dark theme), FR-9 (no continuous animation). Decisions: D-182, D-185, D-471, D-503, D-516, D-517, D-536, D-539, D-540, D-541, D-542, D-545, D-554, D-558, D-559, D-706, D-708; D-1040 to D-1046.
- Item 3 is not a removal. Claude Code's statusline documentation (https://code.claude.com/docs/en/statusline) still lists `rate_limits.five_hour`, each window "may be independently absent", and a window is dropped once its reset passes; the owner's screenshot sent with the feedback shows the five-hour gauge at 4 % with a reset time. The quota file already omitted an absent window (never `null`), so the change is the header's: it shows what the data carries.
- The waiting offsets are computed once, at the first call, from every place of the largest floor (rooms never move when columns are added): a fixed grid of screen offsets per spot, filtered against the fixed things and every other spot, then pairwise between moving spots, the farther candidate dropped. The spot remains the walk graph's node.
- The markdown parser reads text into a tree of plain strings; the component renders the tree with `{text}` only. A link's address is shown in brackets after its text, never as an attribute.

## Testing

- Rust: `quota.rs` (one new case), `quota_doctor.rs` (the doctor case extended).
- Vitest: `markdown.test.ts` (new), `components.test.ts` (WorkCounts, QuotaGauges), `text.test.ts`, `scene.test.ts`, `geometry.test.ts`, `places.test.ts`, `layout.test.ts`, `mover.test.ts`, `objects.test.ts`, `furniture.test.ts`.
- Playwright: `settle.spec.ts`, `header.spec.ts`, `board.spec.ts`, `modes.spec.ts`, `scene.spec.ts`, `session-file.spec.ts`, `triage.spec.ts`, `stable-size.spec.ts`.

## Dev Agent Record

### Debug Log

- The first waiting-offset filter dropped a candidate whenever it met any candidate of another spot, so neighbours killed each other's room and two queue places could not vary. The filter now checks each candidate against the fixed things and the other spots first, then drops only the farther of two meeting candidates.
- `readability.spec.ts > no plaque covers a waiting figure's head` failed in `scene-alert` with a figure moved up under the alert room's plaque: the targets cleared the plaque, the head above the target did not. The head and cap (12 px above the target, 15 px to each side) are now kept off the fixed things too.
- A figure resting off its node and then leaving stepped back onto the node first, which turned it around for a frame (`mover.test.ts > facing changes only past the velocity thresholds`): the route now leaves from where the figure stands.
- The exchange's message count moved from `getByRole("listitem")` to the list's own items: a message's markdown lists are list items too.
- `lint.test.ts`, which runs ESLint on every scene probe, timed out once at 5 s while the whole Vitest suite ran beside other builds; it passed alone and in the next full run.

### Completion Notes

- AC 1 to 8: the cases above, green in the gate on `397cacd`: 2,036 Rust tests passed on Windows and 1,871 in `rust:1-bookworm`, Vitest 381, Playwright `--repeat-each=3` 1,422 passed, 6 skipped; the loaded machine's reruns are in the Epic 4 report.
- AC 9: screenshots under the session's scratchpad `shots-4.20/`, `before-*` from `7981eb4`, `after-*` from this story's code: the header (busy board), Focus's empty state, the scene with the decision office's queue (scene-c) and a session file with a markdown answer (scene-c's `link-fixes`), at 1920 x 1032 in both themes.

## File List

- `app/src/components/SettleNote.svelte`, `DiscardTarget.svelte`, `Header.svelte`, `WorkCounts.svelte` (new), `QuotaGauges.svelte` (new), `WorkOrderCard.svelte`, `FocusList.svelte`, `Markdown.svelte` (new), `SessionFile.svelte`, `scene/Plaque.svelte`, `scene/SceneTargets.svelte`; `EndedCounts.svelte` removed
- `app/src/lib/naming.ts` (new), `markdown.ts` (new), `scene/places.ts`, `scene/layout.ts`, `scene/mover.ts`, `scene/furniture.ts`, `scene/text.ts`; `app/src/App.svelte`, `app/src/app.css`
- `crates/officina-core/src/quota.rs`, `crates/officina-host/src/doctor.rs`, `crates/officina/tests/quota_doctor.rs`
- `app/scripts/e2e-fixtures.mjs`, `app/e2e/fixtures.ts`, the specs and tests above
- `README.md`, `CHANGELOG.md`, `docs/architecture-spine.md`, `docs/stories/epic-4-decisions.md`, `docs/stories/sprint-status.yaml`, `docs/qa/epic-4-report.md`

## Change Log

- 2026-11-01 created from the owner's seven remarks on the installed build; implemented (D-1040 to D-1046)
