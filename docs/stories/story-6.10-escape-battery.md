# Story 6.10: The escape battery

**Status:** Approved

## Story

As the author who claims a boundary only where evidence shows it,
I want a fixed battery of escape attempts played on each image with the worker's own vector, every attempt judged against a control that shows it could have succeeded, and its pass valid only for the engine, its settings, the image's measured programs, the vector and the Officina version it ran on,
so that a launch rests on a battery played on what actually runs, and an engine update, a setting change, a rebuild or a vector change turns the evidence stale instead of silently old.

## Acceptance Criteria

1. **Attempts and plan** (AD-117, FR-15). Core's `battery::ATTEMPTS` holds every attempt, seeded from FR-15's families, each naming its `BatteryFamily`, how it is judged, its control (D-235) and, for the gateway family, the `GatewayRefusal` expected; `battery::plan(...)` builds the plan arguments (gateway name, test host, decoy addresses, the battery mount, for a `login` floor the login mount path, the expected environment names from the vector, the credential's name when the kind has one and the image's recorded `ENV` names, the limits). For a `login` floor the files family also tries a write and a create in the login mount, and the credentials family finds no credential name in the environment (D-603, D-606). The battery's canonical input is one mount and no memory (D-235; story 6.6's shape test extends it to the other shapes). `AttemptLine` (family, attempt, expected refusal, outcome `refused`, `escaped`, `inconclusive` or `not_applicable`, detail at most 1 KiB) and `ControlLine` (family, control, `ok` or `failed`) are one JSON object per line. Proof: section `attempts` of `tests/vectors/battery.json` (formatter-exempt) through `crates/officina-core/tests/battery_vectors.rs::{every_family_has_an_attempt, every_attempt_has_a_case, every_family_judged_from_a_failure_has_a_control, plan_cases_match}` (iterating `BatteryFamily::ALL` and `ATTEMPTS`).
2. **The program** (AD-117, D-235). `officina-entry battery --phase <1|2> <plan>` plays each family's control first, then its attempts, with std only (an ICMP datagram socket through `python3`), one line each; the files family's writes count only after writes to `/tmp` and `/work` succeeded, the egress family's only after the gateway's name resolved and its port 3128 opened; `officina-entry decoy` resolves Docker Desktop's names, prints the addresses, connects to its own port and prints `listening`. Proof: `crates/officina-container/tests/entry_linux.rs::{the_battery_prints_one_line_per_attempt, a_control_runs_before_its_family, the_files_family_reads_read_only_as_refused, the_privileges_family_reads_proc_self_status, the_environment_family_compares_names_only, the_decoy_prints_listening_after_its_own_connect}` (on the CI host, attempts that need a container are run against fixtures through a test-only argument).
3. **Containers and phases** (AD-117, D-205, D-206, D-210, D-235, D-242). The host runs, labelled and removed afterwards: the decoy on its own plain bridge, and a second probe container on the decoy's network that must connect to it (the host family's control); phase 1, gateway A (`--deadline-s 600`, limit 1000, the test allow-list, the two `--add-host` test names) and the battery worker (the `Worker` vector of the floor's credential kind with the battery command, the battery mount from `<home>/workshop/battery/mount/` recreated per battery, and the dummy credential for a token kind, or for a `login` floor an empty labelled battery volume mounted read-only where the login goes, created and removed with the battery, never the real login, D-206, D-603) on a fresh network pair; phase 2, gateway B (`--deadline-s 15`, limit 20) and a second battery worker that waits 15 s from its own start (it starts after gateway B), tries once, then sends 20 bad requests; gateway B's exit code read. At battery start the host measures the clock skew from `info`'s `SystemTime` (a diagnostic, D-242). Attached starts take the phase's limit plus 60 s (D-205); refused while the stop flag is set. Fake engine modes: attached battery starts print fixture attempt and control lines, gateways print fixture refusals. Proof: CLI `crates/officina/tests/workshop_battery.rs::{a_battery_creates_and_removes_its_containers, the_battery_worker_vector_is_the_worker_vector_normalised, the_dummy_credential_is_the_only_credential, a_login_floor_battery_mounts_its_own_empty_volume_never_the_login, the_decoy_control_needs_a_second_container, nothing_starts_while_stopped}`.
4. **Judgement** (AD-117, D-235). `battery::judge` passes only when every control succeeded, every attempt is `refused`, each gateway-family attempt is matched in order by a gateway `refused` line with the expected cause, and gateway B exited 3; a failed control makes its family `inconclusive` (review F9); `escaped` and `inconclusive` fail; `not_applicable` passes only off Docker Desktop (from the engine's platform name) and only for attempts aimed at Docker Desktop's names. Skew is never a judgement input (D-242). Proof: section `judge` of `battery.json` through `battery_vectors.rs::{judge_cases_match, one_escape_fails, one_inconclusive_fails, a_failed_control_is_inconclusive, an_unmatched_gateway_attempt_fails, gateway_b_not_exiting_3_fails, not_applicable_fails_on_docker_desktop}` (the fourth iterates the families that have a control).
5. **Records and validity** (AD-117, AD-120, D-212, D-215, D-232, D-249). Command `run_battery(image?)` (pointer only in the window; this story owns its `build.rs` entry, capabilities, `every_declared_command_is_allowed`, `ipc.ts` wrapper and `e2e-mock.ts` answer, review F33), on `workshop-build`, for one image or every stale one; a battery id drawn by the host (D-215); one `workshop_battery` row with counts, outcome, `EngineKey` (with its settings digest, review F52), image and gateway image ids, Officina's version, `vector_sha256` (D-210's normalised `Battery` vector and plan as this build makes them), the image's `programs_sha256` and the clock skew; one `escape_attempt` journal line per attempt and one `escape_battery` line; the gateway's refusals journaled as `gateway_refused` with `battery_id`, the first writer of that kind (D-249, review F32). `battery::is_valid(rows, key, now_ms)`: the newest passing row with the same `EngineKey`, image id, gateway image id, Officina version, `vector_sha256` and `programs_sha256`, finished in the last 30 days (review F4 a). Proof: core `crates/officina-core/src/workshop/battery.rs::tests::{validity_needs_every_key, validity_needs_the_vector_digest, validity_needs_the_programs_digest, validity_lasts_30_days, a_failed_battery_never_validates}`; `crates/officina-core/src/relay/journal.rs::tests::{escape_attempt_round_trips, escape_battery_round_trips, gateway_refused_round_trips}`; CLI `workshop_battery.rs::one_row_and_its_journal_lines_per_battery`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
6. **CI with a real engine** (AD-125 real-engine row, D-224). The `workshop` job runs the full battery through `crates/officina-host/examples/battery_ci.rs` on the base and gateway images with a dummy credential; the job fails unless the battery passes, attempts aimed at Docker Desktop's names reading `not_applicable` there. Proof: the CI job.
7. **Gate 2, second half** (AD-126 row 2, D-202; agent, owner for the upstream). On Docker Desktop, driven by `crates/officina-host/examples/battery_ci.rs` with the window's resolution of `docker.exe` and a dummy credential, since the window's Run escape battery button is story 6.20's (review F5): the battery passes on the base image and on each owner project image; the CONNECT through the work network's resolved proxy is story 6.25's (D-621). This is section 7's Containment row on the workstation before the first launch. Proof: the research notes' gate 2 entry, second half.
8. **Doctor** (AD-123, D-228). The battery's last outcome and age per image, `battery_stale` named with the key that moved, and the newest clock skew. Each check is a variant of the doctor's workshop check enum with its level. Proof: host `crates/officina-host/src/doctor.rs::tests::{a_stale_battery_names_the_key_that_moved, the_last_clock_skew_is_shown}`.
9. **Documentation** (D-226). README: the battery, its families and their controls described and `battery::ATTEMPTS` named (never listed), its 30-day validity and what makes it stale (engine, engine settings, images, measured programs, vector, Officina version); `CHANGELOG.md`. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/workshop/battery.rs`: `ATTEMPTS`, families, controls, plan, lines, judge, validity; `tests/vectors/battery.json`; `battery_vectors.rs` (AC 1, 4, 5)
- [ ] `crates/officina-container/src/entry/{battery,decoy}.rs`; `entry_linux.rs` cases (AC 2)
- [ ] `crates/officina-host/src/workshop/battery.rs`: containers, controls, phases, collection of lines, records, journal; `run_battery` and its plumbing; fake engine modes; `workshop_battery.rs` (AC 3, 5)
- [ ] `examples/battery_ci.rs` battery part; CI job step (AC 6)
- [ ] Gate 2's second half (AC 7); doctor (AC 8); README (AC 9)

## Dev Notes

- Requirements: FR-15 as amended (amendment 12), FR-6 (`battery_stale`), PRD section 7 Containment. Decisions: AD-102, AD-103, AD-117, AD-120, AD-125, AD-126. Facts: addendum section 1 items 5, 7; the gate 2 entry (first half). Risks: R47, R51, R62. Epic decisions: P-50, P-51, P-55, D-200, D-202, D-205, D-206, D-210, D-212, D-215, D-223, D-224, D-232, D-234, D-235, D-242, D-249.
- Seams (D-200): `JournalKind` (2.5); the `workshop` CI job (story 6.8).
- Waits for: the gate 2 entry (first half); stories 6.6 and 6.8 merged.
- Why controls (review F9, R62): an attempt judged from a failure passes as well when it could not have succeeded, a decoy that never listened or a broken write helper; the control in the same run is what makes `refused` mean refused.
- Why the vector and programs digests (review F4): the Officina version is the workspace version, constant across the builds of `main` the owner runs, so a vector change would otherwise keep a 30-day-old pass; and a project image could replace the programs the battery runs (story 6.8 refuses it, this key notices it).
- Upstream mode (P-55, review F51): the battery's gateways run in direct mode; AD-103's steps 1, 2 and 4 are the same code in upstream mode, where the private-address check is the proxy's (FR-4). The addendum's AD-117 says so.
- The host family of FR-15 (crafted packages, forged stream and gateway lines) is not here: each case lives with the check it attacks, in stories 6.12 to 6.15, and the QA report (6.24) collects them.
- The battery carries a dummy credential, so D-230's replacement has nothing to find in its lines; it applies to runs.
- No employer content in `battery.json`: test names under `.test`, documentation address ranges.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/battery.rs`, `relay/journal.rs`, `crates/officina-core/tests/battery_vectors.rs`; CLI with the fake engine `crates/officina/tests/workshop_battery.rs`; host `crates/officina-host/src/doctor.rs`; app `main.rs` command test.
- CI `core-linux`: `crates/officina-container/tests/entry_linux.rs` (battery program and control cases).
- CI `workshop` job (Linux engine): the full battery on the real gateway and base image.
- Workstation, Docker Desktop (manual, through `battery_ci.rs`): gate 2's second half (AC 7), the Containment row before the first launch.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: validity keyed on the vector and programs digests (F4 a) and an `EngineKey` settings digest (F52); gate 2 driven by `battery_ci.rs` (F5); a control per family judged from a failure (F9); one canonical shape (F10); relative gateway deadlines, skew a diagnostic only (F21); `gateway_refused` first written and round-tripped here (F32); `run_battery` plumbing owned here (F33); upstream note (F51)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
- 2026-10-29 the owner's answers: the battery plays the floor's credential kind, a `login` floor with its own empty read-only volume and a write attempt there; the CONNECT through the work network's proxy moves to story 6.25 (D-603, D-606, D-621)
