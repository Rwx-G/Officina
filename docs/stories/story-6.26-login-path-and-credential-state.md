# Story 6.26: The login path and the credential state

**Status:** Approved

## Story

As the author who signs in to Claude Code with `/login` today and wants his workers to run on that same subscription when he chooses,
I want to log in once inside the workshop from a command Officina gives me, have each run use a read-only copy of that login, and be told plainly when a login or a token needs renewing, without anything retrying behind my back,
so that the workshop works with the sign-in I already use, no run can tamper with the login the next run will use, and an expired credential stops launches with the one action that fixes it.

## Acceptance Criteria

1. **The login volume and the command** (D-603, D-604, D-605, D-617). Command `prepare_login` (pointer only, D-28; this story owns its `build.rs` entry, capabilities, `every_declared_command_is_allowed`, `ipc.ts` wrapper and `e2e-mock.ts` answer, D-249), on thread `workshop`, refused while the workshop stop flag is set: creates `officina-login-<floor key>` with its labels when absent (`volume create`, story 6.3's names) and returns story 6.6's `login_command` rendered on one line, built with the route story 6.25 resolves for `platform.claude.com`. Doctor prints the same command under the credential's line when the floor's kind is `login` and the state is not `ok`, and writes it to `<home>/doctor/workshop-login.txt`, removed when no longer printed. Officina never reads, copies out or displays anything in the volume. Proof: CLI with the fake engine `crates/officina/tests/workshop_login.rs::{prepare_login_creates_the_labelled_volume_once, prepare_login_returns_the_golden_command, nothing_reads_the_login_volume, prepare_login_refuses_while_stopped}` (the fake engine's log shows `volume create` and no `cp` or `run` touching the volume); host `crates/officina-host/src/doctor.rs::tests::{a_login_floor_prints_and_writes_the_login_command, the_login_file_goes_once_the_state_is_ok}`; `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
2. **A login run** (D-603, D-606, D-621). For `auth = "login"` the launch builds story 6.6's `login` worker (the login read-only, no credential name), adds `.claude.json` to story 6.6's `LOGIN_FILES` if the login entry requires it (D-610), and `credential_missing` means the login volume is absent (`volume ls` filtered on its labels). Story 6.11's interim refusal and its test `a_login_floor_is_refused_until_its_story` are removed. The allow-list of a login run holds story 6.6's `LOGIN_HOSTS`, to which this story adds any host the login entry's step 7 recorded as needed (D-606). Proof: CLI `crates/officina/tests/workshop_launch.rs::{a_login_launch_mounts_the_login_read_only, a_login_launch_sets_no_credential_name, a_login_launch_without_its_volume_is_credential_missing, a_login_run_allows_the_refresh_host}`.
3. **The credential state** (D-607). Core's `credential::fold(&[JournalLine]) -> CredentialState` reads the `workshop_credential` lines whose `floor` is this floor's key and whose `credential_kind` is its current kind (a changed `auth` starts from `unchecked`; this story is the kind's first writer, with its round-trip test, D-249); the window folds them at start and moves the state on: a run of this floor finishing `auth_failed` (`run_auth_failed`, to `needs_login` or `needs_new_credential` by kind); the probe's answer (`probe_passed` to `ok`, `probe_failed` to `needs_login`); a token kind's Credential Manager entry whose `LastWritten` changed since the last line (`credential_replaced`, to `unchecked`), read through story 2.11's `CredentialStore` metadata without the value (a seam this story adds to 2.11's read if absent). This fold becomes story 6.11's `CredentialStateSource`, so `credential_expired` now fires on the `needs_*` states, after `credential_missing` in AD-107's order; `missing` stays computed and reported as `credential_missing`. Proof: core `crates/officina-core/src/workshop/credential.rs::tests::{the_last_line_wins, another_floors_lines_are_ignored, a_changed_kind_starts_unchecked, a_forged_auth_failure_cannot_clear_a_state, every_state_and_cause_has_a_case}` (the last iterates `CredentialState::ALL` and the causes); `crates/officina-core/src/relay/journal.rs::tests::workshop_credential_round_trips`; CLI `workshop_login.rs::{an_auth_failed_run_sets_needs_login, a_replaced_token_returns_to_unchecked, the_state_survives_a_restart, an_expired_state_refuses_the_launch}`.
4. **No loop** (D-607). Officina never relaunches a run and never runs the probe on a timer: the probe runs once at window start for a `login` floor, once per Check login click, once after a run of the floor ends `auth_failed`. Proof: CLI `workshop_login.rs::{the_probe_runs_once_per_trigger, no_probe_runs_without_a_trigger}` (the fake engine's `AuthProbe` creates counted over a simulated hour).
5. **The auth probe** (D-608). Command `check_login` (pointer only, plumbing owned here) runs story 6.6's `AuthProbe` vector attached, 30 s deadline, on thread `workshop`, refused while stopped; parses its one line with core's type (a line that does not parse is `probe_failed` with detail `unreadable`); never stores or shows anything but `logged_in` and `auth_method`. Fake engine mode: an attached `AuthProbe` start prints a fixture line. Proof: CLI `workshop_login.rs::{check_login_reads_only_logged_in_and_method, an_unreadable_probe_line_fails_closed, check_login_refuses_while_stopped}`; `main.rs::tests::every_declared_command_is_allowed`.
6. **The scan of a login run** (D-609). At collection of a `login` run, story 6.14's pass takes its ranges from story 6.6's `Scanner` vector, started attached with the records on stdin (the same records the token kinds scan on the host), 120 s deadline; the ranges it prints are applied exactly as host needles' are; no credentials file in the volume (zero needles) is recorded in the run's facts and the pass goes on. Fake engine mode: an attached `Scanner` start reads its stdin and prints fixture ranges. Proof: CLI `crates/officina/tests/workshop_collect.rs::{a_login_run_is_scanned_in_the_scanner, scanner_ranges_quarantine_like_host_needles, a_login_run_with_no_needles_says_so}`; `crates/officina/tests/credential.rs::the_value_is_nowhere_after_a_login_run` (the seeded value planted in the fake engine's login state and in the run's stream: it appears in no file under the test home, no journal line, no registry row, no IPC payload, and never in the fake engine's log).
7. **Doctor and the managed settings** (D-228, D-607, D-614). Doctor shows the floor's credential kind and state with its action (the login command or the token steps); for a company account, the presence of a managed-settings file at the locations the Claude Code settings page documents (presence only) with the warning that workers do not apply it. Each check is a variant of the doctor's workshop check enum with its level. Proof: host `doctor.rs::tests::{the_credential_state_prints_its_action, a_managed_settings_file_warns_by_presence_only}`.
8. **Documentation** (D-226). README: the login path (Log in for the workshop, the command run from PowerShell or `cmd`, the URL opened on the host and the code pasted, the login read-only in every run), what "Login needed" means and that nothing retries, that `setup-token` is the recommended path for unattended workers (and, if the login entry found a run's refresh invalidating the stored login, that the login kind then serves attended use, R127), that the stored login lives in an engine volume a process running as the author can read (R128), and that a company account's policies are its holder's to check (D-614); `CHANGELOG.md`. Proof: review.

## Tasks

- [ ] `prepare_login` and its plumbing; doctor's login command and file (AC 1)
- [ ] The `login` launch path; remove story 6.11's interim refusal (AC 2)
- [ ] `crates/officina-core/src/workshop/credential.rs`, `workshop_credential`, the triggers, `LastWritten` through 2.11's store (AC 3, 4)
- [ ] `check_login`, the `AuthProbe` run; fake engine modes for `AuthProbe` and `Scanner` (AC 5, 6)
- [ ] The `Scanner` as story 6.14's second `ScanSource`; `credential.rs` end-to-end case (AC 6)
- [ ] Doctor (AC 7); README, `CHANGELOG.md` (AC 8)

## Dev Notes

- Requirements: FR-5 as D-603 extends it, FR-8 (`auth_failed`), FR-10 (the scan), FR-18 (the credential). Decisions: AD-3, AD-36, AD-105, AD-107, AD-112, AD-120, AD-123. Facts: the login entry (D-610), the documented pages cited before D-601. Risks: R125, R127, R128. Epic decisions: D-28 (Epic 2); D-86 (Epic 3); D-200, D-202, D-212, D-223, D-226, D-228, D-230, D-249, D-603 to D-610, D-614, D-617, D-621.
- Seams (D-200): story 2.11's `CredentialStore` (the metadata read); story 6.6's `login_command`, `AuthProbe` and `Scanner`; story 6.7's `auth-status` and `scan`; story 6.11's `LaunchRefusal` and `CredentialState`; story 6.12's `auth_failed`; story 6.14's `ScanSource`; story 6.25's routes.
- Waits for: the login entry (D-610: if its step 1 failed, this story is not implemented and `login` stays refused at launch); stories 6.11, 6.12, 6.14 and 6.25 merged.
- Why the login is read-only and copied per run (D-606): the shared login would otherwise be a channel from one run to every later one; the copy dies with the run's volume.
- Why the state lives in the journal (D-607): migration 3 is frozen (D-212), and the state is a short history of signals the window already journals.
- Why the probe is offline (D-608): it needs no gateway and spends nothing; a revoked login is still found by the next run's `auth_failed`.
- The probe and the scanner run fixed code from the measured image (D-233) on the stored login only; no worker code runs beside them.

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/credential.rs`, `relay/journal.rs`; host `doctor.rs`; CLI with the fake engine `crates/officina/tests/{workshop_login,workshop_launch,workshop_collect,credential}.rs`; `main.rs::tests::every_declared_command_is_allowed`.
- Owner, on the workstation: Slice A end to end with a `login` floor (story 6.20 AC 5).

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-29 created from the owner's answer on authentication (D-603 to D-610, D-614, D-617, D-621)
