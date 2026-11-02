# Story 7.22: Critic and research helpers, their reports and the read tool

**Status:** Approved

## Story

As the author who asked for an orchestrator that can call a critic and a researcher on a question,
I want a helper to be a workshop run of a template I wrote, read-only on the project, whose report I keep or discard and which the orchestrator can read but never act on,
so that helpers are launched by my click or by the governor's deterministic rule, never by a model, and a helper template earns autonomy by the same evidence as any other (PRD section 9, D-492, D-1219).

## Acceptance Criteria

Implemented only once story 7.13's Slice B decision entry records **go** (D-331), after stories 7.14 and 7.15 (a role order's launch, the scope check and the hold are theirs). Helpers belong to Slice B (PRD section 9 item 4); a clicked helper run needs this story too, since its report and Keep are built here.

1. **The role at launch** (AD-183, D-1219). A card or a standing order of a template with `role` (validated from story 7.2) launches through v0.6's and v0.7's paths unchanged: a click (v0.5 AD-78) or a grant (story 7.14). The run's plan carries an empty scope (`RunPlan.scope` present and empty, D-333), and `workshop_run.role` is set (story 7.4). Proof: core `crates/officina-core/src/workshop/launch.rs::tests::a_role_run_carries_an_empty_scope`; CLI `crates/officina/tests/governor.rs::a_role_order_launches_with_its_role_and_an_empty_scope`.
2. **Read-only by scope** (D-1219, D-307, D-333). A role run with any commit past the base is quarantined `out_of_scope` at collection (story 7.15's check, both sides of every path against an empty list), settles refused (D-313) and, for an autonomous run, suspends its order (D-314). Proof: CLI `governor.rs::a_role_run_that_commits_is_quarantined_out_of_scope` (the fake engine's worker commits once).
3. **The report** (D-1219, AD-183). A role run that exits 0 with no commit is collected with `WorkshopOutcome::Report` and state `review`; its final text is kept as v0.6 keeps a package's texts (`Untrusted<String>`, v0.6's cap), never parsed; a role run that ends otherwise follows v0.6's outcomes (`failed`, `not_collected`, `unknown`). Proof: core `crates/officina-core/src/workshop/run.rs::tests::a_role_run_with_no_commit_is_a_report`; a `classification.json` case for a report card under its coverage test.
4. **Needs you and the verdict** (D-1219, D-28). A report in `review` enters Needs you as "report: <run name>" (counted in `waiting_count`, held at the ceiling like an autonomous package when the run is autonomous, story 7.15); the review panel shows its text as text and offers **Keep** (pointer only) and Discard with v0.6's reasons (keyboard-reachable, since it only reduces). Keep sets state and verdict `kept` with `decided_at_ms`, appended before the row is written (D-1207); `kept` settles accepted, Discard as for a package (story 7.6). Proof: host unit `crates/officina-host/src/registry/workshop.rs::tests::{keep_sets_kept_once, keep_on_a_package_is_refused}`; Vitest `keep ignores a keyboard click` in the review panel's test file of story 6.19 (named as that story builds it, D-1202); Playwright `app/e2e/governor.spec.ts > a report is kept with a click and read as text ({light,dark})`.
5. **The read tool** (AD-183, D-1211). `officina mcp` gains `McpTool::HelperReports` (`helper_reports {limit <= 10}`): reports of role runs in state `review` or `kept`, newest first, with template, project, role and run id, the text wrapped as untrusted (`untrusted::wrapped`, AD-90); it reads the registry read-only, writes nothing and spawns nothing; no tool sends a report anywhere. Its schema joins `tests/vectors/mcp.json`. Proof: core `crates/officina-core/tests/mcp_vectors.rs` (the tool's case, iterating `McpTool::ALL`); CLI `crates/officina/tests/mcp.rs::{helper_reports_wraps_each_text_as_untrusted, helper_reports_caps_its_limit_at_10, helper_reports_lists_no_discarded_report}`; the orchestrator's tool-list case still derived from `McpTool::ALL` (D-1022).
6. **Injection** (D-1212, AD-74, R112). `injection.json` gains a `helper` section: a report text asking the orchestrator to launch, approve or send something, read through `helper_reports` and the review panel: zero card, zero launch, zero proposal sent without a click, the text rendered inert. Proof: `crates/officina/tests/injection.rs` over the section.
7. **Due and eligibility** (D-1219, FR-3). A role order is eligible by FR-3's rule over its template's settled runs (Keep accepted, Discard refused or not counted); it is due under its cadence, and its base test reads its last settled run whatever its verdict. Proof: `eligibility` cases in `tests/vectors/governor.json` for a role order (from story 7.6's rule).
8. **Counted like any autonomous run** (AD-183, R111). An autonomous role run counts in the ceiling, the allowances, the weekly cap and the review budget like any autonomous run. Proof: CLI frozen-clock `governor.rs::a_live_role_run_counts_in_the_weekly_cap_and_the_ceiling`.
9. **Documentation** (D-340). `CHANGELOG.md` `[Unreleased]` under Added: helper templates (`role`), reports kept or discarded, `helper_reports`. README: what a helper is and is not (read-only, launched by a click or a standing order, never by a model), Keep, the read tool, that a report is untrusted text. Proof: review; `crates/officina/tests/readme.rs` names every `McpTool` (iterating the constant).

## Tasks

- [ ] Role runs at launch and collection: empty scope, `Report`, quarantine (AC 1 to 3)
- [ ] Needs you, Keep in the registry and the review panel (AC 4)
- [ ] `McpTool::HelperReports`, its vector, its tests (AC 5)
- [ ] `injection.json` `helper` section (AC 6)
- [ ] Eligibility and counting cases (AC 7, 8)
- [ ] `CHANGELOG.md`, README (AC 9)

## Dev Notes

- Requirements: PRD section 9 items 1, 2 and 4; FR-2, FR-3, FR-8 as they apply to role runs. Decisions: AD-74, AD-90, AD-183. Risks: R111, R112, R132. Epic decisions: D-28 (Epic 2); D-575, D-690 to D-693, D-964, D-965 (Epic 5); D-1115 (Epic 6); P-63, D-307, D-313, D-314, D-333, D-334, D-492, D-1207, D-1211, D-1212, D-1219.
- Seams (D-1202): `crates/officina-core/src/mcp.rs` (`McpTool`), `crates/officina-host/src/mcp.rs`, `tests/vectors/mcp.json`, `crates/officina-core/src/workshop/{run,review,launch}.rs`, the review panel and the workshop card of stories 6.19 and 6.20, `tests/vectors/injection.json`.
- Waits for: story 7.13's go, stories 7.14 and 7.15; the key (7.2) and the columns (7.4) are already in place.
- `officina mcp` runs in `officina.exe`, inherits the orchestrator's session variables and reads only `CLAUDE_PROJECT_DIR` and the session id (D-693): the new tool reads the registry and nothing else.
- "Spawning" a helper is the orchestrator's proposal and the author's click, or the author's standing order and the governor's rule (D-492): the orchestrator gains no tool that launches.
- A report the author never judges stays unsettled and earns nothing (D-1219's "gives up").
- No employer content in fixtures: placeholder templates, projects and report texts.

## Testing

- Core unit and integration: `crates/officina-core/src/workshop/{run,launch}.rs`, `crates/officina-core/tests/{mcp_vectors,governor_vectors}.rs`.
- Host unit: `crates/officina-host/src/registry/workshop.rs`.
- CLI integration: `crates/officina/tests/{governor,mcp,injection,readme}.rs`.
- Vitest: the review panel's test; Playwright: `app/e2e/governor.spec.ts`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-11-02 created from D-492 and D-1219 (the helpers' stories, written with the 2026-11-02 rework so that their key and columns land with the one format bump and the one migration)
