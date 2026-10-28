---
title: "Architecture addendum: Officina v0.5 - The Orchestrator"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions; gates of PRD section 8 open); addendum of section 6 (2026-10-28: scene v2, orchestrator answers, permissions design)
inputs:
  - prd/prd-v0.5.md
  - (requirement challenge, not published)
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - architecture-v0.4.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - stories/epic-3-decisions.md
  - (the code: crates/officina-core/src/relay, crates/officina-host, app/src-tauri)
  - https://code.claude.com/docs/en/sessions
  - https://code.claude.com/docs/en/cross-session-messaging
  - https://code.claude.com/docs/en/agent-view
  - https://code.claude.com/docs/en/permission-modes
---

# Architecture addendum: Officina v0.5

AD-1 to AD-72 stay in force except where section 3 amends them; this addendum adds AD-73 onward and risks from R33. **Verified** marks a fact observed on the workstation for this addendum (section 1). **Documented** marks a page read on 2026-10-28. **Gate** marks a fact still to check on the workstation, with the procedure of AD-96, before the dependent story closes.

One property organises everything below: model text reaches a process only as data on stdin or as a displayed string, never as a token of an argument vector, and the only argument vectors that carry text carry either author text (a template, an answer) or values the author's own closed lists contain. The type system enforces the first half; a mutation test and an injection battery check the whole (AD-95).

## 1. Checked on the workstation

Claude Code 2.1.312 on Windows 11, 2026-10-28. Every run started from an empty scratch folder outside the repository, with the parent session's variables removed from the environment, print mode unless stated, model Haiku; about USD 0.006 of model turns in all. `claude agents --json --all` listed the same sessions before and after, none in the scratch folder. No page fetched carried instructions addressed to an assistant.

**Verified.**

1. **The binary's own help.** `--permission-mode` accepts `dontAsk`; `--disallowedTools`, `--session-id`, `claude logs <id|name>` and `claude respawn` exist. The `--bg` help says that with `--resume <session-id>` it "continues that session in the background under the same ID, or starts a copy and says so when the session is already running". PRD question 5 is answered: no in-place delivery to a running session through `--bg`.
2. **S7.** With `CLAUDE_CODE_DISABLE_AGENT_VIEW=1`, `claude --bg ... -- <prompt>` prints `'--bg' is disabled by CLAUDE_CODE_DISABLE_AGENT_VIEW.`, exits 1 and starts nothing.
3. **Trust comes first for `--bg`.** In an untrusted folder `claude --bg` exits 1 with `Workspace not trusted` before validating its arguments (an unknown option gave the same error), so question 2 for `--bg` cannot be settled in a scratch folder.
4. **The bare `--`.** `claude -p --resume <unknown id> "--zzz ..."` fails with `unknown option`; with `--` before the prompt it parses and fails with `No conversation found`. With stdout not a TTY, `claude --name x -- --bg x` ran as print mode, a real model turn, and `--bg` arrived as prompt text (S4's rule). A `claude` spawn with a prompt and no TTY is a model turn: the work order vector's `--bg` before `--` is load-bearing.
5. **Argument fidelity.** Spawned with an argument list quoted by MSVC rules (Python's `subprocess`, the rules Rust's `Command` applies, as E13), four prompts placed after a bare `--` reached a `UserPromptSubmit` hook byte for byte: a leading dash with quotes, an escaped quote, a newline and a tab; `%`, `^`, `&`, `|`, `<`, `>`, `!`, `$` and `*`; accented letters, backslashes and a trailing backslash; and inner `--`, `--bg` and `--model` text.
6. **Flag settings survive `--setting-sources ""`.** That hook was supplied only through `--settings` while `--setting-sources ""` was set, and it ran: the flag source stays in force when every file source is off. This is indirect evidence for `crossSessionInbound` from `--settings`, not proof.
7. **The orchestrator's flag set, print mode.** `--tools "" --strict-mcp-config --mcp-config <one stdio server named officina> --setting-sources "" --settings {"crossSessionInbound":"refuse"} --permission-mode dontAsk --allowedTools mcp__officina --append-system-prompt <text>` with `CLAUDE_CODE_DISABLE_AGENT_VIEW=1`, in two runs:
   - Without `--disallowedTools`: the init message lists exactly one tool, `mcp__officina__briefing`, and the permission mode `dontAsk`. The MCP tool ran with no denial, so a server-level allow rule matches its tools. Asked to read a file and run a command, the session had no tool for either.
   - With `--disallowedTools SendMessage,ListAgents` added: the flag is accepted, and the init message lists the same single tool.
   - Without `--tools ""`, the default print-mode tool set includes `ListAgents` and `SendMessage`.
8. **The MCP server's view.** E11's handshake holds on 2.1.312 (`server/discover` answered `-32601`, then `initialize`, `notifications/initialized`, `tools/list`, `tools/call`). The server's `CLAUDE_PROJECT_DIR` and working directory equal the session's folder. Its environment carries `CLAUDE_CODE_SESSION_ID` and the session's messaging socket and token (names checked, values never read).

**Documented.**

- **Sessions page (S3).** As the PRD states, plus: with agent view turned off, `claude --resume` on a running background session sends nothing and exits 1; on success it prints `Sent your prompt to the background session (<id>); opening it…` before attaching. An ended session is loaded from its transcript; `--mcp-config` and `--settings` are not restored.
- **Messaging page (S5).** A `-p` session binds an inbox like an interactive one. A session refusing messages shows no change in its own `/status`; the `/config` row "Messages from your other sessions" is absent while `--settings` sets the key. Deny rules naming `SendMessage` and `ListAgents` stop sending and listing. On Windows the session's token is the only proof of an own-child message.
- **Permission modes page (S2).** `dontAsk` still runs "actions that need no approval in Manual mode", so a tool that never prompts would run if present.
- **Agent view page (S4).** The supervisor keeps `--mcp-config`, `--strict-mcp-config`, `--settings`, `--setting-sources` and a few others across restarts; `--tools` and `--allowedTools` are not on that list. A `--bg` session "starts the way a new `claude` session in that directory would". Bypass acceptance for a background session must be recorded in user or managed settings.

**Gate.** PRD questions 1 (interactive), 2 (interactive and `--bg`), 3, 4 and 7, by AD-96. Question 5 is closed by item 1; question 6 is not a gate.

## 2. Decisions

### AD-73: Module layout and where authority lives

- `officina-core` (pure, `now_ms` as argument):
  - `untrusted`: `Untrusted<T>`, `DisplayText` (AD-74);
  - `relay::catalogue` gains templates, the orchestrator and review tables, format 2 (AD-75, AD-76); `relay::template`: `TemplateName`, `ParamName`, `ProjectName`, `ModelAlias`, `ApprovedTemplate`, rendering (AD-77);
  - `orders`: `work_order` (`WorkOrder`), `card` (issue, edit, launch checks), `proposal` (file schema, ingestion validation), `briefing`, `view` (`OrchestratorView`);
  - `meeting`: view builder, answer rules, delivery choice, escape stripping, diff shaping (AD-84, AD-85);
  - `relay::auto`: review input, prompt, schema, output validation, escalation causes, rule baseline, shadow statistics (AD-86 to AD-88);
  - `spawn`: `VectorKind`, `ClaudeVector`, one builder per kind, the environment rule, the orchestrator's constants (AD-79, AD-80, AD-89);
  - `mcp`: JSON-RPC types, `McpTool`, tool schemas and bounds, pure dispatch (AD-90).
- `officina-host`:
  - `spawn`: the only code that creates a `claude` process. Its read half (`Agents`, `Version`, `Logs`, `Attach`) is always compiled; its launch half (`WorkOrder`, `Answer`, `Orchestrator`, `TriageCall`, `ReviewCall`) only under `window-authority` (AD-21), its error type carrying `AUTHORITY_MARKER` so D-9's CI step covers it. `claude::run` and `platform::windows::open_attach_console` move into it.
  - `git`: read-only git with D-22's flags, shared by the tree fingerprint (AD-33) and the meeting (AD-84).
  - `orders` (`window-authority`): card store, launch, proposal ingestion, orchestrator lifecycle; `relay::review` (`window-authority`): the review worker.
  - `mcp`: the server loop, compiled into `officina.exe`. It reads, and writes only proposal files, which are untrusted by construction (P-5 holds).
- `officina`: subcommand `mcp [--floor <path>]`; `run` serves `auto` (AD-86).
- One spawn module is a lint, not a promise: `crates/officina-host/clippy.toml` sets `disallowed-methods` to `std::process::Command::new`, allowed with a stated reason in exactly three modules: `spawn` (`claude`), `relay::run` (catalogue programs) and `git`.

Gives up: four new core modules and a split `spawn`; in exchange "no CLI path launches a session" is a property of the shipped `officina.exe`, checked by the existing marker step.

### AD-74: `Untrusted<T>` and the display type

- `pub struct Untrusted<T>(T)`, with no `Deref`, `AsRef`, `Borrow`, `Display`, `From`/`Into` to `String` or `&str`, and no public field. It serializes and deserializes transparently (`#[serde(transparent)]`, ts-rs `as = "String"`), so payload contracts and TypeScript types do not change.
- Read access is `display(&self) -> DisplayText<'_>`. `DisplayText` implements only `Serialize` (ts-rs `string`): the frontend renders it as text (`svelte/no-at-html-tags`). Facts computed in core without exposing the text: `char_count`, `truncated(n) -> Untrusted<String>`, `first_line_matching(&[&str]) -> Option<u32>`.
- Every core payload field holding session or model text becomes `Untrusted<String>`: board card name, prompt and answer excerpts, `ApprovalView.session_name`, registry excerpts and `model_answer.reason`, and every v0.5 field (proposal texts, commit subjects, file paths in a diff, log output, review reason, MCP outputs). Serialized bytes are unchanged, so no schema number moves for this.
- **`compile_fail` doctests** on `WorkOrder::validate`: passing an `Untrusted<String>`, its `display()`, `&*untrusted`, `untrusted.as_ref()`, `format!("{}", untrusted.display())` or `String::from(untrusted)` does not compile; neither does a struct literal of `WorkOrder` or `ApprovedTemplate` outside their module. A positive doctest shows the author's path.

What the doctests prove is that those snippets fail, not that no path exists: `serde_json::to_string` still yields a string. The mutation test and the injection battery (AD-95) carry the rest of the claim, as the challenge asked.

### AD-75: Templates and the new floor tables

Schema 2 gains keys; the floor schema number does not move (rationale below).

```toml
[[template]]
name = "dependency-bump"
description = "Bump one dependency and fix what breaks"
projects = ["backend"]
prompt = "Bump {package} to its latest {level} release, run the tests, fix what breaks, commit."
model = "sonnet"                       # optional, closed list

[template.params.package]
values = ["serde_json", "tokio-util"]

[template.params.level]
values = ["patch", "minor"]

[orchestrator]
model = "sonnet"                       # optional

[auto_review]
model = "haiku"                        # optional, default haiku

[[action]]
name = "apply"
class = "auto"
check = "check"                        # optional on auto: the after-check rule applies first
auto_max_per_hour = 6                  # auto only, 1..=60, default 6
```

- **Phase 1** (AD-23) keeps `template`, `orchestrator` and `auto_review` as raw spanned values, as it keeps `action`: nothing about them can invalidate the floor the board uses. `RELAY_KEYS` gains the three, so a schema 1 floor carrying one names `schema = 2` (D-7's test covers them).
- **Phase 2** converts each template alone (`deny_unknown_fields`); each rule is an `IssueKind` variant with a failing fixture, and a failing template is dropped with its issue:
  - name passes `is_valid_action_name`, unique among templates (templates and actions are separate namespaces);
  - `description` 1 to 200 characters, no control or format character;
  - `projects` non-empty; a name that is not a catalogue project is an issue and leaves the template; a template left with none is dropped;
  - `prompt`: braces delimit placeholders only (D-2's rule); every placeholder names a declared parameter and every parameter is used; no control character but newline and tab, no format character (`FORBIDDEN_FORMAT_RANGES`); its first character is author text that is not `/`, `!` or `-` (S3, AD-96 question 2); its worst case, each placeholder replaced by its longest value, is at most 8,000 characters, so rendering cannot exceed the bound;
  - a parameter has `values` only, at least one; `pattern` is the issue `TemplatePattern`; each value passes core's new `template_value` rule: FR-2's length and character set, then AD-30's runner-independent rules (`check_value` is split so D-5's native-only refusal of a leading `/` or `@` stays with argument vectors: a template value lands in a prompt, so `@scope/pkg` is accepted);
  - `model` parses as `ModelAlias`, a closed enum with `ALL` in core (`haiku`, `sonnet`, `opus` today; `fable` appears in 2.1.312's help and joins when the owner asks).
- **Actions:** `class = "auto"` is valid (the `AutoReserved` issue goes); `check` is allowed on `auto` and keeps its `after-check` meaning; `auto_max_per_hour` set on another class is an issue.
- **`[orchestrator]` and `[auto_review]`:** a failing table is an issue. The orchestrator then starts with no model flag; a failing `[auto_review]` makes every `auto` request escalate with cause `review_config_invalid` (AD-87).
- **No floor schema bump.** AD-22's bump was an opt-in to a changed default (guarded); templates change no default. Gives up: a v0.4 binary reading a v0.5 floor reports an unknown key instead of "needs schema 3".

### AD-76: Canonical catalogue format 2, approval voiding

- `CANONICAL_FORMAT` becomes 2. `CanonicalCatalogue` gains `templates: BTreeMap<String, CanonicalTemplate>` (description, sorted projects, prompt as written, parameters with sorted values, model) and `auto_review: CanonicalAutoReview { model }` with the default explicit; `CanonicalAction` gains `auto_max_per_hour: Option<u32>` (`Some` with the default for `auto`, `None` otherwise). `[orchestrator]` is not in it: the orchestrator approves and launches nothing.
- The new fields take `#[serde(default)]`, so an `approved.json` written in format 1 still parses. Its stored `hash` is authoritative and never recomputed. It differs from every format 2 hash, so `officina run` refuses with "catalogue changed, approval pending" (AD-29 step 2), and the gate keeps applying the old snapshot's guarded projects (D-14, D-15). The banner's diff shows `format` 1 to 2 and the templates added: one approval by the author voids nothing else. A test loads a format 1 fixture and asserts each of those four facts.
- `catalogue::diff` gains a templates section (added, removed, changed per field) and an `auto_review` field; the Relay view's catalogue table gains a templates table.
- Later `[[template]]` changes (base branch, budget, capabilities in v0.6 and v0.7) bump the format again: accepted (FR-2). The golden hash test of AD-24 is updated in the commit that changes the form, and nowhere else.

### AD-77: `ApprovedTemplate`, `WorkOrder`, rendering

- `ApprovedTemplate::new(approved: &ApprovedCatalogue, current: &RelayFloor, name: &TemplateName) -> Result<ApprovedTemplate, TemplateUnavailable>` is the only constructor. It requires `current.hash == approved.hash` (a floor edited since approval gives `catalogue_changed`) and reads the template from the approved snapshot. It takes each allowed project's folder from the current floor in its declared spelling, resolved against the floor (D-6's rule), never the lowercased comparison form. Fields are private.
- `WorkOrder::validate(&ApprovedTemplate, &ProjectName, &[(ParamName, &str)]) -> Result<WorkOrder, WorkOrderRefusal>` is the only constructor (FR-3). The checks, in order: the project is one of the template's; every declared parameter is present once and none is extra; each value equals an entry of its list, byte for byte, and the `WorkOrder` stores the template's own string, not the caller's; the rendered prompt is at most 8,000 characters and does not start with `/`, `!` or `-`. `WorkOrderRefusal` is a closed enum, one test per variant.
- `WorkOrder` holds, private, with getters only: template, project, project folder, values, rendered prompt, its SHA-256, model, catalogue hash. `spawn::work_order(&WorkOrder, seq: u32)` is the only builder of the `WorkOrder` vector kind, so the vector can only come from a validated order.
- `ParamName` and `ProjectName` parse with the catalogue's name rules; `TemplateName` with `is_valid_action_name`.

### AD-78: The card store and the one gesture

- **Where:** in memory on the collector thread (AD-3), which already holds the board, the relay view and the registry writer. Cards need no persistence: a proposal's card is issued again after a restart, and a composer card is cheap to rebuild.
- **Card id:** 32 hex characters from two `RandomState` hashes of `(now_ms, counter)` (AD-51's source of randomness). Uniqueness matters here, not secrecy.
- **Issue:** `AuthorRequest::IssueCard { template, project, values, subject }` or `IssueCardFromProposal { proposal_id }` runs `ApprovedTemplate::new`, `WorkOrder::validate` and `spawn::work_order`. It returns a `WorkOrderCard` (ts-rs) or the typed refusal. The card carries:
  - id, template name and description, project;
  - each value with its full list (the composer's choices);
  - the rendered prompt, its SHA-256, the argument vector exactly as `spawn` built it, the working directory and the model;
  - the subject choice with its options, the quota windows from the board;
  - for a proposal, its id and rationale as `DisplayText` under a "proposal" label;
  - the catalogue hash, `issued_at_ms` and its state.
- **Subject choice:** `Existing(subject_id)` or `New(name)` (D-57's name rules). The default is the project's single `active` or `to_resume` subject if there is exactly one, else the proposal's subject, else `New` named after the template.
- **Edit:** `EditCard { card_id, change }` (a value from its list, or the subject) issues a new card and marks the old one superseded. The store keeps at most 32 cards; an evicted id is unknown.
- **Launch checks,** in this order, each a `LaunchRefusal` variant with a test: unknown card; superseded; already consumed; the prompt hash from the click differs from the card's; the approved or current catalogue hash differs from the card's; the template is no longer approved; `claude` resolves to a shim (`.cmd`, `.bat`); the project folder is missing; the chosen subject is archived or merged. Then the card is consumed (single use, so a double click launches once) and the order is recorded (AD-81).
- **Gesture:** Launch acts on a pointer click only (`event.detail > 0`, D-28) and sends `card_id` and `prompt_sha256`. No key launches.

### AD-79: One spawn module, vector kinds, golden vectors

`officina-core::spawn::VectorKind` is a `closed_enum!`. Core builds a `ClaudeVector { kind, args, cwd, console, stdin, capture, deadline_ms, env_set, env_remove }` from typed inputs only; the host's `spawn::execute(&ResolvedClaude, &ClaudeVector)` is the one place that turns it into a process.

| Kind | Arguments after the binary | Working directory | Console | stdin | Capture, deadline | Shim |
|---|---|---|---|---|---|---|
| `Agents` | `agents --json --all` | inherited | none | null | stdout 8 MiB, 5 s | through `cmd.exe /d /c` (AD-6) |
| `Version` | `--version` | inherited | none | null | first line, 5 s | same |
| `Logs` | `logs <bg id>` | the binary's folder | none | null | stdout 256 KiB, 5 s | refused |
| `Attach` | `attach <bg id>` | the binary's folder | new | console | none, not waited | refused |
| `WorkOrder` | `--bg --name <template>-<seq> [--model <alias>] -- <rendered prompt>` | project folder (AD-77) | none | null | stdout and stderr 64 KiB, 60 s | refused |
| `Answer` | `--resume <session id> -- <answer>` | live background: the binary's folder; ended: the session's folder | new | console | none, not waited | refused |
| `TriageCall` | AD-51's flags (amended, section 3) | `<home>/triage/empty` | none, job object | the data | stdout 64 KiB, 60 s | refused |
| `ReviewCall` | AD-51's flags with the review model and schema (AD-86) | `<home>/review/empty` | none, job object | the data | stdout 64 KiB, 60 s | refused |
| `Orchestrator` | AD-89 | `<home>/orchestrator/workdir` | new | console | none; pid kept | refused |

- **Typed inputs.** A background id must match `^[A-Za-z0-9_-]{1,64}$` (AD-15). A session id is a canonical lowercase UUID. The answer is an `Answer` (AD-85), the prompt comes from a `WorkOrder`, and a model is a `ModelAlias`. Every text argument sits after a bare `--` and is exactly one argument (section 1, items 4 and 5).
- **Golden vectors** live in `tests/vectors/spawn.json` (formatter-exempt), with at least one case per kind: inputs, then the expected arguments, directory rule, console, stdin, capture, deadline, and variables set and removed. A coverage test iterates `VectorKind::ALL`. A host test runs every kind through `spawn::execute` against the fake claude and compares what it logged (argument vector, directory, variable names) with the same file.
- **Gives up:** `Attach` starts refusing a `claude.cmd` shim it accepted before (AD-15 ran it from the binary's folder). A shim install now gets the copy-attach-command path only, which doctor states.

### AD-80: Spawn environment

- **Removed from every vector:** the per-session variables Claude Code exports to a session's children, kept in core's `SESSION_ENV_REMOVED` (it replaces AD-51's `TRIAGE_ENV_REMOVED`), plus every `OFFICINA_*` name. That constant covers session identity, project dir and pid, the entrypoint and nesting markers, and the messaging socket and token. Its contents were observed in a Bash child on 2.1.312, and research notes record them as E18. The reason is concrete: on Windows a child holding a session's messaging token is accepted as that session's own child (documented, section 1). A window started from a Claude Code session must not hand a work order or an orchestrator its parent's inbox. A host test seeds every name of the constant in the parent environment and asserts that none reaches the fake claude, for every kind.
- **Set:** the floor's `CLAUDE_CONFIG_DIR` (AD-6) and `NoDefaultCurrentDirectoryInExePath=1` (AD-15), on every kind.
- **Per kind:** `Orchestrator` sets `CLAUDE_CODE_DISABLE_AGENT_VIEW=1`. `WorkOrder`, `Answer` and `Attach` remove it, since `--bg` and `--resume` on a running session fail with it (section 1, item 2, and the sessions page). Doctor warns when the author's environment or user settings turn agent view off (AD-94).
- The author's other variables (for example the Git Bash path Claude Code needs on Windows) are inherited: a work order runs as any session the author starts in that folder (FR-3).

### AD-81: Launch flow, binding, filing

1. The collector applies AD-78's checks, consumes the card and inserts a `work_order` row with outcome `launching` and `seq` = 1 + the template's highest. That makes the name `<template>-<seq>` unique in the registry.
2. The Tauri command runs `spawn::execute` for the vector inside `spawn_blocking` (AD-3), outside the collector, and sends the outcome back as an `AuthorRequest`.
3. **Outcome:** exit 0 gives `started`, with stdout kept (4 KiB, `Untrusted`, display only). Stderr holding `Workspace not trusted` gives `untrusted_workspace`. Any other non-zero exit gives `failed` with the code and the first 500 characters of stderr; a deadline gives `failed` with `timeout`. One journal line `work_order_launched` (template, project, values, argument vector, prompt SHA-256, catalogue hash, outcome). Nothing retries (FR-4). At window start, `launching` rows older than 2 minutes become `unknown`.
4. **Binding:** on each agents refresh, a `started` order younger than 10 minutes binds to the first background entry whose `name` equals the order's name, whose `cwd` lies in the project folder and whose `startedAt` is at most 5 s before `launched_at_ms`. That sets `bg_id`, `session_id` and `bound_at_ms`. The id `--bg` printed is recorded and must equal the bound entry's `id`, else the row notes it. The printed format is undocumented, so a unique name, not stdout parsing, decides. An order unbound after 10 minutes reads `unbound` in the history.
5. **Filing:** D-79's session creator consults bound orders first and files the session under the order's subject with source `author`; a `New` subject is created at outcome `started`, never at the click. If the hook's `SessionStart` created the session first and filing by rule placed it elsewhere, binding re-files it (`author` beats `rules`).

### AD-82: Registry migration 2

`MIGRATIONS[1]`, append-only once v0.3 has shipped (P-14's rule moves to it: edited in place until v0.5 ships, its SHA-256 pinned). `VACUUM INTO registry/backup/pre-v2.db` first (AD-41). `CHECK` lists come from the core enums' `ALL` and AD-40's comparison test covers the new columns.

| Table | Key | Columns |
|---|---|---|
| `work_order` | `id` | `seq`, `template`, `project`, `values_json`, `argv_json`, `prompt_sha256`, `catalogue_hash`, `name`, `subject_id`, `new_subject_name`, `source` (`composer`, `proposal`), `proposal_id`, `launched_at_ms`, `outcome`, `exit_code`, `error`, `printed`, `bg_id`, `session_id`, `bound_at_ms` |
| `proposal` | `id` (the file id) | `kind` (`recap`, `work_order`), `status`, `invalid_reason`, `received_at_ms`, `expires_at_ms`, `orchestrator_session_id`, `target_subject_id`, `target_session_id`, `template`, `project`, `values_json`, `subject_choice_json`, `texts_json`, `decided_at_ms`, `work_order_id` |
| `orchestrator_start` | `id` | `started_at_ms`, `pid`, `outcome`, `error` |
| `auto_review` | `id` | `approval_id`, `request_key`, `project`, `action`, `requested_at_ms`, `mode` (`shadow`, `live`, `rule`), `outcome`, `cause`, `decision`, `reason`, `rule_baseline`, call metrics (AD-40's `triage_call` columns), `author_decision`, `author_reason` (`should_not_run`, `not_now`), `decided_at_ms`, `run_id`, `marked_at_ms` |

- `setting` is rebuilt (create, copy, drop, rename, in the migration's transaction) because its key `CHECK` widens: `SettingKey` gains `auto_approval`, `auto_mode` and `briefing_read_at_ms`.
- **Retention** (AD-44's pass): proposal and review texts fall under D-85's 30-day bound (`texts_json` and `reason` cleared, rows kept); `work_order`, `orchestrator_start` and review rows are deleted at 90 days.
- Untrusted texts are stored only in `proposal.texts_json`, `auto_review.reason`, `work_order.printed` and `work_order.error`, and read back as `Untrusted<String>`.

### AD-83: The briefing

- `orders::briefing::build(&BriefingInput, cursor: Cursor, now_ms) -> Briefing` takes the board, the relay view, the registry view and the orders state (work orders, pending proposals).
- **Sections:**
  - open subjects with their sessions' states; Needs you and Your turn sessions; pending approvals; triage rows;
  - failed and refused runs; launched work orders with their sessions' states; pending proposals;
  - `changed`: items with `at_ms` after the cursor (runs finished, sessions that started waiting, subjects archived or reopened, orders launched, proposals received).
  - Session names and every session text are `Untrusted` (AD-74); subject and template names are author text.
- **Bound:** at most 32 KiB serialized. While over, the globally oldest list item is dropped and counted in `dropped` per section; counts are never dropped. Same input, same bytes (vector cases in `tests/vectors/orders.json`).
- **Cursors:** the window uses `setting.briefing_read_at_ms`, written when the author leaves the Orchestrator view or closes the window. `officina mcp` uses the `started_at_ms` of the orchestrator start before the current one. `Cursor` says which, and `none` on a first run.
- The window and `officina mcp` call the same function on the same kinds of input, read-only on the CLI side (AD-53), so the author and the orchestrator read the same briefing.

### AD-84: The meeting (Receive)

- `meeting::build(...) -> MeetingView` (ts-rs) has these sections:
  - **decision expected:** `waiting_for`, the latest `Notification` message, the answer tail (AD-45), the last prompt;
  - **runs and approvals:** this session's runs from the journal (succeeded, failed with exit code, refused), each with its journal reference, and its pending approvals;
  - **recap:** the latest recap proposal under its label (Slice C);
  - **changes, logs and delivery** (below).
- **Changes:** host `git` runs on Receive only, in a worker, 10 s deadline, `CREATE_NO_WINDOW`, with D-22's flags and `GIT_OPTIONAL_LOCKS=0` so a read never writes the index.
  - Commits: `log --since=<startedAt as ISO 8601 UTC, core's date math> --name-status`.
  - One diff from the parent of the oldest of those commits (or HEAD, or the empty tree) to the working tree, capped at 256 KiB, with per-file offsets for the jump.
  - Untracked files are listed by name and size, never read.
  - Commit subjects and paths are `Untrusted`. Outside git, the section says so.
- **Logs** (background only; Gate, question 7): the `Logs` vector; core strips escape sequences (CSI, OSC, other ESC sequences), keeps the last 64 KiB and finds the first line matching core's `LOG_ERROR_MARKERS`. If question 7 fails, the section and the vector kind go.
- **Delivery:** `meeting::delivery(&Card) -> Delivery`, a closed enum. A live background session not waiting gives `ResumeLive`. A background session waiting (`waiting`, `blocked` or `waitingFor` present) gives `CopyAndAttach`. An ended session (interactive ended or interrupted, background `stopped` or `failed`) gives `ResumeEnded`. A live interactive session gives `FocusOnly`. A `done` background session with a pid is live and idle (E1).
- `open_meeting(card_key)` is request and response; the panel asks again when the board shows that card's state changed. No transcript is read (framing principle 3).
- **Scene:** `SceneInput` gains `received_key`; the layout marks that desk with a neutral `--color-working` outline. Opening a meeting changes the scene once, which AD-70's mutation test expects.

### AD-85: Answer and delivery

- `Answer::parse(&str) -> Result<Answer, AnswerRefusal>`: 1 to 4,000 characters; no control character but newline and tab; no format character; not starting, after leading whitespace, with `/` or `!` (S3). A leading `-` stays allowed while question 2 holds; if it fails, `AnswerRefusal::LeadingDash` is added.
- `send_answer(card_key, expected: Delivery, text)` works in three steps.
  - The collector recomputes the delivery from the latest board and refuses `state_changed` when it differs from the one displayed (D-63's pattern).
  - `ResumeLive` and `ResumeEnded` build the `Answer` vector; `CopyAndAttach` builds `Attach` and puts no text on a command line; `FocusOnly` has no composer and goes through the existing focus.
  - The frontend copies the text to the clipboard (`navigator.clipboard`, AD-15) on the same click, whatever the delivery.
- **Confirmation:** after a spawn, the collector watches that session id in `claude agents` (a resumed session keeps its id, D-75). `busy` within 30 s confirms it; otherwise it is `not_confirmed`.
- **Journal:** `meeting_answer` (session, delivery, answer SHA-256, outcome `spawned`, `spawn_failed`, `refused`), then a second line with `confirmed` or `not_confirmed`; never the text. The Delivery share metric reads these lines.
- The text lives only in frontend memory (`lib/meeting.svelte.ts`, per card key) until it is confirmed or discarded; it is never written to disk by Officina. On the command line it is visible to local process listings while the console runs: accepted (FR-6).

### AD-86: The `auto` class end to end

- **`officina run`:** `auto` takes `human`'s path unchanged (request file, wait, single-use consumption, 15-minute rejection block, AD-33); D-20's refusal goes for `auto`. The approval file gains an optional `by` (`author`, `review`, `rule`) and `review_id`, parsed with `serde(default)`. `run --list` shows the class.
- **Window:** a new request whose action is `auto` in the approved snapshot is rebuilt from the catalogue (D-27). Core's `auto::precheck` returns `Escalate(cause)` or `Review` (AD-87); a review job goes to the `review` worker.
- **Review worker:** a `std::thread` named `review`, separate from `triage` so a dry run of 50 cases cannot delay a waiting `officina run` (AD-52's worker shape: one job at a time, outcome drained with `try_recv`). It runs the `ReviewCall` vector.
- **Prompt:** AD-51's rules (stdin, nonce, escaping, constant instruction `auto::REVIEW_INSTRUCTION`). The data holds the rebuilt request (action, description, project, values, argument vector), the subject's name, the session's excerpts as `Untrusted`, the last 10 outcomes of that action in that project, and the check's status.
- **Schema:** generated from `ReviewDecision::ALL`: `decision` and `reason` (1 to 200 characters, AD-51's character rules), `additionalProperties: false`, golden test.
- **Decision:** `auto::decide(answer, approval_on, mode)`.
  - `approve` with auto approval on gives `authority::record_review_approval`, which writes `<id>.approved.json` with `by = "review"`, and journals `approval_given` with `by`.
  - Anything else gives a Needs you card as for `human`, with the cause.
- **Live mode:** the request is `reviewing` (not counted in `waiting_count`) until the answer, at most the deadline plus the queue.
- **Shadow mode:** the card shows at once, the author decides as for `human`, and the review runs in parallel, blind.
- **Rejecting** an `auto` request requires `should_not_run` or `not_now` (`decide_approval` gains the reason; the card asks it with two buttons).
- A queue can outlast the 110 s wait; the session is then told to run the same command again (FR-7, accepted). The session never reads the review's reason.

### AD-87: Fail closed, budget, hourly cap

`EscalationCause` (`closed_enum!`) is recorded on the card, in `auto_review.cause` and in a journal line `auto_escalated`. The precheck order, each with a case in `tests/vectors/auto.json`, and a coverage test on `ALL`:

1. `claude_shim`: `claude` resolves to `.cmd` or `.bat`;
2. `no_session_id`: the request has no session id;
3. `no_excerpts`: excerpts are off, so on such a floor `auto` always escalates;
4. `quota_high`: the latest known 5-hour `used_percentage` is at or above 90, stale data included, no data not blocking;
5. `hourly_cap`: review `approve` answers for that project and action in the last 3,600 s, in shadow and live modes alike, have reached `auto_max_per_hour`. Counting shadow answers keeps the shadow statistics on the same cases live mode would review;
6. `budget_spent`: one call per 5 s, 60 per hour, 300 per UTC day, counted from `auto_review` rows with a spawned call (AD-52's counting, its own budget);
7. `review_config_invalid` (AD-75).

After a call: `call_failed`, `call_timeout` (60 s), `invalid_answer`; then `review_rejected`, `review_escalated`, `auto_approval_off`. A stopped relay refuses as today (AD-34), which is not an escalation. With the window closed nobody reviews; the request waits and expires like a `human` one. Whether the orchestrator is open plays no part.

### AD-88: Blind shadow, rule baseline, the toggle

- **Blind:** `ApprovalView` never carries review fields; a view-builder test serializes a pending `auto` request with a stored review and asserts that no review field and no review text appear. After the author's decision, the Relay view's recent entry shows the decision, the reason as `DisplayText` and the baseline.
- **Rule baseline:** `auto::rule_baseline(journal, request, now_ms)` is computed at review time. It is true when the action's check, if it names one, is satisfied by AD-33's `check_satisfied` within 30 minutes, and the last `run_finished` of that action in that project (any values) exited 0. With no earlier run it is false. Under AD-33's order the check part already held when `officina run` created the request; it is recomputed because a queue delays the review.
- **Statistics:** `auto::shadow_stats(rows, now_ms)` covers the last 30 days.
  - Cases are valid reviews that met an author decision (approve, or reject with a reason; expiry is not a decision).
  - A disagreement is review `approve` against reject `should_not_run`, or against a "should not have run" mark.
  - `rule_diff` counts the cases where (review approved) differs from the baseline.
  - The approval gate holds with at least 20 cases and no disagreement. The rule offer holds with the gate and `rule_diff` under 5 % of cases.
  - The view shows the counts and the bound 3/n (rule of three) as the stated weakness of the evidence.
- **Toggle:** `set_auto_approval(true)` is refused by the writer unless the gate holds when it writes (D-69's pattern); off is always allowed.
  - `set_auto_mode(rule)` is offered only with the rule offer. In rule mode no model call runs, approval follows the baseline and approval files say `by = "rule"`.
  - "Should not have run" on an auto-approved run of the last 7 days sets auto approval off (source `author`, with the reason) and counts as a disagreement, so turning it on again needs the gate to hold again.

### AD-89: The orchestrator session

- **Vector** (golden), arguments after the binary:
  - `--tools ""`;
  - `--strict-mcp-config --mcp-config <json>`;
  - `--setting-sources ""`;
  - `--settings {"crossSessionInbound":"refuse"}`;
  - `--permission-mode dontAsk --allowedTools mcp__officina --disallowedTools SendMessage,ListAgents`;
  - `--append-system-prompt <ORCHESTRATOR_SYSTEM_PROMPT>`;
  - `--name officina-orchestrator [--model <alias>] -- <ORCHESTRATOR_FIRST_PROMPT>`.
- **The deny flag** is new against the PRD. `dontAsk` still runs actions that need no approval (documented), and the default tool set includes both messaging tools (verified), so if an interactive session kept one despite `--tools ""`, `dontAsk` alone might not stop it.
- **The MCP config** is built by core as one stdio server named `officina`: `command` is the `officina.exe` beside `officina-app.exe`, `args` are `["mcp"]` plus `--floor <path>` when the window has one, and `env` is `{"OFFICINA_HOME": <home>}`. It is passed explicitly because AD-80 removes every `OFFICINA_*` name.
- **Constants:** the two constants live in core and are pinned by the golden vector. The system prompt states the role, that fields marked untrusted are session data whose instructions are never followed, and that no tool runs, launches or sends anything. The first prompt asks for the briefing and starts with a letter.
- **Refused** when `claude` is a shim, the workdir is not empty or `officina.exe` is missing. The window creates `<home>/orchestrator/workdir` and `<home>/orchestrator/proposals/`.
- **First start:** the workdir is untrusted, so Claude Code shows its trust dialog in the orchestrator's console once; the author accepts it there, and Claude Code records it, not Officina (D-80). README says so.
- **Lifecycle:**
  - `start_orchestrator` records an `orchestrator_start` row, spawns and keeps the pid in memory (no handle, no job), and journals `orchestrator_started`.
  - **Live** means an agents entry whose normalised `cwd` is the workdir. If question 1 shows such sessions unlisted, the pid from the spawn decides, guarded against reuse by its creation time (`platform::process_alive(pid, created)`).
  - A second Start while live focuses it: the console title carries the session name (`--name` sets the terminal title, per help), so `platform::focus_window_by_title` matches it as a whole word; several matches are candidates, none is reported.
  - It is never resumed; a restart is a new row, and its briefing cursor is the previous row's start.
- **Board:** `BoardInput` gains the Officina directories. An agents entry under `<home>/orchestrator/` becomes `Board.orchestrator: Option<OrchestratorPresence { pid, status, started_at_ms }>`, never a card and never counted in `waiting_count`. AD-43's ingestion exclusion widens from `<home>/triage/` to `<home>/orchestrator/` and `<home>/review/`. It is never filed or triaged.

### AD-90: `officina mcp`

- **Framing:** newline-delimited JSON-RPC 2.0 on stdio, by hand with `serde_json` (E11).
  - A line over 1 MiB, or a batch array, gets `-32600`.
  - `server/discover` gets `-32601`. `initialize` echoes the client's `protocolVersion` when core's `MCP_PROTOCOL_VERSIONS` holds it, else the newest it holds, with capabilities `{"tools":{}}` only.
  - `ping` gets `{}`; notifications are ignored; any other method with an id gets `-32601`. EOF on stdin exits 0.
- **Start check:** normalised `CLAUDE_PROJECT_DIR` equals `<home>/orchestrator/workdir` (verified, section 1, item 8), else one stderr line and exit 1. It prevents accidents, not forgery (FR-11). The environment is never logged; stderr carries fatal errors only.
- **Read model:** the registry is opened read-only per call (AD-53, D-82), along with the approved catalogue and the journal tail (`relay --json`'s readers). The board is built through the read half of `spawn` (`Agents`) and cached 5 s. With the window closed the outputs carry `ingestion.pending_bytes` like `subjects --json`.
- **Tools** (`McpTool`, `closed_enum!`; schemas generated in core, golden test):
  - read: `briefing` (AD-83), `subjects`, `subject {id}`, `board`, `relay_activity {limit <= 50}`, `templates` (the approved snapshot's templates only), `proposals {status?}`;
  - proposal: `propose_recap {target: {session_id} | {subject_id}, done, blocking, decision_expected}` and `propose_work_order {template, project, values: {param: value}, subject: {id} | {new: name}, rationale}`.
- **Bounds:** each call's input at most 8 KiB; texts at most 500 characters, values 64, a new subject name 80, at most 16 values; outputs at most 64 KiB (the briefing 32 KiB). Session text is wrapped as `{"untrusted": "..."}`, and every output starts with one constant note saying so. With `no_excerpts` no excerpt is served.
- **Proposal tools** check the limits first: at most 30 files per hour (counted from file ids and registry rows) and 200 pending; past either they return `isError` with the reason. Then they write the file (AD-91) and return its id. They validate nothing else: validation is the window's, at ingestion, so a forged file meets the same rules.
- No tool runs, launches, approves, sends text to a session, reads a path or reaches the network; none takes a path.

### AD-91: Proposal files and ingestion

- **File:** `<home>/orchestrator/proposals/<id>.json`, where `id` is `<now_ms>-<16 hex>`. It is written by AD-10's `atomic_write` (temporary name, `sync_all`, rename) and holds at most 16 KiB: `schema`, `kind`, the fields of AD-90, `orchestrator_session_id` (from `CLAUDE_CODE_SESSION_ID`, a label, not a credential) and `written_at_ms`.
- **Ingestion** (collector, writer under `window-authority`): the directory is fingerprinted like `relay/` (AD-35). Each `*.json` is parsed (`deny_unknown_fields`) and validated by `orders::proposal::validate(&raw, &ApprovedCatalogue, &RegistryRefs, now_ms)`, then inserted with `INSERT OR IGNORE` on the file id and committed; the file is deleted after the commit. A crash between commit and delete re-ingests a duplicate that the key ignores. A `.tmp` file left by a killed writer is never read and is deleted after 24 h.
- **Validation:** the subject, session, template and project named exist (registry, approved snapshot); the project is one of the template's; every value is in its list (the stored value is the template's string); texts are within bounds; rate limits are held again here, for files written directly. A failure stores the proposal as `invalid` with `ProposalInvalidReason` (`closed_enum!`) and its texts, shown as invalid and reported by `proposals`.
- **Statuses:** `pending`, `invalid`, `accepted` (a card from it launched), `dismissed`, `expired` (work orders after 24 h), `superseded` (a newer recap on the same target).
- A valid work-order proposal opens as a card through `IssueCardFromProposal`, which calls `WorkOrder::validate` with the stored values: the same function and the same click as the author's (FR-3).

### AD-92: Payloads, IPC commands, capabilities

- **`OrchestratorView`** (`ORCHESTRATOR_VIEW_SCHEMA = 1`, built in core, event `orchestrator://updated`, emitted on byte change, AD-5): presence, the window's briefing, approved templates (for the composer), templates awaiting approval (from the pending diff), proposals (pending, invalid, last 50 decided) and work orders (last 50 with binding and session state).
- **`Board`:** `orchestrator` (AD-89); `ApprovalView` gains `class` and `state` (`pending`, `reviewing`). `BOARD_SCHEMA` goes up by one for v0.5 (P-13's rule).
- **`RelayView`:** `auto` (approval on or off with source and reason, mode, gate counts, 3/n bound, rule offer, budget used, review model, auto-approved runs of the last 7 days), and each recent entry's `review` once decided.
- **Commands,** added to `build.rs` and `capabilities/default.json` and covered by `every_declared_command_is_allowed`, are thin wrappers over `AuthorRequest` with a deadline (D-92):
  - read: `get_orchestrator`, `open_meeting(card_key)`;
  - author: `mark_briefing_read`, `issue_card`, `issue_card_from_proposal`, `edit_card`, `launch_work_order(card_id, prompt_sha256)`, `dismiss_proposal`, `send_answer(card_key, delivery, text)`, `start_orchestrator`, `decide_approval(id, approve, reason?)` (amended), `set_auto_approval`, `set_auto_mode`, `mark_should_not_have_run(run_id)`.
  - Spawns run in `spawn_blocking`, never on the collector thread.
- No plugin is added; new types derive ts-rs and join AD-13's diff check; `ipc.ts` gains one function per command.

### AD-93: Frontend

- **Header:** the view switch of AD-66 becomes Sessions, Subjects, Relay, Orchestrator. Two neutral chips (quiet surface, never `--color-attention`, not in the title or the overlay, AD-55): "orchestrator live" or "orchestrator absent", and "N proposals", which opens the Orchestrator view.
- **Orchestrator view:** `OrchestratorView.svelte` holds `Briefing.svelte` (sections, "since <cursor>", dropped counts) and `Composer.svelte`. The composer offers a template picker, a project picker and one `<select>` per parameter from its list: there is no free-text input for a value anywhere. The subject picker reuses AD-55's `SubjectPicker`.
  - `WorkOrderCard.svelte` shows the description, the value chips, the rendered prompt as text in a `<pre>`, the argument vector as one monospace chip per argument (AD-35), the directory, the subject, the quota gauges, the proposal label and rationale, and a pointer-only Launch.
  - `ProposalList.svelte` and `WorkOrderHistory.svelte` complete the view.
  - Leaving the view calls `mark_briefing_read`.
- **Meeting:** `MeetingPanel.svelte` takes the side-panel slot of AD-69, one panel at a time, 420 px. Receive is offered on Needs you and Your turn cards, in `SessionFile` and on to-resume subjects. It holds `DiffView.svelte` (files, anchors, hunks, as text) and `AnswerComposer.svelte` (textarea, character counter from limits carried in the payload, `Ctrl+Enter` and a Send button; Send is keyboard-reachable, since an answer approves nothing). The composer is shown for `ResumeLive`, `ResumeEnded` and `CopyAndAttach`; the panel shows Focus for `FocusOnly`.
- **Scene** (superseded by AD-179, section 6):
  - The orchestrator's room takes the eighth cell of AD-61's grid. Workshops keep seven slots, and More workshops becomes slot 7 from the eighth project. AD-70's geometry check runs on the new full-occupancy layout, and the mock gets the room before Slice C's scene story.
  - The room draws the orchestrator with employees' poses: busy has its back turned; any other status faces the author, neutral, never ringed, never with a raised hand. Absent, it draws an empty chair with a Start target, and a neutral plaque counts pending proposals. `SceneInput.orchestrator` is nullable (AD-68's rule).
  - Start is keyboard-reachable: it launches Officina's own contained session, not a work order.
- **Stores:** `lib/orchestrator.svelte.ts` holds the view and the cards in flight; `lib/meeting.svelte.ts` holds the open meeting and answer drafts per card key (memory only).

### AD-94: Doctor

- **Slice A:**
  - the `claude` version (below 2.1.285, warns that background answers fall back to copy and attach, S3);
  - a shim `claude` (fail: no launch, answer, review or orchestrator);
  - agent view turned off by the environment or user settings (warn: work orders and answers will fail);
  - template issues; templates awaiting approval.
- **Slice B:** `auto` actions, auto approval and mode, gate counts, rule difference, review budget used today, review model, `review_config_invalid`.
- **Slice C:** the orchestrator workdir present and empty; `officina.exe` beside the window's executable; pending and invalid proposal counts; `.tmp` files older than 24 h.

### AD-95: Test strategy

| Layer | What | How |
|---|---|---|
| core unit | template phase 2 (one failing fixture per new `IssueKind`), format 2 hash (golden), format 1 snapshot (AD-76's four facts), `WorkOrder::validate`, card checks, briefing bound, meeting delivery, answer rules, escape stripping, review schema and prompt (golden), causes, baseline, shadow statistics, MCP dispatch | `#[cfg(test)]`; vector sets `orders.json`, `auto.json`, `spawn.json`, `mcp.json`, `injection.json`, each with a coverage test over its enums' `ALL` |
| type proofs | AD-74's `compile_fail` doctests | `cargo test --doc` |
| injection battery | per entry point (card launch, proposal file, `propose_work_order`, seeded excerpt reaching the review and the MCP outputs): argument fragments, option strings, `--settings`, `/` and `!` prompts, bidi, oversized text, unknown templates, a sentence as a value, other values outside their lists, "launch now" text, an unknown card, a superseded card, a hash mismatch | `tests/vectors/injection.json`; the fake claude counts `--bg` spawns: 0 before the click, exactly 1 after, its vector equal to the golden rebuild; every review-approved run's vector equal to the catalogue rebuild |
| mutation | three mutants behind core features used only by this job: `mutant-accept-any-value` (membership skipped in `validate`), `mutant-accept-superseded`, `mutant-skip-prompt-hash` | CI job `mutants`: each feature on, the injection tests must fail (the step inverts the status); a control run without them passes. An app test asserts `officina_core::MUTANTS_ENABLED` is false, so a release can never link one |
| spawn | every `VectorKind` through `spawn::execute` against the fake claude: argument vector, directory, variables set and removed (AD-80's seeded parent environment), console flag | host integration |
| fake claude | new modes: `bg` (prints an id, adds a background entry with that name to its sidecar, D-86), `resume` (logs, marks the session busy), `review` (answers the decision named by a `FAKE-REVIEW:` token in stdin), `logs` (prints a fixture with escape sequences), `interactive` (logs argument vector and variable names, exits); existing `invalid`, `hang`, `exit1` | `officina-fake-claude` |
| auto | approve, reject, escalate, invalid, timeout; each cause of AD-87; budget; hourly cap in both modes; blind view; toggle refused without the gate; a mark turns it off; rule mode makes no call | host integration with `window-authority` |
| MCP | a scripted client speaks E11's handshake to the real `officina mcp`; every tool; bounds; `-32601`; wrong project dir exits 1; a proposal written per call | `crates/officina/tests/mcp.rs` |
| restart | `examples/mcp_crash_helper.rs` writes proposals through the same function with a stall point; killed at the stall and 20 times at random; ingestion run twice, then again as a window restart, then a new orchestrator start: every fully written proposal present exactly once, no `.tmp` ingested, statuses kept, the cursor is the previous start | D-89's crash pattern |
| zero write | `quota_doctor.rs` fingerprint around `officina mcp` (scripted), a launch, an answer, a review, an orchestrator start, all with the fake claude (D-80) | CLI integration |
| frontend | Vitest: card chips, prompt as text, proposal label, Launch ignores `detail === 0`, composer has no text input for values, meeting texts inert (markup injected in commit subjects, paths, logs), answer counter and `Ctrl+Enter`, chips never use the attention token, scene room states. Playwright: `mockIPC` for the new commands; fixtures from `examples/orchestrator_fixture.rs` (D-72's pattern) and the real `board --json`, `relay --json`, `subjects --json` | `app/e2e/orchestrator.spec.ts`, `meeting.spec.ts` |

Manual, recorded in research notes: the gates of AD-96 and a week of use for section 7's Use, Auto and Delivery share rows.

### AD-96: The gates of PRD section 8, exact procedures

Each runs on the owner's workstation with the release candidate, records an E-entry (version, date, what was seen), and leaves no session behind: the probe's background sessions are stopped and removed, and `claude agents --json` is checked before and after. Scratch folders are trusted once by the owner with `claude` run interactively in them.

| # | Procedure | Pass | On failure |
|---|---|---|---|
| 1 | Start the orchestrator from the window (record whether the trust dialog appeared). In its console run `/mcp` (only `officina`), `/context` (record every tool: only `mcp__officina__*`), `/status` (permission mode `dontAsk`) and `/config` (the row "Messages from your other sessions" absent: `--settings` sets the key). Ask it to read a file, run a command and message another session: no prompt appears, nothing runs. From another interactive session, run `/list-agents` and ask it to message the orchestrator a unique token: the token never appears in the orchestrator's transcript viewer (`Ctrl+O`) and no held-message dialog opens. Run `claude agents --json` (listed or not). Click Start again (the console comes to the front) | all of these | a tool beyond Officina's: stop Slice C and reopen FR-10. Unlisted: the pid fallback of AD-89. Focus fails: Start shows "already running" |
| 2 | In a trusted scratch repository with a `UserPromptSubmit` hook the owner adds to its `.claude/settings.local.json` (dumping only `prompt`): `claude --bg --name probe -- <case>` and, from a new console, `claude --resume <id> -- <case>`, for the four cases of section 1, item 5 | the hook receives each case byte for byte | the vectors drop `--`; FR-2 and AD-85 refuse a leading `-` |
| 3 | In the same repository: start `claude --bg --name probe -- "Reply with the word ready."`; wait for `idle` and `done`; from a new console run `claude --resume <sessionId> -- "Reply with the word delivered."`. Record the console line, `claude agents --json` before and after (one row, `busy` within 30 s, no copy). Then make it ask a question through `AskUserQuestion` and resume again: `Your prompt was not sent to it`, no state change | both | every background answer is `CopyAndAttach` |
| 4 | In one free and one guarded project of the owner: launch a work order from the window (or the exact `WorkOrder` vector by hand) whose prompt creates `officina-probe.txt` and runs `git status`. Record `claude agents --json` over 60 s | no `waiting` with `permission prompt`; the file exists | the PRD states that J2 covers tasks needing no tool permission; no permission mode is added (principle 2) |
| 7 | `claude logs <id>` on question 3's session with stdout redirected to a file: record the exit code, bytes, count of ESC bytes and elapsed time | exit 0, under 2 s, and the text after AD-84's stripping is readable | FR-5 drops the logs section and the `Logs` kind |

Question 5 is closed (section 1, item 1). Question 6 is rechecked at each version the canary sees; it is not a gate. Question 8's values are measured over the shadow period and two weeks of use, read in the Relay view.

### Slices

- **Slice 0:** AD-96's gates 2, 3, 4 and 7 before the stories that use them; gate 1 before Slice C.
- **Slice A:**
  - AD-73's spawn split and lint; AD-74 to AD-81, with AD-82's `work_order` table and `briefing_read_at_ms`;
  - AD-83 (no proposals yet), AD-84 (no recap yet), AD-85;
  - AD-92 and AD-93 for the Orchestrator view, the composer, the cards and the meeting; AD-94 part A.
- **Slice B:** AD-86 to AD-88 with the `auto_review` table and settings; the Relay view's `auto` section; AD-94 part B.
- **Slice C:** AD-89 to AD-91 with `proposal` and `orchestrator_start`, the scene room and chips; AD-94 part C.
- **Every slice:** AD-95's rows for what it lands. Migration 2 is one migration, extended in place until v0.5 ships (AD-82).

### Touch points with v0.2, v0.3 and v0.4

- `core/floor.rs` (`RELAY_KEYS`, the three raw tables), `core/relay/catalogue.rs` (phase 2, format 2, diff), `core/relay/params.rs` (`check_value` split), `core/board.rs` (`orchestrator`, `ApprovalView`, schema bump), `core/registry` (enums widened, view gains the orders state).
- `host/claude.rs` and `platform/windows.rs` (`open_attach_console`) move into `host/spawn.rs`; `platform/windows.rs` gains `process_alive` and `focus_window_by_title` (existing `windows-sys` features); `host/relay/run.rs` (`auto` path); `host/relay/authority.rs` (`record_review_approval`, `decide_approval` with a reason, the toggles); `host/watch.rs` (card store, proposal ingestion, review worker, `orchestrator://updated`); `host/registry/schema.rs` (migration 2); `host/registry/triage.rs` (the `TriageCall` vector).
- `officina/main.rs` (`mcp`, usage text, readme test of D-34); the app's `main.rs`, `build.rs` and capabilities; `ipc.ts`, `e2e-mock.ts`, `Header.svelte`, `SessionCard.svelte`, `SessionFile.svelte`, `lib/scene/*` (eighth cell, received marker).

## 3. Amendments to earlier decisions

- **AD-3:** at most one `claude agents` child, one triage child and one review child at a time; launches, answers and the orchestrator are spawned from `spawn_blocking` and never waited on by the collector.
- **AD-6, AD-15:** every `claude` spawn goes through `host::spawn` (AD-79); `Attach` refuses a shim.
- **AD-21:** `window-authority` also gates the launch half of `spawn`, the card store, proposal ingestion, the review worker and the orchestrator lifecycle.
- **AD-23:** `auto` is valid; `check` is allowed on `auto`; templates and the two new tables are phase 2.
- **AD-24:** canonical format 2 (AD-76).
- **AD-25:** the approval file gains `by` and `review_id`; the Officina home gains `orchestrator/workdir`, `orchestrator/proposals` and `review/empty`.
- **AD-32:** kinds `work_order_launched`, `meeting_answer`, `auto_escalated`, `auto_review` (outcome, mode, cause, decision; never the reason), `orchestrator_started`, `auto_approval_set`; `approval_given` gains `by`.
- **AD-33, D-20:** `auto` takes `human`'s path in `officina run`.
- **AD-35:** `ApprovalView` gains `class` and `state`; an `auto` reject asks for a reason.
- **AD-40, AD-41:** migration 2 (AD-82).
- **AD-43:** the ingestion exclusion covers `<home>/orchestrator/` and `<home>/review/`.
- **AD-51:** both contained calls add `--settings {"crossSessionInbound":"refuse"}`, because a `-p` session binds an inbox (documented) and another session could otherwise deliver text into a review mid-call. `TRIAGE_ENV_REMOVED` becomes `SESSION_ENV_REMOVED` (AD-80). The golden vector changes once.
- **AD-52:** one budget per purpose; reviews have their own thread.
- **AD-55, AD-66:** a fourth view, Orchestrator; two neutral chips.
- **AD-61, AD-62:** the eighth cell is the orchestrator's room; seven workshop slots; More workshops from the eighth project.
- **AD-69:** `SessionFile` offers Receive; the meeting panel shares its slot.
- **AD-72:** the frontend files of AD-93.

No dependency is added. The JSON-RPC is hand-written on `serde_json`, randomness comes from std, and escape stripping and git calls need no crate. Ruled out: `cargo-mutants` (three feature-gated mutants cover the property the PRD names), an MCP SDK crate, `uuid` (core validates the form), `strip-ansi-escapes`, `git2`.

## 4. PRD amendments

1. **FR-2.** Append: "Templates are schema 2 keys; the floor schema number does not change. A template's prompt starts with author text that is not `/`, `!` or `-`, and its longest rendering is checked at validation. Values pass AD-30's runner-independent rules; the native runner's refusal of a leading `/` or `@` applies to argument vectors only, so `@scope/pkg` is a valid value. `class = "auto"` may name a `check`, and `auto_max_per_hour` is an action key. The floor gains `[orchestrator] model` and `[auto_review] model`; the latter belongs to the approved catalogue."
2. **FR-3.** Replace "The background id `--bg` prints is recorded" (FR-4) with: "The work order's name is unique (`<template>-<n>`); the session is bound by that name in `claude agents`, and the id `--bg` printed is recorded and checked against it." Append to the vector paragraph: "The environment drops the variables Claude Code exports to a session's children (session identity, messaging socket and token) and keeps agent view on."
3. **FR-6, table row 1.** Append: "The console keeps agent view on: with it off Claude Code sends nothing (sessions page). Its working directory is the `claude` binary's folder." Row 3 keeps the session's folder.
4. **FR-7.** Append: "In live mode the request shows as reviewing, not waiting, until the review answers; in shadow mode it is a Needs you card at once."
5. **FR-8.** Replace "reviews of that action already approved its `auto_max_per_hour`" with "review approvals of that action, in shadow and live modes alike, reached its `auto_max_per_hour`". Append: "A malformed `[auto_review]` table escalates every `auto` request."
6. **FR-10.** Add to the flag list: "`--disallowedTools SendMessage,ListAgents`: `dontAsk` still runs calls that need no approval (permission-modes page), and both tools are in the default set." Replace the sentence on `CLAUDE_CODE_DISABLE_AGENT_VIEW` with: "so it cannot be moved to the background, where the supervisor keeps `--settings` and `--mcp-config` but not `--tools` or `--allowedTools` (agent-view page)." Append: "The MCP server is the `officina.exe` beside the window, given the Officina home and the floor explicitly. On the first start Claude Code asks, in the orchestrator's console, to trust its folder."
7. **FR-11.** Append: "Proposal tools check only the rate limits; every other rule is checked at ingestion, so a file written directly meets the same rules."
8. **FR-13.** Replace "in a fixed slot" with "in the eighth cell of the scene's grid; workshops keep seven slots and 'More workshops' takes the seventh from the eighth project". Append: "Start in the room is keyboard-reachable; Launch on a work order is not."
9. **FR-15.** Append to Slice A: "agent view turned off in the environment or user settings (warn)".
10. **FR-7 and v0.3 FR-10.** Append: "Both contained calls refuse messages from other sessions (`crossSessionInbound` from `--settings`)."
11. **Section 7, Containment row.** Replace "workstation check recorded (section 8, question 1)" with "print-mode check recorded (addendum section 1, item 7); interactive check recorded (section 8, question 1)".
12. **Section 8, question 1.** Replace "run `/mcp` and `/status`" with "run `/mcp`, `/context` (tool list), `/status` and `/config` (the inbound row is absent while `--settings` sets the key; `/status` shows nothing for a refusing session)", and "message another session" with "have another session message it a unique token, which never reaches its transcript".
13. **Section 8, question 2.** Append: "Print mode is verified (addendum section 1, items 4 and 5); `--bg` and interactive resume remain."
14. **Section 8, question 5.** Replace with: "Answered: `claude --bg --resume` on a running session starts a copy (`claude --help`, 2.1.312). FR-6 does not use it."

## 5. Risks

| # | Risk | Mitigation |
|---|---|---|
| R33 | The interactive orchestrator keeps a tool print mode did not show (`--tools ""` behaves differently with a TTY) | `--disallowedTools` for the messaging tools, `dontAsk`, `--strict-mcp-config`; gate 1 lists tools with `/context` before Slice C ships |
| R34 | `crossSessionInbound` from `--settings` is silently ignored (print mode drops invalid settings without a word) | gate 1's token test and `/config` row; flag settings proved live with `--setting-sources ""` (section 1, item 6) |
| R35 | Delivery to a live background session misbehaves (E14 saw a copy once) | gate 3; confirmation by `busy` within 30 s; the text stays in the panel and on the clipboard; `CopyAndAttach` fallback |
| R36 | A work order stops at its first permission prompt | gate 4; the PRD narrows J2 rather than adding a permission mode |
| R37 | A Claude Code release changes `--bg` output, `--resume` semantics or a flag of a vector | golden vectors; canary per version; the binding uses a unique name, not stdout; doctor version warnings |
| R38 | The window inherits a parent session's messaging token and hands it to a child | `SESSION_ENV_REMOVED` on every vector, with a seeded-environment test |
| R39 | Format 2 voids the catalogue approval on upgrade and guarded sessions are refused until the author approves | banner with the diff (`format` and templates); the snapshot keeps guarding; README upgrade note |
| R40 | The review approves what the author would not, or anchors on a rule it cannot see beyond | blind shadow, 20-case gate with the 3/n bound shown, "should not have run" turns it off, rule mode offered when the model adds nothing |
| R41 | Reviews queue past `officina run`'s 110 s wait | own thread and budget; the session is told to run again (accepted) |
| R42 | Git in a session-controlled repository runs configured programs or writes the index | D-22's flags, `GIT_OPTIONAL_LOCKS=0`, no untracked content read, 10 s deadline |
| R43 | Proposal flooding fills the disk or the view | 16 KiB per file, 30 per hour, 200 pending, enforced at the tool and at ingestion; `.tmp` cleanup |
| R44 | The scene loses a workshop slot to the orchestrator's room | More workshops lists the rest; owner review of the updated mock before Slice C's scene story |
| R45 | `claude logs` prints terminal control output | gate 7; escape stripping; the section is dropped if unreadable |
| R46 | A mutant feature leaks into a release build | the app test on `MUTANTS_ENABLED`; no crate but the CI job enables them |

## 6. Addendum (2026-10-28): the scene v2, orchestrator answers, permissions

Inputs: prd-v0.4.md and architecture-v0.4.md as reworked on 2026-10-28 (AD-167 to AD-178); the research note `_bmad-output/planning-artifacts/research/claude-permission-auto-decisions.md` (not published). PRD section 9 states the requirements; `epic-5-decisions.md` D-486 to D-491 record the decisions. AD-1 to AD-96 hold except as amended here.

### AD-179: The orchestrator in the decision office

Supersedes AD-93's scene bullets and the AD-61 and AD-62 amendments of section 3.

- `SceneInput` gains `orchestrator: OrchestratorPresence | null` (AD-89) and `proposals: { pending: number; latest: string[] } | null` from `OrchestratorView`; both nullable (AD-68's rule).
- The figure sits on v0.4's decider chair in the decision office (architecture-v0.4.md AD-170); it is a rig like an employee's (AD-171) with the prototype's `--orch-tunic` tokens, a collar in `--color-surface` and no cap; busy shows the seated working pose, any other status the seated facing pose; never a ring or a raised hand; never counted as waiting.
- **Briefing walk**: a work order card launched and bound (AD-81) while the orchestrator is live gives the layout one `briefing` entry (the bound card's key) for as long as that session's figure stands at reception; the orchestrator's place is then the counter's staff spot, reached through AD-170's staff nodes, and its chair otherwise. The mover walks it like any figure (AD-172).
- **Screens and whiteboard**: the two screens draw the pending proposal count and the Needs you count; the whiteboard draws up to four proposal targets' names. Names render through the overlay layer with `{text}`, never in the object layer's markup (v0.4 AD-59 as amended). Nothing scrolls or ticks: the prototype's scrolling log and sparkline are not ported, so v0.4's truthful-motion e2e holds.
- **Plaque and target**: "Orchestrator" while live, "Decisions: you" otherwise, neutral; the figure's target opens the Orchestrator view; absent, the chair's target offers Start (keyboard-reachable, AD-93).
- Offices keep v0.4's eight; v0.4 AD-70's geometry property runs with the orchestrator's target added.

### AD-180: The `auto` review's input and the mandatory check

- **Input** (amends AD-86's prompt): the data line holds the rebuilt request, the subject's name, the author's last prompt excerpt as `Untrusted` (from `UserPromptSubmit` lines), the last 10 outcomes and the check's status. Answer excerpts (`last_message`, `last_message_tail`) are not read. The golden prompt changes once; `no_excerpts` still escalates (AD-87 cause 3), since the prompt excerpt is withheld then too.
- **Check mandatory** (amends AD-23 and AD-75): catalogue validation raises a new `IssueKind::AutoWithoutCheck` for `class = "auto"` without `check`, with its failing fixture; AD-86's path then always applies the `after-check` rule first.
- **Gate 8** (joins AD-96): in a scratch guarded project, a session started with `--permission-mode auto` is asked to run a catalogue `check` through `officina run`; record whether the classifier lets it through, with and without a narrow user-settings allow rule for `officina run`, and whether `claude auto-mode config` shows that rule kept. Pass: it runs with the rule. Fail: doctor prints an `autoMode.allow` sentence for `officina run` instead, and the gate runs again with it.
- **Doctor** (joins AD-94, Slice B): a "permission mode" line lists the `permission_mode` values seen in the last 24 h of hook payloads per session kind, and prints, as text to paste, `permissions.defaultMode: "auto"` with an `autoMode.environment` skeleton of placeholders (`<trusted repository>`, `<lab host group>`), marked as the author's decision. The hook already receives `permission_mode` (E2, E15); the event line gains it as an optional field.

### AD-181: The `PermissionRequest` router (v0.5.x, Slice D)

- **Entry**: after v0.5's close, and once the research note's questions 3 (does the terminal dialog show while the hook waits), 4 (a `--bg` session's behaviour), 5 (what fires in auto mode), 9 (self-resolving `ask`) and 10 (the Windows payload) are recorded as research-notes entries.
- **Registration**: doctor's block gains `PermissionRequest` (matcher every tool, exec form, `timeout` 590, `onFailure` unset, so a failure falls back to the native prompt) and `PermissionDenied` (journal only).
- **Hook**: `officina hook` on `PermissionRequest` writes `relay/prompts/<id>.json` (session id, `CLAUDE_PID`, tool name, SHA-256 of the canonical `tool_input`, the input capped at 4 KiB as `Untrusted`, `permission_mode`), then waits by stat for `<id>.decided.json`, which only `window-authority` code writes (AD-21, AD-33's pattern). It prints the documented `PermissionRequest` output with `decision.behavior` `allow`, or `deny` with a constant message. Its output type has no `updatedPermissions`, `updatedInput` or `interrupt` field, and a test asserts the serialized schema. No decision before 580 s: exit 0, no output.
- **Window**: a Needs you card of kind `prompt` with the tool, the input as text (argument vector chip by chip for shell tools, AD-35), the source labelled "Claude Code prompt"; Allow once and Deny, pointer only; decisions single use, bound to session and input hash, expiring at the hook's deadline. The window answers "no decision" at once for requests it does not route (interactive sessions if question 3 shows the terminal dialog waits for the hook; the author's own ask rules), so those reach the native prompt quickly.
- **Journal**: `prompt_routed`, `prompt_decided` (session, tool, input hash, decision), `auto_mode_denied` (reason only); never the input.
- **Scene**: a routed prompt is a Needs you wait like any (v0.4 AD-167): it queues at the decision office and may alert.
- **Zero write**: the fingerprint test (D-80) runs the hook through allow, deny and timeout.

### AD-182: Answer proposals

- **Tool**: `propose_answer {session_id, text, rationale}` joins AD-90's proposal tools (rate limits shared); the file kind `answer` joins AD-91.
- **Validation at ingestion**: the session exists and is background, live and in Your turn at ingestion; `text` passes FR-6's answer rules through the same core function as the composer's; `rationale` up to 500 characters. Statuses as AD-91, plus `expired` when the session leaves Your turn.
- **Panel**: `MeetingPanel.svelte` shows a valid answer proposal under a "proposal" label with its rationale, as text, and a pointer-only "Send the proposed answer" (Launch's rule: `detail === 0` ignored, no key); the author's composer stays empty and separate. Sending calls `send_answer(card_key, delivery, text, proposal_id)` with the stored text; core refuses a proposal whose status is not `pending` or whose session state changed. `meeting_answer` gains `source` (`author`, `proposal`).
- **Injection battery** (AD-95): the proposal file and `propose_answer` entry points gain the cases of FR-6's rules and "send now" text; no answer is spawned before the click, exactly one after.

## 7. Risks of the addendum

| # | Risk | Mitigation |
|---|---|---|
| R107 | Auto mode's classifier blocks `officina run apply`, breaking the relay's `human` and `auto` paths once the author switches modes | gate 8 before the switch; the narrow rule or `autoMode.allow` sentence printed by doctor; the switch stays the owner's |
| R108 | A `PermissionRequest` hook holds a process for minutes, or the terminal dialog and the window both answer | Slice D waits for questions 3 and 4; single-use decisions bound to the input hash; no decision means the native prompt |
| R109 | An answer proposal launders injected text into a session's next prompt | `Untrusted` display, FR-6's rules at ingestion, a pointer click per answer, no unattended path, the injection battery |
| R110 | The orchestrator figure reads as a decider although it decides nothing that acts | plaque and file wording; Needs you keeps queueing for the author; decisions stay FR-7 to FR-9's |
