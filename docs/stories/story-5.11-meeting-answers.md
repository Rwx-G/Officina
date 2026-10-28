# Story 5.11: Answers: the composer, delivery and confirmation

**Status:** Approved

## Story

As the author answering a background employee from the meeting panel,
I want my written answer to reach the session as its next turn when Claude Code can deliver it, to stay copied and in the panel until that is confirmed, and to fall back to copy and attach when it cannot,
so that answering a background session costs me one sentence, and an answer is never lost (J4, journey 4).

## Acceptance Criteria

1. **Answer rules** (AD-85, FR-6, E20, D-127). `meeting::Answer::parse(&str) -> Result<Answer, AnswerRefusal>`: 1 to 4,000 characters; no control character but newline and tab; no character of `FORBIDDEN_FORMAT_RANGES`; not starting, after leading whitespace, with `/` or `!`; when E20's `--resume` cases failed, `AnswerRefusal::LeadingDash` also refuses a leading `-`. `AnswerRefusal` is a `closed_enum!`. Proof: core `crates/officina-core/src/meeting.rs::tests::{answer_bounds, answer_refuses_controls_and_format_characters, answer_refuses_a_leading_slash_or_bang_after_whitespace, every_answer_refusal_has_a_case}` (the last iterates `AnswerRefusal::ALL`).
2. **The `Answer` vector** (AD-79, AD-80, FR-6 as amended by amendment 3, D-128). `VectorKind` gains `Answer`; `spawn::answer(&Answer, &SessionId, cwd)` builds `--resume <session id>`, then `--` per E20, then the answer as one argument; a new console; the binary's folder for a live background session, for an ended one the folder `claude agents` reported for that session id, never one from an event line (D-168); only an `.exe` accepted; D-128's session-kind environment: `SESSION_ENV_REMOVED` and `CLAUDE_CODE_DISABLE_AGENT_VIEW` removed, the author's `OFFICINA_*` names kept, since a resumed session runs the author's hooks (review F1). A session id is a canonical lowercase UUID. Proof: the `Answer` cases of `tests/vectors/spawn.json` (live and ended) through `crates/officina-core/tests/spawn_vectors.rs`; core `crates/officina-core/src/spawn.rs::tests::{session_id_must_be_a_canonical_uuid, the_answer_is_one_argument}`; CLI `crates/officina/tests/spawn.rs::{every_kind_reaches_the_fake_claude_as_its_golden_vector, a_console_kind_gets_a_console, session_kinds_keep_the_authors_officina_settings}` (story 5.2's iterations now cover `Answer`).
3. **Delivery with the version and E21** (AD-85, FR-15, D-139, D-127). `meeting::delivery` takes the `claude` version the collector last read: below 2.1.285 (`RESUME_PROMPT_MIN_VERSION`), or when the version is unknown, a live idle background session gets `CopyAndAttach`; when E21 failed, `ResumeLive` is never returned. Proof: core `meeting.rs::tests::{an_old_claude_falls_back_to_copy_and_attach, an_unknown_version_falls_back_to_copy_and_attach}` and, per E21's outcome, `meeting.rs::tests::resume_live_follows_gate_3`.
4. **Sending** (AD-85, FR-6, D-63's pattern). `send_answer(card_key, expected: Delivery, text)`: the collector recomputes the delivery from the latest board and refuses `state_changed` when it differs from the one displayed; `ResumeLive` and `ResumeEnded` build the `Answer` vector; `CopyAndAttach` builds `Attach` and puts no text on a command line; `CopyOnly` (D-168) spawns nothing, the text being copied; `FocusOnly` has no composer. Spawns run on the command's blocking thread. `send_answer` is declared in `build.rs`, allowed in the capability file, wrapped in `ipc.ts` and answered by `e2e-mock.ts`. Proof: CLI-crate collector tests `crates/officina/tests/watch.rs::{a_changed_state_refuses_the_answer, copy_and_attach_puts_no_text_on_a_command_line, copy_only_spawns_nothing, resume_live_spawns_the_answer_vector, resume_ended_runs_in_the_agents_folder}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
5. **Confirmation** (AD-85, D-135, D-139, D-160; review F14, F37). After a `ResumeLive` or `ResumeEnded` spawn, the collector watches that session id in `claude agents` (a resumed session keeps its id, D-75), refreshing at its minimum spawn interval rather than the 15 s safety poll for those 30 s: `busy` within 30 s confirms; otherwise `not_confirmed`. The outcome and confirmation requests carry no deadline and are never dropped. Fake claude mode `resume` marks the session `busy` in its fixture (D-129). Proof: `watch.rs::{an_answer_is_confirmed_when_the_session_turns_busy, a_two_second_turn_is_confirmed, an_answer_is_not_confirmed_after_30_s}` (the second with the fake turning `busy` then `idle` within two seconds; the third with the fake leaving the session idle, at explicit times).
6. **Journal** (AD-85, AD-32 as amended). `meeting_answer` (session, delivery, answer SHA-256, outcome `spawned`, `spawn_failed` or `refused`), then for resumes a second line `confirmed` or `not_confirmed`; never the text. Proof: core `crates/officina-core/src/relay/journal.rs::tests::meeting_answer_round_trips`; `watch.rs::the_journal_holds_the_hash_never_the_text` (searches the journal file for the answer's bytes).
7. **The composer** (AD-93, FR-6, D-168). `AnswerComposer.svelte` shows for `ResumeLive`, `ResumeEnded`, `CopyAndAttach` and `CopyOnly`: a textarea that is never prefilled, a character counter from the limits in the payload, a Send button reachable by keyboard and `Ctrl+Enter`; Send copies the text to the clipboard (`navigator.clipboard`) on the same gesture whatever the delivery; `lib/meeting.svelte.ts` keeps the draft per card key in memory until confirmed or discarded; the panel shows "not confirmed" with the text still there. Proof: Vitest `components.test.ts > AnswerComposer starts empty and counts characters`, `> Ctrl+Enter sends and copies`, `> a refused answer shows its reason and keeps the text`; Playwright `app/e2e/meeting.spec.ts > an answer to a live background session is sent and confirmed`, `> an unconfirmed answer stays in the panel and on the clipboard`, `> a waiting background session copies and opens attach`, `> an ended session never seen by claude agents is copy only`, `> a live interactive session offers Focus and no composer`.
8. **Zero write** (D-80). An answer through the fake claude joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
9. **Doctor** (AD-94 part A, FR-15). Below 2.1.285 doctor warns that answers to background sessions fall back to copy and attach. Proof: host `crates/officina-host/src/doctor.rs::tests::an_old_claude_warns_about_answers`.
10. **Documentation** (D-149, FR-6). `CHANGELOG.md` `[Unreleased]` under Added: written answers to background and ended sessions; README "The orchestrator (v0.5, in progress)": the deliveries described as families (delivered as a turn, copied with attach or alone, Focus only) naming the `Delivery` constant, never copying its table (D-149, review F44); that the answer sits on a process command line while its console runs; that the journal keeps only its hash. Proof: review.

## Tasks

- [ ] `Answer`, `AnswerRefusal` in `meeting.rs` (AC 1)
- [ ] `VectorKind::Answer`, `spawn::answer`, golden cases, `SessionId` (AC 2)
- [ ] Version and E21 in `delivery` (AC 3)
- [ ] `send_answer` through the collector, spawns, `CopyOnly`, confirmation watch at the minimum spawn interval, journal lines; command, capability, `ipc.ts`, mock (AC 4-6)
- [ ] Fake claude `resume` mode (AC 5)
- [ ] `AnswerComposer.svelte`, drafts in `lib/meeting.svelte.ts` (AC 7)
- [ ] Zero-write, doctor (AC 8, 9)
- [ ] Update `CHANGELOG.md` and the README (AC 10)

## Dev Notes

- Requirements: FR-6 as amended (amendment 3), FR-15 Slice A, PRD section 7 Delivery check and Delivery share rows. Decisions: AD-32 as amended, AD-79, AD-80, AD-85, AD-93. Facts: E20, E21 (gates 2 and 3), S3, S4 (a reply never answers a permission prompt). Risks: R35. Epic decisions: D-63, D-75, D-80, D-86 (Epic 3); P-24, D-124, D-127 to D-129, D-135, D-139, D-149, D-160, D-168.
- Seams (D-124): the focus and attach path (v0.1, moved to `spawn` by 5.2), the collector's agents refresh (v0.1), the journal (2.5).
- Record in the Completion Notes which branches of E20 and E21 shipped (D-127).
- The answer on a command line is visible to local process listings while the console runs: accepted (FR-6); the README says so.
- Send is keyboard-reachable because an answer approves nothing (AD-93); Launch and Approve stay pointer-only.
- The Delivery share metric reads the second journal line (D-139); `CopyAndAttach` answers count in the denominator.

## Testing

- Core unit: `crates/officina-core/src/{meeting,spawn,relay/journal}.rs`; core integration: `spawn_vectors.rs`.
- Host unit: `crates/officina-host/src/doctor.rs`. CLI integration: `crates/officina/tests/{watch,spawn,quota_doctor}.rs`.
- Vitest: `components.test.ts`. Playwright: `app/e2e/meeting.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: session-kind environment keeps `OFFICINA_*` (F1); console proven (F11); confirmation requests never dropped (F14); ended sessions resume only in an agents-reported folder, `CopyOnly` (F20); confirmation refreshes at the minimum spawn interval (F37); README describes deliveries without copying the table (F44)
