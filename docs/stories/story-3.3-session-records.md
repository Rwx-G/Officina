# Story 3.3: Session records, provenance, ends and supersession

**Status:** review

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

- [x] `SessionFact` enum and `SessionRecord` pivot in `crates/officina-core/src/registry/{types,fold}.rs` (AC 1)
- [x] Hook facts in the fold applied by `ingest` (AC 2)
- [x] Agents diff in the collector through the creation function, seeding at start, chain head routing (AC 3)
- [x] End facts and their precedence, alive again, supersession with `SUPERSESSION_WINDOW_MS`, non-interactive (AC 4-7)
- [x] `SESSION_START_SOURCES` beside `SESSION_END_REASONS` in `crates/officina-core/src/events.rs` (AC 8)
- [x] `tests/vectors/triage.json` (`fold` section), `crates/officina-core/tests/triage_vectors.rs` (AC 8)
- [x] `registry_read.rs`; collector tests in `watch.rs` with the copied fake (AC 1, 3-7, 9)
- [x] Update `CHANGELOG.md` and the README (AC 11)

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

- **Decisions where the story is silent:**
  - AC 1: `SessionFact` is `kind`, `name`, `cwd`, `claude_pid`, `background_id`, `status`, `state` (from `claude agents`), `listed` (when `claude agents` began to list the session), `started`, `last_prompt_at`, `last_stop_at`, `ended` (from the hook), `interrupted` (rules) and `successor` (hook). `started` and `ended` hold JSON with the source or reason and the line's own `claude_pid`, so the link reads the pid of the very lines it pairs and a v0.1 line links nothing (D-52). `unknown_facts` counts a name this build does not know and a value of another type. Filing is `SessionRecord.subject: Option<Sourced<Option<i64>>>`, read from the three `subject_*` columns (`None` inside: unfiled since then).
  - AC 2: a line writes `cwd` and `claude_pid` whenever it carries them, and one fact per event (`started`, `last_prompt_at`, `last_stop_at`, `ended`); "a prompt was seen" is `last_prompt_at` being present. Every write is an upsert with `WHERE excluded.at_ms > at_ms`, the SQL form of core `fold::replaces`.
  - AC 3: a field `claude agents` leaves out is unknown, not a change: the last known value stays (FR-3), so no fact is ever written empty. The diff's baseline for a session the snapshot does not hold is what the registry stores, so a session coming back writes only what differs, plus `listed`. The snapshot lives in `WindowRegistry` and is seeded at the first successful refresh from every record, the core keeping D-51's filter; it moves on only once the refresh's transaction commits. `Collector::refresh_agents` now says whether it answered, and the watch loop folds only a refresh that did (AD-3 stale rule). A record is routed to its chain head while the successor's `claude_pid` equals the record's pid.
  - AC 4, AC 5: alive again never deletes a fact, which would break order-independent folding: an end (the hook's or `interrupted`) holds only while no later sign of life exists, a sign being a `UserPromptSubmit` time or the `listed` time. The hook's end is read first, so it beats `interrupted` whatever the times. A background session ends by its state only (AD-47); `interrupted` is still written when one vanishes. `SessionStart` alone is no sign of life (D-77 names two).
  - AC 6: an end takes the earliest start linking to it, a start the latest end, so a chain of clears links each session to its own successor.
  - AC 7: non-interactive is derived in the record (never `listed`, and the hook's end `other` holds), so it needs no write and stays order-independent.
  - AC 8: the vector cases name a session's whole fact map (value and source), its end, whether it is superseded and whether it is non-interactive. The core replays them through an in-memory registry in the test; the host replays the same section through the real writer, event files and snapshot (`registry_ingest.rs::fold_vectors_hold_through_the_writer`), so the SQL and the rules cannot drift.
- **Adversarial pass** (a failed refresh, two records naming one session, a successor cycle, a session listed while its `SessionEnd` waits to be ingested, a v0.1 hook, a superseded session resumed): one real miss, now a test and fixed in its own commit: a conversation `/resume` left and later resumed back kept its successor link, so it read superseded for good, and a record naming the session that replaced it looped back to the wrong head (`fold.rs::tests::a_session_resumed_after_its_successor_is_itself_again` and the vector case "a conversation resumed back after its successor is itself again"). A successor now counts until a later sign of life, as an end does. Checked and holding: a failed refresh keeps the collector's last list and is never folded; two records of one session in a listing write once at that time (equal times never replace); a successor cycle stops on the visited set; ingestion runs before every session list (D-78).
- **Mutation checks:** moving the tick's ingestion after the session list turns `session_end_and_vanishing_in_one_tick_keep_the_real_end` red (its first version passed, since its event tick's spawn was rate-limited; it now waits past the startup's spawns); a changed expectation turns `every_case_matches` red; the former supersession rule turns the resumed-back case red.
- **Per-commit gates:** fmt; clippy on core (`ts`), the host with and without `window-authority`, and the workspace; the tests of core, host (`window-authority`) and the CLI crate's `watch`, `quota_doctor`, `board`, `readme`, `hook`, `fake_claude`, `doctor_registry`; the crash test in release (the pass now writes facts per line, 3.1 s for 200 000 lines). The full CI replay runs at the end of story 3.4.

### Completion Notes

- AC 1: `crates/officina-core/src/registry/fold.rs::tests::{rows_pivot_into_sourced_fields, unknown_fact_names_are_counted}`; `crates/officina-host/tests/registry_read.rs::session_record_round_trips_with_each_source` (every fact, every source).
- AC 2: the `fold` cases of `tests/vectors/triage.json`, replayed by `crates/officina-core/tests/triage_vectors.rs::every_case_matches` and `crates/officina-host/tests/registry_ingest.rs::fold_vectors_hold_through_the_writer`.
- AC 3: `fold.rs::tests::{only_changed_facts_are_written, last_seen_with_a_change_or_after_10_min, vanished_without_session_end_is_interrupted, record_of_a_superseded_session_goes_to_its_chain_head}`; `registry_ingest.rs::no_waiting_for_and_no_status_history_are_stored`; `crates/officina/tests/watch.rs::{agents_facts_carry_source_agents, a_session_first_seen_in_agents_gets_one_record, session_gone_while_the_window_was_closed_is_interrupted}`.
- AC 4: the `fold` cases, among them "vanished first, then a SessionEnd with an earlier ts: the SessionEnd wins" and "a background session is ended by done, failed or stopped"; `watch.rs::session_end_and_vanishing_in_one_tick_keep_the_real_end`.
- AC 5: the `fold` cases "ended, then listed live by claude agents", "background done then working is alive again", "ended, then a newer prompt", "SessionStart with source resume for a known id continues its record".
- AC 6: `fold.rs::tests::{supersession_links_whichever_line_comes_first, start_outside_the_window_links_nothing, line_without_claude_pid_links_nothing}` and the `fold` cases.
- AC 7: the `fold` cases; `watch.rs::session_seen_live_is_never_non_interactive`.
- AC 8: `triage_vectors.rs::{every_case_matches, every_value_has_a_case}` (`SESSION_END_REASONS`, `SESSION_START_SOURCES`, `LifecycleState::ALL`); `tests/vectors/` is under no formatter.
- AC 9: the `watch.rs` tests of AC 3 to 7 run `watch_listing`, a copied fake configured by its sidecar.
- AC 10: `crates/officina-core/tests/classification_vectors.rs` unchanged and green.
- AC 11: `CHANGELOG.md` `[Unreleased]` Added; README "What a session record holds".

## File List

- `crates/officina-core/src/events.rs`, `crates/officina-core/src/registry/{mod,types,fold}.rs`, `crates/officina-core/tests/triage_vectors.rs`, `tests/vectors/triage.json`
- `crates/officina-host/Cargo.toml`, `crates/officina-host/src/{collector,watch}.rs`, `crates/officina-host/src/registry/{mod,read,write,ingest,agents,window}.rs`, `crates/officina-host/tests/{registry_read,registry_ingest,registry_crash}.rs`
- `crates/officina/tests/watch.rs`
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.3-session-records.md`, `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `SessionEnd` beats `interrupted` whatever the times, two facts (D-78); sessions first seen in agents created by the shared function (D-79); alive again on the session side (D-77); resume keeps its id, supersession never files (D-75); re-filing clause removed; fake claude sidecar (D-86)
- 2026-10-28 implemented: session facts with their source and time from event lines and `claude agents`, the agents diff with its seeded snapshot and chain-head routing, ends and their precedence, alive again, supersession, non-interactive runs, the `fold` vector set replayed by core and through the writer; one adversarial fix (a conversation resumed back stayed superseded). Status review
