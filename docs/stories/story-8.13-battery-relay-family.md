# Story 8.13: The escape battery's relay family (Slice B)

**Status:** Approved

## Story

As the author who claims a boundary only where evidence shows it,
I want the escape battery to play, through a real mailbox and the real admission, every way a worker might ask the host for more than a listed read, against a catalogue whose programs do not exist and facts that touch none of my own state,
so that a launch rests on a battery that tried the relay too, each attempt can fail only for its own reason, and a battery older than the relay family no longer counts (FR-8).

## Acceptance Criteria

1. **Entry condition** (P-72, D-1313). 8.12 has merged; v0.6's escape battery (6.10: `battery::ATTEMPTS`, `BatteryFamily`, the phases, `is_valid`, `tests/vectors/battery.json`) is on `main`. Proof: the sprint status.
2. **Attempts** (AD-159, FR-8 as amended by amendment 9, D-420). `BatteryFamily` gains `relay`; `battery::ATTEMPTS` gains its entries, seeded from FR-8's list (an action outside `relay_actions`; a write of each class; a dry run on an out-of-scope target and on a kept-on-order value; a parameter starting with `-` or holding a `..` segment; a malformed and an oversized request; a flood past the rate; a request after the deadline; a request while the relay is stopped), plus D-426's program-read value and D-428's host-pattern target, each naming its expected `WorkerRefusal`; from then on `ATTEMPTS` is the list. Proof: section `attempts` of `tests/vectors/battery.json` through `crates/officina-core/tests/battery_vectors.rs::{every_family_has_an_attempt, every_attempt_has_a_case}` (iterating `BatteryFamily::ALL` and `ATTEMPTS`).
3. **The battery's own facts** (AD-159, D-420, review F19). The window answers the battery's mailbox through the same `admit`, against `RunRelayFacts` of the battery's own: the battery id; core `battery::relay_catalogue()`, an approved snapshot built in core, never the author's, whose every program is a sentinel path that does not exist, on the `native` runner so the control request's 127 needs no WSL, standing as both the approved and the current floor; a synthetic project; a stop flag file under `<home>/workshop/battery/`, never the author's `relay/stopped`; a deadline 30 s after phase 1 starts. One control request is admitted, reaches the runner and ends 127 on its sentinel, which shows the channel works end to end. Proof: core `crates/officina-core/src/workshop/battery.rs::tests::{every_relay_catalogue_program_is_a_sentinel, the_relay_catalogue_is_not_the_authors, the_relay_catalogue_runs_native, battery_facts_name_their_own_stop_flag}`; CLI `crates/officina/tests/workshop_battery.rs::{the_control_request_ends_127, the_battery_never_touches_the_relay_stop_flag}`.
4. **Order and spacing** (D-420, review F19). The relay attempts run in a fixed order, 2 s apart, so none reads `too_soon` by accident; "after the deadline" waits for the battery's own 30 s deadline; "while the relay is stopped" sets the battery's own flag; the flood comes last among the refusal attempts, sized so the run's refusal count stays under `ABUSE_REFUSALS`; `abuse` is one attempt of its own, played last, whose request is the 20th refusal. Proof: core `battery.rs::tests::{relay_attempts_are_spaced_two_seconds, the_flood_stays_under_the_abuse_count, abuse_is_the_last_attempt}`; CLI `workshop_battery.rs::each_relay_attempt_reads_its_own_refusal` (fake engine fixture lines).
5. **Phase and judgement** (AD-159, AD-117, D-420). Phase 1 gains a `RelayMailbox` whose vector equals the run's mailbox vector after D-210's normalisation; the battery worker runs the relay attempts through `officina-relay`. `battery::judge` passes the family only when every attempt is refused with its expected reason and journaled, and no `run_started` exists for a requested action other than the control request's. Proof: section `judge` of `battery.json` through `battery_vectors.rs::{a_relay_attempt_not_refused_fails, a_wrong_refusal_fails, a_launch_of_a_requested_action_fails}`; core `crates/officina-core/tests/workshop_vectors.rs::the_battery_mailbox_is_the_run_mailbox_normalised`; CLI `workshop_battery.rs::the_relay_family_is_played_through_the_mailbox`.
6. **The failing resolution, host family** (AD-159, amendment 9). The case of a dry run whose targets resolution fails runs in the host family with the fake tool, which counts exactly one launch of the targets action and none of the dry run; it is absent when the gate 1 entry left the resolver unbuilt (D-403). Proof: CLI `crates/officina/tests/workshop_relay.rs::a_failing_resolution_launches_the_targets_action_once_and_the_dry_run_never`.
7. **Validity** (AD-120 as amended, D-232, D-420). `battery::is_valid` also requires the row's attempt count to equal `ATTEMPTS.len()` of the running build, so a battery recorded before the relay family joined is stale and the next launch refuses `battery_stale`; D-232's `vector_sha256` already covers the mailbox once the plan holds it, and the count stays as a cheap second check. Proof: core `battery.rs::tests::{a_battery_with_fewer_attempts_is_stale, validity_needs_every_key}`.
8. **Journal** (AD-159, D-420, review F23). Battery relay lines carry the battery id in `workshop_run`. Their grouping under the battery in the Relay view is 8.14's, with its test. Proof: CLI `workshop_battery.rs::battery_relay_lines_carry_the_battery_id`.
9. **The local gate and the workstation** (AD-165, D-224, D-202, D-1303). The `workshop` job of `ci.yml`, replayed by the local gate on the workstation's Docker Desktop, plays the relay family through `crates/officina-host/examples/battery_ci.rs` with the sentinel catalogue; on the workstation, Run escape battery on the base image and each owner project image passes with the family, recorded in the v0.8 gate 2 entry beside its second half; on the work laptop, the battery on the work floor's images passes before 8.16's real use (D-1312). Proof: the local replay's output; the gate 2 entry.
10. **Documentation** (D-423). README: the relay family described, `battery::ATTEMPTS` named and never listed, and that adding attempts makes older batteries stale. `CHANGELOG.md` `[Unreleased]`. Proof: review.

## Tasks

- [ ] `workshop/battery.rs`: family, attempts, `relay_catalogue` on `native`, the battery's facts, order and spacing, judgement, validity by attempt count; `battery.json` sections (AC 2 to 5, 7)
- [ ] Host battery phase with the mailbox and its own stop flag; `battery_ci.rs` (AC 3 to 5, 9)
- [ ] Host-family resolution case (AC 6)
- [ ] Journal ids (AC 8)
- [ ] The workstation run (AC 9); README, `CHANGELOG.md` (AC 10)

## Dev Notes

- Requirements: FR-8 as amended (amendment 9), section 7's Worker containment row. Decisions: AD-117, AD-120, AD-159, AD-165. Risks: R83, R90, R97. Epic decisions: D-202, D-206, D-210, D-212, D-224, D-232, D-242; P-70, P-71, D-400, D-403, D-417, D-420, D-422, D-423, D-426, D-428, D-429, D-1303, D-1307, D-1312, D-1313.
- Seams (D-400, read through `main` as built, D-1306): core `workshop/battery.rs` (as built by 6.8, extended by 6.10: `battery::ATTEMPTS`, `BatteryFamily`, the phases and `is_valid`, D-232), host `workshop` battery phase (6.10), the `workshop` job of `ci.yml` and `crates/officina-host/examples/battery_ci.rs` (6.8, 6.10).
- `journal_unavailable` (D-1307) is no battery attempt: it is the host's refusal, never the worker's.
- Waits for: the entry condition (AC 1).
- The sentinel catalogue is what makes "no launch of a requested action" checkable on a real engine: an attempt that slipped through would start nothing and end 127, which the judgement reads as a failure because `run_started` exists for it.
- Why the battery has facts of its own (review F19): a battery has no run row; a "relay stopped" attempt on the author's real flag would stop his sessions; phase 1's 600 s deadline would make "after the deadline" wait ten minutes; and an unspaced sequence would read `too_soon` and `abuse` for every attempt after the first few.
- `workshop_battery` gains no column (AD-161): the attempt count is already a row field (D-212).
- No employer content in `battery.json`: test names under `.test`, documentation address ranges.

## Testing

- Core unit and vectors: `crates/officina-core/src/workshop/battery.rs`, `crates/officina-core/tests/{battery_vectors,workshop_vectors}.rs`.
- CLI integration: `crates/officina/tests/{workshop_battery,workshop_relay}.rs`.
- The `workshop` job replayed by the local gate on Docker Desktop (D-1303); workstation and work laptop (manual): the battery with the relay family.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: the battery's own facts (sentinel catalogue on `native`, synthetic project, its own stop flag, a 30 s deadline) and the attempts' order, spacing, flood size and `abuse` last (F19, D-420); the Relay view grouping moves to 8.14 (F23); D-232 and D-242 cited in place of the withdrawn D-209; D-426's and D-428's attempts added
- 2026-11-02 reworked against v0.4 to v0.6 as built: entry names v0.6's 6.10 (D-1313); the `workshop` job replayed locally on Docker Desktop, the work floor's battery before real use (D-1303, D-1312); `journal_unavailable` is no attempt (D-1307)
