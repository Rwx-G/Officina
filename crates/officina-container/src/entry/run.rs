//! `officina-entry run` (AD-108): clone the base, link the mounts, copy
//! the login for a `login` floor, then run `claude` under `timeout` with
//! the plan's vector and the prompt on stdin. Officina adds no setting
//! file, hook, `CLAUDE.md` or MCP server.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::process::{Command, Stdio};

use officina_core::workshop::launch::{RUN_PLAN_SCHEMA, RunPlan};
use officina_core::workshop::run::{EntryLine, EntryStep};
use officina_core::workshop::vector::{CONFIG_DIR, LOGIN_DIR, MOUNTS_DIR};

use super::{Layout, git, report};

/// The grace `timeout` gives `claude` after the deadline (AD-108).
pub const KILL_AFTER_S: u32 = 30;

fn fail(layout: &Layout, step: EntryStep, detail: impl Into<String>) -> i32 {
    report(&EntryLine::failed(step, detail), Some(layout));
    1
}

/// Runs the plan in `layout`; the exit code of `claude`, or 1 on a failed
/// step.
pub fn run(layout: &Layout) -> i32 {
    let plan_path = layout.in_dir().join("plan.json");
    let plan: RunPlan = match fs::read_to_string(&plan_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
    {
        Some(plan) => plan,
        None => return fail(layout, EntryStep::Plan, "plan.json does not parse"),
    };
    if plan.schema != RUN_PLAN_SCHEMA {
        return fail(
            layout,
            EntryStep::Plan,
            format!("unknown plan schema {}", plan.schema),
        );
    }
    let clone = layout.at(&plan.clone_path);
    let bundle = layout.in_dir().join("base.bundle");
    let cloned = git(None)
        .args(["clone", "--quiet", "--branch", &plan.base_branch])
        .arg(&bundle)
        .arg(&clone)
        .status()
        .is_ok_and(|s| s.success());
    if !cloned {
        return fail(layout, EntryStep::Clone, "git clone failed");
    }
    let branched = git(Some(&clone))
        .args(["checkout", "--quiet", "-b", &plan.branch, &plan.base_commit])
        .status()
        .is_ok_and(|s| s.success());
    if !branched {
        return fail(layout, EntryStep::Branch, "git checkout -b failed");
    }
    let head = git(Some(&clone))
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    if head.as_deref() != Some(plan.base_commit.as_str()) {
        return fail(layout, EntryStep::Head, "HEAD is not the base commit");
    }
    report(&EntryLine::ok(EntryStep::Clone), Some(layout));
    if let Err(detail) = link_mounts(layout, &plan) {
        return fail(layout, EntryStep::Mounts, detail);
    }
    report(&EntryLine::ok(EntryStep::Mounts), Some(layout));
    for file in &plan.login_files {
        if let Err(detail) = copy_login_file(layout, file) {
            return fail(layout, EntryStep::LoginMissing, detail);
        }
    }
    if !plan.login_files.is_empty() {
        report(&EntryLine::ok(EntryStep::Login), Some(layout));
    }
    run_claude(layout, &plan)
}

/// Each mount path in the clone's `.git/info/exclude`, then a symbolic link
/// at its place pointing at the mount (AD-108 step 2).
fn link_mounts(layout: &Layout, plan: &RunPlan) -> Result<(), String> {
    let clone = layout.at(&plan.clone_path);
    let info = clone.join(".git").join("info");
    fs::create_dir_all(&info).map_err(|e| e.to_string())?;
    let mut exclude = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(info.join("exclude"))
        .map_err(|e| e.to_string())?;
    for path in &plan.exclude {
        writeln!(exclude, "/{}", path.trim_matches('/')).map_err(|e| e.to_string())?;
    }
    for link in &plan.mount_links {
        let place = clone.join(link.path.trim_matches('/'));
        if let Some(parent) = place.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let target = layout.at(&format!("{MOUNTS_DIR}/{}", link.index));
        symlink(&target, &place).map_err(|e| format!("{}: {e}", link.path))?;
    }
    Ok(())
}

/// `file` copied from the login into the run's config dir with mode 0600
/// (D-606); its content is never printed.
fn copy_login_file(layout: &Layout, file: &str) -> Result<(), String> {
    if file.contains('/') || file.contains("..") {
        return Err(format!("{file} is not a file name"));
    }
    let from = layout.at(LOGIN_DIR).join(file);
    let bytes = fs::read(&from).map_err(|_| format!("{file} is missing from the login"))?;
    let config = layout.at(CONFIG_DIR);
    fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    let mut out = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(config.join(file))
        .map_err(|e| e.to_string())?;
    out.write_all(&bytes).map_err(|e| e.to_string())
}

/// `timeout --kill-after=30 <deadline_s> claude <argv>` in the clone, the
/// prompt on stdin, stdout and stderr inherited (AD-108 step 4).
fn run_claude(layout: &Layout, plan: &RunPlan) -> i32 {
    let prompt = match fs::File::open(layout.at(&plan.prompt_file)) {
        Ok(file) => file,
        Err(e) => return fail(layout, EntryStep::Claude, format!("prompt: {e}")),
    };
    let status = Command::new("timeout")
        .arg(format!("--kill-after={KILL_AFTER_S}"))
        .arg(plan.deadline_s.to_string())
        .arg(&layout.claude)
        .args(&plan.claude_argv)
        .current_dir(layout.at(&plan.clone_path))
        .stdin(Stdio::from(prompt))
        .status();
    let code = match status {
        Ok(status) => status.code().unwrap_or(128),
        Err(e) => return fail(layout, EntryStep::Claude, e.to_string()),
    };
    report(&EntryLine::exited(code), Some(layout));
    code
}
