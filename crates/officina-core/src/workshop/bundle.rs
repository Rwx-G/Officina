//! Git bundle headers (AD-107 step 2, AD-111): the signature, the
//! capabilities of a v3 bundle, the prerequisites and the references, read
//! from the first [`MAX_HEADER_BYTES`] of a bundle and never past the line
//! that ends them. A launch checks that the base bundle holds the card's
//! base and nothing else; story 6.13 adds the package's checks.

use serde::{Deserialize, Serialize};

/// The most of a bundle a header may take: past it, the header is refused.
pub const MAX_HEADER_BYTES: usize = 64 * 1024;

/// The largest base bundle a launch copies into a worker (AD-107).
pub const MAX_BASE_BUNDLE_BYTES: u64 = 1024 * 1024 * 1024;

const SIGNATURES: [(&[u8], u8); 2] = [(b"# v2 git bundle\n", 2), (b"# v3 git bundle\n", 3)];

/// One capability of a v3 bundle (`@key` or `@key=value`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub key: String,
    pub value: Option<String>,
}

/// One reference a bundle holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub oid: String,
    pub name: String,
}

/// A bundle's header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleHeader {
    pub version: u8,
    pub capabilities: Vec<Capability>,
    /// The commits the receiver must already hold.
    pub prerequisites: Vec<String>,
    pub references: Vec<Reference>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a header was refused.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum BundleError {
        /// Neither the v2 nor the v3 signature.
        Signature,
        /// No empty line within [`MAX_HEADER_BYTES`].
        Unterminated,
        /// A line that is no capability, prerequisite or reference, an
        /// object id of the wrong form, or an object format other than
        /// SHA-1 and SHA-256.
        Malformed,
    }
}

fn is_oid(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The header of the bundle whose first bytes are `bytes` (git's
/// `gitformat-bundle`): the signature, then for v3 its capabilities, then
/// prerequisites (`-<oid> [comment]`) and references (`<oid> <name>`), up
/// to the empty line.
///
/// ```
/// use officina_core::workshop::bundle::parse_header;
/// let oid = "c93e110a6a285ebb37e2e83f242cc4215695d93f";
/// let bytes = format!("# v2 git bundle\n{oid} refs/heads/main\n\nPACK");
/// let header = parse_header(bytes.as_bytes()).unwrap();
/// assert_eq!(header.references[0].name, "refs/heads/main");
/// ```
pub fn parse_header(bytes: &[u8]) -> Result<BundleHeader, BundleError> {
    let bytes = &bytes[..bytes.len().min(MAX_HEADER_BYTES)];
    let (signature, version) = SIGNATURES
        .iter()
        .find(|(signature, _)| bytes.starts_with(signature))
        .ok_or(BundleError::Signature)?;
    let rest = &bytes[signature.len()..];
    // The empty line ends the header: first in `rest`, or after a line.
    let end = if rest.first() == Some(&b'\n') {
        0
    } else {
        rest.windows(2)
            .position(|pair| pair == b"\n\n")
            .map(|at| at + 1)
            .ok_or(BundleError::Unterminated)?
    };
    let text = std::str::from_utf8(&rest[..end]).map_err(|_| BundleError::Malformed)?;
    let mut header = BundleHeader {
        version: *version,
        capabilities: Vec::new(),
        prerequisites: Vec::new(),
        references: Vec::new(),
    };
    let mut oid_len = 40;
    for line in text.lines() {
        if let Some(capability) = line.strip_prefix('@') {
            if header.version != 3
                || !header.prerequisites.is_empty()
                || !header.references.is_empty()
            {
                return Err(BundleError::Malformed);
            }
            let (key, value) = match capability.split_once('=') {
                Some((key, value)) => (key, Some(value.to_owned())),
                None => (capability, None),
            };
            if key == "object-format" {
                oid_len = match value.as_deref() {
                    Some("sha1") => 40,
                    Some("sha256") => 64,
                    _ => return Err(BundleError::Malformed),
                };
            }
            header.capabilities.push(Capability {
                key: key.to_owned(),
                value,
            });
        } else if let Some(prerequisite) = line.strip_prefix('-') {
            let oid = prerequisite.split(' ').next().unwrap_or_default();
            if !is_oid(oid, oid_len) || !header.references.is_empty() {
                return Err(BundleError::Malformed);
            }
            header.prerequisites.push(oid.to_owned());
        } else {
            let (oid, name) = line.split_once(' ').ok_or(BundleError::Malformed)?;
            if !is_oid(oid, oid_len) || name.is_empty() || name.contains(' ') {
                return Err(BundleError::Malformed);
            }
            header.references.push(Reference {
                oid: oid.to_owned(),
                name: name.to_owned(),
            });
        }
    }
    Ok(header)
}

/// True when `header` holds `refs/heads/<branch>` at `commit` and nothing
/// else (AD-107 step 2): no prerequisite, one reference.
pub fn holds_only(header: &BundleHeader, branch: &str, commit: &str) -> bool {
    header.prerequisites.is_empty()
        && matches!(
            header.references.as_slice(),
            [only] if only.name == format!("refs/heads/{branch}") && only.oid == commit
        )
}

/// The `objects/info/alternates` line of the scratch repository (AD-111,
/// review F44): the project's object directory as one line, its separators
/// forward, every other character kept exactly. Git reads the line
/// literally unless it starts with a double quote, which no Windows path
/// does.
pub fn alternates_line(objects_dir: &str) -> String {
    format!("{}\n", objects_dir.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review F44: a path with spaces and accents is written exactly.
    #[test]
    fn an_alternates_path_with_spaces_and_accents_is_written_exactly() {
        assert_eq!(
            alternates_line("C:\\Users\\user01\\Mes Projets\\Réseau été\\.git\\objects"),
            "C:/Users/user01/Mes Projets/Réseau été/.git/objects\n"
        );
    }

    const OID: &str = "c93e110a6a285ebb37e2e83f242cc4215695d93f";

    #[test]
    fn a_header_past_its_bound_is_unterminated() {
        let mut bytes = format!("# v2 git bundle\n{OID} refs/heads/main\n").into_bytes();
        bytes.extend(std::iter::repeat_n(b'x', MAX_HEADER_BYTES));
        bytes.extend(b"\n\n");
        assert_eq!(parse_header(&bytes), Err(BundleError::Unterminated));
    }

    #[test]
    fn only_the_cards_base_is_held() {
        let header =
            parse_header(format!("# v2 git bundle\n{OID} refs/heads/main\n\n").as_bytes()).unwrap();
        assert!(holds_only(&header, "main", OID));
        assert!(!holds_only(&header, "develop", OID));
        assert!(!holds_only(&header, "main", &"0".repeat(40)));
    }
}
