//! The kept output of a check run (D-170): the first bytes of what it
//! printed, written for the approval card of the action naming it.

use std::io::Write;

use officina_core::relay::approvals::{KEPT_OUTPUT_BYTES, kept_output};

use crate::relay::approvals;
use crate::relay::home::RelayHome;

/// The first [`KEPT_OUTPUT_BYTES`] bytes of one stream, whatever its size.
#[derive(Debug, Default)]
pub(super) struct KeptBytes(Vec<u8>);

impl KeptBytes {
    /// Keeps what fits of `bytes`, the next ones the stream carried.
    pub(super) fn take(&mut self, bytes: &[u8]) {
        let room = KEPT_OUTPUT_BYTES
            .saturating_sub(self.0.len())
            .min(bytes.len());
        self.0.extend_from_slice(&bytes[..room]);
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Passes what an HTTP run prints on, keeping its first bytes for a check
/// run.
pub(super) struct Tee<'a> {
    out: &'a mut dyn Write,
    kept: Option<KeptBytes>,
    total: u64,
}

impl<'a> Tee<'a> {
    /// `keep`: also keep the first bytes (a check run).
    pub(super) fn new(out: &'a mut dyn Write, keep: bool) -> Tee<'a> {
        Tee {
            out,
            kept: keep.then(KeptBytes::default),
            total: 0,
        }
    }

    /// What the run keeps, with the cut marker; `None` when it keeps
    /// nothing.
    pub(super) fn kept(&self) -> Option<Vec<u8>> {
        let kept = self.kept.as_ref()?;
        Some(kept_output(kept.bytes(), &[], self.total, 0))
    }
}

impl Write for Tee<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let written = self.out.write(bytes)?;
        self.total += written as u64;
        if let Some(kept) = &mut self.kept {
            kept.take(&bytes[..written]);
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// Writes a check run's kept output (D-170); a failure is said on `err`, the
/// run stands.
pub(super) fn keep(home: &RelayHome, err: &mut dyn Write, run_id: &str, bytes: &[u8]) {
    if let Err(e) = approvals::keep_output(home, run_id, bytes) {
        let _ = writeln!(err, "officina: this check's output could not be kept: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_bytes_are_kept_across_writes() {
        let mut kept = KeptBytes::default();
        let chunk = vec![b'x'; KEPT_OUTPUT_BYTES / 2 + 1];
        kept.take(&chunk);
        kept.take(&chunk);
        kept.take(b"more");
        assert_eq!(kept.bytes(), &vec![b'x'; KEPT_OUTPUT_BYTES][..]);
    }

    #[test]
    fn a_tee_passes_every_byte_and_keeps_the_head() {
        let mut out = Vec::new();
        let body = vec![b'y'; KEPT_OUTPUT_BYTES + 10];
        let mut tee = Tee::new(&mut out, true);
        tee.write_all(&body).unwrap();
        let kept = tee.kept().unwrap();
        assert_eq!(
            kept,
            kept_output(&body[..KEPT_OUTPUT_BYTES], &[], body.len() as u64, 0)
        );
        assert_eq!(out, body);
        assert_eq!(Tee::new(&mut Vec::new(), false).kept(), None);
    }
}
