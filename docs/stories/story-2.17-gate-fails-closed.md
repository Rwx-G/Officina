# Story 2.17: The gate fails closed

**Status:** in-progress

## Story

As the author whose guarded repositories run in bypass mode,
I want the relay gate to refuse a call when the gate itself fails, where it guards,
so that a crash, a hang or a missing binary cannot let a guarded program through.

## Acceptance Criteria

1. Doctor's printed hooks block gives the gate's `PreToolUse` entry `"onFailure": "block"` (research notes E18: Claude Code 2.1.295 and later block the call when that hook cannot start, times out, or exits with a code other than 0 or 2). The buffered events keep failing open. Proof: `doctor::tests::only_the_gate_entry_fails_closed`; `doctor_relay.rs::a_gate_that_fails_open_fails_doctor` reads the field in the block doctor prints.
2. An installed `officina hook` entry under `PreToolUse` without that exact value, in any of its groups, is reported in the `hooks` line as `PreToolUse fail-closed (onFailure: "block")`. It is a failure, like a matcher that leaves a tool out (D-31, D-452), and pasting the printed block clears it. Another hook beside it, or the field on a buffered event, changes nothing. Proof: `doctor::tests::a_gate_entry_that_fails_open_is_reported`; `doctor_relay.rs::a_gate_that_fails_open_fails_doctor`.
3. While a project is guarded, doctor reads `claude --version` and prints a `gate failure mode` line: ok from the first version that honours the field, a failure before it, a warning when no `major.minor.patch` can be read. There is no line without a guarded project. Proof: `contract::tests::a_version_is_the_first_word_in_three_numbers`; `doctor::tests::a_claude_code_without_on_failure_fails`; `doctor_relay.rs::a_gate_that_fails_open_fails_doctor` runs it with the fake claude at the version before the minimum and at the default.
4. A panic while the gate decides falls back to locating the call alone. A call in a guarded project is denied with a reason saying the gate failed, and so is a call the fallback cannot locate. A call outside every guarded project gets no decision. Each case journals one `gate_error` line with cause `panic`, without the command excerpt; the line names the project when there is one. The hook exits 0, never 2. Proof: `relay::gate::tests::{panic_outside_guarded_projects_gives_no_decision, panic_in_a_guarded_project_denies, panic_that_cannot_locate_the_call_denies, the_fallback_lookup_finds_the_guarded_project, an_unreachable_journal_does_not_change_the_answer}`; the doc test of `fault_reason`.
5. Nothing else changes for the gate's decisions, its cost or the hook's exit code. Proof: `gate.rs`, `hook.rs`, `tests/vectors/gate.json` and `perf.rs` unchanged and green.
6. README (wiring the hooks, doctor, the gate), `CHANGELOG.md` `[Unreleased]` (Security), research notes E18, the AD-28 amendment and D-452 say what the code does. Proof: `readme.rs::readme_states_the_gate_fails_closed` ties the README's field and version to `ON_FAILURE` and `ON_FAILURE_SINCE`.
7. Manual check (owner): paste the block doctor prints again, so the installed gate entry carries the field, and `officina doctor` shows `hooks` and `gate failure mode` ok on the workstation's Claude Code.

## Tasks

- [x] `ON_FAILURE_SINCE` and `claude_version` in core (AC 3)
- [x] Doctor: printed entry, drift, version line (AC 1 to 3)
- [x] Gate: fallback lookup and fault reason on a panic (AC 4)
- [x] README, CHANGELOG, research notes, AD-28 amendment, D-452 (AC 6)
- [ ] Owner's paste and doctor run (AC 7)

## Dev Notes

- Source: the research note on permission decisions (git-ignored, `_bmad-output/planning-artifacts/research/`), sections on hook timeouts and failure, checked against the hooks reference and the changelog on 2026-10-28 (E18).
- `onFailure` is a field of the hook entry (the handler object beside `type` and `command`), not of the matcher group. It does not tell projects apart, so the two cases are served at two levels. Inside the process, the hook decides per project and stays silent outside guarded ones. A failure the process cannot catch blocks every call the matcher names, in every project, on a workstation that guards one (D-452).
- `.claude/rules/officina-rust.md`: nothing Claude Code runs may exit 2. The in-process fault answers with the deny JSON, as every deny does.
- Decisions: D-452 (this story), D-18, D-31, AD-28.

## Dev Agent Record

### Debug Log

- **Docs checked first.** The hooks reference states the field, its values (`"continue"`, the default, and `"block"`), the minimum version 2.1.295, and what counts as a failure. It also says exit 2 already blocks a `PreToolUse` call. The changelog's 2.1.295 entry says the same. The owner runs 2.1.312.
- **What the hook did before.** `officina hook` exits 0 on every path, panics included. The gate's panic gave no decision, so the call ran (AD-28's fail open). With no Officina home, or a payload that is not JSON, it gives no decision; those paths stay as they were, since no guarded project can be read there.
- **Level of the drift.** A failure, not a warning. A gate entry that fails open leaves a path where a guarded call runs unchecked, as a matcher missing a tool does, and the fix is the same paste. An old Claude Code fails for the same reason; there the fix is an update, not a paste.
- **Fallback.** `evaluate` was split into `contain` (the inputs and their faults) and `call_dirs` (the folders, as given then resolved), so `guarded_project_of` repeats the lookup without the lexer and decision. A panic in the fallback too means the gate cannot tell, so it denies.

### Completion Notes

- AC 1 to 3: 1a5cc7d.
- AC 4 and 5: de6ee6b.
- AC 6: both commits (README, CHANGELOG) and the `docs(planning)` commit (E18, AD-28 amendment, D-452, this story).
- AC 7: open, the owner's.

## File List

- `crates/officina-core/src/contract.rs`, `crates/officina-core/src/relay/{gate,journal}.rs`
- `crates/officina-host/src/doctor.rs`, `crates/officina-host/src/relay/gate.rs`
- `crates/officina/src/main.rs`, `crates/officina/tests/{doctor_relay,readme}.rs`
- `app/src/lib/generated/GateFault.ts`
- `README.md`, `CHANGELOG.md`, `docs/architecture-v0.2.md`, `docs/research-notes.md`, `docs/stories/epic-2-decisions.md`, `docs/stories/sprint-status.yaml`, `docs/stories/story-2.17-gate-fails-closed.md`

## Change Log

- 2026-10-28 created and implemented (D-452). Status in-progress until the owner's paste (AC 7).
