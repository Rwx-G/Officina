# Story 2.3: Command lexer and gate decision

**Status:** Approved

## Story

As the author who marked a repository guarded,
I want every `Bash` command of its sessions classified by a lexer that finds the program in command position, through quotes, wrappers and interpreters,
so that a raw `ansible-playbook` or `ssh` is refused while ordinary commands pass untouched.

## Acceptance Criteria

1. `core::relay::shell::lex` implements the quoting, separator, redirection, heredoc, dynamic-text, command-position and program-name rules of AD-27; input over 256 KiB, an unterminated quote and recursion deeper than 3 are errors. Proof: unit tests `crates/officina-core/src/relay/shell.rs::tests::{single_quotes_are_literal, double_quote_escapes, unterminated_quote_is_an_error, oversized_input_is_an_error, depth_over_three_is_an_error}`.
2. `core::relay::gate::decide(command, guarded_programs, protected_dirs, cwd_guarded)` returns no decision or a deny with its kind and the matched program, applying AD-26 step 4 in a guarded repository (lexer error, guarded program in command position, dynamic command word, `OFFICINA_` in the text, a protected directory of D-13 named with either slash and any case) and never denying outside one (AD-26 steps 3 to 5, PRD amendment 6). Proof: vector set `tests/vectors/gate.json` (`command`, `guarded_programs?`, `cwd_guarded`, `expect` as `"none"` or the deny kind, `matched?`, `limit?`) replayed by `crates/officina-core/tests/gate_vectors.rs::every_case_matches`.
3. Non-literal command words (D-10): a command-position word containing an unquoted `*`, `?`, `[`, a brace expansion, `$'...'`, `$"..."` or any expansion is dynamic and denied. Proof: `gate.json` cases `ss?`, `ss[h]`, `ss{h,}`, `/usr/bin/ss? host`, `$'\x73sh' host`, `$"ssh" host`, each `expect: "dynamic"`.
4. Word joining (D-10): quote concatenation (`s"s"h`, `"ss"h`, `s'sh'`), a leading backslash (`\ssh`) and backslash-newline inside a word (`ss\<newline>h`) are joined by the lexer into the word bash runs, then matched. Proof: `gate.json` cases for each form, `expect: "guarded_program"`, `matched: "ssh"`; and `shell.rs::tests::joined_words_equal_what_bash_runs`.
5. Wrapper options (D-10): each wrapper has a table of the options that take a value, so the command word after `timeout -s KILL 10`, `nice -n 5`, `xargs -n1`, `xargs -I{}`, `env -u NAME`, `sudo -u user01`, `stdbuf -o L`, `watch -n 5` and `exec -a name` is found; an unknown option of a wrapper is assumed to take no value, and the case is a vector. Proof: one `gate.json` case per entry of each wrapper's table, enforced by AC 9.
6. Interpreter variants (D-10): sh-family with any short-option cluster containing `c` (`bash -lc`, `sh -ec`, `bash -xc`) and with `-o pipefail -c`; `python`, `python3`, `py`, `pythonw` `-c`; `node -e`, `-p`, `--eval`, `--print`; `perl -e`, `-E`; `powershell` and `pwsh` `-Command` and `-c` in any case are scanned; `powershell -EncodedCommand`, `-enc`, `-e` and every other abbreviation of `-EncodedCommand` are denied outright (`expect: "encoded_command"`); `cmd /c`, `/C`, `/k`, `/K` are scanned after caret removal (`cmd /c s^sh`). `coproc` is a reserved word. Proof: `gate.json` cases for each form, enforced by AC 9.
7. Default guarded list (D-11): Windows `bash.exe` and the distro launchers of D-11 are in the core constant, so `bash.exe -c ...`, `C:/Windows/System32/bash -c ...`, `ubuntu.exe run ...` and `debian run ...` are denied. `bash.exe` is matched on the word as written, before AD-27's `.exe` stripping (D-11), so a bare `bash` (Git Bash) stays a shell interpreter lexed recursively. Proof: `gate.json` cases for each form including bare `bash -c 'ls'` (`expect: "none"`), enforced by AC 9 iterating the constant.
8. Home redirection (D-12): an `officina` command word preceded in the same text by an assignment to `USERPROFILE`, `HOME`, `LOCALAPPDATA`, `APPDATA` or any `OFFICINA_*` name (prefix assignment, `env NAME=...`, `export`, `declare`) is denied. Proof: `gate.json` cases with `expect: "home_redirection"`, one per name of the constant, enforced by AC 9.
9. The vector set cannot age: `gate_vectors.rs::every_constant_appears_in_a_case` iterates the core constants (default guarded programs, separators, reserved words, redirection operators, wrappers and each wrapper's value-taking options, shell interpreters, interpreter flag forms, encoded-command spellings, home-redirection names, deny kinds) and fails when one appears in no case (AD-27, D-10).
10. Honest limits: what AD-27 says a string match cannot catch, plus a `cd` inside the same command (`cd ../other && ssh host` from a free folder), are cases with `expect: "none"` and `"limit": true`, so a later improvement shows up as a vector change, never silently. Proof: `gate_vectors.rs::limits_are_flagged` (a `limit` case must expect no decision) and the README limits paragraph pointing at AD-27 (story 2.10).
11. `gate::normalise_cwd` gives one form for `C:\x`, `C:/x` and Git Bash `/c/x`; `gate::guarded_project` finds the project by longest prefix in a given project set (AD-26 step 2). Proof: `crates/officina-core/src/relay/gate.rs::tests::{cwd_forms_normalise_alike, longest_prefix_wins, outside_every_project_is_none}`. How the project sets combine is story 2.4 (D-14 to D-16).
12. `gate::deny_reason` names the program and the project, lists the actions it is given as `officina run <name> <param>=<...>` with unavailable classes marked (D-17), points to `officina run --list`, and is cut at 2 KiB on a character boundary (AD-26). Proof: `gate.rs::tests::{reason_lists_actions_and_the_list_command, reason_marks_unavailable_classes, reason_is_cut_at_2_kib_on_a_char_boundary}`.
13. Lexer and gate use no regex and no hash (AD-28). Proof: `gate.rs::tests::gate_modules_import_no_regex_or_hash`, which scans the two modules' source (`include_str!`) for `regex` and `sha2` paths.
14. Lexing is linear: a 256 KiB input built to stress quoting and nesting completes, and the lexer's step count for a 128 KiB input is under 2.2 times the count for a 64 KiB input of the same shape. Proof: `shell.rs::tests::step_count_grows_linearly` (counts steps, not wall time, so it is deterministic on any runner).

## Tasks

- [ ] `crates/officina-core/src/relay/shell.rs`: tokens, word joining, command-position tracking, wrapper option tables, interpreters, heredocs, recursion, step counter (AC 1, 3-6, 14)
- [ ] Constants for every family of AD-27 and D-10 to D-12, each iterable (AC 3-9)
- [ ] Extend the default guarded list constant of story 2.1 per D-11 (AC 7)
- [ ] `crates/officina-core/src/relay/gate.rs`: `decide`, `normalise_cwd`, `guarded_project`, `deny_reason` (AC 2, 8, 11-13)
- [ ] Write `tests/vectors/gate.json` by hand (redacted) and `crates/officina-core/tests/gate_vectors.rs` (AC 2-10)
- [ ] Update `CHANGELOG.md` `[Unreleased]` (D-34)

## Dev Notes

- Requirements: FR-6 as amended (amendments 6 and 9: the PRD's "deny `officina` approval commands" bullet goes, since no CLI command approves). Decisions: AD-26 (decision order), AD-27 (lexer), AD-28 (no regex, no hash). Epic decisions: D-10 to D-13, D-17. Risk: R15 (false positives stall guarded sessions: the reason lists the actions).
- The PRD's "single `officina run` without operators" rule is dropped by AD-26 step 5: `officina run ... | head` gets no decision.
- D-10 errs toward denying in a guarded repository: a glob in command position is rare in legitimate use, and a false positive costs one retry with the listed actions. A false negative costs containment.
- Wrapper tables follow each tool's documented options; when a tool has an option that takes a value and is not in its table, the lexer reads the value as the command word and may miss the real one: add the option and its vector, never a generic "skip one word" rule.
- D-11's `bash.exe`: a session on Windows can call the WSL `bash.exe` from `System32`, which runs the command in the default distro without `wsl`. Git Bash's own `bash` is an interpreter and is lexed recursively instead. The program-name rule therefore has two steps: entries that need the written form (`bash.exe`) are matched first, then `.exe` is stripped for the others. Use the vector set to show both.
- `protected_dirs` (D-13) is built by the host from the home in story 2.4; this story takes it as an argument.
- Deny kinds are the `expect` vocabulary of `gate.json` and the `matched?` and journal vocabulary of story 2.4; keep them a closed enum.
- Vector cases are built from the rules, never pasted from a real session (no employer hostnames, groups or paths).

## Testing

- Core unit: `crates/officina-core/src/relay/{shell,gate}.rs`.
- Core integration: `crates/officina-core/tests/gate_vectors.rs` replaying `tests/vectors/gate.json`.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-27 created
- 2026-10-27 reworked after review: D-10 (non-literal words, joining, wrapper tables, interpreter variants, encoded commands, `coproc`, carets), D-11 (`bash.exe`, distro launchers), D-12 (home redirection), linear bound made exact
