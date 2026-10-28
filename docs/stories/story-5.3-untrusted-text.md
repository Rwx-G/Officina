# Story 5.3: `Untrusted<T>` and the display type

**Status:** Approved

## Story

As the author whose sessions' and models' words will soon sit next to a launch button,
I want text that came from a session or a model to be a distinct type in core that can be shown and measured but never handed on as a string,
so that the only ways from session text to a process are the ones the launch path's tests watch (AD-95), and a refactor cannot open a new one silently.

## Acceptance Criteria

1. **The type** (AD-74). `officina-core::untrusted` declares `pub struct Untrusted<T>(T)` with no public field and no `Deref`, `AsRef`, `Borrow`, `Display`, `From` or `Into` towards `String` or `&str`; `Untrusted::new(T)`; `display(&self) -> DisplayText<'_>`, where `DisplayText` implements only `Serialize`; `char_count`, `truncated(n) -> Untrusted<String>` (cut on a character boundary) and `first_line_matching(&[&str]) -> Option<u32>` (1-based). `Untrusted<T>` serializes and deserializes transparently, and ts-rs sees its inner type. Proof: core `crates/officina-core/src/untrusted.rs::tests::{serializes_as_its_inner_value, deserializes_from_its_inner_value, display_serializes_as_the_text, truncated_cuts_on_a_char_boundary, first_line_matching_counts_from_one, first_line_matching_is_none_without_a_match}`.
2. **Type proofs** (AD-74, FR-3). `compile_fail` doctests on `untrusted.rs`: `&*u`, `u.as_ref()`, `String::from(u)`, `let s: String = u.into()`, `format!("{}", u)`, `format!("{}", u.display())` and `let s: &str = u.display()` do not compile; a positive doctest serializes `u.display()`. The doctests on `WorkOrder::validate` arrive with that function (story 5.5). Proof: `cargo test -p officina-core --doc`.
3. **Payload fields** (AD-74, D-130; review F8). Every core payload field whose text came from a session or a model becomes `Untrusted<String>` (or `Option` of it): `Card.name`, `Card.last_prompt`, `Card.last_message`, `Card.last_message_tail` (D-182); `ApprovalView.session_name`; in `RelayView`'s recent entries, `gate_denied.command_excerpt`, the session-supplied `params` values and `refused.detail`; in `RegistryView`, session names, prompt and answer excerpts (head and tail) and the model's reason (story 3.11's `model_reason`). Author-typed names (subjects, projects, actions) and Claude Code values (`waiting_for`, `cwd`, ids) stay plain. Parse records (`AgentRecord`, `EventLine`, journal lines, request files) keep plain strings, and the wrap happens where the payload field is built. Proof: core unit tests `crates/officina-core/src/board.rs::tests::card_texts_serialize_as_plain_strings`, `crates/officina-core/src/relay/view.rs::tests::session_supplied_journal_texts_are_untrusted`, `crates/officina-core/src/registry/view.rs::tests::excerpts_and_model_reason_serialize_as_plain_strings`.
4. **The wrapped mode** (D-153; review F8). `untrusted::wrapped(f)` runs `f` with a thread-local flag set, reset by a guard on return or panic; while it is set, `Untrusted::serialize` emits `{"untrusted": "<text>"}`, otherwise the plain string. Only `mcp::dispatch` (story 5.15) calls it. Proof: core `untrusted.rs::tests::{wrapped_mode_wraps_every_untrusted_field_of_a_nested_payload, outside_wrapped_mode_the_bytes_are_unchanged, the_flag_is_reset_after_a_panic, the_flag_does_not_cross_threads}`.
5. **Nothing on the wire changes** (AD-74). Serialized bytes of every payload and the TypeScript bindings stay identical. Proof: `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types` passes with no change under `app/src/lib/generated/`; `crates/officina-core/tests/classification_vectors.rs` and the CLI tests `crates/officina/tests/{board,subjects,relay}.rs` pass unchanged; the e2e fixtures and Playwright suite pass unchanged.
6. **Frontend unchanged** (AD-74, `officina-frontend` rule). The frontend keeps rendering these fields with `{text}`; nothing in `app/src/` changes. Proof: `pnpm check`, `pnpm lint` (`svelte/no-at-html-tags`), the existing inert-text Vitest cases.
7. **Documentation** (D-149). `CHANGELOG.md` `[Unreleased]` under Security: session and model text is a distinct type in core that is shown and measured but never converted to a string argument; no behaviour change. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/untrusted.rs`: type, `DisplayText`, facts, serde and ts-rs handling, doctests (AC 1, 2)
- [ ] Convert the payload fields in `board`, `relay::approvals`, `relay::view` and `registry::view`; wrap where each field is built (AC 3)
- [ ] `untrusted::wrapped` and its guard (AC 4)
- [ ] Check bindings and fixtures are byte-identical (AC 5, 6)
- [ ] Update `CHANGELOG.md` (AC 7)

## Dev Notes

- Requirements: FR-3 (`Untrusted<T>` and `display()`), glossary "Untrusted text". Decisions: AD-74, AD-13 (bindings). Epic decisions: D-182 (Epic 4); D-130, D-153.
- Seams (D-124): `Card` (v0.1) and `Card.last_message_tail` (4.1), `ApprovalView` (story 2.8), `RelayView`'s journal entries (2.6, 2.9), `RegistryView` and the model's reason (stories 3.4 and 3.11).
- The wrapped mode exists so the MCP layer needs no list of which fields are untrusted (a second copy that would drift): every `Untrusted` field wraps itself when serialized inside `wrapped` (D-153). A thread-local is pure state, allowed in core by its `clippy.toml`.
- Payloads that are stored or cached hold `Untrusted<String>`; `DisplayText` only lives inside a serialization (D-130, review F43). Later stories create their fields as `Untrusted` from the start (meeting texts 5.10, review reason 5.13, MCP outputs 5.15, proposal texts 5.17).
- ts-rs: `#[cfg_attr(feature = "ts", ts(as = "String"))]` on each field, or one `impl TS for Untrusted<T>` delegating to `T`; pick the one that leaves `app/src/lib/generated/` byte-identical.
- What the doctests prove is that those snippets fail, not that no path exists: `serde_json::to_string` still yields a string (AD-74). The launch path's mutation test and the injection battery (5.7) carry the rest.
- Core code that must compare session text (binding by name, AD-81) compares the parse record before the wrap (D-130); no `PartialEq<str>` on `Untrusted`.
- No code outside `officina-core` should need `display()` except serialization; if the host needs a fact, add it to `Untrusted` as a method returning a non-text value.

## Testing

- Core unit and doctests: `crates/officina-core/src/{untrusted,board}.rs`, `src/relay/view.rs`, `src/registry/view.rs`.
- Core integration: `crates/officina-core/tests/{typescript_bindings,classification_vectors}.rs`.
- CLI integration: `crates/officina/tests/{board,subjects,relay}.rs`. Frontend: the full Vitest and Playwright suites, unchanged.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: `last_message_tail` and the relay view's session-supplied texts are untrusted; the wrapped serialization mode for MCP outputs (F8); stored payloads hold `Untrusted<String>` (F43)
