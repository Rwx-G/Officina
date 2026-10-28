//! A process run's output (AD-31): both streams copied through as bytes
//! arrive, under one shared budget, every byte counted and hashed, and a
//! check run's first bytes kept (D-170).

use std::io::{Read, Write};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::relay::StreamHash;
use officina_core::relay::approvals::kept_output;
use officina_core::relay::journal::StreamSummary;

use super::journaling::Streams;
use super::kept::KeptBytes;

const CHUNK: usize = 8192;

/// What one output stream carried so far.
#[derive(Default)]
struct Stream {
    bytes: u64,
    hash: StreamHash,
    /// The first bytes, for a check run (D-170).
    kept: Option<KeptBytes>,
}

impl Stream {
    fn summary(&self) -> StreamSummary {
        StreamSummary {
            bytes: self.bytes,
            sha256: self.hash.hex(),
        }
    }
}

/// The two output streams of a run, copied through as bytes arrive under
/// one shared budget; past it they keep draining, so a full pipe never
/// stalls the program, and every byte is counted and hashed (AD-31). They go
/// to this process's stdout and stderr, where the session reads them.
pub(super) struct Output {
    stdout: Arc<Mutex<Stream>>,
    stderr: Arc<Mutex<Stream>>,
    dropped: Arc<AtomicU64>,
    stdout_at_line_start: Arc<AtomicBool>,
    done: Receiver<()>,
    budget_kib: u32,
}

impl Output {
    /// `keep`: also keep the first bytes of each stream (D-170).
    pub(super) fn start(child: &mut Child, budget_kib: u32, keep: bool) -> Output {
        let budget = Arc::new(AtomicU64::new(u64::from(budget_kib) * 1024));
        let dropped = Arc::new(AtomicU64::new(0));
        let stdout_at_line_start = Arc::new(AtomicBool::new(true));
        let (done_tx, done) = mpsc::channel();
        let stream = || {
            Arc::new(Mutex::new(Stream {
                kept: keep.then(KeptBytes::default),
                ..Stream::default()
            }))
        };
        let stdout = stream();
        let stderr = stream();
        if let Some(pipe) = child.stdout.take() {
            let at_line_start = Arc::clone(&stdout_at_line_start);
            pump(
                pipe,
                Arc::clone(&stdout),
                Arc::clone(&budget),
                Arc::clone(&dropped),
                done_tx.clone(),
                move |bytes| {
                    let mut out = std::io::stdout().lock();
                    if out.write_all(bytes).and_then(|()| out.flush()).is_ok() {
                        at_line_start.store(bytes.ends_with(b"\n"), Ordering::SeqCst);
                    }
                },
            );
        }
        if let Some(pipe) = child.stderr.take() {
            pump(
                pipe,
                Arc::clone(&stderr),
                budget,
                Arc::clone(&dropped),
                done_tx,
                |bytes| {
                    let mut err = std::io::stderr().lock();
                    let _ = err.write_all(bytes).and_then(|()| err.flush());
                },
            );
        }
        Output {
            stdout,
            stderr,
            dropped,
            stdout_at_line_start,
            done,
            budget_kib,
        }
    }

    /// Waits for both streams to close, at most `grace`.
    pub(super) fn drain(&self, grace: Duration) {
        let deadline = Instant::now() + grace;
        for _ in 0..2 {
            let left = deadline.saturating_duration_since(Instant::now());
            if self.done.recv_timeout(left).is_err() {
                return;
            }
        }
    }

    /// What a check run keeps (D-170): stdout then stderr, capped, with the
    /// cut marker; `None` when the run keeps nothing.
    pub(super) fn kept(&self) -> Option<Vec<u8>> {
        let out = self.stdout.lock().ok()?;
        let err = self.stderr.lock().ok()?;
        Some(kept_output(
            out.kept.as_ref()?.bytes(),
            err.kept.as_ref()?.bytes(),
            out.bytes,
            err.bytes,
        ))
    }

    /// What each stream carried so far, and what the budget dropped.
    pub(super) fn streams(&self) -> Streams {
        let summary = |stream: &Arc<Mutex<Stream>>| {
            stream
                .lock()
                .map(|s| s.summary())
                .unwrap_or_else(|poisoned| poisoned.into_inner().summary())
        };
        Streams {
            stdout: summary(&self.stdout),
            stderr: summary(&self.stderr),
            dropped: self.dropped.load(Ordering::SeqCst),
        }
    }

    /// The last line on stdout when the budget cut the output (AD-31).
    pub(super) fn mark_cut(&self) {
        let dropped = self.dropped.load(Ordering::SeqCst);
        if dropped == 0 {
            return;
        }
        let lead = if self.stdout_at_line_start.load(Ordering::SeqCst) {
            ""
        } else {
            "\n"
        };
        let mut out = std::io::stdout().lock();
        let _ = writeln!(
            out,
            "{lead}[officina: output cut at {} KiB, {dropped} bytes dropped]",
            self.budget_kib
        );
        let _ = out.flush();
    }
}

/// The streams of a run that started nothing: empty, nothing dropped.
pub(super) fn no_streams() -> Streams {
    let empty = || Stream::default().summary();
    Streams {
        stdout: empty(),
        stderr: empty(),
        dropped: 0,
    }
}

/// Copies `pipe` through `sink` within the shared `budget`, on a thread of
/// its own, counting and hashing every byte; signals `done` at its end.
fn pump(
    mut pipe: impl Read + Send + 'static,
    stream: Arc<Mutex<Stream>>,
    budget: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    done: Sender<()>,
    sink: impl Fn(&[u8]) + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let chunk = &buf[..n];
            if let Ok(mut stream) = stream.lock() {
                stream.bytes += n as u64;
                stream.hash.update(chunk);
                if let Some(kept) = &mut stream.kept {
                    kept.take(chunk);
                }
            }
            let wanted = n as u64;
            let left = budget
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |b| {
                    Some(b.saturating_sub(wanted))
                })
                .unwrap_or(0);
            let allowed = usize::try_from(left.min(wanted)).unwrap_or(0);
            if allowed > 0 {
                sink(&chunk[..allowed]);
            }
            dropped.fetch_add(wanted - allowed as u64, Ordering::SeqCst);
        }
        let _ = done.send(());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::relay::sha256_hex;

    /// Past the budget the stream keeps draining: what passes is the head,
    /// what is dropped is counted, and the hash covers every byte.
    #[test]
    fn a_pumped_stream_is_cut_at_the_budget_and_hashed_whole() {
        let input: Vec<u8> = (0..3 * CHUNK).map(|i| (i % 251) as u8).collect();
        let stream = Arc::new(Mutex::new(Stream {
            kept: Some(KeptBytes::default()),
            ..Stream::default()
        }));
        let budget = Arc::new(AtomicU64::new(CHUNK as u64 + 100));
        let dropped = Arc::new(AtomicU64::new(0));
        let passed = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done) = mpsc::channel();
        let sink = Arc::clone(&passed);
        pump(
            std::io::Cursor::new(input.clone()),
            Arc::clone(&stream),
            budget,
            Arc::clone(&dropped),
            done_tx,
            move |bytes| sink.lock().unwrap().extend_from_slice(bytes),
        );
        done.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(*passed.lock().unwrap(), input[..CHUNK + 100]);
        assert_eq!(
            dropped.load(Ordering::SeqCst),
            input.len() as u64 - (CHUNK as u64 + 100)
        );
        let stream = stream.lock().unwrap();
        assert_eq!(stream.bytes, input.len() as u64);
        assert_eq!(stream.summary().sha256, sha256_hex(&input));
        assert!(!stream.kept.as_ref().unwrap().bytes().is_empty());
    }
}
