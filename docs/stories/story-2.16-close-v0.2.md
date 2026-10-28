# Story 2.16: Close v0.2

**Status:** Approved

## Story

As the author who does not carry debt from one version to the next,
I want the items the Epic 2 QA report found still open fixed or recorded,
so that v0.2 closes on code with documentation, tracking and tests that say what is true.

## Acceptance Criteria

1. The six quality Medium findings and the five debt Low findings that `docs/qa/epic-2-report.md` lists as left open (section Audits, and Open items) are each fixed, or recorded in `docs/backlog.md` with the reason they wait; the report's list is the checklist, and the story's Completion Notes map each finding to its commit or backlog line. Proof: per finding, the test the fix needs (a refactor keeps the suite green; a behaviour change gets a named test).
2. `CHANGELOG.md` `[Unreleased]` is one coherent set describing the code as it is (story 2.10 AC 8): the fingerprint as story 2.14 built it (file contents of tracked and unignored files, no `HEAD`, no diff), the doctor matcher naming the shell and file tools, one catalogue format number (the current one), no entry contradicting another. Proof: `crates/officina/tests/readme.rs` gains a case that the CHANGELOG's format number equals the core constant, if the CHANGELOG states one.
3. `docs/stories/sprint-status.yaml`: the Epic 2 header comment describes the epic's real state; stories 2.4, 2.10, 2.11 and 2.12 are `in-progress` while their manual check is open (Epic 1's convention), the others `review`. Proof: YAML parses; the header names the open manual checks by story.
4. A differential test of the gate's lexer against real Git Bash, that never executes a generated command: each generated or corpus command is wrapped as a function body (`f() { <command>; }`) and Git Bash prints its own parse with `declare -f f` (parse only, nothing runs); the test compares the command words and assignments bash sees with what the lexer reports, and every disagreement on a guarded program is a failure. The corpus is `tests/vectors/gate.json` plus generated variants (quoting, escapes, line continuations, heredocs, process substitution); the run is `#[ignore]`d, run by a new step of the windows CI job, and skipped with a message where Git Bash is absent. Proof: `crates/officina/tests/lexer_differential.rs`; the step in `.github/workflows/ci.yml`; real misses found become `gate.json` cases.
5. A defensive pass over story 2.11's URL builder and credential binding, by reading and by tests: placeholder values that change the host, port, scheme or path segment count (percent-encoding, `@`, `\`, `//`, IDN and punycode, IPv6 literals, trailing dots, uppercase hosts), and credential host matching against each. Each case is a test in the core URL module's vector set; every real miss is fixed. Proof: the URL vector set gains the cases; the coverage test iterates them.
6. A delta security audit of what changed since the first audits (stories 2.11, 2.13, 2.14, 2.15 and this story) is written to `_bmad-output/planning-artifacts/qa/epic-2-security-delta.md` (git-ignored) by the reviewer the coordinator runs; its High and Medium findings are fixed in this story or recorded with a reason. Proof: the Completion Notes list each finding and its outcome.

## Tasks

- [ ] Report leftovers: fix or record each (AC 1)
- [ ] CHANGELOG coherence and its readme test (AC 2)
- [ ] Sprint status header and statuses (AC 3)
- [ ] Lexer differential test, corpus generator, CI step (AC 4)
- [ ] URL and credential vector cases, fixes (AC 5)
- [ ] Delta audit findings (AC 6)

## Dev Notes

- Source: `docs/qa/epic-2-report.md` (Open items, Verdict) and the git-ignored audit reports under `_bmad-output/planning-artifacts/qa/`.
- AC 4 must never run a generated command: `declare -f` prints the function body bash parsed; build the script that defines `f` with the command as data (a file passed to `bash --norc --noprofile`, never interpolated into a `-c` string), with a timeout, no network, in a temp folder.
- Decisions: D-175 (this story), D-174.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created from the Epic 2 QA report's open items (D-175)
