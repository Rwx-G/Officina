# Story 6.25: The system proxy and the corporate CA

**Status:** In Progress

## Story

As the author whose work network reaches the internet only through a proxy that Windows configures by itself,
I want Officina to resolve that proxy from Windows' own settings for every host a run, a build or a login needs, and hand it to the gateway and the build without the worker ever seeing it, with a corporate root baked into the images when TLS is inspected,
so that the same floor works at home and at work without an edit, nothing in a container has to read a PAC file, and a proxy the workshop cannot use is named before a run fails on it.

## Acceptance Criteria

1. **Resolution on the host** (D-611). `officina-host::platform::windows` gains a read-only WinHTTP reader (`windows-sys` feature `Win32_Networking_WinHttp`, listed for the owner's approval): `WinHttpGetIEProxyConfigForCurrentUser` gives the auto-detect flag, the PAC URL, the static proxy and its bypass list; for a PAC URL or auto-detect, `WinHttpGetProxyForUrl` on `https://<host>/` gives the PAC answer, fetched and evaluated as Windows does. Nothing is ever written to a proxy setting; the reader is the only `unsafe` it adds, in `platform/windows.rs` with `// SAFETY:` comments, every WinHTTP handle closed on every path. The reader is an argument of the resolution (D-86's pattern), so the tests inject answers. Proof: host unit `crates/officina-host/src/platform/windows.rs::tests::reads_the_current_users_proxy_config` (Windows only; asserts a well-formed answer, whatever the network); `crates/officina-host/src/workshop/proxy.rs::tests::{the_reader_is_injected, handles_are_closed_on_error}`.
2. **Routes in core** (D-611). `officina-core::workshop::proxy::routes(setting, allow_list, &SystemProxy) -> Result<Routes, ProxyRefusal>`, pure: `"none"` gives every entry `direct`; `"host:port"` routes every entry there; `"system"` applies the static proxy and its bypass list (`<local>`, `*` patterns, as WinINet reads them) or takes each entry's PAC answer: the first `PROXY host:port` element, `DIRECT` as direct, a `*.` entry resolved on its suffix. A `SOCKS`, `HTTPS` or unparseable element refuses `proxy_unsupported` naming the entry; a PAC URL whose fetch or evaluation failed refuses `proxy_unresolved`; auto-detect finding no PAC (WPAD absent) gives direct. `ProxyRefusal` and `ProxySource` (`floor`, `pac`, `auto_detect`, `static`, `none`) are `closed_enum!`s. Proof: section `proxy` of `tests/vectors/workshop.json` (formatter-exempt; PAC answers as strings, `example.com` proxies) through `crates/officina-core/tests/workshop_vectors.rs::{proxy_cases_match, every_refusal_and_source_has_a_case}` (the second iterates both enums' `ALL`); core `crates/officina-core/src/workshop/proxy.rs::tests::{none_is_direct, an_explicit_proxy_routes_every_entry, a_bypassed_host_is_direct, the_first_proxy_element_wins, socks_is_unsupported, a_wildcard_entry_uses_its_suffix}`.
3. **The host API the consumers call** (D-611, D-621). `officina-host::workshop::proxy::resolve(&WorkshopTable, &[AllowEntry], &dyn SystemProxyReader) -> Result<Routes, ProxyRefusal>` is the one entry point, run on thread `workshop` (never the collector, AD-3 as amended), each WinHTTP call bounded by a 10 s timeout. The consumers merge after this story and wire it themselves: card issue and launch into `LaunchFacts` and the gateway's `--route` arguments (story 6.11, with its `plan_changed`, `proxy_unresolved` and `proxy_unsupported` cases), Build images' build arguments from the route of `downloads.claude.ai`, the native installer's host (story 6.8), the login command from the route of `platform.claude.com` (story 6.26). Proof: host `crates/officina-host/src/workshop/proxy.rs::tests::{resolve_applies_core_routes_to_the_reader_answer, a_slow_pac_times_out_as_unresolved, the_setting_none_never_calls_the_reader}` (the reader injected).
4. **Doctor** (AD-123, D-228, D-611, D-612). Doctor names the proxy setting, the source it resolved from and each allow-list entry's route (proxy host and port, or direct), a refusal as a failure with the entry, and a note when the route of `api.anthropic.com` goes through a proxy and no `ca_file` is set ("if this proxy inspects TLS, export its root with the README's one-liner"); `ca_file`'s readability and label stay story 6.8's check. Each check is a variant of the doctor's workshop check enum with its level. Proof: host `crates/officina-host/src/doctor.rs::tests::{the_proxy_source_and_routes_are_named, a_socks_answer_fails_with_its_entry, a_proxied_api_host_without_a_ca_file_notes_the_export}`.
5. **The work network, second half** (D-613, D-621; owner, on the work network; run once stories 6.8 and 6.10 have merged, since it uses their builds and battery). With a build of `main` holding them: Build images through the resolved proxy (and the exported `ca_file` if the work network entry found inspection), a passing battery, then one `CONNECT` from the real gateway through the resolved proxy to `api.anthropic.com`, the gateway's `started` line reading the upstream `reachable` and an `allowed` line for the host, driven by `crates/officina-host/examples/battery_ci.rs`, whose `--resolve-proxy` option (story 6.8) applies this story's resolution (moved from story 6.10's gate 2 second half). Proof: the research notes' work network entry, second half. Closes this story without blocking dependent code (D-202).
6. **Documentation** (D-226). README: `upstream_proxy = "system"` and what it resolves, `"none"` and an explicit proxy; that the worker never sees the corporate proxy; that an authenticating proxy is not supported; the `*.` approximation; how to export a corporate root for `ca_file` (the one-liner of story 6.1 AC 14, W2) and that a CA change rebuilds the images (D-208); `CHANGELOG.md` under Added. Proof: review.

## Tasks

- [ ] `platform::windows` WinHTTP reader, the `windows-sys` feature after the owner's approval (AC 1): waits for the approval; the interim registry reader is built (D-1124)
- [x] `crates/officina-core/src/workshop/proxy.rs`: `routes`, `ProxyRefusal`, `ProxySource`; the `proxy` section of `workshop.json` (AC 2)
- [x] `crates/officina-host/src/workshop/proxy.rs`: `resolve`, on thread `workshop`, with its timeout (AC 3)
- [x] Doctor (AC 4); [ ] the work network's second half with the owner (AC 5); [x] README, `CHANGELOG.md` (AC 6)

## Dev Notes

- Requirements: FR-4 (upstream proxy) and FR-1 (CA file) as D-611 and D-612 read them; FR-18 (`upstream_proxy`, `ca_file`). Decisions: AD-3, AD-99, AD-103, AD-123. Epic decisions: D-86 (Epic 3); D-200, D-202, D-208, D-226, D-228, D-611, D-612, D-613, D-621. Risks: R126.
- Seams (D-200): `platform::windows` (story 6.3's HKLM read is beside it); story 6.5's `GatewayConfig` routes type; doctor. It calls no consumer: stories 6.8, 6.11 and 6.26 call it.
- Waits for: the gate 0 entry; stories 6.3 and 6.5 merged. Stories 6.8 and 6.11 wait for this one.
- Why the host resolves (the owner's answer, D-611): a container cannot read a PAC file, and a PAC script is code the workshop has no reason to run; WinHTTP evaluates it exactly as the author's own programs see it, read-only.
- Why per entry: a PAC file may send `api.anthropic.com` and a package registry to different proxies, or one direct.
- An authenticating proxy (`407`) is not supported (PRD section 5); the gateway's `refused` line carries the status and doctor names `proxy_auth_required` (story 6.5).
- No employer content: proxies in vectors and tests are `proxy.example.com`, PAC answers are made up, and the work network's values stay on the work machine (D-613).

## Testing

- CI, no Docker: core `crates/officina-core/src/workshop/proxy.rs`, `crates/officina-core/tests/workshop_vectors.rs`; host `crates/officina-host/src/{platform/windows,workshop/proxy,doctor}.rs` with an injected reader.
- Owner, on the work network: AC 5.

## Dev Agent Record

### Debug Log

### Completion Notes

- AC 1: open. The `windows-sys` feature `Win32_Networking_WinHttp` is not approved, so no WinHTTP reader is written. Built instead: the `SystemProxyReader` trait the WinHTTP reader will implement, and `RegistryReader`, which reads the same configuration from the current user's registry with the approved `Win32_System_Registry` (`platform/windows.rs::registry_binary`, one `unsafe` block with its `// SAFETY:`) and cannot evaluate a PAC file (D-1124). Host `workshop/proxy.rs::tests::{the_reader_is_injected, handles_are_closed_on_error, the_connection_flags_are_byte_8, the_registry_reader_cannot_evaluate_a_pac}`. Owner item: approve the feature, then the reader and `platform/windows.rs::tests::reads_the_current_users_proxy_config` land.
- AC 2: core `workshop/proxy.rs::tests::{none_is_direct, an_explicit_proxy_routes_every_entry, a_bypassed_host_is_direct, the_first_proxy_element_wins, socks_is_unsupported, a_wildcard_entry_uses_its_suffix, auto_detect_without_a_pac_is_direct_and_a_failed_pac_is_unresolved, proxy_cases_match, every_refusal_and_source_has_a_case}` over the `proxy` section of `workshop.json` (in core's unit tests, D-1121's pattern).
- AC 3: host `workshop/proxy.rs::tests::{resolve_applies_core_routes_to_the_reader_answer, a_slow_pac_times_out_as_unresolved, the_setting_none_never_calls_the_reader}`. The consumers (6.8, 6.11, 6.26) call `resolve` with the reader.
- AC 4: host `workshop/doctor.rs::tests::{the_proxy_source_and_routes_are_named, a_socks_answer_fails_with_its_entry, a_proxied_api_host_without_a_ca_file_notes_the_export, an_unevaluated_auto_detect_is_named}` (the section's module, as for story 6.3).
- AC 5: open, the owner's, on the work network once stories 6.8 and 6.10 have merged.
- AC 6: README "Proxies and a corporate root"; `CHANGELOG.md` Added.

## File List

- `crates/officina-core/src/workshop/proxy.rs` (new), `workshop/{mod,doctor}.rs`; `tests/vectors/workshop.json` (the `proxy` section)
- `crates/officina-host/src/workshop/proxy.rs` (new), `workshop/{mod,doctor}.rs`, `platform/windows.rs`, `doctor.rs`
- `README.md`, `CHANGELOG.md`, `docs/stories/epic-6-decisions.md` (D-1124), `docs/stories/sprint-status.yaml`

## Change Log

- 2026-10-29 created from the owner's answer to gate 0 (D-611, D-612, D-613, D-621)
- 2026-11-01 ACs 2, 3, 4 and 6 implemented with the interim registry reader (D-1124); AC 1 waits for the feature's approval, AC 5 for the owner on the work network
