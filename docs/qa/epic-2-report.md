---
title: Epic 2 QA report (v0.2, The Relay)
author: Rwx-G
date: 2026-10-28
---

# Epic 2 QA report

## Executive summary

Gate decision: **pass with open items**. All fifteen stories (2.1 to 2.15)
are implemented and covered by automated tests, and the five Epic 2 audits
were answered inside the epic by stories 2.12 to 2.15. Epic 2 is closed on
code. It closes on use once the owner has run the four manual checks listed
High in `docs/backlog.md` and used the relay at work for a week (P-7): the
Delegation metric can only be measured there.

Top findings, all fixed in the epic:

1. A `git` shim first on `PATH` returned a constant tree fingerprint, so a
   session could check, edit the playbook, then run unchecked code under an
   `after-check` or an approved action: `git` now comes from the registry,
   Officina hashes file contents itself, actions get the registry's `PATH`,
   and the gate denies `PATH` and `GIT_` assignments next to `officina`
   (2.14, D-173).
2. A `human` action naming no check ran whatever its file held when the
   approval came: every `human` action in a git tree is bound to its files
   and refuses `tree_changed` after an edit, leaving the approval unused
   (2.14).
3. The Write and Edit tools could forge `<id>.approved.json` and run a
   `human` action without a click: file tools are denied under the Officina
   home once a project is guarded, the window records its own decisions,
   and `officina run` refuses an approval the record does not name
   (`approval_not_given`) (2.14).
4. The gate read `Bash` only, while Claude Code turns the PowerShell tool on
   by default on Windows: `Monitor` is lexed like `Bash`, `PowerShell` is
   refused whole in a guarded project (2.12, D-171).
5. A rename is not a single-use claim on Windows: two calls could use one
   approval. Consumption claims by exclusive create, a stress test runs in
   CI, and AD-33 carries a dated amendment (2.8, 2.15).
6. A closed argument vector does not close what ssh or Ansible does with a
   value: notices on the banner, in `relay --json` and in doctor (2.13,
   D-172), and a permissive pattern on such a value drops the action unless
   it sets `forwarded_values = "accepted"` (2.14).

## Evidence

Figures at the close of story 2.15, from local gates replaying `ci.yml`.

| Gate | Result |
|---|---|
| Rust unit, integration, CLI end to end, vector sets (argv, gate, url, classification), TS bindings drift | 611 passed, 6 ignored (the release-only perf and stress tests below) |
| Perf, release (hook, gate own cost, gate in process, two append stresses) | 5 passed; gate own cost 3.0 ms (budget 5 ms; CI asserts a looser bound on the hosted runner, D-19), in-process p95 2.5 ms (bound 3 ms, a workstation check), hook own cost 12.9 ms (budget 30 ms) |
| Journal appends, 16 processes x 200 lines over 4096 bytes | 0 corrupt |
| Approvals stress, two consumers of one approval | exactly one wins |
| Frontend component tests (Vitest) | 35 passed |
| Playwright on fixtures from the real CLI, served under the window's CSP | 50 passed |
| `core-linux` set in a Linux container | clippy clean, 510 passed |
| WSL on the workstation | every accepted `wsl` case of `argv.json` arrives byte for byte; `--cd` on an accented, mixed-case path with spaces (E13) |
| Zero write under the Claude config dir | fingerprint identical after a gate deny, a PowerShell deny, `run`, `run --list`, `relay --json`, an HTTP run and doctor |
| Shipped `officina.exe` built alone | no authority marker, no test seam, no `webpki-roots`; the marker is found in `officina-app.exe` (positive control) |
| clippy `-D warnings`, rustfmt, svelte-check, ESLint 10, `cargo deny`, `pnpm audit` | clean; one low dev-only advisory with no published fix allow-listed with its reason |
| Toolchain | Rust 1.99.0, every cargo command `--locked` |
| Real window with the relay | not started by any story; its commands are thin wrappers over the tested authority, and a test holds their declarations together |
| CI on GitHub | green through 2.13; red on 2.14 (a Linux-only clippy lint, 8.3 short paths on the hosted runner), both fixed during 2.15; the last push was still running at the time of writing |

## Success metrics (PRD section 7)

- **Delegation** (a guarded subject run start to finish without pasting
  anything): **not yet measured**. The mechanism is proved:
  `run.rs::check_then_apply_runs` and the five `after-check` refusals
  (2.6 AC 4), the WSL argument and `--cd` runs (2.5, 2.10, E13). The metric
  is the author's log at work: story 2.10 AC 10, backlog High.
- **Containment** (100 % of raw guarded programs refused): **met on the
  obvious path**. `gate.rs::every_default_guarded_program_is_denied` sends
  every entry of `DEFAULT_GUARDED_PROGRAMS` through the real binary on
  E15-shaped payloads (2.4 AC 3); `gate.json` with a coverage test
  iterating the lexer's constants (2.3); Monitor and PowerShell (2.12);
  file tools under the home (2.14). What a text match cannot catch stays a
  `limit` case (AD-27). Manual: a deny in an interactive bypass session
  (2.4 AC 18) and a PowerShell deny in one (2.12 AC 6), backlog High; E12
  and E15 were `claude -p` and background sessions.
- **Closed parameters** (100 % of injection attempts refused before a
  process starts): **met**. `run.rs::every_argv_injection_is_refused_before_launch`
  replays every refusal of `argv.json` and the fake tool's launch log stays
  absent (2.5 AC 9); `url.json` does the same for HTTP (2.11). The promise
  is local: what the program does with an accepted value is D-172's notices
  and the `forwarded_pattern` issue (2.13, 2.14). Nothing manual.
- **Journal coverage** (amendment 11: one start line and at most one end
  line per run, exactly one line per refusal and decision): **met**.
  `run_leaves_one_start_and_one_finish_line`,
  `every_refusal_code_exits_126_with_one_journal_line` (exhaustive over
  `RefusalReason::ALL`), `every_request_and_decision_leaves_exactly_one_line`,
  `deny_is_journaled_once`, `approval_is_journaled_once_with_previous_hash`,
  `stopping_twice_journals_once`, concurrent runs and the append stress.
  Two refusals leave no line by design (journal lock held, no Officina
  home): exit 126 and the stderr line (2.5). Nothing manual.
- **Approval round trip** (click to the retried action running in under
  2 s): **met in test**. `approvals.rs::approve_while_waiting_runs_once_under_2_s`
  approves through the window's authority while the call waits (2.8 AC 5).
  Not measured through the real window, which no story started: part of
  the week of use.

## Audits

- **Requirement challenge** (before the PRD): HTTP answered 2 to 5 % of
  the measured pain, so the PRD was rewritten around delegation per
  repository; HTTP stayed secondary (AD-36) and was built inside v0.2 as
  story 2.11 (P-7).
- **Story review**: P-1 to P-7 and D-1 to D-41 settled on 2026-10-27;
  every story reworked before implementation.
- **Security audit** (2.11 read for secret handling only): 0 critical,
  0 high, 2 medium, 4 low, 11 info categories. Medium, both fixed by 2.14
  (D-173): `git` resolved through `PATH` (AC 1, 3, 4); clean filters and a
  restored stat escaping the fingerprint (AC 2, contents hashed by
  Officina). Low: `'unsafe-inline'` styles dropped from the CSP (AC 10);
  the session id stated as advisory (AC 11); Monitor's field confirmed as
  `command`; the gate's fail-open on an internal fault accepted (AD-28),
  journaled and shown by the Relay view and doctor.
- **Penetration audit** (static, 2.11 and 2.13 as forward items): 3 high,
  3 medium, 3 low, 1 info. High, all fixed: the `PATH` shim (2.14 AC 1, 3,
  4); a `human` action without a check left unbound (AC 5); forwarded
  values (2.13 notices, 2.14 AC 9). Medium: relay files writable through
  Write and Edit, fixed (AC 6, AC 7); path aliases (8.3 names, junctions,
  `subst`), fixed (AC 8, then the guarded roots resolved by handle after
  the CI failure); the pointer-only rule enforced in the page only,
  accepted: it guards against a stray key, the CSP and text-only rendering
  keep code out of the page, and the README says so. Low, accepted until
  v0.6: a spoofable session id (documented), a journal that is not
  tamper-evident (hash chain deferred, PRD section 5), the author's
  credentials readable by the author's session.
- **Quality audit**: 4 high, 11 medium, 5 low, 2 info. High, all addressed
  by 2.15 (D-174) as targeted refactors with no behaviour change:
  `run.rs` split by concern with one ending and an injectable stderr
  (AC 1), the lexer's state split from its analysis (AC 2), the catalogue
  validators one rule each (AC 3), shared test support (AC 5). Medium
  fixed: wire names by a generated `code()` (AC 4), the collector's seams
  and its known-projects write error reported (AC 6), outcome tone in
  core, typed IPC errors and the approval card on its own props (AC 7).
  Six Medium findings are open, wholly or in part, with no recorded
  reason (checked in the code at the time of writing): `parse_args` and
  `ArgTemplate::parse` return `Result<_, String>`; `decide` and
  `decide_call` take two positional lists and a bool; `CREATE_NO_WINDOW`
  is defined in four files; the contract field `gate_errors_24h` names its
  window; `read_known_projects` merges missing and unreadable into `None`
  and `read_stdin` lacks its why; the frontend's timers have no handle and
  some durations are bare literals.
- **Debt audit**: 2 high, 4 medium, 9 low, 2 info. High, both fixed by
  2.15 AC 8: the README's Privacy section claimed no network connection
  (now scoped, with a `readme.rs` case), and five places described
  consumption as a rename (now the claim, AD-33 amended). Medium:
  `GateFault::Io` removed, the unreachable "class not available" texts
  removed, the invalid-pattern gate test rewritten and mutation-checked;
  the fourth, `_bmad-output/` not ignored, was wrong (its own `.gitignore`
  ignores everything under it). Low: four fixed (`Lexed.trace` test-only,
  story tasks ticked, deleted test names marked, e2e outcomes asserted
  against fixture literals); five open (checked): story 2.4 cites a backlog
  Medium that does not exist and its AC 14 describes a `Bash`-only
  matcher; six doc comments say the gate decides a `Bash` command; the CI
  perf step is titled "Hook latency and concurrent appends"; the zero-write
  test asserts only that each command exited.
- **Dependency audit**: 0 critical, 0 high, 6 medium, 10 low, 3 info; no
  resolved version matched an advisory. Medium, five fixed by 2.15 (AC 9,
  10): `_ring` replaced by a direct `rustls` dependency with `ring`; Rust
  1.94.0 moved to 1.99.0 (1.99.1 was 13 days old and fixes nothing
  Officina uses); a weekly audit, with `pnpm audit` over dev dependencies;
  `--locked` on every cargo command; ESLint 10. The sixth, a third-party
  notices file in the release artefacts, is in the backlog (High) for the
  first published release. Low: `webpki-roots` banned in `deny.toml`; the
  exact `ureq` pin kept on purpose (unstable feature names); the rest are
  watch items or belong to later work (rusqlite at v0.3, release cache and
  provenance before v1).

## Decisions that changed the PRD or the architecture

- **D-170** (2.8, from the v0.8 requirement challenge): a `human` action
  may name a dry run; its request needs it and records its run and tree,
  the card shows the kept output (256 KiB, 7 days, `relay/outputs/`), and
  the tree is recomputed before the approval is used. Extends FR-8 and
  FR-9.
- **D-171** (2.12, found by the v1 PRD draft): the gate covers every shell
  tool from one constant, `PowerShell` refused whole in a guarded project.
  Replaces AD-26 step 1 ("`tool_name != Bash`: no decision") and D-18's
  `Bash` matcher.
- **D-172** (2.13, from the Epic 8 story review): non-blocking notices for
  values ssh or Ansible reads; approval journal lines name their project
  and action. Corrects the README's "no shell ever reads it"; extends
  AD-32.
- **D-173** (2.14, from the security and penetration audits): `git` from
  the registry and a content fingerprint replace AD-33's `HEAD` plus diff
  and D-22's by-size rule; actions run with the registry's `PATH`; every
  `human` action is bound to its tree, beyond amendment 8; file tools are
  denied under the Officina home, reversing AD-26's `Bash`-only choice for
  that path; folders resolved by handle before the gate (AD-26 step 2); a
  decision record and anomalies, with two files beyond AD-25's table
  (`relay/decisions.json`, `relay/final-roots.json`); `forwarded_values`
  opt-in and catalogue format 3.
- **D-174** (2.15, from the quality, debt and dependency audits): the
  findings fixed inside v0.2 (owner rule: no debt carried across a
  version); third-party notices wait for the first release.
- **AD-33 amendment** (2026-10-28, 2.15): consumption claims with
  `create_new` on `<id>.consumed.json`, then removes the approved name;
  step 2's rename was not single-use on Windows (`approvals_stress.rs`,
  2.8).
- Also during the epic: D-19's baseline moved to the same binary answering
  another tool (2.4), since a smaller program charged the gate with
  loading `officina.exe`; P-7, set before the stories, built FR-5 and
  FR-12 in v0.2 instead of v0.2.x.

## Known limits

The relay is a guardrail against mistakes and injected instructions and a
record, not a security boundary (PRD section 1). The boundary is the
isolated workshop (v0.6).

- A session runs as the author: it can write any relay file by a route the
  gate does not see (AD-25), the decision record included, so the record
  stops cheap forgeries, not a session that also rewrites it. The journal
  is not tamper-evident until the hash chain (v0.6). The author's
  credentials are readable by the author's session.
- The gate is a text match: a guarded program in a script file, started by
  another tool, renamed or aliased by an earlier command, a `cd` from a
  free folder, a command naming the relay through an alias. Each stays a
  `limit` case of `gate.json` (AD-27). A fault inside the gate gives no
  decision (AD-28) and is journaled.
- The file binding skips what git ignores and programs, collections and
  roles outside the tree; outside git only the parameters bind. A per-user
  Git for Windows install and the user `Path` are the user's to write. A
  junction retargeted after its roots were resolved is followed only when
  the floor's roots change.
- The `forwarded_pattern` probe is not a proof, `forwarded_values =
  "accepted"` is the author's word, and a short option glued to its value
  (`-e{x}`) gives no notice. An Ansible `--check` is harmless only if no
  task it reaches sets `check_mode: false` or delegates (D-170).
- The session id on a card is advisory; the pointer-only rule guards
  against slips, not against code in the page.
- `--wait` plus `timeout_s` can outlive the Bash tool's 10-minute cap
  (P-6); doctor warns.

## Open items

Owner's checks, listed High in `docs/backlog.md`, each recorded in
`docs/research-notes.md`:

1. Story 2.10 AC 10: the Delegation metric, a WSL `ansible-playbook` check
   then apply on a lab group, run by a session start to finish.
2. Story 2.4 AC 18: a raw `ssh` denied in an interactive
   `bypassPermissions` session.
3. Story 2.12 AC 6: a PowerShell call denied in an interactive session
   with the PowerShell tool on.
4. Story 2.11 AC 18: an HTTP action with a stored credential against a
   real `https` endpoint whose CA only the Windows store trusts.

Also in the backlog, a release condition rather than a close condition:
the third-party notices file (2.15 AC 11).

Found by this report, not in the backlog:

- The six quality Medium findings and five debt Low findings left open
  above. D-174 rules out carrying them into v0.3: fix them in a short
  story before 3.1 (recommended; signatures, one shared constant and one
  field rename with its TypeScript binding), or record each in the backlog
  with its reason.
- `CHANGELOG.md` `[Unreleased]` keeps text that 2.14 superseded: the
  `after-check` entry describes the fingerprint as `HEAD`, the diff and
  untracked files bound by size past 1 MiB; the doctor entry says its
  matcher names "the shell tools only", while it names the file tools too;
  the canonical format entry says 2, the Security entry 3. Story 2.10 AC 8
  asked for one coherent set: fix before the v0.2 cut.
- `docs/stories/sprint-status.yaml`: the Epic 2 header comment still reads
  "implementation not started", and stories 2.4, 2.10, 2.11 and 2.12 read
  `review` with a manual check open, which Epic 1's convention marks
  `in-progress`.
- No audit ran after 2.14 and 2.15, and the security pass read 2.11 for
  secret handling only; the fixes rest on their stories' tests and
  adversarial passes. The security audit's follow-ups (a lexer fuzzing
  pass against real Git Bash, a defensive pass over 2.11's URL builder and
  credential binding) are not done: recommended during the week of use.

## Verdict

Epic 2 is **closed on code**: fifteen stories in review, every acceptance
criterion proved by a named test except the four manual checks, and every
High and Medium audit finding fixed or accepted with its reason, except
the six quality Medium findings above. It is **closed on use** after the
four owner checks, a week of real use at work that measures the Delegation
metric and approves a request through the real window, and a green CI on
the last push (P-7). v0.3 starts once those hold and the open quality and
debt items are fixed or recorded (D-174).
