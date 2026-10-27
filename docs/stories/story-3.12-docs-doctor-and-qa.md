# Story 3.12: Doctor coherence, documentation, workstation checks and QA closure

**Status:** Approved

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

- [ ] Doctor registry section ordering and level rules; `doctor_registry.rs` cases (AC 1)
- [ ] README "Subjects and triage" made coherent; `readme.rs` cases (AC 2)
- [ ] CHANGELOG consolidation (AC 3)
- [ ] Workstation checks and research notes (AC 4)
- [ ] A week of real use, research notes with the counts (AC 5)
- [ ] `--repeat-each=3` in the CI Playwright step; release tests; zero-write coverage (AC 6)
- [ ] Audits, `docs/qa/epic-3-report.md`, backlog (AC 7)

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

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: UNC doctor case automated, Playwright `--repeat-each=3` in CI (D-91); resume and clear ids recorded on the workstation (D-75); counts of the week of real use named; README privacy states the closed-window and zero-write facts (D-80, D-85); P-17 departures listed in the QA report
