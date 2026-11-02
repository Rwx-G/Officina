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

/// The `path` a PAX record set gives, if any; called after
/// [`pax_records`] accepted the set.
fn pax_path(bytes: &[u8]) -> Option<String> {
    let mut rest = bytes;
    let mut path = None;
    while let Some(space) = rest.iter().position(|b| *b == b' ') {
        let length: usize = std::str::from_utf8(&rest[..space]).ok()?.parse().ok()?;
        let record = rest.get(space + 1..length.checked_sub(1)?)?;
        if let Some(value) = record.strip_prefix(b"path=") {
            path = Some(String::from_utf8_lossy(value).into_owned());
        }
        rest = rest.get(length..)?;
    }
    path
}

/// The files a package may hold (D-229): every other name in the stream
/// is `bundle_invalid`. Compared exactly: a backslash, a drive or stream
/// colon, a device name, a trailing dot or space, another case or any
/// other path fails the comparison before anything is written.
pub const PACKAGE_DIR: &str = "out/";
/// The tip's name in the stream.
pub const PACKAGE_TIP: &str = "out/tip";
/// The manifest's name in the stream.
pub const PACKAGE_MANIFEST: &str = "out/manifest.json";
/// The bundle's name in the stream.
pub const PACKAGE_BUNDLE: &str = "out/work.bundle";

/// The most bytes of `tip` kept in memory.
pub const MAX_TIP_BYTES: usize = 128;
/// The most bytes of `manifest.json` kept in memory.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// Why a package stream is refused: every reason is `bundle_invalid`
/// (D-229), the variant says which for the record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageError {
    #[error(transparent)]
    Tar(#[from] TarError),
    #[error("a name outside the package's list")]
    NameRefused,
    #[error("a name the stream holds twice")]
    Duplicate,
    #[error("an entry of a type the package never holds")]
    TypeRefused,
    #[error("the tip or the manifest is larger than its bound")]
    TooLong,
    #[error("the host refused the bundle's bytes: {0}")]
    Sink(String),
}

/// One stream entry's name and type as the reader classified it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Skip,
    Tip,
    Manifest,
    Bundle,
}

/// What a package stream held (AD-110, D-229): the tip and the manifest in
/// memory, and how many bundle bytes went to the host's sink.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package {
    pub tip: Option<Vec<u8>>,
    pub manifest: Option<Vec<u8>>,
    pub bundle_bytes: Option<u64>,
}

/// Reads `docker cp <packer>:/work/.officina/out -` chunk by chunk (D-218,
/// D-229): only the allow-listed names, `tip` and `manifest.json` kept in
/// memory within their bounds, `work.bundle`'s bytes handed to the host's
/// sink, which writes them under a name Officina chose.
#[derive(Debug, Default)]
pub struct PackageReader {
    buffer: Vec<u8>,
    phase: Phase,
    pax_size: Option<u64>,
    pax_path: Option<String>,
    seen: Vec<String>,
    zero_blocks: u32,
    package: Package,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Header,
    Pax {
        size: u64,
        global: bool,
    },
    Content {
        slot: Slot,
        left: u64,
        pad: u64,
    },
    Padding {
        left: u64,
    },
    End,
}

/// The entry's name: the ustar prefix joined to the name field, or the
/// PAX `path`.
fn header_name(header: &[u8]) -> String {
    let field = |range: std::ops::Range<usize>| {
        let bytes = &header[range];
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    };
    let name = field(0..100);
    let prefix = if &header[257..262] == b"ustar" {
        field(345..500)
    } else {
        String::new()
    };
    if prefix.is_empty() {
        name
    } else {
        format!("{prefix}/{name}")
    }
}

impl PackageReader {
    pub fn new() -> PackageReader {
        PackageReader::default()
    }

    /// Feeds the next bytes; the bundle's bytes go to `sink`.
    pub fn push(
        &mut self,
        mut chunk: &[u8],
        sink: &mut dyn FnMut(&[u8]) -> Result<(), String>,
    ) -> Result<(), PackageError> {
        while !chunk.is_empty() {
            match self.phase {
                Phase::End => return Ok(()),
                Phase::Header => {
                    let take = (BLOCK - self.buffer.len()).min(chunk.len());
                    self.buffer.extend_from_slice(&chunk[..take]);
                    chunk = &chunk[take..];
                    if self.buffer.len() == BLOCK {
                        let header = std::mem::take(&mut self.buffer);
                        self.header(&header)?;
                    }
                }
                Phase::Pax { size, global } => {
                    let want = usize::try_from(size).unwrap_or(usize::MAX);
                    let take = (want - self.buffer.len()).min(chunk.len());
                    self.buffer.extend_from_slice(&chunk[..take]);
                    chunk = &chunk[take..];
                    if self.buffer.len() == want {
                        let records = std::mem::take(&mut self.buffer);
                        let size = pax_records(&records)?;
                        if !global {
                            self.pax_size = size;
                            self.pax_path = pax_path(&records);
                        }
                        self.phase = Phase::Padding {
                            left: padding(records.len() as u64),
                        };
                    }
                }
                Phase::Content { slot, left, pad } => {
                    let take = usize::try_from(left).unwrap_or(usize::MAX).min(chunk.len());
                    let part = &chunk[..take];
                    match slot {
                        Slot::Tip => keep(&mut self.package.tip, part, MAX_TIP_BYTES)?,
                        Slot::Manifest => {
                            keep(&mut self.package.manifest, part, MAX_MANIFEST_BYTES)?
                        }
                        Slot::Bundle => {
                            sink(part).map_err(PackageError::Sink)?;
                            let total = self.package.bundle_bytes.get_or_insert(0);
                            *total += take as u64;
                        }
                        Slot::Skip => {}
                    }
                    chunk = &chunk[take..];
                    let left = left - take as u64;
                    self.phase = if left == 0 {
                        Phase::Padding { left: pad }
                    } else {
                        Phase::Content { slot, left, pad }
                    };
                }
                Phase::Padding { left } => {
                    let take = usize::try_from(left).unwrap_or(usize::MAX).min(chunk.len());
                    chunk = &chunk[take..];
                    self.phase = Phase::Padding {
                        left: left - take as u64,
                    };
                }
            }
            if self.phase == (Phase::Padding { left: 0 }) {
                self.phase = Phase::Header;
            }
        }
        Ok(())
    }

    fn header(&mut self, header: &[u8]) -> Result<(), PackageError> {
        if header.iter().all(|b| *b == 0) {
            self.zero_blocks += 1;
            if self.zero_blocks == 2 {
                self.phase = Phase::End;
            }
            return Ok(());
        }
        if self.zero_blocks > 0 || !checksum_ok(header)? {
            return Err(TarError::BadChecksum.into());
        }
        let size = number(&header[124..136])?;
        if matches!(header[156], b'x' | b'g') {
            if size > MAX_PAX_BYTES as u64 {
                return Err(TarError::PaxTooLarge(MAX_PAX_BYTES).into());
            }
            self.phase = if size == 0 {
                Phase::Header
            } else {
                Phase::Pax {
                    size,
                    global: header[156] == b'g',
                }
            };
            return Ok(());
        }
        let name = self.pax_path.take().unwrap_or_else(|| header_name(header));
        let size = self.pax_size.take().unwrap_or(size);
        let folded = name.to_lowercase();
        if self.seen.contains(&folded) {
            return Err(PackageError::Duplicate);
        }
        self.seen.push(folded);
        let slot = match (header[156], name.as_str()) {
            (b'5', PACKAGE_DIR) => Slot::Skip,
            (b'0' | 0, PACKAGE_TIP) => Slot::Tip,
            (b'0' | 0, PACKAGE_MANIFEST) => Slot::Manifest,
            (b'0' | 0, PACKAGE_BUNDLE) => Slot::Bundle,
            (b'0' | 0 | b'5', _) => return Err(PackageError::NameRefused),
            _ => return Err(PackageError::TypeRefused),
        };
        match slot {
            Slot::Tip => self.package.tip = Some(Vec::new()),
            Slot::Manifest => self.package.manifest = Some(Vec::new()),
            Slot::Bundle => self.package.bundle_bytes = Some(0),
            Slot::Skip => {}
        }
        self.phase = if size == 0 {
            Phase::Header
        } else {
            Phase::Content {
                slot,
                left: size,
                pad: padding(size),
            }
        };
        Ok(())
    }

    /// What the stream held, once it ended.
    pub fn finish(self) -> Result<Package, PackageError> {
        match self.phase {
            Phase::End | Phase::Header if self.buffer.is_empty() => Ok(self.package),
            _ => Err(TarError::Truncated.into()),
        }
    }
}

fn keep(kept: &mut Option<Vec<u8>>, part: &[u8], bound: usize) -> Result<(), PackageError> {
    let kept = kept.get_or_insert_with(Vec::new);
    if kept.len() + part.len() > bound {
        return Err(PackageError::TooLong);
    }
    kept.extend_from_slice(part);
    Ok(())
}

/// A ustar stream of `entries` (name, bytes, type), ended by two zero
/// blocks: for tests and the fake engine's package.
pub fn stream_of(entries: &[(&str, &[u8], u8)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, bytes, kind) in entries {
        let mut one = single_file(name, bytes, *kind);
        one.truncate(one.len() - 2 * BLOCK);
        out.extend(one);
    }
    out.resize(out.len() + 2 * BLOCK, 0);
    out
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
