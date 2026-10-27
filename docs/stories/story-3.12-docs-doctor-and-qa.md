# Story 3.12: Doctor coherence, documentation, workstation checks and QA closure

**Status:** Approved

## Story

As the author installing v0.3 on a workstation, and as a future reader of the repository,
I want doctor to report the registry and the triage model in one coherent section, the README and changelog to explain v0.3 in one piece, and the facts still unverified measured on the workstation,
so that v0.3 is set up without guessing, the epic closes with evidence, and nothing of the PRD is left deferred.

## Acceptance Criteria

1. Doctor groups the registry checks of stories 3.1, 3.2, 3.8 and 3.9 in one section, in a stable order, each once; failures follow the v0.1 exit rule (1 when any check fails): a network registry and a failed `quick_check` fail; a synced folder, a newer schema, a quarantine, waiting event files, missing hook fields and a paused model warn. Proof: CLI `crates/officina/tests/doctor_registry.rs::{registry_section_lists_each_check_once, warnings_alone_exit_0, a_failed_quick_check_exits_1}` (a planted marker for the synced warning, a damaged database file for the failure); the network case stays proved by the host unit test of story 3.1, since the binary has no seam to fake a network folder.
2. README (D-73): the sections the stories added become one "Subjects and triage" section: what the registry is and where it lives, the sync exclusion, subjects and filing, the Subjects view, triage and its keys, the evidence and the gates, the model off by default and contained, auto-archive and routine candidates, and the privacy paragraph (what is kept, for how long, `no_excerpts`); it transcribes no list owned by code. Proof: `crates/officina/tests/readme.rs::readme_mentions_every_subcommand` (story 2.10, or created here per D-44) and `readme.rs::readme_names_no_sync_marker`, which iterates `SYNC_ROOT_MARKERS` and fails when one appears in the README; the prose is reviewed by hand.
3. `CHANGELOG.md` `[Unreleased]` holds one coherent v0.3 set of entries under Added, Changed and Security, built from the entries each story added. Proof: review.
4. Workstation checks recorded in `docs/research-notes.md`, each with the Claude Code version (AD-58 manual list, PRD section 8 as amended by amendment 11, D-52): the `SessionEnd.reason` sent when a VS Code terminal hosting an interactive session is closed; the gap between `SessionEnd` and `SessionStart` for `/clear` and for `/resume` in an interactive session, with `SUPERSESSION_WINDOW_MS` revisited if the gap does not fit; WebView2 rendering of the Subjects view, the Triage panel and the evidence compared with Playwright Chromium by eye. Proof: manual, research notes entries.
5. A week of real use (AD-58 manual list, PRD section 7): no session, subject or verdict lost across window restarts and a closed weekend; filing by rule checked against the author's moves; one confirmation per finished subject; doctor clean apart from expected warnings. Proof: manual, a research notes entry with the counts observed.
6. The full automated suite is stable: Playwright passes three runs in a row; the ignored release tests (`perf.rs`, `registry_crash.rs`) pass; the zero-write test covers every v0.3 command and the triage call. Proof: CI `windows` job and the three local Playwright runs, recorded in Completion Notes.
7. QA closure (D-74): `docs/qa/epic-3-report.md` with the gate decision, the evidence table, the audits (security, focused on triage containment, R18 and the registry's text bounds; quality; test coverage against AD-58), the traceability table from every FR and every AD from AD-38 to AD-58 to the ACs that prove it, and open items moved to `docs/backlog.md`. Proof: review of the report; no FR or AD row without a proving AC.

## Tasks

- [ ] Doctor registry section ordering and level rules; `doctor_registry.rs` cases (AC 1)
- [ ] README "Subjects and triage" made coherent; `readme.rs` cases (AC 2)
- [ ] CHANGELOG consolidation (AC 3)
- [ ] Workstation checks and research notes (AC 4)
- [ ] A week of real use, research notes (AC 5)
- [ ] Three Playwright runs; release tests; Completion Notes (AC 6)
- [ ] Audits, `docs/qa/epic-3-report.md`, backlog (AC 7)

## Dev Notes

- Requirements: FR-1 (doctor), PRD section 7, section 8 as amended (amendment 11). Decisions: AD-53 (doctor line), AD-58 (manual list). Epic decisions: P-7 (Epic 2), P-16, D-44, D-52, D-73, D-74.
- Dependencies: stories 3.1 to 3.11. The week of real use starts once 3.11 is merged on the workstation build; the report waits for it (P-16).
- The VS Code close reason matters even with AD-47's rule: a close while the window is shut would hide a real session (amendment 11, R24).
- If the measured `/clear` or `/resume` gap does not fit D-52's window, edit `epic-3-decisions.md` with the fact and the constant in one change, with its vector case.
- Cutting `[0.3.0]` and bumping the workspace version belong to the v0.3 release, after `[0.2.0]` is cut; not in this story.
- No employer content in the README, the research notes or the report: placeholder repositories and paths only (officina-project rule).

## Testing

- CLI integration: `crates/officina/tests/{doctor_registry,readme}.rs`.
- CI: the full `windows` job, release tests, Playwright.
- Manual: research notes entries (AC 4, 5), the QA report (AC 7).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
