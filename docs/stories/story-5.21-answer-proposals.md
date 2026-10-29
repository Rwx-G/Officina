# Story 5.21: Answer proposals: the orchestrator drafts, the author sends with a click

**Status:** Review

## Story

As the author who asked the orchestrator to keep idle background sessions moving,
I want it to propose a written answer to a session waiting on its turn, shown beside my own composer as untrusted text with its rationale, and sent only when I click "Send the proposed answer",
so that routine nudges cost me one look and one click, while no model text ever becomes a session's next prompt without me (PRD section 9 item 3, principle 8).

## Acceptance Criteria

1. **The tool** (AD-182, AD-90, FR-11 as amended by amendment 7, D-575). `McpTool` gains `ProposeAnswer`: `propose_answer {session_id, text, rationale}`, with its generated schema (golden); input at most 8 KiB, `text` at most 4,000 characters and `rationale` 500; the rate limits are the other proposal tools' (30 per hour, 200 pending, shared); it writes one file of kind `answer` through story 5.15's writer and returns its id, validating nothing else. Proof: the `ProposeAnswer` cases of `tests/vectors/mcp.json` through `crates/officina-core/tests/mcp_vectors.rs::every_tool_and_error_has_a_case`; core `crates/officina-core/src/mcp.rs::tests::schemas_are_pinned`; CLI `crates/officina/tests/mcp.rs::{propose_answer_writes_one_file_and_returns_its_id, propose_answer_shares_the_rate_limits}`.
2. **Validation at ingestion** (AD-182, AD-91, FR-6, D-139, D-168). `orders::proposal::validate` accepts an `answer` file only when: the session exists in the board or the registry, is a background session, live, in Your turn, and `meeting::delivery` (with the `claude` version the collector last read) gives `ResumeLive` at ingestion; `text` passes `meeting::Answer::parse`, the composer's function, unchanged (story 5.11); `rationale` is 1 to 500 characters without control or format character. A failure stores the proposal `invalid` with one of two new `ProposalInvalidReason` variants, unit variants like the others (`session_not_answerable`, `answer_refused`), its texts kept as untrusted; which answer rule refused it is not stored, since the panel shows the text itself. Proof: the `proposal` section of `tests/vectors/orders.json` gains answer cases through `crates/officina-core/tests/orders_vectors.rs::{proposal_cases_match, every_invalid_reason_has_a_case}`; core `crates/officina-core/src/orders/proposal.rs::tests::an_answer_proposal_uses_the_composers_rules` (every `AnswerRefusal::ALL` variant reached through a proposal).
3. **Statuses** (AD-182, AD-91). A valid answer proposal is `pending`; it turns `expired` as soon as the session leaves Your turn or its delivery stops being `ResumeLive` (checked on each board the collector builds), `superseded` when a newer valid answer proposal names the same session (one proposal per session shows, as recaps do), `accepted` once sent, `dismissed` by the author. Proof: core `proposal.rs::tests::{an_answer_expires_when_the_session_leaves_your_turn, an_answer_expires_when_its_delivery_changes, a_newer_answer_supersedes_the_pending_one}`; CLI-crate collector test `crates/officina/tests/watch.rs::an_unsent_answer_expires_on_the_next_board`.
4. **The panel** (AD-182, AD-93, FR-6 as amended by section 9 item 3, D-488, D-28, D-564). `MeetingPanel.svelte` shows the session's pending answer proposal under a "proposal" label: its text and its rationale as text (`{text}`), beside the author's `AnswerComposer`, which stays empty and separate (nothing is ever copied into it); a "Send the proposed answer" button acts on a pointer click only (`event.detail > 0`; Enter and Space do nothing), its label going through `StableLabel` ("Send the proposed answer", "Sending") so it keeps its box; and Dismiss. The proposal block has a reserved slot in the panel, so its coming and going moves nothing; no colour is added. `ProposalList.svelte` lists answer proposals with their session name and an Open meeting button. Proof: Vitest `app/src/components/components.test.ts > the proposed answer renders markup as text`, `> Send the proposed answer ignores a click with detail 0`, `> the author's composer stays empty beside a proposal`; Playwright `app/e2e/meeting.spec.ts > a pending answer proposal shows under its label with its rationale`, `> Send the proposed answer needs a pointer click`, `> a keyboard press never sends a proposal`, `> dismiss sends dismiss_proposal`, each in the light and the dark theme; `app/e2e/stable-size.spec.ts`, a `CASES` entry "send the proposed answer" on the meeting scene; `app/e2e/orchestrator.spec.ts > an answer proposal opens its meeting from the list`.
5. **Sending** (AD-182, AD-85, D-139, D-160). `send_answer(card_key, expected, text, proposal_id?)`: with a `proposal_id` the command carries no text; the collector reads the stored text, refuses a proposal whose status is not `pending` or whose session's delivery is no longer `ResumeLive` (`state_changed`, the proposal then `expired`), builds the `Answer` vector with that text (story 5.11's builder, `ResumeLive` only), sets the proposal `accepted` and confirms as for the author's answers (`busy` within 30 s). The text is copied to the clipboard on the same click, as the author's own Send does. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{a_proposal_is_sent_with_its_stored_text, a_sent_proposal_is_accepted_and_confirmed, a_proposal_is_refused_once_the_state_changed, a_non_pending_proposal_is_refused}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` (the signature changed).
6. **No unattended path** (principle 8, AD-182, R109). Nothing sends a proposal but the author's click: ingestion, expiry, the orchestrator's tools and a window restart spawn nothing. Proof: CLI-crate collector test `watch.rs::an_answer_proposal_is_never_sent_without_the_click` (a valid proposal ingested, boards rebuilt, the window restarted: the fake claude logs no `--resume`); the injection cases of AC 8.
7. **Journal and metrics** (AD-182, AD-85, D-139). `meeting_answer` gains `source` (`author`, `proposal`; absent reads `author`) and, for a proposal, its id; the text never. Story 5.19's `orders::metrics`, written after this story, counts the Delivery share per source from it (PRD section 9 item 3). Proof: core `crates/officina-core/src/relay/journal.rs::tests::meeting_answer_source_round_trips_and_defaults_to_author`; `watch.rs::the_journal_holds_the_proposal_id_and_never_the_text`.
8. **Injection battery** (AD-182, AD-95, D-147). Sections `proposal_file` and `mcp_tool` gain answer cases: a leading `/` or `!` after whitespace, control, bidi and invisible characters, an empty and a 4,001-character text, "send now" and "approve" text, an answer to an interactive session, to a session waiting on a question or a permission prompt, to an ended and to an unknown session, a forged `source` field; each is invalid with its reason (or refused by the tool's bounds) and spawns nothing; a valid one spawns exactly one `--resume` after the click, its vector equal to the golden `Answer` rebuild with the stored text. Proof: `crates/officina/tests/injection.rs::{proposal_file_cases, mcp_tool_cases, every_section_family_and_refusal_has_a_case}`.
9. **Zero write** (D-80). A proposal sent through the fake claude joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
10. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: answer proposals; README "The orchestrator (v0.5, in progress)": what the orchestrator may propose, to which sessions, that the author sends it with a click and nothing sends it otherwise, that it expires when the session moves on. Proof: review.

## Tasks

- [x] `McpTool::ProposeAnswer`, schema, bounds, `mcp.json` cases (AC 1)
- [x] `orders::proposal`: the `answer` kind's validation, reasons, statuses, expiry on the board (AC 2, 3)
- [x] `send_answer` with `proposal_id`; collector path; command signature, `ipc.ts`, mock (AC 5, 6)
- [x] `MeetingPanel.svelte` proposal block, `ProposalList.svelte` answer rows; stable-size case (AC 4)
- [x] `meeting_answer.source` and the proposal id (AC 7)
- [x] Injection cases, zero-write (AC 8, 9)
- [x] Update `CHANGELOG.md` and the README (AC 10)

## Dev Notes

- Requirements: PRD section 9 item 3, FR-6 as it amends, FR-11, FR-12, PRD section 7 Gesture and Delivery share rows. Decisions: AD-182, AD-85, AD-90, AD-91, AD-93, AD-95. Risks: R109. Epic decisions: D-28 (Epic 2); D-139, D-147, D-160, D-168, D-488, D-521 (the owner's confirmation of the click), D-564, D-575, D-576, D-577.
- Dependencies: stories 5.11 (`Answer`, the `Answer` vector, `send_answer`, confirmation), 5.15 (`officina mcp` and its writer), 5.17 (ingestion, statuses, `ProposalList.svelte`); story 5.6 already declared `ProposalKind::Answer` (D-575). It merges before 5.19, which reads its journal field.
- Why only `ResumeLive` (AD-182): a proposal is model text, and the only delivery that sends it as the session's next turn while the author watches the panel is `ResumeLive`; copy-and-attach would paste model text into a console by hand, and an ended session is not idle. A waiting session (a question, a permission prompt) never gets one: S4, a reply answers no prompt.
- The text sent is the stored one, read by the collector, never one the frontend sends back: what the author saw is what runs, and a page that was tampered with cannot change it.
- "Deciding alone" without the click is the owner's to extend (D-505, D-488); until he does, this story has no unattended path and AC 6 proves it.
- No employer content in fixtures and cases: placeholder sessions and texts.

## Testing

- Core unit: `crates/officina-core/src/{mcp,orders/proposal,relay/journal}.rs`; core integration: `crates/officina-core/tests/{mcp_vectors,orders_vectors}.rs`.
- CLI integration: `crates/officina/tests/{mcp,watch,injection,quota_doctor}.rs`. App crate: `main.rs`.
- Vitest: `components.test.ts`. Playwright: `app/e2e/{meeting,orchestrator,stable-size}.spec.ts`.

## Dev Agent Record

### Debug Log

- The board as built never puts a background session in Your turn: an idle one is placed by its state, Done for a finished turn (D-802). AD-182's "in Your turn" is read as live and idle in its turn, Done or Your turn with `ResumeLive` (D-860).
- The rationale's "1 to 500 characters" needed a reason for an empty one: `rationale_missing`, a third unit variant beside the two the story names (D-861).
- `cargo test -p officina-core --all-features` turns the mutants on and fails the launch tests by design; the gate's `--features officina-core/ts` is the run that counts.
- Two races the answer cases' added load exposed in the battery, fixed in the tests: the fake claude wrote a log line in pieces, so a console's resume and the collector's `claude agents` interleaved into an unparsable line (each line is now one write); `auto_approved_run_cases`'s catalogue change read the review row before a refused approval set its cause (D-734; the case now waits for the cause).
- Gate, Windows, on this branch's final code: fmt, build, clippy green; the workspace tests 1,628 passed, 0 of this story failed, 14 ignored, with machine-load flakes outside the story across the runs: `watch.rs::queued_reviews_are_paced_5_s_apart` (4,793 ms then 4,696 ms, the pacing agent's), `watch.rs::a_two_second_turn_is_confirmed` once, `board.rs::{hanging_claude_times_out_instead_of_blocking, a_process_claude_leaves_running_cannot_hold_the_board}` once (8.49 s and 4.18 s), each green when its binary ran alone; perf's `hook_latency_p95` own cost 42.6 ms over its 30 ms (the hook is untouched here), the gate's own cost 6.0 ms within 15 ms; every later step green: lexer differential, approvals stress, both crash suites, authority markers, dependencies, fixtures, `pnpm check`, lint, Vitest 318, Playwright 1,263 (x3), the three mutants caught, `cargo deny`, `pnpm audit` (one low, ignored). Linux (rust:1-bookworm): 1,450 passed, 0 failed, 12 ignored, green.

### Completion Notes

Decisions D-860 to D-867 (D-860 and D-865 to confirm). The tests proving each AC:

1. The tool: `tests/vectors/mcp.json`'s `propose_answer` cases through `crates/officina-core/tests/mcp_vectors.rs::{framing_cases_match, every_tool_and_error_has_a_case}`; `crates/officina-core/src/mcp.rs::tests::schemas_are_pinned` (hash pinned again, D-866); `crates/officina/tests/mcp.rs::{propose_answer_writes_one_file_and_returns_its_id, propose_answer_shares_the_rate_limits}`.
2. Validation: the answer cases of `tests/vectors/orders.json`'s `proposal` section through `crates/officina-core/tests/orders_vectors.rs::{proposal_cases_match, every_invalid_reason_has_a_case}`; `crates/officina-core/src/orders/proposal.rs::tests::an_answer_proposal_uses_the_composers_rules` (every `AnswerRefusal::ALL` variant through a proposal, the rationale's bounds, a forged field).
3. Statuses: `proposal.rs::tests::{an_answer_expires_when_the_session_leaves_your_turn, an_answer_expires_when_its_delivery_changes, a_newer_answer_supersedes_the_pending_one}`; the doctest of `meeting::takes_a_proposed_answer`; `crates/officina/tests/watch.rs::an_unsent_answer_expires_on_the_next_board`.
4. The panel: Vitest `components.test.ts > ProposedAnswer > {the proposed answer renders markup as text, Send the proposed answer ignores a click with detail 0, the author's composer stays empty beside a proposal, a meeting without a proposal keeps the slot, saying so}` and `> ProposalList > an answer proposal names its session and opens its meeting`; Playwright `meeting.spec.ts > {a pending answer proposal shows under its label with its rationale, Send the proposed answer needs a pointer click, a keyboard press never sends a proposal, dismiss sends dismiss_proposal}` in the light and the dark theme (the first with the computed colours), `> no proposal slot for a delivery a proposal never takes`; `stable-size.spec.ts` cases "send the proposed answer" and "proposed answer slot" on the meeting scene; `orchestrator.spec.ts > an answer proposal opens its meeting from the list`; core `meeting.rs::tests::the_pending_answer_shows_beside_the_composer`.
5. Sending: `watch.rs::{a_proposal_is_sent_with_its_stored_text, a_sent_proposal_is_accepted_and_confirmed}` (Windows: console starts), `watch.rs::{a_proposal_is_refused_once_the_state_changed, a_non_pending_proposal_is_refused}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` (the signature changed, `ipc.ts` calls `send_answer` twice).
6. No unattended path: `watch.rs::an_answer_proposal_is_never_sent_without_the_click` (ingested, boards rebuilt, the window restarted: no `--resume`, no line); the injection cases of AC 8.
7. Journal: `crates/officina-core/src/relay/journal.rs::tests::meeting_answer_source_round_trips_and_defaults_to_author`; `watch.rs::the_journal_holds_the_proposal_id_and_never_the_text`.
8. Injection: `crates/officina/tests/injection.rs::{proposal_file_cases, mcp_tool_cases, every_section_family_and_refusal_has_a_case}` with the answer cases of both sections (D-867); the exact `--resume` vector check runs on Windows, where console starts exist.
9. Zero write: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` sends a proposal by its id through the fake (Windows).
10. Documentation: `CHANGELOG.md` `[Unreleased]` Added, README "Proposed answers" under "The orchestrator (v0.5, in progress)" and the tools' paragraph.

## File List

- `crates/officina-core/src/orders/proposal.rs`, `meeting.rs`, `mcp.rs`, `relay/journal.rs`, `relay/mod.rs`
- `crates/officina-core/tests/orders_vectors.rs`, `mcp_vectors.rs`
- `crates/officina-host/src/meeting.rs`, `mcp.rs`, `watch.rs`, `registry/proposals.rs`, `registry/read.rs`, `registry/window.rs`
- `crates/officina-host/examples/orchestrator_fixture.rs`
- `crates/officina/tests/watch.rs`, `mcp.rs`, `injection.rs`, `quota_doctor.rs`, `common/mcp.rs`; `crates/officina/src/bin/officina-fake-claude.rs` (one write per log line)
- `app/src-tauri/src/main.rs`
- `app/src/components/ProposedAnswer.svelte` (new), `MeetingPanel.svelte`, `ProposalList.svelte`, `OrchestratorView.svelte`, `components.test.ts`; `app/src/App.svelte`
- `app/src/lib/ipc.ts`, `meeting.svelte.ts`, `meeting-text.ts`, `orchestrator-text.ts`, `app/src/e2e-mock.ts`
- `app/src/lib/generated/AnswerRefused.ts`, `MeetingView.ts`, `ProposalContent.ts`, `ProposalInvalidReason.ts`
- `app/e2e/meeting.spec.ts`, `orchestrator.spec.ts`, `stable-size.spec.ts`; `app/scripts/e2e-fixtures.mjs`
- `tests/vectors/mcp.json`, `orders.json`, `injection.json`
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this story

## Change Log

- 2026-10-28 created against the addendum (PRD section 9 item 3; AD-182; D-488, D-575)
- 2026-10-28 after the rework's adversarial pass: the invalid reasons are unit variants (rework F2); sending by id and supersession recorded in D-575 (rework F5)
- 2026-10-29 implemented; decisions D-860 to D-867; status review
