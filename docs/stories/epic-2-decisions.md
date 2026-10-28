---
title: "Epic 2 decisions"
author: Rwx-G
date: 2026-10-27
inputs:
  - prd/prd-v0.2.md
  - architecture-v0.2.md
  - architecture-spine.md
  - research-notes.md
  - (review of the Epic 2 stories, not published)
---

# Epic 2 decisions

What the PRD, the addendum and the spine leave open or state twice, settled by the owner on 2026-10-27. These decisions are final. Stories cite them as `D-n` or `P-n` and name the test that proves each one; this file states the rule, never the test data. A later change edits this file with the fact that motivates it, like the architecture.

## Precedence

- **P-1.** The addendum's section 3 amendments win over the PRD text.
- **P-2.** Parameter values are 1 to 256 characters (AD-30). FR-3's "64 characters of input" was not amended and does not apply. (2.1)
- **P-3.** An `after-check` binds to the catalogue hash and the tree fingerprint as well as project and parameters (AD-33), beyond FR-8 and amendment 8. (2.6)
- **P-4.** A `gate_error` line takes the journal lock like a deny; AD-28's "no lock except the journal append on a deny" covers both. (2.4)
- **P-5.** "The `officina` binary contains no authority code" (AD-21) holds for `officina.exe` built alone (`-p officina`), which is what ships. Test builds and workspace builds do contain it. A CI step proves the shipped property, with a positive control (D-9). (2.2)
- **P-6.** `--wait` (up to 540 s) plus `timeout_s` (up to 540 s) can exceed the Bash tool's 10-minute cap; a run killed by Claude Code reads "abandoned". Accepted; doctor warns (D-32). (2.8, 2.10)
- **P-7.** The owner's rule: what a PRD defers is done at the end of that PRD, before the next one. It supersedes amendment 1's "FR-5 and FR-12 follow as v0.2.x" and AD-36's "build it after a week of use, or drop it". AD-36's design is unchanged; the stories do not edit the addendum. (2.11)

## Floor and catalogue

- **D-1.** `env` keys match `^[A-Za-z_][A-Za-z0-9_]{0,63}$`; values are literals, never placeholders. (2.1)
- **D-2.** `{` and `}` in an `args` item are placeholder delimiters only; an unbalanced brace or a second placeholder is an issue. (2.1)
- **D-3.** A `guarded_programs` entry is a non-empty program name, `*` allowed only as its last character. (2.1)
- **D-4.** `distro` must start with a letter or digit: `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`, tighter than AD-23. (2.1)
- **D-5.** On the `native` runner a value starting with `/` or `@` is refused (Windows switches, response files), and a `program` ending in `.bat` or `.cmd` is a validation issue (re-parsed by `cmd.exe`). (2.1)
- **D-6.** The `wsl` vector calls `/usr/bin/env` and `/usr/bin/timeout` by absolute path; the inner timeout is `timeout_s + 5` so the outer job kill fires first and the journal says `timeout`; `WSLENV` is removed from the `wsl.exe` child's environment; `--cd` receives the project folder as resolved from the floor, not the lowercased comparison form. (2.1, 2.5)
- **D-7.** The relay keys refused in a schema 1 floor are one constant, asserted against the floor type by a test that serializes a fully populated schema 2 floor and compares its keys, minus the v0.1 keys, with the constant. (2.1)
- **D-8.** `approve_catalogue(home, hash)` hashes the floor found by AD-24's relay floor resolution, never the window's `--floor`. The pending diff compares the approved snapshot with that same floor. The board keeps using the window's floor (AD-7). (2.2, 2.7)
- **D-9.** The authority marker is a string the authority functions use (in their error type), so `lto` and `strip` cannot remove it from a binary that links them. The CI step builds `-p officina` itself right before searching, and runs the same search on `officina-app.exe`, where the marker must be found. (2.2)

## Gate

- **D-10.** Lexer, beyond AD-27 (review finding F1):
  - a command-position word containing an unquoted `*`, `?`, `[`, a brace expansion, `$'...'` or `$"..."`, or any other non-literal part, is dynamic: deny;
  - quote concatenation (`s"s"h`, `s'sh'`, `\ssh`) and backslash-newline are joined into one word by the lexer, then matched;
  - each wrapper has a table of the options that take a value (`timeout -s X`, `nice -n N`, `xargs -n N` and `-I R`, `env -u NAME`, `sudo -u U`, `stdbuf -o M`, `watch -n N`, `exec -a NAME` and the others of each tool), so the next command word is found;
  - sh-family interpreters match any combined short-option cluster containing `c` (`-lc`, `-ec`, `-xc`) and `-o opt` before it; `python`, `python3`, `py`, `pythonw` with `-c`; `node` with `-e`, `-p`, `--eval`, `--print`; `perl` with `-e` or `-E`; `powershell` and `pwsh` with `-Command` or `-c` (any case) are scanned; `-EncodedCommand`, `-enc`, `-e` and any other abbreviation of it are denied outright, since base64 cannot be scanned; `cmd` with `/c`, `/C`, `/k`, `/K` is scanned after caret removal;
  - `coproc` is a reserved word;
  - everything the lexer still cannot catch stays an honest `limit` case in `gate.json`. (2.3)
- **D-11.** The default guarded list gains Windows `bash.exe` (it starts WSL) and the distro launchers `ubuntu*`, `debian`, `kali`, `opensuse*`, `sles*`, `fedora*`, `alpine*`, `oracle*`. The constant in core is the only list. AD-27 strips `.exe` before matching, which would make `bash.exe` deny every Git Bash `bash -c`; so `bash.exe` is matched on the word as written, before stripping: the explicit `.exe` spelling, or a path under `windows/system32` or `windows/sysnative`. A bare `bash` stays a shell interpreter, lexed recursively. (2.3)
- **D-12.** The gate denies an `officina` command word preceded, in the same command text, by an assignment to `USERPROFILE`, `HOME`, `LOCALAPPDATA`, `APPDATA` or any `OFFICINA_*` name, whether as a prefix assignment, through `env`, or through `export` or `declare`. (2.3)
- **D-13.** Protected directories (AD-26 step 4) are the absolute `<home>/relay` and `<home>/journal`, normalised, plus the `.officina/relay` and `.officina/journal` tails when the home is the default one. A bare `relay/` substring is not matched. (2.3)
- **D-14.** The effective `guarded_programs` is the union of the current floor's list (phase 1, defaults applied) and the approved snapshot's list. A project is guarded if either source says so. (2.4)
- **D-15.** An unreadable or corrupt `approved.json` never opens the gate: every project declared in the current floor (phase 1) is then guarded, with the default list plus the floor's list. If the floor fails phase 1 too, the gate denies guarded programs whenever `CLAUDE_PROJECT_DIR` or `cwd` lies inside a project path of the last readable state, else no decision. The last readable state is `relay/known-projects.json`, written atomically by the window (on approval and whenever it reads a valid floor or snapshot), holding every declared project path and the last approved hash; a missing `approved.json` counts as unreadable when that file records an approved hash. Internal panics stay fail-open: exit 0, `gate_error` journaled. This adds one file to AD-25's table. (2.2, 2.4, 2.7)
- **D-16.** A call is guarded if the project of `CLAUDE_PROJECT_DIR` or the project of the payload `cwd` is guarded (E15: `cwd` follows `cd`, `CLAUDE_PROJECT_DIR` stays the project root). The payload field set and the deny JSON shape are E15's. (2.4)
- **D-17.** The deny reason lists the project's actions from the approved snapshot; without one it lists the action names of the floor's project entry and says the catalogue awaits approval. An action whose class is not available in the current build is marked so, in the deny reason and in `run --list`. (2.4, 2.5)
- **D-18.** `PreToolUse` with matcher `Bash` is a required hook only when the relay floor has at least one guarded project; only then does doctor's printed block include it. The buffered v0.1 events are their own constant, separate from the (event, matcher) registrations. (2.4)
- **D-19.** Gate performance is gated relative to an empty process, like the hook: own cost (gate p95 minus empty-process p95) under 5 ms on the workstation; CI asserts a looser bound set in `ci.yml`, since a hosted runner measured about twice the workstation's cost (2026-10-28). The absolute in-process bound (3 ms p95, AD-28) is asserted only on the workstation, opted in by an environment variable, and printed elsewhere. (2.4)

## Run

- **D-20.** Until stories 2.6 and 2.8 land, `after-check` and `human` actions are refused with reason `class_not_available`; 2.6 removes `after-check` from it and 2.8 removes the code. (2.5)
- **D-21.** `run` serves any project of the approved catalogue that lists the action, `free` or `guarded`: autonomy decides the gate, not the relay. `run --list` lists the cwd project's approved actions, marks unavailable classes (D-17), states a stopped relay or a pending change, and exits 0; outside every project it says so and exits 0. (2.5)
- **D-22.** Tree fingerprint: `git` runs with `--no-ext-diff --no-textconv -c core.fsmonitor=false`; untracked files (`--exclude-standard`, so ignored files do not count) are hashed by content up to 1 MiB each and by size beyond, and the fingerprint records that it did so; a work tree with no commit yet uses a fixed "no HEAD" marker and stays bound; when `git` cannot run in a work tree, or the folder is not one, the field is absent and doctor names the project. (2.6, 2.10)
- **D-23.** Tests that need `git` get a `PATH` holding git's directory, resolved by the harness; when no `git` exists they skip with a printed reason. (2.6)
- **D-24.** `officina relay --json` exits 0, 64 on usage, 1 when the home cannot be read; never 2. `RelayView.catalogue` is the approved snapshot; the current floor's differences are `pending.diff`. (2.6)

## Window and approvals

- **D-25.** E2E fixtures get an approved catalogue from the Rust example `crates/officina-host/examples/approve_fixture.rs` (`required-features = ["window-authority"]`), run by `app/scripts/e2e-fixtures.mjs`; never by recomputing the hash in Node, never by a CLI approve path. Abandoned runs in fixtures come from a seeded journal, as v0.1 seeds events. (2.7, 2.8, 2.9)
- **D-26.** FR-11 stop moves to Slice A: the flag file writer, the refusal and the window toggle ship with story 2.7. (2.7)
- **D-27.** The approval card and `decide_approval` never trust a request file's `argv`: core rebuilds the argument vector (or, for HTTP, method and URL) from the approved catalogue and the request's `(project, action, params)`, recomputes the request key, and a mismatch makes the request invalid: shown as invalid, not approvable, refused by `decide_approval`. (2.8, 2.11)
- **D-28.** Approve and Reject act only on a pointer click (`click` with `event.detail > 0`); Enter and Space on a focused card or button do nothing. There is no keyboard path to approve: a stray key must not run a playbook (AD-35). (2.8)
- **D-29.** A failure after an approval was consumed (spawn failure, lock timeout) burns it; the author approves again. A credential is read before the approval is consumed, so a missing credential does not burn it. (2.8, 2.11)

## Doctor and documentation

- **D-30.** Doctor compares the floor it resolves (AD-7, its own `--floor`) with the relay floor resolution (AD-24) and warns when they differ; it cannot see the window's arguments. (2.10)
- **D-31.** Doctor levels: a missing `PreToolUse` registration while a project is guarded (D-18), a program that does not resolve, and a missing credential used by an approved action are failures; catalogue issues, a pending change and every warning of D-32 are warnings. (2.10, 2.11)
- **D-32.** Doctor warns when an action's `timeout_s`, plus the maximum `--wait` for a `human` action, can exceed the Bash tool's cap (P-6), when an `after-check` project has no tree binding (D-22), on a floor mismatch (D-30), and on unreadable journal lines or gate errors in the last 24 h. (2.10)
- **D-33.** The `CLAUDE.md` paragraph doctor prints is generated from the approved catalogue; with nothing approved it says the catalogue awaits approval in the window and lists no action. (2.10)
- **D-34.** Documentation is incremental: every story updates `CHANGELOG.md` `[Unreleased]`, and every story with user-visible behaviour updates the README's relay section. Story 2.10 finalises and adds a test that the README mentions every CLI subcommand of the usage text. Story 2.10 depends on stories 2.1 to 2.7 only. (all)

## HTTP

- **D-35.** HTTP exit codes: 0 for a 2xx status, 1 for any other status (stderr `officina: http <status>`), 124 on timeout, 127 when no response arrives (name resolution, connection or TLS failure); never 2. (2.11)
- **D-36.** `run_finished` outcomes for HTTP: `http` (with `status`), `timeout`, `connect_failed`. (2.11)
- **D-37.** A credential's `hosts` match the URL host exactly, case-insensitively, port ignored, no wildcard. (2.11)
- **D-38.** `cmdkey` line: `cmdkey /generic:officina:<name> /user:<user> /pass`, `/pass` without a value so `cmdkey` prompts and the secret stays out of shell history. (2.11)
- **D-39.** The canonical catalogue covers credentials and HTTP fields; its `format` is bumped, which invalidates earlier approvals on purpose (AD-24 predates credentials). (2.11)
- **D-40.** A credential `header` is an HTTP token without CR or LF and is not `Host`, `Content-Length`, `Transfer-Encoding`, `Connection` or `Cookie`; `scheme` is a token; a URL template with userinfo is an issue. Each is an issue kind. (2.11)
- **D-41.** The window checks credential presence when the relay view is recomputed for a floor or relay change, and at most once a minute otherwise; never on every tick. (2.11)

## Intervention orders

- **D-170.** A `human` action may name a `check` (its dry run). Its request then needs a successful run of that check under the after-check rule (same project, check action, mapped parameters, approved catalogue hash, tree fingerprint, within 30 minutes), or `run` refuses `check_required`; the request records the check's run id and tree fingerprint. `officina run` keeps the output of every check action it runs (stdout then stderr, capped at 256 KiB with a cut marker) under the relay home, pruned after 7 days by the window's hourly pass. The approval card shows the kept output of the check its request names, and states "tracked and unignored files unchanged since the dry run", never "tree unchanged" (the fingerprint skips ignored files, collections outside the tree, configuration and interpreters). Just before consuming an approval, `run` recomputes the fingerprint; a mismatch refuses `check_not_satisfied` and leaves the approval unconsumed. Taken from the v0.8 requirement challenge, which showed that the intervention order's core (dry run on the card, binding to the dry run's tree) belongs in the approval flow being built rather than in a later scope engine. An Ansible `--check` action is harmless only if no task it reaches sets `check_mode: false` or delegates; the README says so beside the relay classes. (2.8, 2.10)
- **D-171.** The gate covers every tool that runs a shell command, named in one core constant with a rule each: `Bash` and `Monitor` (its `command` form) are lexed and decided by `decide`; `PowerShell` is denied whole in a guarded project, because Claude Code turns it on by default on Windows for claude.ai accounts (tools reference, checked 2026-10-28) and Officina reads Bash only; any other tool gets no decision. The `PreToolUse` matcher doctor prints and requires is built from that constant. Found by the v1 PRD draft; a security fix inside v0.2, not a v1 item. (2.12)
- **D-172.** A closed argument vector does not close what the program does with a value: `ssh` hands the arguments after the destination to the remote shell, and Ansible templates extra vars and module arguments. Phase 2 therefore reports non-blocking notices (`RemoteCommandValue`, `TemplatedValue`) that keep the action, outside the canonical form, shown on the banner, by `relay --json` and by doctor, and the README says which programs read a value. Approval journal lines name their project and action (`dry_run` on requests), optional on read, so later approval metrics are computed from the journal. Found by the Epic 8 story review (F1, F8); the worker-side value refusal stays in Epic 8. (2.13)
