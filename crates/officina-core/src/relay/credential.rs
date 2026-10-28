//! Credentials of HTTP actions (AD-36, D-37, D-40): what the Windows
//! Credential Manager holds for `officina:<name>`, decoded here. The host
//! reads the blob and keeps the secret out of every output; this module
//! only decodes it, and never formats it.

/// The Credential Manager's target of a credential: this prefix, then the
/// credential's name (AD-36).
pub const CREDENTIAL_TARGET_PREFIX: &str = "officina:";

/// The target `cmdkey /generic` and `CredReadW` use for `name`.
///
/// ```
/// assert_eq!(officina_core::relay::credential::target("monitoring"), "officina:monitoring");
/// ```
pub fn target(name: &str) -> String {
    format!("{CREDENTIAL_TARGET_PREFIX}{name}")
}

/// The line doctor prints for a missing credential (D-38): `/pass` without
/// a value, so `cmdkey` prompts for the secret and it never lands in the
/// shell's history; `<user>` is a placeholder the author replaces.
///
/// ```
/// assert_eq!(
///     officina_core::relay::credential::cmdkey_line("monitoring"),
///     "cmdkey /generic:officina:monitoring /user:<user> /pass",
/// );
/// ```
pub fn cmdkey_line(name: &str) -> String {
    format!("cmdkey /generic:{} /user:<user> /pass", target(name))
}

/// Why a stored blob is not a usable secret. Never carries the secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BlobError {
    #[error("the stored secret is empty")]
    Empty,
    #[error("the stored secret has an odd byte length: not UTF-16")]
    OddLength,
    #[error("the stored secret holds a lone surrogate: not UTF-16")]
    LoneSurrogate,
    #[error("the stored secret holds a line break, which a header cannot carry")]
    LineBreak,
    #[error("the stored secret holds a control character, which a header cannot carry")]
    ControlCharacter,
}

/// Decodes a credential blob as UTF-16LE, what `cmdkey /generic ... /pass`
/// stores. An odd length, a lone surrogate, a CR or LF, or any other control
/// character but a tab is refused: the secret goes into one header line.
///
/// ```
/// use officina_core::relay::credential::decode_blob;
/// let blob: Vec<u8> = "s3cret".encode_utf16().flat_map(u16::to_le_bytes).collect();
/// assert_eq!(decode_blob(&blob).unwrap(), "s3cret");
/// ```
pub fn decode_blob(blob: &[u8]) -> Result<String, BlobError> {
    if blob.is_empty() {
        return Err(BlobError::Empty);
    }
    if !blob.len().is_multiple_of(2) {
        return Err(BlobError::OddLength);
    }
    let (pairs, _) = blob.as_chunks::<2>();
    let units = pairs.iter().map(|pair| u16::from_le_bytes(*pair));
    let secret = char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .map_err(|_| BlobError::LoneSurrogate)?;
    if secret.contains(['\r', '\n']) {
        return Err(BlobError::LineBreak);
    }
    if secret.chars().any(|c| c.is_control() && c != '\t') {
        return Err(BlobError::ControlCharacter);
    }
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le(units: &[u16]) -> Vec<u8> {
        units.iter().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn decodes_a_cmdkey_blob() {
        let text = "Bearer-tok3n_é~€𝄞";
        let units: Vec<u16> = text.encode_utf16().collect();
        assert_eq!(decode_blob(&utf16le(&units)).unwrap(), text);
        assert_eq!(decode_blob(&utf16le(&[0x61, 0x09, 0x62])).unwrap(), "a\tb");
        assert_eq!(decode_blob(&[]), Err(BlobError::Empty));
    }

    #[test]
    fn odd_length_is_refused() {
        assert_eq!(decode_blob(&[0x61, 0x00, 0x62]), Err(BlobError::OddLength));
    }

    #[test]
    fn lone_surrogate_is_refused() {
        for units in [&[0xD834][..], &[0x61, 0xDD1E], &[0xD834, 0x61]] {
            assert_eq!(
                decode_blob(&utf16le(units)),
                Err(BlobError::LoneSurrogate),
                "{units:x?}"
            );
        }
    }

    #[test]
    fn line_breaks_are_refused() {
        for units in [&[0x61, 0x0D][..], &[0x0A, 0x61], &[0x61, 0x0D, 0x0A, 0x62]] {
            assert_eq!(
                decode_blob(&utf16le(units)),
                Err(BlobError::LineBreak),
                "{units:x?}"
            );
        }
        assert_eq!(
            decode_blob(&utf16le(&[0x61, 0x00])),
            Err(BlobError::ControlCharacter)
        );
    }

    #[test]
    fn errors_never_carry_the_secret() {
        let marker = "marker-secret";
        let mut units: Vec<u16> = marker.encode_utf16().collect();
        units.push(0x0A);
        let error = decode_blob(&utf16le(&units)).unwrap_err();
        assert!(!error.to_string().contains(marker));
        assert!(!format!("{error:?}").contains(marker));
    }
}
