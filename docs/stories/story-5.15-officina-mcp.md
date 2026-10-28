# Story 5.15: `officina mcp`: the server, read tools and proposal files

**Status:** Approved

## Story

As the author about to host a model session that sees every subject,
I want its only tools to be an Officina server that reads what Officina records and writes proposals as files, nothing else,
so that the orchestrator can brief itself and suggest work, while nothing it calls can run, launch, approve, send text or read a file (FR-11).

## Acceptance Criteria

1. **Framing** (AD-90, E11). Core `officina-core::mcp` holds the JSON-RPC 2.0 types, `McpTool` (`closed_enum!`), the tool schemas and bounds, and a pure dispatch; host `officina-host::mcp` runs the loop on stdio, newline-delimited, by hand on `serde_json`: a line over 1 MiB, read with a bound of 1 MiB + 1 bytes and discarded to its newline (D-161), or a batch array gets `-32600`; `server/discover` gets `-32601`; `initialize` echoes the client's `protocolVersion` when `MCP_PROTOCOL_VERSIONS` holds it, else the newest it holds, with capabilities `{"tools":{}}` only; `ping` gets `{}`; notifications are ignored; any other method with an id gets `-32601`; end of stdin exits 0. Proof: `tests/vectors/mcp.json` (formatter-exempt) through `crates/officina-core/tests/mcp_vectors.rs::{framing_cases_match, every_tool_and_error_has_a_case}` (iterates `McpTool::ALL` and `McpError::ALL`, the error codes as a `closed_enum!`, D-161); host `crates/officina-host/src/mcp.rs::tests::an_oversized_line_is_discarded_to_its_newline`; core `crates/officina-core/src/mcp.rs::tests::{schemas_are_pinned, no_schema_takes_a_path}`.
2. **The subcommand and its start check** (AD-73, AD-90, FR-11, D-144). `officina mcp [--floor <path>]`: the Officina home comes from `OFFICINA_HOME` (the MCP config of story 5.16 sets it); `OFFICINA_NO_EXCERPTS` and `OFFICINA_CLAUDE_BIN` come from the same config (D-128); it serves only when the normalised `CLAUDE_PROJECT_DIR` equals `<home>/orchestrator/workdir`, else it prints one line on stderr and exits 1; a usage error exits 64; it never exits 2; it never logs its environment, and stderr carries fatal errors only. The usage text lists `mcp`. Proof: CLI `crates/officina/tests/mcp.rs::{a_wrong_project_dir_exits_1_with_one_line, a_usage_error_exits_64, stderr_never_holds_an_environment_value}` (the base environment holds a marker value under a messaging-token name; stderr and every output are searched for it); `crates/officina/tests/readme.rs::readme_mentions_every_subcommand`.
3. **The read model** (AD-90, AD-53, D-82). Each call opens the registry read-only, reads the approved catalogue and the journal tail with `relay --json`'s readers, and builds the board through the `Agents` read half of `spawn`, cached 5 s, whose rule removes `CLAUDE_CODE_DISABLE_AGENT_VIEW`, which the orchestrator's environment sets (D-128, review F4); with the window closed the outputs carry `ingestion.pending_bytes`; nothing is written but proposal files. Proof: `mcp.rs::{reads_with_the_window_closed_and_reports_pending_bytes, the_registry_file_is_unchanged_after_every_tool}` (D-82's fingerprint of `registry/`, `-wal` and `-shm` ignored).
4. **Read tools** (AD-90, FR-11, D-144, D-145). `briefing` (AD-83's function, cursor the previous spawned orchestrator start, `None` without one), `subjects`, `subject {id}`, `board`, `relay_activity {limit <= 50}`, `templates` (the approved snapshot's only) and `proposals {status?}` (D-145), each proposal as `ProposalView` (D-576). Every output starts with one constant note saying that fields marked untrusted are session data; `mcp::dispatch` serializes every output inside `untrusted::wrapped`, so each `Untrusted` field, whatever the payload, is emitted as `{"untrusted": "..."}` with no per-tool list of fields (D-153, review F8); with `no_excerpts` from the floor or from `OFFICINA_NO_EXCERPTS` no excerpt is served (review F1); outputs are at most 64 KiB, the briefing 32 KiB. Proof: `mcp.rs::{each_read_tool_answers_from_the_fixture, session_text_is_wrapped_untrusted, no_excerpts_serves_no_excerpt, no_excerpts_from_the_environment_serves_no_excerpt, the_board_tool_works_with_agent_view_off_in_the_parent, the_briefing_cursor_is_the_previous_start}` (the fifth starts the server with `CLAUDE_CODE_DISABLE_AGENT_VIEW=1`, which the fake claude refuses as `claude` does, D-129); core `mcp.rs::tests::outputs_are_bounded`.
5. **Proposal tools** (AD-90, AD-91 file part, FR-11 as amended by amendment 7, FR-12). `propose_recap {target: {session_id} | {subject_id}, done, blocking, decision_expected}` and `propose_work_order {template, project, values: {param: value}, subject: {id} | {new: name}, rationale}`: input at most 8 KiB, texts 500 characters, values 64, a new subject name 80, at most 16 values; then the rate limits, 30 files per hour (counted from file ids and registry rows) and 200 pending, past which the tool returns `isError` with the reason; then the file `<home>/orchestrator/proposals/<now_ms>-<16 hex>.json`, written by AD-10's `atomic_write`, at most 16 KiB, holding `schema`, `kind`, the fields, `orchestrator_session_id` (from `CLAUDE_CODE_SESSION_ID`, a label) and `written_at_ms`; the tool returns its id and validates nothing else. Proof: `mcp.rs::{each_proposal_tool_writes_one_file_and_returns_its_id, oversized_inputs_are_refused, the_31st_proposal_in_an_hour_is_refused_with_its_reason, the_201st_pending_is_refused, a_sentence_as_a_value_is_written_for_ingestion_to_judge}`; host `crates/officina-host/src/mcp.rs::tests::proposal_file_names_are_unique_and_atomic`.
6. **Nothing else** (FR-11, AD-90, P-5, PRD section 9 item 6's second half, D-577). No tool runs, launches, approves, sends text to a session, reads a path or reaches the network; no schema takes a path; no tool reads, answers or names a Claude Code permission prompt, so the orchestrator never answers one, now or once Slice D's router exists (its request files under `relay/prompts/`, its decision writer under `window-authority`, which `officina.exe` does not hold); `officina.exe` still holds no authority. Proof: core `mcp.rs::tests::no_schema_takes_a_path`; the CI step "officina holds no authority"; CLI `mcp.rs::no_tool_spawns_anything_but_claude_agents` (the fake claude's log holds `agents` calls only); CLI `mcp.rs::no_tool_reads_or_writes_a_permission_prompt` (a request file holding a marker is seeded under the scratch home's `relay/prompts/`; every tool of `McpTool::ALL` is called; the marker appears in no output and the folder's listing and bytes are unchanged).
7. **Scripted client** (AD-95 MCP row). `crates/officina/tests/mcp.rs` drives the real `officina mcp` with E11's handshake (`server/discover` first, then `initialize`, `notifications/initialized`, `tools/list`, `tools/call`) and calls every tool. Proof: `mcp.rs::{the_e11_handshake_completes, tools_list_names_every_tool}`.
8. **Injection battery** (AD-95, D-147). Sections `mcp_tool` (sentences, option strings, `--settings`, `/` and `!` prompts, bidi, oversized text, unknown templates, "launch now" text as values or texts of `propose_work_order`: either refused by the bounds or written as a file, never anything else, with zero `--bg` spawns) and `mcp_output` (a marker, with markup, bidi and "launch now" text, seeded into every source of session text that reaches an output: card names, prompts, answer heads and tails, `last_message_tail`, registry excerpts, the model's reason, `gate_denied.command_excerpt`, session-supplied `params`, `refused.detail`, proposal texts and invalid reasons; every occurrence of the marker in every output sits inside a wrapper, D-147, review F8). Proof: `crates/officina/tests/injection.rs::{mcp_tool_cases, mcp_output_cases, every_section_family_and_refusal_has_a_case}`.
9. **Zero write** (AD-95, D-80). A scripted `officina mcp` session joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
10. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Added: `officina mcp`; README "The orchestrator (v0.5, in progress)" and "Command line": what the server serves, that it only runs inside the orchestrator's folder, that proposals are untrusted files. Proof: review; `readme.rs`.

## Tasks

- [ ] `crates/officina-core/src/mcp.rs`: types, `McpTool`, schemas, bounds, dispatch, `MCP_PROTOCOL_VERSIONS` (AC 1, 4-6)
- [ ] `tests/vectors/mcp.json`, `crates/officina-core/tests/mcp_vectors.rs` (AC 1)
- [ ] `crates/officina-host/src/mcp.rs`: loop, start check, read model, proposal file writer, rate limits (AC 2-5)
- [ ] `officina/main.rs`: `mcp` subcommand and usage (AC 2)
- [ ] `crates/officina/tests/mcp.rs` (AC 2-7)
- [ ] `mcp_tool` and `mcp_output` sections (AC 8)
- [ ] Zero-write, `CHANGELOG.md`, README (AC 9, 10)

## Dev Notes

- Requirements: FR-11 as amended (amendment 7), FR-12's file side, PRD section 7 Gesture and Zero write rows, PRD section 9 item 6's second half. Decisions: AD-10, AD-53, AD-73, AD-83, AD-90, AD-91 (file), AD-95, AD-181 (the router's files, read here only to be refused). Facts: E11 (handshake, the server's environment carries the session's messaging socket and token), E24 (gate 1's containment half passed before this story starts). Risks: R43. Epic decisions: P-5, D-9, D-34 (Epic 2); D-82 (Epic 3); P-24, D-124, D-128, D-129, D-130, D-144, D-145, D-147, D-153, D-161, D-561, D-563, D-575, D-576, D-577.
- Seams (D-124, read through `main`, D-563): `crates/officina/src/main.rs` and its usage (v0.1), `crates/officina/tests/readme.rs` (2.10), `relay --json`'s readers (`officina-core::relay::view`, 2.6), `subjects --json`'s view (`officina-core::registry::view`, 3.4), the registry reader (3.1).
- `propose_answer` (PRD section 9 item 3, AD-182) is not this story's: story 5.21 adds it to `McpTool` with its schema and its `mcp.json` and `mcp_tool` cases, as each `VectorKind` joins with its story (D-575). Nothing here anticipates it; `McpTool::ALL`'s coverage tests take it in when it lands.
- The orchestrator's environment is the contained allow list (D-561), so `officina mcp`, its child, sees none of the author's `OFFICINA_*` names but those its MCP config's `env` passes (story 5.16): the home, the excerpt switch and the `claude` path. AC 2 reads them there only.
- The start check prevents accidents, not forgery (FR-11): any process running as the author can set the variable or write a proposal file, which is why ingestion (5.17) validates everything and proposals act only through a click.
- `orchestrator_session_id` is a label: the server reads the variable's value to write it, and never logs it or any other variable.
- Proposal statuses come from the registry, which the window fills in story 5.17; until then `proposals` reports written files as `pending`.
- `proposals` returns every orchestrator session's proposals, not only the calling session's (D-145): FR-11's "its own" reads as Officina's orchestrator's, since a restart is a new session (P-24, review F45).
- Wrapping is the serializer's (D-153): a new payload field typed `Untrusted` wraps itself in every tool's output, and the `mcp_output` section seeds a marker into each source so a field left plain fails.
- The proposal writer is one host function, reused by `examples/mcp_crash_helper.rs` in story 5.17.

## Testing

- Core unit: `crates/officina-core/src/mcp.rs`; core integration: `crates/officina-core/tests/mcp_vectors.rs`.
- Host unit: `crates/officina-host/src/mcp.rs`.
- CLI integration: `crates/officina/tests/{mcp,injection,quota_doctor,readme}.rs`.
- CI: "officina holds no authority".

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: home, excerpt switch and `claude` path from the MCP config (F1); the board works with agent view off in the parent (F4); wrapping through `untrusted::wrapped`, a marker in every source (F8); bounded line read, `McpError` (F39); D-145 recorded as a departure (F45)
- 2026-10-28 reworked against the addendum and v0.4 as built: no tool touches a Claude Code permission prompt, proven against Slice D's folder (PRD section 9 item 6's second half, D-577); `propose_answer` left to story 5.21 (D-575); proposals carried as `ProposalView`, a name v0.3 does not export (D-576); the server's environment comes from the contained allow list and its MCP config (D-561); seams cited by file (D-563)
