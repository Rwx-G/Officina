//! Compile-time spelling of a variant name under serde's `rename_all` rules,
//! for the `code()` that `closed_enum!` generates. Each function follows the
//! rule serde applies to an ASCII variant name; a case serde knows but this
//! module does not spell fails the build, never a message.

/// The serde cases this module spells, with the separator each puts before
/// an inner capital.
const fn separator(case: &str) -> Option<u8> {
    if same(case, "snake_case") {
        Some(b'_')
    } else if same(case, "kebab-case") {
        Some(b'-')
    } else if same(case, "lowercase") || same(case, "UPPERCASE") {
        None
    } else {
        panic!("closed_enum!: a rename_all case wire.rs does not spell")
    }
}

const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// How many bytes `variant` takes once spelled in `case`.
pub(crate) const fn len(variant: &str, case: &str) -> usize {
    let bytes = variant.as_bytes();
    let mut n = bytes.len();
    if separator(case).is_some() {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i].is_ascii_uppercase() {
                n += 1;
            }
            i += 1;
        }
    }
    n
}

/// `variant` spelled in `case`, `N` being [`len`] of the same arguments.
pub(crate) const fn spell<const N: usize>(variant: &str, case: &str) -> [u8; N] {
    let bytes = variant.as_bytes();
    let separator = separator(case);
    let upper = same(case, "UPPERCASE");
    let mut out = [0u8; N];
    let (mut i, mut o) = (0, 0);
    while i < bytes.len() {
        let byte = bytes[i];
        if let Some(separator) = separator
            && i > 0
            && byte.is_ascii_uppercase()
        {
            out[o] = separator;
            o += 1;
        }
        out[o] = if upper {
            byte.to_ascii_uppercase()
        } else {
            byte.to_ascii_lowercase()
        };
        o += 1;
        i += 1;
    }
    out
}

/// The spelled bytes as text; a variant name is ASCII, so this cannot fail
/// once the build passes.
pub(crate) const fn text(bytes: &'static [u8]) -> &'static str {
    match core::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => panic!("closed_enum!: a variant name that is not ASCII"),
    }
}
