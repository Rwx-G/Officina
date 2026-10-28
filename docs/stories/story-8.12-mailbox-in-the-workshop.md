# Story 8.12: The mailbox in the workshop run, end to end (Slice B)

**Status:** Approved

## Story

As the author launching a workshop run whose template lists relay actions,
I want a mailbox container started on the run's internal network and attached to the window before the worker starts, every line it hands over admitted or refused in core, answered, journaled and counted, and the mailbox re-attached after a window restart and removed with the run,
so that a worker reaches my approved reads through one channel that opens no port and mounts no folder on my machine, and a worker that keeps asking for what it may not have ends its own run (FR-6, FR-7).

## Acceptance Criteria

1. **Entry condition** (P-72). 8.9 and 8.11 have merged. Proof: the sprint status.
2. **The role** (AD-157, AD-102 as amended, D-206, D-207, D-415). `ContainerRole` gains `RelayMailbox`: the run image by id; user 1000:1000; the run's internal network only; no volume, mount, credential or environment name; command `officina-entry mailbox --run <id> --listen 0.0.0.0:<MAILBOX_PORT> --deadline-s <s>`, the deadline relative to the mailbox's start (D-431, review F15); `--interactive`; 0.25 CPU, 64 MiB, 32 processes; every role's options; name `officina-relay-<id>`; AD-98's labels. D-206's matches gain the role (no name in the container). Created only for a template whose approved `relay_actions` is not empty; the card's `workshop::vector::RunPlan.relay` is filled (review F28) (address, relay actions, the run's catalogue hash) and `plan_sha256` covers the mailbox's vector. Proof: section `vectors` of `tests/vectors/workshop.json` through `crates/officina-core/tests/workshop_vectors.rs::{every_role_has_a_golden_vector, the_mailbox_holds_no_mount_volume_or_name, plan_hash_covers_the_mailbox, no_relay_actions_no_mailbox}` (the first iterates `ContainerRole::ALL`).
3. **Attached children** (AD-157, D-204, D-416). `EngineCallKind` gains `attach`, a launch kind; `workshop::engine::spawn_attached` starts `start --attach --interactive` and `attach --sig-proxy=false` as long-lived children in a kill-on-close job, returning stdin and stdout; they carry no engine-call deadline. Proof: host test compiled only without `window-authority`, `crates/officina-host/src/workshop/engine.rs::tests::every_launch_kind_is_refused_without_authority` (iterates `EngineCallKind::ALL`); `engine.rs::tests::attach_is_a_launch_kind`; the source lint's count of four sites.
4. **Launch** (AD-107 step 3 as amended). After the gateway and before the worker: the mailbox is created and started attached, its streams held by `workshop::relay`, its `started` line awaited, which carries no clock: no skew is read from it (D-242, D-431, review F15); then the worker starts. A failure ends the launch `failed`, removes what was created and records the step in `workshop_started`. Proof: CLI `crates/officina/tests/workshop_relay.rs::{the_mailbox_starts_before_the_worker, a_mailbox_failure_fails_the_launch_and_cleans_up}` (fake engine call order).
5. **Reading and answering** (AD-157, AD-150, D-414, D-416, D-417). One reader thread per live run with relay actions hands `MailboxLine`s, bounded at `MAILBOX_LINE_MAX_BYTES` (a longer line refused `malformed`), to the collector; the collector runs `worker::admit` on every line, a mailbox `refused` line included, keeps the limiter it returns and never runs a program; a refusal, mailbox refusals included, is answered with a `refused` frame, counted, and journaled with one `worker_relay` line (outcome `refused`, its reason and cause) (D-429, review F4); a replayed `request` line at or below the highest `seq` is dropped and counted in a diagnostic, never run (review F17); `--list` with a `result` frame holding the list text (outcome `listed`); `Run` and `ResolveThen` go to 8.11's `serve` on the run's `workshop-relay` thread. One writer per mailbox serialises frames. Proof: CLI `workshop_relay.rs::{every_refusal_is_answered_and_journaled, a_mailbox_refusal_is_answered_counted_and_journaled, twenty_not_utf8_connections_end_the_run, a_replayed_request_line_is_dropped, list_is_answered_with_the_list_text, an_admitted_read_runs_and_returns_its_output, an_overlong_mailbox_line_is_malformed}`.
6. **Abuse** (AD-158, D-417, amendment 8). The 20th refusal of a run refuses `abuse` and stops the run with the per-run stop; `FinishCause` gains `relay_abuse`, outside `ENVIRONMENT_FAILURE_CAUSES`; `workshop_finished` carries it. Proof: CLI `workshop_relay.rs::the_twentieth_refusal_stops_the_run_relay_abuse`; core `crates/officina-core/src/workshop/view.rs::tests::relay_abuse_is_not_an_environment_failure`; section `finish` of `workshop.json` through `workshop_vectors.rs::every_finish_cause_has_a_case`.
7. **Restart and collection** (AD-109, AD-110 as amended). At window start, reconciliation attaches each live run's mailbox again with `attach --sig-proxy=false` and folds its limiter from the run's `worker_relay` lines, the highest `seq` included, so lines the re-attach replays are dropped (D-429), and `busy` cleared; a request in flight when the window closed is lost (the worker reads `no_answer` or a timeout). Collection stops the mailbox with the gateway, before the packer, and removes it with the run's other objects. Proof: CLI `workshop_relay.rs::{a_restart_reattaches_and_keeps_the_counts, lines_replayed_after_a_reattach_are_dropped, a_lost_request_is_no_answer, collection_stops_and_removes_the_mailbox}`.
8. **Fake engine** (D-223, D-422). The fake engine gains an attached `start` and `attach` for role `RelayMailbox` that replay request lines from a fixture and record every frame written to its stdin, in D-223's log format. Proof: `crates/officina/tests/fake_docker.rs::the_mailbox_mode_replays_and_records` (the log format's parse test).
9. **Slice B flows** (AD-165). With the fake engine and the fake tool: `worker_write` for a write of each class; `order_required` for each `OutOfScopeCause` (a host-pattern target included, D-428) and for a failing resolution; a check listed by a project that does not list its write, judged (D-427); `catalogue_changed` after an approval; both flags; Stop all ending a job (`stopped`); path containment through the channel; when v0.7's governor launch (7.14) has merged, a governor-launched run's request admitted like a clicked run's (D-425). Proof: CLI `workshop_relay.rs::{a_write_of_each_class_is_worker_write, each_scope_cause_is_order_required, a_failing_resolution_is_order_required, a_check_without_its_write_in_the_project_is_judged, an_approval_after_launch_is_catalogue_changed, either_flag_refuses_stopped, stop_all_ends_a_running_read, a_channel_request_runs_in_the_host_folder, a_governor_run_is_admitted_like_a_clicked_run}`.
10. **Injection** (D-147, D-422, AD-165). `tests/vectors/injection.json` gains section `worker_relay`: values shaped as options, absolute and `..` paths, shell text, bidi characters, a forged `seq`, a request right after a catalogue approval, a flood of requests and one of mailbox refusals (D-429, review F4); against a permissive `.+` pattern, ssh-shaped (`nginx;id`), Jinja-shaped (`{{ lookup('pipe','id') }}`), `$VAR` and leading `~` values, each `invalid_value` with zero launches (D-426, review F1); host-pattern target values for a dry run (`lab:prod-web`, `all`), each `order_required` (D-428, review F3); each case names its expected refusal and the fake tool's launch count; the coverage test also iterates `WorkerRefusal::ALL`. `mutant-worker-scope-skip` gains a named case here. Proof: `crates/officina/tests/injection.rs` (section `worker_relay`); the CI `mutants` job.
11. **Gate 2, second half** (AD-166 row 2, D-402). On Docker Desktop with a dev build, from Git Bash with `MSYS_NO_PATHCONV=1`, probe objects named `officina-probe-*`: AD-104's network pair, a probe image `FROM` the base with the musl `officina-entry` and the `officina-relay` link, the mailbox created with this story's vector and started attached by `crates/officina-host/examples/mailbox_probe.rs` (the real attach, answering every request with a fixed result); AD-166's 100 requests, the 1 MiB result, the re-attach, `no_answer` within 10 s once the example is stopped; before and after, listening sockets, every probe container's mounts, the gateway vector against its golden case, and the battery's egress and host families with the mailbox present, plus the mailbox's other ports. Proof: the gate 2 entry (second half).
12. **Zero write** (D-80, D-422). A worker request served through the fake engine and `officina workshop --json` leave the Claude config dir unchanged. Proof: CLI `crates/officina/tests/quota_doctor.rs::a_worker_request_through_the_channel_writes_nothing_under_the_config_dir`.
13. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: the mailbox in workshop runs. README "Relay scope and workers (v0.8, in progress)": the channel (no port, no mount, the run's own network), `no_answer` when the window is closed, the abuse limit. Proof: review.

## Tasks

- [ ] `RelayMailbox` role, vector, D-206 matches, `workshop::vector::RunPlan.relay`, `plan_sha256` (AC 2)
- [ ] `EngineCallKind::Attach`, `spawn_attached` (AC 3)
- [ ] Launch step, reader, writer, collector admission, answers, `worker_relay` refused and listed (AC 4, 5)
- [ ] Abuse, `FinishCause::RelayAbuse` (AC 6); reconciliation and collection (AC 7)
- [ ] Fake engine mode (AC 8); flows (AC 9); `injection.json` section (AC 10)
- [ ] Gate 2's second half with `examples/mailbox_probe.rs` (AC 11)
- [ ] Zero write (AC 12); `CHANGELOG.md`, README (AC 13)

## Dev Notes

- Requirements: FR-6 as amended (amendment 7), FR-7 as amended (amendment 8), section 7's Worker containment row. Decisions: AD-3, AD-98, AD-102, AD-104, AD-107, AD-109, AD-110, AD-150, AD-157, AD-158, AD-165, AD-166. Gates: the gate 2 entry. Risks: R83, R90, R94, R97. Epic decisions: D-80, D-147, D-148, D-204, D-205, D-206, D-207, D-216, D-217, D-223, D-242; P-71, P-73, D-400, D-402, D-403, D-414 to D-417, D-419, D-422, D-425 to D-429, D-431.
- Seams (D-400): `ContainerRole`, golden vectors and `plan_sha256` (6.6), `EngineCallKind` and the engine module (6.3), launch step 3 (6.11), reconciliation and the follower threads (6.12), collection (6.13), the per-run stop and `FinishCause` (6.12), Stop all and the flags (6.17), the fake engine (D-223), the injection harness (5.7, 6.11, 6.16), `quota_doctor.rs` (D-80).
- Waits for: the entry condition (AC 1).
- If the gate 2 entry selected AD-157's fallback, requests come through `docker attach --no-stdin` and frames go through `docker exec -i officina-relay-<id> /opt/officina/bin/officina-entry deliver <seq>` (`EngineCallKind::Exec`, a launch kind, into the mailbox only, never the worker); AC 3 to 5 and the fake engine follow that branch (D-403).
- The window believes no mailbox line: `seq` is the mailbox's, the run is the one whose mailbox's stdout carried the line, and nothing in a request names a run, a project or a folder (AD-157). A worker that took the mailbox over would hold what it already holds.
- No employer content: `lab`, `example.org`, placeholder programs.

## Testing

- Core integration: `crates/officina-core/tests/workshop_vectors.rs`; core unit `crates/officina-core/src/workshop/view.rs`.
- Host unit: `crates/officina-host/src/workshop/engine.rs` (with and without the feature).
- CLI integration: `crates/officina/tests/{workshop_relay,fake_docker,injection,quota_doctor}.rs`.
- CI: `mutants`. Workstation, Docker Desktop (manual): gate 2's second half.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: mailbox refusals answered, counted and journaled (F4, D-429); relative deadline and no skew from `started` (F15, D-431); replayed lines dropped by `seq`, `busy` cleared on restart (F17, F31, D-429); injection cases for program-read values and host-pattern targets (F1, F3, D-426, D-428); a check without its write in the project is judged (F2, D-427); `workshop::vector::RunPlan` named (F28)
