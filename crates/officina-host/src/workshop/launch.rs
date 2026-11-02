//! A workshop card's facts and its launch on the host (AD-107, D-239): the
//! git, engine, proxy and credential-presence facts thread `workshop`
//! reads for a card and again at the click, then the launch's steps. The
//! collector never runs `docker` or the workshop's `git` (AD-3): it hands
//! a [`WorkshopTicket`] to thread `workshop`, and the facts or the launch's
//! report come back to it as requests it never drops.

use std::path::{Path, PathBuf};

use officina_core::orders::workshop_card::MountState;
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::launch::CredentialState;
use officina_core::workshop::scan::{Needles, needles};
use officina_core::workshop::vector::GitIdentity;

use crate::relay::credential::{CredentialStore, Secret};
use crate::workshop::inventory::WorkshopGit;

/// The commit identity of a worker when the project's `git` names none.
pub const FALLBACK_GIT_NAME: &str = "Officina workshop";
/// Its address: `.invalid` never routes (RFC 2606).
pub const FALLBACK_GIT_EMAIL: &str = "workshop@officina.invalid";

/// The name a floor's workshop credential is stored under, read through
/// story 2.11's store as `officina:<name>` (AD-105, D-604).
pub fn credential_store_name(floor_key: &str) -> String {
    officina_core::workshop::names::credential_target(floor_key)
        .strip_prefix("officina:")
        .map(str::to_owned)
        .unwrap_or_default()
}

/// Where a floor's credential state comes from (D-607). This story's
/// source answers `unchecked`; story 6.26 folds the journal's
/// `workshop_credential` lines.
pub trait CredentialStateSource {
    fn state(&self, floor_key: &str, kind: CredentialKind) -> CredentialState;
}

/// The source until story 6.26: nothing is known of the credential.
#[derive(Debug, Clone, Copy, Default)]
pub struct UncheckedStates;

impl CredentialStateSource for UncheckedStates {
    fn state(&self, _floor_key: &str, _kind: CredentialKind) -> CredentialState {
        CredentialState::Unchecked
    }
}

/// Whether the floor's credential is stored (AD-105): read and dropped at
/// once, never shown. A `login` floor carries none.
pub fn credential_present(
    store: &dyn CredentialStore,
    kind: CredentialKind,
    floor_key: &str,
) -> bool {
    kind != CredentialKind::Login
        && store
            .present(&credential_store_name(floor_key))
            .unwrap_or(false)
}

/// A run's needles (AD-112, D-230), kept in the window's memory while it
/// follows the run. Like [`Secret`], it prints nothing and serialises to
/// nothing: it has no `Display` and no `Serialize`.
///
/// ```compile_fail
/// fn wants<T: serde::Serialize>(_: &T) {}
/// let found = officina_host::workshop::launch::RunNeedles::of(
///     officina_core::workshop::floor::CredentialKind::OauthToken,
///     &officina_host::relay::credential::Secret::new("x".repeat(40)),
/// ).unwrap();
/// wants(&found);
/// ```
pub struct RunNeedles(Needles);

impl RunNeedles {
    /// The needles of a token kind's credential; none for `login`, whose
    /// value the host never holds (D-609).
    pub fn of(kind: CredentialKind, secret: &Secret) -> Option<RunNeedles> {
        (kind != CredentialKind::Login).then(|| RunNeedles(needles(secret.expose().as_bytes())))
    }

    /// The forms story 6.12 replaces in every untrusted text.
    pub fn needles(&self) -> &Needles {
        &self.0
    }
}

impl std::fmt::Debug for RunNeedles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RunNeedles(<redacted>)")
    }
}

#[cfg(feature = "window-authority")]
pub use crate::workshop::follow::FollowedRuns;

/// A character the engine's `--mount` cannot carry in a source path
/// (D-236): its options are comma-separated and may be quoted.
fn unsupported_char(c: char) -> bool {
    c == ',' || c == '"' || c == '\'' || c.is_control()
}

#[cfg(windows)]
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

/// A path compared as Windows compares it: case folded, separators
/// unified, the verbatim prefix gone (D-231).
fn compared(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text).to_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// The source path of mount `mount` of the project at `project_dir`, as
/// the vector names it.
pub fn mount_source(project_dir: &Path, mount: &str) -> String {
    project_dir.join(mount).to_string_lossy().replace('\\', "/")
}

/// What the host finds of mount `mount` (FR-2, D-231, D-236): a path the
/// engine can carry, no reparse point from the project folder down, a
/// folder, inside the project once canonical, ignored by a rule (untracked
/// rules included), and holding no file `base_commit` tracks.
pub fn mount_state(
    project_dir: &Path,
    mount: &str,
    git: Option<&WorkshopGit>,
    base_commit: Option<&str>,
) -> MountState {
    let source = mount_source(project_dir, mount);
    if source.chars().any(unsupported_char) {
        return MountState::PathUnsupported;
    }
    let mut at = project_dir.to_path_buf();
    for part in Path::new(mount).components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if is_reparse_point(&meta) => return MountState::ReparsePoint,
            Ok(_) => {}
            Err(_) => return MountState::Missing,
        }
    }
    if !at.is_dir() {
        return MountState::Missing;
    }
    match (
        std::fs::canonicalize(project_dir),
        std::fs::canonicalize(&at),
    ) {
        (Ok(root), Ok(found)) => {
            let root = compared(&root);
            if !compared(&found).starts_with(&format!("{}/", root.trim_end_matches('/'))) {
                return MountState::ReparsePoint;
            }
        }
        _ => return MountState::Missing,
    }
    let Some(git) = git else {
        return MountState::NotIgnored;
    };
    if !ignored(git, project_dir, mount) {
        return MountState::NotIgnored;
    }
    match base_commit.map(|commit| tracked_under(git, project_dir, commit, mount)) {
        Some(Ok(false)) | None => MountState::Ready,
        Some(Ok(true) | Err(())) => MountState::Tracked,
    }
}

/// True when `mount` is ignored by a rule, `.git/info/exclude` and
/// `core.excludesFile` included (FR-2), case-insensitively on Windows
/// (D-231). Any answer but "ignored" is not ignored.
fn ignored(git: &WorkshopGit, dir: &Path, mount: &str) -> bool {
    let path = format!("{}/", mount.trim_end_matches('/'));
    let mut args = Vec::new();
    if cfg!(windows) {
        args.extend(["-c", "core.ignorecase=true"]);
    }
    args.extend(["check-ignore", "-q", "--no-index", "--", &path]);
    crate::workshop::git::run(&git.git, dir, &args, &git.base_env, &git.hooks_dir, None)
        .is_ok_and(|ran| ran.status.success())
}

/// True when `commit` tracks a file under `mount` (review F3), compared
/// case-insensitively on Windows (D-231); an error when the listing could
/// not be read, which the caller takes as tracked.
fn tracked_under(git: &WorkshopGit, dir: &Path, commit: &str, mount: &str) -> Result<bool, ()> {
    let listed = git
        .output(
            dir,
            &["ls-tree", "-r", "-z", "--name-only", "--full-tree", commit],
        )
        .map_err(|_| ())?;
    let prefix = compared(Path::new(&format!("{}/", mount.trim_end_matches('/'))));
    Ok(String::from_utf8_lossy(&listed)
        .split('\0')
        .any(|name| !name.is_empty() && compared(Path::new(name)).starts_with(&prefix)))
}

/// How many changes `git status` lists in the project (AD-107), which a
/// run never carries: `--porcelain=v1 -z`, a rename's or copy's second
/// path not counted twice.
pub fn uncommitted(git: &WorkshopGit, dir: &Path) -> Option<u32> {
    let out = git
        .output(
            dir,
            &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
        )
        .ok()?;
    let text = String::from_utf8_lossy(&out);
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    let mut count = 0u32;
    while let Some(entry) = fields.next() {
        count = count.saturating_add(1);
        if matches!(entry.as_bytes().first(), Some(b'R' | b'C')) {
            fields.next();
        }
    }
    Some(count)
}

/// The commit identity the project's `git` names, else Officina's own.
pub fn git_identity(git: &WorkshopGit, dir: &Path) -> GitIdentity {
    let read = |key: &str| {
        git.output(dir, &["config", "--get", key])
            .ok()
            .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
            .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
    };
    GitIdentity {
        name: read("user.name").unwrap_or_else(|| FALLBACK_GIT_NAME.to_owned()),
        email: read("user.email").unwrap_or_else(|| FALLBACK_GIT_EMAIL.to_owned()),
    }
}

/// The run's folder under the workshop's (AD-124).
pub fn run_dir(home: &Path, run_id: &str) -> PathBuf {
    crate::workshop::runs_dir(home).join(run_id)
}

#[cfg(feature = "window-authority")]
pub use window::*;

#[cfg(feature = "window-authority")]
mod window;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn repo() -> (tempfile::TempDir, WorkshopGit) {
        let home = tempfile::tempdir().unwrap();
        let (git_bin, _) = crate::workshop::git::resolve().expect("git for the tests");
        let hooks_dir = crate::workshop::git::ensure_empty_hooks(home.path()).unwrap();
        let git = WorkshopGit {
            git: git_bin,
            hooks_dir,
            base_env: std::env::vars().filter(|(k, _)| k != "GIT_DIR").collect(),
        };
        let dir = home.path().join("project");
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "User One"],
            vec!["config", "user.email", "user01@example.com"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            git.output(&dir, &args).unwrap();
        }
        (home, git)
    }

    pub(crate) fn commit(git: &WorkshopGit, dir: &Path, message: &str) -> String {
        git.output(dir, &["add", "-A"]).unwrap();
        git.output(dir, &["commit", "-q", "--allow-empty", "-m", message])
            .unwrap();
        String::from_utf8(git.output(dir, &["rev-parse", "HEAD"]).unwrap())
            .unwrap()
            .trim()
            .to_owned()
    }

    #[test]
    fn tips_and_uncommitted_count_come_from_git() {
        let (home, git) = repo();
        let dir = home.path().join("project");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        let tip = commit(&git, &dir, "one");
        assert_eq!(
            git.branch_commit(&dir, "main").as_deref(),
            Some(tip.as_str())
        );
        assert_eq!(git.branch_commit(&dir, "develop"), None);
        assert_eq!(uncommitted(&git, &dir), Some(0));
        std::fs::write(dir.join("a.txt"), "changed").unwrap();
        std::fs::write(dir.join("new.txt"), "new").unwrap();
        git.output(&dir, &["mv", "a.txt", "b.txt"]).unwrap();
        assert_eq!(uncommitted(&git, &dir), Some(2), "a rename counts once");
        assert_eq!(
            git_identity(&git, &dir),
            GitIdentity {
                name: "User One".into(),
                email: "user01@example.com".into()
            }
        );
    }

    #[test]
    fn a_force_added_file_in_a_mount_is_tracked() {
        let (home, git) = repo();
        let dir = home.path().join("project");
        std::fs::write(dir.join(".gitignore"), ".claude/\n").unwrap();
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::fs::write(dir.join(".claude").join("settings.json"), "{}").unwrap();
        let clean = commit(&git, &dir, "ignore");
        assert_eq!(
            mount_state(&dir, ".claude", Some(&git), Some(&clean)),
            MountState::Ready
        );
        git.output(&dir, &["add", "-f", ".claude/settings.json"])
            .unwrap();
        let forced = commit(&git, &dir, "force");
        assert_eq!(
            mount_state(&dir, ".claude", Some(&git), Some(&forced)),
            MountState::Tracked
        );
        assert_eq!(
            mount_state(&dir, "absent", Some(&git), Some(&forced)),
            MountState::Missing
        );
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        assert_eq!(
            mount_state(&dir, "plain", Some(&git), Some(&forced)),
            MountState::NotIgnored
        );
    }

    #[test]
    fn a_mount_ignored_only_by_exclude_is_ready() {
        let (home, git) = repo();
        let dir = home.path().join("project");
        let base = commit(&git, &dir, "empty");
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        std::fs::write(dir.join(".git").join("info").join("exclude"), "notes/\n").unwrap();
        assert_eq!(
            mount_state(&dir, "notes", Some(&git), Some(&base)),
            MountState::Ready
        );
    }

    #[test]
    fn a_comma_in_a_mount_path_is_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a,b")).unwrap();
        assert_eq!(
            mount_state(dir.path(), "a,b", None, None),
            MountState::PathUnsupported
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_mount_differing_only_in_case_is_found() {
        let (home, git) = repo();
        let dir = home.path().join("project");
        std::fs::write(dir.join(".gitignore"), ".claude/\n").unwrap();
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::fs::write(dir.join(".claude").join("x.json"), "{}").unwrap();
        let clean = commit(&git, &dir, "ignore");
        assert_eq!(
            mount_state(&dir, ".Claude", Some(&git), Some(&clean)),
            MountState::Ready
        );
        git.output(&dir, &["add", "-f", ".claude/x.json"]).unwrap();
        let forced = commit(&git, &dir, "force");
        assert_eq!(
            mount_state(&dir, ".Claude", Some(&git), Some(&forced)),
            MountState::Tracked,
            "a tracked file under another case is still tracked"
        );
    }

    #[cfg(windows)]
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test makes a junction with cmd's mklink, which needs no privilege"
    )]
    fn a_junction_on_the_mount_path_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let link = project.join("linked");
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        assert_eq!(
            mount_state(&project, "linked", None, None),
            MountState::ReparsePoint
        );
        std::fs::create_dir_all(outside.path().join("inner")).unwrap();
        assert_eq!(
            mount_state(&project, "linked/inner", None, None),
            MountState::ReparsePoint,
            "a reparse point above the mount"
        );
    }

    #[test]
    fn needles_never_serialize_and_a_login_run_has_none() {
        let secret = Secret::new("a-seeded-credential-value-0123456789".to_owned());
        let found = RunNeedles::of(CredentialKind::OauthToken, &secret).unwrap();
        assert!(!found.needles().is_empty());
        assert!(!format!("{found:?}").contains("seeded"));
        assert!(RunNeedles::of(CredentialKind::Login, &secret).is_none());
    }

    #[test]
    fn the_store_name_is_the_floors_target() {
        assert_eq!(
            format!("officina:{}", credential_store_name("0123456789abcdef")),
            officina_core::workshop::names::credential_target("0123456789abcdef")
        );
    }
}
