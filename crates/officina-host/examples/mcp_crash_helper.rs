//! Restart test helper (story 5.17 AC 7, D-89, D-165): writes recap
//! proposals through `write_proposal_staged`, the host function `officina
//! mcp`'s tools call, so the test that kills it kills the real write path.
//! Each file renamed is logged, one id a line, flushed and synced before
//! the next write, so a proposal that went missing fails the test. With a
//! stall index, the write of that proposal stops between its temporary
//! file and its rename, says so on stdout, and waits to be killed. Never
//! part of a shipped binary.
//!
//! Usage: `mcp_crash_helper <proposals dir> <log file> <count> <first ms>
//! <step ms> [stall index]`: proposal `n` is written at `first + n * step`,
//! its target a session of its own, so no recap supersedes another.
//!
//! Exit 0 once every proposal is written, 1 on a write error, 64 on usage.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use officina_core::orders::proposal::{ProposalFile, ProposalTarget, RecapProposal};
use officina_host::mcp::write_proposal_staged;

const USAGE: &str =
    "usage: mcp_crash_helper <proposals dir> <log file> <count> <first ms> <step ms> [stall index]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, log, numbers) = match args.as_slice() {
        [dir, log, rest @ ..] if (3..=4).contains(&rest.len()) => (dir, log, rest),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(64);
        }
    };
    let Ok(numbers) = numbers
        .iter()
        .map(|n| n.parse::<i64>())
        .collect::<Result<Vec<_>, _>>()
    else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let (count, first, step) = (numbers[0], numbers[1], numbers[2]);
    let stall = numbers.get(3).copied();
    let Ok(mut log) = OpenOptions::new().create(true).append(true).open(log) else {
        eprintln!("mcp_crash_helper: cannot open the log");
        return ExitCode::FAILURE;
    };
    for n in 0..count {
        let file = ProposalFile::recap(
            RecapProposal {
                target: ProposalTarget::SessionId(format!("s-{first}-{n}")),
                done: format!("Proposal {n} of the run at {first}."),
                blocking: "Nothing.".into(),
                decision_expected: "None.".into(),
            },
            Some("s-orchestrator".into()),
            first + n * step,
        );
        let mut stall_here = |tmp: &Path| {
            if stall == Some(n) {
                println!("stalled {}", tmp.display());
                let _ = std::io::stdout().flush();
                loop {
                    std::thread::park();
                }
            }
        };
        match write_proposal_staged(Path::new(dir), &file, first + n * step, &mut stall_here) {
            Ok(id) => {
                let logged = writeln!(log, "{id}").and_then(|()| log.sync_data());
                if logged.is_err() {
                    eprintln!("mcp_crash_helper: cannot log {id}");
                    return ExitCode::FAILURE;
                }
            }
            Err(error) => {
                eprintln!("mcp_crash_helper: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
