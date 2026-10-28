//! The bytes `officina-fake-tool` writes in mode `bytes:<n>`. The fake tool
//! includes this file by path, so the tests that check what passed through
//! `officina run` compute the same bytes from the same definition.

/// Byte `index` of the fake tool's stdout: every value, cycling.
pub fn stdout_byte(index: u64) -> u8 {
    (index % 256) as u8
}

/// Byte `index` of the fake tool's stderr: the same cycle, reversed, so the
/// two streams never hash alike.
pub fn stderr_byte(index: u64) -> u8 {
    255 - (index % 256) as u8
}

/// The first `len` bytes of a stream.
pub fn stream(len: u64, byte: fn(u64) -> u8) -> Vec<u8> {
    (0..len).map(byte).collect()
}
