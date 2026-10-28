# Story 2.12: Every shell tool in the gate

**Status:** Review

## Story

As the author whose guarded repositories run in bypass mode on Windows,
I want the relay gate to cover every tool that runs a shell command, not only `Bash`,
so that a session cannot reach a guarded program through the PowerShell or Monitor tool.

## Acceptance Criteria

1. The gate's tool set is a core constant naming each shell-command tool with its rule (D-171): `Bash` and `Monitor` are lexed and decided as today (story 2.3's `decide`, on `tool_input.command`); `PowerShell` is denied whole in a guarded project; any other tool gets no decision. A `Monitor` call without `tool_input.command` (its WebSocket form) gets no decision. Proof: core unit test `crates/officina-core/src/relay/gate.rs::tests::every_shell_tool_has_a_rule` (iterates the constant with an exhaustive `match`), CLI tests `crates/officina/tests/gate.rs::{monitor_command_is_decided_like_bash, monitor_websocket_gets_no_decision}`.
2. A `PowerShell` call is denied when the project of `CLAUDE_PROJECT_DIR` or of the payload `cwd` is guarded (D-16), and under the broken-snapshot fallbacks of D-15, with a reason naming the guarded project, saying that Officina reads Bash commands only, and pointing at the Bash tool and `officina run --list`; in a free or undeclared folder it gets no decision. Each deny leaves one `gate_denied` line whose deny kind is `powershell_in_guarded_project` (no command excerpt beyond 280 characters, none under `no_excerpts`). Proof: `gate.rs::{powershell_in_a_guarded_project_is_denied, powershell_in_a_free_project_gets_no_decision, powershell_under_a_broken_snapshot_is_denied, powershell_deny_is_journaled_once}`, driven by hand-written, redacted payloads under `crates/officina/tests/data/pretooluse/` with E15's field set and `tool_name` `PowerShell`.
3. Doctor (D-18 as amended by D-171): the `PreToolUse` registration's matcher names every tool of the AC 1 constant (`Bash|PowerShell|Monitor`, built from the constant); doctor requires a matcher naming each of them once a project is guarded, and reports which tool a merged matcher leaves out. Proof: `crates/officina-host/src/doctor.rs::tests::{block_matcher_names_every_shell_tool, a_matcher_naming_bash_only_is_missing_powershell_and_monitor}`, CLI `crates/officina/tests/quota_doctor.rs::doctor_with_a_guarded_project_requires_every_shell_tool` (paste-back of the printed block).
4. The gate's budget and zero-write guarantees hold for the new tools: `perf.rs` keeps its numbers (the tool check is a constant lookup before any lexing), and a PowerShell deny joins the Claude config dir fingerprint test. Proof: `crates/officina/tests/perf.rs` unchanged and green; `quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
5. The README relay section says which tools the gate reads, that PowerShell is refused in guarded projects (Claude Code turns it on by default on Windows for claude.ai accounts), and that a project's own `CLAUDE.md` can tell sessions to use Bash there; it names the constant, never a list. Proof: `crates/officina/tests/readme.rs` (story 2.10) gains `readme_names_the_gate_tools_constant` or an equivalent check that the README's sentence and the constant agree.
6. A deny of a PowerShell call inside an interactive session on the workstation (PowerShell tool on) is recorded in `docs/research-notes.md`. Proof: manual, owner check, added to `docs/backlog.md` High next to story 2.4 AC 18.

## Tasks

- [x] Core: gate tool constant and per-tool rule; `Monitor` through `decide`; `PowerShell` deny kind and reason (AC 1, 2)
- [x] Host: hook dispatch on the constant; journal deny kind (AC 2)
- [x] Doctor: matcher built from the constant, missing-tool report (AC 3)
- [x] Tests and payload fixtures (AC 1-4); README and readme test (AC 5); backlog entry (AC 6)
- [x] Update `CHANGELOG.md` `[Unreleased]` (Security)

## Dev Notes

- Source: Claude Code tools reference, "PowerShell tool": on Windows with Git Bash installed the tool is on by default for claude.ai and Console accounts; PreToolUse hooks receive `tool_input.command` with the Bash fields; hooks that inspect shell commands should match `Bash|PowerShell`. `Monitor` runs a command in the background through the same shell as Bash (tools reference table). Checked 2026-10-28; record the page as an S-reference in the story's Completion Notes.
- Why deny PowerShell instead of lexing it: a PowerShell lexer is a second parser of a second language with its own expansion rules; the session keeps the Bash tool, so the capability it loses is one Officina cannot read. Free projects are untouched.
- A `cd` into a guarded project from a free folder stays a documented limit (story 2.3), for every tool.
- Decisions: D-171 (this story), D-16, D-15, D-17, D-18.

## Testing

- Core unit: `crates/officina-core/src/relay/gate.rs`.
- Host unit: `crates/officina-host/src/doctor.rs`.
- CLI integration: `crates/officina/tests/{gate,quota_doctor,readme,perf}.rs`.

## Dev Agent Record

### Debug Log

- **Order.** The coordinator asked for this story before story 2.8, which was nearly done and not committed: its paths were stashed, this story was built and committed on the tree of story 2.10, and story 2.8 resumes on top of it.
- **Decisions where the documents are silent**, each the reading that fails closed or matches the architecture:
  - The constant is `officina_core::relay::gate::SHELL_TOOLS`, `(name, ToolRule)` pairs; `ToolRule` (`Lexed`, `DeniedInGuardedProject`) is a closed enum, so a rule added later does not compile without its case in `every_shell_tool_has_a_rule` and in `ShellCall::from_payload`. `decide_call(tool, command, ...)` applies a tool's rule; `decide` is unchanged for the command text. Tool names match exactly, as Claude Code writes them (`bash` or `powershell` get no decision, like any tool that runs no shell command).
  - A lexed tool's call without `tool_input.command` (Monitor's WebSocket form) is not a gated call: no decision and no journal line. A PowerShell call is refused whole with or without a command; its `gate_denied` line keeps the command's excerpt when there is one (280 characters, none under `no_excerpts`), and `matched` is absent.
  - The deny kind `powershell_in_guarded_project` joins `DenyKind`, so the journal, the relay view's bindings (`DenyKind.ts` regenerated) and the vector set know it. The vector set (`tests/vectors/gate.json`) gains an optional `tool` per case, the Bash tool when absent, replayed through `decide_call`; the coverage test now also fails when a tool of the constant runs in no case, and the new kind needs its case like every other.
  - Doctor's `Registration` carries the tools its matcher must name (a `Vec`, built from the constant by `gate_registration()`). An event is reported missing when no matcher group running `officina hook` covers it; for the gate, a tool counts when any such group's matcher names it, so two groups sharing the tools pass, and a matcher merged before this story is reported as `PreToolUse (<the tools it leaves out>)`.
  - The `CLAUDE.md` paragraph doctor prints (story 2.10) gains one sentence built from the constant: run commands with the tools the gate reads, the ones refused whole are refused here. AC 5's "a project's own CLAUDE.md can tell sessions to use Bash" is therefore generated, not left to the author.
  - README: the hooks JSON example with its matcher is replaced by prose naming `SHELL_TOOLS` (a matcher spelled out would be a transcription); `readme_names_the_gate_tools_constant` checks that the constant is named, that no `Tool|Tool` pair of it appears, and that each tool refused whole is said to be refused.
- **Adversarial pass.** Tried: PowerShell with a harmless command, with no command, from a guarded `cwd` under a free root and from the guarded root with the shell elsewhere (D-16), under a corrupt snapshot (D-15 fallback), in a free and an undeclared folder; Monitor with a guarded program, an ordinary command and no command; lower-case tool names; a matcher with spaces, in another order, with an extra tool, split over two groups, or naming one tool only; the perf budget with the new lookup. No miss found beyond the AC tests; the lower-case names are covered by `every_shell_tool_has_a_rule` and `only_shell_tool_calls_are_gated`.
- **Not run here:** AC 6 needs an interactive session with the PowerShell tool on; listed in `docs/backlog.md` (High) beside story 2.4 AC 18.

### Completion Notes

- S-reference: Claude Code tools reference, "PowerShell tool" (on Windows with Git Bash installed the tool is on by default for claude.ai and Console accounts; `PreToolUse` hooks receive `tool_input.command`; hooks that inspect shell commands should match `Bash|PowerShell`; Monitor runs its command through the Bash shell), checked 2026-10-28 by the story's author, as the Dev Notes record; not fetched again here.
- AC 1: core `crates/officina-core/src/relay/gate.rs::tests::every_shell_tool_has_a_rule`; the `tool` cases of `tests/vectors/gate.json` replayed by `gate_vectors.rs::every_case_matches` and required by `every_constant_appears_in_a_case`; host `relay/gate.rs::tests::only_shell_tool_calls_are_gated`; CLI `crates/officina/tests/gate.rs::{monitor_command_is_decided_like_bash, monitor_websocket_gets_no_decision}`.
- AC 2: `gate.rs::{powershell_in_a_guarded_project_is_denied, powershell_in_a_free_project_gets_no_decision, powershell_under_a_broken_snapshot_is_denied, powershell_deny_is_journaled_once}` on `crates/officina/tests/data/pretooluse/powershell-guarded-program.json`, which `fixtures_carry_the_verified_field_set` holds to E15's field set.
- AC 3: host `crates/officina-host/src/doctor.rs::tests::{block_matcher_names_every_shell_tool, a_matcher_naming_bash_only_is_missing_powershell_and_monitor}` (and `pre_tool_use_without_the_matcher_is_missing`, `block_includes_pre_tool_use_only_with_a_guarded_project` on the built matcher); CLI `crates/officina/tests/quota_doctor.rs::doctor_with_a_guarded_project_requires_every_shell_tool`.
- AC 4: `crates/officina/tests/perf.rs` unchanged and green in release (gate own cost 3.2 ms, in-process p95 2.8 ms, hook p95 17.9 ms); `quota_doctor.rs::no_command_writes_under_the_claude_config_dir` now sends a PowerShell deny.
- AC 5: `crates/officina/tests/readme.rs::readme_names_the_gate_tools_constant`; `doctor_relay.rs::claude_md_paragraph_lists_the_projects_actions` checks the generated sentence.
- AC 6: open, the owner's to run (`docs/backlog.md`, High).
- Gates at the end of the story: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --features officina-core/ts -- -D warnings` and the Linux job's crate set; `cargo test --workspace --features officina-core/ts` 427 passed, 0 failed, 5 ignored; TS bindings regenerated (`DenyKind.ts`) and the drift test green; `cargo test --release -p officina --test perf -- --ignored` 5 passed; `cargo deny check` ok; `pnpm check`, `lint`, `test` (20 passed); fixtures rebuilt from the real CLI and `pnpm e2e` 28 passed.

## File List

- `crates/officina-core/src/relay/gate.rs`, `crates/officina-core/tests/gate_vectors.rs`, `tests/vectors/gate.json`, `app/src/lib/generated/DenyKind.ts`
- `crates/officina-host/src/relay/{gate,doctor}.rs`, `crates/officina-host/src/doctor.rs`
- `crates/officina/tests/{gate,quota_doctor,doctor_relay,readme}.rs`, `crates/officina/tests/data/pretooluse/powershell-guarded-program.json`
- `README.md`, `CHANGELOG.md`, `docs/backlog.md`

## Change Log

- 2026-10-28 created after the v1 PRD draft found that the gate reads `Bash` only while Claude Code enables a PowerShell tool by default on Windows (D-171)
- 2026-10-28 implemented: `SHELL_TOOLS` with a rule per tool, Monitor lexed, PowerShell refused whole in guarded projects, the doctor matcher and missing-tool report built from the constant, the generated `CLAUDE.md` sentence, README and its check; status Review, AC 6 (manual) open
