//! Host verification of a run's package (AD-111, D-219, D-220, D-231):
//! host git, under the workshop rule, in a scratch repository whose
//! alternates borrow the project's objects read-only. Core's
//! [`assess`] says which step runs next and what the package is; this
//! module only runs the steps and reports their outputs.

use std::io::Read;
use std::path::{Path, PathBuf};

use officina_core::workshop::bundle::{MAX_HEADER_BYTES, parse_header};
use officina_core::workshop::package::{Assessed, Expected, Outputs, PackageAssessment, assess};

use crate::workshop::inventory::WorkshopGit;

/// The scratch repository's folder under a run's folder.
pub const SCRATCH_DIR: &str = "verify.git";

/// Why verification could not run: a host failure, never a verdict on
/// the package.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("verification: {0}")]
pub struct VerifyError(pub String);

/// What a package held once it reached the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    pub tip: Option<Vec<u8>>,
    /// `out/work.bundle` on the host, when the stream held one.
    pub bundle: Option<PathBuf>,
    pub bundle_bytes: Option<u64>,
}

fn succeeds(git: &WorkshopGit, dir: &Path, args: &[&str]) -> Result<bool, VerifyError> {
    crate::workshop::git::run(&git.git, dir, args, &git.base_env, &git.hooks_dir, None)
        .map(|ran| ran.status.success())
        .map_err(|e| VerifyError(e.to_string()))
}

fn text(git: &WorkshopGit, dir: &Path, args: &[&str]) -> Option<String> {
    git.output(dir, args)
        .ok()
        .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
}

/// Makes `<run_dir>/verify.git`: bare, the project's object format, its
/// alternates naming the project's object directory (AD-111, review F17).
pub fn scratch(
    git: &WorkshopGit,
    project_dir: &Path,
    run_dir: &Path,
) -> Result<PathBuf, VerifyError> {
    let format = text(git, project_dir, &["rev-parse", "--show-object-format"])
        .filter(|f| matches!(f.as_str(), "sha1" | "sha256"))
        .ok_or_else(|| VerifyError("the project's object format".to_owned()))?;
    let common = text(
        git,
        project_dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok_or_else(|| VerifyError("the project's git directory".to_owned()))?;
    let dir = run_dir.join(SCRATCH_DIR);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| VerifyError(e.to_string()))?;
    }
    std::fs::create_dir_all(run_dir).map_err(|e| VerifyError(e.to_string()))?;
    let target = dir.to_string_lossy().into_owned();
    git.output(
        run_dir,
        &[
            "init",
            "--quiet",
            "--bare",
            &format!("--object-format={format}"),
            &target,
        ],
    )
    .map_err(VerifyError)?;
    let objects = Path::new(&common).join("objects");
    let alternates = dir.join("objects").join("info").join("alternates");
    std::fs::write(
        &alternates,
        format!("{}\n", objects.to_string_lossy().replace('\\', "/")),
    )
    .map_err(|e| VerifyError(e.to_string()))?;
    Ok(dir)
}

/// Runs AD-111's checks in core's order on `received` and returns what the
/// package is (AD-110).
pub fn verify(
    git: &WorkshopGit,
    scratch: &Path,
    received: &Received,
    expected: &Expected,
) -> Result<PackageAssessment, VerifyError> {
    let mut outputs = Outputs {
        tip: received.tip.clone(),
        bundle_bytes: received.bundle_bytes,
        ..Outputs::default()
    };
    let bundle = received
        .bundle
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = expected.base_commit.as_str();
    let fetch_spec = format!("{0}:{0}", expected.branch_ref);
    loop {
        let tip = outputs
            .tip
            .as_deref()
            .and_then(|t| std::str::from_utf8(t).ok())
            .and_then(officina_core::workshop::package::object_id)
            .unwrap_or_default();
        let range = format!("{base}..{tip}");
        match assess(&outputs, expected) {
            Assessed::Done(done) => return Ok(done),
            Assessed::CheckTipHeld => {
                outputs.tip_held = Some(succeeds(
                    git,
                    scratch,
                    &["cat-file", "-e", &format!("{tip}^{{commit}}")],
                )?);
            }
            Assessed::ParseHeader => {
                let mut head = Vec::new();
                std::fs::File::open(&bundle)
                    .and_then(|f| f.take(MAX_HEADER_BYTES as u64).read_to_end(&mut head))
                    .map_err(|e| VerifyError(e.to_string()))?;
                outputs.header = Some(parse_header(&head));
            }
            Assessed::Verify => {
                outputs.verified = Some(succeeds(
                    git,
                    scratch,
                    &["bundle", "verify", "--quiet", &bundle],
                )?);
            }
            Assessed::Fetch => {
                outputs.fetched = Some(succeeds(
                    git,
                    scratch,
                    &[
                        "fetch",
                        "--quiet",
                        "--no-tags",
                        "--no-write-fetch-head",
                        &bundle,
                        &fetch_spec,
                    ],
                )?);
            }
            Assessed::AncestryAndCount => {
                if outputs.bundle_bytes.is_some() {
                    outputs.base_is_ancestor = Some(succeeds(
                        git,
                        scratch,
                        &["merge-base", "--is-ancestor", base, tip.as_str()],
                    )?);
                }
                let count = text(git, scratch, &["rev-list", "--count", &range])
                    .and_then(|n| n.parse().ok())
                    .ok_or_else(|| VerifyError("rev-list --count".to_owned()))?;
                outputs.commits = Some(count);
            }
            Assessed::Diff => {
                let out = git
                    .output(
                        scratch,
                        &[
                            "diff",
                            "--raw",
                            "-z",
                            "-M",
                            "--no-ext-diff",
                            "--no-textconv",
                            base,
                            tip.as_str(),
                        ],
                    )
                    .map_err(VerifyError)?;
                outputs.paths = Some(raw_paths(&out));
            }
            Assessed::Inflate => {
                outputs.inflated_bytes = Some(inflated(git, scratch, &range)?);
            }
        }
    }
}

/// Every path a `diff --raw -z` names, both sides of a rename or copy.
pub fn raw_paths(out: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut fields = out.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let Some(meta) = fields.next() {
        let status = meta
            .split(|b| *b == b' ')
            .nth(4)
            .and_then(|s| s.first().copied());
        let count = if matches!(status, Some(b'R' | b'C')) {
            2
        } else {
            1
        };
        for _ in 0..count {
            if let Some(path) = fields.next() {
                paths.push(String::from_utf8_lossy(path).into_owned());
            }
        }
    }
    paths
}

/// The inflated size of every object past the base (AD-111 step 7):
/// `rev-list --objects`, then `cat-file --batch-check` fed the ids only.
fn inflated(git: &WorkshopGit, scratch: &Path, range: &str) -> Result<u64, VerifyError> {
    let listed = git
        .output(scratch, &["rev-list", "--objects", range])
        .map_err(VerifyError)?;
    let ids: String = String::from_utf8_lossy(&listed)
        .lines()
        .filter_map(|line| line.split(' ').next())
        .filter(|id| officina_core::workshop::package::object_id(id).is_some())
        .map(|id| format!("{id}\n"))
        .collect();
    let ran = crate::workshop::git::run(
        &git.git,
        scratch,
        &["cat-file", "--batch-check=%(objectsize)"],
        &git.base_env,
        &git.hooks_dir,
        Some(ids.into_bytes()),
    )
    .map_err(|e| VerifyError(e.to_string()))?;
    if !ran.status.success() {
        return Err(VerifyError("cat-file --batch-check".to_owned()));
    }
    Ok(String::from_utf8_lossy(&ran.stdout)
        .lines()
        .filter_map(|n| n.trim().parse::<u64>().ok())
        .sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workshop::launch::tests::{commit, repo};
    use officina_core::workshop::package::QuarantineCause;

    struct Fixture {
        _home: tempfile::TempDir,
        git: WorkshopGit,
        project: PathBuf,
        work: PathBuf,
        run: PathBuf,
        base: String,
    }

    /// A project with one commit, and a worker's clone of it on
    /// `officina/bump-1`.
    fn fixture() -> Fixture {
        let (home, git) = repo();
        let project = home.path().join("project");
        std::fs::write(project.join(".gitignore"), ".claude/\n").unwrap();
        let base = commit(&git, &project, "base");
        let work = home.path().join("work");
        git.output(
            home.path(),
            &[
                "clone",
                "-q",
                &project.to_string_lossy(),
                &work.to_string_lossy(),
            ],
        )
        .unwrap();
        for args in [
            vec!["config", "user.name", "User One"],
            vec!["config", "user.email", "user01@example.com"],
            vec!["checkout", "-q", "-b", "officina/bump-1"],
        ] {
            git.output(&work, &args).unwrap();
        }
        let run = home.path().join("run");
        Fixture {
            _home: home,
            git,
            project,
            work,
            run,
            base,
        }
    }

    fn expected(f: &Fixture) -> Expected {
        Expected {
            base_commit: f.base.clone(),
            branch_ref: "refs/heads/officina/bump-1".to_owned(),
            mounts: vec![".claude".to_owned()],
        }
    }

    /// The package the packer would write from the clone.
    fn received(f: &Fixture, with_bundle: bool) -> Received {
        let tip = text(&f.git, &f.work, &["rev-parse", "HEAD"]).unwrap();
        std::fs::create_dir_all(f.run.join("out")).unwrap();
        let path = f.run.join("out").join("work.bundle");
        let bundle = with_bundle.then(|| {
            f.git
                .output(
                    &f.work,
                    &[
                        "bundle",
                        "create",
                        "-q",
                        &path.to_string_lossy(),
                        "refs/heads/officina/bump-1",
                        &format!("^{}", f.base),
                    ],
                )
                .unwrap();
            path.clone()
        });
        Received {
            tip: Some(format!("{tip}\n").into_bytes()),
            bundle_bytes: bundle.as_ref().map(|p| std::fs::metadata(p).unwrap().len()),
            bundle,
        }
    }

    #[test]
    fn a_commit_past_the_base_is_a_package() {
        let f = fixture();
        std::fs::write(f.work.join("a.txt"), "a").unwrap();
        commit(&f.git, &f.work, "change");
        let scratch = scratch(&f.git, &f.project, &f.run).unwrap();
        let got = verify(&f.git, &scratch, &received(&f, true), &expected(&f)).unwrap();
        assert!(
            matches!(
                got,
                PackageAssessment::Package {
                    commits_past_base: 1,
                    cause: None,
                    ..
                }
            ),
            "{got:?}"
        );
    }

    #[test]
    fn a_held_tip_with_no_bundle_and_no_commit_is_no_commit() {
        let f = fixture();
        let scratch = scratch(&f.git, &f.project, &f.run).unwrap();
        let got = verify(&f.git, &scratch, &received(&f, false), &expected(&f)).unwrap();
        assert!(matches!(got, PackageAssessment::NoCommit { .. }), "{got:?}");
    }

    #[test]
    fn a_committed_mount_is_quarantined() {
        let f = fixture();
        std::fs::create_dir_all(f.work.join(".claude")).unwrap();
        std::fs::write(f.work.join(".claude").join("settings.json"), "{}").unwrap();
        f.git
            .output(&f.work, &["add", "-f", ".claude/settings.json"])
            .unwrap();
        commit(&f.git, &f.work, "mount");
        let scratch = scratch(&f.git, &f.project, &f.run).unwrap();
        let got = verify(&f.git, &scratch, &received(&f, true), &expected(&f)).unwrap();
        assert!(
            matches!(
                got,
                PackageAssessment::Package {
                    cause: Some(QuarantineCause::MountCommitted),
                    ..
                }
            ),
            "{got:?}"
        );
    }

    #[test]
    fn verification_never_writes_the_project_objects() {
        let f = fixture();
        std::fs::write(f.work.join("a.txt"), "a").unwrap();
        commit(&f.git, &f.work, "change");
        let objects = f.project.join(".git").join("objects");
        let before = listing(&objects);
        let scratch = scratch(&f.git, &f.project, &f.run).unwrap();
        verify(&f.git, &scratch, &received(&f, true), &expected(&f)).unwrap();
        assert_eq!(listing(&objects), before);
    }

    fn listing(dir: &Path) -> Vec<(PathBuf, u64)> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(listing(&path));
            } else {
                found.push((path.clone(), std::fs::metadata(&path).unwrap().len()));
            }
        }
        found.sort();
        found
    }

    #[test]
    fn raw_paths_read_both_sides_of_a_rename() {
        let out =
            b":100644 100644 aaa bbb R100\0old.txt\0new.txt\0:100644 100644 aaa bbb M\0x.rs\0";
        assert_eq!(raw_paths(out), ["old.txt", "new.txt", "x.rs"]);
    }
}
