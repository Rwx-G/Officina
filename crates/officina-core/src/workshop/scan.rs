//! The credential scan (AD-112, D-237, D-609): every form of a credential's
//! value a careless worker could write, and where they are in a text.
//! Search is a plain substring search per needle; no crate.

use std::ops::Range;

/// What a found value is replaced with in every text Officina stores.
pub const REPLACEMENT: &str = "[credential]";

/// The shortest needle searched: shorter forms (a base64 run of a short
/// value) would match ordinary text.
pub const MIN_NEEDLE_BYTES: usize = 8;

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The forms of one value. Never printed: no `Debug` shows them.
pub struct Needles {
    plain: Vec<Vec<u8>>,
    /// Base64 forms, searched in texts with CR and LF removed, since
    /// coreutils `base64` wraps at 76 columns (D-237).
    base64: Vec<Vec<u8>>,
}

impl std::fmt::Debug for Needles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Needles({} forms)", self.plain.len() + self.base64.len())
    }
}

/// Base64 of `bytes` with `alphabet`, no padding.
fn encode(bytes: &[u8], alphabet: &[u8; 64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = chunk.len() + 1;
        for i in 0..chars {
            let index = usize::try_from((n >> (18 - 6 * i)) & 63).unwrap_or(0);
            out.push(alphabet[index]);
        }
    }
    out
}

/// The base64 characters of `value` that do not depend on its neighbours
/// when it sits after `offset` other bytes (AD-112): output character `i`
/// covers input bits `6i..6i+6`, kept only when they all are the value's.
fn aligned(value: &[u8], offset: usize, alphabet: &[u8; 64]) -> Vec<u8> {
    let mut prefixed = vec![0u8; offset];
    prefixed.extend_from_slice(value);
    let encoded = encode(&prefixed, alphabet);
    let start = (8 * offset).div_ceil(6);
    let end = (8 * (offset + value.len())) / 6;
    encoded
        .get(start..end)
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

/// Every form of `value` (AD-112): the value raw; its standard and URL-safe
/// base64 at each of the three byte alignments, minus the edge characters
/// its neighbours change; its lowercase and uppercase hex.
pub fn needles(value: &[u8]) -> Needles {
    let mut plain: Vec<Vec<u8>> = vec![value.to_vec()];
    let hex: String = value.iter().map(|b| format!("{b:02x}")).collect();
    plain.push(hex.clone().into_bytes());
    plain.push(hex.to_ascii_uppercase().into_bytes());
    let mut base64 = Vec::new();
    for alphabet in [STANDARD, URL_SAFE] {
        for offset in 0..3 {
            base64.push(aligned(value, offset, alphabet));
        }
    }
    let keep = |forms: &mut Vec<Vec<u8>>| {
        forms.retain(|form| form.len() >= MIN_NEEDLE_BYTES);
        forms.sort();
        forms.dedup();
    };
    keep(&mut plain);
    keep(&mut base64);
    Needles { plain, base64 }
}

impl Needles {
    /// No form to search: the value was too short, or there was none.
    pub fn is_empty(&self) -> bool {
        self.plain.is_empty() && self.base64.is_empty()
    }
}

fn occurrences(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i)
        .collect()
}

fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// The byte ranges of `text` holding a form of the value, merged and in
/// order. A base64 form is found across CR and LF, its range spanning
/// them.
pub fn find(text: &[u8], needles: &Needles) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for needle in &needles.plain {
        ranges.extend(
            occurrences(text, needle)
                .into_iter()
                .map(|i| i..i + needle.len()),
        );
    }
    let kept: Vec<usize> = (0..text.len())
        .filter(|i| text[*i] != b'\r' && text[*i] != b'\n')
        .collect();
    let stripped: Vec<u8> = kept.iter().map(|i| text[*i]).collect();
    for needle in &needles.base64 {
        for start in occurrences(&stripped, needle) {
            let end = start + needle.len() - 1;
            ranges.push(kept[start]..kept[end] + 1);
        }
    }
    merge(ranges)
}

/// `text` with every range [`find`] reports replaced by [`REPLACEMENT`].
pub fn replace(text: &str, needles: &Needles) -> String {
    apply(text.as_bytes(), &find(text.as_bytes(), needles))
}

/// `text` with each of `ranges` replaced by [`REPLACEMENT`], read as
/// UTF-8 lossily. The ranges may come from outside the window (D-609's
/// `Scanner`): they are merged, and cut to the text, before use.
pub fn apply(text: &[u8], ranges: &[Range<usize>]) -> String {
    let within: Vec<Range<usize>> = ranges
        .iter()
        .map(|r| r.start.min(text.len())..r.end.min(text.len()))
        .filter(|r| r.start < r.end)
        .collect();
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    for range in merge(within) {
        out.extend_from_slice(&text[at..range.start]);
        out.extend_from_slice(REPLACEMENT.as_bytes());
        at = range.end;
    }
    out.extend_from_slice(&text[at..]);
    String::from_utf8_lossy(&out).into_owned()
}

/// Where collection's scan gets the credential's byte ranges in the
/// run's records (AD-112, D-609): the run's needles on the host for the
/// token kinds, the `Scanner` container for a `login` run. Whatever the
/// source, the pass applies the ranges alike.
pub trait ScanSource {
    /// The ranges of each of `records`, in their order; `None` when the
    /// source could not scan, so nothing scanned is released.
    fn ranges(&mut self, records: &[&[u8]]) -> Option<Vec<Vec<Range<usize>>>>;
}

impl ScanSource for &Needles {
    fn ranges(&mut self, records: &[&[u8]]) -> Option<Vec<Vec<Range<usize>>>> {
        Some(records.iter().map(|record| find(record, self)).collect())
    }
}

/// Where a scanned record came from (AD-112): what a location names,
/// never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Report,
    /// A stream text kept: an entry line's detail, the result's subtype.
    Stream,
    GatewayLine,
    Manifest,
    /// A commit's message, by commit id.
    CommitMessage(String),
    /// A commit's author and committer lines, by commit id.
    Author(String),
    /// A path the package names, or a tree listing names.
    Path,
    /// A blob's content, by the record holding its path.
    File(Option<usize>),
}

/// One record the scan reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRecord {
    pub place: Place,
    pub bytes: Vec<u8>,
}

/// The first 12 characters of a commit id, as a location shows it.
fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

/// The location of every record where `ranges` found the value, in
/// record order (AD-112): "commit message <id>", "author <id>", "file
/// <path>" with the path's own ranges applied, "path", "report",
/// "stream", "gateway line", "manifest".
pub fn locations(records: &[ScanRecord], ranges: &[Vec<Range<usize>>]) -> Vec<String> {
    let cleaned = |index: usize| {
        let empty = Vec::new();
        let found = ranges.get(index).unwrap_or(&empty);
        records
            .get(index)
            .map(|record| apply(&record.bytes, found))
            .unwrap_or_default()
    };
    records
        .iter()
        .zip(ranges)
        .filter(|(_, found)| !found.is_empty())
        .map(|(record, _)| match &record.place {
            Place::Report => "report".to_owned(),
            Place::Stream => "stream".to_owned(),
            Place::GatewayLine => "gateway line".to_owned(),
            Place::Manifest => "manifest".to_owned(),
            Place::CommitMessage(id) => format!("commit message {}", short(id)),
            Place::Author(id) => format!("author {}", short(id)),
            Place::Path => "path".to_owned(),
            Place::File(Some(path)) => format!("file {}", cleaned(*path)),
            Place::File(None) => "file".to_owned(),
        })
        .collect()
}

/// The values of a stored credentials file that become needles (D-609):
/// every JSON string of at least 32 characters with no whitespace and no
/// `@`, wherever it sits, no field name assumed. An email address never
/// becomes one, so a commit's author never quarantines a package.
pub fn login_values(json: &str) -> Vec<String> {
    fn walk(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s)
                if s.chars().count() >= 32
                    && !s.contains('@')
                    && !s.chars().any(char::is_whitespace) =>
            {
                out.push(s.clone());
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            serde_json::Value::Object(map) => map.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
        walk(&value, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seeded value, derived here and never typed in an encoded form.
    fn seeded() -> Vec<u8> {
        (0u8..48).map(|i| b'A' + (i % 26 * 7) % 26).collect()
    }

    fn forms(value: &[u8]) -> Vec<Vec<u8>> {
        let n = needles(value);
        n.plain.iter().chain(&n.base64).cloned().collect()
    }

    /// D-609: ranges from another source are cut to the text and merged
    /// before they are applied; none past the end, none reversed.
    #[test]
    fn apply_cuts_and_merges_foreign_ranges() {
        let text = b"abcdefghij";
        assert_eq!(apply(text, &[2..4, 3..6]), "ab[credential]ghij");
        let past_the_end = std::iter::once(8..40).collect::<Vec<_>>();
        assert_eq!(apply(text, &past_the_end), "abcdefgh[credential]");
        assert_eq!(apply(text, &[20..30, 5..5]), "abcdefghij");
        assert_eq!(apply(text, &[]), "abcdefghij");
    }

    /// AD-112: a location names where, never the value: a file's path
    /// carries its own ranges applied.
    #[test]
    fn locations_never_carry_the_value() {
        let value = seeded();
        let n = needles(&value);
        let mut path = b"docs/".to_vec();
        path.extend_from_slice(&value);
        let records = vec![
            ScanRecord {
                place: Place::Path,
                bytes: path,
            },
            ScanRecord {
                place: Place::File(Some(0)),
                bytes: value.clone(),
            },
            ScanRecord {
                place: Place::Report,
                bytes: b"nothing here".to_vec(),
            },
            ScanRecord {
                place: Place::CommitMessage("a".repeat(40)),
                bytes: value.clone(),
            },
        ];
        let slices: Vec<&[u8]> = records.iter().map(|r| r.bytes.as_slice()).collect();
        let ranges = (&n).ranges(&slices).unwrap();
        let found = locations(&records, &ranges);
        assert_eq!(
            found,
            [
                "path".to_owned(),
                format!("file docs/{REPLACEMENT}"),
                format!("commit message {}", "a".repeat(12)),
            ]
        );
        let value = String::from_utf8(value).unwrap();
        assert!(found.iter().all(|l| !l.contains(&value)));
    }

    #[test]
    fn every_form_of_a_seeded_value_is_found() {
        let value = seeded();
        let n = needles(&value);
        for form in forms(&value) {
            let mut text = b"before ".to_vec();
            text.extend(&form);
            text.extend(b" after");
            assert!(
                !find(&text, &n).is_empty(),
                "{}",
                String::from_utf8_lossy(&form)
            );
        }
    }

    #[test]
    fn a_value_embedded_at_each_alignment_is_found_in_base64() {
        let value = seeded();
        let n = needles(&value);
        for alphabet in [STANDARD, URL_SAFE] {
            for prefix in 0..3 {
                let mut data = vec![0x5au8; prefix];
                data.extend(&value);
                data.extend([0xa5u8; 2]);
                let encoded = encode(&data, alphabet);
                assert!(!find(&encoded, &n).is_empty(), "prefix {prefix}");
            }
        }
    }

    #[test]
    fn a_wrapped_base64_form_is_found() {
        let value: Vec<u8> = seeded().repeat(3);
        let n = needles(&value);
        let encoded = encode(&value, STANDARD);
        let mut wrapped = Vec::new();
        for (i, chunk) in encoded.chunks(76).enumerate() {
            if i > 0 {
                wrapped.extend(b"\r\n");
            }
            wrapped.extend(chunk);
        }
        let found = find(&wrapped, &n);
        assert_eq!(found.len(), 1);
        assert!(found[0].end - found[0].start > 76);
    }

    #[test]
    fn replace_leaves_no_form() {
        let value = seeded();
        let n = needles(&value);
        let mut text = String::from("token: ");
        for form in forms(&value) {
            text.push_str(&String::from_utf8_lossy(&form));
            text.push(' ');
        }
        let replaced = replace(&text, &n);
        assert!(find(replaced.as_bytes(), &n).is_empty());
        assert!(replaced.contains(REPLACEMENT));
    }

    #[test]
    fn an_unrelated_text_is_not_found() {
        let n = needles(&seeded());
        assert!(find(b"an ordinary commit message about dependencies", &n).is_empty());
        assert!(format!("{n:?}").contains("forms"));
    }

    #[test]
    fn login_values_skip_emails_and_short_values() {
        let long = "x".repeat(40);
        let json = format!(
            r#"{{"a":{{"accessToken":"{long}","email":"user01@example.com{long}","short":"abc","spaced":"{long} {long}"}}}}"#
        );
        assert_eq!(login_values(&json), [long]);
    }
}
