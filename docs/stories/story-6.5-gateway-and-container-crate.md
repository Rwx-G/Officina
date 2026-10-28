# Story 6.5: The gateway, the `officina-container` crate and the musl build

**Status:** Approved

## Story

As the author whose worker's only way out is a proxy Officina wrote,
I want every rule the gateway applies decided in core with a vector case, the gateway itself a small std-only Linux program built from the same source, and its behaviour tested on loopback without Docker or the internet,
so that the gateway and the host cannot disagree on a rule, and a regression in what it lets through turns CI red.

## Acceptance Criteria

1. **The crate and the build** (AD-97, AD-100, AD-1 as amended, the gate 9 entry, D-224). `crates/officina-container` joins the workspace with binaries `officina-gateway` and `officina-entry` (the latter a stub until story 6.7), depending on `officina-core` and `serde_json` only, workspace lints, `unsafe` denied. `officina-entry`'s modules are `#[cfg(unix)]` and its Windows `main` exits 64 with a message, so the `windows` job's workspace clippy and tests compile the crate (review F23). `rust-toolchain.toml` gains `targets = ["x86_64-unknown-linux-musl"]`; `.cargo/config.toml` sets `[target.x86_64-unknown-linux-musl] linker = "rust-lld"` with the flavour flag the gate 9 entry recorded; `deny.toml` gains `x86_64-unknown-linux-musl`. `cargo build --release -p officina-container --target x86_64-unknown-linux-musl` succeeds on the `windows` CI job and on `core-linux`; both `core-linux` steps (clippy, test) name `-p officina-container`; `officina-core` stays pure Rust with no C dependency. Proof: the CI steps; `cargo deny` on the audit job; `crates/officina-container/tests/gateway.rs::the_crate_depends_on_core_and_serde_json_only` (reads `cargo metadata` for the crate's normal dependencies); `crates/officina-container/tests/entry_windows.rs::officina_entry_exits_64_on_windows` (`#![cfg(windows)]`).
2. **Requests and hosts** (AD-103 steps 1 and 2, D-234). `officina-core::workshop::gateway::parse_request` accepts `CONNECT <host>:<port> HTTP/1.x` within 8 KiB, nothing else, and returns the bytes that followed the header terminator, which start the TLS buffer (review F6); `gateway::host` lowercases, drops a trailing dot, checks labels and length, refuses an IP literal in any spelling, and matches the allow-list exactly or through a leading `*.` entry (one or more labels before the suffix). `GatewayRefusal` is a `closed_enum!` with AD-103's causes and the HTTP status each answers. `CLAUDE_CODE_HOSTS`, the fixed allow-list, is declared here with the hosts the gate 3 entry recorded as needed (D-203, review F46), with phase 2's issue for a project host repeating one of them (an `IssueKind` with its fixture under `every_issue_kind_has_a_case`). Proof: sections `requests` (a pipelined ClientHello case among them) and `hosts` of `tests/vectors/gateway.json` (formatter-exempt) through `crates/officina-core/tests/gateway_vectors.rs::{request_cases_match, host_cases_match, every_refusal_has_a_case}` (the last iterates `GatewayRefusal::ALL`); core `crates/officina-core/src/workshop/floor.rs::tests::a_fixed_host_in_hosts_is_an_issue` (iterates `CLAUDE_CODE_HOSTS`).
3. **Address classes** (AD-103 step 3, D-234). `gateway::addr::is_public` reads one constant table of non-public IPv4 and IPv6 ranges built from IANA's special-purpose address registries, named as its source in one comment and transcribed nowhere else (review F8): the ranges D-234 lists, with IPv4-mapped, NAT64, 6to4 and Teredo forms unwrapped and their embedded IPv4 checked; a name with any non-public address refuses `private_address`, no IPv4 address `no_ipv4`; after every address is checked the gateway connects to the first IPv4 one. Proof: section `addresses` of `gateway.json` through `gateway_vectors.rs::{every_range_has_a_case_inside_and_at_each_edge, embedded_ipv4_forms_are_unwrapped, the_first_ipv4_address_is_chosen}` (the first derives the inside, first, last and just-outside addresses of each range from the table itself).
4. **The ClientHello** (AD-103 step 4, D-234). `gateway::tls::server_name(&[u8])` parses a TLS record holding a ClientHello within 16 KiB, starting with the bytes `parse_request` returned, and returns the server name or `not_tls` and `sni_missing`, and whether an `encrypted_client_hello` extension is present; the relay forwards exactly the buffer it parsed first (review F6); `gateway::tls::client_hello(name)` builds one (for tests and the battery). Proof: section `client_hello` of `gateway.json` (byte fixtures: a browser-shaped hello, a fragmented record, no SNI, two names, an ECH extension, truncated lengths) through `gateway_vectors.rs::client_hello_cases_match`; core `crates/officina-core/src/workshop/gateway/tls.rs::tests::a_built_hello_parses_to_its_name`.
5. **The limiter, the tunnels and the lines** (AD-103, D-234, D-242). `gateway::Limiter` is a pure state machine over `now_ms`: a sliding window of refusals, `limit` at the threshold. `gateway::MAX_TUNNELS` drives the `busy` threshold and, through story 6.6, the gateway's `--pids-limit` (review F7). `GatewayConfig::{to_args, from_args}` round-trips (`--run`, `--listen`, `--allow`, `--deadline-s` counted from the gateway's start, `--refusal-limit`, `--refusal-window-s`, `--upstream`), the allow-list starting with `CLAUDE_CODE_HOSTS` (review F21: the deadline is relative). `GatewayLine` (`started` with configuration digest and upstream state, `allowed` with host, port and the ECH flag, `refused` with the host made printable and cut to 253 characters, port and cause, `limit`) serializes one JSON object per line. Proof: core `crates/officina-core/src/workshop/gateway/mod.rs::tests::{the_limiter_trips_inside_the_window_only, args_round_trip, the_allow_list_starts_with_the_fixed_hosts, a_refused_host_is_made_printable, the_deadline_is_relative_seconds}`; section `lines` of `gateway.json` through `gateway_vectors.rs::line_cases_match`.
6. **The program** (AD-103, D-234). `officina-gateway` holds no credential, writes no file, runs no shell, reads no environment: per connection one thread spawned with `thread::Builder`, answering `busy` when the spawn fails or `MAX_TUNNELS` tunnels are open (never panicking); 10 s deadlines on request, ClientHello and connect; direct mode resolves once and connects to the checked address, never resolving again; upstream mode sends its own `CONNECT <host>:443 HTTP/1.1` with a `Host` header, reads at most 8 KiB of headers and needs a 2xx (`upstream_refused`), and reports `reachable` or `unreachable` in `started` after one probe connection; from its deadline (relative to its start) every request is refused `deadline` and open tunnels close; at the limit it writes `limit` and exits 3. Proof: `crates/officina-container/tests/gateway.rs` against the real program built natively on loopback, CI `windows` and `core-linux`, no Docker, no internet: `{upstream_mode_relays_an_allowed_host, every_refusal_reachable_offline_is_refused_with_its_cause, a_mismatched_server_name_is_refused, a_pipelined_client_hello_is_checked, an_oversized_request_is_refused, after_the_deadline_everything_is_refused, the_limit_exits_3, one_more_than_max_tunnels_is_busy, lines_are_one_json_object_each, direct_mode_refuses_localhost_as_private_address, the_started_line_reports_the_upstream_state}` (a test CONNECT proxy and a TLS-shaped echo server on loopback stand in for the upstream and the host; "every refusal" iterates `GatewayRefusal::ALL` minus the causes that need a public resolver, which the test names from the enum and story 6.10's battery covers; the pipelined case sends a matching hello in the request's write and a mismatched one after, and expects the first to be checked and forwarded).
7. **Documentation** (D-226). `CHANGELOG.md` `[Unreleased]` under Added: the gateway; README: the gateway's gives-up of FR-4 (plain HTTP, SSH and UDP; per-URL filtering; what goes to an allowed host; fronting inside TLS; the inner name under ECH), and the refusal causes named as the constant `GatewayRefusal`, never listed. Proof: review.

## Tasks

- [ ] Workspace member `crates/officina-container`, `#[cfg(unix)]` entry modules and the Windows `main`; toolchain target; `.cargo/config.toml`; `deny.toml`; CI musl build and `-p officina-container` on both Linux steps (AC 1)
- [ ] `crates/officina-core/src/workshop/gateway/{mod,addr,tls}.rs`, `CLAUDE_CODE_HOSTS` and its issue (AC 2 to 5)
- [ ] `tests/vectors/gateway.json`, `crates/officina-core/tests/gateway_vectors.rs` (AC 2 to 5)
- [ ] `crates/officina-container/src/bin/officina-gateway.rs` and its library module (AC 6)
- [ ] `crates/officina-container/tests/gateway.rs` with the loopback test proxy and server (AC 6)
- [ ] `CHANGELOG.md`, README (AC 7)

## Dev Notes

- Requirements: FR-4 as amended (amendments 1 and 5), FR-1 (the gateway image's program). Decisions: AD-1 as amended, AD-97, AD-100, AD-103. Facts: addendum section 1 items 5 and 13; the gate 9 entry (the linker flag; on failure the fallback of AD-100, D-203), the gate 3 entry (`CLAUDE_CODE_HOSTS`). Risks: R51, R52. Epic decisions: P-50, P-51, D-200, D-203, D-224, D-226, D-234, D-242.
- Seams (D-200): `rust-toolchain.toml`, `.cargo/config.toml`, `deny.toml`, the `core-linux` and `windows` jobs of `ci.yml`; `relay::shell` (2.3, std only) must keep compiling for the musl target, which this story's build proves before story 6.9 uses it; `relay::catalogue`'s issue list (2.1, 6.2).
- Waits for: the gate 7, 0, 9 and 3 entries.
- Why a crate, not `officina gateway`: `officina` links `officina-host`, which links SQLite's bundled C sources; a musl build of it needs a musl C compiler the workstation does not have (AD-100, amendment 1).
- Why the deadline is relative (D-242, review F21): an absolute deadline lives on the engine VM's clock, which may drift from the window's after host sleep (the review's belief, not verified here; the change is defensive); counting from the gateway's start removes the question, and the skew becomes a doctor diagnostic.
- Why the pids limit follows `MAX_TUNNELS` (review F7): the pids controller counts threads, each tunnel takes two (one per direction), plus the listener and the init; AD-102's 128 would fail before `busy`.
- Encrypted ClientHello is recorded, not refused: a client's GREASE extension cannot be told from a real one by design (AD-103); the outer name must still be allowed.
- `cargo deny` sees no new crate: the gateway uses std networking and parses the ClientHello by hand.
- The resolution-dependent causes (`private_address` on a public name, `no_ipv4`, `resolve_failed`) are reached offline through `localhost` and an unresolvable name; what still needs the engine's resolver runs in the battery (story 6.10) and the CI `workshop` job.
- No employer content in `gateway.json`: `example.com` names, documentation address ranges (RFC 5737, RFC 3849).

## Testing

- CI, no Docker, no internet: core `crates/officina-core/src/workshop/gateway/*.rs`, `crates/officina-core/tests/gateway_vectors.rs`; the real gateway on loopback, `crates/officina-container/tests/gateway.rs`, on `windows` and `core-linux`; the musl build on both; `entry_windows.rs` on `windows`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: a pipelined ClientHello is the one checked and forwarded (F6); `MAX_TUNNELS` and spawn failures answer `busy` (F7); the range table from IANA's registries, the first IPv4 address (F8); `--deadline-s` relative, no clock in `started` (F21); `officina-entry` on Windows, `-p officina-container` on Linux, `deny.toml` musl (F23); `CLAUDE_CODE_HOSTS` declared here, waits for the gate 3 entry (F46)
- 2026-10-29 research-note entries named by subject, no number reserved (D-600)
