# Story 4.19: Session panel polish

**Status:** Review (code done; the owner's look at the panel in the installed build is open)

## Story

As the author who opens a session's file from the scene,
I want the panel to sit cleanly under the header and its exchange to read like a conversation,
so that the file looks deliberate on both my screens and I can tell at a glance what I asked and what Claude answered.

The owner's remarks of 2026-10-29, using the installed v0.4 at work:

1. "In the scene, selecting an agent opens a side bar. The side bar's integration with the top bar is not great: it is cut off abruptly, not cleanly, between the two." The panel touched the header's padding, ran to the sessions area's edges, its shadow cut flat at the top by the area, and a strip of page showed at its right, the scrollbar's room.
2. "In the recap of the human/agent exchanges, the formatting is not great: there should be a styled text area with the agent's text on one side and the human's on the other, like SMS/messaging apps." The last prompt and the last answer were two grey paragraphs, prefixed "> " and "... ".

## Acceptance Criteria

1. The panel's edge (D-702): the session file stands in the sessions area as a surface, a gap under the header, never over it; its right edge on the header's last control; as far below the header's content as it stands from the area's right edge; four bordered edges rounded with `--radius-surface`; its shadow held whole by the insets, at 1920 x 1032 (over the scene) and 3440 x 1392 (docked), in both themes. Proof: Playwright `session-file.spec.ts > the session file stands under the header with whole edges, <placement>, <theme>` (four cases, the shadow's reach parsed from the computed style); `scene.spec.ts > clicking a figure opens its file with Bring to front focused` (its right edge at the inset).
2. Docking holds (D-703, D-543): the scene docks beside the panel when the area leaves room for the panel's whole box, its inset included, and the panel lies over it otherwise. Proof: `session-file.spec.ts` (the placement asserted in each AC 1 case); `wide.spec.ts > the session file docks beside the scene when there is room at <screen>`, unchanged.
3. No strip of the register above the panel (D-704): over the scene, the open file covers the register whole (D-555); docked, the register stays in view. Proof: `session-file.spec.ts` (each AC 1 case); `readability.spec.ts`, unchanged, at the register's new top.
4. The exchange as a conversation (D-705): a list labelled "Last exchange"; the author's last prompt in a bubble on the right, labelled "Your last prompt"; Claude's last answer on the left, labelled "Claude's last answer", its head and tail in one bubble; two bubble tokens, each with a dark value and its contrast pairs (text 7:1, the label 4.5:1) in both themes. Proof: Playwright `session-file.spec.ts > the exchange puts each message on its author's side, labelled by its author, <theme>`; Vitest `tokens.test.ts` (the four pairs).
5. Untrusted text stays text (D-706): the prompt, the answer's head and tail render exactly as the card holds them, line breaks kept, nothing parsed: no element but the list's items, labels and paragraphs, no link, no image, no script run, from a fixture whose last round went through the real hook with HTML-like and markdown-like text. Proof: `session-file.spec.ts > the exchange shows markup and markdown as written, never rendered`; Vitest `components.test.ts > SessionFile renders session text as text`, unchanged.
6. Missing data is unknown (D-708): a side the card does not carry shows nothing, and a card with neither shows no exchange; no time is shown, since the card carries none per message. Proof: `session-file.spec.ts > a side the card does not carry shows nothing`.
7. Nothing else moves (D-707, D-709): Bring to front keeps the focus and stays in view on opening, the exchange coming after the actions; the slide-in is D-545's; story 4.15's one header, the radius tokens, the dark theme, D-516 and D-517 and the minimum window are unchanged. Proof: the gate's Playwright `--repeat-each=3` run (`scene.spec.ts`, `motion.spec.ts`, `stable-size.spec.ts`, `header.spec.ts`, `themes.spec.ts`, `min-size.spec.ts`, `wide.spec.ts`), Vitest `tokens.test.ts`.
8. Documentation: this story, D-702 to D-709, `CHANGELOG.md` `[Unreleased]`, the README's "Acting from the scene", `sprint-status.yaml`, the Epic 4 report; before and after screenshots at 1920 x 1032 and 3440 x 1392 in both themes for the owner. Proof: review.

## Tasks

- [x] The panel's insets, border, radius and shadow in `SessionFile.svelte` and `app.css` (AC 1)
- [x] The docking room in `Scene.svelte`; the register's top in `geometry.ts` (AC 2, 3)
- [x] The exchange list, its bubbles and tokens (AC 4, 5, 6)
- [x] The markup round on scene-c's `link-fixes`, through the real hook (AC 5)
- [x] The gate (AC 7)
- [x] Documentation and screenshots (AC 8)

## Dev Notes

- Requirements: FR-5 (the file), FR-8 (the dark theme). Decisions: D-111, D-182, D-516, D-517, D-539, D-540, D-543, D-545, D-546, D-555; D-702 to D-709.
- The session file is outside the scene element (D-111): none of this story writes inside it on opening, and the register's lower top is a constant, not a reaction to the file.
- The card carries the last prompt and the last answer's head and tail (`Card.last_prompt`, `last_message`, `last_message_tail`), never their times: `SessionEvents` folds `last_prompt_at_ms` and `last_stop_at_ms` in core but `Card` does not expose them (D-708).
- The hook already marks a cut: the head ends and the tail starts with an ellipsis (`events.rs` `excerpt`, `tail_excerpt`), so the file's own "... " prefix is gone.

## Testing

- Playwright: `session-file.spec.ts` (new: four edge cases, two side cases, the literal text case, the absent sides case); `scene.spec.ts` (one case's right edge).
- Vitest: `tokens.test.ts` (four pairs).

## Dev Agent Record

### Debug Log

- The before screenshots first came out with the panel faded: under Playwright's installed clock, Svelte starts the slide-in on the page's next frame, so `transitionsDone` found nothing running yet. The new spec runs the clock a second after opening, then waits for the transitions.
- The first gap assertion measured from the controls' bottom and was 5.7 px off: the brand's second line is the header's lowest content. The gap is measured from the header's content box, which the token comment states.
- Over the scene, the panel's 12 px gap first showed the top 4 px of the register, which started 8 px down the scene: the register now starts at 12 px (D-704).

### Completion Notes

- AC 1 to 7: the cases above, green in the gate (the Epic 4 report's figures).
- AC 8: screenshots under the session's scratchpad `shots-4.19/`, `before-*` from `b86051d`, `after-*` from this story's code, for the owner.

## File List

- `app/src/components/SessionFile.svelte`, `app/src/components/scene/Scene.svelte`, `app/src/lib/scene/geometry.ts`, `app/src/app.css`
- `app/scripts/scene-fixtures.mjs`
- `app/e2e/session-file.spec.ts` (new), `app/e2e/scene.spec.ts`, `app/src/tokens.test.ts`
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-4-decisions.md`, `docs/stories/sprint-status.yaml`, `docs/qa/epic-4-report.md`

## Change Log

- 2026-10-29 created from the owner's two remarks on the installed v0.4; implemented (D-702 to D-709)
