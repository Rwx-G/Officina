//! `officina-entry pack` (AD-110, D-248): the packer writes the run's tip,
//! a bundle of the commits past the base when there are any, and a
//! manifest, all untrusted, into a fresh `out/`. Its branch, base and clone
//! come from its own command, never from the worker-writable plan.

use std::fs;
use std::io::Read;
use std::path::Path;

use super::{Layout, git};

/// The largest bundle kept (FR-10): past it the bundle is deleted and the
/// manifest says `too_large`.
pub const MAX_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;
/// The most of the steps file the manifest carries.
const MAX_STEPS_BYTES: u64 = 64 * 1024;

/// What `pack` takes from its command (story 6.6's `Packer` vector).
#[derive(Debug, Clone)]
pub struct PackArgs {
    pub branch: String,
    pub base: String,
    /// The clone, a container path (`/work/<project>`).
    pub clone: String,
}

/// Removes `out` without following a symbolic link (review F48): a link or
/// a file there is removed, a folder removed with its content, then `out`
/// is created as a folder.
fn fresh_out(out: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(out) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(out)?,
        Ok(_) => fs::remove_file(out)?,
        Err(_) => {}
    }
    fs::create_dir_all(out)
}

fn git_text(clone: &Path, args: &[&str]) -> Option<String> {
    let out = git(Some(clone)).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Packs the run; 0 when `out/` was written, 1 otherwise.
pub fn pack(layout: &Layout, args: &PackArgs) -> i32 {
    let out = layout.out_dir();
    if let Err(e) = fresh_out(&out) {
        eprintln!("officina-entry pack: {}: {e}", out.display());
        return 1;
    }
    let clone = layout.at(&args.clone);
    let reference = format!("refs/heads/{}", args.branch);
    let tip = git_text(
        &clone,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    );
    let mut too_large = false;
    if let Some(tip) = &tip {
        if fs::write(out.join("tip"), format!("{tip}\n")).is_err() {
            return 1;
        }
        let past = git_text(
            &clone,
            &["rev-list", "--count", &format!("{}..{tip}", args.base)],
        )
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0);
        if past > 0 {
            let bundle = out.join("work.bundle");
            let made = git(Some(&clone))
                .args(["bundle", "create", "--quiet"])
                .arg(&bundle)
                .arg(&reference)
                .arg(format!("^{}", args.base))
                .status()
                .is_ok_and(|s| s.success());
            if made && fs::metadata(&bundle).is_ok_and(|m| m.len() > MAX_BUNDLE_BYTES) {
                let _ = fs::remove_file(&bundle);
                too_large = true;
            }
        }
    }
    let uncommitted = git(Some(&clone))
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=normal"])
        .output()
        .map(|o| {
            o.stdout
                .split(|b| *b == 0)
                .filter(|e| !e.is_empty())
                .count()
        })
        .unwrap_or(0);
    let mut steps = String::new();
    if let Ok(file) = fs::File::open(layout.steps_file()) {
        let _ = file.take(MAX_STEPS_BYTES).read_to_string(&mut steps);
    }
    let manifest = serde_json::json!({
        "steps": steps.lines().collect::<Vec<_>>(),
        "uncommitted": uncommitted,
        "too_large": too_large,
    });
    if fs::write(out.join("manifest.json"), manifest.to_string()).is_err() {
        return 1;
    }
    0
}
