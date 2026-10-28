# Story 3.12: Doctor coherence, documentation, workstation checks and QA closure

**Status:** in-progress

## Story

As the author installing v0.3 on a workstation, and as a future reader of the repository,
I want doctor to report the registry and the triage model in one coherent section, the README and changelog to explain v0.3 in one piece, and the facts still unmeasured measured on the workstation,
so that v0.3 is set up without guessing, the epic closes with evidence, and nothing of the PRD is left deferred.

## Acceptance Criteria

1. Doctor groups the registry checks of stories 3.1, 3.2, 3.8, 3.9 and 3.11 in one section, in a stable order, each once; failures follow the v0.1 exit rule (1 when any check fails): a network registry and a failed `quick_check` fail; a synced folder, a newer schema, a quarantine, a restore notice, waiting or rewritten event files, missing hook fields and a paused model warn. Proof: CLI `crates/officina/tests/doctor_registry.rs::{registry_section_lists_each_check_once, warnings_alone_exit_0, a_failed_quick_check_exits_1, unc_officina_home_fails_the_registry_location}` (a planted marker for the synced warning, a damaged database file for the failure, `OFFICINA_HOME` on a UNC path for the network case per D-91).
2. README (D-73, D-80, D-85): the sections the stories added become one "Subjects and triage" section: what the registry is and where it lives, the sync exclusion, subjects, filing and what wakes a subject up, the Subjects view, triage and its keys, empty subjects, the evidence and the gates, the model off by default and contained, auto-archive and routine candidates, and the privacy paragraph: what is kept, for how long, `no_excerpts`, that bounds apply while the window runs and at its start but not while it is closed, and that Officina itself writes nothing under the Claude config dir while the `claude` processes it starts keep their own state. It transcribes no list owned by code. Proof: `crates/officina/tests/readme.rs::readme_mentions_every_subcommand` (story 2.10, or created here per D-44) and `readme.rs::readme_names_no_sync_marker`, which iterates `SYNC_ROOT_MARKERS` and fails when one appears in the README; the prose is reviewed by hand.
3. `CHANGELOG.md` `[Unreleased]` holds one coherent v0.3 set of entries under Added, Changed and Security, built from the entries each story added. Proof: review.
4. Workstation checks recorded in `docs/research-notes.md`, each with the Claude Code version (AD-58 manual list, PRD section 8 as amended by amendment 11, D-52, D-75): the `SessionEnd.reason` sent when a VS Code terminal hosting an interactive session is closed; for `/clear` and for `/resume` in an interactive session, the session ids before and after and the gap between `SessionEnd` and `SessionStart`, with `SUPERSESSION_WINDOW_MS` revisited if the gap does not fit; that `claude --resume <id>` and `claude -c` continue the same session id (D-75); WebView2 rendering of the Subjects view, the Triage panel and the evidence compared with Playwright Chromium by eye. Proof: manual, research notes entries.
5. A week of real use (AD-58 manual list, PRD section 7): recorded counts of window restarts and closed nights or weekends, sessions, subjects and confirmations before and after each restart (none lost), sessions filed by rule and how many of those the author later moved, confirmations per finished subject (one each), subjects woken up by a session alive again, and doctor's state (clean apart from expected warnings). Proof: manual, a research notes entry with those counts.
6. The full automated suite is stable: CI runs Playwright with `--repeat-each=3` (D-91); the ignored release tests (`perf.rs`, `registry_crash.rs`) pass; the zero-write test covers every v0.3 command and the triage call, on the config dir and `<home>/.claude.json` (D-80). Proof: the CI `windows` job.
7. QA closure (D-74): `docs/qa/epic-3-report.md` with the gate decision, the evidence table, the audits (security, focused on triage containment, R18 and the registry's text bounds; quality; test coverage against AD-58), the traceability table from every FR and every AD from AD-38 to AD-58 to the ACs that prove it, the owner's departures from the addendum listed with their decisions (P-17), and open items moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC.

## Tasks

- [x] Doctor registry section ordering and level rules; `doctor_registry.rs` cases (AC 1)
- [x] README "Subjects and triage" made coherent; `readme.rs` cases (AC 2)
- [x] CHANGELOG consolidation (AC 3)
- [ ] Workstation checks and research notes (AC 4): the owner's
- [ ] A week of real use, research notes with the counts (AC 5): the owner's
- [x] `--repeat-each=3` in the CI Playwright step; release tests; zero-write coverage (AC 6)
- [x] Audits, `docs/qa/epic-3-report.md`, backlog (AC 7): the five audits fixed, decided (D-435 to D-451) or not reproduced, the report written, the open items in the backlog

## Dev Notes

- Requirements: FR-1 (doctor), PRD section 7, section 8 as amended (amendment 11). Decisions: AD-53 (doctor line), AD-58 (manual list). Epic decisions: P-7 (Epic 2), P-16, P-17, D-44, D-52, D-73 to D-75, D-80, D-85, D-91.
- Dependencies: stories 3.1 to 3.11. The week of real use starts once 3.11 is merged on the workstation build; the report waits for it (P-16).
- The VS Code close reason matters even with AD-47's rule: a close while the window is shut would hide a real session (amendment 11, R24).
- If a measured `/clear` or `/resume` gap does not fit D-52's window, or an id behaves otherwise than D-75 states, edit `epic-3-decisions.md` with the fact and the code in one change, with its vector case.
- The UNC doctor test uses a share on `localhost` that does not exist, so nothing waits on a remote host (story 3.1 Dev Notes).
- Cutting `[0.3.0]` and bumping the workspace version belong to the v0.3 release, after `[0.2.0]` is cut; not in this story.
- No employer content in the README, the research notes or the report: placeholder repositories and paths only (officina-project rule).

## Testing

- CLI integration: `crates/officina/tests/{doctor_registry,readme,quota_doctor}.rs`.
- CI: the full `windows` job, release tests, Playwright with `--repeat-each=3`.
- Manual: research notes entries (AC 4, 5), the QA report (AC 7).

## Dev Agent Record

### Debug Log

- **Scope of this pass:** the code and documentation parts, AC 1, 2, 3 and 6. AC 4 and AC 5 are the owner's (workstation checks, a week of real use) and AC 7 (audits and the QA report) runs after this pass; their tasks stay open and the story stays `in-progress`.
- **Decisions where the story is silent:**
  - AC 1: `doctor::REGISTRY_SECTION` names the section's titles in their order; `run` gathers the registry's checks, the hook fields and the triage model's lines and sorts them by it, so the section reads the same whichever check emits a line. The hook fields line moved from the `claude` group into the section; `check_claude` returns the version it read for it. A missing hook field now warns rather than fails, as the AC lists it with the warnings (story 3.8 had it fail): the board keeps working and the registry records the session without the field. An unreadable registry (`registry: cannot be read`) still fails, like a failed `quick_check`.
  - AC 1, tests: one home holds every registry warning at once (a planted sync marker, a quarantine, a recent restore, eight waiting event files, a rewritten one, a missing hook field, a paused model) beside a doctor otherwise clean (hooks merged, a hook line, the quota written), so `warnings_alone_exit_0` proves the exit rule rather than another check's; the newer-version warning is the same home with `user_version` raised in the file's header. The damaged database fails on `quick_check` itself or on SQLite's corruption error, both under `registry integrity`.
  - AC 2: the README's privacy facts of the registry are gathered in "Privacy in the registry" (excerpts, confirmation copies, model answers, triage calls, `no_excerpts`, the bounds applied while the window runs and at its start but not while it is closed, and the zero write), which the top-level Privacy section links; `officina subjects --json` moved under the Subjects view; the registry part says what doctor's section holds, what fails and what warns. `readme_names_no_sync_marker` iterates `SYNC_ROOT_MARKERS`, with a positive control on the exclusion sentence.
  - AC 3: the v0.3 entries of `[Unreleased]` are one set, each tagged "(v0.3)", in the order of the README; the triage containment and the registry writer's absence from `officina.exe` moved to Security, beside the zero write.
  - AC 6: the zero-write fingerprint now runs the window's own model call (a winning dry run seeded, the model on, a pending row sent through the fake with the fingerprinted config dir), beside the dry run's call and every command. The release tests (`perf.rs`, `registry_crash.rs`) ran in the full gate below.

### Full gate at the end of stories 3.11 and 3.12 (code and docs parts)

Every job of `.github/workflows/ci.yml`, run locally with `--locked`, on a0e5ade: `cargo fmt --check`; frontend install (frozen lockfile) and build; clippy `--workspace --all-targets --features officina-core/ts` clean, in the `core-linux` feature set, and on the host without `window-authority`; `cargo test --workspace --features officina-core/ts` 970 passed, 0 failed, 11 ignored (TypeScript drift green); `perf.rs` alone in release 5 passed (hook own cost 12.8 ms, budget 150 ms; gate own cost 3.0 ms, budget 15 ms; in-process p95 2.8 ms); the lexer differential 2 passed; `approvals_stress` passed; the crash step 3 passed (helper built first); "officina holds no authority": both markers absent from `officina.exe`, present in `officina-app.exe`; "Registry dependencies" ok; "No bundled roots" ok; `pnpm check`, `lint`, `test` 47 passed; fixtures rebuilt from the real CLI (`model` and `model-paused` new); Playwright with `--repeat-each=3`, as CI now runs it, 303 passed (101 cases); `cargo deny --locked check` ok; `pnpm audit` 1 low, ignored with its reason (unchanged). The `core-linux` job replayed in a `rust:1-bookworm` container with Rust 1.99.0: fmt, clippy clean, 858 passed, 0 failed, 9 ignored. Node 25.4 locally against CI's 22.

### Audit fixes and QA closure (AC 7)

- **Inputs:** the five Epic 3 audit reports (security, penetration, quality, debt, dependencies), kept out of the repository; each finding was checked against the code before it was changed, and the report lists the two that did not hold (penetration M2, and parts of quality M9 and M10) with the reason.
- **Owner's decisions applied**, each recorded in `epic-3-decisions.md` (the Epic 3 range D-42 to D-97 is full, so D-435 to D-451 continue after the last id used, D-434): the same-user boundary and restore by the day in the copies' names (D-435), the shim refused for every `claude` call through one runner (D-436), a line's time held within its file's day (D-437), read-side text bounds (D-438), the triage environment as an allow list (D-439), Confirm all leaving out a model's `finished` (D-440), `no_excerpts` at read time and the hook's floor warning (D-441), a refused row sent once more when the model is turned on again (D-442), the gone triage thread (D-443), one verdict snapshot (D-444), the ingestion plan in core with vectors (D-445), the thresholds the page reads (D-446), the Privacy section on what the model sends (D-447); D-448 to D-451 record what stays as it is and why.
- **Tests that fail without each fix:** `registry_store.rs::restore_follows_the_day_in_the_name_not_the_modification_time`; `claude.rs::tests::every_call_refuses_a_cmd_shim_before_it_starts`; `ingestion.rs::tests::a_line_is_recorded_within_its_files_day_and_never_after_now`, `registry_ingest.rs::{a_future_dated_line_is_recorded_no_later_than_its_ingestion, text_beyond_the_hooks_bounds_never_reaches_the_registry}`; `triage_call.rs::tests::{the_prompt_is_bounded_whatever_the_case_holds, environment_is_an_allow_list_derived_from_the_constants}` and the new `prompt`, `model_output` and `ingestion` vector cases; `triage.rs::{only_the_allowed_variables_reach_the_child, refused_row_is_sent_once_more_when_the_model_is_turned_on_again}`; `triage.spec.ts > {confirm all leaves out a model's finished, which Enter confirms on its row, Enter and Space on confirm all confirm nothing, a row's project name is bidi-isolated}`, `registry.test.ts`; `registry_read.rs::no_excerpts_hides_every_excerpt_when_the_view_is_read`, `watch.rs::tests::excerpts_turned_off_runs_the_pass_at_once`, `doctor.rs::tests::a_no_excerpts_floor_the_hook_cannot_see_warns`; `registry_triage.rs::a_gone_worker_ends_the_dry_run_and_refuses_another`, `triage.rs::tests::a_gone_thread_is_seen_on_submit_and_on_results`; `ingest.rs::tests` (`LineReader`); `evidence.spec.ts > the windows the evidence names come from the view's thresholds`; `readme.rs::{readme_privacy_names_what_the_triage_model_sends, readme_states_the_registry_is_no_boundary_against_the_author}`; `registry_retention.rs::a_failed_read_of_the_cards_subjects_is_reported`; `registry_store.rs::the_pragmas_are_the_ones_ad_39_names`; `main.rs::tests::every_declared_command_is_allowed` against `ipc.ts`. Refactors (the runner, the snapshot, the ingestion loop, the publishers, the command answers) keep the suite green, the crash tests included.
- **Not testable here:** a job that cannot be created or assigned (D-436) has no fault seam; the code path is read, not run. The allow list's effect on a real `claude` behind a corporate proxy is the owner's probe run (backlog).
- **Documents:** README (how `claude` is found and the shim refusal, the allow list, Confirm all, `no_excerpts`, the registry's boundary, the Privacy section's triage model entry), CHANGELOG `[Unreleased]` (Security and Fixed), research note E5, story 3.8's doctor sentence, story 3.9's renamed test names, `docs/backlog.md` (the owner's items and the recurring PRD close list), `docs/qa/epic-3-report.md`, and Epic 3 set closed on code in `sprint-status.yaml` with 3.12 kept in progress for AC 4 and AC 5.

### Full gate at the AC 7 closure

Every job of `.github/workflows/ci.yml`, run locally with `--locked` on Rust 1.99.1, after the audit fixes: `cargo fmt --check`; frontend install (frozen lockfile) and build; clippy `--workspace --all-targets --features officina-core/ts` and the `core-linux` feature set, clean; `cargo test --workspace --features officina-core/ts` 999 passed, 0 failed, 11 ignored (TypeScript drift green); `perf.rs` alone in release 5 passed (hook own cost 12.6 ms, budget 150 ms; gate own cost 2.4 ms, budget 15 ms; in-process p95 3.3 ms); the lexer differential 2 passed; `approvals_stress` passed; the crash step 3 passed (helper built first); "officina holds no authority": both markers absent from `officina.exe`, present in `officina-app.exe`; "Registry dependencies" ok; "No bundled roots" ok; `pnpm check`, `lint`, `test` 51 passed; fixtures rebuilt from the real CLI; Playwright with `--repeat-each=3` 315 passed (105 cases); `cargo deny --locked check` (0.19.4) ok; `pnpm audit` (11.5.3) 1 low, ignored with its dated reason. The `core-linux` job replayed in a `rust:1-bookworm` container with Rust 1.99.1: fmt, clippy clean, 886 passed, 0 failed, 9 ignored.

### Completion Notes

- AC 1: `crates/officina/tests/doctor_registry.rs::{registry_section_lists_each_check_once, warnings_alone_exit_0, a_failed_quick_check_exits_1, unc_officina_home_fails_the_registry_location}`; `crates/officina-host/src/doctor.rs::tests::{run_reports_every_check_in_order, model_state_and_counts_are_reported}`; `crates/officina/tests/quota_doctor.rs::doctor_reports_hook_fields_missing_and_not_observed` (now a warning).
- AC 2: `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_names_no_sync_marker}`; the prose reviewed by hand.
- AC 3: `CHANGELOG.md` `[Unreleased]`; review.
- AC 4: open, the owner's: the VS Code close reason, the `/clear` and `/resume` ids and gaps, `claude --resume <id>` and `claude -c`, WebView2 against Playwright Chromium, each with the Claude Code version, in `docs/research-notes.md`.
- AC 5: open, the owner's: a week of real use on a build holding story 3.11, with the counts the AC names, in `docs/research-notes.md`.
- AC 6: `.github/workflows/ci.yml` (Playwright `--repeat-each=3`); the release tests in the gate above; `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`, which covers every command, the window's own model call and a dry run's call, on the config dir and `<home>/.claude.json`.
- AC 7: `docs/qa/epic-3-report.md` (gate decision closed on code, evidence, the five audits with what was fixed, decided or not reproduced, the traceability table from FR-1 to FR-14 and AD-38 to AD-58, the P-17 departures, open items); `docs/stories/epic-3-decisions.md` D-435 to D-451; `docs/backlog.md`.

## File List

- `crates/officina-host/src/doctor.rs`, `crates/officina/tests/{doctor_registry,quota_doctor,readme}.rs`
- `.github/workflows/ci.yml`
- `README.md`, `CHANGELOG.md`, `docs/stories/story-3.12-docs-doctor-and-qa.md`, `docs/stories/sprint-status.yaml`
- AC 7 and the audit fixes: `docs/qa/epic-3-report.md`, `docs/stories/epic-3-decisions.md`, `docs/backlog.md`, `docs/research-notes.md`, `docs/stories/story-3.8-hook-fields-canary.md`, `docs/stories/story-3.9-triage-call-and-dry-run.md`; code across `crates/officina-core/src/{agents,dates}.rs`, `crates/officina-core/src/registry/`, `crates/officina-host/src/{claude,process,paths,doctor,watch}.rs`, `crates/officina-host/src/registry/`, `crates/officina/src/main.rs`, `app/src-tauri/src/main.rs`, `app/src/`, `app/e2e/`, `app/scripts/e2e-fixtures.mjs`, `tests/vectors/triage.json`; `.github/workflows/{ci,release}.yml`, `deny.toml`, `rust-toolchain.toml`, `app/pnpm-workspace.yaml`

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: UNC doctor case automated, Playwright `--repeat-each=3` in CI (D-91); resume and clear ids to be recorded on the workstation (D-75; AC 4, still open: nothing was recorded then, the debt audit noted the past tense); counts of the week of real use named; README privacy states the closed-window and zero-write facts (D-80, D-85); P-17 departures listed in the QA report
- 2026-10-28 code and documentation parts implemented (AC 1, 2, 3, 6): doctor's registry section in one stable order with missing hook fields as a warning, the README's Subjects and triage made one piece with its privacy gathered, the v0.3 changelog consolidated, Playwright three times over in CI, the window's model call in the zero-write fingerprint; full gate green. AC 4 and 5 (the owner's) and AC 7 (audits, QA report) open; status in-progress
- 2026-10-28 AC 7: the five audits answered with no debt carried (fixed, decided as D-435 to D-451, or not reproduced), `docs/qa/epic-3-report.md` written, the open items in the backlog, Epic 3 closed on code; status stays in-progress for the owner's AC 4 and AC 5
