---
title: "PRD: Officina v0.3 - The Registry and Triage"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28, with the amendments of ../architecture-v0.3.md section 4, which take precedence over the text below
inputs:
  - (framing, not published)
  - (requirement challenge, not published)
  - prd/prd-v0.1.md
  - prd/prd-v0.2.md
  - architecture-spine.md
  - architecture-v0.2.md
  - research-notes.md
---

# PRD: Officina v0.3 - The Registry and Triage

## 0. Document Purpose

Scopes v0.3, Epic 3. v0.1 (board) and v0.2 (relay) hold. Framing principle 7
("the registry is the memory") and principle 8 ("no LLM launches anything;
LLM text enters as untrusted") are what this version makes real. Revised after
a requirement challenge: events are ingested before any retention, `state`
yields proposals the author confirms, verdicts are given per subject, filing
and verdict take one gesture, and the triage model ships disabled and must
prove itself on the author's own confirmations before it is offered.

## 1. Vision

The board shows what is happening now and forgets after 24 hours. The author
works by **subject** (a change, an incident, a migration) that spans several
sessions over several days. v0.3 gives Officina a durable memory: every
session ever seen is recorded with where each fact came from; sessions are
filed under subjects; when a subject has no live session left, Officina
proposes whether it is finished or to resume, and the author confirms
proposals in batches. Those confirmations measure each source of proposals,
and only measured accuracy unlocks anything automatic.

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Pick up where I left.** After a weekend, I see my active subjects,
  which have a live session, which wait for me to resume them, with the last
  prompt and the end of the last answer.
- **J2 - Close without effort.** Subjects whose sessions all ended are sorted
  for me; I confirm a batch in seconds, filing unfiled sessions in the same
  gesture.
- **J3 - Trust by evidence.** I see how often each source of proposals was
  wrong; nothing is archived automatically and no model is consulted until
  measurements say so.
- **J4 - Spot the routine.** When I archive a subject I can mark it "routine,
  could have run alone": those seed the autonomous catalogue (v0.7).

### 2.2 Key User Journeys

1. **Monday.** The Subjects view lists active subjects; "alert-rules rollout"
   has a session waiting on a permission prompt; "certificate renewal" has no
   live session and shows its last prompt and the end of its last answer.
2. **Sorting.** Four subjects have no live session left. Each triage row
   proposes a verdict from Claude Code's state or from deterministic rules,
   shows the excerpts, and, for sessions not yet filed, the subject to file
   them under. The author confirms three, flips one.
3. **Filing.** A new session starts in a repository that has exactly one
   active subject: it joins it. In a repository with two active subjects it
   stays unfiled until the author files it, from the triage row or the
   Subjects view.
4. **Evidence.** After 50 confirmations the Triage panel shows, per source,
   how often its "finished" was flipped, and how the disabled model would have
   scored on those same cases; the author enables the model only if it beats
   the default.
5. **Restart.** Officina closed on Friday evening, reopened on Monday: every
   session that ended meanwhile is in the registry, every subject and verdict
   too.

## 3. Glossary

- **Registry** - the SQLite database (WAL) in the Officina home; the only
  durable store of sessions, subjects, verdicts and confirmations.
- **Subject** - a named unit of work owning sessions; status `active` (a
  session is live, or one ended without a confirmed verdict), `to_resume`
  (confirmed), `archived`.
- **Unfiled** - a session not attached to any subject, listed under its
  project.
- **Proposal** - a verdict suggestion (`finished`, `to_resume`, `unsure`) with
  its source (`state`, `rules`, `model`) and reason; untrusted until the
  author confirms or flips it.
- **Provenance** - the source (`agents`, `hook`, `rules`, `model`, `author`)
  and time of each stored fact.

## 4. Features

### 4.1 Registry

#### FR-1: Durable store
A SQLite database in WAL mode, `synchronous = FULL` for author actions,
`secure_delete` on, migrations indexed by `user_version`. Only the window's
collector thread writes; every CLI opens it read-only. A database written by a
newer Officina opens read-only with a doctor warning; one that cannot be
opened or migrated is reported on the board and in doctor, and the board keeps
working from live sources. Doctor warns when the Officina home sits in a
synchronised or network folder.

#### FR-2: Ingestion before retention
The collector ingests every event file not yet ingested (file name and byte
offset kept in the registry), oldest first, before any retention pass.
Retention deletes only event files fully ingested; no CLI command deletes
event files any more; while the registry is unavailable nothing is deleted.

#### FR-3: Session record
Every session seen through `claude agents` or hook events is recorded once
(key: session id) with project, kind, name, first and last seen, last known
status and state, claude process id, how it ended (`SessionEnd` reason), and
the provenance of each field. Sessions superseded by `clear` or `resume` are
linked to their successor (E14) instead of standing alone.

#### FR-4: Excerpts and privacy
The registry keeps, per non-archived subject, the latest prompt excerpt and
the head and tail of the latest answer; archiving clears them, and none
survives 30 days. `no_excerpts` (environment or floor) keeps none. The hook
records the last 280 characters of `last_assistant_message` as well as the
first 280 (event line version 2, readers accept version 1).

### 4.2 Subjects

#### FR-5: Filing
A new session joins its project's subject only when exactly one subject of
that project is active; otherwise it stays unfiled. No subject is created
without an author gesture. The author creates, renames, merges subjects, and
files or moves sessions.

#### FR-6: Subjects view
The window gains a Subjects view: active and to-resume subjects first, each
with its sessions (live state from the board), excerpts, and actions (rename,
move, merge, archive with the "routine" box, reopen), unfiled sessions per
project, and archived subjects on demand. Board cards show their subject.

### 4.3 Triage

#### FR-7: What needs a verdict
Only a subject whose sessions have all ended needs a verdict, given on its
latest session; earlier sessions and sessions superseded by `clear` or
`resume` take none.

#### FR-8: Proposals
Every proposal is confirmed by the author before it counts. Sources, first
match wins:
- `state`: a background session's `done` proposes `finished`; `failed` and
  `stopped` propose `to_resume`;
- `rules`: a last `UserPromptSubmit` newer than the last `Stop` proposes
  `to_resume` (cut mid-turn); a `SessionEnd` with `prompt_input_exit` after a
  `Stop` proposes nothing by itself;
- default: `to_resume`, source `rules`;
- `model`, when enabled (FR-10), replaces the default for sessions no earlier
  rule settled.

#### FR-9: Batch confirmation
A Triage panel lists subjects needing a verdict: the proposal with its source
and reason, the excerpts, and for unfiled sessions the subject to file them
under (proposed when obvious, choosable). The author confirms, flips, or
confirms all shown; confirming `finished` archives the subject, `to_resume`
sets it so. Each confirmation records the proposal it answered.

#### FR-10: Triage model (shipped disabled)
A contained `claude -p` call (E16): light model, empty folder, no tool, no
MCP server, no settings, no session persistence, prompt on stdin with the
excerpts delimited as untrusted data, answer constrained by a JSON schema to
`verdict` and a `reason` of at most 200 characters. Refused when `claude`
resolves to a `.cmd` shim. Batched, rate-limited, counted. Disabled by
default. While disabled, the window can evaluate it on demand on the
author's past confirmations (dry run, results never shown as proposals) and
show its accuracy next to the default's; the author enables it with a toggle.
It disables itself when its measured accuracy falls below the default's.

#### FR-11: Measured error
Per source, the window shows the false-`finished` rate over that source's
last 50 confirmations, with the count it rests on.

#### FR-12: Auto-archive gate
Automatic archiving is offered only for `state`-source `finished` proposals
and only while that source's false-`finished` rate is below 2 % over at least
50 confirmations; the toggle explains why it is disabled otherwise and turns
itself off if the rate rises.

#### FR-13: Routine marker
Archiving offers "routine, could have run alone"; marked subjects are listed
as candidates for the v0.7 autonomous catalogue.

### 4.4 Contract

#### FR-14: Hook fields in the canary
The contract canary checks the hook fields Officina reads, per Claude Code
version, alongside the agents fields, in the existing per-version record.

## 5. Non-Goals (Explicit)

- Reading transcripts to decide anything (framing principle 3).
- Scene, orchestrator, meeting room (v0.4, v0.5).
- Launching or resuming sessions from Officina.
- Syncing the registry between machines.

## 6. MVP Scope

### 6.1.1 Delivery order
- Slice A: FR-1 to FR-9, FR-14: memory, ingestion, subjects, filing,
  proposals from state and rules, batch confirmation.
- Slice B: FR-10 to FR-13: model (disabled, evaluable), measured error,
  auto-archive gate, routine marker.

## 7. Success Metrics

| Metric | Target | How measured |
|---|---|---|
| Memory | zero session, subject or verdict lost across restarts, crashes and a closed weekend | e2e: kill mid-write, reopen; e2e: three days of event files ingested before retention |
| Attention | one confirmation per finished subject, not per session | e2e on seeded multi-session subjects |
| Evidence | per-source false-`finished` rate shown | e2e on seeded confirmations |
| Containment | triage call has no tool, gets excerpts on stdin only, leaves no transcript; Officina itself writes nothing under the Claude config dir or into `~/.claude.json` (the claude process keeps its own state, E17) | e2e on the argument vector and environment; fingerprint test; manual flag check recorded (E16) |

## 8. Open Questions

- Which `SessionEnd.reason` Claude Code sends when a VS Code terminal hosting
  an interactive session is closed. The registry records sessions whatever the
  reason; the board's rule hiding `other` stays; to verify on the
  workstation.
