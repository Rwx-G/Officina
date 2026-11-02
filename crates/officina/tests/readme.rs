//! The README against the code it describes (story 2.10 AC 7, D-34, story
//! 3.12 AC 2): every subcommand of the usage text is documented, and no list
//! the code owns is transcribed. The prose itself is reviewed by hand.

mod common;

use common::TestEnv;
use officina_core::relay::catalogue::{CANONICAL_FORMAT, DEFAULT_GUARDED_PROGRAMS, NoticeKind};
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

/// Story 5.20 AC 3 and 6 (D-574): the usage text and the README name the
/// flag that prints the auto-mode blocks.
#[test]
fn usage_and_readme_name_the_permissions_flag() {
    let out = TestEnv::new().run(&["--help"], b"");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("doctor [--floor <path>] [--permissions]")
    );
    assert!(readme().contains("officina doctor --permissions"));
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

/// Story 2.17 AC 6 (D-452): the README's gate entry carries the field and
/// value doctor prints, and the version it names is the one doctor checks.
#[test]
fn readme_states_the_gate_fails_closed() {
    use officina_core::contract::ON_FAILURE_SINCE;
    use officina_host::doctor::ON_FAILURE;
    let readme = readme();
    let (field, value) = ON_FAILURE;
    assert!(
        readme.contains(&format!("\"{field}\": \"{value}\"")),
        "README.md does not show the gate entry's {field}"
    );
    let [major, minor, patch] = ON_FAILURE_SINCE;
    assert!(
        readme.contains(&format!("from {major}.{minor}.{patch}")),
        "README.md does not name the version that honours {field}"
    );
}

/// Story 5.23 AC 6 (AD-181): the README's hooks block shows every
/// registration doctor requires without the gate, `PermissionDenied`
/// included, each event once, and its denials section says Officina never
/// asks for a retry and that the block is to be pasted again.
#[test]
fn readme_shows_every_registration_and_never_a_retry() {
    use officina_host::doctor::registrations;
    use officina_host::relay::journal::DENIED_EVENT;
    let (_, block_lines) = spans_and_block_lines(&readme());
    for registration in registrations(false) {
        let key = format!("\"{}\":", registration.event);
        let shown = block_lines
            .iter()
            .filter(|line| line.trim_start().starts_with(&key))
            .count();
        assert_eq!(
            shown, 1,
            "{} in the README's hooks block",
            registration.event
        );
    }
    let readme = readme();
    let section = readme
        .split("### Auto-mode denials")
        .nth(1)
        .and_then(|rest| rest.split("\n### ").next())
        .expect("an Auto-mode denials section");
    assert!(section.contains(DENIED_EVENT), "{section}");
    assert!(
        section.contains("never asks Claude Code to let the model retry"),
        "{section}"
    );
    assert!(section.contains("paste it again"), "{section}");
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

/// Debt audit H2: the Privacy section says plainly that the triage model,
/// when on, sends excerpts to the Anthropic API through `claude`, which
/// ones (at the hook's own excerpt length), and how to prevent it.
#[test]
fn readme_privacy_names_what_the_triage_model_sends() {
    use officina_core::events::EXCERPT_CHARS;
    let text = readme();
    let privacy: String = text
        .split("\n## ")
        .find(|section| section.starts_with("Privacy"))
        .expect("the README has a Privacy section")
        .to_owned();
    let bullet = privacy
        .lines()
        .find(|line| line.contains("Triage model"))
        .expect("the Privacy section has a triage model entry");
    for needed in [
        "Anthropic API",
        "`claude -p`",
        "last prompt",
        "answer",
        &format!("{EXCERPT_CHARS} characters"),
        "Nothing is sent while the model is off",
        "`no_excerpts`",
        "keep the model off",
    ] {
        assert!(bullet.contains(needed), "missing {needed:?} in: {bullet}");
    }
    let network = privacy
        .lines()
        .find(|line| line.contains("network"))
        .expect("the Privacy section says what reaches the network");
    assert!(network.contains("triage call"), "{network}");
}

/// The registry's README section states its trust boundary, as the
/// relay's does (penetration audit H1, H2).
#[test]
fn readme_states_the_registry_is_no_boundary_against_the_author() {
    let text = readme();
    let registry: String = text
        .split("\n### ")
        .find(|section| section.starts_with("The registry"))
        .expect("the README has a registry section")
        .to_owned();
    assert!(
        registry.contains("not a security boundary against code running as you"),
        "{registry}"
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

/// Story 2.16 AC 2 (story 2.10 AC 8): `[Unreleased]` states the canonical
/// catalogue's format as the code has it, and only that one.
#[test]
fn changelog_states_the_current_catalogue_format() {
    let changelog =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../CHANGELOG.md"))
            .unwrap();
    let unreleased = changelog
        .split("\n## [")
        .find(|section| section.starts_with("Unreleased]"))
        .expect("the CHANGELOG has an [Unreleased] section");
    let stated: Vec<u32> = unreleased
        .split("format ")
        .skip(1)
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .collect();
    assert!(
        !stated.is_empty(),
        "[Unreleased] states no catalogue format"
    );
    assert!(
        stated.iter().all(|format| *format == CANONICAL_FORMAT),
        "[Unreleased] states the formats {stated:?}, the code {CANONICAL_FORMAT}"
    );
}

/// Story 3.12 AC 2 (AD-39): the README tells the reader to exclude the
/// registry's folder from a sync client by naming the clients, never by
/// transcribing the folder markers the code looks for.
#[test]
fn readme_names_no_sync_marker() {
    let text = readme();
    assert!(!officina_host::registry::store::SYNC_ROOT_MARKERS.is_empty());
    for marker in officina_host::registry::store::SYNC_ROOT_MARKERS {
        assert!(!text.contains(marker), "the README transcribes {marker}");
    }
    assert!(
        text.contains("Exclude `registry/` from any sync client"),
        "positive control: the exclusion is stated"
    );
}

/// Story 5.19 AC 2 (D-149): the README describes the variables a session
/// kind drops by their families and names the constant, never one of its
/// entries, which the session-variables entry (research notes E84) owns.
#[test]
fn readme_names_no_session_variable() {
    use officina_core::spawn::SESSION_ENV_REMOVED;
    let text = readme();
    assert!(!SESSION_ENV_REMOVED.is_empty());
    for name in SESSION_ENV_REMOVED {
        assert!(!text.contains(name), "the README transcribes {name}");
    }
    assert!(
        text.contains("`SESSION_ENV_REMOVED`"),
        "positive control: the constant is named"
    );
}

/// Story 6.12 AC 8: the run limits the README states are the constants'.
#[test]
fn readme_states_the_run_limits() {
    use officina_core::workshop::lifecycle::DEADLINE_GRACE_MS;
    use officina_core::workshop::output::{MAX_ALLOWED_HOSTS, OUTPUT_CAP_BYTES};
    use officina_core::workshop::run::MAX_ENTRY_LINE_BYTES;
    use officina_core::workshop::stream::MAX_LINE_BYTES;
    let text = readme();
    let start = text.find("### Following a run and its limits").unwrap();
    let end = start + 4 + text[start + 4..].find("\n### ").unwrap();
    let section = text[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mib = |bytes: u64| bytes / (1024 * 1024);
    for stated in [
        format!("a line over {} MiB is skipped", mib(MAX_LINE_BYTES as u64)),
        format!("over {} KiB is skipped", MAX_ENTRY_LINE_BYTES / 1024),
        format!("for the first {MAX_ALLOWED_HOSTS}"),
        format!("at {} MiB of worker output", mib(OUTPUT_CAP_BYTES)),
        format!(
            "asks again every {} seconds",
            officina_host::workshop::follow::ENGINE_RETRY.as_secs()
        ),
    ] {
        assert!(section.contains(&stated), "missing {stated:?}");
    }
    assert_eq!(DEADLINE_GRACE_MS, 60 * 1000, "the README says one minute");
}
