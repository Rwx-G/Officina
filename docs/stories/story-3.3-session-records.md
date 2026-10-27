# Story 3.3: Session records, provenance, ends and supersession

**Status:** Approved

## Story

As the author who has to trust what Officina says about a session days later,
I want every session recorded once with each fact's source and time, its real end, and its successor when `/clear` or `/resume` replaced it in the same process,
so that "ended", "interrupted" and "superseded" are facts I can trace, never guesses.

## Acceptance Criteria

1. Provenance (FR-3, AD-40): each observed fact is one `session_fact` row holding its latest value, `src` and `at_ms`; core pivots the rows into `SessionRecord` with `Sourced<T>` fields and counts fact names it does not know instead of dropping them; filing and status stay columns with their own source and time. Proof: core `crates/officina-core/src/registry/fold.rs::tests::{rows_pivot_into_sourced_fields, unknown_fact_names_are_counted}`; host `crates/officina-host/tests/registry_read.rs::session_record_round_trips_with_each_source`.
2. Hook facts (FR-3, AD-43): each ingested line records, with source `hook`, the facts the later rules read (process id, working directory, end and its reason, whether a prompt was seen, times of the last prompt and of the last `Stop`). Proof: the `fold` cases of `tests/vectors/triage.json` (AC 8).
3. Agents snapshots (AD-46, D-51, D-79): after each successful refresh the collector diffs the records with its previous snapshot and writes changed facts only, with source `agents`; a session first seen there is created through the creation function of story 3.2 (D-79), so it gets its row and project like one first seen in an event; `last_seen_ms` is written with another change or when over 10 minutes old; a session gone from the list without `SessionEnd` gets `interrupted` with source `rules`; at start the previous snapshot is seeded from the registry; a record naming a session superseded in the same process goes to its chain head; no raw snapshot, status history or `waitingFor` is stored. Proof: core `fold.rs::tests::{only_changed_facts_are_written, last_seen_with_a_change_or_after_10_min, vanished_without_session_end_is_interrupted, record_of_a_superseded_session_goes_to_its_chain_head}`; host `crates/officina-host/tests/registry_ingest.rs::no_waiting_for_and_no_status_history_are_stored`; CLI-crate collector tests `crates/officina/tests/watch.rs::{agents_facts_carry_source_agents, a_session_first_seen_in_agents_gets_one_record, session_gone_while_the_window_was_closed_is_interrupted}`.
4. Ends (AD-47, D-78): an interactive session is ended by `SessionEnd` or by `interrupted`, two distinct facts; an explicit `SessionEnd` always beats `interrupted`, whatever the timestamps; a background session is ended by `state` `done`, `failed` or `stopped`, a `done` session being alive and idle (E1) yet ended for triage. Proof: the `fold` cases, among them "vanished first, then a `SessionEnd` with an earlier `ts`"; CLI-crate `watch.rs::session_end_and_vanishing_in_one_tick_keep_the_real_end`.
5. Alive again (D-77, session side): a session held as ended that `claude agents` lists not ended, or that receives a `UserPromptSubmit` newer than its end, is no longer ended; a resumed conversation keeps its session id (D-75), so `--resume <id>` continues the same record. The subject side is story 3.4. Proof: the `fold` cases "ended then listed live", "background `done` then `working`", "ended then a newer prompt", "`SessionStart` with source `resume` for a known id continues its record".
6. Supersession (AD-47, D-52, D-75): the `clear` and `resume` link of D-52 sets the `successor` fact (source `hook`) whichever line is ingested first; a start outside the window, or lines without `claude_pid`, link nothing; the link never files or moves a session, it only marks the superseded session (no triage row of its own, story 3.6) and serves the board's live card (E14). Proof: `fold.rs::tests::{supersession_links_whichever_line_comes_first, start_outside_the_window_links_nothing, line_without_claude_pid_links_nothing}` and the `fold` cases.
7. Non-interactive (FR-3 as amended by amendment 3, AD-47): a session never seen in `claude agents` that ended with `other` is recorded as non-interactive; a session seen live is never non-interactive. Proof: the `fold` cases; `watch.rs::session_seen_live_is_never_non_interactive`.
8. Vector set `tests/vectors/triage.json` (AD-57), hand-written, redacted, formatter-exempt, is created with its `fold` section: agents records and event lines in, expected session records with the `src` of each fact out; `crates/officina-core/tests/triage_vectors.rs::every_case_matches` replays every section present; `triage_vectors.rs::every_value_has_a_case` iterates `SESSION_END_REASONS`, a new core constant `SESSION_START_SOURCES` (the documented values of E2) and `LifecycleState::ALL`, failing on a value no case uses (D-45: later stories add their enums). Proof: those two tests.
9. In-process collector tests hand the fake claude its agents fixtures through the sidecar of story 3.2 (D-86). Proof: the `watch.rs` tests of AC 3 to 7 run against a copied fake.
10. The board does not change: classification still reads event files (AD-9, AD-11). Proof: `crates/officina-core/tests/classification_vectors.rs` unchanged and green.
11. Documentation (D-73): `CHANGELOG.md` `[Unreleased]`; README "Subjects and triage": what a session record holds and that each fact says where it came from. Proof: review.

## Tasks

- [ ] `SessionFact` enum and `SessionRecord` pivot in `crates/officina-core/src/registry/{types,fold}.rs` (AC 1)
- [ ] Hook facts in the fold applied by `ingest` (AC 2)
- [ ] Agents diff in the collector through the creation function, seeding at start, chain head routing (AC 3)
- [ ] End facts and their precedence, alive again, supersession with `SUPERSESSION_WINDOW_MS`, non-interactive (AC 4-7)
- [ ] `SESSION_START_SOURCES` beside `SESSION_END_REASONS` in `crates/officina-core/src/events.rs` (AC 8)
- [ ] `tests/vectors/triage.json` (`fold` section), `crates/officina-core/tests/triage_vectors.rs` (AC 8)
- [ ] `registry_read.rs`; collector tests in `watch.rs` with the copied fake (AC 1, 3-7, 9)
- [ ] Update `CHANGELOG.md` and the README (AC 11)

## Dev Notes

- Requirements: FR-3 as amended (amendment 3). Decisions: AD-40 (provenance), AD-43 (folding is order-independent), AD-46, AD-47, AD-57. Facts: E1 (background `done` is idle and alive; `--all` keeps completed sessions listed), E2 (documented `source` and `reason` values), E14 (`CLAUDE_PID` equals the agents pid). Epic decisions: P-17, D-45, D-51, D-52, D-75, D-77 to D-79, D-86.
- The end fact model (D-78): the hook's end (with its reason) and the rule's `interrupted` are two facts, and the record's "ended" reads the hook's first. The tick order of story 3.2 (ingest, then agents) makes the usual case right; the precedence makes the late case right.
- Every new session id follows the normal filing rule (D-75); supersession never files. Filing itself is story 3.4, applied by the creation function both paths share (D-79).
- The previous snapshot lives in memory (AD-46); D-51 only seeds it at start, so the "vanished" rule also covers a session that died while the window was closed. A failed refresh keeps the previous snapshot and writes nothing (AD-3 stale rule).
- A new fact needs no migration (AD-40): `SessionFact` is a core enum stored as text with no `CHECK` (D-88), and the pivot counts names it does not know so a downgrade shows them instead of dropping them.
- D-52's 60 s window is a guess, not a measurement: story 3.12 records the real gap on the workstation. Keep the constant named so that fact can move it.
- `waitingFor` stays out of the registry (AD-46): it is a live display fact, the board has it.
- `tests/vectors/` is exempt from every formatter (derived-not-transcribed rule). Agent samples come from the shapes of E1, never from a real workstation.

## Testing

- Core unit: `crates/officina-core/src/registry/fold.rs`.
- Core integration: `crates/officina-core/tests/{triage_vectors,classification_vectors}.rs`.
- Host integration: `crates/officina-host/tests/{registry_read,registry_ingest}.rs`.
- CLI-crate collector: `crates/officina/tests/watch.rs`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `SessionEnd` beats `interrupted` whatever the times, two facts (D-78); sessions first seen in agents created by the shared function (D-79); alive again on the session side (D-77); resume keeps its id, supersession never files (D-75); re-filing clause removed; fake claude sidecar (D-86)
