//! The workshop on a real engine, without the window (story 6.8 AC 6 and 7,
//! D-202, D-224): CI's `workshop` job on the runner's engine, and the
//! second halves of gates 9 and 1 on the workstation's Docker Desktop.
//! Story 6.10 adds the escape battery.
//!
//! `cargo run -p officina-host --features window-authority --example
//! battery_ci -- --floor <officina.toml> --linux <dir> --claude-version
//! <x.y.z> [--home <dir>] [--resolve-proxy] [--worker-probe [--mount-dir
//! <dir>]] [--keep-images]`
//!
//! 1. Builds the floor's images with Build images' own code (contexts,
//!    builder, labels, measurement), the floor read with its workshop files
//!    but not approved: CI has no window. `--resolve-proxy` resolves the
//!    routes with the system's configuration (story 6.25's resolution) for
//!    the builds and the gateway; without it the floor's `upstream_proxy`
//!    decides, `"none"` on CI.
//! 2. Checks the base image: AD-99's labels present, no setuid or setgid
//!    file, `claude --version` equal to `--claude-version`, and a project
//!    image's programs equal to the base's.
//! 3. Runs the real gateway with AD-102's `Gateway` vector on a test
//!    internal network and a test egress network, and a probe client on the
//!    internal network: one allowed host (`api.anthropic.com`) and one
//!    refused (`example.com`), each seen in the gateway's lines.
//! 4. `--worker-probe`: the base image with `workshop.json`'s `Worker`
//!    vector, its command replaced by a probe (`id`, a write to `/`, a read
//!    and a write in the mounted folder, `--mount-dir` or a scratch one).
//!
//! Test objects are named `officina-test-*` and labelled
//! `officina-test=1`; the images Build images names are removed at the end
//! unless `--keep-images`. A dummy credential reaches the worker probe's
//! `docker` only, never a real token.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::sync::Arc;

use officina_core::workshop::gateway::GatewayConfig;
use officina_core::workshop::image::{BuildOutcome, GATEWAY_IMAGE, ImageKind, WORKSHOP_BASE};
use officina_core::workshop::names::ObjectName;
use officina_host::workshop::build::{BuildReport, build_all, find_programs, plan_from, preflight};
use officina_host::workshop::engine::{ResolvedDocker, resolve};
use officina_host::workshop::proxy::{RegistryReader, SystemProxyReader};

const USAGE: &str = "usage: battery_ci --floor <officina.toml> --linux <dir> --claude-version <x.y.z> [--home <dir>] [--resolve-proxy] [--worker-probe [--mount-dir <dir>]] [--keep-images]";
const TEST_LABEL: &str = "officina-test=1";
const INTERNAL: &str = "officina-test-int";
const EGRESS: &str = "officina-test-out";
const GATEWAY: &str = "officina-test-gateway";
const VOLUME: &str = "officina-test-vol";
const ALLOWED_HOST: &str = "api.anthropic.com";
const REFUSED_HOST: &str = "example.com";
const DUMMY_CREDENTIAL: &str = "officina-battery-dummy-credential";

struct Options {
    floor: PathBuf,
    linux: PathBuf,
    claude_version: String,
    home: PathBuf,
    resolve_proxy: bool,
    worker_probe: bool,
    mount_dir: Option<PathBuf>,
    keep_images: bool,
}

fn options() -> Result<Options, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let flag = |name: &str| args.iter().any(|a| a == name);
    Ok(Options {
        floor: value("--floor").ok_or(USAGE)?.into(),
        linux: value("--linux").ok_or(USAGE)?.into(),
        claude_version: value("--claude-version").ok_or(USAGE)?,
        home: value("--home").map_or_else(
            || std::env::temp_dir().join("officina-test-battery-ci"),
            PathBuf::from,
        ),
        resolve_proxy: flag("--resolve-proxy"),
        worker_probe: flag("--worker-probe"),
        mount_dir: value("--mount-dir").map(PathBuf::from),
        keep_images: flag("--keep-images"),
    })
}

struct Run {
    docker: ResolvedDocker,
    failures: Vec<String>,
}

impl Run {
    #[expect(
        clippy::disallowed_methods,
        reason = "a gate driver runs docker directly, as the gate probes do (D-202)"
    )]
    fn docker(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        println!("$ docker {}", args.join(" "));
        let mut command = Command::new(self.docker.path());
        command.args(args);
        for (name, value) in env {
            command.env(name, value);
        }
        command.output().unwrap_or_else(|e| {
            eprintln!("docker did not start: {e}");
            std::process::exit(2);
        })
    }

    fn check(&mut self, ok: bool, what: &str) {
        println!("[{}] {what}", if ok { "pass" } else { "FAIL" });
        if !ok {
            self.failures.push(what.to_owned());
        }
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn print_report(report: &BuildReport) {
    println!(
        "image {} ({}): {} {} programs {}{}",
        report.record.name,
        report.record.kind.code(),
        report.record.outcome.code(),
        report.record.image_id.as_deref().unwrap_or("-"),
        report.record.programs_sha256.as_deref().unwrap_or("-"),
        report
            .detail
            .map(|d| format!(" ({})", d.code()))
            .unwrap_or_default()
    );
    if report.record.outcome != BuildOutcome::Built
        && let Some(tail) = &report.tail
    {
        let start = tail.len().saturating_sub(4000);
        println!("{}", &tail[tail.floor_char_boundary(start)..]);
    }
}

fn main() -> ExitCode {
    let options = match options() {
        Ok(options) => options,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(64);
        }
    };
    let docker = match resolve() {
        Ok(docker) => docker,
        Err(e) => {
            eprintln!("docker: {e}");
            return ExitCode::from(2);
        }
    };
    let mut run = Run {
        docker: docker.clone(),
        failures: Vec::new(),
    };
    let base_env: Vec<(String, String)> = std::env::vars().collect();
    let floor = match officina_host::relay::home::load_relay_floor(&options.floor) {
        Ok(floor) => floor,
        Err(e) => {
            eprintln!("the floor: {e}");
            return ExitCode::from(2);
        }
    };
    let mut plan = match plan_from(
        floor,
        &options.home,
        Some(&options.claude_version),
        &base_env,
    ) {
        Ok(plan) => plan,
        Err(refusal) => {
            eprintln!("refused: {}", refusal.code());
            return ExitCode::from(1);
        }
    };
    if options.resolve_proxy {
        plan.upstream_proxy = officina_core::workshop::floor::UpstreamProxy::System;
    }
    let programs = match find_programs(None, Some(options.linux.as_os_str())) {
        Ok(programs) => programs,
        Err(refusal) => {
            eprintln!("refused: {}", refusal.code());
            return ExitCode::from(1);
        }
    };
    let reader: Arc<dyn SystemProxyReader> = Arc::new(RegistryReader);
    let prepared = match preflight(&plan, &programs, &docker, Arc::clone(&reader)) {
        Ok(prepared) => prepared,
        Err(refusal) => {
            eprintln!("refused: {}", refusal.code());
            return ExitCode::from(1);
        }
    };
    println!(
        "builder {}, build args {:?}",
        prepared.builder,
        prepared.build_args.keys().collect::<Vec<_>>()
    );
    let mut reports = Vec::new();
    build_all(&prepared, &officina_host::files::now_ms, &mut |report| {
        print_report(&report);
        reports.push(report);
    });
    let of = |kind: ImageKind| reports.iter().filter(move |r| r.record.kind == kind);
    let built = |r: &&BuildReport| r.record.outcome == BuildOutcome::Built;
    let base = of(ImageKind::Workshop).find(built).cloned();
    let gateway = of(ImageKind::Gateway).find(built).cloned();
    run.check(gateway.is_some(), "the gateway image built");
    run.check(base.is_some(), "the base image built");
    for project in of(ImageKind::Project) {
        run.check(
            built(&project)
                && project.record.programs_sha256
                    == base.as_ref().and_then(|b| b.record.programs_sha256.clone()),
            &format!("{} built, its programs the base's", project.record.name),
        );
    }
    if let (Some(base), Some(gateway)) = (&base, &gateway) {
        let base_id = base.record.image_id.clone().unwrap_or_default();
        check_base(&mut run, &base_id, &options.claude_version);
        let routes = if options.resolve_proxy {
            officina_host::workshop::proxy::resolve(
                &plan.upstream_proxy,
                &[ALLOWED_HOST.to_owned(), REFUSED_HOST.to_owned()],
                Arc::clone(&reader),
            )
            .map(|r| r.gateway_routes())
            .unwrap_or_default()
        } else {
            Default::default()
        };
        let gateway_id = gateway.record.image_id.clone().unwrap_or_default();
        check_gateway(&mut run, &gateway_id, &base_id, routes);
        if options.worker_probe {
            worker_probe(
                &mut run,
                &base_id,
                options.mount_dir.as_deref(),
                &options.home,
            );
        }
    }
    cleanup(&run, &reports, options.keep_images);
    if run.failures.is_empty() {
        println!("BATTERY-CI-OK");
        ExitCode::SUCCESS
    } else {
        println!("BATTERY-CI-FAILED: {}", run.failures.join("; "));
        ExitCode::from(1)
    }
}

/// AD-99's labels on the base, no setuid or setgid file, and its Claude
/// Code at the host's version.
fn check_base(run: &mut Run, base_id: &str, claude_version: &str) {
    let labels = run.docker(
        &[
            "image",
            "inspect",
            "--format",
            "{{json .Config.Labels}}",
            base_id,
        ],
        &[],
    );
    let labels: serde_json::Value =
        serde_json::from_slice(&labels.stdout).unwrap_or(serde_json::Value::Null);
    println!("labels {labels}");
    for name in [
        "officina.base",
        "officina.version",
        "officina.dockerfile.sha256",
        "officina.entry.sha256",
        "officina.gateway.sha256",
        "officina.claude.version",
        "officina.ca.sha256",
        "org.opencontainers.image.source",
    ] {
        run.check(
            labels.get(name).is_some(),
            &format!("the base carries {name}"),
        );
    }
    let found = run.docker(
        &[
            "run",
            "--rm",
            "--network",
            "none",
            "--label",
            TEST_LABEL,
            "--user",
            "0:0",
            "--entrypoint",
            "/usr/bin/find",
            base_id,
            "/",
            "-xdev",
            "-perm",
            "/6000",
            "-type",
            "f",
        ],
        &[],
    );
    run.check(
        found.status.success() && found.stdout.is_empty(),
        &format!(
            "no setuid or setgid file in the base ({})",
            text(&found).trim()
        ),
    );
    let version = run.docker(
        &[
            "run",
            "--rm",
            "--network",
            "none",
            "--label",
            TEST_LABEL,
            "--entrypoint",
            "/opt/officina/bin/claude",
            base_id,
            "--version",
        ],
        &[],
    );
    let printed = text(&version);
    run.check(
        version.status.success() && printed.contains(claude_version),
        &format!("claude --version in the base: {}", printed.trim()),
    );
}

/// The real gateway with AD-102's `Gateway` vector, a probe client on the
/// internal network, one allowed and one refused host.
fn check_gateway(
    run: &mut Run,
    gateway_id: &str,
    base_id: &str,
    routes: std::collections::BTreeMap<String, String>,
) {
    let _ = run.docker(&["rm", "--force", GATEWAY], &[]);
    let _ = run.docker(&["network", "rm", INTERNAL, EGRESS], &[]);
    run.docker(
        &[
            "network",
            "create",
            "--internal",
            "--label",
            TEST_LABEL,
            INTERNAL,
        ],
        &[],
    );
    run.docker(&["network", "create", "--label", TEST_LABEL, EGRESS], &[]);
    let name = ObjectName::parse(GATEWAY).expect("a name");
    let egress = ObjectName::parse(EGRESS).expect("a name");
    let labels = std::collections::BTreeMap::from([("officina-test".to_owned(), "1".to_owned())]);
    let config = GatewayConfig::new("officina-test", &[], 600, routes);
    let vector =
        officina_core::workshop::vector::gateway(&name, &labels, gateway_id, &egress, &config);
    let call = officina_core::workshop::engine::EngineCall::create(&vector);
    let args: Vec<&str> = call.args().iter().map(String::as_str).collect();
    let created = run.docker(&args, &[]);
    run.check(
        created.status.success(),
        &format!("the gateway created: {}", text(&created).trim()),
    );
    run.docker(&["network", "connect", INTERNAL, GATEWAY], &[]);
    let started = run.docker(&["start", GATEWAY], &[]);
    run.check(started.status.success(), "the gateway started");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let probe = |run: &Run, host: &str| {
        run.docker(
            &[
                "run",
                "--rm",
                "--network",
                INTERNAL,
                "--label",
                TEST_LABEL,
                "--entrypoint",
                "/usr/bin/curl",
                base_id,
                "-sS",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "--max-time",
                "20",
                "-x",
                &format!("http://{GATEWAY}:3128"),
                &format!("https://{host}/"),
            ],
            &[],
        )
    };
    let allowed = probe(run, ALLOWED_HOST);
    run.check(
        allowed.status.success(),
        &format!(
            "{ALLOWED_HOST} through the gateway: {}",
            text(&allowed).trim()
        ),
    );
    let refused = probe(run, REFUSED_HOST);
    run.check(
        !refused.status.success(),
        &format!(
            "{REFUSED_HOST} refused by the gateway: {}",
            text(&refused).trim()
        ),
    );
    let logs = run.docker(&["logs", GATEWAY], &[]);
    let lines = text(&logs);
    println!("{lines}");
    run.check(
        lines
            .lines()
            .any(|l| l.contains("\"line\":\"allowed\"") && l.contains(ALLOWED_HOST)),
        "the gateway logged an allowed line",
    );
    run.check(
        lines
            .lines()
            .any(|l| l.contains("\"line\":\"refused\"") && l.contains(REFUSED_HOST)),
        "the gateway logged a refused line",
    );
}

/// The `Worker` vector of `workshop.json` on the base, its command replaced
/// by a probe: `id`, a write to `/`, a read and a write in the mounted
/// folder (gate 1's second half; the owner's run mounts a git-ignored
/// folder of one of his projects with `--mount-dir`).
fn worker_probe(run: &mut Run, base_id: &str, mount_dir: Option<&Path>, home: &Path) {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/vectors/workshop.json")).expect("JSON");
    let golden: Vec<String> = vectors["vectors"]["launch_worker"]
        .as_array()
        .expect("the launch's worker")
        .iter()
        .filter_map(|a| a.as_str().map(str::to_owned))
        .collect();
    let scratch = home.join("officina-test-mount");
    let mount = match mount_dir {
        Some(dir) => dir.to_path_buf(),
        None => {
            let _ = std::fs::create_dir_all(&scratch);
            let _ = std::fs::write(scratch.join("probe.txt"), "read me\n");
            scratch
        }
    };
    let _ = run.docker(&["rm", "--force", "officina-test-worker"], &[]);
    run.docker(&["volume", "create", "--label", TEST_LABEL, VOLUME], &[]);
    let mut args: Vec<String> = Vec::new();
    let mut iter = golden.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        let next = iter.peek().map(|s| s.as_str());
        match (arg.as_str(), next) {
            ("--name", Some(_)) => {
                iter.next();
                args.extend(["--name".into(), "officina-test-worker".into()]);
            }
            ("--network", Some(_)) => {
                iter.next();
                args.extend(["--network".into(), INTERNAL.into()]);
            }
            ("--entrypoint", Some(_)) => {
                iter.next();
                args.extend(["--entrypoint".into(), "/bin/bash".into()]);
            }
            ("--label", Some(_)) => {
                iter.next();
            }
            ("--mount", Some(value)) if value.contains("type=volume") => {
                iter.next();
                args.extend([
                    "--mount".into(),
                    format!("type=volume,source={VOLUME},target=/work"),
                ]);
            }
            ("--mount", Some(value)) if value.contains("type=bind") => {
                iter.next();
                args.extend([
                    "--mount".into(),
                    format!(
                        "type=bind,source={},target=/work/.officina/mounts/0,readonly",
                        mount.display()
                    ),
                ]);
            }
            (image, _) if image.starts_with("sha256:") => break,
            (other, _) => args.push(other.to_owned()),
        }
    }
    args.extend([
        "--label".into(),
        TEST_LABEL.into(),
        base_id.into(),
        "-c".into(),
    ]);
    args.push(
        "id; \
         (touch /probe-root && echo ROOT_WRITE_ALLOWED) || echo ROOT_WRITE_REFUSED; \
         ls /work/.officina/mounts/0 >/dev/null && echo MOUNT_READ_OK; \
         (touch /work/.officina/mounts/0/officina-test-probe && echo MOUNT_WRITE_ALLOWED) || echo MOUNT_WRITE_REFUSED; \
         (touch /work/probe && echo WORK_WRITE_OK) || echo WORK_WRITE_REFUSED"
            .into(),
    );
    let mut create = vec!["create".to_owned()];
    create.extend(args);
    let refs: Vec<&str> = create.iter().map(String::as_str).collect();
    let created = run.docker(&refs, &[("CLAUDE_CODE_OAUTH_TOKEN", DUMMY_CREDENTIAL)]);
    run.check(
        created.status.success(),
        &format!("the worker probe created: {}", text(&created).trim()),
    );
    let ran = run.docker(&["start", "--attach", "officina-test-worker"], &[]);
    let out = text(&ran);
    println!("{out}");
    run.check(out.contains("uid=1000"), "the worker runs as uid 1000");
    run.check(
        out.contains("ROOT_WRITE_REFUSED"),
        "a write to / is refused",
    );
    run.check(out.contains("MOUNT_READ_OK"), "the mount reads");
    run.check(
        out.contains("MOUNT_WRITE_REFUSED"),
        "a write in the mount is refused",
    );
    run.check(out.contains("WORK_WRITE_OK"), "a write in /work succeeds");
    let _ = run.docker(&["rm", "--force", "officina-test-worker"], &[]);
    let _ = run.docker(&["volume", "rm", VOLUME], &[]);
}

fn cleanup(run: &Run, reports: &[BuildReport], keep_images: bool) {
    let _ = run.docker(&["rm", "--force", GATEWAY], &[]);
    let _ = run.docker(&["network", "rm", INTERNAL, EGRESS], &[]);
    if keep_images {
        return;
    }
    let mut tags: Vec<&str> = reports
        .iter()
        .filter(|r| r.record.kind == ImageKind::Project)
        .map(|r| r.record.name.as_str())
        .collect();
    tags.extend([WORKSHOP_BASE, GATEWAY_IMAGE]);
    for tag in tags {
        let _ = run.docker(&["image", "rm", tag], &[]);
    }
}
