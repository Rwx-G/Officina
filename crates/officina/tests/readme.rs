//! The README against the code it describes (story 2.10 AC 7, D-34): every
//! subcommand of the usage text is documented, and no list the code owns is
//! transcribed. The prose itself is reviewed by hand.

mod common;

use common::TestEnv;
use officina_core::relay::catalogue::{DEFAULT_GUARDED_PROGRAMS, NoticeKind};
use officina_core::relay::gate::{FILE_TOOLS, SHELL_TOOLS, ToolRule, gated_tools};
use officina_core::relay::run::TREE_BINDING_REFUSALS;

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

/// D-171, D-173: the README points at the shell and file tool constants,
/// transcribes no matcher built from them, and names each tool the gate
/// refuses whole, as the constant says.
#[test]
fn readme_names_the_gate_tools_constant() {
    let readme = readme();
    for constant in ["`SHELL_TOOLS`", "`FILE_TOOLS`"] {
        assert!(
            readme.contains(constant),
            "README.md does not name {constant}"
        );
    }
    assert!(!FILE_TOOLS.is_empty());
    let tools = gated_tools();
    for a in &tools {
        for b in &tools {
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

/// Story 2.13 AC 4 (D-172): the README no longer says that no shell reads a
/// value; its paragraph on the values a program reads names the notice
/// kinds' constant and, for every kind, the program that reads the value,
/// without transcribing the kinds themselves.
#[test]
fn readme_names_the_notice_kinds() {
    let text = readme();
    assert!(
        !text.contains("no shell ever reads it"),
        "the README still says no shell reads a value"
    );
    let paragraph = text
        .lines()
        .find(|line| line.contains("`NoticeKind`"))
        .expect("the README names NoticeKind");
    for kind in NoticeKind::ALL {
        assert!(
            paragraph.contains(kind.program()),
            "the paragraph does not say that {} reads a value",
            kind.program()
        );
        assert!(
            !text.contains(kind.code()),
            "the README transcribes the kind {}",
            kind.code()
        );
    }
}

/// Story 2.14 AC 11 (D-173): the sentence on the file binding names the
/// constant of its refusals and its limits, and the card's session and its
/// pointer-only rule are stated for what they are.
#[test]
fn readme_states_the_binding_and_its_limits() {
    let text = readme();
    let binding = text
        .lines()
        .find(|line| line.contains("`TREE_BINDING_REFUSALS`"))
        .expect("the README names TREE_BINDING_REFUSALS beside the binding");
    for limit in ["files git ignores", "outside the work tree"] {
        assert!(
            binding.contains(limit),
            "the binding does not state {limit:?}"
        );
    }
    assert!(
        TREE_BINDING_REFUSALS
            .iter()
            .any(|reason| !binding.contains(reason.code())),
        "the binding sentence transcribes the refusal codes instead of naming the constant"
    );
    assert!(
        text.contains("advisory, not proof of which session asked"),
        "the README does not say the card's session is advisory"
    );
    assert!(
        text.contains("guards against a stray key, not against code running inside the window"),
        "the README does not say what the pointer-only rule guards against"
    );
}

/// Story 2.15 AC 8: the Privacy section scopes the network to the HTTP
/// actions, and the code agrees: no source file but the HTTP sender names a
/// network API.
#[test]
fn readme_scopes_the_network_to_http_actions() {
    let text = readme();
    let privacy: String = text
        .split("\n## ")
        .find(|section| section.starts_with("Privacy"))
        .expect("the README has a Privacy section")
        .to_owned();
    assert!(
        !privacy.contains("opens no network connection"),
        "the Privacy section says Officina opens no connection at all"
    );
    let sentence = privacy
        .lines()
        .find(|line| line.contains("network"))
        .expect("the Privacy section says what reaches the network");
    assert!(sentence.contains("HTTP actions"), "{sentence}");

    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in [
        "crates/officina-core/src",
        "crates/officina-host/src",
        "crates/officina/src",
        "app/src-tauri/src",
    ] {
        sources(&root.join(dir), &mut files);
    }
    let openers: Vec<String> = files
        .iter()
        .filter(|path| {
            let code = std::fs::read_to_string(path).unwrap();
            ["ureq::", "TcpStream", "UdpSocket", "TcpListener"]
                .iter()
                .any(|api| code.contains(api))
        })
        .map(|path| {
            path.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(openers, ["crates/officina-host/src/relay/http.rs"]);
}
