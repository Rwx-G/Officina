//! The README against the code it describes (story 2.10 AC 7, D-34): every
//! subcommand of the usage text is documented, and no list the code owns is
//! transcribed. The prose itself is reviewed by hand.

mod common;

use common::TestEnv;
use officina_core::relay::catalogue::DEFAULT_GUARDED_PROGRAMS;
use officina_core::relay::gate::{SHELL_TOOLS, ToolRule};

fn readme() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap()
}

/// The README's inline code spans, outside fenced code blocks, and the lines
/// of its fenced code blocks.
fn spans_and_block_lines(text: &str) -> (Vec<String>, Vec<String>) {
    let mut spans = Vec::new();
    let mut block_lines = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if in_block {
            block_lines.push(line.to_owned());
            continue;
        }
        spans.extend(
            line.split('`')
                .skip(1)
                .step_by(2)
                .map(|span| span.trim().to_owned()),
        );
    }
    (spans, block_lines)
}

/// The fixed words of each usage line of `officina --help`: the subcommand
/// and its literal options, up to the first placeholder or optional part
/// (`board --json`, `quota write`, `run --list`).
fn subcommands() -> Vec<String> {
    let out = TestEnv::new().run(&["--help"], b"");
    assert!(out.status.success());
    let usage = String::from_utf8(out.stdout).unwrap();
    let found: Vec<String> = usage
        .lines()
        .filter(|line| {
            line.strip_prefix("  ")
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_lowercase()))
        })
        .map(|line| {
            let synopsis = line.trim().split("  ").next().unwrap_or_default();
            synopsis
                .split_whitespace()
                .take_while(|word| !word.starts_with(['[', '<']))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert!(found.len() > 3, "the usage text was not parsed: {usage}");
    found
}

#[test]
fn readme_mentions_every_subcommand() {
    let readme = readme();
    for subcommand in subcommands() {
        let spelled = format!("officina {subcommand}");
        assert!(
            readme.contains(&spelled),
            "README.md does not mention `{spelled}`"
        );
    }
}

/// The default guarded list lives in core only; the README describes its
/// families and points at `officina doctor`, which prints it. No inline code
/// span names an entry of it, and no fenced example declares a
/// `guarded_programs` list.
#[test]
fn readme_lists_no_default_guarded_program() {
    let (spans, block_lines) = spans_and_block_lines(&readme());
    for entry in DEFAULT_GUARDED_PROGRAMS {
        let stem = entry.trim_end_matches('*');
        for span in &spans {
            assert!(
                !span.eq_ignore_ascii_case(entry) && !span.eq_ignore_ascii_case(stem),
                "README.md names the default guarded program {entry} in the code span `{span}`"
            );
        }
    }
    for line in &block_lines {
        let key = line.trim_start().trim_start_matches('#').trim_start();
        assert!(
            !key.starts_with("guarded_programs"),
            "README.md's example declares a guarded_programs list: {line}"
        );
    }
}

/// D-171: the README points at the shell tool constant, transcribes no
/// matcher built from it, and names each tool the gate refuses whole, as the
/// constant says.
#[test]
fn readme_names_the_gate_tools_constant() {
    let readme = readme();
    assert!(
        readme.contains("`SHELL_TOOLS`"),
        "README.md does not name the constant"
    );
    for (a, _) in SHELL_TOOLS {
        for (b, _) in SHELL_TOOLS {
            if a != b {
                assert!(
                    !readme.contains(&format!("{a}|{b}")),
                    "README.md transcribes the matcher ({a}|{b})"
                );
            }
        }
    }
    for (tool, rule) in SHELL_TOOLS {
        if *rule == ToolRule::DeniedInGuardedProject {
            assert!(
                readme.contains(&format!("{tool} tool")) && readme.contains("refused whole"),
                "README.md does not say the {tool} tool is refused"
            );
        }
    }
}
