# Story 5.19: Doctor coherence, documentation, use metrics and QA closure

**Status:** In Progress (ACs 1 to 3, 8 and 9's report done; AC 4's code done; ACs 4 to 7 wait on the owner's real use and runs)

## Story

As the author installing v0.5 on a workstation, and as a future reader of the repository,
I want doctor to report the orchestrator's setup in one section, the README and changelog to explain v0.5 in one piece, the use metrics read after real use, and the epic closed with evidence,
so that v0.5 is set up without guessing, I know whether it saves me attention, and nothing of the PRD is left deferred.

## Acceptance Criteria

1. **Doctor** (AD-94, FR-15 as amended by amendment 9). Doctor groups the checks of stories 5.2, 5.4, 5.7, 5.11 to 5.14, 5.16, 5.17 and 5.20 (the permission line, and the line pointing at `--permissions`, D-574) in one "Orchestrator" section, in a stable order, each once; failures follow the v0.1 exit rule (1 when any check fails): a `claude` that is not an `.exe`, a non-empty orchestrator workdir and a missing `officina.exe` fail; agent view off, a `claude` below 2.1.285, template issues, templates awaiting approval, `review_config_invalid`, a stranger under `<home>/orchestrator/` (D-166) and stale `.tmp` files warn; counts and states inform. Proof: CLI `crates/officina/tests/doctor_orchestrator.rs::{the_orchestrator_section_lists_each_check_once, warnings_alone_exit_0, a_shim_exits_1, a_non_empty_workdir_exits_1}`.
2. **README** (D-149, D-127). The sections the stories added become one "The orchestrator" section: the briefing; templates, the composer, the card and pointer-only Launch, and what a work order session may do without a prompt (E22's outcome); the meeting, the deliveries described as families with the `Delivery` constant named, never its table copied, that an answer sits on a process command line while its console runs and only its hash is journaled; `auto`: blind shadow, what the review reads (the author's last prompt, never the session's answer), the mandatory check, the causes that escalate without a call described as families with `EscalationCause` named, never listed (D-149, review F44), the gate and its stated weakness, the rule offer, "should not have run"; permissions (PRD section 9 items 6, 7 and 9, D-577): who decides what, as tiers (Claude Code's own rules and reads; Claude Code's auto mode for free-form commands once the author switches, his decision; the relay for catalogue actions; the author for everything section 9 item 9 names), that classing an outward action `auto` is the author's decision under the check, the gate and the cap (D-583), that no Officina model judges a Claude Code permission prompt, gate 8's outcome and `officina doctor --permissions`; the orchestrator: what it can and cannot do, the first-start trust, never resumed, its place at the decider's desk, the briefing walk and the view switch's mark; answer proposals and their pointer-only Send; `officina mcp`; privacy (what is stored, for how long, `no_excerpts`); the upgrade note on the catalogue format; the roadmap's v0.5 row. It transcribes no list owned by code. Proof: `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_names_no_session_variable}` (the second iterates `SESSION_ENV_REMOVED` and fails when one appears); the prose is reviewed by hand.
3. **CHANGELOG** (D-149). `[Unreleased]` holds one coherent v0.5 set under Added, Changed and Security, built from the entries each story added. Proof: review.
4. **Use metrics** (PRD section 7 Use, Auto and Delivery share rows, P-25, D-139, D-142). `officina-core::orders::metrics::per_day(journal lines)` counts, per UTC day, work orders started, answers sent and answers confirmed (each split by `meeting_answer.source`, so answer proposals are counted apart, PRD section 9 item 3, story 5.21) and the author's `human` and `auto` decisions; `crates/officina-host/examples/use_metrics.rs` prints them from the journal with `relay --json`'s reader. After real use, research notes record: Use (the working days with at least one work order started or one answer sent, against 10), Auto (decisions per day in the week before Slice B shipped and in the last week of shadow), Delivery share (confirmed over sent, per source). Proof: core `crates/officina-core/src/orders/metrics.rs::tests::{days_are_utc, author_decisions_exclude_review_and_rule, delivery_share_counts_copy_and_attach_in_the_denominator, proposals_are_counted_apart_from_the_authors_answers}`; manual, the research notes entry.
5. **Question 8's values** (PRD section 8 question 8, P-25). Over the shadow period and two weeks of use, research notes record: the highest approvals per hour per `auto` action, the 5-hour use at each `quota_high` escalation, the shadow cases and disagreements, and `rule_diff`; a value that the facts contradict changes in the same commit as its constant, with the fact added to `epic-5-decisions.md`. Proof: the research notes entry; in the change case, the vector cases of `auto.json`.
6. **Question 6** (P-25). The documented surfaces for delivering text to a live interactive session are checked again at the Claude Code version the canary last saw; recorded; nothing changes unless a documented author-input surface exists, which then becomes a new PRD question for the owner. Proof: the research notes entry.
7. **Gates closed** (AD-96, D-125 to D-127, D-164, D-560, D-572). The session-variables entry, E19 to E25 and the gate 8 entry are all recorded, both halves of gates 1, 4 and 7 included; for each failed gate, the branch that shipped is named, and for E22 the README states the limit and the owner amended PRD FR-3. Proof: review of `docs/research-notes.md` against AD-96's table.
8. **Suite stable** (AD-95, D-91, D-148). CI runs Playwright with `--repeat-each=3`; the ignored release tests (`proposals_crash.rs`, `registry_crash.rs`, `perf.rs`) pass; the `mutants` job is green, each mutant compiling and failing on its named case (D-148); the step running `cargo test -p officina-host --lib spawn` alone passes on both jobs (D-128); the zero-write test covers a launch, an answer, a review, `officina mcp` and an orchestrator start. Proof: the CI `windows` and `mutants` jobs.
9. **QA closure** (D-150). `docs/qa/epic-5-report.md`: gate decision; evidence table (the session-variables entry, E19 to E25, the gate 8 entry, the injection battery's sections, the mutants job, the restart e2e, the use metrics); audits (security centred on principle 8, untrusted text, the spawn environment and the containment of the orchestrator, the review and the triage call; quality; test coverage against AD-95); traceability from FR-0 to FR-15, PRD section 9's items, AD-73 to AD-96 and AD-179 to AD-182 to the ACs that prove them (D-577's map); Slice D's entry state (D-578: which of SD-1 to SD-7 are recorded); the departures of P-24 with their decisions; FR-10's reading of principle 2 and the decisions marked "(to confirm)" in `epic-5-decisions.md` (D-561, D-565, D-574 and those after), listed for the owner's confirmation; open findings moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC.

## Tasks

- [x] Doctor section ordering and levels; `doctor_orchestrator.rs` (AC 1)
- [x] README "The orchestrator" made coherent; `readme.rs` cases (AC 2)
- [x] CHANGELOG consolidation (AC 3)
- [x] `orders::metrics`, `examples/use_metrics.rs` (AC 4)
- [ ] Real use; research notes entry for Use, Auto and Delivery share (AC 4, the owner's)
- [ ] Question 8's measurements; question 6's recheck (AC 5, 6, the owner's)
- [x] Gates review (AC 7): the agent's review in the QA report; the owner's halves open
- [x] CI stability and zero-write coverage (AC 8)
- [x] Audits, `docs/qa/epic-5-report.md`, backlog (AC 9); the epic's closure waits on the owner's items

## Dev Notes

- Requirements: FR-15, PRD section 7, section 8 questions 6 and 8. Decisions: AD-94, AD-95, AD-96. Epic decisions: P-7 (Epic 2), P-16 (Epic 3), P-21 (Epic 4); P-24, P-25, D-125 to D-128, D-139, D-142, D-148 to D-150, D-164, D-166.
- Dependencies: stories 5.1 to 5.18, 5.20 and 5.21; Slice D (5.22, 5.23) comes after this story and is not read by it, except AC 9's entry state. The real-use period runs on a build of `main` with all of them merged; the report waits for it (P-25). The Auto metric's first week is read back from the journal, kept 90 days (AD-32).
- `orders::metrics` is the executable form of section 7's "how measured": the numbers in the report come from it, never from a hand count.
- Cutting `[0.5.0]` and bumping the workspace version belong to the v0.5 release, after `[0.4.0]`; not in this story.
- No employer content in the README, the research notes or the report: placeholder projects, templates and paths.

## Testing

- Core unit: `crates/officina-core/src/orders/metrics.rs`.
- CLI integration: `crates/officina/tests/{doctor_orchestrator,readme,quota_doctor}.rs`.
- CI: the full `windows` job, release tests, Playwright with `--repeat-each=3`, `mutants`.
- Manual: research notes entries (AC 4 to 7), the QA report (AC 9).

## Dev Agent Record

### Debug Log

- Doctor printed no section headings: the registry section (story 3.12) is a run of lines in one order. The Orchestrator section follows it, printed last; the `claude` line stays first, since every section reads it (D-797).
- `review_config_invalid` failed doctor since story 5.13 (its AC 12 test asserted `Fail`); AC 1 lists it among the warnings, and every `auto` request then goes to the author, so it now warns and the test says so (D-797).
- Template issues were `relay issue` lines: a template's issue is now titled `template issue` and moves to the section by its title, and a `templates` line gives the approved count and what awaits approval; the relay approval line keeps the whole pending change.
- `readme_names_no_session_variable` found `CLAUDE_PROJECT_DIR` in the `officina mcp` paragraph; it now says "the project folder Claude Code gives it".
- The security audit's High on the meeting's filter lookup was reproduced first: with the old `--local` lookup, the new include case ran the marker program (`filter driver through include.path: the meeting ran the marker program`); with `--includes --show-scope` all seven `tests/git.rs` cases pass (D-686).
- The use metrics reader, run on this workstation's own Officina home, counted no day: the installed window is v0.4's, so there is no real-use data yet (AC 4 stays open).
- Gate runs: three full Windows runs each stopped on one timing case under the workspace run's load, while other agents' builds and a Docker build ran on the machine: `injection.rs::{card_launch_cases, proposal_file_cases}` (a collector's first board not within 10 s), `board.rs::hanging_claude_times_out_instead_of_blocking` (its elapsed bound), `spawn.rs::a_work_order_deadline_ends_the_child_alone`; each passed alone at once. None touches this story's code. The fourth run took the workspace tests with `--no-fail-fast` (the same commands otherwise) so every step ran.

### Completion Notes

Decisions D-686 to D-688 and D-797 to D-799 (`epic-5-decisions.md`); to confirm: D-687 (the session binding of the review's input before auto approval is turned on) and D-797 (the `claude` line outside the section, no printed heading).

- **AC 1, done.** `crates/officina/tests/doctor_orchestrator.rs::{the_orchestrator_section_lists_each_check_once, warnings_alone_exit_0, a_shim_exits_1, a_non_empty_workdir_exits_1}` (a home where every orchestrator line that can warn warns at once: agent view off in doctor's environment, a fake `claude` below `RESUME_PROMPT_MIN_VERSION`, a stranger under the orchestrator's folder, a stale `.tmp` proposal file, and on Windows a template issue, templates awaiting approval and a failing `[auto_review]`; run plain and with `--permissions`); host `doctor.rs::tests::run_reports_every_check_in_order` (the section last), `relay/doctor.rs::tests::{templates_awaiting_approval_are_listed, an_invalid_auto_review_table_is_reported, template_issues_are_listed_with_their_lines}`.
- **AC 2, done; the prose is the owner's to review.** `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_names_no_session_variable}` and the existing README cases. "Who decides what" states the tiers and D-583; "What v0.5 stores, and for how long" the privacy; "Checking the setup" doctor's section; the deliveries and the escalation causes are families naming `Delivery` and `EscalationCause` (D-798).
- **AC 3, done; review.** `[Unreleased]` Added opens v0.5 with one entry and follows the README's order; Changed and Security keep v0.5's entries together; the meeting's filter fix is under Security.
- **AC 4, code done; real use open (the owner's).** `crates/officina-core/src/orders/metrics.rs::tests::{days_are_utc, author_decisions_exclude_review_and_rule, delivery_share_counts_copy_and_attach_in_the_denominator, proposals_are_counted_apart_from_the_authors_answers}`; `crates/officina-host/examples/use_metrics.rs` (D-688). After real use: `cargo run -p officina-host --example use_metrics`, then the research notes entry for Use, Auto and Delivery share.
- **AC 5, open (the owner's).** Over the shadow period and two weeks of use: the highest approvals per hour per `auto` action, the 5-hour use at each `quota_high` escalation, the shadow cases and disagreements and `rule_diff` (doctor's `relay auto review` line, the Relay view's Auto approval section).
- **AC 6, open (the owner's, P-25).** At the version the canary last saw (doctor's `claude` line), Claude Code's documented surfaces for text to a live interactive session, recorded in the research notes.
- **AC 7, reviewed; open on the owner's halves.** E84, E19, E20, E21, E23, E25 and E85 (print mode) recorded and passed; E22's agent half and E24's agent and owner halves recorded; open: E22's owner and window halves, E23's owner readability check, E24's window half, E85's optional interactive half. No gate failed, so no failed branch shipped; E22's branch (the supervisor's environment) is stated in the README.
- **AC 8, done.** The full local gate (below): Playwright `--repeat-each=3`, the release tests (`proposals_crash.rs`, `registry_crash.rs`, `perf.rs`), the mutants step, the no-feature `cargo test -p officina-host --lib spawn` on both scripts; the zero-write test covers a launch, an answer, a review, `officina mcp` and an orchestrator start.
- **AC 9, report written; the closure waits.** `docs/qa/epic-5-report.md` (D-799): gate decision open, evidence, both audits with their outcomes, traceability over FR-0 to FR-15, PRD section 9 and the ADs, Slice D's entry state, P-24's departures, the side fixes, the owner's items with their one-liners and the decisions to confirm; open findings in `docs/backlog.md`. Audit fix: the meeting's filter lookup (D-686), `crates/officina-host/src/git.rs::tests::only_the_repositorys_own_filters_count` and the two new cases of `crates/officina-host/tests/git.rs::no_configured_program_runs_on_receive`.

Gate, on `19bc2fd`'s code (the later commits change documentation only): fmt and clippy clean; workspace 1,642 passed, 1 failed (`spawn.rs::a_work_order_timeout_kills_only_claude`, a timing case; `spawn.rs` passed alone, 15 of 15), 14 ignored; the `--lib spawn` step 7 passed; perf 5 passed (hook own cost 13.3 ms, gate own cost 3.0 ms); lexer, approvals stress, registry and proposals crash, markers, dependencies and roots pass; svelte-check and ESLint clean, Vitest 350 passed; Playwright `--repeat-each=3` 1,326 passed, 6 skipped (baselines, D-508, D-840); the three mutants caught, control run green; `cargo deny` ok, `pnpm audit` 1 low ignored; `core-linux`: fmt and clippy clean, 1,462 passed, 0 failed, 12 ignored, `--lib spawn` green (a first run stopped on `spawn.rs::every_kind_reaches_the_fake_claude_as_its_golden_vector`, "Text file busy" on a freshly copied fake, the race the stability work's linked fake removes).

No new dependency and no new feature (Cargo or npm).

## File List

- `crates/officina-core/src/orders/metrics.rs` (new), `src/orders/mod.rs`
- `crates/officina-host/examples/use_metrics.rs` (new)
- `crates/officina-host/src/doctor.rs`, `src/relay/doctor.rs`, `src/git.rs`, `tests/git.rs`
- `crates/officina/tests/doctor_orchestrator.rs` (new), `tests/readme.rs`
- `README.md`, `CHANGELOG.md`
- `docs/qa/epic-5-report.md` (new), `docs/backlog.md`, `docs/stories/epic-5-decisions.md`, `docs/stories/sprint-status.yaml`, this file

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: README describes deliveries and causes as families naming their constants (F44); E25, gate 4's window half, the no-feature spawn step and the compile-first mutants in the closure checks (F5, F22, F23, F25); doctor's executable rule and stranger warning (F27, F28)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: the session-variables and gate 8 entries named by subject (D-560); doctor's permission line, the permission tiers in the README, answer proposals, the decision office and the view switch's mark (D-565, D-574, D-577); traceability over section 9 and AD-179 to AD-182; Slice D's entry state
- 2026-10-28 after the rework's adversarial pass: the README says classing an outward action `auto` is the author's decision under its guards (D-583, rework F4)
- 2026-10-29 developer agent: doctor's Orchestrator section, the use metrics and their reader, the README and changelog made one, both audits with the meeting's filter fix, the QA report; ACs 4 to 7 wait on the owner's real use and runs
