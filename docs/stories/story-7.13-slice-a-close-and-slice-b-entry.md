# Story 7.13: Slice A close, the remaining gates and the Slice B entry record

**Status:** Approved

## Story

As the author who will let the governor launch only once it has shown, on my own days, that its conditions pass,
I want Slice A closed on its own table, doctor and the documentation made one, gates 0, 4 and 5 recorded, and every condition of section 6.1.1 written down with its source,
so that Slice B's launch code is written only when the evidence allows it, v0.8 starts without waiting otherwise, and the decision is mine on recorded facts (PRD section 6.1.1, section 7's Slice A rows, FR-0).

## Acceptance Criteria

1. **Doctor's Governor section** (AD-147 Slice A, D-341). Doctor groups the checks of stories 7.2, 7.3 and 7.6 to 7.11, all in `crates/officina-host/src/governor/doctor.rs`, in one "Governor" section in the order of `GovernorCheck::ALL`, a `closed_enum!` of its checks, each once (D-341 as amended by review F30, F33); levels follow the v0.1 exit rule (1 when any check fails): `<home>/usage/empty` not empty fails; issues, a paused probe, a back-off, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` set, no quota file or one without rate limits, an unknown offset and a self-stop warn; counts, ratios, the mode and the next window inform. Doctor runs no probe and no git job. Proof: CLI `crates/officina/tests/doctor_governor.rs::{the_governor_section_lists_each_check_once, warnings_alone_exit_0, a_non_empty_usage_folder_exits_1, doctor_spawns_no_probe_and_no_git}` (the first iterates `GovernorCheck::ALL`, never a typed list; the last counts the fake claude's and a git shim's calls).
2. **README and CHANGELOG for Slice A** (D-340). The README additions of stories 7.2 to 7.12 become one coherent "The governor (v0.7, in progress)" section, saying that launches wait for Slice B: the keys and the approval, the exposure sentence beside `[[standing_order]]`, merged detection and its corrections, settlement and eligibility, quota observations, the probe and its fallback, the projection, the reserve and the weekly cap, the review budget, the decision and its cause families, the shadow, the Governor section and `officina governor --json`, privacy, the upgrade note, the window in local time (or UTC, D-303). It transcribes no list owned by code. `CHANGELOG.md` `[Unreleased]` holds one coherent Slice A set. Proof: `crates/officina/tests/readme.rs::{readme_mentions_every_subcommand, readme_names_no_session_variable, readme_quotes_no_usage_degraded_marker}` (the last iterates `USAGE_DEGRADED_MARKERS` and fails when an entry appears); the prose is reviewed by hand.
3. **Slice A's table** (PRD section 7, Slice A rows, D-331). `governor::evidence::slice_a_table(lines, rows, days, now_ms, offset)` computes Would launch (would-launch decisions over 5 shadow working days: a day listed in `days` on which the window ran with the governor in shadow for at least half of that day's autonomy window, read from the hold and would-launch lines and their heartbeats; D-353, review F37), Hold causes (shares failing first and failing at all, a cause over 80 % named), Ceiling room, Merge detection (settled by rule over imports read, author corrections), Probe (structural failures over probes, back-offs, cost per probe, differences above 5 points between a probe and a fresh file) and Shadow inert (governor-sourced work orders and container creations in shadow, from the registry and the journal); `crates/officina-host/examples/governor_metrics.rs` prints it from the real home. After 5 working days of shadow on a build of `main` holding stories 7.1 to 7.12, the agent runs it and records the table in a research notes entry. Proof: core `crates/officina-core/src/governor/evidence.rs::tests::{would_launch_counts_listed_working_days_only, a_day_with_the_window_open_under_half_its_autonomy_window_is_not_a_shadow_day, a_cause_over_80_percent_is_named, the_probe_row_counts_structural_failures_over_probes}`; manual, the research notes entry.
4. **E64, gate 0** (AD-149 row 0, PRD question 0, D-301, D-303; owner). The owner records the text of S5's sentence and the plan's own terms as he reads them on that date; for each confirmed candidate of PRD section 2.3, its floor and account and whether that account may run that project's work (on an employer plan, the employer's written answer, quoted); on an api-key floor, his weekly USD figure for autonomous runs. Pass: recorded, at least one candidate allowed. Proof: research notes entry E64.
5. **E65, gate 4** (AD-149 row 4 as D-347 reads it, PRD question 4, D-303; owner; review F11). A workshop template with `max_minutes = 60` and a prompt that commits once, then runs a 10-minute sleep through Bash; once the commit shows in the run's stream, the owner stops the worker container with the engine call v0.6's per-run stop issues (`docker stop -t 3 officina-worker-<id>`, the line the agent prepares), and the window collects it as after any stop. With `max_minutes = 5`, the entry point's own `timeout` would end `claude` before the window's stop acts, so the gate would test the entry point instead of the stop AD-140's window-end path uses. Pass: a package in state `review` holding the commit. Proof: research notes entry E65, naming D-303's branch for story 7.16.
6. **E66, gate 5, first half** (AD-149 row 5, D-302; agent prepares, owner presses). A scratch program, never committed, registers `Ctrl+Alt+Shift+X` with `MOD_NOREPEAT` on a dedicated thread with its own message loop and prints on `WM_HOTKEY`; the owner presses the chord with the console minimized, then behind a full-screen editor. Pass: registered, and fires in both cases. Proof: research notes entry E66 (first half).
7. **Questions 7, 8 and 13, and section 2.3** (PRD section 6.1.1, section 8, D-331; owner and agent). Recorded with their sources: question 7, the templates with at least 10 settled runs and their acceptance, read from `officina governor --json`; question 8, the owner's decision on the shadow's Hold causes row (keep the framing's condition, or `fresh_window`, which reopens FR-4 and FR-10 with him, D-331); the share of window minutes under the ceiling, and, under 20 %, the owner's answer to question 13; for each confirmed candidate, the routine-marked subjects it covers or that none do, which the owner writes into PRD section 2.3. Proof: research notes entries; the PRD diff for section 2.3 by the owner.
8. **The Slice B decision** (PRD section 6.1.1, P-62, D-331). One research notes entry states each condition of section 6.1.1 with the entry that records it and the outcome: **go** (every condition holds), **wait** (the shadow shows fewer than 3 would-launch decisions, no template is eligible yet, or question 8 reopened an FR: v0.8 starts, and this entry says what must change), or **not built** (E64 allows no candidate and no API key exists, or E62 shows no rise on `oauth-token`, or an api-key floor has no enforced per-run cap because v0.6's gate 6 failed, D-343). With fewer than 3 would-launch decisions, the dominant hold cause names the starting value revisited before any launch code; a floor key changes by the owner's edit and approval, a code constant in the same commit as its vector cases, with the fact added to `epic-7-decisions.md`. Proof: the research notes entry; in the change case, the vector cases.
9. **Suite stable for Slice A** (AD-148, D-148, D-338). CI's `windows` and `core-linux` jobs pass; the `mutants` job is green, each governor mutant compiling and failing on its named case; Playwright runs `governor.spec.ts` with `--repeat-each=3`; the zero-write test covers `governor --json`, a probe, a merge pass and a shadow step. Proof: the CI jobs.
10. **QA, Slice A** (D-342). `docs/qa/epic-7-slice-a-report.md`: gate decision; evidence (E60 to E66 as recorded, Slice A's table, the vector coverage, the mutants job, the shadow-inert e2e, the R79 search); audits (security centred on principle 8: what reaches `decide`, the probe's containment and its text, host git on the author's repository; quality; coverage against AD-148); traceability from Slice A's FRs (FR-0 to FR-6, FR-10, FR-11 in shadow, FR-17 and FR-18 without launch controls) and AD-127 to AD-138, AD-142 to AD-145, AD-147 to AD-149 to the ACs that prove them; the departures of P-61 so far; open findings moved to `docs/backlog.md`. When the outcome is not built, the report says it is the epic's report. Proof: review of the report.

## Tasks

- [ ] Doctor's section ordering and levels; `doctor_governor.rs` (AC 1)
- [ ] README made coherent; `readme.rs` cases; CHANGELOG (AC 2)
- [ ] `evidence::slice_a_table`, `examples/governor_metrics.rs`; 5 working days of shadow; the research notes entry (AC 3)
- [ ] Gate 0's checklist for the owner; E64 (AC 4)
- [ ] Gate 4's template and prompt for the owner; E65 (AC 5)
- [ ] Scratch hotkey program; E66's first half with the owner (AC 6)
- [ ] Questions 7, 8, 13 and section 2.3 with the owner (AC 7)
- [ ] The Slice B decision entry (AC 8)
- [ ] CI stability (AC 9)
- [ ] Audits, `docs/qa/epic-7-slice-a-report.md`, backlog (AC 10)

## Dev Notes

- Requirements: FR-0, PRD section 6.1.1, section 7's Slice A table, section 8 questions 0, 4, 5, 7, 8 and 13. Decisions: AD-147, AD-148, AD-149, Slices. Epic decisions: P-7 (Epic 2), P-16 (Epic 3), P-21 (Epic 4), P-25 (Epic 5), P-52 (Epic 6); P-61, P-62, P-64, P-65, D-301 to D-304, D-331, D-338, D-340 to D-343, D-347, D-353.
- Dependencies: stories 7.1 to 7.12 merged; 5 working days of shadow on the author's `days` with the governor in shadow on a build of `main`.
- `governor::evidence::slice_a_table` is the executable form of section 7's "How measured": the numbers in the notes and the report come from it, never from a hand count.
- This story is Slice 0's second half: it holds the gates FR-0 places before Slice B, and the record every Slice B story checks before it starts (D-331). It writes no launch code.
- Epic 7's research-note entries are E60 to E79 (D-301); this story writes E64 to E66 and the Slice A entries from E67.
- Gate 0 is the owner's record about terms and accounts; the agent prepares the checklist and quotes nothing it has not been given. No employer content: an employer's answer is quoted only as the owner provides it, redacted of names.
- The scratch hotkey program needs `windows-sys` with `Win32_UI_Input_KeyboardAndMouse` in its own scratch manifest; the workspace gains the feature in story 7.17.
- If the outcome is wait, v0.8 starts (P-62); this story is then done, and the Slice B stories start when a later entry records go.

## Testing

- Core unit: `crates/officina-core/src/governor/evidence.rs`.
- CLI integration: `crates/officina/tests/{doctor_governor,readme,quota_doctor}.rs`.
- CI: the full `windows` and `core-linux` jobs, `mutants`, Playwright with `--repeat-each=3`.
- Manual: research notes entries (AC 3 to 8), the QA report (AC 10).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: gates renumbered into Epic 7's range E60 to E79 (F7, superseded by the owner's range); gate 4 stops the worker container from outside, after the commit (F11); doctor's section iterates `GovernorCheck::ALL` from `governor/doctor.rs` (F30, F33); a shadow working day defined (F37); an api-key floor without an enforced cap is not built (F1)
