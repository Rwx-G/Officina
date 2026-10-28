---
title: "PRD: Officina v0.7 - The Governor"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions), with the amendments of ../architecture-v0.7.md section 4 applied; Slice B gated (gate 0, eligibility, shadow evidence); addendum of section 9 (2026-10-28: the critic and research helpers)
inputs:
  - (framing, not published)
  - (requirement challenge, not published)
  - prd/prd-v0.6.md
  - (v0.6 requirement challenge, not published)
  - prd/prd-v0.1.md
  - prd/prd-v0.2.md
  - prd/prd-v0.3.md
  - prd/prd-v0.4.md
  - prd/prd-v0.5.md
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - architecture-v0.4.md
  - architecture-v0.5.md
  - architecture-v0.6.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - stories/epic-3-decisions.md
  - stories/epic-4-decisions.md
  - stories/epic-5-decisions.md
  - https://code.claude.com/docs/en/statusline
  - https://code.claude.com/docs/en/agent-sdk/typescript
  - https://code.claude.com/docs/en/costs
  - https://code.claude.com/docs/en/commands
  - https://code.claude.com/docs/en/headless
  - https://code.claude.com/docs/en/legal-and-compliance
---

# PRD: Officina v0.7 - The Governor

## 0. Document Purpose

Scopes v0.7, Epic 7. v0.1 to v0.6 hold. The framing's last core component
arrives: the governor, "deterministic code: quota, attention budget,
autonomous catalogue" (framing section 5), with its roadmap row: autonomous
launches from the catalogue, the attention budget, suspension of templates
under the threshold, the daily autonomy window. The framing's exit criteria
are the targets: over two weeks, the weekly reserve never entered, zero launch
on stale quota, the antechamber never above its ceiling.

For the first time something may start without the author's click. Principle
8 holds as follows: the governor launches only standing orders the author
wrote and approved with the catalogue, through v0.5's single constructor
`WorkOrder::validate` (v0.5 FR-3, AD-77), only in the isolated workshop
(principle 9, v0.6), and its decision reads numbers, counts and the author's
own verdicts, never text. No model chooses what runs or when.

v0.6 hands v0.7 what it deferred (v0.6 section 5): launches without a gesture,
pre-approved permanent templates, the attention budget, the governor, template
suspension by acceptance rate, the daily autonomy window, the global stop
shortcut, and merged detection with its consumer. All are taken here.

Revised after a requirement challenge: Slice A is evidence and a shadow
governor that launches nothing, and Slice B's launches wait on gate 0, an
eligible template and five days of shadow decisions; v0.8 no longer waits on
the two-week measure; acceptance settles at the review verdict, with a
`no_change` outcome that counts nowhere; file observations are anchored on
hook events and the probe backs off instead of pausing; weekly autonomous
spend has a hard cap and an allowance measured in cost; candidates are pinned
to the floor where their projects live; FR-15 is a read-only table; and the
shortcut stops autonomy only.

**Order of evidence.** No workshop run has happened yet, so nobody knows how
often a template earns autonomy or how often the governor's conditions pass
together on this author's day. Slice A answers both before any launch code is
written: it settles runs, computes eligibility, and runs the decision in
shadow, journaling what it would have launched. Slice B, the launches, is
implemented only once gate 0 is recorded, one template is eligible, and the
shadow shows the conditions do pass (section 6.1.1).

**Null option.** Without v0.7, a v0.6 card launches a workshop run in one
click. The governor removes that click and, more, the watching around it:
noticing a 5-hour window about to reset with margin left, checking the weekly
pace, choosing a moment the antechamber is quiet. It costs review time, which
the author bounds (FR-9). Section 7's Throughput row decides whether it pays.

Facts about Claude Code rest on `docs/research-notes.md` (E-numbers), on
v0.6's documented surfaces (cited "v0.6 S2"), or on the pages listed at the
head of section 4 (S-numbers). What none settles is in section 8 with the check
that settles it; the gating ones run first (FR-0). This PRD is written before
v0.6's architecture addendum: an FR naming a v0.6 FR follows that FR as the
addendum amends it.

## 1. Vision

The author's scarce resource is attention, not quota (framing section 1). The
5-hour window resets every five hours whether he used it or not, and the
margin he leaves is lost. v0.7 gives the workshop a governor: inside a daily
window the author sets, it launches in the isolated workshop the routine work
he declared and that has earned it, spending margin that would otherwise be
lost, a bounded share of the week and never the reserve he keeps for himself,
and never handing him more than he said he can take.

**What it launches.** Standing orders only: a workshop template, a project,
values from the template's lists, a base branch and a cadence, written by the
author in the floor file and approved with the catalogue. A standing order
runs alone only while its template's packages are imported often enough at
review, and stops by itself when they no longer are. A run that correctly
finds nothing to do costs the template nothing and slows its cadence.

**What it reads.** The account's 5-hour and weekly use, from the quota file
anchored on hook events and from a contained usage probe, never older than 30
minutes; how many items wait in Needs you; how many minutes of review the
author spent today; the author's verdicts; the run's own cost estimate. It
reads no transcript, no excerpt, no model answer.

**How it hands work back.** A finished autonomous run's package enters Needs
you only while the antechamber is under its ceiling; otherwise the governor
holds it, quietly, and stops producing more until the author has room.

**Where it ends.** The governor moves work as far as a review package. The
author still receives, imports and merges, as in v0.6; Officina never merges
or pushes. v0.7 does not touch the measured mailbox pain, ansible and SSH run
from WSL against servers, which is v0.8's; v0.8 does not wait for v0.7's
two-week measure (section 6.1.1). Standing orders may name free or guarded
repositories alike, since a worker holds no relay and no key either way; an
infrastructure repository keeps v0.6's `review = "all"` (v0.6 FR-11).

## 2. Target User

### 2.1 Jobs To Be Done

- **J1 - Spend what I would lose.** Routine work runs in the margin of a 5-hour
  window I will not use, within a weekly share I set, never in my reserve.
- **J2 - Never more than I can take.** Autonomous results wait behind my
  antechamber's ceiling and within the review time I give them per day.
- **J3 - Only what earned it.** A template runs alone only after I imported
  its packages often enough, and stops by itself when I stop importing them.
- **J4 - Know why.** One line tells me why nothing runs now, or why something
  did or would have, with the numbers it used.
- **J5 - Stop it at once.** A shortcut from anywhere stops autonomy; the end
  of the autonomy window stops it by itself.

### 2.2 Key User Journeys

The floor is the owner's personal floor (section 2.3); the project is a
personal Rust project, `backend` in the examples.

1. **Setup (Slice A).** The Orchestrator view's Governor section lists the
   templates: `dependency-bump` 7 imported of its last 10 settled runs, with
   merged detection beside it (5 merged, 1 retouched, 1 pending); `lint-fix`
   6 settled only. The author adds a `[governor]` table (window
   `18:00-23:30`, every day) and the standing order `bump-serde-json`
   (template `dependency-bump`, project `backend`, `package = serde_json`,
   `level = minor`, base `main`, every 168 hours). The catalogue banner shows
   the diff; one click approves it. He sets the governor to shadow.
2. **Shadow week (Slice A).** For five evenings the section shows what the
   governor would have done: two would-launch decisions, each with its
   numbers, and the share of window minutes per hold cause (`not_perishable`
   41 %, `ceiling` 22 %, `no_active_window` 18 %). Nothing is launched. With
   gate 0 recorded, the template eligible and the shadow showing enough
   would-launch decisions, Slice B is built.
3. **An evening launch (Slice B).** The 5-hour window resets at 21:40. At 19:45
   the section reads: 5-hour 48 % plus allowance 6 against 70, weekly 38 % with
   4.5 days elapsed (projection 59, plus reserve 25, plus allowance 2: 86),
   autonomous points this week 2 of 10, Needs you 1 of 3, review today 0 of 30
   minutes. A usage probe confirms the numbers; the governor launches
   `dependency-bump-7` with a 60-minute deadline. The board shows it in
   Working with "isolated" and "autonomous" chips.
4. **Arrival.** The run ends at 20:35 while three sessions wait: the package
   is held, and a neutral chip reads "1 held". At 20:50 the author clears two;
   the package enters Needs you as "review: dependency-bump-7, autonomous". He
   receives it and imports it in four minutes: the run settles accepted.
5. **Nothing to do.** A week later `serde_json` has no new minor release. The
   run exits 0 with no commit past the base: `no_change`. Nothing enters
   Needs you and acceptance does not move. The base is unchanged the next
   week, so the order is not due. After three `no_change` runs in a row, the
   section says its cadence is now 336 hours.
6. **Suspension.** `lint-fix` has earned it; two of its autonomous packages
   are discarded as `poor_result`, and its acceptance falls to 5 of 10. The
   standing order turns itself off with that reason, and the briefing says so.
   It runs again alone only after a newer clicked run settles and the rate is
   back over the threshold, and the author clicks it on.
7. **Stop.** At 23:28 a run is live; at 23:30 the window ends and the run
   stops with its package kept. Another evening, the author presses
   `Ctrl+Alt+Shift+X` from his editor: autonomy stops within 10 seconds, and
   the run he clicked himself goes on.

### 2.3 Candidate standing orders (proposed, to confirm by the owner)

v0.6 proposed its templates for the owner's personal Rust, Python and Svelte
projects (v0.6 section 2.3). Those projects live on his **personal floor**: his
personal machine and his personal Claude account, the floor Officina itself is
developed on. None lives on the **work floor**, and a standing order for
personal work there would spend an employer's plan on it, so none is proposed
there. v0.7's first floor is therefore the personal floor. The governor acts
only while a 5-hour window is active on that account (FR-4), that is, while the
author works there interactively during the autonomy window; whether that
leaves launch room is section 8, question 8, decided before Slice B.

Each candidate needs its template eligible first (FR-3). Gate 0 records, per
candidate, whether the personal account may run that project's work. The Use
metric counts the candidates the owner confirms. Routine-marked subjects
covered (v0.3 FR-13): none yet, since v0.3's marker is not built; before
Slice B, this section lists for each candidate the marked subjects it covers or
states that none do (FR-17 flags an order with none).

1. **`bump-<package>`** from `dependency-bump`. Floor: personal. Project: a
   personal Rust or Python project, which lives on the personal floor. One
   standing order per package of the template's list, every 168 hours,
   `paths` covering the manifest, the lock file, sources and tests. It fits
   autonomy: no decision to take, a result judged by the project's tests, one
   package per run; a week with no release is a `no_change`.
2. **`lint-<project>`** from `lint-fix`. Floor: personal. Project: a personal
   Rust, Python or Svelte project, which lives on the personal floor. Every 72
   hours; while the base has not moved since a `no_change` or a `not_needed`
   discard, it is not due (FR-3), so a quiet project costs nothing.
3. **`tests-<module>`** from `missing-tests`. Floor: personal. Project: a
   personal project with a closed module list, which lives on the personal
   floor. One per module, every 168 hours, `paths = ["tests/**"]`: a change
   outside the tests quarantines the package (FR-12).

## 3. Glossary

- **Governor** - the window's deterministic code that decides, launches,
  holds and stops autonomous runs (FR-11 to FR-14). Its modes are off, shadow
  and on.
- **Shadow** - the mode in which the governor decides on every tick and
  journals what it would launch, and launches nothing (FR-11).
- **Standing order** - an author-written, catalogue-approved entry naming a
  workshop template, a project, values, a base and a cadence: the only thing
  the governor launches (FR-10). The framing's "permanent template".
- **Autonomous run** - a v0.6 workshop run launched by the governor instead of
  a click.
- **Grant** - the value only the governor's decision constructs, which the
  launch path accepts in place of a click (FR-12).
- **Settled outcome** - the reading of one run for acceptance, fixed at its
  review verdict: accepted, refused or not counted (FR-2).
- **`no_change`** - a run whose worker exited 0 with no commit past the base,
  read from the bundle check (FR-2).
- **Merged detection** - Officina's read-only check of whether an imported
  branch reached its base, shown beside acceptance (FR-1).
- **Acceptance** - accepted over settled among a template's last settled runs
  (FR-2).
- **Eligible**, **due** - a standing order whose template's gate holds and
  that is neither suspended nor paused; due when its cadence and base allow a
  run now (FR-3).
- **Suspended**, **paused** - a standing order turned off by the rules: by
  evidence, or after repeated environment failures (FR-3).
- **Observation** - the account's 5-hour and weekly use at one moment, with
  its source and anchored time (FR-4).
- **Usage probe** - a contained `claude -p "/usage"` call whose structured
  report gives an observation (FR-5).
- **Fresh** - an observation whose anchored time is under 30 minutes old, with
  the weekly window, no window past its reset (FR-4).
- **Perishable margin** - what the 5-hour window has left in its last
  `lead_minutes` before reset (FR-11).
- **Projection** - the weekly use the author's pace on his `days` reaches at
  the weekly reset (FR-6).
- **Reserve** - the part of the weekly window the governor never plans to
  spend (FR-6).
- **Points-per-USD ratio** - how many points of a window one US dollar of a
  worker's cost estimate takes, measured at gate 3 (FR-6).
- **Allowance** - what one run of a template is expected to take of the week
  or of the 5-hour window, in points (FR-6).
- **Weekly autonomous cap** - the most points of one weekly window the
  governor's runs may take together (`autonomous_week_max_pct`, FR-6).
- **Antechamber**, **Needs you count** - the Needs you column and its count,
  the board's `waiting_count` (AD-35, v0.6 FR-10).
- **Ceiling** - the Needs you count at and above which the governor adds
  nothing (FR-8).
- **Held package** - an autonomous package verified and waiting, outside
  Needs you, for room under the ceiling (FR-8).
- **Review budget** - the minutes per local day the author gives autonomous
  packages (FR-9).
- **Autonomy window** - the daily local-time range, on chosen days, in which
  the governor may launch; at its end autonomous runs stop (FR-13).
- **Hold cause** - a condition that keeps the governor from launching now,
  shown with its numbers; the first is the one displayed; per-order causes are
  those of the first due order in pick order, every order's own causes being
  listed with it (FR-11).
- **Self-stop** - the governor turning itself off on a rule (FR-14).

## 4. Features

Documented surfaces this PRD rests on, beyond E1 to E17 and v0.6's S1 to S8.
Each becomes an E-entry once the check of section 8 that uses it has run.
Pages are under `https://code.claude.com/docs/en/`, read on 2026-10-28.
Numbers are local to this PRD; v0.6's are cited as "v0.6 S2".

- **S1** (`statusline`) The status line input carries
  `rate_limits.five_hour` and `rate_limits.seven_day`, each with
  `used_percentage` (0 to 100) and `resets_at` (Unix seconds). `rate_limits`
  "appears only for claude.ai Pro and Max subscribers, or behind a Claude apps
  gateway that sets a spend limit for you, and only after the first API
  response in the session. Each window ... may be independently absent, and
  Claude Code drops a window once its `resets_at` time passes." The script
  runs when a session starts or resumes, then when a new assistant message
  arrives, `/compact` finishes, the permission mode changes, vim mode toggles,
  the `command` changes, a `refreshInterval` timer elapses if set, a
  rate-limit window reaches its `resets_at`, or a warm prompt cache reaches
  its expiry; updates are debounced at 300 ms. "The event-driven triggers can
  go quiet when the main session is idle." E3 recorded the two fields first.
- **S2** (`agent-sdk/typescript`) "When you send a command such as `/context`
  or `/usage` as a prompt, its output arrives as an `SDKAssistantMessage`."
  The `/usage` result carries `usage_report` (`SDKUsageReport`, "experimental:
  its shape may change"). Its `rate_limits.limits` holds the server's usage
  rows: `kind` ("such as `session`, `weekly_all`, or `weekly_scoped`. Classify
  a row on this, never on a label"), `group`, `percent` (0 to 100),
  `resets_at` (ISO 8601 or null), optional `scope`, `severity`, `is_active`.
  `rate_limits` "is `null` when Claude Code couldn't get the plan's usage, for
  example when the session's OAuth token lacks the `user:profile` scope"; an
  empty `limits` means the server reported no meters. `SDKRateLimitEvent`
  (`type: "rate_limit_event"`) is "emitted when the session encounters a rate
  limit", with `status` (`allowed`, `allowed_warning`, `rejected`) and optional
  `resetsAt` and `utilization`. A `result` message carries `total_cost_usd`,
  `usage`, `num_turns`, and subtypes including `error_max_budget_usd`.
- **S3** (`costs`) On Pro, Max, Team and Enterprise plans `/usage` shows plan
  usage limits. "When the request for your plan limits fails, most often
  because the usage endpoint is rate limited, `/usage` shows the last usage
  bars it loaded on this machine within the past 60 minutes, along with a
  `Showing last-known usage` note"; "rate limited until 3:14am" means Claude
  Code "can't fetch your plan limits again until then". "Some commands like
  `/usage` may generate requests to check status" (background token usage,
  "typically under $0.04 per session"). On Team and Enterprise plans usage
  "draws from a per-seat allowance that resets on a rolling five-hour window
  and a weekly window", shared with Claude chat and Cowork.
- **S4** (`commands`) `/usage`: "Show session cost, plan usage limits, and
  activity stats"; `/cost` and `/stats` are aliases. The page does not say
  whether `/usage` runs under `-p`. Verified on 2.1.312: `/usage` runs under
  `-p` as a local command (addendum section 1).
- **S5** (`legal-and-compliance`) "Advertised usage limits for Pro and Max
  plans assume ordinary, individual usage of Claude Code and the Agent SDK."
  OAuth "is designed to support ordinary use of Claude Code"; developers
  building products "should use API key authentication". It does not prevent
  "an end user from signing in to the unmodified Claude Code binary with their
  own Claude subscription" (v0.6 S3).
- **S6** (`headless`) In `-p`, user-invoked skills and custom commands work,
  and "Built-in commands that only run in the terminal interface, such as
  `/login`, aren't available". `total_cost_usd` and per-model costs are
  "client-side estimates". `system/api_retry` carries the `error` category
  `rate_limit` for a 429. On SIGTERM a `-p` run exits 143, runs `SessionEnd`
  hooks and records no result for the interrupted turn.

### 4.0 Slice 0: checks

#### FR-0: Gates first
The section 8 questions marked **gate** are run on the workstation and
recorded as E-entries.
- Before any Slice A story is implemented: gates 1, 2, 3 and 6. Gate 3 needs
  one clicked v0.6 workshop run.
- Before any Slice B story is implemented: gate 0, plus the other Slice B
  conditions of section 6.1.1. Gate 4 comes before FR-13's window-end stop,
  gate 5 before FR-16.

If gate 0 refuses on every candidate's floor and no API key is available, or
gate 3 shows that a worker's spend does not appear on the floor's account
under `oauth-token`, Slice B is not built and the close reads Slice A's
evidence alone. A failed gate changes the FR it names first, as E12 reshaped
v0.2's approvals.

### 4.1 Evidence

#### FR-1: Merged detection
Merged detection is shown beside acceptance. It is the gate only after the
owner switches it on (FR-2).

For each package imported under v0.6 FR-12 (branch `officina/<name>` created
at tip T, from base branch B at base commit C), Officina reads whether the
author merged it. Only imports with at least one commit past C are read; a
`no_change` run is never imported (FR-2). Host `git` runs under v0.6 FR-10's
git rule, with D-22's flags and `GIT_OPTIONAL_LOCKS=0`, read-only, on a worker
thread, hourly and when the Governor section opens. It never fetches: a merge
made on a forge counts once the author's repository has fetched it. Outcomes,
first match wins:
- `merged`: T is an ancestor of `refs/heads/B` or of a remote-tracking
  `refs/remotes/<remote>/B`.
- `merged_changed`: the local `officina/<name>` now points past T (T is its
  ancestor), and that tip is an ancestor of B or of a remote-tracking B: the
  author retouched the branch, then merged it.
- `merged_equivalent`: every commit of `C..T` has a patch-equivalent commit in
  `C..B` (`git cherry`): rebased or cherry-picked.
- `not_merged`: `officina/<name>` is gone with none of the above, or 14 days
  have passed since the import.
- otherwise `pending`.

A squash merge is invisible to these rules. The per-template history offers
two pointer-only corrections (D-28) on a `pending` or `not_merged` row,
"merged" and "not merged"; a correction has source `author`, which beats
`rules` as v0.3's provenance does, and is final. Journal `merge_settled`
(work order, outcome, source). Only the author's refs decide: the worker never
writes the project repository (v0.6 FR-12). Remote-tracking branches are those
of the repository's remotes (`git remote`). Patch equivalence is read with
`git cherry` against each base reference. An outcome other than `pending`, and
any correction, is final; `pending` rows are read again hourly. A merge pass
runs no program the repository configures.

#### FR-2: Acceptance
A run of a workshop template, clicked or autonomous, **settles at its review
verdict**, as the framing feeds acceptance from review verdicts (framing
section 7):
- **accepted**: Import (v0.6 FR-12);
- **refused**: Discard `poor_result` or `should_not_have_run`; a quarantine
  (v0.6 FR-10, and `out_of_scope`, FR-12); Discard `not_needed` of an
  autonomous run;
- **not counted**: Discard `not_needed` of a clicked run, since the author
  chose to run it; an `expired` held package (FR-8), on which nobody judged;
  a run that ended without a package, whose cause FR-3 counts instead; and
  `no_change`.

**`no_change`.** The worker exited 0 and the branch has no commit past the
base. It is read from v0.6 FR-10's bundle check, never from model text. It
never enters Needs you, is never held, counts nowhere in acceptance, and is
recorded by v0.6's `workshop_finished` line with outcome `no_change`. It is
the modal correct outcome of a routine
order, and must not push a good template toward suspension. v0.6 does not yet
define a zero-commit outcome: its architecture must state whether the bundle
check of an empty range yields a package or none, and v0.7 reads that outcome
as `no_change` either way.

A template's acceptance is accepted over settled among its last
`acceptance_runs` settled runs (default 10), shown with the count it rests on,
per template across projects, as the framing states it (an acceptance rate
"per task template", framing section 7). Gives up: a template good in one
project and poor in another is judged on the mix (section 8, question 9). The
Governor section shows per template: acceptance, `no_change` count, refusals
by kind, and, beside it, FR-1's merged, retouched (`merged_changed`) and not
merged counts.

**Gate on merges.** The window offers the owner a pointer-only switch that
makes FR-1's outcome the gate instead of the verdict (an Import then settles
as accepted only once merged, and as refused at `not_merged`). It is offered
only while section 7's Merge detection row shows author corrections under 30 %
over at least 20 imports; the writer re-checks that when it writes (D-69's
pattern). It ships off. Section 8, question 10 asks the owner how he merges.

#### FR-3: Eligibility and suspension
A standing order is **eligible** when:
- its template has at least `acceptance_runs` settled runs and an acceptance
  of at least `acceptance_pct` (default 60 %) over them (framing section 7:
  "an acceptance threshold of 60 % over 10 outputs"); this replaces v0.6's
  provisional "imported over decided at least 50 % with 5 verdicts" for
  autonomy (v0.6 section 7);
- it is neither suspended nor paused.

An eligible standing order is **due** when, in addition:
- its last autonomous launch is at least its effective cadence old: its
  `every_hours`, doubled after three `no_change` runs in a row; the first run
  with another outcome restores it, and the Governor section says which
  applies;
- no run of it is live and no package of it awaits a verdict, in Needs you or
  held;
- the base tip has moved since its last `no_change` run, and since its last
  decided run if that run was discarded `not_needed`.

It turns itself off, recorded in the registry with source `rules`, the cause
and the numbers (D-69), and journaled `automation_changed`:
- **suspended** when a newly settled run takes acceptance below the threshold,
  and at once when an autonomous package of it is discarded
  `should_not_have_run` or quarantined (any cause, `out_of_scope` included);
- **paused** after three of its autonomous runs in a row end with an
  environment-failure cause (v0.6 section 7: a refused host, a missing
  program, `hook_failed`), as the triage model pauses after three failed calls
  (AD-50).

It comes back only by the author's pointer click in the Governor section,
offered while the gate holds again and, after a suspension, once a clicked run
of the template has settled after the suspension: proved again by hand, then
trusted again. Clicked runs of a suspended template stay available; suspension
removes autonomy only. Turning a standing order off by hand is always offered.
In Slice A eligibility is computed and shown, never acted on.

### 4.2 Quota

Only the quota regime v0.6 gate 7 recorded for the floor is built: FR-4 to
FR-6 for `oauth-token`, FR-7 for `api-key`.

#### FR-4: Quota observations
An observation is, for the floor's account (AD-7), per window, `used` (0 to
100) and `resets_at`, plus, from the probe, the scoped weekly rows (S2); with
its source and its **anchored time**. Two sources:
- **Quota file** (v0.1 FR-5, AD-8). A statusline writes it on the triggers of
  S1, several of which (a permission mode change, a vim toggle, a prompt cache
  expiry, a `refreshInterval` timer) rewrite numbers fetched earlier, so
  `written_at` does not say when the numbers were fetched. The anchored time
  is instead that of the latest `UserPromptSubmit` or `Stop` event of the
  file's `session_id` in the event buffer (AD-8, AD-43) at or before
  `written_at`: a turn's numbers come from that turn's responses, which lie
  between those two events. A write with no such event in the 30 minutes
  before it is not fresh. Neither a worker in a container nor a `-p` call
  updates the file.
- **Usage probe** (FR-5): the anchored time is the probe's end.

**Fresh** means an anchored time within the last 30 minutes (framing section
7), the weekly window present, and no window's `resets_at` past. The governor
combines its fresh observations window by window and keeps the higher `used`:
a forged file with low numbers, which any process running as the author can
write (the limit AD-25 accepts for the relay's files), cannot lower what the
governor sees beside a probe.

- No fresh observation, or none with the weekly window, holds the governor
  (`quota_unknown`, `quota_stale`). Missing data is unknown, never room
  (principle 3).
- A fresh observation without a 5-hour window means none is active: S1 drops a
  window at its reset, and none exists before a first response. It holds
  (`no_active_window`), never launches (section 8, question 8).
- After an autonomous run ends, the next launch needs an observation anchored
  after that end (`no_post_run_observation`), so a run's own spend is seen
  before the next run starts.

Every observation the governor uses is journaled once (`quota_observed`:
source, anchored time, `written_at` for a file, values; never text). The quota
file contract does not change. An event's `ts` counts at or before
`written_at` up to the end of that second. Two fresh observations whose reset
times for one window differ by more than 15 minutes, or that disagree on
whether a 5-hour window is active, hold the governor (`quota_conflict`).
Within that tolerance, the earliest 5-hour reset bounds the deadline and the
latest weekly reset sets the projection.

#### FR-5: Usage probe
When gate 1 passes, Officina runs a contained call whose only purpose is the
plan's usage: `claude -p "/usage" --output-format stream-json --verbose
--model haiku` with AD-51's containment flags (`--tools ""`,
`--strict-mcp-config --mcp-config {"mcpServers":{}}`, `--setting-sources ""`,
`--no-session-persistence`) and `--settings {"crossSessionInbound":"refuse"}`
(AD-51 as amended by v0.5), from the empty folder
`<officina home>/usage/empty`, with the floor's `CLAUDE_CONFIG_DIR`, AD-80's
removals as D-128 reads them for contained kinds, a kill-on-close job, a 30 s
deadline and stdout capped at 256 KiB. `--model haiku`: it bounds the cost if
a release sends `/usage` to a model; verified to change nothing for the local
command. The environment rule adds: without `CLAUDE_CODE_OAUTH_TOKEN` and
`ANTHROPIC_API_KEY`: the probe answers for the floor's own login, since a
setup-token lacks the scope the report needs. It is a new `VectorKind`,
`UsageProbe`, with its golden vector (AD-79), and goes through the one spawn
module (D-128); it is Officina's own session, which loads no user or project
configuration (v0.5 FR-10's reading of principle 2).

It reads only the structured `usage_report.rate_limits.limits` of the
assistant message (S2), never the message text, and maps rows by `kind`:
`session` to the 5-hour window, `weekly_all` to the weekly window,
`weekly_scoped` to scoped weekly rows (gate 2 confirms the mapping against the
statusline). The fields it reads become canary constants (AD-12). A `session`
row whose `resets_at` is null means no active 5-hour window. Outcomes:
- **observation**: the rows are present and current;
- **back-off**: the report's text contains an entry of core's
  `USAGE_DEGRADED_MARKERS`, seeded from S3's phrases; the text is matched,
  never parsed, stored or shown. No observation, and no probe for 60 minutes.
  A back-off does not count toward the pause below;
- **structural failure**: a missing field, a null `rate_limits`, an empty
  `limits`, a missing row, a model turn: a `result` with `num_turns` above 0,
  a `local_command` other than `usage`, or a non-zero cost (verified clean: 0,
  `usage`, 0), a non-zero exit or the deadline. Three in a row pause the probe
  until the `claude` binary changes (AD-50's pause rule), and the governor
  falls back to the file.

Probes run only when the governor asks: before a launch or a shadow
would-launch (FR-11), every 10 minutes while an autonomous run is live
unless FR-13 reads the worker's own utilization, and, in shadow or on, a
bracket around each workshop run, clicked or autonomous (FR-6). Never more
than one per 2
minutes, 6 per hour and 60 per local day, counted from the journal
(`usage_probe`: outcome, cause, duration, cost from the `result` line, values;
AD-52's counting, its own budget). Gate 1 tests that rate over a full working
day before the budget is kept.

**Fallback.** Without the probe (gate 1 failed, or paused), the governor
decides on fresh file observations and at most once per 5-hour window
(`one_per_window`), so a quiet statusline cannot hide more than one run's
spend. Doctor and the README say the governor then acts only while an
interactive session keeps the statusline fresh.

#### FR-6: Projection, reserve, allowances, weekly cap
**Projection.** f is the elapsed share of the author's `days` within the
weekly window, not of calendar time: the hours of listed days elapsed since
the window started (its `resets_at` minus 7 days), over the hours of listed
days the whole window holds, never less than one listed day's share. The
projection at reset is `used / f`: the pace of the days he works, carried to
the reset. It counts every use of the account, the governor's own included,
which errs toward caution.

**Reserve.** `reserve_pct` (default 25) is the part of the week the governor
never plans to spend (framing section 7: "a weekly reserve of 25 %").

**Points-per-USD ratio.** For each window, the points one US dollar of a
worker's `result` cost estimate (v0.6 FR-8, S6) takes. Gate 3 measures it
first: one worker run bracketed by two probes. Each workshop run, clicked or
autonomous, bracketed by a probe at most 2 minutes before its launch and one
within 15 minutes after its end, is a sample; the ratio in use is the largest
of the last 5 clean samples (no other session of the floor Working during the
run, and a rise of at least 2 points, since gate 2 may show whole points).
Before any clean sample, allowances take the starting values. Use from
claude.ai chat or other devices is invisible to the board (S3), so a ratio
can only err high.

**Allowances.** A finished run takes its own cost estimate times the ratio, in
weekly points and in 5-hour points. A template's allowance for its next run is
the largest of its last 5 runs' points, and, before any run, its `max_usd`
times the ratio, or 3 weekly and 10 five-hour points without `max_usd`
(starting values, unmeasured).

A launch needs:
- projection + reserve + weekly allowance at most 100 (`weekly_projection`);
- no scoped weekly row at or above 100 minus the reserve (`scoped_weekly`),
  whatever the template's model: rows are classed by `kind`, never by label
  (S2);
- the weekly points of this weekly window's autonomous runs, plus this
  template's weekly allowance, at most `autonomous_week_max_pct` (default 10,
  2..=25) (`autonomous_week_cap`): a bound that holds whatever the
  projection's error;
- the 5-hour `used` plus the 5-hour allowance at most `five_hour_max_pct`
  (`five_hour_room`, FR-11).

Points are computed in thousandths, rounded toward caution.

#### FR-7: Floors on an API key (only if v0.6 gate 7 recorded `api-key`)
When `[workshop] auth = "api-key"`, workers do not draw on the plan (v0.6 S2,
S3), so FR-4 to FR-6 are not built. J1 does not apply: an API key loses
nothing unused, and the governor's gain is the click it saves, paid in
dollars. `[governor] usd_per_week` is required: the weekly figure the owner
records at gate 0. Every autonomous template must set `max_usd` (v0.6 FR-7). A
launch needs this weekly window's autonomous spend (the `result` cost
estimates of its autonomous runs, S6), plus the `max_usd` of live ones, plus
this template's `max_usd`, at most `usd_per_week` (`usd_budget`); the week is
the local week starting on the first of the author's `days`. Costs are
client-side estimates and a run can pass its cap (v0.6 S7): accepted, and
stated beside the key. Throughput is read per USD at the close.

### 4.3 Attention

#### FR-8: Ceiling and held packages
The **Needs you count** is the board's `waiting_count` (AD-35, v0.6 FR-10):
sessions waiting, approvals, review packages. The **ceiling** (`ceiling`,
default 3, framing section 7: "an antechamber of 3 seats") bounds what the
governor adds, not what the author's own sessions do: it cannot keep a session
from waiting. Holding moves review in time; only the review budget (FR-9) and
suspension (FR-3) bound it.

- The governor launches nothing while the count is at or above the ceiling
  (FR-11). In shadow and on, it journals `ceiling_crossed` (above or below)
  each time the count crosses the ceiling inside the window, so the share of
  window minutes under the ceiling is read whatever the first hold cause.
- An autonomous run's package that passes v0.6 FR-10's checks enters Needs you
  only while the count is below the ceiling. Otherwise it is **held**:
  recorded, listed in the Governor section, counted in a neutral "N held" chip
  (AD-55: not amber, not in the title, the overlay or the scene), and absent
  from `waiting_count`. Journal `package_held` (run, Needs you count).
- On every board change, while the count is below the ceiling, the oldest held
  package is released into Needs you, one per free seat. Journal
  `package_released` (run, count at that moment).
- A quarantined autonomous package and an autonomous run ended without a
  package never enter Needs you: the Governor section lists them with their
  cause and Discard, and FR-2 and FR-3 count them. `credential_found` also
  self-stops the governor (FR-14), whose terracotta header chip shows it
  outside Needs you. A `no_change` run is listed, never held.
- A package held for 7 days is discarded with the reason `expired`, which
  v0.6 already gives a package undecided at its text bound (automatic,
  journaled `package_expired`) (its containers were removed at collection).
  One expiry self-stops the governor (FR-14): it already means a full week at
  or above the ceiling.

Packages of clicked runs are never held: the author asked for them. Before
Slice B, the shadow's share of window minutes under the ceiling is read; under
20 %, the close asks the owner whether the ceiling should count review
packages and approvals only, which departs from the framing's antechamber and
is therefore his decision (section 8, question 13).

#### FR-9: Review budget
The **review budget** (`review_minutes`, default 30 per local day, framing
section 7: "an autonomous review budget of 30 minutes a day") bounds the
review the governor creates:
- **Spent today**: over autonomous packages decided since local midnight, the
  minutes from Receive to verdict as v0.6's Review cost row measures them,
  each counted at most 30; a package discarded without Receive counts 1.
- **Estimate** of a template: the median of that measure over its last 5
  verdicts, clicked or autonomous, and 10 minutes until 3 exist (a starting
  value, twice v0.6's 5-minute target).
- **Pending**: the estimates of autonomous packages in Needs you or held, and
  of live autonomous runs.

A launch needs spent + pending + the template's estimate at most the budget
(`review_budget`). Releases of held packages depend on the ceiling only: the
budget was counted when the run was launched. Slice A computes the budget in
shadow; Slice B enforces it.

### 4.4 The governor

#### FR-10: Standing orders and the governor's keys
Schema 2 gains keys; the floor schema number does not change (AD-75's
reasoning). Every key below belongs to the canonical catalogue (AD-24): a new
or changed key is usable only after the author approves the catalogue, which
is how a standing order is pre-approved, and the canonical format goes up once
for v0.7 (AD-76, D-131). A session that edits the floor to add a standing
order or loosen a threshold gets nothing until the author approves the diff.

```toml
[governor]
window = "18:00-23:30"          # local time; "22:00-06:00" crosses midnight
days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
ceiling = 3                     # 1..=9
review_minutes = 30             # per local day, 5..=240
reserve_pct = 25                # 10..=50
autonomous_week_max_pct = 10    # 2..=25
five_hour_max_pct = 70          # 30..=85
lead_minutes = 120              # 30..=300
acceptance_pct = 60             # 50..=100
acceptance_runs = 10            # 5..=50
max_autonomous = 1              # 1..=2
stop_shortcut = "Ctrl+Alt+Shift+X"   # Slice B (FR-16)
usd_per_week = 20.0             # FR-7 only, required on an api-key floor

[[standing_order]]
name = "bump-serde-json"
template = "dependency-bump"    # a workshop template (v0.6 FR-2)
project = "backend"
values = { package = "serde_json", level = "minor" }
base = "main"                   # one of the template's base list
every_hours = 168               # 12..=720, default 72
paths = ["Cargo.toml", "Cargo.lock", "src/**", "tests/**"]   # optional
```

Phase 2 rules (AD-23), each an issue kind with a failing fixture:
- `window`: two `HH:MM` times, different; the range belongs to the day it
  starts. `days`: one to seven distinct day names.
- every number within its range; `usd_per_week` positive, required when
  `[workshop] auth = "api-key"` and refused otherwise; `five_hour_max_pct`
  at most 85, so FR-13's live stop stays under 100.
- a standing order's `name` passes `is_valid_action_name`, unique among
  standing orders, at most 16 of them;
- `template` names an approved workshop template (one with
  `[template.workshop]`, v0.6 FR-2); a host template is an issue, since the
  governor never launches on the host (principle 9);
- `project` is one of the template's, with a `[project.workshop]` table;
- `values` pass `WorkOrder::validate` against the template (AD-77) at
  validation: each parameter present once, each value in its list;
- `base` is in the template's base list;
- `paths`: glob patterns, at most 32, no `..` segment.

Without a valid `[governor]` table the governor is unavailable and every
standing order is listed with that issue.

#### FR-11: The decision, in shadow and on
The governor is window code under `window-authority` (AD-21, AD-73): with the
window closed nothing autonomous starts, and the shipped `officina.exe` holds
no governor code (P-5's CI step). A pure core function decides, on every
collector recompute (AD-5) and every 60 s while the governor is in shadow or
on: `governor::decide(&GovernorInput, now_ms, local_offset_min) -> Decision`,
where `Decision` is `Hold(causes)`, `Probe` or `Launch`. The input is the
approved snapshot, the mode and stop flags, the board's counts, the registry's
evidence, the observations, the probe state, the shadow's bookings and v0.6
FR-6's refusal checks per standing order, computed by the host. The local
offset comes from Windows at each tick (gate 6), so daylight saving changes
move the window as the clock does.

Conditions, in this order; every failing one is recorded and the first is
shown (`HoldCause`, a closed enum):
1. `governor_off`, `self_stopped`: the mode is off, or a self-stop holds
   (FR-14).
2. `stopped`: the workshop or relay stop flag is set (v0.6 FR-14, AD-34); a
   flag that cannot be read counts as set.
3. `catalogue_changed`: the floor differs from the approved catalogue (AD-77's
   rule). Standing orders and `[governor]` keys come only from the approved
   snapshot, and a pending change holds everything. `governor_unavailable`:
   the approved snapshot has no valid `[governor]` or no standing order.
4. `local_time_unknown` (first): the local offset cannot be read;
   `outside_window`: the local time is outside `window` on a listed day;
   `window_ending`: under 20 minutes to its end.
5. `autonomous_full`: `max_autonomous` autonomous runs are live (default 1,
   which keeps each run's spend measurable).
6. `held_waiting`: an autonomous package is held.
7. `ceiling`: the Needs you count is at or above `ceiling`.
8. `none_due`: no standing order is eligible and due (FR-3); the Governor
   section shows each one's reason.
9. `workshop_refused`: every due standing order meets one of v0.6 FR-6's
   refusal causes (stop flag, engine, images, `battery_stale`,
   `hook_program_missing`, `workshop_full`, `reviews_pending`,
   `credential_missing`); the cause shows per order.
10. `review_budget`: FR-9's sum exceeds the budget for every due order.
11. Quota, on the combined fresh observations (FR-4 to FR-6; FR-7's
    `usd_budget` replaces this step on an API-key floor): `quota_unknown`,
    `quota_stale`, `quota_conflict`, `no_post_run_observation`,
    `no_active_window`,
    `five_hour_room` (the 5-hour `used` plus the template's 5-hour allowance
    above `five_hour_max_pct`, framing section 7: "no autonomous launch above
    70 % of the 5-hour window"), `not_perishable` (the 5-hour reset more than
    `lead_minutes` or less than 20 minutes away: framing section 7's first
    condition, "the 5-hour window nears its reset with a margin that would be
    lost"), `weekly_projection`, `scoped_weekly`, `autonomous_week_cap`, and,
    without the probe, `one_per_window`.
12. With the probe available: a quota condition that fails on fresh data holds
    without probing, since a probe can only add a higher number. A stale or
    missing observation, or one with no probe in the last 2 minutes, gives
    `Probe` (within FR-5's rate; `probe_wait` while the rate or a back-off
    forbids it). The governor decides again on the probe's observation, and
    reaches `Launch` only when a probe anchored in the last 2 minutes is among
    the observations it used. A structural failure holds (`probe_failed`).

Then it picks, among the due standing orders that passed, the one whose last
autonomous launch is oldest (never launched first), ties by name.

**Shadow** (Slice A). The decision returns a would-launch with no grant; only
mode on returns a launch: the governor journals
`governor_would_launch` with every input the decision used (standing order,
observations with source, anchored time and values, projection, reserve,
allowances, weekly autonomous points, Needs you count, review minutes spent,
pending and estimate, window, local time) and books the decision as if it had
launched: the order's last launch time, a shadow run live until the deadline
FR-12 would set, its review estimate pending until the end of the local day,
and its allowance in this week's autonomous points. Shadow decisions therefore
follow the cadence and the caps a real launch would. Shadow needs no gate 0
and creates no container. The Governor section labels everything shadow.

**On** (Slice B). `Launch` goes to FR-12, and `governor_launch` carries the
same inputs.

In both modes the journal gets `governor_hold` when the set of failing
conditions changes, at most one line per minute, with every failing condition
and its numbers ("weekly projection 79 % + reserve 25 % + allowance 3 % >
100"), so section 7 reads the share of window minutes per cause. Behaviour is
held by `tests/vectors/governor.json` (formatter-exempt): input, then the
expected decision; a coverage test fails when a `HoldCause` variant appears in
no case. `governor_hold` is repeated every 15 minutes while unchanged, and a
line counts at most 15 minutes past itself. `governor_would_launch` and
`governor_launch` carry the decision's whole input, so each can be replayed.

#### FR-12: The launch (Slice B)
The governor launches through v0.5's and v0.6's path, nothing beside it:
1. `ApprovedStandingOrder`, whose only constructor reads the approved snapshot
   (AD-77's pattern), gives the template, project, values and base, all author
   text from the approved catalogue.
2. `WorkOrder::validate` builds the work order exactly as for a click (v0.5
   FR-3); a value outside its list is refused there, as always.
3. The launch takes a `LaunchAuthority`: `Click { card_id, prompt_sha256 }`
   from the window (D-28), or `Governor(GovernorGrant)`. A `GovernorGrant`
   has private fields, is built only inside `governor::decide` from a
   `Launch` decision, names the standing order and the approved catalogue
   hash, is consumed once and is valid 60 s. `compile_fail` doctests show that
   no other code builds one, as AD-74's do for `WorkOrder`. The launch's git
   facts are read first, off the collector; then, in one collector step, the
   decision is taken again and must name the same order, the grant is built,
   and the card is issued and consumed with it through AD-78's path, without
   the prompt-hash and plan-hash comparisons that belong to a click. The card
   is never stored or shown. v0.6 FR-6's refusal checks run again at launch.
4. The run is v0.6's in every respect: container, gateway, limits, package,
   quarantine, review, Import. Its deadline is the least of the template's
   `max_minutes`, the minutes to the 5-hour reset and the minutes to the
   window's end, so autonomous spend never runs into the author's next 5-hour
   window and every run ends by the window's end even with the window closed
   (v0.6 FR-8 enforces deadlines three ways). On an API-key floor only the
   template's and the window's limits apply.
5. Records: the `work_order` row gains `source = governor` and the standing
   order's name; `work_order_launched` carries `by: governor`; the board's
   workshop card shows an "autonomous" chip.

**Scope.** When the standing order sets `paths`, a package with an added,
changed, deleted or renamed path outside them is quarantined `out_of_scope`
(v0.6 FR-10 gains the cause), counts as refused (FR-2) and suspends the
standing order (FR-3). This is the framing's per-worker trigger "diff out of
scope" (framing section 6), as a boundary on what a run may hand back, not on
what it may read. Paths match case-sensitively.

**Exposure.** v0.6 section 1 holds for every standing order, and unattended
cadences multiply the runs nobody watches live: anything a worker reads can
leave through `api.anthropic.com`. Do not write a standing order for a
repository you would not paste into a model. The README says so beside
`[[standing_order]]`.

Principle 8, checked. The decision's inputs are the approved catalogue,
numbers parsed from the quota file and the probe's structured report, the
board's counts, the author's verdicts and the runs' cost estimates (numbers
from v0.6 FR-8's `result` line). No `Untrusted<T>` (AD-74) is an input of
`governor::decide`. An orchestrator proposal stays a proposal: it opens a card
and waits for a click (v0.5 FR-12), even when it names a standing order's
values. A forged quota file can change timing within the author's limits,
never what runs. Three mutants join AD-95's job (D-148):
`mutant-governor-skip-freshness`, `mutant-governor-skip-ceiling`,
`mutant-governor-skip-reserve`; each must turn the governor vectors red.

#### FR-13: Autonomy window and live guards (Slice B)
**Window end.** At the window's end ("a daily autonomy window beyond which
everything stops by itself", framing section 6) the governor stops every live
autonomous run by v0.6 FR-14's per-run stop: the entry point writes the
package (v0.6 FR-10), which follows FR-8. Clicked runs are not touched.
Journal `autonomous_stopped` (cause `window_end`). With the window closed,
FR-12's deadline already ends the run by then.

**Live guards.** While an autonomous run is live, every fresh observation,
from either source, is checked; the probe runs every 10 minutes when
available. If gate 3 shows `rate_limit_event.utilization` (S2) in the worker's
stream, a reading of it at or above a stop threshold also stops the runs,
mapped as gate 3 records; it never replaces the 10-minute probe, since stream
lines are untrusted (v0.6 FR-8) and the event names no window. A reading
showing the weekly window, or a scoped weekly row,
at or above 100 minus the reserve stops every autonomous run
(`reserve_reached`) and self-stops the governor (FR-14). One showing the
5-hour window at or above `five_hour_max_pct + 10` (default 80) stops them
(`five_hour_high`), so the author's own work keeps room. Stops keep the
packages.

#### FR-14: Modes, stop autonomy, self-stop
- **Shadow** (Slice A). The governor ships off. Shadow is offered, pointer only
  (D-28), while the approved floor has a valid `[governor]` and at least one
  standing order; it needs no eligible order, so the shadow can show why none
  is.
- **On** (Slice B). Offered, pointer only, while the approved floor has a
  valid `[governor]`, at least one standing order is eligible, a passing
  escape battery is fresh (v0.6 FR-15), the quota source is usable (the probe
  passed gate 1 and is not paused, or a file observation is fresh) and no
  self-stop holds. The writer re-checks these conditions when it writes
  (D-69's pattern). Journal `governor_mode`.
- **Off** is always offered and stops new launches; live autonomous runs go on
  to their deadline.
- **Stop autonomy** sets the mode to off and stops every live autonomous run,
  packages kept; clicked runs and the relay go on. This is the framing's
  per-profile emergency stop ("each profile has its own emergency stop"), and
  what the shortcut does (FR-16). v0.6's Stop all still stops everything,
  autonomy included.
- **Self-stop**, recorded with source `rules` and its cause (D-69), journaled,
  shown as a terracotta header chip like "relay stopped" (AD-35):
  `reserve_reached` (FR-13), `held_expired` (one expiry, FR-8),
  `credential_found` on an autonomous package (v0.6 FR-10). Turning it on
  again needs the conditions of On and the author's click; for
  `credential_found`, also a recorded credential change (doctor names the
  stored credential's last-written time, never its value).

### 4.5 Control and view

#### FR-15: Automations table (Slice B)
A read-only table in the Governor section, and a section of the briefing (v0.5
FR-1), built from the records each automation already writes: the `setting`
rows with source `rules` of the triage model and auto-archive (D-69), the
`auto_approval_set` lines of auto approval (v0.5, AD-32 as amended), and the
`automation_changed` lines of standing orders (FR-3). Per automation: its
current state, its last change and the cause recorded with it. No shared core
type, no new rule, no change to v0.3 or v0.5: the framing suspends task
templates only, and this table only lets the author ask, in one place, what
runs without him and why something stopped.

#### FR-16: Stop shortcut (Slice B)
v0.6 deferred the framing's global shortcut ("a button, a global shortcut,
and `officina stop --all`", framing section 6). The window registers
`[governor] stop_shortcut` (default `Ctrl+Alt+Shift+X`; at least two
modifiers among Ctrl, Alt, Shift and Win, plus one key among A to Z, F1 to F24
and Pause) as a system-wide hotkey through Win32 `RegisterHotKey` on a
dedicated thread with its own message loop, using the existing `windows-sys`
crate; no plugin. Pressing it is Stop autonomy (FR-14), not Stop all: a stray
press must not halt the relay or the author's clicked runs, and a run holds no
key that would need more. It is keyboard-reachable since it only reduces. It
works while the window is minimized or behind another; with the window closed,
`officina stop --all` remains. A chord another program holds fails to
register: the Governor section and doctor say so, and the floor names another.
Gate 5. The chord is registered with `MOD_NOREPEAT`, so holding it stops once.

#### FR-17: In the window
- **Governor section** of the Orchestrator view (AD-93), beside v0.6's
  Workshop section:
  - the mode (off, shadow, on), its controls (pointer only; Slice A offers off
    and shadow, Slice B adds on and Stop autonomy), and any self-stop with its
    cause;
  - the state line: launching, would launch (shadow), holding with the first
    cause and its numbers, or off; and the next time a condition is expected
    to change when the governor can compute it ("the 5-hour window enters its
    last 120 minutes at 19:40");
  - quota: each observation's source, anchored time, age and values, the
    projection, the reserve, the allowances, this week's autonomous points
    against the cap;
  - attention: Needs you against the ceiling, review minutes spent and pending
    against the budget;
  - shadow evidence: would-launch decisions with their inputs, the share of
    window minutes per hold cause, and the share under the ceiling;
  - standing orders: template, project, values, eligibility with acceptance
    (n of N), effective cadence, last run and outcome, next due time, state
    and reason, the per-order on and off (Slice B), and the routine-marked
    subjects the author linked to it (v0.3 FR-13; linked by pointer, stored
    with source `author`, nothing derived), with a "no routine subject" flag
    when none is linked;
  - held packages, oldest first, and autonomous runs that ended without a
    package, with `no_change` or in quarantine, with Discard where it applies;
  - per template, FR-2's figures, FR-1's outcomes beside them with their two
    corrections, and the "Gate on merges" switch when FR-2 offers it;
  - in Slice B, FR-15's Automations table.
- **Header:** neutral chips "governor shadow", "governor on" or "governor
  holding", and "N held" (AD-55); a terracotta chip for a self-stop.
- **Board** (Slice B): autonomous workshop cards carry an "autonomous" chip in
  Working and in Needs you. `Board` gains a governor summary (mode, held
  count); `BOARD_SCHEMA` goes up once for v0.7 (P-13's rule).
- **Briefing** (v0.5 FR-1): governor mode, would-launch decisions or
  autonomous runs and their outcomes since the cursor, held packages,
  suspensions, pauses and self-stops. `officina mcp` serves them through
  `briefing` and `board`; no tool is added, and none changes the mode,
  launches, releases, links or corrects.
- **Scene** (amended by section 9, item 3, for helpers): no new element. Autonomous workers sit at their project's desks
  like v0.6's isolated workers; a held package is not drawn, since nobody
  waits on the author until it is released (v0.4 FR-3: no motion without a
  state change).
- Every text from a run stays `Untrusted<T>` (AD-74), as in v0.6.
- `officina governor --json` prints the section read-only, its state line
  from the journal's latest decision.

#### FR-18: Doctor
Slice A:
- `[governor]` and standing order issues; for each standing order, eligible or
  why not;
- quota sources: whether the statusline carries rate limits (absent means a
  plan without them in the statusline or no response yet, S1), the quota
  file's `written_at` and anchored time, gate 1's outcome, the probe's last
  outcome, any back-off and today's count;
- gate 3's recorded outcome and the points-per-USD ratios in use;
- the local UTC offset the governor uses and the next window;
- the governor's mode, self-stop and cause.

Slice B: held packages and their age; the stop shortcut registered or not, and
the chord.

## 5. Non-Goals (Explicit)

- Autonomous launches on the host, outside the workshop (principle 9); a
  standing order naming a host template is an issue (FR-10).
- Any model deciding what or when to launch. Orchestrator proposals, the
  `auto` review and the triage model never launch; standing orders are author
  text.
- Autonomous Import, merge, push, or verdict. The only automatic verdict is
  `expired`, which counts nowhere in acceptance (FR-2, FR-8).
- Spending a 5-hour window that is not active, at night or after a long
  break, unless section 8, question 8 decides otherwise before Slice B.
- Relay actions, server identities and the framing's `iterative` profile in
  the workshop; the workstation's browser and commands for workers;
  intervention orders with a dry run (v0.8). The mailbox pain, ansible and SSH
  from WSL, is v0.8's. No HTTP probe for workers either: HTTP is 2 to 5 % of
  the owner's need (AD-36).
- The installer, updates, `doctor` for a fresh machine and the Linux build
  (v1). v0.7's Windows-only pieces, `RegisterHotKey` (FR-16) and the time zone
  offset (FR-11), are named here as inputs of v1's Linux build.
- Thresholds that learn: every starting value changes only by the author's
  floor edit and approval.
- Acceptance per template and project (section 8, question 9).
- Squash-merge detection; it is the author's one-click correction (FR-1).
- Fetching from remotes, or any write to the project repository beyond v0.6's
  Import.
- Officina's own contained calls (triage, review, probe) counted in
  allowances, the weekly cap or the reserve: they keep their own budgets
  (AD-52, AD-87, FR-5); gate 1 records the probe's cost.
- Desktop notifications or sound: the governor never interrupts; it waits for
  room.
- Building both quota regimes: only the one v0.6 gate 7 recorded (FR-4 to FR-7).
- A shared state type or rule across v0.3's, v0.5's and v0.7's automations
  (FR-15 is read-only).
- A standing order on the work floor for personal work (section 2.3); several
  floors at once; a worker credential on an account other than the floor's
  (gate 3).
- Changing any v0.3 or v0.5 threshold.

## 6. MVP Scope

### 6.1.1 Delivery order
- **Slice 0:** FR-0. Gates 1, 2, 3 and 6 before any Slice A story is
  implemented; gate 3 needs one clicked v0.6 workshop run.
- **Slice A, evidence and shadow:** FR-1, FR-2, FR-3 (eligibility shown, never
  acted on), FR-4 to FR-6 (or FR-7 on an API-key floor, under the one-regime
  rule), FR-10, FR-11 run in shadow, FR-17 and FR-18 without launch controls.
  In shadow, `governor::decide` runs on every tick and journals
  `governor_would_launch` with its full inputs; nothing is launched, gate 0 is
  not needed, and FR-9's budget and FR-8's ceiling are computed as decision
  inputs. Usable alone: runs settle at their verdict with merges shown beside,
  each template's eligibility is visible, and the author sees what the
  governor would launch and why it holds.
- **Slice B, launch:** FR-8, FR-9's enforcement, FR-12 to FR-14, FR-15, FR-16,
  and the launch controls of FR-17 and FR-18. A Slice B story is implemented
  only after:
  - gate 0 is recorded (on an API-key floor, with the owner's weekly USD
    figure);
  - question 7 shows one eligible template;
  - 5 working days of shadow, counted on the author's `days`, show at least 3
    would-launch decisions, with the share of window minutes per hold cause;
  - question 8 is decided, and the shadow's share of window minutes under the
    ceiling is read (FR-8);
  - section 2.3 lists the routine-marked subjects each confirmed candidate
    covers, or states that none do.

  If the shadow shows fewer than 3, the close reads the Hold causes row and
  revisits the starting value that dominates before any launch code is
  written.

**v0.8 does not wait on the measure.** v0.8 starts when v0.7's built slices
close. The two-week measure of section 7 runs once a standing order becomes
eligible and Slice B is on, alongside v0.8, and is read at the latest at v1's
close. When Slice B's conditions do not yet hold at Slice A's close, Slice B
is built when they hold, before v1's close. This is an explicit exception to
P-16, and for Slice B to P-7, listed for the owner's confirmation: v0.8, which
addresses the measured mailbox pain, consumes no governor code, and the
governor's evidence arrives at the pace of clicked runs.

### 6.1.2 Dependencies for the owner's approval

| Item | Where | Licence | Why |
|---|---|---|---|
| `windows-sys` features for `RegisterHotKey` and the message loop (`Win32_UI_Input_KeyboardAndMouse`, with `Win32_UI_WindowsAndMessaging` already present) | host | existing crate, MIT or Apache-2.0 | FR-16 |
| `windows-sys` feature for `GetDynamicTimeZoneInformation` and `SystemTimeToTzSpecificLocalTimeEx` (`Win32_System_Time`) | host | existing crate | FR-11's local offset (gate 6) |

No crate, image, tool or Node package is added. Merged detection uses host
`git` under v0.6's rule; the probe is a `claude` spawn; the decision is pure
core code. Both Windows features need Linux counterparts at v1 (section 5).

## 7. Success Metrics

Slice A, read on shadow data before Slice B:

| Metric | Target | How measured |
|---|---|---|
| Would launch | at least 3 `governor_would_launch` decisions over 5 working days of shadow, counted on the author's `days`; fewer sends the close to the dominant hold cause's starting value before any launch code | journal |
| Hold causes | share of window minutes in shadow by condition, failing first and failing at all; a cause over 80 % names the starting value revisited; `no_active_window` or `not_perishable` over 80 % feeds question 8's decision | `governor_hold` lines |
| Ceiling room | share of window minutes with Needs you under the ceiling; under 20 %, the owner decides question 13 | `ceiling_crossed` lines |
| Merge detection | settled by rule over all imports read by FR-1, and author corrections; corrections under 30 % over at least 20 imports offer the "Gate on merges" switch (FR-2) | FR-1 sources |
| Probe | over gate 1's working day and then the shadow: structural failures under 10 % of probes, back-offs counted, cost per probe recorded, no change in the author's interactive `/usage`; differences above 5 points between a probe and a fresh file recorded | `usage_probe` lines; gate 1 |
| Shadow inert | zero container created and zero work order launched by the governor in shadow | fake engine counts in e2e; registry |

Slice B, with the governor on:

| Metric | Target | How measured |
|---|---|---|
| Reserve | over the 14 days of the measure: zero autonomous launch whose recorded projection + reserve + allowance exceeded 100, or with a scoped weekly row at or above 100 minus the reserve; zero reading at or above 100 minus the reserve while an autonomous run was live, or within 10 minutes after its end, without that run stopped within 60 s; and at each weekly reset, the last observation before it: above 100 minus the reserve in a week with autonomous spend counts as the reserve entered, reported with that week's autonomous points beside it | `governor_launch` lines (each carries its inputs), `quota_observed` and `usage_probe` lines, run intervals from `workshop_started` and `workshop_finished`; vector cases; `mutant-governor-skip-reserve` |
| Fresh quota | over the 14 days: zero autonomous launch on an observation whose anchored time is older than 30 minutes, lacking the weekly window, past a reset, or older than the previous autonomous run's end; with the probe, every launch used a probe anchored under 2 minutes before it | `governor_launch` fields; vector cases; `mutant-governor-skip-freshness` |
| Ceiling | over the 14 days: zero autonomous launch and zero autonomous package entering Needs you while the Needs you count was at or above the ceiling | `governor_launch` and `package_released` lines carry the count at that moment; e2e with seeded boards; `mutant-governor-skip-ceiling` |
| Weekly cap | zero weekly window whose autonomous points exceeded `autonomous_week_max_pct` at launch time | `governor_launch` fields |
| Gesture | zero launch without a pointer click other than a governor grant on an approved standing order; zero launch reachable from untrusted text | the injection battery (D-147) gains a `governor` section: proposal files and `propose_work_order` carrying a standing order's exact values, a floor edit adding a standing order (catalogue changed: no launch), a forged low quota file beside a higher probe, seeded excerpts; a fake engine counts container creations; `compile_fail` doctests on `GovernorGrant` |
| Window | zero autonomous launch outside the window; zero autonomous run live 60 s after the window's end or after the 5-hour reset its deadline was cut to | journal; e2e with a fake clock and a fake engine |
| Stop | the shortcut stops every autonomous run within 10 s with the window minimized or behind another, and leaves clicked runs and the relay running | e2e on the stop path; once on the workstation (gate 5) |
| Use | over the first 10 days of the autonomy window with the governor on: at least 70 % of the due launches of eligible orders happened (an order due inside the window launched before the window's end that day), and at least 4 autonomous runs reached a verdict | registry, journal |
| Throughput | autonomous packages imported per week, merged beside them; under 1 import in each of the two weeks means the governor costs more attention than it returns, and the close decides whether it stays on; on an API-key floor, read per USD | FR-2 verdicts, FR-1 outcomes |
| Autonomous acceptance | accepted over settled for autonomous runs, at least 60 %; under it, autonomy does worse than the clicked evidence predicted, and the close raises `acceptance_runs` | FR-2 |
| Review time | zero launch with spent + pending + estimate above the budget; actual autonomous review minutes per day reported; over budget on more than 2 of 10 days, the default estimate goes up at the close | journal |
| Held | zero held package expired (one expiry self-stops the governor); any expiry is read at the close against the ceiling and the cadences | `package_expired` lines |

The framing's exit criteria are the Reserve, Fresh quota and Ceiling rows over
14 consecutive days with the governor on and at least one standing order
eligible, read alongside v0.8 and at the latest at v1's close (section
6.1.1). Slice A closes on its own table. The v0.7 close reads Use, Throughput,
Autonomous acceptance, Review time and Held when the measure ends (P-16, with
section 6.1.1's exception).

## 8. Open Questions

0. **Owner gate (Slice B), before any launch code.** v0.6 gate 7 accepted a
   model credential for clicked workers. v0.7 removes the click and times runs
   to use up margin. The owner records, in the research notes:
   - the text of the terms the answer relies on: S5's "Advertised usage
     limits for Pro and Max plans assume ordinary, individual usage of Claude
     Code and the Agent SDK", and the plan's own terms as he reads them on that
     date;
   - for each confirmed candidate of section 2.3, its floor and account, and
     whether that account may run that project's work: on the personal floor,
     his personal account on his personal projects; on an employer plan, the
     employer's written answer, quoted;
   - on an API-key floor, his weekly USD figure for autonomous runs (FR-7).

   If no account may run any candidate and no API key is available, Slice B is
   not built.
1. **Gate (FR-5), before Slice A.** Does `claude -p "/usage" --output-format
   stream-json --verbose`, with FR-5's flags, from an empty folder on the
   workstation, print an assistant message carrying `usage_report` with
   `rate_limits.limits` rows of kind `session` and `weekly_all`? S2 documents
   the report for the SDK; S4 and S6 do not say whether `/usage` runs under
   `-p`. Check over a full working day at FR-5's planned rate (one probe per
   10 minutes, plus bursts of one per 2 minutes), and record: exit codes,
   elapsed times, each `result` line's cost, turns and output tokens (a probe
   that makes a model turn is refused), every last-known or rate-limited
   report and how it shows (S3), and whether the author's own interactive
   `/usage` showed last-known data or a rate limit that day. Also record
   whether `--setting-sources ""` and `--tools ""` change it. If the prompt
   form fails, try `--input-format stream-json` with one user message
   `/usage`. If both fail, or the day shows the author's `/usage` degraded,
   FR-5 is not built, or its budget is lowered to what the day allowed, and
   FR-4's fallback governs. The prompt form, the report's rows, a clean
   probe's result and its zero cost are verified (addendum section 1); the
   working day and the degraded forms remain.
2. **Gate (FR-4, FR-5), before Slice A.** Within one minute, does an
   interactive session's statusline (`five_hour`, `seven_day`) agree with the
   probe's `session` and `weekly_all` rows: same reset times, percentages
   within 1 point? If resets differ, the mapping is wrong and the probe is not
   used. Record whether percentages come as whole points, and any
   `weekly_scoped` row with its scope.
3. **Gate (FR-4 to FR-6), before Slice A.** Does a worker's spend show on the
   floor's account? Check: a probe; one clicked worker run with a fixed
   multi-step prompt while no other session of the account works; a probe
   within 5 minutes after it ends. The `session` and `weekly_all` percentages
   must rise. Record the rise against the run's `result` cost estimate: the
   first points-per-USD ratios (FR-6). If nothing rises, the worker's
   credential draws on another account or on the API: on `api-key`, FR-7
   applies; on `oauth-token`, the governor cannot see its own spend and Slice
   B is not built until the worker uses the floor's account. Record also
   whether the worker's stream carried a `rate_limit_event` (S2) and, if it
   carries `utilization`, which window it tracks: FR-13 then reads it as its
   live guard.
4. **Gate (FR-13), before its window-end stop.** Does v0.6 FR-14's per-run
   stop, applied mid-run, still yield a package that passes v0.6 FR-10's
   checks? If not, the window end lets autonomous runs reach their deadline,
   which FR-12 already cuts to the window's end, and FR-13's stop is dropped.
5. **Gate (FR-16), before it.** Does `RegisterHotKey` from a dedicated thread
   of the Tauri app register `Ctrl+Alt+Shift+X` on the workstation, and fire
   with the window minimized and behind a full-screen editor? If not, Stop
   autonomy stays a button and Stop all a command.
6. **Gate (FR-11), before Slice A.** Does the offset read from Windows match
   the local clock, and follow a daylight saving change (checked by setting
   the clock's time zone on a test account, or by the next transition)? If
   not, the window is read in UTC and the README says so.
7. **Precondition (Slice B).** Which templates have at least 10 settled runs,
   and what is their acceptance? Read from FR-2's computation on v0.6's
   records once Slice A runs over them. Since a run now settles at its review
   verdict, a template needs about 10 decided clicked runs, Import or
   counting Discard. Until one is eligible, Slice B waits; the PRD makes no
   provision to lower the gate for it.
8. **Decision before Slice B.** Should the governor ever start a fresh 5-hour
   window, when none is active? The candidates live on the personal floor
   (section 2.3), where the author may not work interactively every evening of
   the window, so `no_active_window` may dominate. Recommendation: decide on
   the shadow's Hold causes row. If `no_active_window` or `not_perishable` is
   under 80 %, keep the framing's condition ("a margin that would be lost").
   Over it, add a `fresh_window` option bounded by the weekly projection and
   the weekly autonomous cap, which hold whatever starts the window.
9. Should acceptance be per template and project? The framing says per
   template. Revisit if one template's acceptance differs by more than 30
   points between two projects with 5 settled runs each.
10. **Owner, for FR-1 and FR-2.** How does the owner merge `officina/*`
    branches: locally, or by pushing and merging on a forge; with fast-forward
    or merge commits, or squash; how often does he fetch? The answer says how
    much FR-1 can see, and whether "Gate on merges" is worth offering.
11. v0.6's architecture must define the zero-commit outcome of its bundle
    check (FR-2): whether `git bundle create` of an empty range yields a
    package or none. v0.7 reads either as `no_change`. Answered by v0.6
    AD-110: with no commit past the base the packer writes no bundle, and a
    worker that exited 0 ends `no_change`.
12. The roadmap order. Section 6.1.1 lets v0.8 start once v0.7's built slices
    close; the alternative the owner may choose instead is to swap v0.7 and
    v0.8, so that v0.6's clicked runs accumulate evidence while v0.8 is built,
    at the cost of the framing's order.
13. **Owner, if the shadow's share under the ceiling is under 20 %.** Should
    the ceiling count review packages and approvals only, leaving waiting
    sessions out? That departs from the framing's antechamber.
14. The values 3 seats, 30 minutes a day, 60 % over 10 settled runs, 25 %
    reserve, 70 % of the 5-hour window and 30 minutes of freshness (framing
    section 7), and 10 weekly autonomous points, 120 minutes of lead, 20
    minutes minimum, the live 5-hour stop at 80 %, 3 weekly and 10 five-hour
    points of starting allowance, a 2-point minimum bracket, 10 minutes of
    starting review estimate, 1 autonomous run at a time, the doubled cadence
    after 3 `no_change`, 14 days to read a merge, 7 days of hold, 72 hours of
    default cadence, 10 minutes between live probes, 60 minutes of default
    back-off and FR-5's probe budget are starting values. The shadow measures
    the ones the decision uses; the two-week measure measures the rest.

## 9. Addendum (2026-10-28): the critic and research helpers

The owner's approved scene prototype (prd-v0.4.md, Change Log) shows, in its
v0.5-and-later mode, a critic and a research helper that come to the
orchestrator's desk when it works on a question. This section places them in
v0.7; the architecture's section 6 says how, and `epic-7-decisions.md` D-492
and D-493 say why.

1. **Helpers are governor-launched work.** A helper is a v0.6 workshop run of
   an author-written template whose `role` is `critic` or `research`: read-only
   on the project (no package to Import, only its report), within the
   template's budget. It launches either by the author's click on a card the
   orchestrator proposed (v0.5 FR-4), or by the governor from a standing order
   whose trigger is a deterministic condition the author wrote, counted in
   FR-8's ceiling and FR-6's allowances like any autonomous run. No model
   decides to launch a helper (section 5).
2. **What a helper's result can do.** Its report is untrusted text: shown to
   the author in the review panel, and served read-only to the orchestrator
   (`officina mcp` gains a `helper_reports` read tool). It reaches a session
   only through an answer the author sends (prd-v0.5.md section 9, item 3).
3. **FR-17, Scene.** Replace "no new element" for helpers: a live helper run
   is a smaller figure without a cap, in the orchestrator's tunic, that walks
   in through the decision office's staff door when the run starts, stands at
   the orchestrator's desk while it runs and walks out when it ends. Other
   autonomous workers still sit at their project's desks, and a held package
   is still not drawn.
4. **Delivery.** Helpers belong to Slice B (they launch). Their templates and
   standing orders follow FR-10's rules and approval.
