//! The process tree on Linux, from `/proc` (D-960): a process's parent and
//! its start stamp, the clock ticks since boot at which it started
//! (`proc(5)`, `stat` field 22). No `unsafe`: every value is a file read.

/// A process's `stat` fields after its name: its state, its parent and its
/// start stamp.
struct Stat {
    state: char,
    parent: u32,
    started: u64,
}

/// Reads `/proc/<pid>/stat`. The name (field 2) sits in parentheses and
/// may hold spaces and parentheses itself, so the fields are read after
/// its last `)`.
fn stat(pid: u32) -> Option<Stat> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, rest) = text.rsplit_once(')')?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After the name: state is field 3, the parent field 4, the start
    // stamp field 22.
    Some(Stat {
        state: fields.first()?.chars().next()?,
        parent: fields.get(1)?.parse().ok()?,
        started: fields.get(19)?.parse().ok()?,
    })
}

/// The start stamp of process `pid` while it runs; `None` once it has
/// exited (a zombie has) or when `/proc` cannot be read.
pub fn running_since(pid: u32) -> Option<u64> {
    stat(pid)
        .filter(|found| !matches!(found.state, 'Z' | 'X'))
        .map(|found| found.started)
}

/// `pid` and its ancestors with their start stamps, nearest first, at most
/// 16 hops, stopping at a parent that started after its child (a pid given
/// to another process) or at the top of the tree. Empty when `pid` itself
/// cannot be read.
pub fn ancestry(pid: u32) -> Vec<(u32, u64)> {
    let Some(mut current) = stat(pid) else {
        return Vec::new();
    };
    let mut chain = vec![(pid, current.started)];
    for _ in 0..16 {
        let parent = current.parent;
        if parent == 0 || chain.iter().any(|(seen, _)| *seen == parent) {
            break;
        }
        let Some(found) = stat(parent) else {
            break;
        };
        if found.started > current.started {
            break;
        }
        chain.push((parent, found.started));
        current = found;
    }
    chain
}
