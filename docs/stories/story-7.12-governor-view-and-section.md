# Story 7.12: The governor view, its CLI and the Governor section

**Status:** Approved

## Story

As the author who wants to know, in one place, why nothing runs and what would,
I want a Governor section beside the Workshop section, a read-only `officina governor --json`, briefing lines and no header chip, all built from one view that carries numbers and never a run's text,
so that I can read each template's evidence, each order's eligibility and the shadow's would-launch decisions, correct a merge, link my routine subjects and switch the shadow, without anything here launching or interrupting me (FR-17, FR-18 Slice A, J4).

## Acceptance Criteria

1. **`GovernorView`** (AD-144 Slice A, D-330). Built in core by `governor::view::build`, `GOVERNOR_VIEW_SCHEMA = 1`, emitted as `governor://updated` on byte change (AD-5), separate from `OrchestratorView` and `WorkshopView`: mode and self-stop; the state line (decision kind, first cause with its numbers, every cause, `next_change_ms`); quota (each observation with source, anchored time and values, the combined readings, projection, reserve, ratios with their samples, allowances with their source, this week's autonomous points against the cap, the probe's last outcome, back-off, pause and today's count); attention (count against ceiling, review spent and pending against budget); shadow evidence (story 7.11); standing orders (assessment, per-order causes, state and reason, effective cadence, next due time, linked routine subjects or the "no routine subject" flag); autonomous runs ended without a package, `no_change` or quarantined; per template, story 7.6's figures and story 7.5's outcomes with corrections, and whether the gate switch is offered. Lists hold at most 50 rows; time-relative text is the frontend's. No field holds a run's text. Proof: core `crates/officina-core/src/governor/view.rs::tests::{lists_hold_at_most_50_rows, the_gate_switch_is_offered_only_when_the_writer_would_accept, an_order_without_a_link_carries_the_flag}`; story 7.2's source scan covers `view.rs`; `crates/officina-core/tests/typescript_bindings.rs` regenerated.
2. **The read half and the CLI** (AD-127, AD-144, FR-17 as amended by amendment 11, D-330). `officina-host::governor` builds the view read-only from disk (approved snapshot, registry read-only, journal, quota file, event buffer); `officina governor --json` prints it, its state line from the journal's latest decision line, since the CLI decides nothing and runs no probe; exit 0, 3 when the registry cannot be read (D-47), 64 on usage, never 2. Proof: CLI `crates/officina/tests/governor_cli.rs::{governor_json_prints_the_view, its_state_line_is_the_latest_decision_line, an_unreadable_registry_exits_3, a_usage_error_exits_64}`; `crates/officina/tests/readme.rs::readme_mentions_every_subcommand`.
3. **Commands** (AD-144 Slice A, D-330). `get_governor` (reads the cached view, and sends story 7.5's `MergePass` when the last pass is older than 5 minutes), `link_routine_subject(order, subject_id)` and `unlink_routine_subject(order, subject_id)` join `set_governor_mode`, `correct_merge` and `set_gate_on_merges`; links are stored with source `author`; a link the writer cannot accept is refused with `LinkRefusal` (`unknown_order`, `unknown_subject`, `other_project`, `not_routine`, `merged`), a `closed_enum!`. Since v0.3 marks a subject routine when it is archived (P-10, D-62), an archived routine subject is linkable: a link is coverage evidence for the "no routine subject" flag and PRD section 2.3, not a filing target (D-330 as amended by review F13). Each command is in `build.rs`, the capabilities, `ipc.ts` and `e2e-mock.ts`. Proof: host unit `crates/officina-host/src/governor/writers.rs::tests::{every_link_refusal_is_reachable, a_link_is_stored_with_source_author, an_archived_routine_subject_is_linkable, get_governor_asks_a_merge_pass_after_5_minutes}` (the first iterates `LinkRefusal::ALL`); `app/src-tauri/src/main.rs::tests::every_declared_command_is_allowed`.
4. **Fixtures** (AD-144, D-25, D-72, D-181, D-330). `crates/officina-host/examples/governor_fixture.rs` (`required-features = ["window-authority"]`) seeds journals, registry rows, quota files and event lines at explicit `now_ms` for each scenario (a shadow week with would-launch decisions; each first cause the view can show; a template with merges and corrections; an order suspended; probe paused and backed off) and prints the view through the CLI's host function at that `now_ms`; `app/scripts/e2e-fixtures.mjs` and D-181's index gain kind `governor`. Proof: the fixture script's self-check that each scenario's first cause matches the scenario's declared intent (Epic 4 review F24).
5. **The Governor section** (AD-145 Slice A, FR-17). `app/src/components/governor/GovernorSection.svelte` beside the Workshop section holds `GovernorMode.svelte`, `GovernorState.svelte`, `QuotaPanel.svelte`, `AttentionPanel.svelte`, `ShadowEvidence.svelte`, `StandingOrderTable.svelte` (with AD-55's subject picker offering routine-marked, non-merged subjects of the order's project, archived included), `HeldList.svelte` (empty until story 7.15) and `TemplateEvidence.svelte` (acceptance, merges, the two corrections, the gate switch when offered); `lib/governor.svelte.ts` is the store; `lib/governor-text.ts` holds exhaustive `Record<HoldCause, (numbers) => string>` and the like, so a new Rust variant fails `pnpm check`; numbers are formatted there from thousandths, and no rule is computed in TypeScript. Opening a package from the section opens v0.6's review panel. Proof: Vitest `app/src/lib/governor-text.test.ts::{every_hold_cause_text_renders_with_sample_numbers, thousandths_format_as_points}`; Playwright `app/e2e/governor.spec.ts::{a_shadow_week_shows_its_would_launch_decisions, each_first_cause_shows_its_text_and_numbers, the_next_change_is_shown, an_order_shows_eligibility_and_cadence, a_template_shows_merges_beside_acceptance, the_gate_switch_appears_only_when_offered}` with `page.clock.install` at the fixture's `now_ms`.
6. **Controls** (D-28, AD-144). Shadow, a correction, a link and the gate on act on a pointer click only (`event.detail > 0`); off, the gate off and an unlink are keyboard-reachable, since they only reduce. Proof: Vitest `app/src/components/governor/GovernorMode.test.ts::{shadow_ignores_a_keyboard_click, off_is_keyboard_reachable}`, `app/src/components/governor/StandingOrderTable.test.ts::{a_link_ignores_a_keyboard_click, an_unlink_is_keyboard_reachable}`, `app/src/components/governor/TemplateEvidence.test.ts::{a_correction_ignores_a_keyboard_click, the_gate_off_is_keyboard_reachable}`.
7. **No header chip in Slice A** (D-1209). The header gains nothing while the governor is off or in shadow: shadow, holding, its first cause and the held count show in the Governor section and the briefing; D-1209's single slot lands with story 7.15. Proof: Playwright `governor.spec.ts > shadow adds nothing to the header` (the status slot's children against a fixture with the governor off), `governor.spec.ts > the title and overlay ignore the governor`; `scene.spec.ts > eight projects fit a maximised Full HD window with the alert band` unchanged.
8. **Briefing and `officina mcp`** (AD-144, FR-17, D-330). The briefing (AD-83) gains, as folds over journal lines since its cursor (spill files included, never the ledger, D-1211), the governor mode, would-launch decisions since the cursor, suspensions, pauses and self-stops, all labelled shadow where they are; `officina mcp` serves them through `briefing` and `board`, adds no tool, and reaches no governor writer, being built without `window-authority`. Proof: core `crates/officina-core/src/orders/briefing.rs::tests::governor_sections_since_the_cursor_are_labelled_shadow`; CLI `crates/officina/tests/mcp.rs::the_briefing_tool_carries_the_governor_sections`; D-9's marker step on `officina.exe`.
9. **Zero write** (D-80). `officina governor --json` joins the fingerprint test. Proof: `crates/officina/tests/quota_doctor.rs::no_command_writes_under_the_claude_config_dir`.
10. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: the Governor section, `officina governor --json`. README "The governor (v0.7, in progress)": what the section shows, the CLI, that the header shows the governor only when it is on or stopped (story 7.15, D-1209). Proof: review.
11. **What every UI change carries** (D-1210, D-564, D-620). Every control of the section whose label, state or presence changes keeps its box (`StableLabel` or `data-stable-slot`) with a case in `stable-size.spec.ts`'s `CASES` (a `SCENES` entry where no scene shows it); a new colour is a token of `app.css` with its value in both dark blocks and a `PAIRS` entry in `src/tokens.test.ts`; every user-visible behaviour of the section has a Playwright case in `app/e2e/governor.spec.ts`, run in both themes where it asserts colours; run text renders as text. Proof: `stable-size.spec.ts`, `src/tokens.test.ts`, `governor.spec.ts` in both themes; `pnpm check` with `governor-text.ts`'s exhaustive records.
12. **The orchestrator reads it** (D-1211). `officina mcp`'s `briefing` serves the governor sections without seeding a ledger and without any governor writer (it runs in `officina.exe`); the orchestrator's tool list is `McpTool::ALL` unchanged by this story. Proof: CLI `crates/officina/tests/mcp.rs::the_briefing_tool_reads_the_governor_sections_from_the_journal`; `crates/officina/tests/mcp.rs`'s existing tool-list case.

## Tasks

- [ ] `crates/officina-core/src/governor/view.rs`; TypeScript bindings (AC 1)
- [ ] Host read half; `officina governor --json`; `governor_cli.rs` (AC 2)
- [ ] `get_governor`, link and unlink writers and commands, `LinkRefusal` (AC 3)
- [ ] `examples/governor_fixture.rs`; fixture script and index (AC 4)
- [ ] Svelte components, store, `governor-text.ts`; Vitest and Playwright cases (AC 5 to 7)
- [ ] Briefing sections; MCP case (AC 8)
- [ ] Zero write; `CHANGELOG.md`, README (AC 9, 10)

## Dev Notes

- Reworked 2026-11-02 (D-1209, D-1210, D-1211): the one-line header of story 4.20 shows exceptions only, so Slice A adds no chip; the Governor section carries D-564's list; the briefing reads the journal since its cursor. Files as built: `app/src/components/OrchestratorView.svelte` (the section's host, beside v0.6's Workshop section from story 6.20), `app/src/lib/ipc.ts`, `app/src/e2e-mock.ts`, `app/e2e/fixtures.ts` and `fixture-index.ts` (D-181's index), `crates/officina-core/src/orders/briefing.rs`, `crates/officina-host/src/mcp.rs`.
- Requirements: FR-17 as amended (amendment 11), FR-18 Slice A (the CLI the doctor shares), J4. Decisions: AD-5, AD-13, AD-55, AD-83, AD-93, AD-127, AD-144, AD-145. Epic decisions: D-9, D-25, D-28, D-34 (Epic 2); D-47, D-72, D-80, D-92 (Epic 3); D-181 (Epic 4); D-149, D-163 (Epic 5); P-63, P-65, D-300, D-305, D-330, D-340, D-348.
- Seams (D-300): the Orchestrator view (5.8, 6.20), `Header.svelte` (2.9, 3.5, 4.4, 5.18, 6.20), AD-55's subject picker (3.5), the briefing (5.8) and `officina mcp` (5.15), `officina/main.rs` usage and `readme.rs` (2.10), D-181's index (4.1), `build.rs`, the capabilities, `ipc.ts` and `e2e-mock.ts` (2.7).
- Waits for: stories 7.5, 7.6 and 7.11.
- The header gains nothing in Slice A (D-1209). `Board` does not change in Slice A, so `BOARD_SCHEMA` (5 on `main`) moves once, in story 7.14 (P-63).
- `officina governor --json` and the view read the same ledger type (D-348): the CLI seeds its own at each invocation.
- v0.3's routine marker is set at archive (P-10, D-62) and filing into an archived subject is refused (D-54): links accept archived routine subjects, and story 7.14 files a launch into a linked subject only when it is not archived.
- The view carries no run text: a held or quarantined package is listed by run id, template and cause, and its texts stay in v0.6's review panel, where they are `Untrusted` (AD-74).
- Slice A shows no on, no Stop autonomy and no per-order switch; story 7.16 adds them.
- No employer content in fixtures: placeholder orders, templates, projects and subjects.

## Testing

- Core unit: `crates/officina-core/src/{governor/view,orders/briefing}.rs`, `crates/officina-core/tests/typescript_bindings.rs`.
- Host unit: `crates/officina-host/src/governor/writers.rs`.
- CLI integration: `crates/officina/tests/{governor_cli,mcp,readme,quota_doctor}.rs`. App crate: `app/src-tauri/src/main.rs`.
- Frontend: Vitest in `app/src/lib/` (`governor-text.test.ts`) and `app/src/components/governor/`, Playwright `app/e2e/governor.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: archived routine subjects are linkable and `LinkRefusal` gains `merged` (F13); components and their tests under `app/src/components/governor/` (F30); the CLI reads the ledger (F14)
- 2026-11-02 reworked against v0.4 to v0.6 as built: no header chip in Slice A (D-1209, AC 7); D-564's UI list (D-1210, AC 11); the briefing folded from the journal and `officina mcp` as built (D-1211, AC 8, AC 12)
