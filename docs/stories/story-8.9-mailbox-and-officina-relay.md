# Story 8.9: The mailbox program, `officina-relay` and their frames (Slice B)

**Status:** Approved

## Story

As the author who lets a worker ask for reads but never hands it a key or a port on my machine,
I want a fixed program in the run's own network that carries one bounded request line from the worker to the window and the window's answer back byte for byte, and a client in the worker that paces itself and says plainly when nobody answers,
so that the worker's only way to reach a server is a request the window parses in core, on a channel that names its run by construction (FR-6).

## Acceptance Criteria

1. **Entry condition** (FR-0, P-72, D-402, D-403). The gate 2 entry's first half passed with attached stdin, or with AD-157's fallback (then AC 9 applies); the gate 3 entry's first half is recorded; the `officina-container` crate and the entry point have merged (6.5, 6.7). Proof: the gate entries and `epic-8-decisions.md`.
2. **Frames in core** (AD-157, D-414, D-431). `relay::worker` declares the request (`{"v":1,"argv":[...]}` or `{"v":1,"list":true}`, one line of at most `REQUEST_MAX_BYTES` with its newline), `MailboxLine` (`started` with the port only, no clock (review F15); `request` with `seq` and the line as a string; `refused` with `seq` and `oversized`, `not_utf8` or `read_deadline`) and `HostFrame` (a header line with `seq` and `kind`: `ack` with a remaining duration in milliseconds, never a clock value; `result` with `exit_code` and the byte lengths of stdout and stderr; or `refused` with the reason and a one-line detail; for a result, exactly those bytes after the header), and the constants `REQUEST_MAX_BYTES`, `MAILBOX_LINE_MAX_BYTES`, `MAILBOX_MAX_CONNECTIONS`, `ACK_TIMEOUT_MS`, `MAILBOX_PORT`. `relay::worker` and `relay::scope` build for `x86_64-unknown-linux-musl` on the `core-linux` job. Proof: section `frames` of `tests/vectors/worker_relay.json` (formatter-exempt) through `crates/officina-core/tests/worker_relay_vectors.rs::{frames_cases_match, host_frames_round_trip_every_byte_value, a_request_line_past_the_bound_is_refused_by_construction, started_carries_no_clock}`; the CI musl step.
3. **`officina-entry mailbox`** (AD-157, AD-108 as amended, D-414, D-429, D-431). `--run <id> --listen 0.0.0.0:<MAILBOX_PORT> --deadline-s <s>`, the deadline counted from its own start (D-242's rule), std networking only: prints `started`; serves at most `MAILBOX_MAX_CONNECTIONS` at once, well under its 32-process limit, closing a connection past it unread (review F31); numbers each connection; reads one line per connection under a read deadline, refusing `oversized` (past `REQUEST_MAX_BYTES`, or a `request` line that would pass `MAILBOX_LINE_MAX_BYTES` once serialised), `not_utf8` and `read_deadline` with a `refused` line; prints every other line as a `request`, parsing a connection's bytes no further; forwards each `HostFrame` from its stdin verbatim to the connection with that `seq` and closes the connection after a `result` or `refused` frame; drops a frame for an unknown or closed `seq`; exits at its deadline. Proof: `crates/officina-container/tests/mailbox.rs::{started_comes_first, one_request_line_per_connection, oversized_and_not_utf8_are_refused_by_the_mailbox, a_slow_writer_meets_the_read_deadline, two_connections_get_their_own_answers, connections_past_the_cap_are_closed_unread, a_frame_for_an_unknown_seq_is_dropped, the_mailbox_exits_at_its_relative_deadline}` (native, loopback, both CI jobs, no Docker, D-422).
4. **`officina-relay`** (AD-157, FR-6 as amended by amendment 7, D-432). `officina-entry` run under the name `officina-relay` (dispatch on the program name's last path segment): `officina-relay <action> [name=value ...]` and `--list`. It reads the mailbox's address from `workshop::vector::RunPlan.relay` (no environment name); serialises its own calls with a lock file under `/tmp` and waits out the 2 s spacing from a last-send time kept beside it before sending (review F20); connects (10 s), sends one request line, waits `ACK_TIMEOUT_MS` for its first frame, which may be an `ack`, a `refused` or a `result` (review F31), else prints `officina: refused: no_answer` and exits 126; after an `ack`, waits for the result until the ack's duration plus 30 s, else exits 124; a result prints stdout and stderr as received and exits with the program's code (127 when it could not start, 124 on its timeout, AD-29); a refusal prints `officina: refused: <code>: <detail>` and exits 126; a malformed argument exits 64; no plan, or a plan without `relay`, prints `officina: refused: no_relay` and exits 126. Proof: `crates/officina-container/tests/mailbox.rs::{relay_prints_a_result_and_exits_with_its_code, relay_prints_a_refusal_and_exits_126, a_refusal_or_result_may_come_first, no_first_frame_within_10_s_is_no_answer, no_result_after_the_ack_duration_is_124, a_1_mib_result_with_every_byte_value_arrives_intact, a_bad_argument_is_usage, no_relay_in_the_plan_is_refused, the_program_name_selects_the_client, two_clients_at_once_are_paced_by_the_client}`.
5. **The plan's relay** (AD-157, AD-102 as amended, D-415, review F28). `workshop::vector::RunPlan.relay: Option<RelayPlan { address, relay_actions, catalogue_hash }>`, absent and not serialised when the template lists no relay action, so an existing plan's golden case is unchanged. The window fills it in 8.12. Proof: section `vectors` of `tests/vectors/workshop.json` through `crates/officina-core/tests/workshop_vectors.rs::{plan_without_relay_is_unchanged, plan_with_relay_round_trips}`.
6. **The image** (AD-99 as amended, AD-150). `docker/workshop.Dockerfile` adds `/opt/officina/bin/officina-relay` as a link to `officina-entry`; the image gains a link, not a binary, and `officina.entry.sha256` covers the program it names. Proof: the CI `workshop` job's image check (`docker run --rm --entrypoint /opt/officina/bin/officina-relay <base> --list` prints `no_relay` and exits 126; `readlink` names `officina-entry`); hadolint stays clean.
7. **What it is not** (AD-157, D-414, review F39). The mailbox holds no credential, mount, volume or environment name and believes nothing it reads: it parses no request and never decides; the only JSON it reads is the window's `HostFrame` headers on its stdin. Proof: `crates/officina-container/tests/mailbox.rs::the_mailbox_parses_json_only_from_its_stdin` (a source scan of `src/entry/mailbox.rs` finds `serde_json::from` only inside the stdin header reader); 8.12's role vector (no mount, volume, credential or name).
8. **Mailbox refusals are requests to the window** (D-429, review F4). Every `refused` line the mailbox prints reaches the window as a `MailboxLine`, which 8.10 maps to its refusal and counts; the mailbox itself counts nothing. Proof: `mailbox.rs::every_mailbox_refusal_is_printed_as_a_line`.
9. **Fallback, only if the gate 2 entry selected it** (D-403). `officina-entry deliver <seq>` reads one `HostFrame` on stdin and hands it to the running mailbox over a local socket inside the mailbox container; otherwise this AC is not built. Proof: `mailbox.rs::deliver_hands_a_frame_to_the_mailbox` when built.
10. **Documentation** (D-423). `CHANGELOG.md` `[Unreleased]` under Added: the mailbox and `officina-relay`. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/relay/worker.rs`: frame types, constants; `worker_relay.json` section `frames`; `worker_relay_vectors.rs` (AC 2)
- [ ] CI `core-linux` musl step for `relay::worker` and `relay::scope` (AC 2)
- [ ] `crates/officina-container/src/entry/mailbox.rs` with the connection cap and relative deadline; program-name dispatch; `officina-relay` client with its own pacing (AC 3, 4, 7, 8)
- [ ] `workshop::vector::RunPlan.relay` (AC 5)
- [ ] Dockerfile link, image check (AC 6)
- [ ] `deliver`, only per gate 2 (AC 9); `CHANGELOG.md` (AC 10)

## Dev Notes

- Requirements: FR-6 as amended (amendment 7). Decisions: AD-99, AD-100, AD-102, AD-108, AD-150, AD-157, AD-165. Gates: the gate 2 entry (first half), the gate 3 entry (first half). Risks: R83, R90, R97. Epic decisions: D-202, D-216, D-224, D-242; P-71, P-72, D-400, D-402, D-403, D-414, D-415, D-422, D-423, D-429, D-431, D-432.
- Seams (D-400): the `officina-container` crate, the entry point's dispatch, the musl build and the `core-linux` job (6.5, 6.7), `workshop::vector::RunPlan` and its golden case (6.6), `docker/workshop.Dockerfile` and the `workshop` job (6.8).
- Waits for: the entry condition (AC 1). Independent of 8.3 to 8.5: frames carry an argument vector, never a catalogue decision.
- The mailbox numbers connections, so `seq` is the mailbox's, never the worker's: a worker cannot forge another connection's answer (a forged `seq` is an injection case, 8.12).
- No clock crosses the channel (D-431, D-242): the mailbox's deadline counts from its start, the ack carries a duration, and skew, a diagnostic only, comes from `docker info`. D-209 is withdrawn and is not followed.
- The client's pacing is a courtesy to the worker's own parallel tool calls (D-432); the window's limiter stays the rule, so a client that skipped it would only lose its own requests.
- No employer content: test names under `.test`, documentation address ranges.

## Testing

- Core unit and vectors: `crates/officina-core/src/relay/worker.rs`, `crates/officina-core/tests/{worker_relay_vectors,workshop_vectors}.rs`; the musl build.
- Container: `crates/officina-container/tests/mailbox.rs` (native, loopback, `core-linux` and `windows`).
- CI `workshop` job: the image check.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: mailbox refusals are lines the window counts (F4, D-429); relative deadline, no clock in `started`, an ack with a duration (F15, D-431); the client paces its own calls (F20, D-432); `workshop::vector::RunPlan` named (F28); a first frame may be `refused` or `result`, and connections are capped (F31, D-429); the mailbox's parsing is checked by a source scan, not by review (F39)
