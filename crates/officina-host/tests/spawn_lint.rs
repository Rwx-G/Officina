//! One spawn module is a lint, and the lint has exactly three exceptions
//! (AD-73, D-128 as amended by D-562, review F18): clippy refuses
//! `std::process::Command::new` anywhere in this crate (`clippy.toml`);
//! this test holds `src/` outside its test modules to one `expect` of it
//! in each of `spawn`, `relay::run` and `git`, which no lint can count. It
//! reads the source text on purpose.

use std::path::{Path, PathBuf};

/// The modules where a process starts, as files under `src/`.
const SITES: &[&str] = &["spawn.rs", "relay/run/spawn.rs", "git.rs"];

const EXPECT: &str = "#[expect(";
const LINT: &str = "clippy::disallowed_methods";

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

/// `text` without its top-level `#[cfg(test)]` (or `#[cfg(all(test, ..))]`)
/// modules: from the attribute to the module's closing brace at column 0.
fn outside_test_modules(text: &str) -> String {
    let mut kept = String::new();
    let mut pending: Vec<&str> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let is_test_cfg = line.starts_with("#[cfg(test)]") || line.starts_with("#[cfg(all(test");
        if is_test_cfg || (!pending.is_empty() && line.starts_with("#[")) {
            pending.push(line);
            // A multi-line attribute runs to its closing line.
            if line.starts_with("#[") && !line.trim_end().ends_with(']') {
                for more in lines.by_ref() {
                    pending.push(more);
                    if more.starts_with(")]") || more.trim_end().ends_with(")]") {
                        break;
                    }
                }
            }
            continue;
        }
        if !pending.is_empty() {
            let opens_test_module = pending
                .iter()
                .any(|l| l.starts_with("#[cfg(test)]") || l.starts_with("#[cfg(all(test"))
                && line.starts_with("mod ")
                && line.trim_end().ends_with('{');
            if opens_test_module {
                for inner in lines.by_ref() {
                    if inner == "}" {
                        break;
                    }
                }
                pending.clear();
                continue;
            }
            for held in pending.drain(..) {
                kept.push_str(held);
                kept.push('\n');
            }
        }
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

/// How many `#[expect(clippy::disallowed_methods, ...)]` attributes `text`
/// holds, the attribute written on one line or over several.
fn expects(text: &str) -> usize {
    text.match_indices(EXPECT)
        .filter(|(at, _)| {
            let rest = &text[at + EXPECT.len()..];
            rest.trim_start().starts_with(LINT)
        })
        .count()
}

#[test]
fn command_new_is_expected_in_three_modules_only() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let mut holding: Vec<(String, usize)> = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        let relative = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            !text.contains(&format!("allow({LINT}"))
                && !text.contains(&format!("allow(\n    {LINT}")),
            "{relative}: an allow would hide a spawn from the count; use expect"
        );
        let count = expects(&outside_test_modules(&text));
        if count > 0 {
            holding.push((relative, count));
        }
    }
    holding.sort();
    let mut expected: Vec<(String, usize)> =
        SITES.iter().map(|site| ((*site).to_owned(), 1)).collect();
    expected.sort();
    assert_eq!(holding, expected);
}

/// The skipping above is what makes test modules free to start processes:
/// a test module's own `expect` is not counted, and code after it is.
#[test]
fn test_modules_are_skipped_and_nothing_else() {
    let text = "fn a() {}\n#[cfg(test)]\n#[expect(\n    clippy::disallowed_methods,\n    reason = \"x\"\n)]\nmod tests {\n    #[expect(clippy::disallowed_methods, reason = \"y\")]\n    fn t() {}\n}\n#[expect(clippy::disallowed_methods, reason = \"z\")]\nfn b() {}\n";
    let kept = outside_test_modules(text);
    assert_eq!(expects(text), 3);
    assert_eq!(expects(&kept), 1);
    assert!(kept.contains("fn a()") && kept.contains("fn b()"));
    assert!(!kept.contains("fn t()"));
}
