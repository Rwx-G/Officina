---
title: "Epic 3 decisions"
author: Rwx-G
date: 2026-10-28
inputs:
  - prd/prd-v0.3.md
  - architecture-v0.3.md
  - architecture-v0.2.md
  - architecture-spine.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - (the code at the start of Epic 3, v0.2 in progress)
  - (review of the Epic 3 stories, not published)
---

# Epic 3 decisions

What the PRD, the addendum, the spine and the code leave open or state twice for v0.3, settled while writing the Epic 3 stories on 2026-10-28, then reworked the same day with the owner's final decisions after a review of the stories (D-75 to D-91) and the review's smaller notes (D-92 to D-97). Numbering continues Epic 2's (`epic-2-decisions.md`), so a `P-n` or `D-n` is unique across epics and Epic 3 stories cite both files. Stories cite these decisions and name the test that proves each one; this file states the rule, never the test data. A later change edits this file with the fact that motivates it, like the architecture.

## Precedence

- **P-8.** The addendum's section 4 amendments win over the PRD text (PRD front matter). Where an AD body and the unamended PRD text still differ, the decision named below settles it; the stories edit neither document. (all)
- **P-9.** FR-4 keeps excerpts "per non-archived subject, the latest"; AD-40 stores `session_excerpt` per session. The table stays per session, since triage rows and unfiled sessions need their own; FR-4's bound holds through D-50. (3.2, 3.4)
- **P-10.** FR-6 (slice A) lists archive "with the routine box"; FR-13 (slice B) is the routine marker. The box and the `routine` column ship with archive in slice A (3.5); the candidates list is FR-13's slice B part (3.10). (3.5, 3.7, 3.10)
- **P-11.** AD-40 keeps untrusted model text "only in `model_answer.reason`", while AD-40 and AD-49 have a confirmation copy the proposal it answered. A confirmation copies the verdict, the source, a `ReasonCode` (for a model proposal, the model code) and the call id (D-88); the model's text never leaves `model_answer` (D-68). (3.1, 3.7, 3.11)
- **P-12.** Superseded by D-90: `Notification` is verified (E17), so story 3.8 has no workstation gate.
- **P-13.** `BOARD_SCHEMA` goes up by one for all of v0.3, in the first Epic 3 story that changes `Board` (3.2), relative to the value on `main` at that merge; later Epic 3 stories extend the type under the same number, since no release ships in between. Story 2.8 AC 11's "becomes 2" reads "goes up by one" if v0.3 merges first (AD-54: whichever merges second bumps last). `REGISTRY_VIEW_SCHEMA` stays 1 through Epic 3 for the same reason. (3.2, 3.4, 3.8)
- **P-14.** Schema version 1 is written whole in story 3.1, every table of AD-40 with the additions of D-88 and every enum its `CHECK` constraints name (D-45, D-88), and its SHA-256 is pinned there (AD-41). Until v0.3 ships, a later Epic 3 story that must change it edits migration 1 and its pinned hash in one commit and says why in its change log: no released database holds version 1 yet. From the release on, migrations are append-only. (3.1, all)
- **P-15.** AD-51 takes "AD-31's helper" for the triage child's kill-on-close job, but that helper assigns the calling process; in the window it would bind every child to the window, attach consoles included, which must outlive it (AD-15). D-64 settles the triage job. (3.9)
- **P-16.** The owner's rule (P-7) holds for v0.3: nothing in the PRD or the addendum is deferred. AD-58's manual items close the story they gate (3.9) or the epic (3.12). (all)
- **P-17.** The owner's final decisions after the review depart from the addendum or the PRD in places, and the stories follow them: D-75 (AD-48's resume filing clause and the first sentence of PRD amendment 5), D-76 (AD-49's anchor; FR-7 amendment 7 for subjects), D-77 (AD-48's statuses), D-80 (how the PRD section 7 Containment row and AD-58's zero-write row read), D-82 (replaces D-46), D-85 (AD-44's 28-day TTL becomes a derived value; AD-52's 28-day evaluation window becomes the 30-day bound). The stories do not edit the PRD or the addendum. (all)

## Shared code and v0.2 seams

- **D-42.** `closed_enum!` and `sha256_hex` live at the core crate root, re-exported from `relay` so v0.2 call sites compile unchanged; `closed_enum!` is exported with `#[macro_export]`, since host enums (reader states, writer refusals) need an `ALL` too. Story 3.1 moves them; v0.2 story 2.1, which wrote them, has merged. Every registry enum is declared with the macro, so its `ALL` cannot miss a variant. (3.1)
- **D-43.** The registry writer compiles only under `window-authority` (AD-38). The feature and the CI step "officina holds no authority" (P-5, D-9) exist since story 2.2 merged; the registry's own marker string, carried by the writer's error type, is checked as D-83 splits it. (3.1, 3.2)
- **D-44.** Each v0.2 seam of the addendum's touch points is owned by one Epic 3 story:
  - `watch::spawn`'s callbacks become one `Emit` trait in 3.5, with `board` and `registry` methods, plus `relay` when v0.2 story 2.7 has merged (otherwise 2.7 adds that method);
  - `CollectorMsg` loses `Copy` and gains `Author` in 3.5, beside v0.2's `RelayChanged`;
  - the hourly pass becomes one function in 3.2 whose event-file deletions come from the registry; v0.2's journal and approval cleanups (2.8, 2.9) run in it ungated;
  - the hook changes only in `events::{EventLine, reduce_payload}` (3.2, 3.8), never in v0.2's dispatch (AD-26);
  - `HOOK_FIELDS` is checked against the buffered-events constant of D-18 when story 2.4 has merged, else against `doctor::HOOK_EVENTS`, which 2.4 renames (3.8);
  - `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` comes from 2.7, or from 3.5 if 2.7 has not merged;
  - `crates/officina/tests/readme.rs` comes from 2.10, or from 3.12 if 2.10 has not merged;
  - the Job Object code of 2.5 is shared with the triage runner per D-64 and D-97 (3.9).

## Registry types and store

- **D-45.** Enums of `registry::types`, each with `ALL` (D-42): `Source` holds the provenance sources of the PRD glossary; `ProposalSource` the sources of FR-8; `Verdict` the two verdicts an author confirms (FR-9); `ModelVerdict` the three a proposal may carry (glossary, Proposal), which is also the model's schema enum (AD-51); `ReasonCode` one variant per FR-8 rule, one for an empty subject (D-76) and one for a model proposal; `RowKind` the row kinds of AD-49 and D-76; `SubjectStatus` the glossary's three; `SettingKey` the keys of `setting` (D-88); the triage call's purpose (proposal or dry run) and outcome (an answer, or one of AD-51's outcomes). AD-57's `Source::ALL` covers both source enums. Vector cases name the `src` of every row they expect, a stored model answer counting as a `model` row, so every provenance value has a case once story 3.11 has landed; each story adds to the coverage test the enums its sections complete. (3.1, 3.3, 3.6, 3.11)
- **D-46.** Replaced by D-82.
- **D-47.** Only the window's writer runs `quick_check` at open (AD-42); readers do not, except doctor, which runs it read-only. Every reader state in which nothing is read (absent, older, newer, unavailable, network) gives `officina subjects --json` exit 3 with the reason in the view, and `officina board --json` null subjects with the reason in `diagnostics.registry` (AD-41, AD-53). (3.1, 3.2, 3.4)
- **D-48.** An event file is deletable only when each kept daily copy, opened read-only and passing `quick_check`, holds that file's `event_file` row with `complete_at_ms` set; with fewer valid daily copies than are kept, nothing is deleted. This is FR-2 amendment 2 ("contained in every kept registry copy") and AD-58's "deletion only once both copies cover a file"; AD-44's "earlier than every kept copy" alone would hold with no copy at all. Copies are named, kept and ordered per D-94. (3.1, 3.2)

## Ingestion and sessions

- **D-49.** A file dated before today is complete once fully read and at least `COMPLETE_GRACE_MS` (60 s) after the end of its UTC day: the hook takes `ts` on receipt and appends within its 5 s timeout (AD-16), so a line dated 23:59:59 can land after midnight. (3.2)
- **D-50.** Excerpt lifetimes count from the excerpt's event `ts`, never from ingestion, so a closed window cannot lengthen them (AD-44). A filed session's `session_excerpt` row is cleared when a later session of the same subject records excerpts, and when the session is filed into a subject that holds a later session with excerpts; an unfiled session keeps its row until it is filed, archived or expired. The registry therefore holds, per subject, the latest excerpts only (FR-4, P-9). (3.2, 3.4)
- **D-51.** At start the collector seeds its previous agents snapshot (AD-46) from the registry's sessions that are not ended and whose status came from `agents`, so a session that vanished while the window was closed, with no `SessionEnd`, is `interrupted` at the first successful refresh (and D-78 lets a later-ingested `SessionEnd` win). (3.3)
- **D-52.** Supersession link (AD-47): a `SessionEnd` with `clear` or `resume` and a `SessionStart` with the same `source` under the same `claude_pid` link when the start's `ts` is at or after the end's and within `SUPERSESSION_WINDOW_MS` (60 s), whichever line is ingested first; a line without `claude_pid` (written by a v0.1 hook) links nothing. The link has D-75's two uses only and never files or moves a session. The gap is not measured yet: story 3.12 measures it on the workstation and the constant is revisited with that fact. (3.3, 3.12)
- **D-53.** A session's project is recorded at first sight from its `cwd` with the board's rule (`project_for`) and the window's floor, and is never rewritten by a later floor change; the author moves sessions. The two names that never file (AD-48) become core constants used by `project_for` and by filing; a folder bearing one of those names, with no floor, is treated as that name: accepted. (3.2, 3.4)

## Subjects

- **D-54.** Filing into an archived subject is refused; the author reopens it first. Filing an ended session never changes the target's status; filing a live one into a `to_resume` subject makes it `active` (AD-48). A subject left with no session keeps its status; while `active`, it reaches triage as an empty subject (D-76). (3.4, 3.6)
- **D-55.** Merge takes two non-archived subjects: the source's sessions move to the target, the source becomes `archived` with `merged_into` and is left out of the archived list and of the routine candidates; the target becomes `active` if either was. Confirmations keep the subject they were given on. (3.4)
- **D-56.** `file_sessions(session_ids, subject)` files one or several sessions in one gesture; no subject unfiles them. The author may file a non-interactive session; rules never do (AD-47). (3.4, 3.5)
- **D-57.** A subject name is trimmed, 1 to 80 characters, free of control characters and of the ranges of `relay::params::FORBIDDEN_FORMAT_RANGES` (AD-30), and unique, ignoring case, among the non-archived subjects of its project. Each refusal is a variant of a fieldless refusal enum with `ALL`, distinct from the writer's error type, which carries the marker and wraps SQLite errors. (3.4)
- **D-58.** Reopen applies to an archived subject: it sets `to_resume` (AD-48) and stamps `reopened_at_ms` on the subject's latest confirmation, which AD-49's "unreopened" and AD-50's auto-archive flip read. A session alive again stamps it the same way (D-77). (3.4, 3.10)

## Triage

- **D-59.** `unsure` is never correct: it counts against the model in the dry run and in self-disable, is not a flip (no definite proposal was reversed), and is counted apart in the evidence. A row proposing `unsure` has no verdict until the author presses `f` or `t`, and Confirm all leaves it out (AD-55). (3.6, 3.7, 3.10)
- **D-60.** AD-49's "then the verdict applies if the subject still needs one", for an unfiled row: after filing, the verdict applies when the filed session is the subject's latest session that saw a prompt (D-76) and the subject is neither archived nor holding a live session, whatever its status; otherwise the item answers "filed, verdict not needed" with the cause. (3.7)
- **D-61.** The Triage panel heads the Subjects view; the header's "N to sort" chip opens that view on the panel. There is no view beyond AD-55's. (3.5, 3.7)
- **D-62.** Every archiving gesture offers the routine box (FR-13): archive in the Subjects view and a `finished` confirmation in a triage row. An auto-archive never marks routine. (3.5, 3.7, 3.10)
- **D-63.** A confirmation item carries the row key and the proposal shown: verdict, source, `ReasonCode`, and for a model proposal the id of the call that answered, stored in `confirmation.call_id` (D-88). The writer recomputes the proposal and refuses an item that differs, with a typed reason the row shows (AD-49). (3.7, 3.11)

## Model

- **D-64.** The triage child runs in its own kill-on-close Job Object, assigned right after the spawn; the window never joins a job. The gap between spawn and assignment is accepted: the call has no tool and starts no child (E16). The Job Object functions of story 2.5 are shared: whichever of 2.5 and 3.9 lands first writes them in `platform/windows.rs`, and 3.9 adds the assignment of a child process. (3.9)
- **D-65.** The budget (AD-52) counts spawned calls only. A refusal before spawning is a `triage_call` row with outcome `refused` that spends no budget, does not count toward the pause, and is not queued again until its inputs change (D-87); `failed`, `timeout` and `invalid` proposal calls count toward the pause (D-81). (3.9, 3.11)
- **D-66.** A pause records the `claude` binary's stamp (path, size, modification time) in `setting`; a different stamp at the next check lifts it, and so does re-enabling (D-81, which owns the pause rule). (3.11)
- **D-67.** A dry run counts in `cases` every case it ran; an unanswered case is not correct for the model. A dry run runs while the model is paused or disabled (D-81), stops when the budget is spent and records what ran; a second dry run while one runs is refused. (3.9)
- **D-68.** A `model_answer` row is deleted when its row is confirmed and when the session's excerpts are cleared (archive, TTL, `no_excerpts` per D-85). (3.11)
- **D-69.** When the model disables itself or auto-archive turns off, `setting` records the change with source `rules` and the gate state that caused it; turning either on again requires its gate to hold again (for the model, a newer winning dry run). (3.10, 3.11)
- **D-70.** Auto-archive runs on the collector thread after each view build, on rows of filed subjects whose proposal is `state` `finished`; unfiled rows, empty-subject rows and model proposals are never auto-archived (R18). The view lists the auto-archives of the last 7 days with a reopen action. (3.10)

## Contract

- **D-71.** `ContractRecord` gains `first_seen_at` (serde default: `checked_at` for old records), which is AD-56's "first sighting", plus `hook_missing` and `hook_not_observed`. `ContractView` gains `hook_missing` only (AD-54), like `missing` on the agents side; doctor prints both (AD-56). The window (hook fields) and `board --json` (agents fields) both rewrite `state/contract.json`; a lost update is rewritten by the next line carrying `absent` or the next check: accepted. (3.8)

## Fixtures, documentation, closure

- **D-72.** E2E fixtures for the registry come from `crates/officina-host/examples/registry_fixture.rs` (`required-features = ["window-authority"]`): it writes event files and runs ingestion and author actions at explicit `now_ms` through the window's writer; then `officina subjects --json` and `officina board --json`, with the window closed (D-82), print the fixtures (AD-58). `app/scripts/e2e-fixtures.mjs`, `pnpm e2e:fixtures` and the CI fixture step run the example with `--features window-authority`. Never SQL from Node, never a CLI write path (D-25's rule). (3.5 onward)
- **D-73.** Documentation is incremental, as D-34: every story updates `CHANGELOG.md` `[Unreleased]`, and every story with user-visible behaviour updates the README section "Subjects and triage". Story 3.12 makes them coherent. (all)
- **D-74.** Epic 3 closes with `docs/qa/epic-3-report.md` in the Epic 1 report's shape, with a traceability table from every FR and every AD from AD-38 to AD-58 to the story ACs that prove it; open findings go to `docs/backlog.md`. (3.12)

## Owner's final decisions after the review

- **D-75.** A resumed conversation keeps its session id (documented: `--resume <id>` continues the same id), so `/resume`, `claude --resume <id>` and `claude -c` continue that session's own record and subject; nothing is filed through supersession. Every new session id, whatever its `SessionStart.source`, follows AD-48's normal rule: it joins its project's single `active` subject, else stays unfiled. Supersession (D-52) has two uses only: a superseded session gives no triage row of its own (D-76), and the board links a live card to the events of its process (E14). (3.3, 3.4, 3.12)
- **D-76.** A subject's triage row is anchored on its latest session that saw a prompt, superseded or not: the row is the subject's, and supersession only means a session gives no row of its own (an unfiled superseded session gives none). A subject none of whose sessions saw a prompt, including one left with no session, gets a row of its own kind, "empty subject: archive?", proposing `finished` with source `rules` and its own `ReasonCode`, confirmed like any row, with no `confirmation_input` and a null `confirmation.session_id`. (3.6, 3.7, 3.10)
- **D-77.** A session the registry holds as ended (AD-47) that is seen alive again, because `claude agents` lists it not ended or a `UserPromptSubmit` newer than its end arrives, makes its subject `active` again (source `rules`) when the subject was `to_resume` or `archived`; reopening an archived subject this way stamps `reopened_at_ms` on its latest confirmation (D-58), so a wrong auto-archive counts as a flip of the `state` source (AD-50). (3.3, 3.4, 3.10)
- **D-78.** The hook's end (`SessionEnd` and its reason) and the inferred `interrupted` (source `rules`) are distinct facts; an explicit `SessionEnd` always beats `interrupted`, whatever the timestamps, and only a later sign of life (D-77) reopens either. Each collector tick ingests events before it applies the agents snapshot. (3.2, 3.3)
- **D-79.** One host function creates a session record (row, project per D-53, filing per D-75) whichever path sees the session first, an event line or an agents record, in the transaction that creates it (AD-43). (3.2, 3.3, 3.4)
- **D-80.** Officina itself never writes under the Claude config dir or into `~/.claude.json` (E17); the `claude` processes it starts (`claude agents`, `--version`, the triage call) maintain their own state like any session. The zero-write tests fingerprint the config dir and `<home>/.claude.json` around Officina's commands with the fake claude. The real triage probe (3.9) checks only that no transcript appears under the config dir for the call's session id (`--no-session-persistence`, E16) and the answer's shape; it has no "write found" branch. This is how the PRD section 7 Containment row and AD-58's zero-write row read (P-17). (3.9, 3.12)
- **D-81.** Three failed proposal calls in a row pause the model (AD-50); re-enabling (`set_triage_model`, under the enable gate) lifts the pause, as does a change of the binary's stamp (D-66). A dry run runs while the model is paused or disabled, and its failed calls do not count toward the pause. Story 3.11 owns the pause rule; 3.9 refers to it. (3.9, 3.11)
- **D-82.** Replaces D-46: a read-only reader may create `-wal` and `-shm` when the window is closed; that is accepted. A reader changes no row, never takes the write lock and never blocks the writer. `subjects --json`, `board --json`, doctor and the fixture pipeline work with the window closed, and tests prove it after a clean close and after a killed writer; fingerprint checks of `registry/` ignore those two file names and compare the database file. (3.1, 3.4, 3.5)
- **D-83.** The registry marker's CI checks split: 3.1 adds the negative check (absent from a fresh `officina.exe`); 3.2, the first story where the window calls the writer, adds the positive control (present in `officina-app.exe`). The step gains a second marker extraction. The writing half of `registry::store` (create, migrate, quarantine, restore, copies) is gated with `write` and `ingest`; the reading half is not. (3.1, 3.2)
- **D-84.** Every ingestion pass (start, growth of today's file, hourly) covers every incomplete event file, oldest first; the hourly pass ingests before it retains. A file shorter than its stored offset was rewritten: it is ingested again from offset 0, relying on idempotent folding (AD-43), its corrupt count restarts, and a diagnostic counts it in the view's ingestion section and in doctor. (3.2)
- **D-85.** Privacy bounds are enforced at window start too: the start sequence purges before the first board or view is emitted. Turning `no_excerpts` on (environment or the window's floor) clears stored excerpts, in `session_excerpt` and `confirmation_input`, at the next pass. `confirmation_input` rows expire with the excerpts they copy, so they fall under the same 30-day bound; the dry run's evaluation set is the confirmations whose inputs are still held, by construction under 30 days old. The bound test sums the excerpt TTL, the copy lifetime and the hourly interval, and `EXCERPT_TTL_MS` is derived as 30 days minus the other two (AD-44's 28 days, less one hour). With the window closed nothing expires until its next start, and the README says so. (3.2, 3.7, 3.9, 3.12)
- **D-86.** The fake claude reads its configuration from the `FAKE_CLAUDE_*` environment variables or, when absent, from a `fake-claude.json` sidecar next to its own executable. In-process tests (collector, triage runner) copy the fake binary into their temp dir with a sidecar and point the collector at the copy; no test calls `std::env::set_var`. The triage runner takes its base environment as an argument, which the window fills from its own environment. (3.2, 3.3, 3.9)
- **D-87.** A refused triage call is recorded once (`triage_call` outcome `refused`, counted and shown with its cause) and its row is not queued again until the session's inputs (excerpts, end, the facts the prompt carries) change. (3.9, 3.11)
- **D-88.** Schema 1 adds to AD-40: `confirmation.call_id`, nullable (the call that answered a model proposal, D-63); `model_evaluation.input_tokens`, `output_tokens` and `cost_microusd` (the dry run's cost); `confirmation.session_id` nullable (D-76). `CHECK` constraints sit on every enum-valued column:
  - `Source`: `session_fact.src`, `session.subject_src`, `subject.status_src`, `setting.src`;
  - `SubjectStatus`: `subject.status`;
  - `ModelVerdict`: `confirmation.proposed_verdict`, `model_answer.verdict`;
  - `Verdict`: `confirmation.verdict`;
  - `ProposalSource`: `confirmation.proposed_source`;
  - `ReasonCode`: `confirmation.proposed_reason`;
  - triage purpose and outcome: `triage_call.purpose`, `triage_call.outcome`;
  - `SettingKey`: `setting.key`;
  - 0 or 1: `subject.routine`, `confirmation.auto`, `confirmation_input.cut_mid_turn`.

  Without `CHECK`: `session_fact.fact` (a new fact needs no migration, AD-40), `confirmation_input.end_reason` (Claude Code's values may grow, E2), project, name and text columns. (3.1, 3.9, 3.11)
- **D-89.** Crash tests must be able to fail. The helper can stall inside a transaction at a chosen point, and the test kills it there as well as at random points. After the kills, the registry equals one built by an uninterrupted ingestion of the same files (sessions, facts, excerpts, offsets, corrupt counts), so a missing event fails, and every offset matches the rows it covers. For confirmations, the helper logs each commit it saw acknowledged; every acknowledged confirmation is present and no row is confirmed twice. (3.2, 3.7)
- **D-90.** `Notification` is verified (E17, a background session in `default` mode asked to use Write): its `HOOK_FIELDS` entry rests on E17, and that method, not a print-mode capture, is how the entry is re-checked after a Claude Code change. Supersedes P-12's gate. (3.8)
- **D-91.** Automated proofs added: corruption detected while the window runs makes the registry unavailable until the next start, the board continues, nothing is deleted and offsets do not move (AD-42); the hourly pass runs `wal_checkpoint(TRUNCATE)` unconditionally (AD-39); doctor's network-folder failure runs end to end with `OFFICINA_HOME` set to a UNC path, which the classification calls network without any call; CI runs Playwright with `--repeat-each=3`; a CI step fails when `cargo tree -p officina-host -e normal` lists a crate the addendum rules out (scoped to the host crate, since Tauri brings some of them into the workspace); the triage probe passes when the answer is schema-valid, no transcript exists for its session id, and its input tokens are within 20 % of E16's figure. (3.1, 3.2, 3.9, 3.12)

## Review notes settled

- **D-92.** An `AuthorRequest` carries its deadline, and the collector drops a request whose deadline passed, so the window never shows an error for an action that later happened. `get_registry` reads the cached view, never the collector. `evaluate_triage_model` answers once the dry run is queued and reports progress in the view. (3.5, 3.9)
- **D-93.** A restore writes the `setting` key for the restore notice with the copy name and time; the view shows it for 7 days, `subjects --json` included. (3.1, 3.4, 3.5)
- **D-94.** Daily copies are named by UTC date, and that date decides "due" and "older than 2 days", so tests run at explicit `now_ms`; modification time only orders restore candidates. `VACUUM INTO` writes a temporary name, renamed when complete. The two kept are the two newest valid copies: a damaged copy never pushes out a valid one. Amends D-48. (3.1, 3.2)
- **D-95.** `create_subject` takes a project: the project of the selected sessions when created from them, else one chosen among the projects the registry knows. (3.4, 3.5)
- **D-96.** The view lists, per project, the count of non-interactive runs and the 10 most recent; `subjects --json` prints the same. (3.4, 3.5)
- **D-97.** Runtime edges: a quarantine move that fails (a file held by another process) leaves the registry unavailable until the next start, nothing deleted; the triage-workdir exclusion compares with `quota::normalise_path` (case and slashes); the triage runner keeps draining stdout past its cap and drains stderr, so an oversized answer is `invalid`, not a timeout; if 3.9 lands before story 2.5 it adds the `windows-sys` feature `Win32_System_JobObjects` (v0.2 addendum section 2); `subjects --json` exits 0 when the registry is readable, a stale board inside the view notwithstanding. (3.1, 3.2, 3.4, 3.9)
