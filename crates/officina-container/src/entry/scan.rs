//! `officina-entry scan` (D-609): the credential scan of a `login` run,
//! whose credential the host never reads. Needles come from the stored
//! login; records come framed on stdin; the answer is ranges, never a
//! value.

use std::io::BufRead;

use officina_core::workshop::scan::{
    Needles, find, login_values, needles, scanner_error_line, scanner_hits_line,
    scanner_no_needles_line,
};
use officina_core::workshop::vector::LOGIN_DIR;

use super::Layout;
use super::auth_status::CREDENTIALS_FILE;

/// The largest record read (AD-112's scope per blob).
pub const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;
/// The most read over all records, past which the scan stops refusing.
pub const MAX_TOTAL_BYTES: usize = 1024 * 1024 * 1024;

/// Reads framed records from `input` (a header line `<index> <length>`,
/// then the record's bytes) and returns, per record with hits, its line.
/// An oversized or malformed frame ends the scan with an error line.
pub fn scan_records(input: &mut dyn BufRead, needles: &[Needles]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut total = 0usize;
    loop {
        let mut header = String::new();
        match input.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => {
                lines.push(scanner_error_line(None, "unreadable"));
                break;
            }
        }
        let mut parts = header.split_whitespace();
        let (Some(index), Some(len)) = (
            parts.next().and_then(|i| i.parse::<u64>().ok()),
            parts.next().and_then(|n| n.parse::<usize>().ok()),
        ) else {
            lines.push(scanner_error_line(None, "malformed"));
            break;
        };
        total = total.saturating_add(len);
        if len > MAX_RECORD_BYTES || total > MAX_TOTAL_BYTES {
            lines.push(scanner_error_line(Some(index), "too_large"));
            break;
        }
        let mut record = vec![0u8; len];
        if input.read_exact(&mut record).is_err() {
            lines.push(scanner_error_line(Some(index), "truncated"));
            break;
        }
        let mut ranges: Vec<[usize; 2]> = needles
            .iter()
            .flat_map(|n| find(&record, n))
            .map(|r| [r.start, r.end])
            .collect();
        ranges.sort_unstable();
        ranges.dedup();
        if !ranges.is_empty() {
            lines.push(scanner_hits_line(index, &ranges));
        }
    }
    lines
}

/// Builds the needles from the stored login and scans stdin's records;
/// with no login, prints `{"needles": 0}` alone.
pub fn scan(layout: &Layout) -> i32 {
    let stored =
        std::fs::read_to_string(layout.at(LOGIN_DIR).join(CREDENTIALS_FILE)).unwrap_or_default();
    let all: Vec<Needles> = login_values(&stored)
        .iter()
        .map(|value| needles(value.as_bytes()))
        .filter(|n| !n.is_empty())
        .collect();
    if all.is_empty() {
        println!("{}", scanner_no_needles_line());
        return 0;
    }
    let stdin = std::io::stdin();
    for line in scan_records(&mut stdin.lock(), &all) {
        println!("{line}");
    }
    0
}
