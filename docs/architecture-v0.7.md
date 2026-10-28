---
title: "Architecture addendum: Officina v0.7 - The Governor"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions; windows-sys features Win32_System_Time and Win32_UI_Input_KeyboardAndMouse accepted; gates open); addendum of section 6 (2026-10-28: the critic and research helpers)
inputs:
  - prd/prd-v0.7.md (revised after the requirement challenge)
  - (requirement challenge, not published)
  - prd/prd-v0.6.md
  - architecture-v0.6.md
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - architecture-v0.4.md
  - architecture-v0.5.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - stories/epic-3-decisions.md
  - stories/epic-4-decisions.md
  - stories/epic-5-decisions.md
  - (the code: crates/officina-core/src/quota.rs, board.rs, relay/journal.rs; crates/officina-host/src/collector.rs, watch.rs; v0.2 in progress)
  - https://code.claude.com/docs/en/agent-sdk/typescript
  - https://code.claude.com/docs/en/costs
  - https://code.claude.com/docs/en/headless
  - https://code.claude.com/docs/en/statusline
  - https://code.claude.com/docs/en/commands
  - (Claude Code 2.1.312 on the workstation, one usage probe, 2026-10-28)
---

# Architecture addendum: Officina v0.7

AD-1 to AD-126 stay in force except where section 3 amends them; this addendum adds AD-127 onward and risks from R66. **Verified** marks a fact observed on the workstation for this addendum (section 1). **Documented** marks a page read on 2026-10-28. **Gate** marks a fact still to check, with the procedure of AD-149, before the dependent story closes. The v0.6 addendum is a draft: where this addendum names a v0.6 AD, it builds on that draft and amends it in section 3.

One property organises everything below: **an autonomous launch exists only as a value the pure decision returns in mode `on`, and that value is spent in the same collector step that builds and consumes its card.** The decision reads numbers, counts, the author's verdicts and the approved catalogue; no `Untrusted<T>` is among its inputs, and no text a session, a model or a worker wrote can reach it. In shadow the decision has no launch to return at all, so Slice A launches nothing by construction, not by care.

## 1. Checked on the workstation

Windows 11 Pro 10.0.26200, 2026-10-28. Claude Code 2.1.312 (`claude.exe` in the user's local bin). Every `claude` run started from a fresh empty folder in the session's scratch directory, outside the repository, with the parent session's thirteen `CLAUDE*` variable names removed from the child (names listed, values never read). Nothing touched a Claude settings file; one print-mode call ran; no model turn, no USD spent; nothing was left running.

**Verified.**

1. **The binary's help.** `--bare` exists and its help says Anthropic auth under it "is strictly ANTHROPIC_API_KEY or apiKeyHelper": a bare run reads no OAuth login. `--no-session-persistence`, `--max-budget-usd`, `--permission-prompts`, `--setting-sources`, `--settings`, `--tools`, `--strict-mcp-config` and `--input-format` exist with the meanings v0.5 and v0.6 rely on.
2. **`/usage` runs in print mode (gate 1, first half).** `claude -p /usage --output-format stream-json --verbose --tools "" --strict-mcp-config --mcp-config {"mcpServers":{}} --setting-sources "" --settings {"crossSessionInbound":"refuse"} --no-session-persistence --model haiku`, stdin null, from an empty folder with the default config dir: exit 0 in 1.8 s, 9.3 KiB on stdout, nothing on stderr, the folder still empty afterwards. Three lines:
   - `system/init`: no tool, no MCP server, permission mode `default`, model the Haiku alias's model;
   - one `assistant` line: `message.model` is `<synthetic>`, every token count is 0, one text block, a top-level `local_command_run` naming `usage`, and `usage_report`;
   - `result`, subtype `success`: `num_turns` 0, `total_cost_usd` 0, `duration_api_ms` 0, `modelUsage` empty, `local_command` `usage`.

   No `rate_limit_event`, `api_retry` or hook line. `--model haiku` was added to bound the cost had `/usage` reached a model; it did not.
3. **The report's shape.** `usage_report.rate_limits.limits` held three rows, `kind` `session` (group `session`), `weekly_all` (group `weekly`) and `weekly_scoped` (group `weekly`, `scope.model` with a display name, `scope.surface` null). Every row carried `percent` as a whole number, `resets_at` as ISO 8601 with a numeric UTC offset, with microseconds on two rows and without fractional seconds on the third, `severity` and `is_active`. `rate_limits.extra_usage` and the `session` block were present.
4. **The report's text.** 22 lines: the plan bars as sentences, then an attribution of this machine's recent local usage naming skills, subagents and files. None of the degraded phrases the costs page documents appeared. Officina must never store this text (AD-134).
5. **Which credential answered.** `system/init` reported no API key source: the call used the floor's own interactive login.
6. **The time zone (gate 6, first half).** Windows' time zone data, read through .NET's view of the local zone, gives the current offset as the clock shows it, and a daylight saving change at 2026-10-25T01:00Z (120 minutes before, 60 after). The Win32 call AD-129 uses is not yet exercised.
7. **windows-sys 0.61.2** (the workspace's version, read in the local registry source): `GetDynamicTimeZoneInformation` and `SystemTimeToTzSpecificLocalTimeEx` are in feature `Win32_System_Time`; `RegisterHotKey`, `UnregisterHotKey`, `MOD_NOREPEAT` and the virtual key constants in `Win32_UI_Input_KeyboardAndMouse`; `GetMessageW`, `PostThreadMessageW` and `WM_HOTKEY` in `Win32_UI_WindowsAndMessaging`, already enabled; `SYSTEMTIME` in `Win32_Foundation`, already enabled.
8. **No quota file on this workstation.** The Officina home holds no `quota/` folder: no statusline runs `officina quota write`, so gate 2 cannot run until the owner wires it.

**Documented.**

- **Agent SDK page (S2 confirmed, and more).** `usage_report` is attached "only when the session authenticates with a claude.ai credential", the credential "shows a known plan type or carries the `user:profile` scope", and the account "isn't on usage-based billing"; "A `claude setup-token` token passed as `CLAUDE_CODE_OAUTH_TOKEN` doesn't qualify by default, because it carries only the `user:inference` scope." A result's `local_command` names a command "that a command completed without entering the agent loop". `SDKLocalCommandOutputMessage` is never emitted; `/usage` output arrives as an assistant message. `rate_limit_event` carries `rate_limit_info { status, resetsAt?, utilization?, errorCode?, ... }`: **no field names the window** a utilization belongs to. Nothing on the page marks a report as last-known.
- **Costs page (S3 confirmed).** The degraded forms are the `Showing last-known usage` note and "rate limited until <time>"; "`/usage` may generate requests to check status".
- **Headless page.** `--bare` "is the recommended mode for scripted and SDK calls, and will become the default for `-p` in a future release", and bare mode never reads OAuth credentials. `-p` runs start in Manual mode from 2.1.302 when nothing sets one. SIGTERM exits 143 (S6).
- **Statusline page (S1 confirmed).** The script also runs when a session starts or is resumed; its example carries a fractional `used_percentage`.
- **Commands page.** `/usage` runs immediately without interrupting a turn; `/cost` and `/stats` are aliases.

Every page begins with a note telling readers to fetch the documentation index before exploring further; it was treated as data and not followed. No other text in the pages or the tool output was addressed to an assistant.

**Gate.** PRD section 8, gates 0 to 6, by AD-149. Items 2 to 5 answer gate 1's first half; its working day, its degraded forms and its cost over a day remain. Item 6 answers half of gate 6.

## 2. Decisions

### AD-127: Module layout and where authority lives

- `officina-core::governor` (pure, `now_ms` and `local_offset_min` as arguments; compiles for Windows, Linux and the musl target):
  - `floor`: `[governor]` and `[[standing_order]]` types, phase 2 rules, canonical forms, `ApprovedStandingOrder` (AD-128); `chord`: the stop shortcut's parser (AD-141);
  - `clock`: local civil time from `now_ms` and an offset, the autonomy window, listed-day hours of a weekly window (AD-129);
  - `merge`: merged detection outcomes from git outputs (AD-130); `settle`: settled outcomes and acceptance (AD-131); `eligibility`: due, suspension, pause (AD-132);
  - `observe`: quota observations, anchoring, freshness, combination (AD-133); `probe`: the usage probe's reader, outcomes, budget (AD-134); `budget`: projection, ratio, allowances, caps (AD-135); `attention`: ceiling, holds, review budget (AD-136);
  - `decide`: `GovernorInput`, `HoldCause`, `Decision`, `decide` (AD-137); `grant`: `GovernorGrant`, behind the core feature `governor-launch` (AD-139);
  - `evidence`: hold shares, ceiling room, would-launch history from journal lines; `automations` (AD-146); `view`: `GovernorView` and `GovernorSummary` (AD-144).
- `officina-host::governor`:
  - read half, always compiled: building `GovernorView` from disk (approved snapshot, registry read-only, journal, quota file, event buffer, the `Agents` read half), for `officina governor --json` and doctor;
  - authority half under `window-authority` (AD-21), its error type carrying `AUTHORITY_MARKER` (D-9): the governor step on the collector thread, the writers, the probe worker, the git worker, the launch from a grant, the hotkey thread. Built without the feature, none of it exists.
- The feature `window-authority` of `officina-host` turns on `officina-core/governor-launch`; `officina` built alone (`-p officina`, P-5) compiles core without `GovernorGrant` and without `Decision::Launch`. A CI step fails when `cargo tree -p officina -e features` lists that core feature, beside D-9's marker step.
- Threads (AD-3 amended): `governor-probe` runs one usage probe at a time; `governor-git` runs merged detection and base-tip reads one job at a time. The collector runs neither; outcomes return as `AuthorRequest`s without deadline (D-135's outcome pattern).
- `officina`: subcommand `governor --json` (AD-144).
- Lint: the `Command::new` sites of `officina-host` stay four; the probe goes through `spawn` (AD-134), the git jobs through `git`.

Gives up: a core feature flag and two threads. In exchange "the shipped CLI cannot hold a grant" is a property of its build graph, checked in CI, and the collector never waits on a probe or a repository.

### AD-128: Floor keys, phase 2, canonical form

- **Phase 1:** the floor gains a raw `governor: Option<toml::Spanned<toml::Value>>` and a raw `standing_order: Vec<toml::Spanned<toml::Value>>`; `RELAY_KEYS` gains both (D-7's test). Nothing in them can invalidate the floor the board uses.
- **Phase 2**, each rule an `IssueKind` variant with a failing fixture under `every_issue_kind_has_a_case` (D-132): PRD FR-10's rules, read as follows:
  - `window`: `HH:MM-HH:MM`, both in 00:00..=23:59, different; stored as two minute-of-day numbers;
  - `days`: day names `mon` to `sun`, case-sensitive, one to seven, distinct;
  - `usd_per_week`: at most two decimals, stored as cents (AD-101's rule for `max_usd`); required with `[workshop] auth = "api-key"`, refused otherwise;
  - `stop_shortcut`: AD-141's grammar;
  - a standing order's `values` pass `template::check_values(&Template, &ProjectName, &values)`, the function `WorkOrder::validate` calls (AD-77): one function, two callers, and a vector case shows both refuse the same input;
  - `paths`: at most 32, AD-113's glob syntax, no `.` or `..` segment, compiled at validation;
  - on an api-key floor, a standing order whose template sets no `max_usd` is the issue `MaxUsdRequired` (PRD FR-7).
- **Canonical form:** `CANONICAL_FORMAT` goes up by one relative to `main` at that merge (D-131's rule), once for v0.7. `CanonicalGovernor` holds every key with its default explicit, `window` as two minute numbers, `days` in week order, `usd_per_week` in cents, the chord in AD-141's normal form; `standing_orders: BTreeMap<String, CanonicalStandingOrder>` holds template, project, values sorted by parameter, base, `every_hours`, paths sorted and deduplicated. New fields take `#[serde(default)]`; a test loads an `approved.json` in the previous format and asserts AD-76's four facts again. `catalogue::diff` gains a governor section and a standing orders section; the approval banner shows them.
- **`ApprovedStandingOrder::new(approved: &ApprovedCatalogue, current: &RelayFloor, name)`** is the only constructor, AD-77's pattern: it requires `current.hash == approved.hash` and reads the order and its template from the approved snapshot. The governor reads `[governor]` and the standing orders from the approved snapshot only; a pending change holds everything (`catalogue_changed`, AD-137).
- A standing order's state (AD-132) is keyed on its name, so editing an order does not lift a suspension: only the author's click does.

### AD-129: Local time, the autonomy window, the week's listed hours

- **The offset** is read by `platform::windows::local_offset_min(now_ms) -> Option<i32>`: core converts `now_ms` to a UTC civil time (`dates.rs`), the host calls `GetDynamicTimeZoneInformation`, then `SystemTimeToTzSpecificLocalTimeEx` on that instant, and core returns local minus UTC in minutes. It is read at every governor step, so a daylight saving change moves the window as the clock does. `None` (the call failed, or `platform::other`) holds the governor (`local_time_unknown`, AD-137): missing data is unknown (principle 3). The same function converts any instant, so gate 6 checks a past transition directly (AD-149).
- **The window** (core `clock::window_at(cfg, now_ms, offset) -> WindowState`): a range belongs to the local day it starts; `22:00-06:00` that starts on a listed day ends at 06:00 the next day. `WindowState` gives inside or outside, the current range's start and end as instants, and the next start. `window_ending` holds under 20 minutes to the end (`WINDOW_ENDING_MS`).
- **Listed-day hours of a weekly window** (PRD FR-6's `f`): the window runs from its `resets_at` minus 7 days to its `resets_at`; core cuts that span at local midnights with the current offset and sums the milliseconds falling on listed days, elapsed and total. `f = max(elapsed, one listed day) / total`.
- **Gives up:** one offset for a whole computation. A daylight saving change inside a weekly window shifts a day boundary by an hour (an error of at most one hour in 168), and a change inside a run moves the window's end relative to the deadline computed at launch; AD-140's window-end stop reads the offset at each step and catches the second (R75).

### AD-130: Merged detection

- **Rows read:** every v0.6 run imported (state `imported`, AD-114) with `commits_past_base` above 0. A `no_change` run is never imported (AD-110).
- **Facts** (host, `governor-git` thread, `GitRule::Workshop` of AD-111 with D-22's flags and `GIT_OPTIONAL_LOCKS=0`, read-only, never `fetch`): the current tip of `refs/heads/officina/<name>` (`rev-parse --verify --quiet ...^{commit}`, or gone); the remotes (`git remote`), then for each, `refs/remotes/<remote>/<B>` when it exists; `merge-base --is-ancestor` of T and of the current tip against `refs/heads/<B>` and each remote-tracking B; `git cherry <B ref> <T> <C>` against each B ref, one line per commit of `C..T`, `-` meaning a patch-equivalent commit exists there.
- **Rules** (core `merge::classify(&MergeFacts, imported_at_ms, now_ms) -> MergeOutcome`, first match wins, a vector case per row): `merged` (T an ancestor of a B ref); `merged_changed` (the branch's tip is past T, T its ancestor, and that tip an ancestor of a B ref); `merged_equivalent` (every `cherry` line against one B ref is `-`); `not_merged` (the branch is gone, or `MERGE_READ_DAYS`, 14 days, passed since the import); else `pending`.
- **When:** hourly, and when `get_governor` is called if the last pass is older than 5 minutes. Only `pending` rows are read again: an outcome other than `pending`, or any author correction, is final. Journal `merge_settled` once per final outcome.
- **Corrections** (D-28, pointer only): `correct_merge(run_id, merged | not_merged)` on a `pending` or `not_merged` row of source `rules`; source `author` beats `rules` (v0.3's provenance) and is final.
- **No program runs:** `cherry` computes patch ids through git's diff machinery. One host test per configuration D-154 neutralises (external diff, textconv, fsmonitor, a filter driver) points it at a marker program that must not run during a merge pass (R42's pattern).

### AD-131: Settled outcomes and acceptance

- **`settle::settle(&RunEvidence, gate_on_merges: bool) -> Settlement`** (core), a closed enum `Accepted`, `Refused(RefusalKind)`, `NotCounted(NotCountedReason)` or `Unsettled`, from the registry's `workshop_run` and `work_order` rows and the `merge_check` row. One vector case per row of PRD FR-2's table:
  - Import: `Accepted` at the verdict; with `gate_on_merges` on, `Unsettled` until a merge outcome, then `Accepted` (`merged*`) or `Refused(not_merged)`;
  - Discard `poor_result`, `should_not_have_run`, a quarantine (any cause, `out_of_scope` included), and `not_needed` of an autonomous run: `Refused` with that kind;
  - Discard `not_needed` of a clicked run, `expired` (AD-136), `no_change`, a run ended without a package (`failed`, `unknown`): `NotCounted`.
- **The settlement time** is the verdict's time, or the merge outcome's with the gate on. A template's acceptance is accepted over settled among its last `acceptance_runs` settled runs, by settlement time, clicked and autonomous alike, across projects (PRD FR-2's "gives up").
- **`no_change`** is v0.6's `WorkshopOutcome::NoChange` (AD-110), decided by host git on host objects. v0.7 adds no reader of its own and no journal kind: `workshop_finished` with that outcome is the record (section 4, amendment 2).
- **Gate on merges:** `set_gate_on_merges(true)` is refused by the writer unless, when it writes, at least 20 imports have a final merge outcome and author corrections are under 30 % of them (D-69's pattern); off is always allowed. Stored in `setting.gate_on_merges` with its source.
- Settlements are recomputed, never stored: a corrected merge or a switched gate moves the figures at the next view.

### AD-132: Eligibility, due, suspension, pause

- **Core** `eligibility::assess(&OrderEvidence, &TemplateAcceptance, &GovernorConfig, now_ms) -> OrderAssessment`: eligible or the reason (`too_few_settled`, `below_threshold`, `suspended`, `paused`, `off`), then due or the reason (`cadence`, `live_run`, `package_awaiting`, `base_unchanged_since_no_change`, `base_unchanged_since_not_needed`), with the effective cadence and the next due instant.
- **Effective cadence:** `every_hours`, doubled while the order's last three autonomous runs ended `no_change`; the first other outcome restores it.
- **Base tips** of every standing order's base, read on `governor-git` every 5 minutes (`rev-parse --verify refs/heads/<base>^{commit}`) and again at launch (AD-139). The base compared is each prior run's `base_commit` (`workshop_finished`). A tip read 5 minutes ago can make an order due late, never launch it on a moved base: AD-107's `base_moved` stands.
- **State** (`standing_order_state`, AD-142): absent means on. Transitions, each journaled `automation_changed` with source, cause and numbers (D-69):
  - `suspended` (source `rules`): a newly settled run takes the template's acceptance from at or above the threshold to below it (every order of that template); an autonomous package of the order discarded `should_not_have_run` or quarantined (that order);
  - `paused` (source `rules`): the order's last three autonomous runs ended with a cause of v0.6's `ENVIRONMENT_FAILURE_CAUSES` (AD-109);
  - `off` and `on` (source `author`): `set_standing_order(name, on)` is refused by the writer unless, when it writes, the order is eligible but for its own state and, after a suspension, a clicked run of the template settled after the suspension's time (D-69's pattern); off is always allowed.
- In Slice A, eligibility is computed and shown; the state machine exists, since suspension and pause read clicked evidence too, but no order is turned on or off by the author until Slice B.
- **Shadow cadence:** in shadow, an order's last launch is the latest of its real autonomous launches and its shadow bookings (AD-138); in mode on, real launches only.

### AD-133: Quota observations

- **`observe::Observation`** (core): source (`file`, `probe`), `anchored_ms`, `written_at_ms` for a file, and per window an optional `WindowReading { used_milli, resets_at_ms }` for the 5-hour and weekly windows, plus the probe's scoped weekly readings (`used_milli`, `resets_at_ms`, scope kind, a display label cut to 32 printable characters). Percentages are integers in thousandths of a point (AD-135).
- **Anchoring a file** (`observe::anchor_file(&QuotaFile, &[EventLine], now_ms)`): among the event buffer's lines (AD-8, the collector already reads two UTC days), those with `session_id` equal to the file's, event `UserPromptSubmit` or `Stop`, and `ts` at most `written_at` times 1000 plus 999 (the file holds seconds); the latest gives `anchored_ms`. None within `FRESH_MS` (30 minutes) before `written_at`: no observation. A file without `session_id` never anchors. The quota file contract does not change; AD-10's skip rule can only make an anchor older, which errs toward caution.
- **Fresh:** `anchored_ms` within `FRESH_MS` of `now_ms`, the weekly window present, and no window of the observation with `resets_at_ms` at or before `now_ms`.
- **Combination** (`observe::combine(&[Observation], now_ms) -> Combined`), over fresh observations, per window: the highest `used_milli`. Reset times of one window that differ between fresh observations by more than `RESET_TOLERANCE_MS` (15 minutes, a starting value gate 2 measures), or a 5-hour window active in one fresh observation and absent from another, give `quota_conflict` (AD-137): two sources disagreeing on which window they describe is a mapping fault, not a number to pick from. Within tolerance, the earliest 5-hour reset bounds the deadline and the perishable test, and the latest weekly reset sets the projection's elapsed share; each choice errs toward caution.
- **Journal** `quota_observed` once per distinct observation the governor uses (source, `anchored_ms`, `written_at_ms`, values; never text).
- **A forged file** with low numbers cannot lower a reading beside a probe (the maximum wins). Without the probe, it can (R73).

### AD-134: The usage probe

- **Vector kind `UsageProbe`** (AD-79's table gains a row; golden case in `tests/vectors/spawn.json`):

  | Arguments after the binary | Working directory | Console | stdin | Capture, deadline | Shim |
  |---|---|---|---|---|---|
  | `-p /usage --output-format stream-json --verbose --model haiku --tools "" --strict-mcp-config --mcp-config {"mcpServers":{}} --setting-sources "" --settings {"crossSessionInbound":"refuse"} --no-session-persistence` | `<home>/usage/empty`, refused unless empty | none, kill-on-close job | null | stdout 256 KiB, 30 s | refused |

  - `/usage` is a constant of core, the only prompt the kind can carry. `--model haiku` changes nothing for a local command (section 1, item 2) and bounds the cost of the failure in which a release sends `/usage` to a model.
  - **Environment** (D-128, a contained kind): `SESSION_ENV_REMOVED`, every `OFFICINA_*` name and `CLAUDE_CODE_DISABLE_AGENT_VIEW` removed, plus `CLAUDE_CODE_OAUTH_TOKEN` and `ANTHROPIC_API_KEY`: the probe answers for the floor's own login, which carries the profile scope a setup-token lacks (section 1). The floor's `CLAUDE_CONFIG_DIR` and `NoDefaultCurrentDirectoryInExePath=1` are set. `--bare` is never passed: it reads no OAuth login.
- **Reading** (core `probe::read(stdout, exit_code, timed_out) -> ProbeOutcome`), permissive line parsing. The fields read are core's `USAGE_FIELDS`, which become canary constants (AD-12: `state/contract.json` gains a `usage` surface checked on each probe). From the `result` line: `num_turns`, `local_command`, `total_cost_usd`. From the `assistant` line: `usage_report.rate_limits.limits[]` (`kind`, `percent`, `resets_at`, `scope`), and its text, for one purpose only (below).
- **Rows** are classed by `kind`, never by a label: `session` is the 5-hour window, and a `resets_at` of null means no active window; `weekly_all` the weekly window; `weekly_scoped` a scoped weekly reading; any other kind is counted and ignored. `resets_at` parses ISO 8601 with `Z` or a numeric offset and optional fractional seconds (both shapes of section 1, item 3; core's parser, vector cases for each).
- **Outcomes**, in this order:
  1. **structural failure** (`ProbeFailure`, a closed enum, a vector case each): non-zero exit, deadline, output cap, no `result` line, a model turn (`num_turns` above 0, `local_command` not `usage`, or a non-zero cost), no `usage_report`, `rate_limits` null with no degraded marker, `limits` empty or null, no `session` row, no `weekly_all` row, a duplicated `session` or `weekly_all` row, a percent outside 0 to 100, an unparsable `resets_at`;
  2. **back-off**: the report's text contains an entry of core's `USAGE_DEGRADED_MARKERS`, compared case-insensitively. The constant is seeded once from S3's two documented phrases, and gate 1 adds what it observes; from then on the constant is the list. No observation is taken and no probe runs for `BACKOFF_MS` (60 minutes) whatever time the text names; a back-off never counts toward the pause;
  3. **observation**, anchored at the probe's end.

  The text is matched in memory and dropped: it is never parsed for a number, never stored, never journaled, never shown (it carries the author's usage attribution, section 1, item 4). A marker can only turn numbers into a back-off, so a wrong match errs toward caution.
- **Pause:** three structural failures in a row with the same `claude` stamp (path, size, modification time; D-66) pause the probe until the stamp changes (AD-50's rule). The governor then falls back to file observations and `one_per_window` (AD-137).
- **Budget** (`probe::budget(lines, now_ms, offset)`, counted from the journal's `usage_probe` lines, its own budget, AD-52's counting): the last probe at least 2 minutes ago, fewer than 6 in the last 3,600 s, fewer than 60 since local midnight. Gate 1's working day confirms or lowers these numbers before Slice A ships them.
- **Who asks:** the decision (`Decision::Probe`, AD-137); in Slice B, every 10 minutes while an autonomous run is live (AD-140); and, in shadow or on, the bracket of every workshop run, clicked or autonomous: a probe at launch and one within 15 minutes after its end, for the ratio (AD-135). The collector sends one job at a time to `governor-probe`; a job asked while one runs waits for it.
- **Journal** `usage_probe` per spawn: outcome, failure or back-off cause, why it ran (`decision`, `live_guard`, `bracket`), duration, exit code, `total_cost_usd`, `num_turns`, the stamp's fingerprint, the values when an observation resulted. Never text.
- **Fake claude** (D-129) gains mode `usage` with variants (clean, degraded, null `rate_limits`, empty `limits`, missing row, model turn, exit 1, hang, oversized), each printing lines built from section 1's shape with every value synthetic.

### AD-135: Projection, reserve, ratio, allowances, caps

- **Arithmetic** in integers: points in thousandths, money in micro-USD; every division rounds toward caution (projections and allowances up). Vector cases sit at the exact boundaries (100 passes, 100.001 holds).
- **Projection** `used / f` with AD-129's `f`, counting every use of the account (PRD FR-6).
- **Ratio** (points per USD, one per window): a **sample** is a workshop run, clicked or autonomous, bracketed by a probe observation anchored at most 2 minutes before its launch and one anchored within 15 minutes after its end. It is clean for a window when no other card of the floor was in Working during the run (the collector keeps that flag per live run from each board) and the window rose by at least 2 points (whole points, section 1, item 3). Journal `ratio_sample` per bracket, clean or not, with its cause. **The ratio in use** for a window is the largest of the last 5 clean samples of the last 90 days: a single noisy bracket cannot lower it, and invisible use from chat or other devices makes it err high (PRD FR-6).
- **A run's points:** its `result` cost estimate (v0.6 AD-109) times the ratio, per window. **A template's allowance:** the largest of its last 5 runs' points; before any run, its `max_usd` times the ratio; without a ratio or without `max_usd`, the starting values `START_ALLOWANCE` (3 weekly, 10 five-hour points). Each allowance carries its source (`measured`, `max_usd`, `starting`) to the view and the journal.
- **This week's autonomous points:** over autonomous runs launched since the weekly window's start, a finished run's points, a live run's allowance; in shadow, each booking's allowance (AD-138).
- **Conditions** (AD-137, step 11): `weekly_projection`, `scoped_weekly`, `autonomous_week_cap`, `five_hour_room`, exactly as PRD FR-6 states them.
- **API-key regime** (PRD FR-7), built instead of the above only when v0.6 gate 7 recorded `api-key` (D-127's rule: one regime, never both). `usd_budget`: this local week's autonomous spend (finished runs' cost estimates), plus the `max_usd` of live ones, plus this template's `max_usd`, at most `usd_per_week`; the week starts at local midnight of the first listed day. Observations, the probe, the ratio and the five-hour room are not built on such a floor.

### AD-136: Ceiling, held packages, review budget

- **Needs you count** is `Board.waiting_count` (AD-35, v0.6 AD-121). It now leaves out held packages and autonomous quarantines, which never enter Needs you.
- **Held:** `workshop_run` gains `held_at_ms` and `released_at_ms` (AD-142). A package of an autonomous run that reaches state `review` (it passed AD-111's checks) while the count is at or above the ceiling gets `held_at_ms`: journal `package_held` (run, count). The board builder (v0.6 AD-121) puts a run in Needs you only when it is in `review` and not held, or quarantined and clicked. A state column with a new `RunState` variant was set aside: it would widen a `CHECK` and rebuild `workshop_run`.
- **Release:** on every governor step, while the count is below the ceiling, the oldest held packages are released, one per free seat: `released_at_ms` set, journal `package_released` (run, count at that moment). The release depends on the ceiling only (PRD FR-9).
- **Expiry:** a package held for `HELD_EXPIRY_MS` (7 days) is discarded with v0.6's `DiscardReason::Expired` (AD-114 already holds it for the text bound), journal `package_expired` (run, `held_at_ms`), and self-stops the governor (`held_expired`, AD-140). Its containers were already removed at collection (AD-110).
- **Quarantined autonomous packages and autonomous runs ended without a package** are listed in the Governor section with their cause and, for a quarantine, Discard; a `no_change` run is listed, never held.
- **`ceiling_crossed`** (direction, count, ceiling) is journaled in shadow and on each time the count crosses the ceiling while local time is inside the window, plus one line at each window start giving the current side, so the share of window minutes under the ceiling is computable from lines alone.
- **Review budget** (core `attention::review(...)`), from the journal's `review_opened` and verdict lines (v0.6 AD-120): spent today (autonomous packages decided since local midnight, Receive to verdict, each counted at most 30 minutes, 1 without Receive); a template's estimate (median over its last 5 verdicts, clicked or autonomous, 10 minutes until 3 exist); pending (estimates of autonomous packages in Needs you or held, of live autonomous runs and, in shadow, of today's bookings).

### AD-137: The decision

`governor::decide(input: &GovernorInput, now_ms: i64, local_offset_min: Option<i32>) -> Decision`, pure, cheap, called on every collector recompute and on every governor step (below).

**Input** (`GovernorInput`, serializable, built by the host, every field a number, a count, a closed-enum value or author text from the approved catalogue):

| Field | Content |
|---|---|
| `mode`, `self_stop` | `off`, `shadow`, `on`; the self-stop cause if any (AD-140) |
| `flags` | relay and workshop stop flags, each `clear`, `set` or `unreadable` (AD-116) |
| `catalogue` | whether the current floor's hash equals the approved one; the approved `GovernorConfig` and standing orders, or why there are none |
| `board` | `waiting_count`, live autonomous runs, held packages, whether any other card is in Working |
| `orders[]` | per approved standing order: AD-132's evidence (last launch, live run, package awaiting, base tips, no-change streak, state), its template's acceptance, its v0.6 readiness (AD-107's side-effect-free `LaunchRefusal`, from the cached `WorkshopView`), its review estimate, its allowances, its `max_minutes` |
| `review` | spent today and pending (AD-136) |
| `quota` | the observations (AD-133), the probe state (available, paused, back-off until, budget counts, last outcome), this week's autonomous points, the last autonomous run's end, the 5-hour windows already used by an autonomous launch (fallback) |
| `bookings` | shadow bookings of the last 7 days (AD-138) |

**Conditions**, evaluated in this order; every failing one is recorded with its numbers, the first is shown (PRD FR-11, with three additions marked):

| Step | `HoldCause` | Scope |
|---|---|---|
| 1 | `governor_off`, `self_stopped` | global |
| 2 | `stopped` (a flag set or unreadable) | global |
| 3 | `catalogue_changed`; `governor_unavailable` (the approved snapshot has no valid `[governor]` or no standing order; **added**) | global |
| 4 | `local_time_unknown` (**added**), `outside_window`, `window_ending` | global |
| 5 | `autonomous_full` | global |
| 6 | `held_waiting` | global |
| 7 | `ceiling` | global |
| 8 | `none_due` | global |
| 9 | `workshop_refused` | per order |
| 10 | `review_budget` | per order |
| 11 | `quota_unknown`, `quota_stale`, `quota_conflict` (**added**), `no_post_run_observation`, `no_active_window`, `not_perishable`, `scoped_weekly`, `one_per_window` (probe unavailable only) | global |
| 11 | `five_hour_room`, `weekly_projection`, `autonomous_week_cap` | per order |
| 12 | `probe_wait`, `probe_failed` | global |

On an api-key floor, step 11 is `usd_budget` (per order) alone. The table seeds `HoldCause`, a `closed_enum!`; from then on `HoldCause::ALL`, the order of `decide`'s code and the vector set are the list. A cause whose condition is not built (a regime or a probe that its gate removed) is not a variant, so the coverage test never meets an unreachable member.

- **Per-order steps** are evaluated for every due order. The decision's causes are the failing global causes in order, followed by the failing per-order causes of the first due order in pick order (oldest last launch, never launched first, ties by name); every due order's own causes travel in `orders[]` of the view and the journal line. An order is a candidate when it passes every per-order step.
- **Step 12, the probe** (probe available): when steps 1 to 10 leave a candidate and the quota conditions either pass on fresh data or cannot be evaluated (`quota_unknown`, `quota_stale`), and no observation from a probe anchored in the last `PROBE_FRESH_MS` (2 minutes) is among those used, the decision is `Probe` if the budget and any back-off allow, else a hold with `probe_wait` beside the quota cause. A quota condition that fails on fresh data holds without probing, since a probe can only add a higher number. `probe_failed` holds while the last probe failed structurally and the pause has not begun. `WouldLaunch` and `Launch` therefore always rest on a probe anchored in the last 2 minutes. Without the probe (paused, or gate 1 failed): fresh file observations and `one_per_window`.
- **`Decision`:** `Hold(HoldReport)`; `Probe`; `WouldLaunch(LaunchRecord)` in shadow; `Launch(GovernorGrant, LaunchRecord)` in on, which exists only with `governor-launch` (AD-139). Built without it, an input in mode on gives `WouldLaunch`.
- **`LaunchRecord`:** the chosen order, its template, project, values and base; the deadline (`now_ms` plus the least of `max_minutes`, the time to the 5-hour reset and the time to the window's end) and which bound set it; the observations used with source, anchored time and values; the combined readings; projection, reserve, allowances with their source, this week's autonomous points and the cap; Needs you count and ceiling; review spent, pending, estimate and budget; the window's instants, the local time and offset; and `input`, the canonical JSON of the `GovernorInput`. Recording the input makes every journaled decision replayable (AD-148).
- **`HoldReport`:** the causes with typed numbers (`HoldNumbers`, one variant per cause), per-order causes, and `next_change_ms`: the earliest instant at which a time-based input flips a condition (window start, end and ending threshold, a cadence coming due, an observation's freshness ending, a reset, the perishable start, the probe budget or back-off clearing). The view shows it ("the 5-hour window enters its last 120 minutes at 19:40"); the collector wakes for it.
- **Governor step** (host, collector thread, under `window-authority`): while the mode is shadow or on, at every recompute, every 60 s (`GOVERNOR_TICK`, AD-4 amended) and at `next_change_ms`. It assembles the input, calls `decide`, journals what changed, sends a probe job, releases held packages, writes the self-stops and stops of AD-140, and in Slice B starts a launch (AD-139).
- **`governor_hold`** is journaled when the set of failing causes changes, at most one line per minute, and again every `HOLD_HEARTBEAT_MS` (15 minutes) while unchanged, with every cause and its numbers. A reader counts a line's causes until the next line, at most `HOLD_HEARTBEAT_MS` past it, so a closed window or a crash cannot inflate a cause's share.

Gives up: the input is assembled from several caches of different ages (base tips 5 minutes, readiness from the last `WorkshopView`). Each age errs toward holding or is checked again at launch.

### AD-138: Shadow

- In shadow, `WouldLaunch` carries no grant; Slice A's `Decision` has no `Launch` variant at all. Shadow creates no card, no row, no container (PRD section 7's Shadow inert row, by construction and by the fake engine's count).
- The governor step journals `governor_would_launch` with the `LaunchRecord`, then **books** it: the order's last launch at `now_ms`, a shadow run live until the record's deadline (so `autonomous_full` and `max_autonomous` behave as on), the template's review estimate pending until the end of the local day, and its weekly allowance in this week's autonomous points. Bookings are read back from those lines (`bookings` of the input), so they survive a restart.
- Shadow needs no gate 0. It uses probes as on would, within the same budget: gate 1's day bounds what a shadow week costs.
- Everything shadow is labelled so in the view, the briefing and the chips (AD-144, AD-145).

### AD-139: The grant and the launch (Slice B)

- **`GovernorGrant`** (core, feature `governor-launch`): private fields (standing order name, approved catalogue hash, `issued_at_ms`, the `LaunchRecord`'s deadline); no `Clone`, `Copy`, `Default`, `Serialize` or `Deserialize`; one constructor, private to `decide`, reached only from a `Launch` in mode on. `compile_fail` doctests: a struct literal, `.clone()`, `serde_json::from_str::<GovernorGrant>`, and passing one grant twice (use after move), as AD-74's do for `WorkOrder`.
- **`LaunchAuthority`** (core `orders`): `Click { card_id, prompt_sha256, plan_sha256 }` from the window (D-28, AD-107) or `Governor(GovernorGrant)`.
- **Flow:**
  1. a step's `decide` returns `Launch` for order O. The collector keeps no grant: it sends `GatherLaunchFacts(O)` to the `workshop` thread, which reads AD-107's issue facts (base tip, uncommitted count, mounts, image, inventory, credential presence) as for a card;
  2. the facts return as `AuthorRequest::GrantFacts`. In that one collector step: the stop flags are read; `decide` runs again on current inputs and must return `Launch` for O, else nothing launches and `governor_launch` records outcome `revoked` with the new causes; `ApprovedStandingOrder::new` and `WorkOrder::validate` build the order exactly as for a click (v0.5 FR-3); AD-78's issue function builds the card with AD-107's facts, the subject (the order's single linked routine subject, else `New(<standing order name>)`, which D-158 reuses), and `seq` reserved by D-135's rule; the card is consumed with `LaunchAuthority::Governor(grant)`, which runs AD-78's checks and AD-107's refusals in their order, without the prompt-hash and plan-hash comparisons that belong to a click, and checks the grant's catalogue hash and age (`GRANT_TTL_MS`, 60 s); then the `work_order` row (source `governor`, the order's name) and the `workshop_run` row are inserted, `governor_launch` and `work_order_launched` (`by: governor`) are journaled, and AD-107's steps 2 to 4 go to the `workshop` thread.
  3. The governor's card never enters the card store: no click can name it, it takes no slot of the 32, and its id never reaches the frontend.
- **The grant built in step 2 is spent in step 2.** A grant never crosses a thread, a message or a restart; the TTL is a backstop for a future change, not a working rule.
- **Deadline:** the record's deadline becomes the run's `deadline_ms` (v0.6 AD-109's three enforcements). On an api-key floor, only `max_minutes` and the window's end bound it.
- **Scope** (PRD FR-12): when the order sets `paths`, AD-111 step 6 also checks every added, changed, deleted and renamed path (both sides) against them; a path outside gives the quarantine cause `out_of_scope`. Matching uses AD-113's glob semantics but **case-sensitively**: an allow-list errs toward refusing, where AD-113's sensitive groups match case-insensitively because they err toward flagging.
- **Proposals stay proposals:** `IssueCardFromProposal` never yields a grant; an orchestrator proposal naming a standing order's exact values opens a card and waits for a click (v0.5 FR-12).

### AD-140: Live guards, window end, modes, stop autonomy, self-stop (Slice B)

- **Per-run stop** is v0.6 AD-109's (stop the worker, record the reason, collect); `FinishCause` gains `autonomy_stopped`, and journal `autonomous_stopped` gives the cause (`AutonomyStopCause`: `window_end`, `reserve_reached`, `five_hour_high`, `stop_autonomy`). Packages are kept and follow AD-136. With no commit past the base such a run is `failed` with that cause (AD-110's table), which is not in `ENVIRONMENT_FAILURE_CAUSES` and counts nowhere in acceptance.
- **Window end** (gate 4): at each governor step, an autonomous run live past the window's end, with the step's offset, is stopped (`window_end`). The collector wakes at the end instant (AD-137's `next_change_ms`), so the stop starts within a tick of it.
- **Live guards:** while an autonomous run is live, every fresh observation, from either source, is checked as it arrives, and the probe runs every 10 minutes within its budget. A weekly reading or a scoped weekly reading at or above 100 minus the reserve stops every autonomous run (`reserve_reached`) and self-stops the governor; a 5-hour reading at or above `five_hour_max_pct` plus 10 stops them (`five_hour_high`).
- **The worker's `rate_limit_event`** may only add a stop, never replace the 10-minute probe: stream lines are untrusted and any process in the container can write them (AD-109), so a forged low utilization must not silence a guard. Since the event names no window (section 1), gate 3 records which window its utilization follows before it is read at all (section 4, amendment 9).
- **Modes** (`setting.governor_mode`, source `author`; journal `governor_mode`): shadow is offered, pointer only, while the approved floor has a valid `[governor]` and a standing order; on adds PRD FR-14's conditions; the writer checks them when it writes (D-69's pattern). Off is always offered and keyboard-reachable; live autonomous runs go on to their deadline.
- **Stop autonomy** (`stop_autonomy`, keyboard-reachable, and the shortcut of AD-141): mode off and a per-run stop of every live autonomous run, cause `stop_autonomy`. Clicked runs and the relay go on. v0.6's Stop all still stops everything.
- **Self-stop** (`setting.governor_self_stop`, source `rules`, cause and numbers; journal `governor_self_stopped`; a terracotta header chip): `reserve_reached`, `held_expired`, and `credential_found` on an autonomous package (AD-112). Turning on again needs On's conditions and a click; after `credential_found`, also a credential whose `LastWritten` (read with AD-105's `CredReadW`, the value dropped at once) is later than the self-stop.
- **Window start:** the persisted mode and self-stop hold; v0.6's reconciliation runs first (AD-109); then a first governor step journals the current state, and any autonomous run past the window's end is stopped.

### AD-141: The stop shortcut (Slice B)

- **Grammar** (core `chord::parse`): tokens joined by `+`, case-insensitive; at least two distinct modifiers among `Ctrl`, `Alt`, `Shift`, `Win`, then exactly one key among `A` to `Z`, `F1` to `F24` and `Pause`. The normal form orders modifiers `Ctrl`, `Alt`, `Shift`, `Win` and spells the key in uppercase; it is the canonical value (AD-128). A vector case per rule.
- **Registration** (host `platform::windows::hotkey`, under `window-authority`): a thread named `hotkey` calls `RegisterHotKey(NULL, id, modifiers | MOD_NOREPEAT, vk)` (`MOD_NOREPEAT`: holding the chord sends one stop), then loops on `GetMessageW`. `WM_HOTKEY` sends `AuthorRequest::StopAutonomy { by: shortcut }`. The key maps to its virtual key with windows-sys's constants, never retyped numbers. A chord change or shutdown posts `WM_QUIT` with `PostThreadMessageW`; the thread unregisters and ends; a new thread registers the new chord.
- **Failure:** a chord another program holds fails to register: the Governor section and doctor say so with the chord. No plugin and no crate: two functions of an existing crate (section 3).
- **Scope:** the chord works while the window runs, minimized or behind another window (gate 5). With the window closed, `officina stop --all` remains.

### AD-142: Registry migration 4

`MIGRATIONS[3]`, written whole in the first Epic 7 story that writes the registry, its SHA-256 pinned there (D-133's rule: a hole found later is migration 5). `VACUUM INTO registry/backup/pre-v4.db` first. `CHECK` lists come from the enums' `ALL`; causes that will grow carry none (D-133).

| Table | Key | Columns |
|---|---|---|
| `work_order` (rebuilt) | `id` | every column of migrations 2 and 3; `source` `CHECK` from `WorkOrderSource::ALL`, which gains `governor`; new `standing_order TEXT` (null unless `governor`) |
| `workshop_run` (added columns) | | `held_at_ms INTEGER`, `released_at_ms INTEGER` (null) |
| `setting` (rebuilt) | | `SettingKey` gains `governor_mode`, `governor_self_stop`, `gate_on_merges` |
| `standing_order_state` | `name` | `template`, `state` (`CHECK` from `OrderState::ALL`: `on`, `off`, `suspended`, `paused`), `src`, `cause` (no `CHECK`), `numbers_json`, `at_ms` |
| `merge_check` | `run_id` (= `workshop_run.id`) | `outcome` (`CHECK` from `MergeOutcome::ALL`), `src`, `checked_at_ms`, `settled_at_ms`, `tip_seen` |
| `standing_order_subject` | (`standing_order`, `subject_id`), WITHOUT ROWID | `at_ms` (source always `author`) |

- The two rebuilds (create, copy, drop, rename, indexes and `UNIQUE` constraints recreated, in the migration's transaction) follow migration 2's `setting` rebuild: widening a `CHECK` in SQLite needs one. A migration test runs from a version 3 fixture with rows in both tables and compares every row after.
- Indexes: `work_order(standing_order, launched_at_ms)`, `merge_check(outcome)`.
- No foreign key (D-133). Retention: `merge_check` rows go with their `workshop_run` (90 days, D-134); `standing_order_state` and `standing_order_subject` rows of an order absent from the approved snapshot for 90 days are deleted. No untrusted text is stored in any new column.
- Probe counts, ratio samples, bookings and hold history live in the journal, not the registry: each is a count or a fold over lines the journal already keeps 90 days (AD-32).

### AD-143: Journal kinds

`JOURNAL_LINE_VERSION` stays 1; `JournalKind` gains these variants, which earlier readers skip. No line carries text from a session, a model, a worker or the probe's report.

| kind | fields |
|---|---|
| `quota_observed` | `source`, `anchored_ms`, `written_at_ms?`, `five_hour?`, `weekly?`, `scoped` (`used_milli`, `resets_at_ms`, scope kind, label) |
| `usage_probe` | `outcome`, `failure?`, `asked_by`, `duration_ms`, `exit_code?`, `cost_usd?`, `num_turns?`, `stamp`, `backoff_until_ms?`, `anchored_ms?` |
| `ratio_sample` | `run_id`, `cost_micro_usd`, `pre_anchored_ms`, `post_anchored_ms`, per window `rise_milli`, `clean`, `cause?` |
| `governor_hold` | `mode`, `causes` (cause, numbers), `orders` (name, causes), `heartbeat`, `next_change_ms?` |
| `governor_would_launch` | AD-137's `LaunchRecord`, `officina_version` |
| `governor_launch` | the same, `run_id?`, `outcome` (`launched`, `revoked`, `refused`), `refusal?`, `revoke_causes?` |
| `ceiling_crossed` | `direction`, `count`, `ceiling`, `window_start?` |
| `package_held`, `package_released` | `run_id`, `count` |
| `package_expired` | `run_id`, `held_at_ms` |
| `merge_settled` | `run_id`, `outcome`, `source` |
| `automation_changed` | `standing_order`, `from`, `to`, `source`, `cause`, `numbers` |
| `governor_mode` | `from`, `to`, `by` |
| `governor_self_stopped` | `cause`, `numbers` |
| `autonomous_stopped` | `run_id`, `cause` |

`work_order_launched` gains `by` (`author`, `governor`); a missing `by` reads `author`. `governor_would_launch` and `governor_launch` lines are at most 16 KiB by construction (16 orders, bounded fields); a test builds the largest input and checks it.

### AD-144: Payloads, IPC commands, CLI

- **`GovernorView`** (`GOVERNOR_VIEW_SCHEMA = 1`, built in core, event `governor://updated`, emitted on byte change, AD-5), separate from `OrchestratorView` and `WorkshopView` so probes and steps do not churn them: mode, self-stop, the state line (decision kind, first cause with its numbers, every cause, `next_change_ms`); quota (each observation with source, anchored time and values, the combined readings, projection, reserve, ratios with their samples, allowances with their source, this week's autonomous points against the cap, the probe's last outcome, back-off, pause and today's count); attention (count against ceiling, review spent and pending against budget, held packages); shadow evidence (would-launch decisions of the last 14 days with their records, hold shares failing first and failing at all over the last 5 listed days, the share under the ceiling); standing orders (AD-132's assessment, per-order causes, state and reason, linked routine subjects or the "no routine subject" flag); autonomous runs ended without a package, `no_change` or quarantined; per template, AD-131's figures and AD-130's outcomes with corrections, and whether the gate switch is offered; in Slice B, AD-146's table and the shortcut's registration. Lists hold at most 50 rows; time-relative text is the frontend's (AD-5).
- **`Board`:** `governor: Option<GovernorSummary { mode, state, held, self_stopped }>` (built by the host from the last step, an input of `build_board`); the workshop card gains `autonomous: bool`. `BOARD_SCHEMA` goes up by one for v0.7 in the first Epic 7 story that changes `Board` (P-13's rule).
- **Briefing** (AD-83): governor mode, would-launch decisions or autonomous runs and their outcomes since the cursor, held packages, suspensions, pauses and self-stops. `officina mcp` serves them through `briefing` and `board`; no tool is added, and no tool changes a mode, launches, releases, links or corrects.
- **Commands** (`build.rs`, `capabilities/default.json`, `every_declared_command_is_allowed`; thin wrappers over `AuthorRequest` with a deadline, D-92):
  - read: `get_governor` (also asks AD-130's pass when older than 5 minutes);
  - Slice A: `set_governor_mode(off | shadow)`, `correct_merge(run_id, outcome)`, `set_gate_on_merges(on)`, `link_routine_subject(order, subject_id)`, `unlink_routine_subject(order, subject_id)`;
  - Slice B: `set_governor_mode(on)`, `stop_autonomy`, `set_standing_order(name, on)`; `discard_package` (v0.6) serves held and autonomous quarantined packages.
  - Pointer only (D-28): shadow, on, a standing order on, the gate on, a correction, a link. Keyboard-reachable, since they only reduce: off, stop autonomy, a standing order off, the gate off, an unlink.
- **CLI:** `officina governor --json`, `GovernorView` built read-only from disk with the host's read half; its state line is the latest decision line of the journal, since the CLI decides nothing and runs no probe. Exit 0; 3 when the registry cannot be read (D-47); 64 on usage; never 2. `readme.rs` (D-34) and `quota_doctor.rs` (D-80) cover it. **E2E fixtures** come from `crates/officina-host/examples/governor_fixture.rs` (`window-authority`): it seeds journals, registry rows, quota files and event lines at explicit `now_ms` and prints the view through the same host function as the CLI, at that `now_ms` (D-25 and D-72's pattern; the shipped binary carries no clock override).

### AD-145: Frontend

- **Governor section** of the Orchestrator view, beside the Workshop section: `GovernorSection.svelte` holds `GovernorMode.svelte` (mode, controls, self-stop), `GovernorState.svelte` (the state line and next change), `QuotaPanel.svelte`, `AttentionPanel.svelte`, `ShadowEvidence.svelte`, `StandingOrderTable.svelte` (with routine subject links through AD-55's `SubjectPicker`), `HeldList.svelte`, `TemplateEvidence.svelte` (acceptance, merges, corrections, the gate switch), and in Slice B `AutomationsTable.svelte`.
- **Text:** `lib/governor-text.ts` holds exhaustive `Record<HoldCause, (numbers) => string>`, `Record<AutonomyStopCause, string>` and the like, so a new Rust variant fails `pnpm check` (AD-13's rule). Numbers are formatted there from the payload's thousandths; no rule is computed in TypeScript.
- **Header:** neutral chips (AD-55: quiet surface, never `--color-attention`, not in the title, the overlay or the scene): "governor shadow", "governor on" or "governor holding", and "N held"; a terracotta chip for a self-stop, like "relay stopped" (AD-35).
- **Board:** the "autonomous" chip on workshop cards in Working and in Needs you. **Scene:** no new element (PRD FR-17).
- **Store:** `lib/governor.svelte.ts`. Every run text stays `Untrusted` and renders as text; author names (orders, templates, projects) are plain.

### AD-146: The Automations table (Slice B)

- Core `automations::table(&[SettingRow], &[JournalLine]) -> Vec<AutomationRow>`, read-only, built from records each automation already writes: `setting` rows with source `rules` of the triage model and auto-archive (D-69); the latest `auto_approval_set` line (v0.5); `automation_changed` lines of standing orders; `governor_mode` and `governor_self_stopped` lines.
- `AutomationKind` (`triage_model`, `auto_archive`, `auto_approval`, `governor`, `standing_order`) names the row's source; each row gives the current state, the last change, its source and its cause as recorded. No shared state type, no new rule, no write: v0.3's and v0.5's automations are untouched (PRD FR-15).
- Shown in the Governor section and as a briefing section. A vector case per kind, and the coverage test iterates `AutomationKind::ALL`.

### AD-147: Doctor

- **Slice A:** `[governor]` and standing order issues; per order, eligible and due or why not (core's functions on the read-only registry and the approved snapshot); the quota file's `written_at`, `session_id` and anchored time, and whether its windows are present (absent: no subscription rate limits in the statusline, or no response yet, S1); no file at all names the `officina quota write --tee` wiring; the probe's last outcome, back-off, pause with its stamp, and today's count; the ratios in use with their sample counts; the local offset the governor uses and the next window; the mode and any self-stop; `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` named when set in doctor's environment (a warning: it may turn the usage request off, as it once did for the gateway's spend fields); `<home>/usage/empty` present and empty.
- **Slice B:** held packages and their age; the shortcut's chord and whether the window registered it (read from the view the window last wrote, `governor --json`'s source).
- Doctor runs no probe and no git job.

### AD-148: Test strategy

| Layer | What | Where and how |
|---|---|---|
| core unit | phase 2 (a failing fixture per new `IssueKind`); canonical format (golden hash, previous-format fixture, AD-76's four facts); chord grammar; local time, window and listed-hours math at fixed offsets, a range crossing midnight, both daylight saving directions; settlement table; eligibility and state transitions; merge rules; anchoring, freshness, combination and conflicts; probe reading of every outcome; budget counting across a local midnight; projection, ratio and allowance at exact boundaries; review budget; hold shares with the heartbeat cap; automations table; view builder | CI, Windows and Linux; vector set `tests/vectors/governor.json` (formatter-exempt), sections `decide`, `clock`, `projection`, `settle`, `eligibility`, `merge`, `observe`, `probe`, `chord`, `automations`; coverage tests over `HoldCause::ALL` (each the **first** cause of at least one case), `Decision` kinds, `Settlement`, `MergeOutcome`, `ProbeFailure::ALL`, `OrderState::ALL`, `AutomationKind::ALL` |
| blocking property | for every `decide` case expecting a launch and every `HoldCause`, the test's `make_fail(cause, &mut input)`, an exhaustive `match`, makes that one condition fail: `decide` must hold with that cause present. A new variant fails the test's compilation until it gets its failing input | same test file, no crate |
| replay | every `governor_would_launch`, `governor_launch` and `governor_hold` line written by the host tests is parsed back and its `input` replayed through `decide`: same decision, same causes | host integration |
| principle 8 | a test reads `crates/officina-core/src/governor/**` and fails on the token `Untrusted` (D-128's source-reading pattern): no input type can hold untrusted text | core |
| type proofs | AD-139's `compile_fail` doctests; a doctest built without `governor-launch` showing `Decision::Launch` absent | `cargo test --doc` |
| spawn | `UsageProbe` golden case; through `spawn::execute` against the fake claude's `usage` mode: argument vector, directory refused when not empty, variables removed by name (AD-80's seeded environment plus the two credential names) | host integration |
| frozen clocks | the governor step with an injected `now_ms` and offset, seeded journals, registry, quota files and event lines: shadow bookings and cadence over a simulated week; probe budget, back-off and pause across a restart and a stamp change; freshness expiry; held, release, expiry and the self-stop; Slice B with the fake engine: zero creations in shadow, exactly one per launch in on, revoked grants, window end, live guards, stop autonomy, self-stops, suspension and pause | host and CLI integration, `window-authority` through the dev-dependency; `officina-fake-docker` (v0.6) and the fake claude |
| merges | real git on fixture repositories (D-23): merged locally, merged through a remote-tracking branch, retouched then merged, cherry-picked, squashed (pending, then the correction), branch deleted, 14 days; marker programs that must not run (AD-130) | host integration |
| injection battery | `injection.json` gains a `governor` section (D-147): proposal files and `propose_work_order` carrying a standing order's exact values, a floor edit adding an order (catalogue changed), a forged low quota file beside a higher probe, seeded excerpts and worker texts; the fake engine counts creations | D-147's harness |
| mutation | `mutant-governor-skip-freshness`, `mutant-governor-skip-ceiling`, `mutant-governor-skip-reserve` join D-148's job: for each, `cargo test -p officina-core --test governor_vectors` and `cargo test -p officina --test governor` must fail with a named case; `MUTANTS_ENABLED` covers them | CI `mutants` |
| zero write | `quota_doctor.rs` around `governor --json`, a probe with the fake claude, a merge pass (the project repository's references, index and work tree unchanged), a shadow step | CLI integration (D-80) |
| hotkey | chord to virtual key mapping on Windows; a thread registers a free chord and receives a `WM_HOTKEY` posted to it with `PostThreadMessageW`, which yields `StopAutonomy`; a chord held by the test itself fails to register and is reported | host integration, Windows only |
| frontend | Vitest: hold-cause texts for every variant, pointer-only controls ignore `detail === 0`, reducing controls keyboard-reachable, run texts inert. Playwright (`app/e2e/governor.spec.ts`) on AD-144's fixtures with `page.clock.install` at the fixture's `now_ms`: a shadow week, each first cause, held packages, a self-stop chip, the Automations table; chips never use the attention colour (D-163) | CI `windows` |

Manual, recorded in research notes: AD-149's gates; Slice A's shadow week; PRD section 7's rows at the close.

### AD-149: Gates, exact procedures

Each records an E-entry, numbered from the first free number after Epic 6's, in FR-0's order, with the date, versions and who ran it. An agent runs what needs no credential beyond the floor's own login and touches no settings file; the owner runs the rest. Every probe runs from a fresh empty folder outside the repository with the parent session's variables removed, and leaves nothing running.

| # | Procedure | Pass | On failure |
|---|---|---|---|
| 1 | Agent, a scratch script never committed: the exact `UsageProbe` vector of AD-134 every 10 minutes from 09:00 to 19:00 local on a working day, plus, every 2 hours, a burst of one per 2 minutes for 10 minutes: about 80 probes, above the budget's 60, so the day also measures a margin. Per probe, record start, exit code, elapsed time, `num_turns`, `local_command`, `total_cost_usd`, row kinds, percents, resets, and whether an entry of `USAGE_DEGRADED_MARKERS` matched; never the text. Owner: whether his interactive `/usage` showed last-known data or a rate limit that day, and its exact wording if so. **Verified in part** (section 1, items 2 to 5) | every probe clean or backed off; structural failures under 10 %; the owner's `/usage` unaffected | prompt form failing: retry with `--input-format stream-json` and one user message `/usage`; both failing, or the owner's `/usage` degraded: FR-5 is not built, or its budget is lowered to what the day allowed, and FR-4's fallback governs (D-127) |
| 2 | Owner: add `officina quota write --tee` in front of his statusline in his own user settings (Officina never writes them); open one interactive session and send a prompt; within one minute of the quota file's `written_at`, run the probe by hand. Three times over a day. Record both readings per window, the reset difference, whether the statusline's percent has fractions, and the `weekly_scoped` rows with their scope kinds | resets within `RESET_TOLERANCE_MS`, percents within 1 point | resets disagreeing beyond it: the probe is not used (FR-5 off) and the mapping is reopened; the tolerance is set from the recorded differences |
| 3 | Owner: with no other session of the account working, a probe; one clicked v0.6 workshop run on a scratch repository with a fixed multi-step prompt; a probe within 5 minutes after it ends. Record both readings, the rises, the run's cost estimate, the ratios, and whether the worker's stream carried `rate_limit_event`: its `status`, `utilization` and `resetsAt` against each window's reading | the 5-hour and weekly percents rise | nothing rises on `oauth-token`: Slice B is not built until the worker uses the floor's account; on `api-key`, FR-7 applies |
| 4 | Owner: a workshop template with `max_minutes = 5`, a prompt that commits once, then runs a 10-minute sleep through Bash; let the deadline stop it (the same per-run stop as AD-140) | a package in state `review` holding the commit | AD-140's window-end stop is dropped; FR-12's deadline, cut to the window's end, remains |
| 5 | Owner, a dev build: `examples/hotkey_probe.rs` registers `Ctrl+Alt+Shift+X` on a thread as AD-141 and prints on `WM_HOTKEY`; press it with the probe's console minimized, then behind a full-screen editor | registered; fires in both cases | Stop autonomy stays a button and Stop all a command |
| 6 | Agent: `examples/local_offset_probe.rs` prints AD-129's offset for now, for 2026-10-25T00:59Z and 01:00Z, and for 2027-03-28T00:59Z and 01:00Z, beside the clock's local time. **Verified in part** through .NET (section 1, item 6) | the current offset matches the clock and the four instants match the zone's rules | the window is read in UTC, the README says so |
| 0 | Owner, before any Slice B story: PRD question 0's record (terms text, per candidate floor and account, the employer's written answer where it applies, the weekly USD figure on an api-key floor) | recorded, at least one candidate allowed | Slice B is not built |

Question 7 is read from Slice A's view; question 8 is decided on the shadow's hold shares; questions 10 and 13 are the owner's; none is a gate of this addendum.

### Slices

- **Slice 0:** gates 1, 6 (agent) and 2, 3 (owner) before any Slice A story is implemented (FR-0); gate 3 needs one clicked v0.6 run. Gates 4 and 5 before the stories they gate; gate 0 before Slice B.
- **Slice A:** AD-127 without the grant, the hotkey and the launch; AD-128 to AD-138; AD-142 with every table; AD-143's kinds but `governor_launch` and `autonomous_stopped`; AD-144 without Slice B's commands; AD-145 without the Automations table; AD-147's Slice A part; AD-148's rows for what it lands. `Decision` has no `Launch` variant.
- **Slice B,** implemented only once PRD section 6.1.1's conditions are recorded: AD-139 to AD-141, AD-146, the `governor-launch` feature and its CI step, the Slice B commands, chips and doctor items, the mutants' launch half.

### FR to AD

| FR | Built by |
|---|---|
| FR-0 | AD-149, Slices |
| FR-1 | AD-130 |
| FR-2 | AD-131 |
| FR-3 | AD-132 |
| FR-4 | AD-133 |
| FR-5 | AD-134 |
| FR-6 | AD-129, AD-135 |
| FR-7 | AD-135 |
| FR-8 | AD-136 |
| FR-9 | AD-136 |
| FR-10 | AD-128 |
| FR-11 | AD-137, AD-138 |
| FR-12 | AD-139 |
| FR-13 | AD-140 |
| FR-14 | AD-140 |
| FR-15 | AD-146 |
| FR-16 | AD-141 |
| FR-17 | AD-144, AD-145 |
| FR-18 | AD-147 |

### Touch points with v0.2 to v0.6

- `core/floor.rs` (`RELAY_KEYS`, raw tables), `core/relay/catalogue.rs` (phase 2, the format bump, the diff), `core/relay/template.rs` (`check_values` shared with `WorkOrder::validate`), `core/relay/journal.rs` (kinds, `by`), `core/board.rs` (`governor`, the workshop card's `autonomous`, `waiting_count`'s exclusions, schema), `core/quota.rs` (unchanged contract; `observe` reads it), `core/dates.rs` (civil time from an offset, ISO 8601 parsing), the v0.5 `orders` module (`LaunchAuthority`, the governor card), `core/spawn` (`UsageProbe`), the v0.6 `workshop` modules (`FinishCause::AutonomyStopped`, `QuarantineCause::OutOfScope`, held columns, the card builder's Needs you rule), `core/contract.rs` (the `usage` surface).
- `host/watch.rs` (the governor step, its timer and wake-ups, two worker threads, `governor://updated`), `host/collector.rs` (event lines for anchoring), `host/spawn.rs` (the kind), `host/git.rs` (merge facts), `platform/windows.rs` (offset, hotkey), the registry schema (migration 4), the hourly pass, doctor, the workshop launch and collection paths.
- `officina/main.rs` (`governor`, usage), `quota_doctor.rs`, `readme.rs`, `injection.rs`; the app's `main.rs`, `build.rs` and capabilities; `ipc.ts`, `e2e-mock.ts`, `Header.svelte`, `OrchestratorView.svelte`, `WorkshopCard.svelte`; `e2e-fixtures.mjs` (kind `governor`, D-181's index).
- `.github/workflows/ci.yml` (the feature check of AD-127, the mutants' targets).

## 3. Amendments to earlier decisions

- **Spine, section 3 "Rejected for v0.1: Global hotkey":** v0.7 registers one chord with `RegisterHotKey` from the existing crate, no plugin (AD-141); the conflict concern is answered by a refused registration shown in the window and doctor.
- **AD-3:** threads `governor-probe`, `governor-git` and `hotkey`; the collector runs none of their work.
- **AD-4:** a governor timer (`GOVERNOR_TICK`, 60 s) and wake-ups at `next_change_ms` while the mode is shadow or on.
- **AD-12:** a `usage` surface in the contract canary, from `USAGE_FIELDS`.
- **AD-21:** `window-authority` also gates the governor step, its writers, the probe and git workers, the launch from a grant and the hotkey; it turns on core's `governor-launch`.
- **AD-24, AD-76, D-131:** the canonical format goes up by one for v0.7.
- **AD-32:** AD-143's kinds; `work_order_launched` gains `by`.
- **AD-35, v0.6 AD-121:** `waiting_count` leaves out held packages and autonomous quarantines.
- **AD-52:** one budget per purpose; the probe's is counted from the journal.
- **AD-55, AD-93:** the Governor section and its neutral chips.
- **AD-78:** `LaunchAuthority`; a governor card is issued and consumed in one collector step and never stored; the prompt-hash and plan-hash comparisons apply to clicks only.
- **AD-79, AD-80, D-128:** `VectorKind::UsageProbe`, a contained kind that also removes the two credential names.
- **AD-82, D-133, v0.6 AD-119:** migration 4 rebuilds `work_order` and `setting`.
- **AD-83:** briefing sections.
- **AD-95, D-147, D-148:** the `governor` injection section and three mutants with their test targets.
- **v0.6 AD-107:** a launch also takes `LaunchAuthority::Governor`; the issue facts are gathered on the `workshop` thread before the collector step.
- **v0.6 AD-109:** `FinishCause::AutonomyStopped`; per-run stops by the governor.
- **v0.6 AD-111:** step 6 checks a standing order's `paths` (`out_of_scope`).
- **v0.6 AD-114:** `expired` also discards a package held 7 days; the per-template table gains AD-131's figures.
- **v0.6 AD-119:** `workshop_run` gains `held_at_ms` and `released_at_ms`.
- **v0.6 AD-123:** doctor gains AD-147's items.

**Dependencies for approval.** No crate, image, tool or Node package is added.

| Item | Where | Licence | Why |
|---|---|---|---|
| windows-sys feature `Win32_System_Time` | host, existing crate | MIT/Apache-2.0 | `GetDynamicTimeZoneInformation`, `SystemTimeToTzSpecificLocalTimeEx` (AD-129, gate 6) |
| windows-sys feature `Win32_UI_Input_KeyboardAndMouse` | host, existing crate | MIT/Apache-2.0 | `RegisterHotKey`, `UnregisterHotKey`, `MOD_*`, `VK_*` (AD-141, gate 5); Slice B |

Both need Linux counterparts at v1 (PRD section 5). Ruled out: `chrono`, `time` and `jiff` (Windows converts the instant; core does civil arithmetic), `tauri-plugin-global-shortcut` and `global-hotkey` (a plugin and a crate for two calls), `proptest` (AD-148's blocking property derives its inputs from `HoldCause::ALL`), an ISO 8601 crate (two observed shapes), `git2`.

## 4. PRD amendments

1. **FR-1.** Append: "Remote-tracking branches are those of the repository's remotes (`git remote`). Patch equivalence is read with `git cherry` against each base reference. An outcome other than `pending`, and any correction, is final; `pending` rows are read again hourly. A merge pass runs no program the repository configures."
2. **FR-2.** Replace "and is journaled (`run_no_change`)" with "and is recorded by v0.6's `workshop_finished` line with outcome `no_change`". Section 8, question 11, is answered by v0.6 AD-110: with no commit past the base the packer writes no bundle, and a worker that exited 0 ends `no_change`.
3. **FR-4.** Append: "An event's `ts` counts at or before `written_at` up to the end of that second. Two fresh observations whose reset times for one window differ by more than 15 minutes, or that disagree on whether a 5-hour window is active, hold the governor (`quota_conflict`). Within that tolerance, the earliest 5-hour reset bounds the deadline and the latest weekly reset sets the projection."
4. **FR-5.** Add `--model haiku` to the vector ("it bounds the cost if a release sends `/usage` to a model; verified to change nothing for the local command") and to the environment rule "without `CLAUDE_CODE_OAUTH_TOKEN` and `ANTHROPIC_API_KEY`: the probe answers for the floor's own login, since a setup-token lacks the scope the report needs". Replace "a model turn (gate 1 records what a clean probe reports)" with "a model turn: a `result` with `num_turns` above 0, a `local_command` other than `usage`, or a non-zero cost (verified clean: 0, `usage`, 0)". Replace the back-off's detection and duration with: "the report's text contains an entry of core's `USAGE_DEGRADED_MARKERS`, seeded from S3's phrases; the text is matched, never parsed, stored or shown. No observation, and no probe for 60 minutes." Append: "A `session` row whose `resets_at` is null means no active 5-hour window." Add to the triggers: "and, in shadow or on, a bracket around each workshop run, clicked or autonomous (FR-6)".
5. **FR-6.** Replace "Every later clean bracket refreshes it" with "Each workshop run, clicked or autonomous, bracketed by a probe at most 2 minutes before its launch and one within 15 minutes after its end, is a sample; the ratio in use is the largest of the last 5 clean samples. Before any clean sample, allowances take the starting values." Append: "Points are computed in thousandths, rounded toward caution."
6. **FR-8.** Replace "the new reason `expired` (v0.6 FR-12 gains it" with "the reason `expired`, which v0.6 already gives a package undecided at its text bound (". Replace "and its container is removed as after any verdict" with "(its containers were removed at collection)".
7. **FR-11.** Add the hold causes `governor_unavailable` (step 3), `local_time_unknown` (step 4, first) and `quota_conflict` (step 11). Replace "the first is the one displayed" with "the first is the one displayed; per-order causes are those of the first due order in pick order, every order's own causes being listed with it". Replace "`Launch` launches nothing" with "the decision returns a would-launch with no grant; only mode on returns a launch". Append: "`governor_hold` is repeated every 15 minutes while unchanged, and a line counts at most 15 minutes past itself. `governor_would_launch` and `governor_launch` carry the decision's whole input, so each can be replayed."
8. **FR-12.** Replace step 3's note with: "The launch's git facts are read first, off the collector; then, in one collector step, the decision is taken again and must name the same order, the grant is built, and the card is issued and consumed with it through AD-78's path, without the prompt-hash and plan-hash comparisons that belong to a click. The card is never stored or shown." Append to Scope: "Paths match case-sensitively."
9. **FR-13.** Replace "the guard reads that instead of probing every 10 minutes, mapped as gate 3 records" with "a reading of it at or above a stop threshold also stops the runs, mapped as gate 3 records; it never replaces the 10-minute probe, since stream lines are untrusted (v0.6 FR-8) and the event names no window".
10. **FR-16.** Append: "The chord is registered with `MOD_NOREPEAT`, so holding it stops once."
11. **FR-17.** Append: "`officina governor --json` prints the section read-only, its state line from the journal's latest decision."
12. **Section 4, S4.** Append: "Verified on 2.1.312: `/usage` runs under `-p` as a local command (addendum section 1)."
13. **Section 6.1.2.** Replace the time zone row's feature use with "`GetDynamicTimeZoneInformation` and `SystemTimeToTzSpecificLocalTimeEx`".
14. **Section 8, question 1.** Append: "The prompt form, the report's rows, a clean probe's result and its zero cost are verified (addendum section 1); the working day and the degraded forms remain."

## 5. Risks

| # | Risk | Mitigation |
|---|---|---|
| R66 | A release stops running `/usage` under `-p`, which the CLI pages do not document (only the SDK page does) | golden vector; `USAGE_FIELDS` in the canary; structural failure, pause, file fallback with `one_per_window`; doctor |
| R67 | `usage_report`'s shape changes (the SDK page calls it experimental) | canary constants; every missing field is a structural failure, never a number |
| R68 | A degraded report is not recognised (new wording) and stale numbers, which only underestimate within a window, pass as fresh | markers seeded from the documentation and gate 1; the maximum with a fresh file; the weekly cap from cost estimates; the 10-minute live probe; resets in the past refuse freshness |
| R69 | Probing rate-limits the author's own `/usage` | budget counted from the journal; back-off without pause; gate 1's working day before the budget ships |
| R70 | `--bare` becomes the default for `-p` (documented as planned) and reads no OAuth login: the probe, triage and review calls fail | each failure is structural and pauses its call; the canary and doctor show it; the vectors gain whatever flag keeps the current mode when one exists |
| R71 | The weekly projection misjudges an uneven week | the weekly autonomous cap holds whatever the projection's error; the live reserve guard; the Reserve row's reading at each reset |
| R72 | The ratio errs low (whole points, short runs, use from chat) | largest of 5 clean samples; starting allowances until a sample; the weekly cap; the live guard |
| R73 | A process running as the author forges a low quota file while the probe is unavailable | outside the threat model (AD-25); the weekly cap does not read the file; `one_per_window`; doctor states the fallback |
| R74 | The chord collides with another program's or is pressed by accident | registration failure shown; the chord stops autonomy only; it needs approval to change |
| R75 | A daylight saving change inside a run moves the window's end by an hour | the window-end stop reads the offset at each step; accepted for the deadline |
| R76 | A worker forges `rate_limit_event` lines to keep running | the stream can add a stop, never remove the probe (AD-140) |
| R77 | The shadow shows the conditions never pass, and Slice B waits | the hold shares name the dominant starting value (PRD section 7); v0.8 does not wait |
| R78 | Squash merges and rebases read as not merged | the corrections; the gate on merges ships off and needs corrections under 30 % |
| R79 | The probe's text, which lists the author's local usage, leaks into a store | matched in memory and dropped; no field, line or payload carries it; the zero-write and credential-style search tests seek a seeded marker of the fake report's text in every file and line |
| R80 | The author's environment sets a credential variable or turns non-essential traffic off, and the probe answers for another account or not at all | the vector removes both credential names; doctor names the traffic variable; a missing report is a structural failure |
| R81 | Migration 4's rebuilds of `work_order` and `setting` lose a row or a constraint | `pre-v4.db` copy; a migration test from a version 3 fixture compares every row, `UNIQUE` and index |
| R82 | A suspended order comes back through an edit of the floor | state keyed on the order's name and kept across edits; only the author's click turns it on |

## 6. Addendum (2026-10-28): the critic and research helpers

Inputs: prd-v0.4.md and architecture-v0.4.md as reworked on 2026-10-28; architecture-v0.5.md section 6. PRD section 9 states the requirements; `epic-7-decisions.md` D-492 and D-493 record the decisions. AD-127 to AD-149 hold except as amended here.

### AD-183: Helpers as governor-launched work

- **Template key** (joins AD-128, phase 2, part of the canonical catalogue): `role = "critic" | "research"` on a workshop template; a template with a role must declare no Import (its run ends with a report only) and names the read-only capabilities v0.6 already defines; validation refuses a role on a host template.
- **Launch**: unchanged paths. A clicked card (v0.5 AD-78) or the governor's decision (AD-137) on a standing order whose trigger is one of the closed conditions of AD-137; no new trigger reads model text. Helper runs count in the ceiling, the allowances and the weekly cap like any autonomous run.
- **Report**: the run's final report is stored as v0.6 stores a package's summary, `Untrusted<String>`, capped; `officina mcp` gains the read tool `helper_reports {limit <= 10}` returning reports with their template, project and run id, wrapped as untrusted (v0.5 AD-90's rule). No tool sends a report to a session.
- **Journal**: the existing workshop and governor kinds, with the template's role.

### AD-184: Helpers in the scene

- `SceneInput` gains `helpers: { runId: string; role: "critic" | "research" }[] | null`, the live helper runs from the workshop view (nullable, AD-68's rule).
- Each is a rig scaled 0.68 without a cap, in `--orch-tunic` (v0.4 AD-171); its place is a spot beside the orchestrator's desk (one per role, then the decision office's group), reached through the staff door nodes of v0.4 AD-170; it walks in when the run appears and out when it ends (v0.4 AD-172). It is static once placed: no bubbles, no blinking. Its target opens the run's card.
- Helpers are never counted as waiting; a helper whose run waits on the author (a package to review) is shown by the run's card, not by a second figure.

## 7. Risks of the addendum

| # | Risk | Mitigation |
|---|---|---|
| R111 | Helper runs spend quota the author expected for project work | they count in the ceiling, allowances and weekly cap; standing orders for helpers are the author's text and approval |
| R112 | A helper's report steers the orchestrator's next proposal toward an injected goal | reports are untrusted text; proposals act only through the author's click (v0.5 FR-12, prd-v0.5.md section 9 item 3) |
