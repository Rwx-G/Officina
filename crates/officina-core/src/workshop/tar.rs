//! The tar streams `docker cp <container>:<path> -` writes (D-218): read in
//! core, pushed chunk by chunk, so the host never writes what a container
//! holds before core has looked at it. Story 6.8 measures one program per
//! stream with [`FileDigest`]; story 6.13's package reader joins here.
//!
//! The format is ustar with PAX extended records (`x`, `g`) before long or
//! non-ASCII names (the gate 1 entry's saved stream). No crate: the header
//! fields are fixed.

use sha2::{Digest, Sha256};

/// One tar block.
pub const BLOCK: usize = 512;

/// Why a stream is not the one regular file it should be.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TarError {
    #[error("a header's checksum does not match")]
    BadChecksum,
    #[error("a header's size field is not a number")]
    BadSize,
    #[error("a PAX record is malformed")]
    BadPax,
    #[error("a PAX record changes what an entry is ({0})")]
    PaxRefused(String),
    #[error("an entry of type {0:?} is not a regular file")]
    NotRegular(char),
    #[error("the stream holds more than one file")]
    MoreThanOne,
    #[error("the stream holds no file")]
    NoFile,
    #[error("the stream ended inside an entry")]
    Truncated,
    #[error("an extended record is larger than {0} bytes")]
    PaxTooLarge(usize),
}

/// The largest PAX record set kept in memory to be parsed.
pub const MAX_PAX_BYTES: usize = 64 * 1024;

#[derive(Debug)]
enum State {
    /// Collecting a 512-byte header.
    Header,
    /// Reading a PAX record set of `size` bytes, then its padding.
    Pax { size: u64, global: bool },
    /// Hashing the file's `left` bytes, then `pad` bytes of padding.
    Content { left: u64, pad: u64 },
    /// Skipping `left` bytes of padding.
    Padding { left: u64 },
    /// Two zero blocks seen, or one followed by the end.
    End,
}

/// The SHA-256 of the one regular file a `docker cp` stream holds, fed in
/// chunks of any size (D-233). A link, a device, a folder or a second file
/// refuses the stream: `docker cp` of a program path that a project image
/// replaced with a link gives a link entry, never the file it names.
///
/// ```
/// use officina_core::workshop::tar::FileDigest;
/// let mut digest = FileDigest::new();
/// assert!(digest.finish().is_err(), "an empty stream holds no file");
/// ```
#[derive(Debug)]
pub struct FileDigest {
    state: State,
    buffer: Vec<u8>,
    hasher: Sha256,
    files: u32,
    zero_blocks: u32,
    pax_size: Option<u64>,
    bytes: u64,
}

impl Default for FileDigest {
    fn default() -> Self {
        FileDigest::new()
    }
}

fn padding(size: u64) -> u64 {
    let block = BLOCK as u64;
    (block - size % block) % block
}

/// A numeric header field: octal digits, or base-256 when the first byte's
/// high bit is set (GNU's form for large sizes).
fn number(field: &[u8]) -> Result<u64, TarError> {
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        let mut value: u64 = u64::from(field[0] & 0x7f);
        for byte in &field[1..] {
            value = value
                .checked_mul(256)
                .and_then(|v| v.checked_add(u64::from(*byte)))
                .ok_or(TarError::BadSize)?;
        }
        return Ok(value);
    }
    let text: String = field
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| char::from(*b))
        .collect();
    let digits = text.trim();
    if digits.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(digits, 8).map_err(|_| TarError::BadSize)
}

fn checksum_ok(header: &[u8]) -> Result<bool, TarError> {
    let stored = number(&header[148..156])?;
    let sum: u64 = header
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if (148..156).contains(&i) {
                u64::from(b' ')
            } else {
                u64::from(*b)
            }
        })
        .sum();
    Ok(sum == stored)
}

/// The `size` a PAX record set gives, if any; a `linkpath` or a `type`
/// changes what the entry is and is refused (D-218).
fn pax_records(bytes: &[u8]) -> Result<Option<u64>, TarError> {
    let mut size = None;
    let mut rest = bytes;
    while !rest.is_empty() {
        let space = rest
            .iter()
            .position(|b| *b == b' ')
            .ok_or(TarError::BadPax)?;
        let length: usize = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or(TarError::BadPax)?;
        if length <= space + 1 || length > rest.len() || rest[length - 1] != b'\n' {
            return Err(TarError::BadPax);
        }
        let record = &rest[space + 1..length - 1];
        let eq = record
            .iter()
            .position(|b| *b == b'=')
            .ok_or(TarError::BadPax)?;
        let key = String::from_utf8_lossy(&record[..eq]).into_owned();
        let value = &record[eq + 1..];
        match key.as_str() {
            "size" => {
                size = Some(
                    std::str::from_utf8(value)
                        .ok()
                        .and_then(|t| t.parse().ok())
                        .ok_or(TarError::BadPax)?,
                );
            }
            "linkpath" | "type" | "GNU.sparse.major" | "GNU.sparse.map" => {
                return Err(TarError::PaxRefused(key));
            }
            _ => {}
        }
        rest = &rest[length..];
    }
    Ok(size)
}

impl FileDigest {
    pub fn new() -> FileDigest {
        FileDigest {
            state: State::Header,
            buffer: Vec::with_capacity(BLOCK),
            hasher: Sha256::new(),
            files: 0,
            zero_blocks: 0,
            pax_size: None,
            bytes: 0,
        }
    }

    /// Feeds the next bytes of the stream.
    pub fn push(&mut self, mut chunk: &[u8]) -> Result<(), TarError> {
        while !chunk.is_empty() {
            match &mut self.state {
                State::End => return Ok(()),
                State::Header => {
                    let take = (BLOCK - self.buffer.len()).min(chunk.len());
                    self.buffer.extend_from_slice(&chunk[..take]);
                    chunk = &chunk[take..];
                    if self.buffer.len() == BLOCK {
                        let header = std::mem::take(&mut self.buffer);
                        self.header(&header)?;
                    }
                }
                State::Pax { size, global } => {
                    let want = usize::try_from(*size).unwrap_or(usize::MAX);
                    let take = (want - self.buffer.len()).min(chunk.len());
                    self.buffer.extend_from_slice(&chunk[..take]);
                    chunk = &chunk[take..];
                    if self.buffer.len() == want {
                        let global = *global;
                        let records = std::mem::take(&mut self.buffer);
                        let size = pax_records(&records)?;
                        if !global {
                            self.pax_size = size;
                        }
                        self.state = State::Padding {
                            left: padding(records.len() as u64),
                        };
                    }
                }
                State::Content { left, pad } => {
                    let take = usize::try_from(*left)
                        .unwrap_or(usize::MAX)
                        .min(chunk.len());
                    self.hasher.update(&chunk[..take]);
                    self.bytes += take as u64;
                    *left -= take as u64;
                    chunk = &chunk[take..];
                    if *left == 0 {
                        let pad = *pad;
                        self.state = State::Padding { left: pad };
                    }
                }
                State::Padding { left } => {
                    let take = usize::try_from(*left)
                        .unwrap_or(usize::MAX)
                        .min(chunk.len());
                    *left -= take as u64;
                    chunk = &chunk[take..];
                    if *left == 0 {
                        self.state = State::Header;
                    }
                }
            }
            if matches!(self.state, State::Padding { left: 0 }) {
                self.state = State::Header;
            }
        }
        Ok(())
    }

    fn header(&mut self, header: &[u8]) -> Result<(), TarError> {
        if header.iter().all(|b| *b == 0) {
            self.zero_blocks += 1;
            if self.zero_blocks == 2 {
                self.state = State::End;
            }
            return Ok(());
        }
        if self.zero_blocks > 0 {
            return Err(TarError::BadChecksum);
        }
        if !checksum_ok(header)? {
            return Err(TarError::BadChecksum);
        }
        let size = number(&header[124..136])?;
        match header[156] {
            b'x' | b'g' => {
                if size > MAX_PAX_BYTES as u64 {
                    return Err(TarError::PaxTooLarge(MAX_PAX_BYTES));
                }
                self.state = State::Pax {
                    size,
                    global: header[156] == b'g',
                };
            }
            b'0' | 0 => {
                if self.files > 0 {
                    return Err(TarError::MoreThanOne);
                }
                self.files += 1;
                let size = self.pax_size.take().unwrap_or(size);
                self.state = State::Content {
                    left: size,
                    pad: padding(size),
                };
            }
            other => return Err(TarError::NotRegular(char::from(other))),
        }
        if matches!(self.state, State::Pax { size: 0, .. }) {
            self.state = State::Header;
        }
        if matches!(self.state, State::Content { left: 0, .. }) {
            self.state = State::Header;
        }
        Ok(())
    }

    /// The file's SHA-256 and length once the stream ended.
    pub fn finish(self) -> Result<(String, u64), TarError> {
        match self.state {
            State::End | State::Header if self.buffer.is_empty() => {}
            _ => return Err(TarError::Truncated),
        }
        if self.files == 0 {
            return Err(TarError::NoFile);
        }
        Ok((crate::hex(&self.hasher.finalize()), self.bytes))
    }
}

/// A one-file ustar stream holding `bytes` at `name`, of entry type
/// `kind`, as `docker cp` writes it: for tests and the fake engine.
pub fn single_file(name: &str, bytes: &[u8], kind: u8) -> Vec<u8> {
    let mut header = [0u8; BLOCK];
    let name_bytes = name.as_bytes();
    let n = name_bytes.len().min(100);
    header[..n].copy_from_slice(&name_bytes[..n]);
    header[100..107].copy_from_slice(b"0000755");
    header[108..115].copy_from_slice(b"0000000");
    header[116..123].copy_from_slice(b"0000000");
    let size = format!("{:011o}", bytes.len());
    header[124..135].copy_from_slice(size.as_bytes());
    header[136..147].copy_from_slice(b"00000000000");
    header[156] = kind;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    let check = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(check.as_bytes());
    let mut out = header.to_vec();
    out.extend_from_slice(bytes);
    out.resize(
        out.len() + usize::try_from(padding(bytes.len() as u64)).unwrap_or(0),
        0,
    );
    out.resize(out.len() + 2 * BLOCK, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest_in_chunks(stream: &[u8], chunk: usize) -> Result<(String, u64), TarError> {
        let mut digest = FileDigest::new();
        for part in stream.chunks(chunk) {
            digest.push(part)?;
        }
        digest.finish()
    }

    #[test]
    fn a_regular_file_hashes_to_its_bytes_whatever_the_chunks() {
        let bytes: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let stream = single_file("officina-entry", &bytes, b'0');
        for chunk in [1, 7, 511, 512, 513, 4096, stream.len()] {
            assert_eq!(
                digest_in_chunks(&stream, chunk),
                Ok((crate::sha256_hex(&bytes), bytes.len() as u64)),
                "chunk {chunk}"
            );
        }
        let empty = single_file("empty", b"", b'0');
        assert_eq!(
            digest_in_chunks(&empty, 100),
            Ok((crate::sha256_hex(b""), 0))
        );
    }

    #[test]
    fn a_link_or_a_folder_is_refused() {
        for kind in *b"123456" {
            let stream = single_file("claude", b"", kind);
            assert_eq!(
                digest_in_chunks(&stream, 512),
                Err(TarError::NotRegular(char::from(kind)))
            );
        }
    }

    #[test]
    fn two_files_or_none_are_refused() {
        let mut two = single_file("a", b"1", b'0');
        two.truncate(two.len() - 2 * BLOCK);
        two.extend(single_file("b", b"2", b'0'));
        assert_eq!(digest_in_chunks(&two, 512), Err(TarError::MoreThanOne));
        assert_eq!(FileDigest::new().finish(), Err(TarError::NoFile));
    }

    #[test]
    fn a_cut_stream_or_a_bad_checksum_is_refused() {
        let stream = single_file("a", &[7u8; 1000], b'0');
        assert_eq!(
            digest_in_chunks(&stream[..700], 512),
            Err(TarError::Truncated)
        );
        let mut bad = stream.clone();
        bad[0] ^= 1;
        assert_eq!(digest_in_chunks(&bad, 512), Err(TarError::BadChecksum));
    }

    /// The gate 1 entry's saved stream: PAX `x` records before the long and
    /// the non-ASCII names, a directory first. As a whole it holds more
    /// than one entry, so the one-file reader refuses it; its PAX records
    /// parse on the way.
    #[test]
    fn the_saved_docker_stream_parses_its_pax_records() {
        let stream = include_bytes!("../../../../tests/vectors/fixtures/docker-cp-out.tar");
        assert_eq!(
            digest_in_chunks(stream, 300),
            Err(TarError::NotRegular('5')),
            "the directory entry comes first"
        );
        // The long name's PAX record and its file, then the end: the one
        // file it then holds hashes to its five bytes.
        let mut long_name = stream[512..2560].to_vec();
        long_name.resize(long_name.len() + 2 * BLOCK, 0);
        let (_, length) = digest_in_chunks(&long_name, 100).expect("PAX then a file");
        assert_eq!(length, 5);
        assert_eq!(pax_records(b"20 path=a/b/c/d/e/f\n"), Ok(None));
        assert_eq!(pax_records(b"11 size=42\n"), Ok(Some(42)));
        assert_eq!(
            pax_records(b"17 linkpath=/etc\n"),
            Err(TarError::PaxRefused("linkpath".into()))
        );
        assert_eq!(pax_records(b"99 size=1\n"), Err(TarError::BadPax));
    }

    #[test]
    fn a_pax_size_overrides_the_header() {
        let bytes = b"0123456789";
        let mut stream = Vec::new();
        let record = b"11 size=10\n";
        let mut pax = single_file("PaxHeaders/x", record, b'x');
        pax.truncate(pax.len() - 2 * BLOCK);
        stream.extend(pax);
        let mut file = single_file("x", bytes, b'0');
        // The ustar size field says 0; the PAX record says 10.
        file[124..135].copy_from_slice(b"00000000000");
        file[148..156].copy_from_slice(b"        ");
        let sum: u32 = file[..BLOCK].iter().map(|b| u32::from(*b)).sum();
        file[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        stream.extend(file);
        assert_eq!(
            digest_in_chunks(&stream, 64),
            Ok((crate::sha256_hex(bytes), 10))
        );
    }
}
