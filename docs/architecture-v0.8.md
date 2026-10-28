---
title: "Architecture addendum: Officina v0.8 - The Workstation and Production"
author: Rwx-G
date: 2026-10-28
status: approved 2026-10-28 (owner delegated design decisions; gates open)
inputs:
  - prd/prd-v0.8.md (revised after the requirement challenge)
  - (requirement challenge, not published)
  - architecture-v0.7.md (cited by AD number)
  - architecture-spine.md
  - architecture-v0.2.md
  - architecture-v0.3.md
  - architecture-v0.4.md
  - architecture-v0.5.md
  - architecture-v0.6.md
  - research-notes.md
  - stories/epic-2-decisions.md
  - stories/epic-5-decisions.md
  - stories/epic-6-decisions.md
  - stories/story-2.8-approvals-and-class-human.md
  - (the code: crates/officina-core/src/relay/catalogue.rs, classes.rs, approvals.rs, run.rs, journal.rs, view.rs; crates/officina-host/src/relay/run.rs, tree.rs; story 2.8 in progress)
  - (WSL on the workstation, probed read-only, 2026-10-28)
---

# Architecture addendum: Officina v0.8

AD-1 to AD-149 stay in force except where section 3 amends them; this addendum adds AD-150 onward and risks from R83. **Verified** marks a fact observed on the workstation for this addendum (section 1). **Gate** marks a fact still to check, with the procedure of AD-166, before the dependent story closes. The v0.7 addendum is a draft; v0.8 consumes none of its code, and names its ADs only where a run the governor launched meets the relay.

One property organises everything below: **what runs on the author's side of the boundary is decided in core from the author's declarations and the run's own record, never from what a worker, a session or an inventory says about itself.** A worker's request is untrusted bytes on a channel that names its run by construction (a network and a pipe that belong to that run alone): nothing in it chooses the run, the project, the catalogue or the folder; it can only pick, among the read actions the author listed for its template, one to run in the author's project folder. Scope is the author's declaration compared with parameter values and with the names an inventory lists; Officina infers no effect and no scope. What a resolution, a dry run or a worker returns is shown as untrusted text and never parsed beyond the one stdout grammar of AD-153.

## 1. Checked on the workstation

Windows 11 Pro 10.0.26200, 2026-10-28. Read-only WSL commands only: no distribution setting, file or package changed, no network connection from WSL, no SSH key or agent touched, no Ansible run, no value of an environment variable read (names tested for presence). Docker was not probed for this addendum: gate 2 is open. No tool output carried instructions addressed to an assistant.

**Verified.**

1. **Distributions.** `wsl.exe --list --verbose`: `Debian` (default, WSL 2) and `docker-desktop`, both stopped before the probe; the kernel reported inside Debian is `6.18.33.2-microsoft-standard-WSL2`, the one v0.6 recorded.
2. **Tools in the default distribution.** `/usr/bin/env` and `/usr/bin/timeout` exist (D-6's absolute paths), as do `/usr/bin/ssh`, `/usr/bin/ssh-add` and `/usr/bin/python3`. `ansible` is not installed on this personal workstation (`/usr/bin/env ansible --version` exits 127): every Ansible fact of this addendum is a gate run at work.
3. **No agent under `--exec`.** In a `wsl.exe -d Debian --exec /usr/bin/env sh -c ...` child, `SSH_AUTH_SOCK` is unset. `--exec` runs no login shell, so an agent, or an agent bridge, that a shell profile starts is invisible to every relay action, from a session's `officina run` and from the window alike, unless the action's `env` names its socket. Gate 3's question is therefore less "do the two paths differ" (both build AD-30's vector) than "is an agent reachable under `--exec` with no WSL terminal open".
4. `WSL_DISTRO_NAME` in that child names the distribution chosen with `-d`, which gate 3 compares across the two paths.

**Read in the code** (main and the work in progress, 2026-10-28).

5. Story 2.8 is under way: `classes::is_available` accepts `human`; `classes::satisfying_check` returns the check run a request records; `approvals::KeyInputs` carries `check_run_id` and `tree`; `approvals::kept_output` writes stdout then stderr into one file under `relay/outputs/`; `run::RunPlan` marks `binds_tree` and `keeps_output` for check actions. The host half of D-170 (consumption-time recheck) is not merged. v0.8 builds on these names.
6. `catalogue::CANONICAL_FORMAT` is 1 on main and story 2.11's HTTP actions are not merged (`IssueKind::HttpNotInThisBuild`): every bump below is relative to main at its merge (D-131).
7. `ActionIssue` has no level: every phase 2 issue drops its action. FR-1's `effect_undeclared` and `target_not_limit` must not (AD-151).
8. `TreeFingerprint` is `{ sha256, untracked_by_size }`; its hash input starts with `officina-tree 1` and lists files with `--exclude-standard`, so ignored files are outside it, as D-170's card line says.
9. `run_started` has no field naming a workshop run, and `RunOutcome` has no outcome for a run Officina ended on a stop.

**Documented.** No page was fetched: the PRD's S1 to S6 back the deferred browser work and the non-goals, and no v0.8 decision rests on them.

**Gate.** PRD section 8, gates 0 to 3, by AD-166. Item 3 answers part of gate 3.

## 2. Decisions

### AD-150: Module layout and where authority lives

- `officina-core` (pure; `relay::scope` and `relay::worker` also compile for the musl target, since the container crate shares their frame types):
  - `relay::catalogue`: `Effect`, the keys `effect`, `target`, `targets`, `[project.scope]`, `bind` and `relay_actions`, their phase 2 rules, canonical forms, diff and autonomy labels (AD-151, AD-152, AD-155, AD-156); `IssueKind::keeps_action` (AD-151);
  - `relay::scope`: `CanonicalScope`, `ScopeStanding`, `OutOfScopeCause`, `judge`, the targets grammar and the names hash (AD-153, AD-154);
  - `relay::worker`: request and frame types, `WorkerRefusal`, `Limiter`, `admit`, `after_targets`, the worker's list text (AD-157, AD-158);
  - `relay::ansible_scan`: `ANSIBLE_CHECK_ESCAPES` and the line scan doctor runs (AD-164);
  - `workshop::vector`: role `RelayMailbox`; `workshop::battery`: the relay family and its catalogue (AD-159).
- `officina-host`:
  - `relay::run` (linked into `officina.exe`): Slice A's scope step, the targets resolution and the consumption-time re-resolution (AD-154); `relay::run::execute` gains an output sink, pass-through for `officina run`, capture for the window; `run::launch_for`, steps 3 to 5 of AD-29 as one function both callers use; `relay::tree`: bound paths (AD-155);
  - `workshop::relay`, under `window-authority`, its error type carrying `AUTHORITY_MARKER` (D-9): the mailbox's attached child, its reader and writer, the per-run limiter state, the job thread that runs a worker's action through `relay::run::execute` (AD-157, AD-158). Built without the feature, none of it exists: `officina.exe` holds no path that runs an action for a worker.
- `officina-container`: `officina-entry mailbox` (AD-157). `officina-relay` is `officina-entry` reached through a link of that name in `/opt/officina/bin` (dispatch on the program name): the image gains a link, not a binary, and AD-99's `officina.entry.sha256` label covers it.
- `officina`: no new subcommand.
- Lint: the `Command::new` sites stay four (D-204). A worker's action goes through `relay::run`, the mailbox through `workshop::engine`.
- Threads (AD-3 amended): one `workshop-relay` job thread per run with a request in flight, so at most `max_workers`; one mailbox reader per live run with relay actions, beside its followers. The collector admits or refuses in core and never runs a program.

Gives up: the window becomes a second caller of the relay runner, with its own job handling (AD-158). In exchange the worker's decision is one pure function with one vector set, and the shipped CLI cannot serve a worker.

### AD-151: Effect and target (FR-1)

- **Keys** of the raw action table (phase 2): `effect` (`read` or `write`), `target` (a parameter name), `targets` (an action name).
- **Levels.** `IssueKind::keeps_action(self) -> bool`, one exhaustive `match`: true for a notice, which leaves the action valid, false for an issue, which drops it as today. Notices: `effect_undeclared` (no `effect`; it reads as `write`), `target_undeclared` (a write without `target`; every call of it is out of scope), `target_not_limit`. Issues: `target` or `targets` on a read; `target` naming none of the action's parameters; `targets` naming no valid read action, or one whose parameter names are not a subset of the write's with the target among them; `effect = "write"` on an HTTP action. Notices reach `RelayView.issues` with their level and doctor as warnings, never the board's `floor_error`. Each kind keeps D-132's failing fixture.
- **`target_not_limit`**, computed on `ArgTemplate`, not on text: when the last path segment of `program`, lowercased, is `ansible-playbook`, the target's placeholder must be the whole item after a literal `--limit` or `-l`, or the item `--limit={target}`; otherwise the notice. The runner cannot know every tool, so it warns.
- **HTTP actions:** `effect` absent reads `read` (GET or HEAD by construction, AD-36), and `effect = "write"` is an issue. The fail-closed default exists for programs Officina cannot see into; an HTTP action is not one, and reading it as a write would drop every existing HTTP action from `relay_actions` and, under Slice A, send dashboard reads to approvals (PRD amendment 1).
- **`check`.** Without Slice A, D-170's rule stands. With Slice A, a write of any class may name a `check`, used only out of scope (AD-154); a read naming a check under a class that cannot use it stays an issue.
- **Canonical:** `CanonicalAction` gains `effect` (explicit: `write` when absent, `read` for an HTTP action when absent), `target` and `targets` (optional). Writing `effect = "write"` where it was absent leaves the hash unchanged, since the canonical form holds meaning; the notice comes from the raw table.

### AD-152: The scope table, the canonical form, the labels (FR-2)

- **`[project.scope]`:** `targets` and `hosts`, each `values` or `pattern` under AD-30's value rules and AD-23's pattern limits (the type is `CanonicalParam`'s); `on_order`, a map from a parameter name to values. Issues: an unknown key; a rule with neither or both forms; an `on_order` name that is a parameter of no action the project lists; a value failing AD-30. An invalid scope table gives its issue and an **empty** scope, never a partial one.
- **Canonical:** `CanonicalProject.scope: CanonicalScope` (`targets` and `hosts` optional, `on_order` a sorted map of sorted values), explicit and empty when the table is absent, so an empty table and no table hash alike.
- **Format bump.** `CANONICAL_FORMAT` goes up by one, relative to main at that merge (D-131), in the first v0.8 story that adds a canonical key. That story lands every key v0.8 uses: AD-151's, AD-152's, AD-156's, and AD-155's `bind` when gate 1 calls for it. Keys validate from then and are enforced by the slice that uses them (P-53's pattern). **Slice A's enforcement bumps it again** if it merges after that bump: the same keys then change what a guarded session may run, and the author's next approval must be the moment FR-3 starts, on a banner that lists the writes it sends to story 2.8's path (PRD question 10, amendment 2). A test loads an `approved.json` of the previous format and asserts AD-76's four facts.
- **Labels** (core `catalogue::autonomy_labels(approved, current) -> Vec<AutonomyLabel>`, beside `diff`):
  - `widens`, each with a `WidenCause` seeded from FR-9's list (a scope value added, a pattern changed, an `on_order` value removed, an effect changed to `read`, a `target` or targets action removed, a bound path removed, an action added to `relay_actions`);
  - `approval_required`: each write whose scope inputs narrowed (a value removed, an `on_order` value added, an effect changed to `write`, a `target` added) and, on Slice A's bump, every write of every guarded project.
  - A pattern change carries both labels: Officina does not compare patterns. From then on `WidenCause::ALL` is the list, and a coverage test iterates it.

### AD-153: Target resolution (FR-3 item 3, shared by both slices)

- **Who runs it:** the relay runner, as a run of the targets action with the write's values for the targets action's parameters, its own `run_id`, journaled `run_started` (with `resolves`, the write's action name) and `run_finished`. For a session, inside the session's `officina run`, in its job (AD-31); for a worker, on the run's `workshop-relay` thread (AD-158).
- **Grammar** (core `scope::parse_targets(stdout, &RunFinished) -> TargetsOutcome`), stdout only:
  - the run exited 0, its stdout was not cut, and it is UTF-8;
  - lines split on `\n`, a trailing `\r` removed, leading and trailing ASCII spaces removed, empty lines skipped;
  - the first line is exactly `hosts (N):` with N decimal, and no other line is a header;
  - every other line is a host name under `scope::is_host_name`: 1 to 253 ASCII letters, digits, `.`, `-` or `_`, the first a letter or a digit (an escape sequence fails it);
  - N equals the count of names, and N is at least 1.

  Anything else fails the resolution with a `TargetsFailure` (`exit`, `timeout`, `cut`, `not_utf8`, `no_header`, `count_mismatch`, `bad_name`, `none`), seeds of a `closed_enum!`. Stderr (Ansible's `[WARNING]` lines) is kept and shown, never parsed. Gate 1's recorded output, redacted, becomes vector cases; a listing gate 1 does not confirm leaves the resolver unbuilt, and every write is then judged on its target value alone (PRD question 1).
- **Names hash:** `targets_sha256` is the SHA-256 of the names, sorted and deduplicated, joined by `\n`. An approval binds the set, not the indentation: an Ansible upgrade that changes spacing voids nothing, a host added or removed voids it (PRD amendment 4).
- **Kept:** a targets run keeps `relay/outputs/<run_id>.stdout` and `<run_id>.stderr`, each capped at D-170's 256 KiB with its cut marker, written under a temporary name then renamed, pruned after D-170's 7 days. The approval card re-parses the stdout file and shows its names only when their hash equals the request's (AD-162).
- **Scope match:** every name must match the project's `hosts`; the first that does not gives `host_not_in_scope`. A scope without `hosts` puts no resolved name in scope.
- The README recommends `ANSIBLE_NOCOLOR = "1"` in a targets action's `env`; gate 1 records whether it is needed.

### AD-154: The decision for a host session's write (Slice A, FR-3)

- **Order** (AD-29 amended): steps 1 to 5 unchanged; step 6 becomes three.
  - **6a** (core `scope::judge(&CanonicalAction, params, &CanonicalProject, Enforcement) -> Judgement`): a read gives `read`; a write in a free project, judged for a session, gives `unenforced`; otherwise, first match: no `target`, `out(target_undeclared)`; the target value failing `targets` (an empty scope fails it), `out(target_not_in_scope)`; any parameter value listed in `on_order`, `out(kept_on_order)`; a declared targets action, `needs_targets`; else `in`.
  - **6b** (host): `needs_targets` runs AD-153. The session reads one stderr line, `officina: targets: N hosts, in scope`, or the first name out of scope, or the failure; `scope::after_targets` gives `in` or `out(host_not_in_scope | targets_failed)`.
  - **6c:** `read`, `unenforced` and `in` take the class exactly as in v0.2 and v0.5. `out`, whatever the class, takes story 2.8's `human` path with the action's `check`: the after-check rule, the kept output on the card, the request bound to the check run and the tree, the fingerprint rechecked before consumption (D-170). A write naming no `check` is refused `no_dry_run`. An `auto` write out of scope never reaches its model: the window's precheck rebuilds the request, judges it again from the rebuilt values and the kept names, and escalates `out_of_scope`, checked first (`EscalationCause` gains it).
- **Request:** `KeyInputs` and `ApprovalRequest` gain `targets_sha256` and `scope_cause`, both optional and skipped when absent, so the key of every other request is unchanged. `run_started` gains `scope` (`ScopeStanding`) and `targets_sha256`.
- **Before consuming** an approval whose request has `targets_sha256`: after D-170's fingerprint recheck, `run` resolves the targets again; a different names hash, or a failed resolution, refuses `targets_changed` and leaves the approval unconsumed (D-29's spirit).
- `RefusalReason` gains `no_dry_run`, `targets_changed` and, with AD-155, `bind_too_large`; `every_refusal_code_exits_126_with_one_journal_line` reaches each.
- **Seeds:** `ScopeStanding` (`read`, `in`, `out`, `unenforced`) and `OutOfScopeCause` (`target_undeclared`, `target_not_in_scope`, `kept_on_order`, `host_not_in_scope`, `targets_failed`) are `closed_enum!`s; from then on `ALL` and `judge`'s order are the list.

Gives up: two Ansible starts before each guarded write that declares a targets action, three when it goes to an approval; PRD section 7's Friction row reports their time. The resolution reads inventory names: an `ansible_host` mapping, a dynamic inventory changing between the resolution and the spawn, and delegation stay outside it (R86).

### AD-155: Bound paths (FR-4, built only if gate 1 finds ignored roles or collections)

- **`[[project]] bind`:** at most `BIND_MAX_PATHS` (8), relative to the project folder, `/` separators, no `.` or `..` segment, not `.git` or under it, no duplicate; issues otherwise. Canonical: sorted.
- **Fingerprint** (host `relay::tree::fingerprint_with(program, dir, bind)`): after the git part, for each bound path in order, a walk with `symlink_metadata`, never following a link or a junction. Entries by relative path, sorted; a file by content up to D-22's 1 MiB and by size beyond; a link by its target text; a missing path by a marker, so absence binds too. Past `BIND_MAX_FILES` (10,000 entries) or `BIND_MAX_BYTES` (256 MiB read) the walk stops and the fingerprint says `bind_too_large`. The bound section is appended only when `bind` is not empty, so a project without it keeps its `officina-tree 1` value. `TreeFingerprint` gains `bound: Option<BoundSummary { files, bytes, too_large }>`.
- A check run on a too-large fingerprint still runs, being a read; an after-check run or a request that needs that check is refused `bind_too_large`.
- `DryRunView.bound` turns the card's line into "tracked, unignored and bound files unchanged since the dry run".

### AD-156: Relay actions of a workshop template (FR-5)

- **`[template.workshop] relay_actions`:** at most 16, distinct. Issues: an action unknown or dropped; one that some project of the template does not list; one whose raw table does not declare `effect = "read"` (a write, or an undeclared effect); a class other than `free` or `after-check`, since a worker never creates an approval request (PRD amendment 6). An HTTP read may be listed; the exposure note applies to it as to any read.
- **Canonical:** `CanonicalWorkshopTemplate.relay_actions`, sorted. The card shows them (v0.6 AD-107); the plan carries them (AD-157), so `plan_sha256` and the author's launch click cover what the worker may ask for.

### AD-157: The worker's channel (FR-6, gate 2)

**A mailbox container per run, attached to the window by its standard streams.**

- **Role `RelayMailbox`** (AD-102's table gains a row): the run image by id (D-207); user 1000:1000; the run's internal network only; no volume, no mount, no credential, no environment name; command `officina-entry mailbox --run <id> --listen 0.0.0.0:<MAILBOX_PORT> --deadline-ms <ms>` (a core constant above 1024); `--interactive`; 0.25 CPU, 64 MiB, 32 processes; every role's options (AD-102). Name `officina-relay-<id>`. Created only for a template whose approved `relay_actions` is not empty.
- **Launch** (AD-107 step 3 amended): after the gateway, before the worker, the mailbox is created and started with `docker start --attach --interactive` as a long-lived child of the window in a kill-on-close job, its stdin and stdout held by `workshop::relay`; then the worker starts. At window start, reconciliation attaches a live run's mailbox again with `docker attach --sig-proxy=false`. `EngineCallKind` gains `attach`, a launch kind under D-204, since it writes into a container.
- **Why this mechanism.** The worker reaches the mailbox by name on its run's internal network, which no other container shares (AD-104), so the run is authenticated by construction: there is no token to steal or forge, and nothing in a request names a run. The window's side is a pipe to a container Officina created. No host port listens, no route leads from the run's networks to the host, no host path is mounted, and the gateway's arguments, image and allow-list do not change; the gateway refuses private addresses, so no worker reaches a mailbox through it. No process is ever started inside the worker's container. The mailbox runs fixed code; a worker that took it over would hold what it already holds, since the window believes no mailbox line.
- **Rejected:** `docker exec` into the worker for each response (a host-started process inside a container the worker controls, once per call); `docker cp` into the running worker (the symlink-race class of copy defects); requests on the worker's own stdout through its follower (re-read from the start after a restart, AD-109, so requests would replay); a Unix socket on the run volume (the worker can replace it, and the packer meets it).
- **Frames** (core types, shared by both crates):
  - worker to mailbox: one line of at most `REQUEST_MAX_BYTES` (16 KiB) with its newline, `{"v":1,"argv":[...]}` or `{"v":1,"list":true}`;
  - mailbox to window, one JSON line each on stdout (`MailboxLine`): `started` (port, the mailbox's clock, D-209's pattern), `request` (`seq`, the line as a string), `refused` (`seq`; `oversized`, `not_utf8` or `read_deadline`). The mailbox numbers connections, parses nothing else, and closes a connection after its answer;
  - window to mailbox, on stdin (`HostFrame`): a header line (`seq`; `kind` `ack` with the deadline, `result` with `exit_code` and the byte lengths of stdout and stderr, or `refused` with the reason and a one-line detail), then, for a result, exactly those bytes. The mailbox forwards each frame verbatim to the connection with that `seq`.
- **`officina-relay`** (in the worker): reads the mailbox's address from the plan (`RunPlan.relay`, which also carries `relay_actions` and the run's catalogue hash; the environment gains no name); connects (10 s); sends; waits up to `ACK_TIMEOUT_MS` (10 s) for an ack, else prints `officina: refused: no_answer` and exits 126 (the window is closed or gone, FR-6's last rule); then waits for the result until the ack's deadline plus 30 s, else 124. A result prints stdout and stderr as received and exits with the program's code (127 when it could not start, 124 on its timeout, AD-29); a refusal prints `officina: refused: <code>: <detail>` and exits 126. `--list` prints the list text.
- One writer per mailbox serialises frames on its stdin; one reader thread hands `MailboxLine`s to the collector, bounded at 17 KiB per line (D-216's pattern).
- Collection (AD-110) stops the mailbox with the gateway, before the packer, and removes it with the run's other objects.

Gate 2 checks the attached stdin on Docker Desktop, byte-exact results, the re-attach, the round trip and every absence above. **Fallback** if attached stdin proves unreliable: requests through `docker attach --no-stdin` to the mailbox, responses through `docker exec -i officina-relay-<id> /opt/officina/bin/officina-entry deliver <seq>` into the mailbox, never into the worker. Both failing, Slice B is not built (PRD question 2).

### AD-158: What the window runs for a worker (FR-7)

- **Admission** (core `worker::admit(&MailboxLine, &RunRelayFacts, &Limiter, now_ms) -> Admission`, on the collector, pure); the first failing step refuses with its `WorkerRefusal`:
  1. the line parses as one request (`malformed`), within its size (`oversized`); a `--list` request is answered here (`listed`);
  2. limits (`Limiter`, a pure state machine per run): nothing in flight (`busy`); 2 s since the last request (`too_soon`); fewer than 120 (`run_cap`); the run `running` with at least 5 s before its deadline (`not_live`);
  3. the relay's and the workshop's stop flags, set or unreadable (`stopped`);
  4. the current floor's hash equals the approved one and the plan's (`catalogue_changed`);
  5. the action is in the approved catalogue and the run's project lists it (`not_listed`);
  6. it is not a write (`worker_write`), whatever its class, target or scope;
  7. it is in the plan's `relay_actions` (`not_listed`);
  8. parameters through `run::launch_for` with AD-30's rules (`missing_parameter`, `unknown_parameter`, `invalid_value`), plus one: a value that is absolute (a leading `/` or `\`, or a drive letter and `:`) is `invalid_value`;
  9. a **dry run**, the `check` of at least one write the project lists: for every such write, `scope::judge` with `Enforcement::Worker` on the dry run's values (same parameter names, D-170); any `out` refuses `order_required` with its cause; a write without a target is `out`; a write with a targets action makes the admission `ResolveThen`. Scope binds a worker's dry run in free and guarded projects alike: the free or guarded choice concerns host sessions and the gate, and a worker has no other route to a server (PRD amendment 8).

  Then `Admission::Run(RunPlan)` with `workshop_run` set. Steps 5 to 7 reorder FR-7's 3 and 5, so a requested write is named `worker_write` although phase 2 keeps writes out of `relay_actions` (PRD amendment 8). `WorkerRefusal` is a `closed_enum!` seeded by these steps, plus `check_required` and `abuse` below; from then on `ALL` and `admit`'s order are the list.
- **Running** (host, `workshop-relay` thread): an `ack` first; the targets run (AD-153) when asked, then `worker::after_targets`, out of scope `order_required`; for an `after-check` read, `run::check_step` (`check_required`); both stop flags read again; the action through `relay::run::execute` with a capture sink, in the **host project folder** resolved from the relay floor as `officina run` resolves it, from the author's tree, never a path of the run folder; timeout the least of `timeout_s` and the time left to the run's deadline; output capped at `max_output_kib` with AD-31's marker. The child is assigned to a kill-on-close job right after spawn (D-64's pattern, whose race AD-98 accepts; D-6's inner `timeout` stays the second line). A check action records the tree and keeps its output (D-170), so a worker's successful dry run satisfies a session's after-check or human request on the same values and tree (R95).
- **Stops:** Stop all, `officina stop --all` and the per-run stop (deadline, limits, abuse) terminate the job; `run_finished` says `stopped` (`RunOutcome` gains it).
- **Abuse:** the 20th refusal of a run (`ABUSE_REFUSALS`, every kind counted) refuses `abuse` and stops the run with AD-109's per-run stop; `FinishCause` gains `relay_abuse`. It is not in `ENVIRONMENT_FAILURE_CAUSES`: a worker asking for what it may not have says nothing about the environment, and the run counts in v0.7's acceptance as any failed run does.
- **Restart:** the limiter is folded from the run's `worker_relay` lines (AD-160) at window start, so a restart resets no count. A request in flight when the window closed is lost; the worker reads `no_answer` or a timeout.
- **Review:** at collection the host folds the run's `worker_relay` lines into its facts (`facts_json`'s typed version goes up by one): per call, outcome, exit code and refusal; the action and values go to `texts_json` as `Untrusted` (D-130). Never an output. `ReviewView.relay_calls` lists them; an `order_required` refusal reads "the worker asked for a dry run out of scope: run it from a session".
- **Governor runs** (v0.7 AD-139) take the same path: nothing in the admission reads who launched the run.

### AD-159: The battery's relay family and the host test (FR-8)

- `BatteryFamily` gains `relay`; `battery::ATTEMPTS` gains its entries, seeded from FR-8's list, each naming its expected `WorkerRefusal`. A battery phase gains a `RelayMailbox`, and the window answers through the same `admit` against `battery::relay_catalogue()`, an approved snapshot built in core, never the author's, whose every program is a sentinel path that does not exist.
- **Judgement:** every relay attempt refused with its expected reason and journaled; no `run_started` for a requested action; one control request admitted, reaching the runner and ending 127 on its sentinel, which shows the channel works end to end. The case of a failing targets resolution runs in the host family, where the fake tool (AD-37) counts exactly one launch of the targets action and none of the dry run.
- Battery relay lines carry the battery id in `workshop_run`; the Relay view lists them under the battery.
- **Host test** (FR-8's last sentence): with the fake engine and the fake tool, a worker request's working directory equals the host project folder, and every argument of the launched vector that holds a path separator resolves under it, never under `<home>/workshop/runs/<id>/`.

### AD-160: Journal kinds and fields

`JOURNAL_LINE_VERSION` stays 1.

| kind | change |
|---|---|
| `worker_relay` (new) | `workshop_run`, `seq`, `received_ms`, `outcome` (`ran`, `refused`, `listed`), `action?`, `params?` (only when every value passed step 8), `reason?` (`WorkerRefusal`), `cause?` (`OutOfScopeCause`), `run_id?`, `targets_run_id?`, `refusals` (the run's count so far) |
| `run_started` | `workshop_run?` (with `session_id` null), `relay_seq?`, `resolves?`, `scope?` (`ScopeStanding`), `targets_sha256?` |
| `run_finished` | outcome `stopped` |
| `refused` | reasons `no_dry_run`, `targets_changed`, `bind_too_large` |
| `approval_requested` | `scope_cause?` |
| `workshop_finished` | cause `relay_abuse` (no new field) |

No line carries an output, a resolved name or a refused value. A build older than v0.8 counts a line with a new outcome or reason as corrupt: downgrades are not supported (R94).

### AD-161: Registry: no migration

v0.8 writes no registry migration; migration 5 stays unwritten until a later need (D-133's rule). Worker calls, refusals, targets runs and scope standings are journal facts (90 days, AD-32), folded where they are read, as v0.7 keeps probe counts in the journal (AD-142). A run's relay calls live in `facts_json` and `texts_json`, typed and versioned in core; `relay_actions` and the run's catalogue hash ride in `plan_json` (AD-157); `workshop_run.cause` has no `CHECK` (AD-119), so `relay_abuse` needs no rebuild.

### AD-162: Payloads, IPC commands, CLI (FR-9)

- **`ApprovalView`** (Slice A): `scope: Option<ScopeView { cause, names: Option<Vec<Untrusted<String>>>, names_missing: Option<String>, stderr: Option<Untrusted<String>>, resolved_at_ms }>`, the names re-parsed from the kept stdout and present only when their hash equals the request's; with AD-155, `DryRunView.bound`. The card says the names were read from the inventory and are read again just before the run, which replaces a per-project dynamic-inventory note Officina could not derive (PRD amendment 10). `BOARD_SCHEMA` goes up by one in the first v0.8 story that changes `Board` (P-13's rule).
- **`RelayView`** (`RELAY_VIEW_SCHEMA` up by one): the pending change gains `labels` (AD-152); per project, its scope, kept values and bound paths; per action, effect, target, targets action, whether it is a dry run listed for workers, and for a write whether an out-of-scope call has a dry run (`no_dry_run` named); `workers`, the last 50 `worker_relay` lines with their run.
- **Workshop:** `WorkshopCardFacts.relay_actions` (`ORCHESTRATOR_VIEW_SCHEMA` up by one); `ReviewView.relay_calls`; `WorkshopView` gains, per live run, the mailbox's state (attached, detached) and its calls and refusals so far, and per template the calls and refusals of the table's period (`WORKSHOP_VIEW_SCHEMA` up by one). Each schema moves once for v0.8, in the first story that changes its payload.
- **Briefing** (AD-83): worker relay refusals since the cursor. `officina mcp` serves them through `briefing` and `relay_activity`; no tool is added, and no kept output, resolved name or worker value is served.
- **Commands:** none added; `get_relay`, `get_workshop` and `open_review` carry the new fields. v0.8 adds no author decision.
- **CLI:** `officina relay --json` and `officina workshop --json` print the new fields. E2E fixtures come from the existing examples (D-25's pattern): `approve_fixture` gains scope requests with kept targets output, and the workshop fixture gains a run with relay calls.

### AD-163: Frontend

- `ApprovalCard.svelte` gains `ScopeBlock.svelte`: the cause, the names as one chip per name, the stderr in a `<pre>`, every text inert; and the bound-files line.
- The catalogue banner (AD-35) renders the labels "widens autonomy" and "approval now required" on neutral surfaces, never `--color-attention` (D-163).
- Relay view: `ScopeTable.svelte`, the effect and dry-run columns, `WorkerCalls.svelte`.
- Workshop: the relay actions on `WorkOrderCard.svelte`; `RelayCalls.svelte` in `ReviewPanel.svelte`.
- Text: `lib/relay-text.ts` holds exhaustive `Record<OutOfScopeCause, string>`, `Record<WorkerRefusal, string>`, `Record<WidenCause, string>` and `Record<ScopeStanding, string>`, so a new Rust variant fails `pnpm check` (AD-13's rule). No rule is computed in TypeScript.

### AD-164: Doctor (FR-10)

- **With the first v0.8 story:** kept outputs under `relay/outputs/` (count, size, oldest), and AD-39's location check on that folder (`SYNC_ROOT_MARKERS`, a `OneDrive*` path, the Cloud Files attribute): a synced folder is a warning naming the folder to exclude, since kept outputs may hold what `--diff` shows.
- **Keys and Slice A:** the notices; writes without a `check` ("an out-of-scope call is refused `no_dry_run`") and without a targets action ("scope judged on names only"); guarded projects without a scope ("every write takes story 2.8's path"); free projects with one ("scope not enforced"); bound paths with their file count and size from one walk, and `bind_too_large`.
- **Slice B:** per template, its relay actions; the `officina-relay` link in each workshop image (AD-118's inventory resolves it as a fixed entry); "worker reads run under your own SSH identity" until PRD question 7's accounts exist, with gate 3's outcome as the README records it; per project with dry runs listed for workers, every file under the project (ignored folders included, `.git` excluded, at most 4,000 files read up to 256 KiB each) where `relay::ansible_scan` finds an entry of `ANSIBLE_CHECK_ESCAPES`, with its counts, as a warning. The constant is seeded once from FR-7's forms; from then on it is the list. The scan is line-based and names files: Officina does not resolve which playbook reaches which file (PRD amendment 11).
- Doctor runs no action, no targets resolution and no container.

### AD-165: Test strategy

| Layer | What | Where and how |
|---|---|---|
| core unit | phase 2 (a failing fixture per new `IssueKind`; notices kept, issues dropped, `keeps_action` exhaustive); canonical format (golden hash, previous-format fixture, AD-76's four facts, `effect = "write"` added where absent leaves the hash); `autonomy_labels`; `judge`; the targets grammar; the names hash; `admit` in order; `Limiter` (rate, cap, busy, abuse, fold from lines); frames both ways; `ansible_scan` | CI Windows and Linux, plus a musl build of `relay::worker`; vector sets `tests/vectors/scope.json` (sections `judge`, `targets`, `labels`) and `tests/vectors/worker_relay.json` (`request`, `admit`, `limiter`, `frames`), formatter-exempt; coverage over `ScopeStanding::ALL`, `OutOfScopeCause::ALL`, `TargetsFailure::ALL`, `WorkerRefusal::ALL`, `WidenCause::ALL` and `ANSIBLE_CHECK_ESCAPES`; gate 1's listings, redacted to `host01`, `web01` and group `lab`, as `targets` cases |
| blocking property | for every `admit` case expecting `Run` and every `WorkerRefusal`, the test's `make_fail(refusal, &mut facts)`, an exhaustive `match`, makes that one condition fail and `admit` must refuse with it (AD-148's pattern); the same for `judge` over `OutOfScopeCause` | same test files, no crate |
| runner | `relay::run::execute` with the capture sink gives the bytes, hashes and cut marker of the pass-through sink | host integration |
| Slice A flows | the fake tool as write and targets program: in scope runs under its class; each out-of-scope cause takes story 2.8's path and needs the check; `no_dry_run`; each resolution failure; a warning on stdout fails, on stderr does not; `targets_changed` at consumption leaves the approval unconsumed; a free project is `unenforced`; an `auto` write out of scope escalates with no review call; a bound file changed refuses `check_not_satisfied`; `bind_too_large`; a bound link is not followed | CLI integration, `window-authority` through the dev-dependency |
| mailbox and client | `officina-entry mailbox` and `officina-relay` built natively, on loopback, driven by a harness on the mailbox's stdio: frames, oversize, not UTF-8, read deadline, two connections, `no_answer` after 10 s, timeout, exit codes, a 1 MiB result with every byte value | CI Windows and Linux, no Docker (AD-125's gateway pattern) |
| Slice B flows | the fake engine (D-223) gains attached `start` and `attach` for role `RelayMailbox`, replaying request lines from fixtures and recording frames: every refusal reachable; `worker_write` for a write of each class; `order_required` per cause and on a failing resolution; `catalogue_changed` after an approval; both flags; Stop all ending a job (`stopped`); the 20th refusal stopping the run (`relay_abuse`); a restart folding the limiter; the review's relay calls; FR-8's path containment | CLI integration |
| battery | the relay family on the CI `workshop` job and on the workstation, with the sentinel catalogue | AD-117's harness |
| injection | `injection.json` gains `worker_relay` (D-147): values shaped as options, absolute and `..` paths, shell text, bidi characters, a forged `seq`, a request right after a catalogue approval, a flood; the fake tool counts launches | D-147's harness |
| mutation | `mutant-scope-skip` (6a skipped), `mutant-targets-skip` (6b skipped), `mutant-worker-scope-skip` (step 9 skipped) join D-148's job; each turns a named case red in `scope.json` or `worker_relay.json` and in the CLI flows | CI `mutants` |
| zero write | `quota_doctor.rs` around a Slice A refusal, an approval with targets, a worker request served with the fake engine, `relay --json` and doctor: the Claude config dir unchanged; the project repository unchanged by a worker's read | CLI integration (D-80) |
| frontend | Vitest: the text records for every variant; `ScopeBlock` inert with markup and bidi characters in names and stderr; labels never on the attention colour. Playwright (`app/e2e/relay-scope.spec.ts`, `app/e2e/workshop-relay.spec.ts`) on the examples' fixtures | CI `windows` |

Manual, recorded in research notes: AD-166's gates; PRD section 7's first table over story 2.8's two weeks; Slice A's and Slice B's rows at the close.

### AD-166: Gates, exact procedures

Each records an E-entry, numbered from the first free number after Epic 7's, in FR-0's order, with the date, the versions (Claude Code, WSL, Ansible, Docker Desktop where used) and who ran it. Work-floor values never appear: counts, shapes and redacted samples (`host01`, `web01`, `lab`, `example.org`), never an inventory name, a path or a playbook name. An agent runs what needs no server, key or inventory; the owner runs the rest.

| # | Procedure | Pass | On failure |
|---|---|---|---|
| 0 | Owner, at work, before any v0.8 story. A scratch script the agent prepares (never committed, run on the work machine, printing counts only) reads the measured session's Bash commands from the owner's own record of that session and classifies them: `ansible-playbook` runs (direct, through `wsl`, through `bash -c`), with or without `--check` or `-C`, their limit (`--limit`, `-l`, `--limit=`) against the owner's lab pattern, their playbook; `ssh`, `scp` and `rsync` runs by host class, listed for the owner to mark state-changing by hand. Record: total commands; production writes; distinct playbooks and groups they touched; per guarded project, the write actions an action-pair catalogue would need. Part (b), only if Slice A triggers: per guarded project, target values and host names in scope, kept values, reads | recorded | not a failure: over 10 write actions in a project makes Slice A worth building without waiting for the trigger (PRD question 0) |
| 1 | Owner, at work, per guarded project, every command through the relay's own vector (`wsl.exe [-d <distro>] --cd <project> --exec /usr/bin/env <the action's env> <program> ...` from Git Bash with `MSYS_NO_PATHCONV=1`, or as a scratch catalogue action run with `officina run`), stdout and stderr to separate scratch files outside every repository. (a) `ansible <lab group> -i <inventory> --list-hosts`, then an empty group and an unknown one, each with and without `ANSIBLE_NOCOLOR=1`: exit code, time, stdout's shape as a redacted sample, stderr's line prefixes, the count of ESC bytes on each stream. (b) Size distribution and ESC count of story 2.8's kept outputs (`relay/outputs/*.txt`) of production checks; no new run. (c) `grep -rEc` of FR-7's four forms under the project, ignored folders included (doctor's scan once AD-164 ships): count per form, number of files. (d) `git -C <project> status --ignored --porcelain=v1` filtered to roles, collections, vault files, `host_vars` and `group_vars`; `ansible-galaxy collection list` and `ansible-galaxy role list` through the vector, each entry inside the tree and ignored or outside it; `ansible-config dump --only-changed` through the vector, the names of changed settings and, for path settings, inside or outside the tree, never a value; the inventory static or dynamic (a `plugin:` key, or an executable in it), and the count of host entries setting `ansible_host`. No playbook runs and no host is contacted | recorded per project | a listing AD-153 does not parse: the resolver is not built and writes are judged on names only; ignored roles or collections under the tree: AD-155 is built; the counts go to the README beside `effect`; `ansible_host` entries or a dynamic inventory: README and R86 |
| 2 | Agent, on the workstation's Docker Desktop, a dev build, from Git Bash with `MSYS_NO_PATHCONV=1`, probe objects named `officina-probe-*`, all removed and their absence checked. AD-104's network pair; a probe image `FROM` the probe base with the musl `officina-entry` and the `officina-relay` link; the mailbox created with AD-157's vector and started attached by `crates/officina-host/examples/mailbox_probe.rs` (the real `workshop::relay` attach, answering every request with a fixed result); a probe worker on the internal network running `officina-relay probe n=<i>` 100 times, one per 2 s, timing each, then one 1 MiB result holding every byte value (SHA-256 compared); the example killed and started in re-attach mode, 10 more requests; the example stopped, 3 requests. Before and after: the host's listening TCP sockets (`Get-NetTCPConnection -State Listen`), every probe container's mounts (`docker inspect`), the gateway vector against its golden case, and the battery's egress and host families with the mailbox present, plus the mailbox's other ports | p95 from send to result at the worker under 2 s; byte-exact result; re-attach works; `no_answer` (126) within 10 s once the example is stopped; no new listener, no bind mount, gateway unchanged, every family attempt refused | AD-157's fallback, run the same way; both failing: Slice B is not built (PRD question 2) |
| 3 | Owner, at work, every WSL terminal closed first (`wsl.exe --list --running` names no work distribution). Scratch read actions in the work catalogue, with the `env` the real actions declare: `agent-probe` (`/usr/bin/ssh-add -l`) and `config-probe` (`ansible --version`). From a guarded session, `officina run` each; from the window, `crates/officina-host/examples/worker_relay_probe.rs` (dev build, `window-authority`) runs both through AD-158's runner. Record per path: exit codes, line counts, the SHA-256 of the `config file =` line, `WSL_DISTRO_NAME`. Then, with keys loaded as the owner works (`ssh-add -c` if he uses it), a lab action running `ssh -o BatchMode=yes <lab host> true` from both paths: whether a confirmation appeared, where, whether the call waited, its time. **Verified in part** (section 1, item 3) | both paths equal; the agent reachable (`ssh-add -l` exits 0); no prompt, or the owner accepts one per worker read | agent unreachable under `--exec`: the actions' `env` names the socket, or question 7's dedicated key (a key file in the action's `args`, with `IdentitiesOnly`) becomes a precondition of Slice B; a prompt per use: the same |

### Slices

- **Slice 0:** gate 0 before any v0.8 story; gate 1 before Slice A or B; gates 2 and 3 before Slice B. Gate 2 can run as soon as v0.6's musl programs exist.
- **Keys,** the first v0.8 story with a canonical key, whichever slice comes first: AD-151, AD-152 with its bump and labels, AD-156, AD-155's key when gate 1 called for it, AD-164's first item.
- **Slice A, conditional** (PRD section 7's first table, the author's request, or gate 0 above 10 write actions in a project): AD-153 for host sessions, AD-154, AD-155 when gate 1 called for it, the Slice A parts of AD-160 and AD-162 to AD-165, and the second bump if it merges after the keys story. Needs story 2.8 merged.
- **Slice B:** AD-153 (built here if Slice A is not), AD-157 to AD-159, AD-160's `worker_relay`, AD-161, the Slice B parts of AD-162 to AD-165. Needs v0.6's Slice A merged and the keys.

### FR to AD

| FR | Built by |
|---|---|
| FR-0 | AD-166, Slices |
| FR-1 | AD-151 |
| FR-2 | AD-152 |
| FR-3 | AD-153, AD-154 |
| FR-4 | AD-155 |
| FR-5 | AD-156 |
| FR-6 | AD-157 |
| FR-7 | AD-158 |
| FR-8 | AD-159, AD-165 |
| FR-9 | AD-162, AD-163 |
| FR-10 | AD-164 |

### Touch points with v0.2 to v0.7

- `core/relay/catalogue.rs` (keys, levels, phase 2 with `check` on writes under Slice A, the format bump, `diff` and labels), `core/relay/run.rs` (step 6, `launch_for`), `core/relay/approvals.rs` (`KeyInputs`, `ApprovalRequest`, `ApprovalView.scope`, `DryRunView.bound`, kept targets files), `core/relay/journal.rs` (`worker_relay`, the new fields, outcome and reasons), `core/relay/view.rs` (schema, scope, workers), v0.5's `relay::auto` (`out_of_scope`), the v0.6 `workshop` modules (`RelayMailbox`, `RunPlan.relay`, `FinishCause::RelayAbuse`, `EngineCallKind::Attach`, `BatteryFamily::Relay`, the inventory's fixed entry), `core/board.rs` (schema).
- `host/relay/run.rs` (scope step, resolution, consumption, sink), `host/relay/tree.rs` (bound paths), `host/relay/home.rs` (kept targets files and their pruning), `host/workshop/*` (launch step 3, reconciliation, collection, the relay module), `host/watch.rs` (mailbox lines, admission), doctor.
- `crates/officina-container` (`mailbox`, the `officina-relay` name), `docker/workshop.Dockerfile` (the link).
- `officina/tests` (`run.rs`, `approvals.rs`, `injection.rs`, `quota_doctor.rs`, the workshop flows); the app's views and components named in AD-163; `ipc.ts` types through ts-rs, `e2e-mock.ts`; `e2e-fixtures.mjs`.
- `.github/workflows/ci.yml`: the musl build of `relay::worker`, the mutants' targets.

## 3. Amendments to earlier decisions

- **AD-3:** threads `workshop-relay` (one per run with a request in flight) and one mailbox reader per live run with relay actions.
- **AD-21:** `window-authority` also gates running an action for a worker and attaching a mailbox.
- **AD-23, D-132:** issues gain a level (`IssueKind::keeps_action`); notices leave the action valid.
- **AD-24, AD-76, D-131:** the canonical format goes up by one when v0.8's keys land, and again when Slice A's enforcement lands if that is later.
- **AD-29:** step 6 becomes 6a to 6c in guarded projects (Slice A); steps 3 to 5 become `run::launch_for`, shared with the worker's admission.
- **AD-31:** `execute` takes an output sink; jobs the window starts use D-64's assignment.
- **AD-32:** AD-160's kind and fields.
- **AD-33, D-170:** the request key gains `targets_sha256`; consumption re-resolves targets after the fingerprint recheck; a check run for a worker counts as a check.
- **AD-35, AD-92, v0.6 AD-121:** `BOARD_SCHEMA`, `RELAY_VIEW_SCHEMA`, `ORCHESTRATOR_VIEW_SCHEMA` and `WORKSHOP_VIEW_SCHEMA` each up by one for v0.8.
- **AD-87:** `EscalationCause` gains `out_of_scope`, checked first.
- **v0.6 AD-98, D-204:** `EngineCallKind::Attach`, a launch kind.
- **v0.6 AD-99:** the `officina-relay` link in the base image.
- **v0.6 AD-102:** role `RelayMailbox`; `RunPlan.relay`; `plan_sha256` also covers the mailbox's vector.
- **v0.6 AD-104:** the internal network also holds the mailbox.
- **v0.6 AD-107:** step 3 creates and attaches the mailbox before the worker starts.
- **v0.6 AD-108:** `officina-entry mailbox`; `officina-relay` by program name.
- **v0.6 AD-109:** `FinishCause::RelayAbuse`, outside `ENVIRONMENT_FAILURE_CAUSES`; reconciliation attaches live mailboxes again.
- **v0.6 AD-110:** collection stops and removes the mailbox.
- **v0.6 AD-117:** the relay family and the battery mailbox.
- **v0.6 AD-118:** the link as a fixed inventory entry.
- **v0.6 AD-123:** AD-164's items.
- **D-22:** the bound section of the fingerprint.

**Dependencies for approval.** None. No crate, image, tool, Node package or Windows feature is added; `docker start --attach --interactive` and `docker attach` are subcommands of the CLI v0.6 already drives. Ruled out: a JSON-RPC or base64 crate (frames are a header line and counted bytes), `walkdir` (`read_dir` with `symlink_metadata`), any YAML parser (doctor's scan is line-based, and Officina does not parse Ansible), `nix` (the mailbox uses std networking and stdio).

## 4. PRD amendments

1. **FR-1.** Append: "On an HTTP action, `effect` absent reads `read`."
2. **FR-1.** Replace "the canonical format goes up once for v0.8" with "the canonical format goes up once when v0.8's keys land, and again when Slice A's enforcement lands if that is later, so the author approves the change that starts FR-3 on a banner listing the writes it sends to story 2.8's path".
3. **FR-1.** Append: "`effect_undeclared`, `target_undeclared` and `target_not_limit` are notices: the action stays valid."
4. **FR-3.** Replace "the SHA-256 of the targets stdout" with "the SHA-256 of the resolved names, sorted", and "a different stdout refuses `targets_changed`" with "a different set of names, or a failed resolution, refuses `targets_changed`". In item 3, replace "one per line, empty lines and Ansible's `hosts (N):` header skipped" with "one per line after trimming spaces, empty lines skipped, after exactly one first line `hosts (N):` whose N equals the count of names". Append: "With Slice A, a write of any class may name a `check`, used only out of scope. An `auto` write out of scope escalates before any review call."
5. **FR-4.** Append: "The walk never follows a link; a link binds by its target text and a missing bound path binds its absence."
6. **FR-5.** Append: "A relay action's class is `free` or `after-check`: a worker never creates an approval request."
7. **FR-6.** Replace "The architecture chooses the mechanism" with "A mailbox container per run, on the run's internal network only, attached to the window by its standard streams, carries requests and responses; `officina-relay` is `officina-entry` by another name and reads the mailbox's address from the plan. With no answer within 10 s it prints `no_answer` and exits 126."
8. **FR-7.** Replace steps 3 to 5 with "3. the action is in the approved catalogue and the run's project lists it; 4. any write is refused `worker_write`; 5. the action is in its template's `relay_actions` as approved at launch, and its parameters pass AD-30, a value that is an absolute path refused". Append: "Scope binds a worker's dry run in free and guarded projects alike. A worker's successful dry run counts as a check for a session's after-check or human request on the same values and tree. `relay_abuse` ends the run and is not an environment failure."
9. **FR-8.** Replace "Pass: every attempt refused and journaled, zero launches" with "Pass: every attempt refused and journaled, no launch of a requested action; on the engine every program of the battery's catalogue is a sentinel that does not exist, and the failing targets resolution case, which launches its targets action once, runs in the host family".
10. **Section 8, question 1.** Replace "a dynamic inventory is stated on the card" with "the card states that the names were read from the inventory and are read again just before the run".
11. **FR-10.** Replace "the playbooks with gate 1's counts beside the dry-run actions listed for workers" with "the files under the project, ignored ones included, in which a line-based scan finds a `check_mode` false or no, `delegate_to`, `local_action` or `run_once`, with counts, beside the dry-run actions listed for workers"; replace "gate 3's outcome" with "gate 3's outcome as the README records it".
12. **Section 7, Slice B Round trip.** Replace "worker request to the action's start" with "from the window's receipt of a request without a targets resolution to the action's start, plus gate 2's in-container hop; requests with a resolution are reported apart".
13. **Section 8, question 3.** Append: "On the workstation, `SSH_AUTH_SOCK` is unset in a `wsl.exe --exec` child: an agent started by a shell profile is invisible to both paths unless the action's `env` names its socket (addendum section 1)."

## 5. Risks

| # | Risk | Mitigation |
|---|---|---|
| R83 | Attached stdin to a container is unreliable on Docker Desktop, or a release changes it | gate 2 with byte-exact results; the fallback through `exec` into the mailbox; the battery's control request on every validity key |
| R84 | A worker's reads reach production under the author's identity and their output leaves through `api.anthropic.com` | the README's exposure note beside `relay_actions`; arguments closed by the catalogue; question 7's diagnostic accounts; doctor names the identity |
| R85 | A dry run writes (`check_mode` false, delegation) | worker dry runs only in scope; doctor's scan and gate 1's counts; the README beside `effect`; for host sessions a stated limit (PRD section 1) |
| R86 | An inventory maps a lab name to a production address, a dynamic inventory changes after the resolution, or delegation leaves the limit | stated on the card and in the README; resolution again before consumption; gate 1 counts `ansible_host` entries and dynamic inventories |
| R87 | Slice A's fail-closed defaults stall every undeclared read in guarded projects on its first approval | the banner's "approval now required" list; notices; doctor; question 10's confirmation |
| R88 | No SSH agent is reachable under `--exec` from the window, or every use prompts | gate 3 with no WSL terminal open; the action's `env`; question 7's dedicated key as a precondition |
| R89 | An Ansible upgrade changes the `--list-hosts` listing | the grammar fails closed (out of scope, an approval, never a run); names hash over sorted names; vector cases from gate 1; the Friction row |
| R90 | A worker floods the window or the journal | 16 KiB per request, one in flight, 2 s spacing, 120 per run, the 20th refusal stops the run; the reader's line bound |
| R91 | A bound path is large and slows every check | `BIND_MAX_FILES` and `BIND_MAX_BYTES` refuse with `bind_too_large`; doctor shows sizes |
| R92 | Kept outputs hold secrets `--diff` printed, in a synced home | doctor's location warning; 7-day pruning; README |
| R93 | Slice A lands after Slice B and the author approves twice | the second approval is the moment FR-3 starts, by design; the banner lists its effect |
| R94 | A build older than v0.8 reads a new outcome or reason as a corrupt line | downgrades unsupported; doctor counts corrupt lines (D-32) |
| R95 | A worker's dry run satisfies a session's after-check or human request | it is a real dry run, in scope, on the author's tree and values; `run_started` names the workshop run |
| R96 | A window-run action spawns a process between spawn and job assignment | D-64's race, accepted; D-6's inner `timeout`; Stop all terminates the job |
| R97 | A worker takes over its mailbox | the mailbox holds no credential, mount or volume and only the run's internal network; the window believes no mailbox line; the battery plays the relay family through it |
