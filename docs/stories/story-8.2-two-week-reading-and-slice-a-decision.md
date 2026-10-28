# Story 8.2: Story 2.8's two weeks at work and the Slice A decision

**Status:** Approved

## Story

As the author who keeps production writes behind a click with their dry run on the card (story 2.8),
I want two working weeks of that flow read from the journal and from my own log, the dry run's stability measured, and the decision on per-project scope recorded with its trigger,
so that a scope engine is built only when use shows a write that reached production without my click, or I ask for it, and not on a guess (PRD section 7's first table, FR-0).

## Acceptance Criteria

1. **Entry condition and timing** (FR-0, P-72, D-400, D-404; review F8, F22). Stories 2.8 (ACs 18 to 21) and 2.13 (approval lines naming their action) have merged; this story's code (AC 2, AC 4) lands then, exempt from D-400's ordering, and is on the build the owner runs before the reading starts; gate 0 is recorded (8.1). The reading covers 10 working days the owner names, all after 2.13 merged and within the journal's 90 days (AD-32). Proof: the entry's dates against the merges of 2.8, 2.13 and this story's code.
2. **Executable rows** (PRD section 7 Use, Decision time, Void; D-404; review F8, F22). Host `relay::journal::read_days(home, days)` reads the journal lines of a list of UTC days (the two-day `read_recent` serves no metric). Core's `relay::metrics` computes from them, joining decisions to requests by `approval_id` on story 2.13's `project`, `action` and `dry_run` fields, the class read from the approved snapshot that the day's latest `catalogue_approved` line names: Use (approvals of `human` actions naming a check), Decision time (median from `approval_requested` to `approval_given` or `approval_rejected` of those requests, in seconds) and Void (D-404's numerator over D-404's denominator, a `check_required` refusal counting as an attempt with no request); `crates/officina-host/examples/relay_metrics.rs` prints them. Proof: core `crates/officina-core/src/relay/metrics.rs::tests::{days_are_utc, use_counts_human_with_check_only, decisions_join_requests_by_approval_id, a_catalogue_changed_mid_period_is_read_per_day, decision_time_is_the_median_of_decided_requests, void_counts_a_check_required_refusal_as_an_attempt, an_approval_unconsumed_after_15_min_is_void, an_approval_consumed_in_time_is_not_void}`; host `crates/officina-host/tests/relay_journal.rs::read_days_reads_exactly_the_named_days`.
3. **Owner rows** (PRD section 7 Production writes, Hand writes, Mailbox; D-404). The owner records, counts and causes only: every production change over the 10 days from the servers' own records (an Ansible callback log or the hosts' change logs, whichever exists), each matched to an approved `human` request in the journal or explained; his hand writes on guarded projects' targets with the gap each names (a missing action, a missing check, an approval too slow); the production subjects carried from start to finish by approvals with their dry run, with no output pasted. Proof: the reading's research notes entry.
4. **Question 4, drift** (PRD section 8 question 4, D-404; owner). Over the same days, the owner reruns each approved action's dry run a few minutes later on unchanged lab targets; `crates/officina-host/examples/dry_run_drift.rs <run id> <run id>` reads the two kept outputs and calls core's `relay::metrics::same_after_stripping`, which compares them after v0.5's escape stripping (AD-84); it prints `identical` or `different` with the byte lengths, never the text. The entry records the share identical. Proof: core `metrics.rs::tests::{outputs_differing_only_in_escapes_are_the_same, a_changed_byte_is_different}`; the example built in CI; the entry.
5. **The decision** (PRD section 7, section 6.1.1, D-404). The entry and `epic-8-decisions.md` record, with the date: Slice A built and which trigger held first (a production write through a non-`human` action, the author's request for per-project scope, or gate 0 above 10 write actions in one project, which may be recorded before the two weeks end), or Slice A not built and why, with section 6.1.1's exception named (P-72). Decision time above 3 minutes is recorded as holding 8.7 and 8.8 until the card is revisited; Void above 20 % names its causes and the values changed. Proof: review of the entry and of `epic-8-decisions.md`.
6. **If Slice A triggers** (PRD question 0 (b), question 10, D-403). Before 8.6 starts, the owner records per guarded project the target values and host names in scope, the values kept on order and which actions are reads (as counts and redacted patterns in the entry; the real values go in his own floor file only), and confirms question 10's fail-closed defaults. Proof: the entry.
7. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: `examples/relay_metrics.rs` and `examples/dry_run_drift.rs`. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/relay/metrics.rs` and its tests; host `journal::read_days` (AC 2)
- [ ] `examples/relay_metrics.rs`, `examples/dry_run_drift.rs` (AC 2, 4)
- [ ] The owner's two weeks, his log and the servers' records; the entry (AC 1, 3, 4)
- [ ] The decision; question 0 (b) and question 10 if triggered (AC 5, 6)
- [ ] `CHANGELOG.md` (AC 7)

## Dev Notes

- Requirements: PRD section 7's first table, section 6.1.1, section 8 questions 0 (b), 4, 10, 11. Decisions: AD-165 (manual rows), AD-166. Epic decisions: P-72, D-401, D-403, D-404.
- Seams (D-400): story 2.8's journal lines (`approval_requested`, `approval_given`, `approval_rejected`, `refused` with `check_required` and `check_not_satisfied`, `run_started.approval_id`) and kept outputs; story 2.13's approval fields (`project`, `action`, `dry_run`); the host journal reader (2.5, 2.6, 2.9), which gains `read_days`; v0.5's escape stripping (AD-84, 5.10). No Epic 8 seam.
- Waits for: stories 2.8 and 2.13 merged; 8.1's gate 0. Its code is exempt from D-400's ordering (it must exist before the owner's two weeks, since kept outputs are pruned after 7 days); its reading waits for the two weeks. This story does not hold Slice B or 8.3 to 8.5: only Slice A (8.6, 8.7) waits for its decision, and 8.8 for the Decision time row only (P-71).
- Without story 2.13's fields an approval id cannot be attributed to an action: request files are pruned after 24 hours and only consumed approvals have a `run_started` naming their action (review F8).
- `relay::metrics` is the executable form of the first table's "how measured": the numbers in the entry come from it, never from a hand count. 8.16 extends it with Slice A's and Slice B's rows.
- "Expired" in the Void row means an approval given and not consumed within `DECISION_VALID_MS`; a pending request that nobody decided within `PENDING_MS` is not void, it is unanswered, and is reported apart.
- No employer content: the entry carries counts, causes and redacted patterns; the owner's floor holds the real scope values.

## Testing

- Core unit: `crates/officina-core/src/relay/metrics.rs`.
- Host integration: `crates/officina-host/tests/relay_journal.rs`.
- Host examples: `crates/officina-host/examples/{relay_metrics,dry_run_drift}.rs`, built in CI (`cargo build --examples`).
- Manual: the reading's research notes entry, the decision.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the metrics join decisions to requests on story 2.13's fields, and the 10 days start after 2.13 merged (F8, D-404, P-74); this story's code lands before the two weeks, exempt from D-400, with `journal::read_days` and a reading within the journal's 90 days (F22, D-400, D-404)
