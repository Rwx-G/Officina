# Story 7.19: Doctor coherence, documentation and QA for the built slices

**Status:** Approved

## Story

As the author installing v0.7 with launches on, and as a future reader of the repository,
I want doctor to report the whole governor in one section, the README and changelog to explain v0.7 in one piece, every gate closed in both halves, and the epic's built slices closed with evidence,
so that v0.7 is set up without guessing, v0.8 can start on a closed epic, and the two-week measure runs on code that has passed its own checks (PRD section 6.1.1, P-62).

## Entry conditions

Implemented only once story 7.13's Slice B decision entry records **go** (D-331; the conditions are listed in story 7.14), after stories 7.14 to 7.18. When Slice B is not built, 7.13's report is the epic's report and this story stays unimplemented (P-62).

## Acceptance Criteria

1. **Doctor** (AD-147, D-341). The "Governor" section of story 7.13 gains Slice B's items in its stable order, each once: held packages and their age (7.15), the shortcut's chord and whether the window registered it, or that it is not available (7.17); story 7.11's mode and self-stop item now also reports the causes stories 7.15 and 7.16 write; the regime and each allowance's and ratio's source (story 7.9, review F31) stay in the section. `GovernorCheck::ALL` gains the Slice B checks. Levels follow the v0.1 exit rule. Proof: CLI `crates/officina/tests/doctor_governor.rs::{the_governor_section_lists_each_check_once, an_unregistered_shortcut_warns, held_packages_inform_with_their_age}` (the first iterates `GovernorCheck::ALL`).
2. **README** (D-340). "The governor (v0.7, in progress)" becomes one "The governor" section covering both slices: what the governor launches (approved standing orders only, in the workshop only), what it reads and never reads, the exposure sentence beside `[[standing_order]]`, eligibility and suspension, the quota sources and the regime built, the probe and its fallback, the reserve and the weekly cap, the ceiling and held packages, the review budget, the window and its end, the live guards, the modes, Stop autonomy and the shortcut against Stop all, the self-stops, the Automations table, `officina governor --json`, privacy, and that the governor never imports, merges or pushes; the roadmap's v0.7 row. It transcribes no list owned by code. Proof: `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_names_no_session_variable, readme_quotes_no_usage_degraded_marker}`; the prose is reviewed by hand.
3. **CHANGELOG** (D-340). `[Unreleased]` holds one coherent v0.7 set under Added, Changed and Security, built from the entries each story added. Proof: review.
4. **Gates closed** (AD-149, D-301 to D-303). the v0.7 gate entries are all recorded, both halves of gates 5 and 6 included; for each failed gate the branch that shipped is named. Proof: review of `docs/research-notes.md` against AD-149's table.
5. **Suite stable** (AD-148, D-148, D-305, D-326, D-338, D-355; review F2, F3, F10, F39). The local full gate replays CI's `windows` and `core-linux` jobs green (D-1201) with the `cargo tree -p officina -e normal,features -i officina-core` check and D-9's marker step, and with the lone-crate step that runs the without-feature proofs (`officina-core` and `officina-host` alone, then `officina-core` with `governor-launch`); the `mutants` job is green, each governor mutant, `mutant-governor-skip-hold` included, compiling and failing on its named case in each of its targets; the hotkey tests run on the local gate's `windows` leg, or with `--ignored` if that session refuses registration, recorded in the completion notes; Playwright runs with `--repeat-each=3`; the zero-write test covers `governor --json`, a probe, a merge pass, a shadow step and an autonomous launch to collection. Proof: the local gate's log, cited in the report.
6. **QA closure** (D-342). `docs/qa/epic-7-report.md`, citing `docs/qa/epic-7-slice-a-report.md`: gate decision; evidence (the v0.7 gate entries, Slice A's table, the injection battery's `governor` section, the mutants job, the shadow-inert and launch e2e, the hotkey tests, the R79 search); audits (security centred on principle 8 and the grant: what reaches `decide`, the launch path, the probe's containment and text, host git on the author's repository, the live guards; quality; test coverage against AD-148); traceability from FR-0 to FR-18 and from AD-127 to AD-149 to the ACs that prove them; every departure of P-61 with its decision; P-62's exception to P-7 and P-16, listed for the owner's confirmation; open findings moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC.
7. **The rework's traceability** (D-1214, D-1219, D-1220). The report traces PRD section 9's items to stories 7.22 and 7.23, D-1214's worktree reading to story 7.21, lists the decisions marked visible in the 2026-11-02 rework with the owner's answer or "stands", and the owner items of D-1220 with their state. Proof: review of `docs/qa/epic-7-report.md`.

## Tasks

- [ ] Doctor's Slice B items in the section; `doctor_governor.rs` (AC 1)
- [ ] README made one; `readme.rs` cases (AC 2)
- [ ] CHANGELOG consolidation (AC 3)
- [ ] Gates review (AC 4)
- [ ] CI stability and zero-write coverage (AC 5)
- [ ] Audits, `docs/qa/epic-7-report.md`, backlog (AC 6)

## Dev Notes

- Reworked 2026-11-02 (D-1201, D-1213, D-1218, D-1220): the suite is the local gate's; doctor names the floor's credential kind; v0.8 may already have started (D-1218). GitHub CI is off (D-1201): every CI job named here is that job replayed by the local full gate.
- Requirements: FR-18, PRD section 6.1.1. Decisions: AD-147, AD-148, AD-149. Epic decisions: P-7 (Epic 2), P-16 (Epic 3), P-21 (Epic 4), P-25 (Epic 5), P-52 (Epic 6); P-61, P-62, P-64, P-65, D-301 to D-303, D-305, D-326, D-338, D-340 to D-342, D-355.
- Dependencies: stories 7.1 to 7.18 and 7.21 to 7.23. This story closes the built slices: v0.8 may start when it is done, if it had not already (P-62). The two-week measure is story 7.20 and does not hold this one.
- F22 of the story review (appending Epic 7's block to `docs/stories/sprint-status.yaml`) is the coordinator's (P-65); this story checks it has been done before the QA report states the epic's status.
- Cutting a release and bumping the workspace version belong to the v0.7 release, after v0.6's; not in this story.
- No employer content in the README, the research notes or the report: placeholder projects, orders and paths.

## Testing

- CLI integration: `crates/officina/tests/{doctor_governor,readme,quota_doctor}.rs`.
- CI: the full `windows` and `core-linux` jobs, `mutants`, Playwright with `--repeat-each=3`.
- Manual: the gates review (AC 4), the QA report (AC 6).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the CI checks read the corrected tree command and the lone-crate step (F2, F3); the hold mutant joins the suite (F10); the regime and sources stay in doctor (F31); ignored hotkey tests run on the workstation (F39)
- 2026-11-02 reworked against v0.4 to v0.6 as built: the local gate (D-1201); helpers and worktrees traced (D-1214, D-1219, AC 7); either order with v0.8 (D-1218)
