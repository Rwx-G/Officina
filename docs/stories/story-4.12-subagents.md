# Story 4.12: Sub-agents beside their lead

**Status:** Review (gate 1 passed, research notes E83; implemented)

## Story

As the author whose sessions spawn sub-agents,
I want each running sub-agent shown as a smaller figure beside its lead's desk, from the moment Claude Code reports it started to the moment it reports it stopped,
so that I see how much is going on in an office without reading transcripts, and never see a sub-agent Officina did not really observe.

## Acceptance Criteria

1. Entry condition (FR-12, D-476, D-479): story 4.1's gate 1 entry ends with **pass**. If it ends with **fail**, this story is not implemented: no code, no field, no registration; its status stays approved with the entry referenced, and story 4.14's report records FR-12 as not shipped by its own rule. Proof: the research-notes entry referenced in Completion Notes.
2. Registration (AD-176, AD-16 as amended): `BUFFERED_EVENTS` in `crates/officina-host/src/doctor.rs` gains `SubagentStart` and `SubagentStop`, so the printed hooks block registers them like the other buffered events (synchronous, `timeout: 5`, no `onFailure`), and the "hooks registered" check fails until the author pastes them; Officina writes no Claude configuration. Proof: `doctor.rs::tests::hooks_block_registers_every_event_and_is_recognised` (iterating `BUFFERED_EVENTS`); CLI `crates/officina/tests/doctor.rs::doctor_asks_for_the_sub_agent_events`; the zero-write fingerprint test `crates/officina/tests/quota_doctor.rs` unchanged and green.
3. The event line (AD-8, AD-176): `officina hook` records `agent_id` and `agent_type` for these two events, each cut to 64 characters on a char boundary; `last_assistant_message`, `agent_transcript_path` and `transcript_path` are never recorded; the hook still exits 0 on every path. Proof: `crates/officina-core/src/event.rs::tests::{subagent_fields_are_kept_and_capped, subagent_texts_and_paths_are_never_kept}`; CLI `crates/officina/tests/hook.rs::a_subagent_start_and_stop_are_appended` (payloads shaped as the documented examples, redacted).
4. Contract canary (AD-12): `core::contract`'s hook fields gain the two events' fields that Officina reads, so a release that drops one shows as missing. Proof: `contract.rs::tests::subagent_fields_are_in_the_contract`.
5. Folding (AD-176, D-476): per session id, a sub-agent runs from its first `SubagentStart` to the next `SubagentStop` with the same `agent_id`; a repeated start is the same sub-agent; a stop without a start is ignored; `SessionEnd` and a `/clear` that supersedes the session end them all; `Card.subagents: Vec<SubagentView { agent_id, agent_type }>` lists the running ones in start order, at most 16; TypeScript bindings regenerated under story 4.2's bump. Proof: core vectors `tests/vectors/scene.json` gain sub-agent cases replayed by `scene_vectors.rs::every_case_places_its_cards` (start then stop; two in parallel; a repeated start; a stop without a start; a session end with one running; a cap at 16; an agent-team teammate with the start and stop shape gate 1 recorded, listed once while it runs, D-507); `typescript_bindings.rs::committed_bindings_match_the_rust_types`.
6. Drawing (FR-12, AD-171, AD-176, D-476, D-477): the layout places at most three sub-agents beside the lead's desk at the prototype v2's offsets, whatever the lead's zone; each is a figure scaled 0.68 without a cap, collar in the project colour, static once placed (no blinking dots); a sub-agent appears with a fade and leaves with one (none under reduced motion); a lead without a desk shows none in the scene. Proof: Vitest `app/src/lib/scene/layout.test.ts > sub-agents stand beside their lead's desk, three at most`, `> sub-agents stay at the desk while the lead waits or rests`, `> a lead without a desk draws no sub-agent`; Playwright `app/e2e/scene.spec.ts > a sub-agent appears beside its lead and leaves when it stops`, `> nothing moves once the sub-agents are placed` (mutation observer after `data-moving="0"`).
7. File and targets (AD-69 as amended): the lead's file lists every running sub-agent with its `agent_type` as text; a sub-agent's target, when it does not overlap (story 4.6's property), opens the lead's file; the lead's `aria-label` counts its sub-agents. Proof: Vitest `components.test.ts > SessionFile lists sub-agents as text`; `text.test.ts > a lead's name counts its sub-agents`.
8. Fixtures (D-101): a scenario `scene-subagents` writes `SubagentStart` and `SubagentStop` lines through `officina hook` and its intent names the expected sub-agents per card. Proof: CI step "e2e fixtures from the real CLI".
9. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` (Added: sub-agents beside their lead; Changed: the hooks block registers `SubagentStart` and `SubagentStop`, to paste again); README: the hooks block and the sub-agent figure. Proof: review; `crates/officina/tests/readme.rs` stays green.

## Tasks

- [x] Check AC 1's entry; stop here on **fail**: pass (E83)
- [x] `doctor.rs` `BUFFERED_EVENTS`; doctor CLI test (AC 2)
- [x] Event line fields and caps; hook CLI test; contract fields (AC 3, 4)
- [x] Core folding, `Card.subagents`, vectors; bindings (AC 5)
- [x] Layout, rig scale, drawing, file and names (AC 6, 7)
- [x] `scene-subagents` in `e2e-fixtures.mjs` (AC 8)
- [x] Update `CHANGELOG.md` and the README (AC 9)

## Dev Notes

- Requirements: FR-12, FR-13. Decisions: AD-8, AD-12, AD-16 as amended, AD-69, AD-171, AD-176. Epic decisions: D-101, D-121, D-476, D-477, D-479, D-497, D-507.
- Verified in the hooks reference on 2026-10-28 (AD-176): `SubagentStart` fires on spawn, on resume and for each message an in-process teammate handles, so starts repeat for one `agent_id`; `SubagentStop` also fires for prompt suggestions and `/btw` without a start; tool events inside a sub-agent carry `agent_id`. Gate 1 settles what the docs do not: whether `session_id` is the lead's.
- Dependencies: story 4.1's gate 1 (entry condition); stories 4.3 (core scene module and vectors), 4.7 (rig) and 4.10 (the mover's fades).
- Two more hook processes per sub-agent are cheap next to the session's own work; `PreToolUse` stays unregistered (D-463).
- `agent_type` is untrusted text: it renders with `{text}` in the file and the names, never in layer 3.
- A lost `SubagentStop` leaves a sub-agent drawn until the session ends (R106); the file shows it, and story 4.14 records it if it happens.

## Testing

- Core unit and vectors: `event.rs`, `contract.rs`, `tests/scene_vectors.rs`.
- CLI integration: `crates/officina/tests/{doctor,hook,quota_doctor}.rs`.
- Vitest: `layout.test.ts`, `text.test.ts`, `components.test.ts`.
- Playwright: `app/e2e/scene.spec.ts`.

## Dev Agent Record

### Debug Log

- Registration, the event line's fields and the board's and registry's skipping of the lines were already in place from story 4.1's gate 1 tooling (D-518); this story adds the doctor CLI test and the contract test, and folds the lines.
- Gate 1's lines (E83) show a resumed background sub-agent as repeated start and stop pairs under one `agent_id`, the next start from 119 ms to minutes after a stop. Folding keeps one helper per `(session, agent_id)` whose rank is its first start (D-547); the window holds a stopped helper 1 s before its fade, so a stop and a start on two boards a quarter of a second apart do not blink (D-548).
- The fold lives in `group_events` beside the session's own state, in the same time-sorted pass, so `SessionEnd` ends the sub-agents at its place in the order; a session that only sub-agent lines name gets no group, as before. `new_card` lists them for live columns only.
- The story expected the prototype's offsets to put every sub-agent's target over its lead's; they do not when the lead is seated, its target following its seat's offset, so the fixtures' sub-agents get targets of their own. They are a third target kind, `helper`, appended after the others, whose activation opens the lead's file, and which never stands for the lead when Escape returns focus (D-549).
- The mover keeps sub-agents apart from walkers (`Standing`): they never route, they fade where they stand. A frame gap over 0.25 s (D-502) drops a held sub-agent at once, as it places walkers.
- `SUBAGENT_SCALE` lives in `layout.ts` rather than `rig.ts`: the e2e specs import it, and their TypeScript project has no DOM iterables for `rig.ts`.

### Completion Notes

- AC 1: **pass**, research notes E83 (story 4.1's gate 1, 2026-10-29).
- AC 2: `doctor.rs::tests::{hooks_block_registers_every_event_and_is_recognised, subagent_events_are_buffered}`; CLI `crates/officina/tests/doctor.rs::doctor_asks_for_the_sub_agent_events` (the block's entries, the hooks check failing until both are pasted, the Claude config dir's fingerprint unchanged); `quota_doctor.rs` unchanged and green.
- AC 3: `events.rs::tests::{subagent_fields_are_kept_and_capped, subagent_texts_and_paths_are_never_kept}`; CLI `hook.rs::a_subagent_start_and_stop_are_appended`.
- AC 4: `contract.rs::tests::subagent_fields_are_in_the_contract` (a payload holding only the contract's fields gives every field the fold reads).
- AC 5: `tests/vectors/scene.json`'s twelve sub-agent cases (start; stop; two in parallel; a repeated start; a stop without a start; a resumed helper's rank; the teammate as pairs or as a start per message; a session end then a prompt; the cap at sixteen; another session's line and a line without `agent_id`; a superseding `/clear`; a stale board) replayed by `scene_vectors.rs::every_case_places_its_cards`; `board.rs::tests::an_ended_card_lists_no_subagent`; `typescript_bindings.rs::committed_bindings_match_the_rust_types` (`SubagentView.ts`, `Card.ts`).
- AC 6: Vitest `layout.test.ts > layoutScene: sub-agents > sub-agents stand beside their lead's desk, three at most`, `> sub-agents stay at the desk while the lead waits or rests`, `> a lead without a desk draws no sub-agent`; `mover.test.ts > mover: sub-agents` (placed at once on the first layout, fade in, hold then fade out, resumed during the hold or the fade, hidden page and reduced motion); `objects.test.ts > a placed figure sits among statics by its depth` (sub-agents included); Playwright `scene.spec.ts > a sub-agent appears beside its lead and leaves when it stops`, `> nothing moves once the sub-agents are placed`, and `the scene draws every live session of each fixture > scene-subagents`.
- AC 7: Vitest `components.test.ts > SessionFile lists sub-agents as text`; `text.test.ts > a lead's name counts its sub-agents`; `layout.test.ts > a sub-agent's target sits at its place and names its lead, where it overlaps no target`; the overlap property `> every live card of every fixture board is in exactly one target`.
- AC 8: `scene-subagents` in `app/scripts/scene-fixtures.mjs` writes its sub-agent lines through `officina hook` and its intents name each card's running sub-agents; CI step "e2e fixtures from the real CLI".
- AC 9: `CHANGELOG.md` `[Unreleased]` (Added: sub-agents in the scene; Changed: the hooks block's sub-agent events, to paste again); README "Wiring the hooks" and "Sub-agents"; `crates/officina/tests/readme.rs` green.
- Not observed on the workstation (E83): an agent-team teammate, a `/btw` question by name, a `claude --bg` session, a nested sub-agent. The teammate's documented shape is a vector case; a lost `SubagentStop` leaves a helper drawn until its lead ends (R106), which story 4.14 records if it happens.

## File List

- `crates/officina-core/src/board.rs`, `contract.rs`
- `crates/officina-core/tests/scene_vectors.rs`, `tests/vectors/scene.json`
- `crates/officina-host/src/focus.rs`
- `crates/officina/tests/doctor.rs`
- `app/src/lib/generated/Card.ts`, `SubagentView.ts`
- `app/src/lib/scene/layout.ts`, `mover.ts`, `objects.ts`, `text.ts`, `testing.ts`
- `app/src/components/SessionFile.svelte`, `scene/Scene.svelte`
- `app/src/lib/scene/layout.test.ts`, `mover.test.ts`, `objects.test.ts`, `text.test.ts`, `app/src/components/components.test.ts`, `app/src/lib/board.test.ts`
- `app/scripts/scene-fixtures.mjs`, `app/e2e/fixtures.ts`, `app/e2e/scene.spec.ts`
- `README.md`, `CHANGELOG.md`

## Change Log

- 2026-10-28 created as story 4.9 with the scene v2 rework (D-460, D-476)
- 2026-10-28 renumbered 4.12 after the plan challenge (D-497, D-498, D-507): the teammate vector case; story numbers updated
- 2026-10-29 gate 1 passed (E83); implemented with D-547 to D-549. Status review
