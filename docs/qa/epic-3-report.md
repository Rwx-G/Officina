---
title: Epic 3 QA report (v0.3, The Registry and Triage)
author: Rwx-G
date: 2026-10-28
---

# Epic 3 QA report

## Executive summary

Gate decision: **closed on code**. All twelve stories (3.1 to 3.12) are
implemented and covered by automated tests, and the five Epic 3 audits
(security, penetration, quality, debt, dependencies) were answered inside
the epic, with no debt carried: every finding is fixed, decided with its
reason (`epic-3-decisions.md` D-435 to D-451) or not reproduced, and what
waits on the owner is in `docs/backlog.md`. Epic 3 closes **on use** once
the owner has run story 3.12 AC 4 (the workstation checks) and AC 5 (a week
of real use), and run the triage probe once more with the environment's new
allow list (D-439).

Top findings, all fixed in the epic:

1. A line dated in the future escaped the 30-day privacy bound and froze
   later observations: a line's time is now held within its file's day and
   never past the pass that reads it (security M1, D-437).
2. Nothing bounded text that did not come from the hook: a line, an excerpt
   or a triage case could be any size, and so could a call's cost. Lines
   longer than the hook writes are corrupt, excerpts are cut to the hook's
   bound, a case is sent bounded, and files are read in windows (security
   M2, D-438).
3. The triage call's environment was the window's minus a few names, so a
   variable mapping the `haiku` alias or an IDE's could reach it: it is now
   an allow list of system, network and authentication variables (security
   L1, D-439).
4. Confirm all took a model's `finished` pre-selected, so one click could
   archive a subject on text a session read: such rows are confirmed one by
   one (security L2, D-440).
5. A `.cmd` shim of `claude` was refused for triage but run for `claude
   agents` and `--version`: no call runs one now, through one contained
   runner (penetration M1, quality M4, D-436).
6. A dry run waited forever when the triage thread was gone (quality H1,
   D-443); the README's Privacy section did not say what the triage model
   sends (debt H2, D-447).

## Evidence

Figures from the full local gate replaying every job of
`.github/workflows/ci.yml` with `--locked`, on the last commit of this
closure, Rust 1.99.1.

| Gate | Result |
|---|---|
| `cargo fmt --check`; clippy `-D warnings`, workspace with `officina-core/ts`, and the `core-linux` feature set | clean |
| Rust unit, integration, CLI end to end, doc tests, vector sets (classification, argv, gate, url, triage with its new `ingestion` section), TypeScript bindings drift | 999 passed, 0 failed, 11 ignored (the release-only perf, stress, crash and lexer tests below) |
| Perf, release, alone (hook own cost, gate own cost, gate in process, two append stresses) | 5 passed; hook own cost 12.6 ms (bound 150 ms on CI, 30 ms on the workstation, D-19); gate own cost 2.4 ms (bound 15 ms on CI, 5 ms on the workstation); gate in process p95 3.3 ms (asserted as a workstation check only) |
| Lexer against Git Bash's own parse | 2 passed |
| Approvals stress, two consumers of one approval | 1 passed: exactly one wins |
| Crash, release (helper killed inside a chunk's and a confirmation's transaction and at 20 random points each) | 3 passed: no event lost, offsets at line ends, every acknowledged confirmation present once |
| `officina.exe` built alone | neither the relay's authority marker nor the registry writer's; both found in `officina-app.exe` (positive controls); no `window-authority` or `test-seams` in its feature graph |
| Registry dependencies, no bundled roots | `rusqlite` present and no ruled-out crate in `officina-host`; no `webpki-roots` in `officina`'s graph, `rustls` found (positive control) |
| Frontend: svelte-check, ESLint 10, Vitest | svelte-check 0 errors, ESLint clean, Vitest 51 passed in 6 files (`registry.test.ts` new) |
| e2e fixtures rebuilt from the real CLI, Playwright under the window's CSP, `--repeat-each=3` | 315 passed (105 cases, three times each) |
| `cargo deny` (0.19.4), `pnpm audit` (11.5.3) | `cargo deny` ok (advisories, bans, licenses, sources); `pnpm audit` 1 low, dev only, ignored with its dated reason |
| `core-linux` replayed in a `rust:1-bookworm` container | Rust 1.99.1: fmt and clippy clean, 886 passed, 0 failed, 9 ignored |
| Zero write under the Claude config dir and `~/.claude.json` | fingerprint identical around every command, a dry run's call and the window's own model call (`quota_doctor.rs::no_command_writes_under_the_claude_config_dir`) |
| Triage probe, one real call (story 3.9 AC 13) | PASS on Claude Code 2.1.312 (E16), before D-439: to run again (backlog) |
| CI on GitHub | runs on the push of this closure |

## Success metrics (PRD section 7)

- **Memory survives restarts and nights** (sessions, subjects and
  confirmations before and after each restart, none lost): **mechanism
  proved, metric not yet measured**. Proved by `registry_crash.rs` (no
  event lost, every acknowledged confirmation present once after a kill at
  any point), `registry_store.rs` (quarantine, restore, copies) and
  `watch.rs::start_ingests_and_purges_before_the_first_board`. The counts
  are story 3.12 AC 5, the owner's week of real use.
- **Filing by rule is right** (sessions the author moves after filing):
  **not yet measured**; the rule is the vector set's `filing` section and
  `registry_ingest.rs::filing_runs_in_the_creating_transaction`. AC 5.
- **One confirmation per finished subject**: **met in test**
  (`registry_write.rs::batch_is_one_transaction_answered_per_item`,
  `confirm` vectors, D-63's recomputation); counted in AC 5.
- **Privacy bound** (no excerpt survives 30 days): **met**, a compile-time
  assertion on the TTL, the copy lifetime and the pass interval
  (`retention.rs::tests::ttl_copy_lifetime_and_pass_interval_stay_within_30_days`),
  now also against a line dated in the future
  (`registry_ingest.rs::a_future_dated_line_is_recorded_no_later_than_its_ingestion`).
  With the window closed nothing expires; the README says so.
- **Containment of the model call**: **met in test** (argument vector,
  stdin only, allow-listed environment, empty work folder, kill-on-close
  job, 60 s, 64 KiB, `structured_output` only) and once on the workstation
  (E16); to confirm on the workstation with D-439's environment.

## Audits

Each report was read against the code before anything was changed; a
finding that does not hold is listed as not reproduced, with the reason.
The audit texts stay out of the repository (they name workstation
details); paths below are the repository's.

- **Security audit** (defensive, OWASP Top 10:2025, ASVS 5.0, LLM and
  agentic Top 10): 0 critical, 0 high, 2 medium, 4 low, 6 info. Fixed:
  M1, a future-dated line (D-437); M2, read-side text bounds (D-438); L1,
  the environment's allow list (D-439); L2, Confirm all and a model's
  `finished` (D-440); L3 and L4, `no_excerpts` applied when the view is
  read and its pass run at once, doctor's warning for a floor the hook
  cannot see (D-441); Info, a job that cannot be set up stops the call
  (D-436), a timeout no longer says nothing was changed, registry text
  bidi-isolated beside other words. Decided (D-448): the confirmation
  vectors unbounded at the command wrapper (the CSP leaves no script to send
  one) and excerpt deletions outside archive truncating the WAL at the next
  pass. Moved to the backlog: whether `--setting-sources ""` stops a
  `CLAUDE.md` above the work folder (a probe extension, owner's).
  Not reproduced: none. Totals: 9 fixed, 2 decided, 1 backlog.
- **Penetration review** (static, offensive): 2 high, 5 medium, 5 low, 4
  info (positive). High, both downgraded to Info by D-435: the registry,
  its copies and its `triage_call` rows are not a boundary against code
  running as the author, as the relay's files are not; the README states
  it; the cheap fix is in, restore ordered by the day in the copies' names
  (L5). Fixed: M1, the shim refused for every call (D-436); M3, a call's
  usage out of range read as unknown and the sums saturating; L5. Decided
  (D-448): M4 supersession links from hook lines, M5 the work folder's
  aliases, L1 `has_data`'s quoting, L2 the job's assignment gap (D-64), L3
  a reader's `-wal` and `-shm` (D-82; the checks compare the database file
  only, verified), L4 a request dropped near its deadline (D-92). Not
  reproduced: M2, "the nonce is seeded with the time only": `RandomState`
  is keyed from the operating system's random source (once per thread, then
  stepped), so the time adds to an unpredictable key rather than replacing
  it, and the escaping of the data line, which the report itself calls the
  primary defence, makes the markers unforgeable whatever the nonce.
  Totals: 3 fixed, 8 decided, 1 not reproduced (the four Info notes are
  positive; the bidi one was fixed with the security Info).
- **Quality audit**: 3 high, 12 medium, 8 low, 3 info. High, all fixed: the
  dead triage thread (D-443), one `VerdictSnapshot` read once per batch and
  again only after a write (D-444), the file plan in core with vectors and
  bounded reads (D-445). Medium fixed: read errors reported instead of
  defaults (auto-archive setting, doctor's model facts, the cards'
  subjects, settings serialised with a checked helper); one guard for every
  window command and one time per request; one contained runner; one day
  and one hour in `core::dates`, doctor's restore notice from the view;
  each command's answer through one conversion and the command list derived
  from `ipc.ts`; timing tests proved by the next emission and bounded from
  the constants they guard; a Playwright case for "no key confirms all" and
  `registry.test.ts` for the store; doctor's dates and window from core.
  Fixed in part, the rest decided (D-449): settings glue (only
  `AuthorAction::involved` moved to core), `watch.rs` (one `Published`
  shape, the file kept whole), `check_registry` (its facts from core, its
  length kept), the store's per-row maps (the pointer check moved out of
  the relay store, the panel's keys from one map). Decided (D-449): the
  two long core functions, the contract store's place. Low fixed: row
  mappers and a setting read by key, named pragma fields with AD-39's
  values pinned, the triage runner's smells (work folder created apart, the
  dead branch gone, the stderr limits named), the command line's named
  exit and read outcome. Low decided (D-449): the writer's test harness
  (marked), `store.rs`'s module, per-line queries. Not reproduced, in part:
  M9's "no proving test for AD-55" (a Vitest case, `components.test.ts >
  Enter and Space on Confirm all do nothing`, existed; the Playwright case
  was added anyway) and M10's "the model input is computed again per row"
  (`row_model` reads the facts, not a second input). Totals: 3 high and 14
  medium or low fixed wholly or in part, 6 decided, 2 not reproduced in
  part.
- **Debt audit** (dead code and false statements): 2 high, 5 medium, 13
  low, 6 info. High, both fixed: the view ships only the thresholds the
  page writes and the page reads them (D-446); the Privacy section says
  what the triage model sends, with a test that keeps it (D-447). Medium,
  all fixed: the window's unread `integrity` dropped, three unused board
  fixtures no longer written, `RESTORE_NOTICE_MS` and `DAY_MS` single,
  research note E5 brought to E16, future-tense test comments corrected.
  Low fixed: story 3.8's `[FAIL]` sentence annotated, the weak
  `quota_doctor.rs` assertion explained by its `[FAIL] hooks` line, the
  self-asserting schema check removed, `AUTHOR_WAIT`'s and
  `SYNC_ROOT_MARKERS`'s docs corrected, the command list derived, two
  unexported TypeScript derives removed, the sprint status legend and story
  3.12's change log corrected. Low decided: `Source::Model` reserved and so
  documented, the writer's harness marked, AD-56's E2 note (D-451). Info:
  the fixture tool's step list moved onto `Step`'s variants; the tripwires
  it names (exhaustive `Record`s, `every_past_version_has_a_fixture`, the
  contract's schema tag, the blanket `dead_code` waiver of the test
  helpers) are kept. Totals: 17 fixed, 3 decided, 0 not reproduced.
- **Dependency audit**: 0 critical, 0 high, 1 medium, 8 low, 5 info; no
  resolved version matched an advisory. Fixed: cargo-deny and pnpm at exact
  versions in CI, `unsound = "workspace"` stated and `webpki-root-certs`
  banned in `deny.toml`, the deepmerge ignore dated, Rust 1.99.1 with CI, a
  run by hand allowed, and a recurring PRD close list in the backlog (the
  bundled SQLite against sqlite.org's latest, the toolchain's point
  releases, the pinned tools, the ignores' dates). Decided (D-450): the
  exact `ureq` pin, `pkg-config` and `vcpkg` under the bundled SQLite,
  duplicate versions, `@types/node` 24 against Node 22 (types only, a
  superset of what the build uses); the third-party notices file stays the
  backlog's release item. Totals: 6 fixed, 4 decided, 0 not reproduced.

## Traceability

Every functional requirement of `docs/prd/prd-v0.3.md` and every decision
from AD-38 to AD-58 of `docs/architecture-v0.3.md`, with the story
acceptance criteria and the tests that prove it. Paths are under
`crates/` unless they start with `app/` or `tests/`.

| Requirement | Proving ACs | Tests |
|---|---|---|
| FR-1 Durable store | 3.1 AC 4 to 13, 3.2 AC 8, 3.12 AC 1 | `officina-host/tests/registry_store.rs` (pragmas, the values AD-39 states, network, sync markers, version rule, quarantine and restore by the day in the names, copies), `registry_schema.rs`, `officina/tests/doctor_registry.rs`, `watch.rs::corruption_while_running_makes_the_registry_unavailable_until_restart` |
| FR-2 Ingestion before retention | 3.2 AC 2, 6, 7, 10 | `registry_ingest.rs`, `registry_retention.rs::{hourly_pass_ingests_then_copies_then_retains, four_utc_days_are_ingested_before_any_deletion, unavailable_registry_deletes_nothing}`, `officina/tests/board.rs::board_json_deletes_no_event_file`, the `ingestion` vector section |
| FR-3 Session record | 3.2 AC 4, 3.3 AC 1 to 7 | `fold.rs` tests, `registry_read.rs::session_record_round_trips_with_each_source`, the `fold` vector section, `watch.rs::session_seen_live_is_never_non_interactive` |
| FR-4 Excerpts and privacy | 3.2 AC 1, 4, 6; 3.4 AC 5, 6; 3.7 AC 2; audit fixes D-437, D-438, D-441 | `events.rs` tail tests, `retention.rs::tests::ttl_copy_lifetime_and_pass_interval_stay_within_30_days`, `registry_ingest.rs::{a_future_dated_line_is_recorded_no_later_than_its_ingestion, text_beyond_the_hooks_bounds_never_reaches_the_registry, no_excerpts_drops_excerpts_at_ingestion}`, `registry_read.rs::no_excerpts_hides_every_excerpt_when_the_view_is_read`, `registry_write.rs::{archive_clears_excerpts_from_the_file_and_the_wal, finished_leaves_one_expiring_copy_then_none}` |
| FR-5 Filing | 3.4 AC 1 | the `filing` vector section, `registry_ingest.rs::filing_runs_in_the_creating_transaction`, `watch.rs::a_session_first_seen_in_agents_is_filed` |
| FR-6 Subjects view | 3.4 AC 7 to 9, 3.5 AC 6 to 10 | `view.rs` tests, `officina/tests/subjects.rs`, `board.rs::cards_carry_their_subject`, `app/e2e/subjects.spec.ts`, `app/e2e/board.spec.ts > cards show their subject` |
| FR-7 What needs a verdict | 3.6 AC 1 | the `triage` vector section, `verdict.rs` tests |
| FR-8 Proposals | 3.6 AC 3, 3.11 AC 2 | `proposal.rs` tests, `officina/tests/triage.rs::{enabled_model_answers_default_rows_only, one_case_per_call}` |
| FR-9 Batch confirmation | 3.7 AC 1 to 9; audit fix D-440 | the `confirm` vector section, `registry_write.rs::batch_is_one_transaction_answered_per_item`, `registry_crash.rs::killed_while_confirming_keeps_every_acknowledged_confirmation_once`, `app/e2e/triage.spec.ts > {confirm all shown confirms the shown rows with one click, confirm all leaves out a model's finished, which Enter confirms on its row, Enter and Space on confirm all confirm nothing}`, `app/src/lib/registry.test.ts` |
| FR-10 Triage model | 3.9 AC 1 to 13, 3.11 AC 1 to 8; audit fixes D-436, D-438, D-439, D-442, D-443 | `triage_call.rs` tests (argv, schema, prompt, the allow list, the bound prompt), the `prompt` and `model_output` vector sections, `officina/tests/triage.rs` (zero launches on each refusal, stdin only, `only_the_allowed_variables_reach_the_child`, timeout ends the tree, `refused_row_is_sent_once_more_when_the_model_is_turned_on_again`), `registry_triage.rs::a_gone_worker_ends_the_dry_run_and_refuses_another`, `registry_write.rs` enable gate, pause and self-disable, `app/e2e/evidence.spec.ts`, E16 |
| FR-11 Measured error | 3.10 AC 1, 5 | the `rates` vector section, `rates.rs` tests, `evidence.spec.ts > per source, flipped finished proposals with the count they rest on` |
| FR-12 Auto-archive gate | 3.10 AC 1, 3, 4, 6 | `registry_write.rs::{auto_archive_is_refused_while_the_gate_does_not_hold, auto_archive_never_takes_a_model_unfiled_or_empty_subject_row}`, `watch.rs::a_re_instructed_auto_archived_session_counts_as_a_flip`, `evidence.spec.ts > the auto-archive toggle explains why it is disabled` |
| FR-13 Routine marker | 3.7 AC 9, 3.10 AC 7 | `view.rs::tests::routine_candidates_exclude_merged_sources`, `subjects.rs::routine_candidates_are_printed`, `triage.spec.ts > confirming finished offers the routine box` |
| FR-14 Hook fields in the canary | 3.8 AC 1 to 4, 3.12 AC 1 | `doctor.rs::tests::every_buffered_event_has_hook_fields`, `events.rs::tests::absent_lists_the_missing_hook_fields`, `hook.rs::payload_missing_a_field_records_absent`, `registry_ingest.rs::absent_after_the_first_sighting_updates_hook_missing`, `board.spec.ts > the contract chip names missing hook fields`, `quota_doctor.rs::doctor_reports_hook_fields_missing_and_not_observed` |
| AD-38 Module layout, single writer | 3.1 AC 3, 7, 12; 3.5 AC 1 | `watch.rs::{author_request_is_answered_by_the_collector_thread, expired_author_request_is_dropped_unexecuted}`, `app/src-tauri/src/main.rs::tests::author_reply_times_out`, the CI marker step (both markers absent from `officina.exe`, present in the window) |
| AD-39 File and connection settings | 3.1 AC 6, 7; 3.5 AC 9 | `store.rs::tests` (location), `registry_store.rs::{writer_sets_every_pragma, the_pragmas_are_the_ones_ad_39_names, reader_never_blocks_the_writer}`, `registry_retention.rs` (WAL truncated) |
| AD-40 Schema version 1 | 3.1 AC 3, 4; 3.3 AC 1; 3.11 AC 3 | `registry_schema.rs::{every_table_is_strict, check_lists_match_the_core_enums}`, `registry_write.rs::model_text_never_leaves_model_answer` |
| AD-41 Migrations and the version rule | 3.1 AC 5, 9; 3.2 AC 8 | `registry_schema.rs::migration_hashes_are_pinned`, `registry_store.rs::{older_version_is_copied_then_migrated, newer_version_opens_read_only_for_the_writer, reader_reports_each_state}` |
| AD-42 Integrity, copies, quarantine | 3.1 AC 10, 11; 3.2 AC 8; audit fix D-435 | `registry_store.rs::{damaged_page_is_quarantined_and_the_newest_valid_copy_restored, restore_follows_the_day_in_the_name_not_the_modification_time, a_damaged_copy_never_pushes_out_a_valid_one}`, `restore_order`'s doc test, `registry_retention.rs::a_failed_read_of_the_cards_subjects_is_reported` |
| AD-43 Ingesting event files | 3.2 AC 2, 3, 5, 7; audit fixes D-437, D-438, D-445 | `registry_ingest.rs`, the `ingestion` vector section (`open_event_file`, `close_event_file`, `recorded_at`), `ingest.rs::tests` (`LineReader`), `registry_crash.rs::killed_mid_transaction_and_at_random_loses_no_event` |
| AD-44 Retention per surface | 3.2 AC 6, 7, 10, 13; 3.4 AC 6; 3.9 AC 10 | `retention.rs` tests, `registry_retention.rs`, `doctor.rs::tests::more_than_seven_waiting_event_files_warn`, `registry_retention.rs::triage_call_rows_are_kept_90_days` |
| AD-45 Event line additions | 3.2 AC 1, 3.8 AC 2 | `events.rs` tests, `hook.rs` |
| AD-46 Agents snapshots | 3.3 AC 3 | `fold.rs::tests::{only_changed_facts_are_written, vanished_without_session_end_is_interrupted}`, `registry_ingest.rs::no_waiting_for_and_no_status_history_are_stored` |
| AD-47 Ends, supersession, non-interactive | 3.3 AC 4, 6, 7; 3.6 AC 1 | `fold.rs` supersession tests, `watch.rs::session_end_and_vanishing_in_one_tick_keep_the_real_end`, the `fold` and `triage` vector sections |
| AD-48 Subjects and filing | 3.4 AC 1, 3; 3.5 AC 7 | `registry_write.rs::{author_actions_round_trip, merge_moves_sessions_and_marks_the_source, name_rules_are_enforced, every_author_refusal_has_a_case}`, `AuthorAction::involved`'s doc test, `subjects.spec.ts` |
| AD-49 Verdict, proposals, confirmation | 3.6 AC 1, 3, 4; 3.7 AC 1, 2 | `confirm.rs::tests::changed_proposal_is_refused`, the `confirm` vector section, `registry_write.rs::confirmation_copies_the_proposal_it_answered`, `ItemOutcome::wrote`'s doc test |
| AD-50 Rates and gates | 3.10 AC 1 to 3, 3.11 AC 1, 5, 6 | `rates.rs` tests, the `rates` vector section, `registry_write.rs` (gates, pause, self-disable), `triage.rs::self_disable_through_the_fake_claude` |
| AD-51 The triage call | 3.9 AC 1 to 4, 13; audit fixes D-436, D-438, D-439 | `triage_call.rs::tests::{argv_is_pinned, schema_is_pinned, prompt_is_pinned_for_a_fixed_nonce, environment_is_an_allow_list_derived_from_the_constants, the_prompt_is_bounded_whatever_the_case_holds}`, `officina/tests/triage.rs`, `claude.rs::tests::every_call_refuses_a_cmd_shim_before_it_starts`, E16 |
| AD-52 Worker, budget, dry run | 3.9 AC 6 to 8, 3.11 AC 2; audit fixes D-442, D-443 | `watch.rs::board_keeps_its_pace_while_a_triage_call_hangs`, `registry_triage.rs` (budget across a restart, dry run, gone worker), `triage.rs` proposals tests |
| AD-53 CLI readers | 3.1 AC 13, 3.2 AC 10, 3.4 AC 8, 3.9 AC 11 | `subjects.rs` (exit 0, 3, 64; no write), `board.rs::board_json_reports_the_registry_state_with_the_window_closed`, `doctor_registry.rs`, `doctor.rs::tests::{cmd_shim_is_reported_as_refusing_triage, a_no_excerpts_floor_the_hook_cannot_see_warns}` |
| AD-54 Window payloads and commands | 3.4 AC 7, 9; 3.5 AC 2, 3; 3.7 AC 11; 3.8 AC 4; 3.9 AC 9; audit fix D-446 | `view.rs` tests, `typescript_bindings.rs::committed_bindings_match_the_rust_types`, `main.rs::tests::every_declared_command_is_allowed` (now against `ipc.ts` too), `watch.rs::{registry_view_is_emitted_only_on_change, author_action_emits_a_new_view}`, `evidence.spec.ts > the windows the evidence names come from the view's thresholds` |
| AD-55 Views, keyboard, focus | 3.5 AC 4, 5, 8, 10; 3.7 AC 5 to 10; 3.9 AC 9; 3.10 AC 6; 3.11 AC 3; audit fix D-440 | `subjects.spec.ts`, `triage.spec.ts` (keys, Confirm all and its left-out causes, no key confirms all, bidi isolation, text never HTML), `components.test.ts`, `registry.test.ts` |
| AD-56 Hook fields in the canary | 3.8 AC 1, 3, 4 | `doctor.rs::tests::every_buffered_event_has_hook_fields`, `registry_ingest.rs::{absent_after_the_first_sighting_updates_hook_missing, hook_not_observed_shrinks_as_events_arrive}` (E2, and E17 for `Notification`, D-90, D-451) |
| AD-57 Vector set | 3.3 AC 8, 3.6 AC 7, 3.11 AC 8; audit fixes D-438, D-445 | `officina-core/tests/triage_vectors.rs::{every_case_matches, every_value_has_a_case}` over `tests/vectors/triage.json`, `officina-host/tests/registry_ingest.rs::fold_vectors_hold_through_the_writer` |
| AD-58 Test strategy | 3.2 AC 11, 3.7 AC 3, 3.9 AC 5, 12, 13, 3.12 AC 6 (automated); 3.12 AC 4, 5 (manual, the owner's) | `registry_crash.rs` (release), `perf.rs` (release), `quota_doctor.rs::no_command_writes_under_the_claude_config_dir`, the fake claude's `triage` modes, Playwright `--repeat-each=3`; the triage probe (E16); the VS Code close reason and the week of use wait on the owner (backlog) |

## Departures from the addendum (P-17)

The stories follow the owner's decisions where they depart from the PRD or
`architecture-v0.3.md`, which are not edited:

- **D-75** replaces AD-48's resume filing clause and PRD amendment 5's
  first sentence: a resumed conversation keeps its id and subject.
- **D-76** changes AD-49's anchor (the subject's latest session that saw a
  prompt) and adds the empty-subject row to FR-7.
- **D-77** adds "alive again" to AD-48's statuses.
- **D-80** is how the PRD's Containment row and AD-58's zero-write row read:
  Officina writes nothing under the config dir; the `claude` it starts
  keeps its own state.
- **D-82** replaces D-46: a reader may create `-wal` and `-shm`.
- **D-85** makes AD-44's 28-day TTL a value derived from the 30-day bound,
  and AD-52's 28-day evaluation window the same bound.
- After the audits: **D-435** amends D-94 (restore by the day in the
  names); **D-436** extends AD-51's shim refusal to every `claude` call;
  **D-439** replaces AD-51's environment bullet with an allow list;
  **D-440** extends AD-55's Confirm all exclusions; **D-441** amends D-85
  (`no_excerpts` at read time, the pass at once); **D-442** amends D-87
  (a refused row sent once more on turning the model on again, the owner's
  decision); **D-445** adds an `ingestion` section to AD-57's vector set;
  **D-446** narrows the thresholds AD-54's view carries; **D-451** notes
  AD-56's E2 reference covers four events, E17 the fifth.

## Known limits

- The registry, like the relay, is not a security boundary against code
  running as the author (D-435): a process of the author's can rewrite it,
  its copies and its call rows, and so set subjects, verdicts, gates and
  the call budget. The boundary is the isolated workshop (v0.6).
- With the window closed nothing expires, and while the registry cannot be
  written the event files keep the hook's excerpts for as long as that
  lasts (D-85, D-441); doctor warns past seven waiting files.
- The triage call's environment lists come from Claude Code's
  documentation and E16's run before them; the workstation run with the
  allow list is open (backlog).
- The model's answer is untrusted text that decides nothing, but a
  persuaded `finished` can still be confirmed by the author one row at a
  time; auto-archive never takes it.

## Open items

Owner's items, listed High in `docs/backlog.md`, each recorded in
`docs/research-notes.md` when done:

1. Story 3.12 AC 4: the VS Code close reason, the `/clear` and `/resume`
   ids and gaps, `claude --resume` and `claude -c`, WebView2 against
   Playwright Chromium, each with the Claude Code version.
2. Story 3.12 AC 5: a week of real use with the counts the AC names.
3. The triage probe once more with D-439's allow list.
4. Whether a `CLAUDE.md` above the triage work folder reaches the call (a
   probe extension, security Info).

Also in the backlog: the third-party notices file before the first
release (unchanged since Epic 2), and the recurring PRD close list (the
bundled SQLite, the toolchain, the pinned audit tools, the ignores).

## Verdict

Epic 3 is **closed on code**: twelve stories in review except 3.12, every
acceptance criterion proved by a named test except the owner's two manual
ones, every requirement and every decision from AD-38 to AD-58 traced to
its proof, and every finding of the five audits fixed, decided with its
reason or not reproduced, with nothing left silently. It is **closed on
use** after story 3.12 AC 4 and AC 5 and the probe's second run, and a
green CI on the last push (P-7, P-16).
