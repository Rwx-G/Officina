//! What the session binding reads of the process tree (D-960): the chain
//! `officina run` records in its request, itself and its ancestors with
//! their start stamps, and the start stamp of a live session's process,
//! which the window compares with it. Windows reads a process snapshot,
//! Linux `/proc`; elsewhere both read nothing, so no session binds and
//! every `auto` request is the author's.

use officina_core::relay::approvals::CallerProcess;

/// This process and its ancestors, nearest first.
pub fn own_chain() -> Vec<CallerProcess> {
    chain_of(std::process::id())
}

/// Process `pid` and its ancestors, nearest first; empty when `pid` cannot
/// be read.
pub fn chain_of(pid: u32) -> Vec<CallerProcess> {
    #[cfg(windows)]
    let found = crate::platform::windows::ancestry(pid);
    #[cfg(target_os = "linux")]
    let found = crate::platform::linux::ancestry(pid);
    #[cfg(not(any(windows, target_os = "linux")))]
    let found: Vec<(u32, u64)> = {
        let _ = pid;
        Vec::new()
    };
    found
        .into_iter()
        .map(|(pid, started)| CallerProcess { pid, started })
        .collect()
}

/// The start stamp of process `pid` while it runs, in the units of
/// [`chain_of`]; `None` once it has exited or when it cannot be read.
pub fn running_since(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    return crate::platform::windows::running_since(pid);
    #[cfg(target_os = "linux")]
    return crate::platform::linux::running_since(pid);
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chain starts with this process, holds its parent, runs from
    /// the newest process to the oldest, and its stamps are the ones
    /// [`running_since`] reads of the same processes.
    #[test]
    fn the_own_chain_starts_here_and_climbs_to_older_processes() {
        let chain = own_chain();
        assert_eq!(chain[0].pid, std::process::id());
        assert!(chain.len() >= 2, "the test runner has a parent: {chain:?}");
        assert!(
            chain.windows(2).all(|w| w[1].started <= w[0].started),
            "every ancestor is older than its child: {chain:?}"
        );
        assert_eq!(running_since(chain[0].pid), Some(chain[0].started));
        assert_eq!(running_since(chain[1].pid), Some(chain[1].started));
    }

    /// A process that exited has no stamp, so a session whose process is
    /// gone never binds.
    #[test]
    fn an_exited_process_has_no_stamp() {
        #[expect(
            clippy::disallowed_methods,
            reason = "the test starts a short-lived process to watch it exit"
        )]
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert_eq!(running_since(pid), None);
    }
}
