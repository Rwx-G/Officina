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
---

# Epic 3 decisions

What the PRD, the addendum, the spine and the code leave open or state twice for v0.3, settled while writing the Epic 3 stories on 2026-10-28. Numbering continues Epic 2's (`epic-2-decisions.md`), so a `P-n` or `D-n` is unique across epics and Epic 3 stories cite both files. Stories cite these decisions and name the test that proves each one; this file states the rule, never the test data. A later change edits this file with the fact that motivates it, like the architecture.

## Precedence

- **P-8.** The addendum's section 4 amendments win over the PRD text (PRD front matter). Where an AD body and the unamended PRD text still differ, the decision named below settles it; the stories edit neither document. (all)
- **P-9.** FR-4 keeps excerpts "per non-archived subject, the latest"; AD-40 stores `session_excerpt` per session. The table stays per session, since triage rows and unfiled sessions need their own; FR-4's bound holds through D-50. (3.2, 3.4)
- **P-10.** FR-6 (slice A) lists archive "with the routine box"; FR-13 (slice B) is the routine marker. The box and the `routine` column ship with archive in slice A (3.5); the candidates list is FR-13's slice B part (3.10). (3.5, 3.7, 3.10)
- **P-11.** AD-40 keeps untrusted model text "only in `model_answer.reason`", while AD-40 and AD-49 have a confirmation copy the proposal it answered. A confirmation copies the verdict, the source and a `ReasonCode` (for a model proposal, the model code); the model's text never leaves `model_answer` (D-68). (3.1, 3.7, 3.11)
- **P-12.** AD-56 says every `HOOK_FIELDS` entry is "verified always present (E2)"; E2 verified four of the buffered events and only documents `Notification`. A real `Notification` payload checked on the workstation is a gate of story 3.8; a field not observed there does not enter the constant. (3.8)
- **P-13.** `BOARD_SCHEMA` goes up by one for all of v0.3, in the first Epic 3 story that changes `Board` (3.2), relative to the value on `main` at that merge; later Epic 3 stories extend the type under the same number, since no release ships in between. Story 2.8 AC 11's "becomes 2" reads "goes up by one" if v0.3 merges first (AD-54: whichever merges second bumps last). `REGISTRY_VIEW_SCHEMA` stays 1 through Epic 3 for the same reason. (3.2, 3.4, 3.8)
- **P-14.** Schema version 1 is written whole in story 3.1, every table of AD-40 and every enum its `CHECK` lists name (D-45), and its SHA-256 is pinned there (AD-41). Until v0.3 ships, a later Epic 3 story that must change it edits migration 1 and its pinned hash in one commit and says why in its change log: no released database holds version 1 yet. From the release on, migrations are append-only. (3.1, all)
- **P-15.** AD-51 takes "AD-31's helper" for the triage child's kill-on-close job, but that helper assigns the calling process; in the window it would bind every child to the window, attach consoles included, which must outlive it (AD-15). D-64 settles the triage job. (3.9)
- **P-16.** The owner's rule (P-7) holds for v0.3: nothing in the PRD or the addendum is deferred. AD-58's manual items close the story they gate (3.8, 3.9) or the epic (3.12). (all)

## Shared code and v0.2 seams

- **D-42.** `closed_enum!` and `sha256_hex` move from `relay/mod.rs` to the core crate root, re-exported from `relay` so v0.2 call sites compile unchanged, in story 3.1, which starts after v0.2 story 2.1 has merged. Every registry enum is declared with the macro, so its `ALL` cannot miss a variant. (3.1)
- **D-43.** The registry writer compiles only under `window-authority` (AD-38). If v0.2 story 2.2 has not merged when 3.1 starts, 3.1 creates the feature and the `officina` dev-dependency on the host with it; 2.2 then adds its module behind it. The writer's error type carries its own marker string; the CI step "officina holds no authority" (P-5, D-9) fails when either marker is in a fresh `officina.exe` or missing from `officina-app.exe`; 3.1 adds the step if 2.2 has not. (3.1)
- **D-44.** Each v0.2 seam of the addendum's touch points is owned by one Epic 3 story:
  - `watch::spawn`'s callbacks become one `Emit` trait in 3.5, with `board` and `registry` methods, plus `relay` when v0.2 story 2.7 has merged (otherwise 2.7 adds that method);
  - `CollectorMsg` loses `Copy` and gains `Author` in 3.5, beside v0.2's `RelayChanged`;
  - the hourly pass becomes one function in 3.2 whose event-file deletions come from the registry; v0.2's journal and approval cleanups (2.8, 2.9) run in it ungated;
  - the hook changes only in `events::{EventLine, reduce_payload}` (3.2, 3.8), never in v0.2's dispatch (AD-26);
  - `HOOK_FIELDS` is checked against the buffered-events constant of D-18 when story 2.4 has merged, else against `doctor::HOOK_EVENTS`, which 2.4 renames (3.8);
  - `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed` comes from 2.7, or from 3.5 if 2.7 has not merged;
  - `crates/officina/tests/readme.rs` comes from 2.10, or from 3.12 if 2.10 has not merged;
  - the Job Object code of 2.5 is shared with the triage runner per D-64 (3.9).

## Registry types and store

- **D-45.** Enums of `registry::types`, each with `ALL` (D-42): `Source` holds the provenance sources of the PRD glossary; `ProposalSource` the sources of FR-8; `Verdict` the two verdicts an author confirms (FR-9); `ModelVerdict` the three a proposal may carry (glossary, Proposal), which is also the model's schema enum (AD-51); `ReasonCode` one variant per FR-8 rule plus one for a model proposal; `SubjectStatus` the glossary's three; the triage call's purpose (proposal or dry run) and outcome (an answer, or one of AD-51's outcomes). AD-57's `Source::ALL` covers both source enums. Vector cases name the `src` of every row they expect, a stored model answer counting as a `model` row, so every provenance value has a case once story 3.11 has landed; each story adds to the coverage test the enums its sections complete. (3.1, 3.3, 3.6, 3.11)
- **D-46.** Gate: a reader opens a WAL registry with `SQLITE_OPEN_READ_ONLY` while no writer runs, after a clean close and after a writer was killed, and creates or changes no file in `registry/`. The addendum verified a reader beside a running writer only; `subjects --json`, `board --json` and doctor with the window closed rely on this. If it does not hold, readers report the registry unavailable with "open the window once" and the result goes to research notes. (3.1)
- **D-47.** Only the window's writer runs `quick_check` at open (AD-42); readers do not, except doctor, which runs it read-only. Every reader state in which nothing is read (absent, older, newer, unavailable, network) gives `officina subjects --json` exit 3 with the reason in the view, and `officina board --json` null subjects with the reason in `diagnostics.registry` (AD-41, AD-53). (3.1, 3.2, 3.4)
- **D-48.** An event file is deletable only when each kept daily copy, opened read-only and passing `quick_check`, holds that file's `event_file` row with `complete_at_ms` set; with fewer valid daily copies than are kept, nothing is deleted. This is FR-2 amendment 2 ("contained in every kept registry copy") and AD-58's "deletion only once both copies cover a file"; AD-44's "earlier than every kept copy" alone would hold with no copy at all. A restore takes the newest copy passing `quick_check` among daily and pre-migration copies, by modification time. (3.1, 3.2)

## Ingestion and sessions

- **D-49.** A file dated before today is complete once fully read and at least `COMPLETE_GRACE_MS` (60 s) after the end of its UTC day: the hook takes `ts` on receipt and appends within its 5 s timeout (AD-16), so a line dated 23:59:59 can land after midnight. (3.2)
- **D-50.** Excerpt lifetimes count from the excerpt's event `ts`, never from ingestion, so a closed window cannot lengthen them (AD-44). A filed session's `session_excerpt` row is cleared when a later session of the same subject records excerpts, and when the session is filed into a subject that holds a later session with excerpts; an unfiled session keeps its row until it is filed, archived or expired. The registry therefore holds, per subject, the latest excerpts only (FR-4, P-9). (3.2, 3.4)
- **D-51.** At start the collector seeds its previous agents snapshot (AD-46) from the registry's sessions that are not ended and whose status came from `agents`, so a session that vanished while the window was closed, with no `SessionEnd`, is `interrupted` at the first successful refresh. (3.3)
- **D-52.** Supersession (AD-47): a `SessionEnd` with `clear` or `resume` and a `SessionStart` with the same `source` under the same `claude_pid` link when the start's `ts` is at or after the end's and within `SUPERSESSION_WINDOW_MS` (60 s), whichever line is ingested first; a line without `claude_pid` (written by a v0.1 hook) links nothing. A `resume` successor filed by rule before its link arrived moves to its predecessor's subject when the link is made; one filed by the author stays. The gap is not measured yet: story 3.12 measures it on the workstation and the constant is revisited with that fact. (3.3, 3.12)
- **D-53.** A session's project is recorded at first sight from its `cwd` with the board's rule (`project_for`) and the window's floor, and is never rewritten by a later floor change; the author moves sessions. The two names that never file (AD-48) become core constants used by `project_for` and by filing; a folder bearing one of those names, with no floor, is treated as that name: accepted. (3.2, 3.4)

## Subjects

- **D-54.** Filing into an archived subject is refused; the author reopens it first. Filing an ended session never changes the target's status; filing a live one into a `to_resume` subject makes it `active` (AD-48). A subject left with no session keeps its status until the author archives, merges or files into it. (3.4)
- **D-55.** Merge takes two non-archived subjects: the source's sessions move to the target, the source becomes `archived` with `merged_into` and is left out of the archived list and of the routine candidates; the target becomes `active` if either was. Confirmations keep the subject they were given on. (3.4)
- **D-56.** `file_sessions(session_ids, subject)` files one or several sessions in one gesture; no subject unfiles them. The author may file a non-interactive session; rules never do (AD-47). (3.4, 3.5)
- **D-57.** A subject name is trimmed, 1 to 80 characters, free of control characters and of the ranges of `relay::params::FORBIDDEN_FORMAT_RANGES` (AD-30), and unique, ignoring case, among the non-archived subjects of its project. Each refusal is a variant of the writer's error enum. (3.4)
- **D-58.** Reopen applies to an archived subject: it sets `to_resume` (AD-48) and stamps `reopened_at_ms` on the subject's latest confirmation, which AD-49's "unreopened" and AD-50's auto-archive flip read. (3.4, 3.10)

## Triage

- **D-59.** `unsure` is never correct: it counts against the model in the dry run and in self-disable, is not a flip (no definite proposal was reversed), and is counted apart in the evidence. A row proposing `unsure` has no verdict until the author presses `f` or `t`, and Confirm all leaves it out (AD-55). (3.6, 3.7, 3.10)
- **D-60.** AD-49's "then the verdict applies if the subject still needs one", for an unfiled row: after filing, the verdict applies when the filed session is the subject's latest and the subject is neither archived nor holding a live session, whatever its status; otherwise the item answers "filed, verdict not needed" with the cause. (3.7)
- **D-61.** The Triage panel heads the Subjects view; the header's "N to sort" chip opens that view on the panel. There is no view beyond AD-55's. (3.5, 3.7)
- **D-62.** Every archiving gesture offers the routine box (FR-13): archive in the Subjects view and a `finished` confirmation in a triage row. An auto-archive never marks routine. (3.5, 3.7, 3.10)
- **D-63.** A confirmation item carries the row key and the proposal shown: verdict, source, `ReasonCode`, and for a model proposal the id of the call that answered. The writer recomputes the proposal and refuses an item that differs, with a typed reason the row shows (AD-49). (3.7, 3.11)

## Model

- **D-64.** The triage child runs in its own kill-on-close Job Object, assigned right after the spawn; the window never joins a job. The gap between spawn and assignment is accepted: the call has no tool and starts no child (E16). The Job Object functions of story 2.5 are shared: whichever of 2.5 and 3.9 lands first writes them in `platform/windows.rs`, and 3.9 adds the assignment of a child process. (3.9)
- **D-65.** The budget (AD-52) counts spawned calls only. A refusal before spawning is a `triage_call` row with outcome `refused` that spends no budget and does not count toward the three-failure pause; `failed`, `timeout` and `invalid` do. (3.9)
- **D-66.** A pause records the `claude` binary's stamp (path, size, modification time) in `setting`; a different stamp at the next check, or re-enabling, lifts it (AD-50). (3.9, 3.11)
- **D-67.** A dry run counts in `cases` every case it ran; an unanswered case is not correct for the model. A dry run stops when the budget is spent and records what ran; a second dry run while one runs is refused. (3.9)
- **D-68.** A `model_answer` row is deleted when its row is confirmed and when the session's excerpts are cleared (archive, TTL). (3.11)
- **D-69.** When the model disables itself or auto-archive turns off, `setting` records the change with source `rules` and the gate state that caused it; turning either on again requires its gate to hold again (for the model, a newer winning dry run). (3.10, 3.11)
- **D-70.** Auto-archive runs on the collector thread after each view build, on rows of filed subjects whose proposal is `state` `finished`; unfiled rows and model proposals are never auto-archived (R18). The view lists the auto-archives of the last 7 days with a reopen action. (3.10)

## Contract

- **D-71.** `ContractRecord` gains `first_seen_at` (serde default: `checked_at` for old records), which is AD-56's "first sighting", plus `hook_missing` and `hook_not_observed`. `ContractView` gains `hook_missing` only (AD-54), like `missing` on the agents side; doctor prints both (AD-56). The window (hook fields) and `board --json` (agents fields) both rewrite `state/contract.json`; a lost update is rewritten by the next line carrying `absent` or the next check: accepted. (3.8)

## Fixtures, documentation, closure

- **D-72.** E2E fixtures for the registry come from `crates/officina-host/examples/registry_fixture.rs` (`required-features = ["window-authority"]`): it writes event files and runs ingestion and author actions at explicit `now_ms` through the window's writer; then `officina subjects --json` and `officina board --json` print the fixtures (AD-58). Never SQL from Node, never a CLI write path (D-25's rule). (3.5 onward)
- **D-73.** Documentation is incremental, as D-34: every story updates `CHANGELOG.md` `[Unreleased]`, and every story with user-visible behaviour updates the README section "Subjects and triage". Story 3.12 makes them coherent. (all)
- **D-74.** Epic 3 closes with `docs/qa/epic-3-report.md` in the Epic 1 report's shape, with a traceability table from every FR and every AD from AD-38 to AD-58 to the story ACs that prove it; open findings go to `docs/backlog.md`. (3.12)
