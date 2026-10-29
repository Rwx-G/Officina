---
title: Epic 5 QA report (v0.5, The orchestrator)
author: Rwx-G
date: 2026-10-29
---

# Epic 5 QA report

## Executive summary

Gate decision: **open** (D-799, P-25). The orchestrator is built and
covered by automated tests: stories 5.1 to 5.21 are implemented (5.1's
agent halves, 5.2 to 5.18, 5.20, 5.21, and this story's code); the full
local gate replaying every job of `.github/workflows/ci.yml` passed every
step on the closing code but one timing case of the workspace run, which
passed alone (a machine shared with other agents' builds, below); and the
two audits this story ran found one defect in scope, fixed (D-686). Epic 5
cannot close on code alone: P-25 closes it
after real use, with the owner's runs of the gates' second halves, the
use metrics and question 8's values read from that use, and question 6
rechecked. The owner's items are listed with their one-liners at the end,
with the decisions he is asked to confirm.

Slice D (stories 5.22 and 5.23, the `PermissionRequest` router and the
auto-mode denials, AD-181) ships as v0.5.x, after this closure, and is
reported apart (D-578, D-581). Its entry state is below.

What closes the epic, in order:

1. The owner's gate runs: gate 1's window half (story 5.16 AC 10), which
   also opens story 5.18's entry (D-841); gate 4's owner half (E22) and
   window half (story 5.7 AC 11); gate 7's readability check on one of his
   sessions (story 5.10 AC 4); gate 8's optional interactive half.
2. Real use on a build of `main` with every story merged: the Use, Auto
   and Delivery share rows of PRD section 7 read with
   `examples/use_metrics.rs` (story 5.19 AC 4), and question 8's values
   over the shadow period and two weeks of use (AC 5).
3. Question 6 rechecked at the Claude Code version the canary last saw
   (AC 6).
4. The scene baselines with the orchestrator (story 5.18 AC 1 and AC 10:
   the `workflow_dispatch` run, the comparison, the commit, D-840).
5. The owner's confirmation of the decisions listed at the end.

Findings of this story, all decided:

1. The meeting's filter lookup missed a filter set through an `include`
   or in `config.worktree`, so a session could have the window run a
   program of its choosing on Receive: fixed, with the two cases reproduced
   before the fix (D-686).
2. The review of an `auto` request reads the "author's last prompt" of
   whichever session the request claims, and nothing binds the claim:
   recorded High (D-687), then fixed before auto approval is turned on:
   the claim binds through the caller's process ancestry or escalates
   `session_unbound`, and the event buffer and the orchestrator's folder
   joined the gate's protected folders (D-960, D-961).
3. Doctor's orchestrator lines were spread over the output and
   `review_config_invalid` failed doctor: they now print as one section,
   last, and that cause warns (D-797, to confirm).

## Evidence

Figures from the full local gate on the closing code (see "Gate on the
closing commit" below), Windows 11 host; `core-linux` in a
`rust:1-bookworm` container.

| Evidence | Where | Result |
|---|---|---|
| The session variables (AD-80) | research notes E84 | verified by the agent, 2026-10-29, 2.1.312: names only; `SESSION_ENV_REMOVED` cites it |
| E19 `--bg`, trust and agent view | E19 | verified (agent), items 1 to 3 hold |
| E20 the bare `--` (gate 2) | E20 | verified (agent): both forms pass byte for byte; `--resume` from the binary's folder stops at the trust dialog (D-635) |
| E21 resume delivery (gate 3) | E21 | verified (agent): delivered into a live idle session, refused on one waiting on a question |
| E22 work order permissions (gate 4) | E22 | agent half verified: a `--bg` session takes the supervisor's environment (D-127's branch, README states it); **owner half and window half open** |
| E23 `claude logs` (gate 7) | E23 | verified (agent): exit 0 in 0.89 s; agent readability check recorded; **owner readability check open** |
| E24 the orchestrator's flag set (gate 1) | E24 | agent half and owner half passed (containment holds; D-690 to D-693); **window half open** (story 5.16 AC 10) |
| E25 memory files in the review call | E25 | verified (agent): the parent's `CLAUDE.md` did not load; D-164's first branch |
| Gate 8, auto mode and `officina run` | E85 | passed in case a in print mode (agent): nothing added for the relay (D-573); **interactive half open, optional** |
| Injection battery | `crates/officina/tests/injection.rs`, `tests/vectors/injection.json` | every entry point AD-95 and AD-182 name holds a section of `SECTIONS` (the card launch, the proposal file with work orders and answers, the MCP proposal tools, the seeded excerpt reaching the review and the MCP outputs, the review-approved run); the fake claude counts `--bg` and `--resume` spawns: none before the click, one after, its vector equal to the rebuild; `every_section_family_and_refusal_has_a_case` iterates the families and refusals |
| Mutants job | CI `mutants`; the gate's mutants step | each `mutant-*` feature of `officina-core` compiles and the battery fails on the case that guards it; the control run passes; `MUTANTS_ENABLED` is a compile-time assertion in the app (D-148) |
| Restart e2e | `crates/officina-host/tests/proposals_crash.rs`, `crates/officina/tests/mcp.rs::killing_officina_mcp_during_propose_recap_loses_and_duplicates_nothing` | release, ignored by default, run by the gate: no proposal lost or duplicated across kills, window restarts and an orchestrator restart |
| Zero write | `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir` | covers a launch, an answer and an answer proposal (Windows: console starts), a review, `officina mcp` over every tool and an orchestrator start, plus `doctor --permissions` |
| Use metrics | `officina-core::orders::metrics`, `examples/use_metrics.rs` | the reader is built and tested (D-688); **no real use yet**: run on this workstation's own Officina home on 2026-10-29, it counted no day with a work order, an answer or a decision, the installed window being v0.4's |

## Gate on the closing commit

The full local gate on story 5.19's code (`c9ea928` and the README test of
`19bc2fd`; the later commits change documentation only), run while other
agents' builds and a Docker build loaded the same machine:

| Gate | Result |
|---|---|
| `cargo fmt --check`; clippy `-D warnings`, workspace with `officina-core/ts` | clean |
| Rust unit, integration, CLI end to end, doc tests, vector sets, TypeScript bindings drift | 1,642 passed, 1 failed, 14 ignored (the release-only tests below); the failure, `spawn.rs::a_work_order_timeout_kills_only_claude`, is a timing case: the whole `spawn.rs` passed alone at once (15 passed) |
| The launch half without the authority (`cargo test -p officina-host --lib spawn`) | 7 passed |
| Perf, release | 5 passed; hook own cost 13.3 ms (budget 150 ms), gate own cost 3.0 ms (budget 15 ms), gate in process p95 3.9 ms |
| Lexer against Git Bash; approvals stress; registry crash; proposals crash and the `officina mcp` kill test (release) | pass |
| Authority and registry markers, registry dependencies, no bundled roots | pass |
| Frontend: svelte-check, ESLint, Vitest | 0 errors, clean, 350 passed in 26 files |
| e2e fixtures from the real CLI, Playwright `--repeat-each=3` | 1,326 passed, 6 skipped (the baseline cases, `fixme` until D-508 and D-840) |
| Mutants | each of the three `mutant-*` features caught by its guarding case; the control run passed |
| `cargo deny`, `pnpm audit` | `cargo deny` ok (advisories, bans, licenses, sources); `pnpm audit` 1 low, ignored with its dated reason |
| `core-linux` in `rust:1-bookworm` | fmt and clippy clean, 1,462 passed, 0 failed, 12 ignored, the `--lib spawn` step green; a first run stopped on `spawn.rs::every_kind_reaches_the_fake_claude_as_its_golden_vector`, "Text file busy": Linux refuses to run a freshly copied fake while another thread still holds it open (the stability work's "link the fake claude rather than copy it" fixes it, not yet on main) |

Stability: three earlier full runs of this story's gate each stopped on
one timing case, never the same one, none in this story's code:
`injection.rs::{card_launch_cases, proposal_file_cases}` (a collector's
first board not within 10 s), `board.rs::hanging_claude_times_out_instead_of_blocking`
(its elapsed bound) and `spawn.rs::a_work_order_deadline_ends_the_child_alone`;
each passed alone at once. The machine was shared with two other agents'
builds and a Docker build during every run. Story 5.21 recorded the same
class of load flake in `watch.rs::queued_reviews_are_paced_5_s_apart`.
The stability work on main's test races (D-880 to D-889) had not reached
`origin/main` when this report was written; its commits sit on a branch
of their own.

## Story status

| Story | Status | What is open |
|---|---|---|
| 5.1 Workstation gates | in progress | gate 4's owner half (E22); gate 8's optional interactive half; Slice D's entry checks SD-1 to SD-5 (AC 11, gate no v0.5 story) |
| 5.2 Spawn module | review | |
| 5.3 Untrusted text | review | |
| 5.4 Templates and catalogue format | review | |
| 5.5 Work order validation | review | |
| 5.6 Migration 2 and records | review | |
| 5.7 Cards and launch | in progress | AC 11, gate 4's window half |
| 5.8 Briefing and the Orchestrator view | review | D-677, D-679 to confirm |
| 5.9 Composer and cards | review | D-680 to confirm |
| 5.10 Meeting, receive | review | gate 7's readability on one of the owner's sessions |
| 5.11 Meeting, answers | review | |
| 5.12 `auto` in run and approvals | review | |
| 5.13 Review call and worker | review | |
| 5.14 Shadow gate, toggle | review | |
| 5.15 `officina mcp` | review | D-770 to confirm |
| 5.16 Orchestrator session | in progress | AC 10, gate 1's window half |
| 5.17 Proposals and restart | review | |
| 5.18 Orchestrator in the decision office | in progress | AC 1 and AC 10 (baselines, D-840); waits on 5.16 AC 10 (D-841) |
| 5.19 Docs, doctor, use and QA | in progress | ACs 4 to 7: real use, question 8, question 6, the owner's gate halves |
| 5.20 Permission modes in doctor | review | |
| 5.21 Answer proposals | review | |
| 5.22, 5.23 (Slice D, v0.5.x) | approved | after this closure |

## Slice D's entry state (D-578)

| Item | State |
|---|---|
| SD-1 to SD-5 (the owner's workstation checks) | not recorded; the setup and each check are written in story 5.1's Completion Notes (AC 11) |
| SD-6 (which requests the window routes) | the owner's decision, after SD-1 to SD-5 |
| SD-7 (routed prompts in v0.7's budget) | the owner's decision, recorded for v0.7, not blocking |

Stories 5.22 and 5.23 do not wait on them (D-581); the routing story is
written only once SD-1 to SD-6 are answered.

## Audits

### Security, centred on principle 8

Run by the project's `security-auditor` agent on `3c681b9`, read-only:
the launch path, untrusted text, the spawn environment, the containment
of Officina's three sessions (the orchestrator, the review call, the
triage call), proposal ingestion, the relay's `auto` class and the
injection battery. Two High, one Medium, four Low, five Info.

Held, verified by the auditor in the code: argument quoting and `--`
before every text argument; the `Answer` and `WorkOrder` rules; the shim
refusal for every kind before anything starts; the contained allow list,
with `OFFICINA_*` and `SESSION_ENV_REMOVED` excluded and tested on a
seeded base; the orchestrator's flag set (E24); the review prompt's
framing (nonce, escaped JSON line, schema-only output); card checks and
consumption serialized on the collector thread; no `{@html}`, a CSP
without inline script; the release marker check on `officina.exe`.

| Finding | Severity | Outcome |
|---|---|---|
| The meeting's filter lookup missed a filter reached through `include.path`, `includeIf` or `config.worktree`: Receive could run a session's clean filter | High | **fixed** (D-686): `git config --includes --show-scope`, local and worktree scopes counted; `tests/git.rs` cases for the include and the worktree config, the include case red before the fix |
| The review's "author's last prompt" follows the requesting session's unverified claim (session id from the environment, forged event lines, a child `--bg`) | High | **fixed** (D-960, D-961): `officina run` records its process ancestry, the review runs only for a live interactive session, or a work order's background session, among those ancestors, otherwise `session_unbound` with no call; `events/` and `orchestrator/` are protected folders; `auto.rs::a_forged_session_claim_is_never_reviewed` red with the rule disabled |
| A proposal file written by any process shows as the orchestrator's | Medium | **fixed in the window** (D-965, D-961): the labels name the proposals folder, and a guarded session's shell cannot name it; signing stays in the backlog |
| Proposal limits counted from the id's time | Low | **fixed for the tools** (D-964): ids dated ahead no longer count, `mcp.rs::proposals_dated_ahead_do_not_reach_the_hourly_limit` red before; past-dated files at ingestion stay in the backlog |
| A review's approval re-reads the request file | Low | not a defect: the id binds 64 bits of the key and `rebuild` checks both (D-687) |
| A console launcher outside the lint | Low | **fixed** (D-963): in `disallowed-methods`, its caller holds the `expect` |
| `Card.cwd` unmarked in the orchestrator's tools | Low | **fixed** (D-962): marked inside an MCP output, `mcp_output` case `card_cwd` |
| `event.isTrusted` not read by the pointer check | Info | backlog (defence in depth) |
| A proposed answer is model text on a command line by design | Info | stated in the README beside the click it needs |

Two backlog items changed state: the Epic 3 question whether
`--setting-sources ""` stops a parent `CLAUDE.md` is answered for the same
flag set by E24 and E25 (one triage probe run would close it); D-439's
triage probe rerun is answered in part by E25's real contained call. D-760
and D-850 were not re-reported: the spill and the pacing hold in the code.

### Quality

Run by the project's `quality-auditor` agent on `3c681b9`, read-only: no
Critical, two High, ten Medium, eleven Low, six Info. Core stays pure (no
filesystem, process, clock or environment in `officina-core`), no
unjustified lint waiver, no TODO, no `{@html}` or `any`, and every vector
set has a coverage test over its enums. The findings are maintainability,
none a wrong behaviour this story could prove: the review worker repeats
one escalation block five times and drops the `set_reviewing` errors; the
answer grant is one 130-line function; two causes of
`auto_vectors.rs::every_cause_has_a_case` are seeded from the vector file
itself; constants are restated in prose and test names; three
`compile_fail` proofs carry no error code; the large modules. All are in
`docs/backlog.md` (D-799). Two were checked and set aside: the zero-write
answer leg accepts `not_confirmed`, which a console that started gives
(a start that failed gives `spawn_failed` and fails the assertion), so the
leg is not vacuous; and the answer grant accepting a proposal before its
`claude` check cannot be reached, since a collector without `claude` lists
no background session to answer.

### Test coverage against AD-95

| AD-95 row | Proof |
|---|---|
| core unit | `relay/catalogue.rs` loops over `IssueKind::ALL`; `orders_vectors.rs` (validate, cards, briefing, proposals); `auto_vectors.rs` (prompt, output, baseline, rule, stats, causes); `meeting.rs` doctests and tests; `mcp.rs` and `mcp_vectors.rs`; `orders/metrics.rs` |
| type proofs | the `compile_fail` doctests of `untrusted.rs` and `orders/work_order.rs` (with error codes), `relay/template.rs`, `spawn.rs`, `relay/auto.rs`, `registry/read.rs` (three without a code: backlog) |
| injection battery | `injection.rs`, every section and family covered |
| mutation | CI `mutants`, three features, inverted status, named guarding case |
| spawn | `crates/officina/tests/spawn.rs` over `VectorKind::ALL`; `spawn_vectors.rs`; `spawn_lint.rs`; the no-feature `--lib spawn` step on both jobs (D-128) |
| fake claude | modes `bg`, `resume`, `review-*`, `logs`, `interactive`, `orchestrator`, plus `invalid`, `hang`, `exit1` |
| auto | `crates/officina/tests/auto.rs`, `crates/officina-host/tests/{authority,registry_review}.rs` |
| MCP | `crates/officina/tests/mcp.rs` with the scripted client |
| restart | `proposals_crash.rs`, `mcp.rs`'s kill test |
| zero write | `quota_doctor.rs` (above) |
| frontend | Vitest `components.test.ts` (cards, composer, meeting, proposals, proposed answer), `lib/orchestrator.test.ts`, the scene's tests; Playwright `orchestrator.spec.ts`, `meeting.spec.ts`, `scene.spec.ts`, `stable-size.spec.ts`, `--repeat-each=3` |

## Traceability

### Functional requirements

| Requirement | Stories | Proof |
|---|---|---|
| FR-0 Gates first | 5.1 | E19 to E25, E84, E85; `examples/gates_v05.rs` for the owner's halves |
| FR-1 Briefing | 5.8, 5.17 | `orders_vectors.rs::{briefing_cases_match, every_change_and_run_kind_has_a_briefing_case}`, `orchestrator.spec.ts` |
| FR-2 Templates | 5.4, 5.5 | `relay/catalogue.rs`'s `IssueKind::ALL` cases, `catalogue_hash.rs`, `relay/doctor.rs::tests::{template_issues_are_listed_with_their_lines, templates_awaiting_approval_are_listed}` |
| FR-3 The launch path | 5.2, 5.3, 5.5 | `spawn_vectors.rs`, `crates/officina/tests/spawn.rs`, `orders_vectors.rs::validate_cases_match`, the `compile_fail` proofs, `spawn_lint.rs` |
| FR-4 One gesture | 5.6, 5.7, 5.9 | `injection.rs::card_launch_cases`, `orders_vectors.rs::card_cases_match`, `components.test.ts > WorkOrderCard`, `orchestrator.spec.ts`, `crates/officina-host/tests/registry_orders.rs` binding cases |
| FR-5 Receive | 5.10, 5.17 | `meeting.rs` tests, `crates/officina-host/tests/git.rs`, `watch.rs::a_folder_from_an_event_line_runs_no_git`, `meeting.spec.ts` |
| FR-6 Answer | 5.11, 5.21 | `meeting.rs` (`Answer::parse`, `delivery`), `watch.rs` answer and confirmation cases, `meeting.spec.ts` |
| FR-7 Auto review | 5.12, 5.13, 5.14 | `auto_vectors.rs`, `auto.rs::each_review_mode_records_its_outcome`, `injection.rs::review_excerpt_cases` |
| FR-8 Fail closed | 5.13 | `auto.rs::each_precheck_cause_escalates_without_a_call` (over `EscalationCause::ALL`), `authority.rs::a_burst_cannot_approve_past_the_hourly_cap` |
| FR-9 Blind shadow, rule baseline | 5.13, 5.14 | `auto_vectors.rs::stats_cases_match`, `registry_review.rs::{turning_on_is_refused_without_the_gate, a_mark_turns_approval_off_and_resets_the_mode}`, `auto.rs::rule_mode_makes_no_call` |
| FR-10 Only Officina's tools | 5.16 | the `orchestrator` case of `tests/vectors/spawn.json`, E24; the window half open |
| FR-11 `officina mcp` | 5.15, 5.21 | `crates/officina/tests/mcp.rs`, `mcp_vectors.rs`, `injection.rs::{mcp_tool_cases, mcp_output_cases}` |
| FR-12 Proposals | 5.17, 5.21 | `orders/proposal.rs` tests, `proposal_ingest.rs`, `injection.rs::proposal_file_cases`, `orchestrator.spec.ts` |
| FR-13 The orchestrator's room (as section 9 item 1 supersedes it) | 5.18 | `layout.test.ts`, `scene.spec.ts`, `stable-size.spec.ts`; baselines open (D-840) |
| FR-14 Restart without loss | 5.6, 5.16, 5.17 | `proposals_crash.rs`, `mcp.rs`'s kill test, `registry_retention.rs` |
| FR-15 Doctor | 5.2, 5.4, 5.7, 5.11 to 5.14, 5.16, 5.17, 5.20, 5.19 | `crates/officina/tests/doctor_orchestrator.rs::{the_orchestrator_section_lists_each_check_once, warnings_alone_exit_0, a_shim_exits_1, a_non_empty_workdir_exits_1}`, the host `doctor.rs` and `relay/doctor.rs` tests |

PRD section 7's rows: Gesture (the battery, the mutants, the type
proofs), Restart (the restart e2e), Containment (E24 and the vector; the
window half open), Auto fail-closed (`auto.rs`), Delivery check (E21 and
the `Answer` vector), Zero write (`quota_doctor.rs`) are proved; Use, Auto
and Delivery share wait on real use (AC 4).

### PRD section 9 (the addendum, D-577's owners)

| Item | Story | Proof |
|---|---|---|
| 1 The orchestrator in the decision office | 5.18 | `layout.test.ts`, `scene.spec.ts`; baselines open |
| 2 Briefing at the reception counter | 5.18 | `layout.test.ts`'s once-per-order cases, `walk` cases |
| 3 Answer proposals | 5.21 | `watch.rs` proposal cases, `injection.rs` answer cases, `components.test.ts > ProposedAnswer` |
| 4 The review's input | 5.13 | `auto_vectors.rs::prompt_cases_match`, `auto.rs::an_answer_excerpt_never_reaches_the_review` |
| 5 `check` mandatory on `auto` | 5.4 | `relay/doctor.rs::tests::an_auto_action_without_a_check_is_listed`, `IssueKind` cases |
| 6 The judge stays narrow | 5.13, 5.15 | the review limited to catalogue requests; `mcp.rs::no_tool_spawns_anything_but_claude_agents` |
| 7 Auto mode, the author's decision | 5.20 | `doctor.rs` permission tests, `quota_doctor.rs::doctor_permissions_writes_no_settings_file` |
| 8 Gate 8 | 5.1, 5.20 | E85; `doctor.rs::tests::the_gate_8_addition_is_the_recorded_one` |
| 9 Always the author | 5.19 | README "Who decides what"; no Officina code decides those cases (D-583) |
| 10 The `PermissionRequest` router | Slice D | v0.5.x, after this closure |

### Architecture decisions

| AD | Stories |
|---|---|
| AD-73 module layout | 5.2, 5.5 |
| AD-74 `Untrusted<T>` | 5.3 |
| AD-75, AD-76 templates, format | 5.4 |
| AD-77 `WorkOrder` | 5.5 |
| AD-78 card store, one gesture | 5.7, 5.9 |
| AD-79, AD-80 spawn module, environment | 5.1 (E84), 5.2 |
| AD-81 launch, binding, filing | 5.6, 5.7 |
| AD-82 migration 2 | 5.6 |
| AD-83 briefing | 5.8 |
| AD-84 meeting | 5.10 |
| AD-85 answers | 5.11 |
| AD-86 to AD-88 `auto` | 5.12, 5.13, 5.14 |
| AD-89 orchestrator session | 5.16 |
| AD-90 `officina mcp` | 5.15 |
| AD-91 proposal files | 5.17 |
| AD-92, AD-93 payloads, frontend | 5.8, 5.9, 5.14, 5.17, 5.21 |
| AD-94 doctor | 5.2, 5.4, 5.7, 5.11 to 5.14, 5.16, 5.17, 5.20, 5.19 |
| AD-95 test strategy | every story; 5.19 AC 8 |
| AD-96 gates | 5.1; second halves in 5.7, 5.10, 5.13, 5.16 |
| AD-179 the decision office | 5.18 |
| AD-180 the review's input, the mandatory check | 5.4, 5.13, 5.20 |
| AD-181 the router | Slice D (5.22, 5.23) |
| AD-182 answer proposals | 5.21 |

Each story's Completion Notes name the test proving each AC.

## Departures (P-24)

The stories follow these departures from the addendum or the PRD, each
with its reason in its decision; neither document was edited:

| Decision | Departure |
|---|---|
| D-126 | AD-96's gates 1, 4 and 7 run in parts |
| D-128 | AD-80's removals for the contained kinds only; agent view on for every kind but the orchestrator; launch kinds accept only an `.exe` |
| D-131 | AD-76's "format 2" and its format 1 fixture |
| D-133 | AD-82 extended in place until v0.5 ships, its extra columns, keys and indexes |
| D-139, D-168 | AD-85's delivery reads the `claude` version; the delivery `CopyOnly` |
| D-140 | AD-87's causes gain `workdir_not_empty` |
| D-141 | AD-87 and AD-88 in rule mode |
| D-142 | the review's pacing waits instead of escalating |
| D-143 | AD-88's cases count shadow rows only |
| D-145 | FR-11's "its own" proposals read "every orchestrator session's" |
| D-146 | Epic 4's D-102 and D-104 for the eighth cell |
| D-148 | the app's `MUTANTS_ENABLED` test is a compile-time assertion |
| D-151 | `auto` gains a check at consumption |
| D-152 | a proposal's subject default excludes a proposed new name |
| D-155 | misnamed and rate-limited files are deleted, not stored invalid |
| D-157 | AD-25's file table gains `<id>.decided` |
| D-164 | the memory-file refusals, not needed (E25, first branch) |
| D-166 | presence and exclusion narrowed to the exact workdirs |
| D-561 | the contained kinds take v0.3's allow list |
| D-562 | no kind runs a shim |
| D-565 | the header chips carried by the Orchestrator view switch |
| D-566 | the received desk read as the received figure |
| D-574 | `doctor --permissions` prints whole keys; the per-session mode read as per-kind counts |
| D-575 | AD-182's sent text is the text the collector stored |

**FR-10's reading of principle 2** (story 5.16's Dev Notes): Officina's
own sessions load no user or project configuration (`--setting-sources
""`), so their flags displace nothing of the author's; decided on
2026-10-28, listed here for the owner's confirmation.

## Side fixes found during the epic

| Fix | Decision | Where |
|---|---|---|
| The relay journal dropped a line when its lock stayed held past 2 s; such lines now go to a spill file read, pruned and watched with the day file | `epic-2-decisions.md` D-760 | journal writer, CHANGELOG Fixed |
| Two queued reviews could start less than 5 s apart: the spacing counted from the tick that submitted the call, now from the call's own start | D-850 | `relay/review.rs`, CHANGELOG Fixed |
| A fake claude log line written in pieces interleaved with another process's line under load (stories 5.17 and 5.21): each line is now one append | story 5.21, `5e93f20` | `officina-fake-claude.rs` |
| A test read a refused review's row before its cause was written (story 5.21): it now waits for the cause | story 5.21, `ded2b9f` | `crates/officina/tests` |
| The meeting's filter lookup missed includes and the worktree config | D-686 | `crates/officina-host/src/git.rs` |
| The review's input followed an unverified session claim | D-960, D-961 | `relay/review.rs`, `relay/run/request.rs`, `process_tree.rs`, `relay/gate.rs` (core), CHANGELOG Security |
| `Card.cwd` unmarked, the console launcher outside the lint, proposal ids dated ahead counted, the proposals' origin labelled as the orchestrator | D-962 to D-965 | `board.rs`, `clippy.toml`, `orders/proposal.rs`, the meeting and Proposals components, CHANGELOG Security |

## Open owner items

Each with its one-liner, as the stories' Completion Notes give it.

1. **Gate 4's owner half (E22, story 5.1 AC 5).** From the repository
   root, in a console of his own: `cargo build -p officina-host --features
   window-authority --example gates_v05`, then
   `target\debug\examples\gates_v05.exe gate4`. Report:
   `target/gates-v05/report-gate4.md`, recorded in E22. It runs again once
   he switches his default mode (D-573).
2. **Gate 4's window half (story 5.7 AC 11, D-673).** With a template
   approved on a trusted scratch repository: open the Orchestrator view,
   choose the template, the project and each value, click Launch with the
   mouse, leave the window open ten minutes, and record in E22 the
   outcome, `printed`, whether the row binds and the variable names the
   session's Bash sees.
3. **Gate 1's window half (story 5.16 AC 10).** Build the window from the
   branch (with `officina.exe` beside `officina-app.exe`), then from his
   own console: `cargo build -p officina-host --features window-authority
   --example gates_v05` and `target\debug\examples\gates_v05.exe
   gate1-window`; the report `target/gates-v05/report-gate1-window.md`
   goes to E24. A failure reopens FR-10 (D-126).
4. **Story 5.18's entry (D-841).** Waits on item 3; then AC 1 and AC 10:
   the `workflow_dispatch` run with `update_snapshots`, the comparison
   with the prototype's target mode, the commit of the baselines (D-840).
5. **Gate 7's readability (story 5.10 AC 4, D-126).** On one of his
   background sessions: `cargo run -p officina-host --example logs_probe
   -- <id>`, the id from `claude agents --json`; readable keeps the
   section, unreadable removes it and the kind.
6. **Gate 8's interactive half (E85, optional).** `cargo build -p
   officina`, `cargo build -p officina-host --features window-authority
   --example gates_v05`, then `target\debug\examples\gates_v05.exe gate8`;
   report `target/gates-v05/report-gate8.md`.
7. **Slice D's entry checks SD-1 to SD-5 (story 5.1 AC 11, D-578).** In a
   new empty scratch folder trusted once, with the logging
   `PermissionRequest` hook and the steps written in story 5.1's
   Completion Notes; each becomes an entry numbered when written.
8. **The use metrics (story 5.19 AC 4, PRD section 7).** After real use
   on a build of `main` with every story merged: `cargo run -p
   officina-host --example use_metrics` (with `OFFICINA_HOME` set as for
   the window when it is not the default), then record in the research
   notes Use (days with a work order started or an answer sent, against
   10 working days), Auto (decisions per day in the week before Slice B
   shipped and in the last week of shadow) and Delivery share (confirmed
   over sent, per source).
9. **Question 8's values (story 5.19 AC 5).** Over the shadow period and
   two weeks of use, record the highest approvals per hour per `auto`
   action, the 5-hour use at each `quota_high` escalation, the shadow
   cases and disagreements and `rule_diff` (`officina doctor`'s `relay
   auto review` line and the Relay view's Auto approval section give
   them); a value the facts contradict changes with its constant and a
   decision.
10. **Question 6 (story 5.19 AC 6, P-25).** Read the version the canary
    last saw (`officina doctor`'s `claude` line), check Claude Code's
    documentation at that version for a surface that delivers text to a
    live interactive session as the author's prompt, and record it in the
    research notes; nothing changes unless one exists, which becomes a new
    PRD question.

## Decisions for the owner to confirm

From `epic-5-decisions.md`, every decision marked "(To confirm.)":

- D-635: the `Answer` vector runs from the session's folder in both deliveries.
- D-638: a failing `[auto_review]` is canonical as `model: null`.
- D-677: closing the window records the briefing's reading whatever view is shown.
- D-679: no "contract held" in the header; 8 px view button sides.
- D-680: the composer chooses nothing but a single-entry list.
- D-687: the audit's remaining findings, now fixed by D-960 to D-965 but the Medium's signing and the past-dated ingestion limit, which stay in the backlog.
- D-691: the orchestrator vector carries no first prompt.
- D-770: `no_excerpts` withholds prompt and answer excerpts in `officina mcp`, not past command excerpts.
- D-773: the orchestrator's presence from `claude agents` alone.
- D-780: the Orchestrator view's presence block above the briefing.
- D-785: deleted proposal files counted in the window's memory.
- D-787: the proposals' statuses and expiry as built.
- D-790: the proposal list between the briefing and the composer.
- D-793: the battery's `proposal_file` section as built.
- D-794: an invalid proposal keeps its values with the row.
- D-797: doctor's `claude` line outside the Orchestrator section; no printed heading.
- D-802: Receive also on a background card idle in Done.
- D-815: `meeting_answer.outcome` gains `copied`.
- D-830: the screens and the whiteboard carry words only while the orchestrator is live.
- D-832: the prototype's staff chain.
- D-833: the pending count in the view switch button's corner.
- D-860: "in Your turn" read as live idle in Done for a background session.
- D-865: the proposed answer's fixed slot in the panel.

Confirmed only in part, from the same file: D-565 (provisionally; the
owner's final validation in real use) and D-574 (in principle). And FR-10's
reading of principle 2 above. No decision of another file that the Epic 5
work touched is marked to confirm (`epic-2-decisions.md` D-760 is not).
