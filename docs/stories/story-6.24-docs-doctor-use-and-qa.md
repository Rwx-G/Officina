# Story 6.24: Doctor coherence, documentation, use metrics and QA closure

**Status:** Approved

## Story

As the author installing v0.6 on a workstation, and as a future reader of the repository,
I want doctor to report the workshop's setup in one section, the README and changelog to explain v0.6 in one piece, the use metrics read after real use, and the epic closed with evidence,
so that the workshop is set up without guessing, I know whether it earns its place, and nothing of the PRD is left deferred before v0.7 starts.

## Acceptance Criteria

1. **Doctor** (AD-123, FR-18 as amended by amendment 14, D-228). The workshop checks the stories added (6.3, 6.4, 6.8 to 6.11, 6.13, 6.17, 6.21, 6.22) are one `closed_enum!` with a level function (fail, warn, inform) and one order (review F45); doctor prints them in one "Workshop" section, in that order, each once, and adds FR-18's two printed notes: on Docker Desktop's WSL 2 backend the engine runs in a utility VM sharing one kernel with the author's WSL distributions, so a container or kernel escape may reach them and the Windows drives they mount (the cautious reading, not verified); and the standing advice never to leave the SSH agent unlocked, failing that `ssh-add -c`. Failures follow the v0.1 exit rule (1 when any check fails). The families: what would make a launch impossible or unsafe fails (program resolution, the engine's platform, missing Linux programs, a missing guard program, a mount the engine cannot carry or that holds tracked files); what weakens the evidence or the setup warns (a stale battery, version and resource mismatches, same-user configuration around `docker.exe`, sync placeholders and reparse points, settings a worker can read, kept and orphan objects); counts and states inform. The level of each check is its variant's, never a list in this story. With no `[workshop]` table the section says the workshop is off and nothing else. Proof: CLI `crates/officina/tests/doctor_workshop.rs::{the_workshop_section_lists_each_check_once, every_check_prints_at_its_level, warnings_alone_exit_0, a_docker_shim_exits_1, a_missing_pretooluse_program_exits_1, both_notes_are_printed, no_workshop_table_says_off}` (the first two iterate the check enum's `ALL`).
2. **README** (D-226). The sections the stories added become one "The workshop" section: what it is for and where the boundary ends (PRD section 1); setup (the token and `cmdkey`, the terms question of the gate 7 entry, the floor keys with placeholders, the engine sentence of FR-1, one CA per engine, Build images, the battery); a run (card, launch refusals described as families with `LaunchRefusal` named, limits); review (the package, `QuarantineCause` named, `no_change`, sensitive groups with `SENSITIVE_PATTERNS` named, Import and Discard); Stop all, `officina stop --all` and Resume; memory proposals and the browser base; what the workshop leaves out (FR-9); the revoke advice (the question 10 entry); that a process running as the author can read the credential from the engine (R63); the roadmap's v0.6 row. It transcribes no list owned by code. Proof: `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_names_no_session_variable}`; the prose, and that no constant's members are listed, are reviewed by hand (a test over `SENSITIVE_PATTERNS` would fail on file names the README names for other reasons, such as `CLAUDE.md`).
3. **CHANGELOG** (D-226). `[Unreleased]` holds one coherent v0.6 set under Added, Changed and Security, built from the entries each story added. Proof: review.
4. **Use metrics** (PRD section 7 Use, Workshop share, Environment failures, Review cost, Acceptance; P-52, D-214, D-217, D-222). `officina-core::workshop::metrics` computes from journal lines, per UTC day and week: runs reaching a verdict and their templates and projects (Use), the workshop share (D-214), environment failures over runs (D-217), the median time from `review_opened` to a verdict and the share of packages needing a sensitive approval (Review cost), imported over decided per template (D-222); `crates/officina-host/examples/workshop_metrics.rs` prints them with `relay --json`'s journal reader. After real use, the research notes' use entry records each row against its target, which of section 2.3's templates the owner confirmed, and section 8 question 13's values; a value the facts contradict changes in the same commit as its constant, with the fact added to `epic-6-decisions.md`. Proof: core `crates/officina-core/src/workshop/metrics.rs::tests::{days_and_weeks_are_utc, no_change_counts_as_a_run_never_a_failure, review_cost_starts_at_review_opened, acceptance_counts_author_verdicts_only}`; manual, the use entry.
5. **Gates closed** (AD-126, D-201 to D-203, D-600). The Epic 6 entries are recorded, each under the next free number when it was written and named by its subject, both halves of each gate run in halves; for each failed gate the branch that shipped is named; question 11 is recorded if a page stated it. Proof: review of `docs/research-notes.md` against AD-126's table and D-202.
6. **Suite stable** (AD-125, D-224, D-225). CI runs Playwright with `--repeat-each=3`; the `mutants` job is green, each mutant compiling and failing on its named case; the steps running `cargo test -p officina-host --lib spawn` and `--lib workshop::engine` alone pass on both jobs; the `entry_linux` target reports its tests on `core-linux`; the `workshop` job's last run passed on `main`; the zero-write test covers a launch, a collection, a quarantine, a discard, an import, a memory run, `stop --all` and `workshop --json`. Proof: the CI jobs.
7. **QA closure** (D-227). `docs/qa/epic-6-report.md`: gate decision; evidence (the Epic 6 entries, the battery on CI and on the workstation, the injection sections, the mutants, the zero-write and credential tests, the use metrics); audits (security centred on principle 9: what the worker holds, the gateway, the credential's path, host git on worker-made objects, worker-named paths on the host, untrusted text; quality; test coverage against AD-125's rows, including FR-15's host family collected from stories 6.12 to 6.15); traceability from FR-0 to FR-18 and from AD-97 to AD-126 to the ACs that prove them; P-51's departures with their decisions; AD-106's reading of principle 2, the dropped hash-chained journal and P-56's two owner decisions (the relay-section hash, the extended sensitive seed), listed for the owner's confirmation; open findings moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC.

## Tasks

- [ ] Doctor section ordering, levels and notes; `doctor_workshop.rs` (AC 1)
- [ ] README "The workshop" made coherent; `readme.rs` cases (AC 2)
- [ ] CHANGELOG consolidation (AC 3)
- [ ] `workshop::metrics`, `examples/workshop_metrics.rs`; real use; the use entry (AC 4)
- [ ] Gates review (AC 5); CI stability (AC 6)
- [ ] Audits, `docs/qa/epic-6-report.md`, backlog (AC 7)

## Dev Notes

- Requirements: FR-18 as amended (amendment 14), PRD section 7 as amended (amendment 15), section 8 questions 11 to 13. Decisions: AD-123, AD-125, AD-126. Epic decisions: P-7 (Epic 2), P-16 (Epic 3), P-21 (Epic 4), P-25 (Epic 5); P-50 to P-56, D-201 to D-203, D-214, D-217, D-222, D-224 to D-228.
- Seams (D-200): doctor (1.8, 2.10, 5.19), `readme.rs` (2.10), the CI jobs.
- Waits for: stories 6.1 to 6.23. The real-use period runs on a build of `main` with all of them merged; the report waits for it (P-52). The journal keeps 90 days (AD-32).
- `workshop::metrics` is the executable form of section 7's "how measured": the numbers in the report come from it, never from a hand count.
- Cutting `[0.6.0]` and bumping the workspace version belong to the v0.6 release; not in this story, and no tag without the owner.
- No employer content in the README, the research notes or the report: placeholder projects, templates, hosts and paths.

## Testing

- Core unit: `crates/officina-core/src/workshop/metrics.rs`.
- CLI integration: `crates/officina/tests/{doctor_workshop,readme,quota_doctor}.rs`.
- CI: the full `windows` job, `core-linux`, `workshop`, `mutants`, Playwright with `--repeat-each=3`.
- Manual: the use entry, the gates review, the QA report.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: doctor's workshop checks as a `closed_enum!` with levels, families described instead of listed (F45); the Slice A and use entries reserved (F29); the QA report lists P-56's owner decisions and worker-named paths among the audits
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
