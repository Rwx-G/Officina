# Story 5.19: Doctor coherence, documentation, use metrics and QA closure

**Status:** Approved

## Story

As the author installing v0.5 on a workstation, and as a future reader of the repository,
I want doctor to report the orchestrator's setup in one section, the README and changelog to explain v0.5 in one piece, the use metrics read after real use, and the epic closed with evidence,
so that v0.5 is set up without guessing, I know whether it saves me attention, and nothing of the PRD is left deferred.

## Acceptance Criteria

1. **Doctor** (AD-94, FR-15 as amended by amendment 9). Doctor groups the checks of stories 5.2, 5.4, 5.7, 5.11 to 5.14, 5.16, 5.17 and 5.20 (the permission line, and the line pointing at `--permissions`, D-574) in one "Orchestrator" section, in a stable order, each once; failures follow the v0.1 exit rule (1 when any check fails): a `claude` that is not an `.exe`, a non-empty orchestrator workdir and a missing `officina.exe` fail; agent view off, a `claude` below 2.1.285, template issues, templates awaiting approval, `review_config_invalid`, a stranger under `<home>/orchestrator/` (D-166) and stale `.tmp` files warn; counts and states inform. Proof: CLI `crates/officina/tests/doctor_orchestrator.rs::{the_orchestrator_section_lists_each_check_once, warnings_alone_exit_0, a_shim_exits_1, a_non_empty_workdir_exits_1}`.
2. **README** (D-149, D-127). The sections the stories added become one "The orchestrator" section: the briefing; templates, the composer, the card and pointer-only Launch, and what a work order session may do without a prompt (E22's outcome); the meeting, the deliveries described as families with the `Delivery` constant named, never its table copied, that an answer sits on a process command line while its console runs and only its hash is journaled; `auto`: blind shadow, what the review reads (the author's last prompt, never the session's answer), the mandatory check, the causes that escalate without a call described as families with `EscalationCause` named, never listed (D-149, review F44), the gate and its stated weakness, the rule offer, "should not have run"; permissions (PRD section 9 items 6, 7 and 9, D-577): who decides what, as tiers (Claude Code's own rules and reads; Claude Code's auto mode for free-form commands once the author switches, his decision; the relay for catalogue actions; the author for everything section 9 item 9 names), that no Officina model judges a Claude Code permission prompt, gate 8's outcome and `officina doctor --permissions`; the orchestrator: what it can and cannot do, the first-start trust, never resumed, its place at the decider's desk, the briefing walk and the view switch's mark; answer proposals and their pointer-only Send; `officina mcp`; privacy (what is stored, for how long, `no_excerpts`); the upgrade note on the catalogue format; the roadmap's v0.5 row. It transcribes no list owned by code. Proof: `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_names_no_session_variable}` (the second iterates `SESSION_ENV_REMOVED` and fails when one appears); the prose is reviewed by hand.
3. **CHANGELOG** (D-149). `[Unreleased]` holds one coherent v0.5 set under Added, Changed and Security, built from the entries each story added. Proof: review.
4. **Use metrics** (PRD section 7 Use, Auto and Delivery share rows, P-25, D-139, D-142). `officina-core::orders::metrics::per_day(journal lines)` counts, per UTC day, work orders started, answers sent, answers confirmed and the author's `human` and `auto` decisions; `crates/officina-host/examples/use_metrics.rs` prints them from the journal with `relay --json`'s reader. After real use, research notes record: Use (the working days with at least one work order started or one answer sent, against 10), Auto (decisions per day in the week before Slice B shipped and in the last week of shadow), Delivery share (confirmed over sent). Proof: core `crates/officina-core/src/orders/metrics.rs::tests::{days_are_utc, author_decisions_exclude_review_and_rule, delivery_share_counts_copy_and_attach_in_the_denominator}`; manual, the research notes entry.
5. **Question 8's values** (PRD section 8 question 8, P-25). Over the shadow period and two weeks of use, research notes record: the highest approvals per hour per `auto` action, the 5-hour use at each `quota_high` escalation, the shadow cases and disagreements, and `rule_diff`; a value that the facts contradict changes in the same commit as its constant, with the fact added to `epic-5-decisions.md`. Proof: the research notes entry; in the change case, the vector cases of `auto.json`.
6. **Question 6** (P-25). The documented surfaces for delivering text to a live interactive session are checked again at the Claude Code version the canary last saw; recorded; nothing changes unless a documented author-input surface exists, which then becomes a new PRD question for the owner. Proof: the research notes entry.
7. **Gates closed** (AD-96, D-125 to D-127, D-164, D-560, D-572). The session-variables entry, E19 to E25 and the gate 8 entry are all recorded, both halves of gates 1, 4 and 7 included; for each failed gate, the branch that shipped is named, and for E22 the README states the limit and the owner amended PRD FR-3. Proof: review of `docs/research-notes.md` against AD-96's table.
8. **Suite stable** (AD-95, D-91, D-148). CI runs Playwright with `--repeat-each=3`; the ignored release tests (`proposals_crash.rs`, `registry_crash.rs`, `perf.rs`) pass; the `mutants` job is green, each mutant compiling and failing on its named case (D-148); the step running `cargo test -p officina-host --lib spawn` alone passes on both jobs (D-128); the zero-write test covers a launch, an answer, a review, `officina mcp` and an orchestrator start. Proof: the CI `windows` and `mutants` jobs.
9. **QA closure** (D-150). `docs/qa/epic-5-report.md`: gate decision; evidence table (the session-variables entry, E19 to E25, the gate 8 entry, the injection battery's sections, the mutants job, the restart e2e, the use metrics); audits (security centred on principle 8, untrusted text, the spawn environment and the containment of the orchestrator, the review and the triage call; quality; test coverage against AD-95); traceability from FR-0 to FR-15, PRD section 9's items, AD-73 to AD-96 and AD-179 to AD-182 to the ACs that prove them (D-577's map); Slice D's entry state (D-578: which of SD-1 to SD-7 are recorded); the departures of P-24 with their decisions; FR-10's reading of principle 2 and the decisions marked "(to confirm)" in `epic-5-decisions.md` (D-561, D-565, D-574 and those after), listed for the owner's confirmation; open findings moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC.

## Tasks

- [ ] Doctor section ordering and levels; `doctor_orchestrator.rs` (AC 1)
- [ ] README "The orchestrator" made coherent; `readme.rs` cases (AC 2)
- [ ] CHANGELOG consolidation (AC 3)
- [ ] `orders::metrics`, `examples/use_metrics.rs`; real use; research notes (AC 4)
- [ ] Question 8's measurements; question 6's recheck (AC 5, 6)
- [ ] Gates review (AC 7)
- [ ] CI stability and zero-write coverage (AC 8)
- [ ] Audits, `docs/qa/epic-5-report.md`, backlog (AC 9)

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

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: README describes deliveries and causes as families naming their constants (F44); E25, gate 4's window half, the no-feature spawn step and the compile-first mutants in the closure checks (F5, F22, F23, F25); doctor's executable rule and stranger warning (F27, F28)
- 2026-10-28 checked against the addendum and v0.3 and v0.4 as built: the session-variables and gate 8 entries named by subject (D-560); doctor's permission line, the permission tiers in the README, answer proposals, the decision office and the view switch's mark (D-565, D-574, D-577); traceability over section 9 and AD-179 to AD-182; Slice D's entry state
