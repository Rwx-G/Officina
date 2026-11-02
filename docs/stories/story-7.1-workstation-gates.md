# Story 7.1: Workstation gates before Slice A

**Status:** Approved

## Story

As the author whose governor will decide on numbers nobody has yet read from my own account,
I want gates 1, 2, 3 and 6 of PRD section 8 run on my workstation, each recorded as an E-entry that says what was seen and who ran it,
so that every Slice A story implements an observed fact about the usage probe, the statusline, a worker's spend and the local clock, and a failed gate reshapes its FR before code is written (FR-0).

## Acceptance Criteria

1. **Gate 1: the probe over a working day** (AD-149 row 1, PRD question 1 as amended by amendment 14, D-301, D-303; agent, then owner). The agent writes a scratch script, never committed, outside the repository, that runs the exact `UsageProbe` argument vector of AD-134 (`-p /usage --output-format stream-json --verbose --model haiku --tools "" --strict-mcp-config --mcp-config {"mcpServers":{}} --setting-sources "" --settings {"crossSessionInbound":"refuse"} --no-session-persistence`), each time from a fresh empty folder, stdin null, with the environment D-1206 gives the probe (the names of `CONTAINED_ENV_SYSTEM` and `CONTAINED_ENV_NETWORK` found in the agent's environment, none of `CONTAINED_ENV_AUTH`, the floor's `CLAUDE_CONFIG_DIR`, `NoDefaultCurrentDirectoryInExePath=1` and `CLAUDE_CODE_DISABLE_ADVISOR_TOOL=1`; so none of the parent session's names reaches the child; names printed, values never), a 30 s deadline and stdout read at most 256 KiB. It runs every 10 minutes from 09:00 to 19:00 local time on a working day, plus, every 2 hours, a burst of one probe per 2 minutes for 10 minutes. Per probe it records the start, the exit code, the elapsed time, `num_turns`, `local_command`, `total_cost_usd`, the row kinds with their percents and resets, and whether an entry of the two degraded phrases of S3, compared case-insensitively, matched; the report's text is matched in memory and never written anywhere. Once in the day it runs the same vector without `--setting-sources ""`, then without `--tools ""`, and records whether either changes the outcome. If the prompt form fails, the same day uses `--input-format stream-json` with one user message `/usage`. The owner records whether his interactive `/usage` showed last-known data or a rate limit that day, with its exact wording if so. Pass: every probe clean or backed off, structural failures under 10 %, the owner's `/usage` unaffected. Proof: the research notes' v0.7 gate 1 entry, naming D-303's branch that follows (the vector form, the budget's numbers, the observed phrases for `USAGE_DEGRADED_MARKERS`).
2. **Gate 2: the statusline against the probe** (AD-149 row 2, PRD question 2, D-303; owner). The owner adds `officina quota write --tee` in front of his statusline command in his own user settings (Officina never writes them; the agent gives the exact line to paste), opens one interactive session on the floor and sends a prompt; within one minute of the quota file's `written_at`, he runs the probe command line the agent prepared. Three times over a day. Recorded per window: the file's `used_percentage` and `resets_at`, the probe's `percent` and `resets_at`, the reset difference; whether the statusline's percent carries fractions; each probe `percent` beside the file's fractional value for the same window, which says whether the server floors or rounds its whole points (D-322, review F16); the `weekly_scoped` rows' scope kinds (labels not copied). Pass: resets within 15 minutes (`RESET_TOLERANCE_MS`), percents within 1 point. Proof: the research notes' v0.7 gate 2 entry.
3. **Gate 3: a worker's spend on the floor's account** (AD-149 row 3, PRD question 3, D-303, D-304, D-321; owner). Once v0.6's Slice A has closed (story 6.20), on the personal floor's machine (D-1216), with no other session of the account working: the owner records the floor's `[workshop] auth` (`oauth-token` or `login`, D-1204); runs a probe; clicks one v0.6 workshop run on a scratch repository with a fixed multi-step prompt the agent writes; runs a probe within 5 minutes after the run ends. Recorded: both readings per window, the rise bounds (D-322), the run's budget (`max_usd`, a soft cap, D-1205) and, for display only, its `result` cost estimate (P-64), the points per USD of the run's budget per window, the run's measured points per window, and whether the worker's stream carried a `rate_limit_event`, with its `status`, `utilization` and `resetsAt` beside each window's reading. Pass: the 5-hour and weekly percents rise. Proof: the research notes' v0.7 gate 3 entry, naming the floor's credential kind (one regime, D-1204) and D-303's branch for FR-13's stream stop.
4. **Gate 6, first half** (AD-149 row 6, D-302; agent). A scratch Rust program, never committed, built against `windows-sys` 0.61.2 with feature `Win32_System_Time`, converts now, 2026-10-25T00:59Z, 2026-10-25T01:00Z, 2027-03-28T00:59Z and 2027-03-28T01:00Z with `GetDynamicTimeZoneInformation` then `SystemTimeToTzSpecificLocalTimeEx`, and prints each offset in minutes beside the clock's local time and the zone's published transition rules. Pass: the current offset matches the clock and the four instants match the zone's rules. The second half closes story 7.3. Proof: the research notes' v0.7 gate 6 entry (first half).
5. **Nothing left behind** (AD-149 preamble, D-301). No probe process remains after each run of the script (the process list by name before and after the day); scratch folders lie outside the repository; the agent touches no settings file and no `.claude` folder, shown by a listing of the Claude config dir's file names, sizes and modification times before and after the agent's probes (no content read), with what `claude` itself wrote while probing recorded apart (names, sizes, times), since it says whether probing refreshes the cache the author's `/usage` reads (R69, review F29); the repository's working tree changes only in `docs/research-notes.md`. Proof: the listings in the v0.7 gate 1 entry; `git status` at the end of the story.
6. **Recording rules** (D-301). Each of the v0.7 gate 1, 2, 3 and 6 entries says verified or documented, the Claude Code version, the Windows version and the date, who ran it (agent or owner), the procedure, what was seen and the outcome against AD-149's pass column; scratch names and placeholder paths only (`CLAUDE.md` *Provided data redaction*); no environment value and no line of the usage report's text appear. Proof: review of `docs/research-notes.md`.

## Tasks

- [ ] Scratch probe script with the exact vector, the environment removals and the in-memory marker match; the working day; the two variants; the fallback form if needed (AC 1)
- [ ] Ask the owner about his interactive `/usage` that day (AC 1)
- [ ] Exact `--tee` line and probe command line for the owner; three readings (AC 2)
- [ ] Fixed multi-step prompt and the gate 3 checklist for the owner; the run and its brackets (AC 3)
- [ ] Scratch time zone program; the v0.7 gate 6 entry's first half (AC 4)
- [ ] Before and after listings (AC 5)
- [ ] Research notes: the v0.7 gate 1, 2, 3 and 6 entries (AC 6)

## Dev Notes

- Reworked 2026-11-02 (D-1200 to D-1206, D-1216): entries are named by subject and take the next free number; the probe child's environment is v0.5's contained allow list without the auth names, plus the advisor switch (D-692), exactly as story 7.8 will build it; every gate runs on the home PC, the personal floor's machine; the regime is the subscription one, so gate 3 records the kind and never selects a regime. GitHub CI is off (D-1201): every CI job named here is that job replayed by the local full gate.
- Requirements: FR-0, PRD section 8 questions 1, 2, 3 and 6 as amended (amendments 12 and 14), section 7's Probe row. Decisions: AD-129, AD-133, AD-134, AD-135, AD-149. Facts: addendum section 1, items 2 to 6 (the prompt form, the report's shape, a clean probe's zero cost, the floor's own login, the .NET view of the zone); E3 (statusline). Risks: R66 to R70, R72, R79, R80. Epic decisions: D-80 (Epic 3); D-125, D-128 (Epic 5); D-201 (Epic 6); P-62, P-64, P-65, D-301 to D-304, D-320, D-321, D-322.
- Research-note entries take the next free number when written and are named by subject (D-1200); this story writes the v0.7 gate 1, 2, 3 and 6 entries.
- Order: this story precedes the stories whose facts it records (FR-0 as D-1203 reads it); 7.2, 7.4, 7.5, 7.6 and 7.21 do not wait for it. The v0.7 gate 1 and 2 entries before 7.8; the v0.7 gate 3 entry before 7.9 and before any Slice B story; the v0.7 gate 6 entry's second half closes 7.3. Gates 0, 4 and 5 are 7.13's.
- Gate 3 needs v0.6's Slice A closed (story 6.20) and one clicked workshop run. Gates 1, 2 and 6 need neither: they run once the owner wires his statusline (gate 2) and the agent has a working day (gate 1). Stories 7.2, 7.4, 7.5, 7.6 and 7.21 rest on no gate fact and may start before this story closes (D-1203).
- The agent runs inside a Claude Code session, so its children carry that session's identity and messaging token. Every probe child has those names removed exactly as D-128 and D-318 will do in code; otherwise the gate would test a child Officina never spawns.
- The usage report's text attributes the author's recent local usage to skills, subagents and files (addendum section 1, item 4). It is matched in memory for the two phrases and dropped: never copied into the notes, a log or a file (R79).
- A clean probe costs nothing (addendum section 1, item 2); `--model haiku` bounds the cost of a release that sends `/usage` to a model. Gate 3's worker run costs what a small workshop run costs.
- The scratch script types the probe vector and S3's two phrases before the constants exist, which is acceptable for a gate; story 7.8 compares the v0.7 gate 1 entry's recorded vector once with its golden case.
- A gate that fails is recorded as such; then D-303 says what changes, and `epic-7-decisions.md` gains the fact in the same change as the code that follows it.
- No employer content: the floor is "the personal floor", the scratch repository a placeholder name.

## Testing

- Manual, recorded in `docs/research-notes.md` (the v0.7 gate 1, 2, 3 and 6 entries). No code changes in this story.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created
- 2026-10-28 reworked after review: entries renumbered into Epic 7's range E60 to E79 (F7, superseded by the owner's range); gate 2 records floor or round of the probe's whole points (F16); gate 3 records the cap and rise bounds, the reported cost for display only (F1, P-64); the probe's own config-dir writes recorded apart (F29)
- 2026-11-02 reworked against v0.4 to v0.6 as built: entries named by subject (D-1200); the probe's environment as v0.5 built contained kinds (D-1206); gate 3 on the personal floor once v0.6's Slice A closes, with its credential kind and the soft budget (D-1204, D-1205, D-1216); gates hold only the stories whose facts they are (D-1203)
