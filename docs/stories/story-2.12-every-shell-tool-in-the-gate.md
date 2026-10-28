# Story 2.12: Every shell tool in the gate

**Status:** Approved

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

- [ ] Core: gate tool constant and per-tool rule; `Monitor` through `decide`; `PowerShell` deny kind and reason (AC 1, 2)
- [ ] Host: hook dispatch on the constant; journal deny kind (AC 2)
- [ ] Doctor: matcher built from the constant, missing-tool report (AC 3)
- [ ] Tests and payload fixtures (AC 1-4); README and readme test (AC 5); backlog entry (AC 6)
- [ ] Update `CHANGELOG.md` `[Unreleased]` (Security)

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

### Completion Notes

## File List

## Change Log

- 2026-10-28 created after the v1 PRD draft found that the gate reads `Bash` only while Claude Code enables a PowerShell tool by default on Windows (D-171)
