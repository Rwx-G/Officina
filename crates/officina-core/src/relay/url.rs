//! URL templates of HTTP actions (AD-36, D-40). The scheme is `https`, or
//! `http` for an action without a credential (a catalogue rule); scheme,
//! host and port are literal; no userinfo. A placeholder fills exactly one
//! whole path segment or one whole query value, never a query key; its
//! value passes the AD-30 rules, then every byte outside `A-Za-z0-9-._~` is
//! percent-encoded, so a value can add neither a segment nor a parameter.
//! Held by the vector set `tests/vectors/url.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::catalogue::is_valid_param_name;
use super::params::{ValueRefusal, ValueTarget, check_value_for};

closed_enum! {
    /// Why a URL template is refused, in the order the checks run: the first
    /// that applies is reported. Every variant has a case in the vector set.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum UrlIssue {
        /// No `scheme://`.
        Malformed,
        /// A scheme other than `https` or `http`.
        Scheme,
        /// A `#`: a fragment is never sent.
        Fragment,
        /// An unbalanced brace, or a placeholder whose name is not a
        /// parameter name.
        Braces,
        /// `user@` or `user:password@` before the host (D-40).
        Userinfo,
        /// A placeholder in the scheme, host or port: they are literal.
        PlaceholderInAuthority,
        /// An empty or malformed host.
        Host,
        /// A port that is not a number from 1 to 65535.
        Port,
        /// A literal character a URL cannot carry as written.
        InvalidCharacter,
        /// A placeholder that does not fill a whole path segment.
        PartialSegment,
        /// A placeholder in a query key.
        PlaceholderInQueryKey,
        /// A placeholder that does not fill a whole query value.
        PartialQueryValue,
    }
}

impl fmt::Display for UrlIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "the URL must start with `https://` or `http://`",
            Self::Scheme => "the URL scheme must be `https` or `http`",
            Self::Fragment => "the URL cannot hold a `#` fragment",
            Self::Braces => "a placeholder is `{name}` with a parameter name, braces balanced",
            Self::Userinfo => "the URL cannot carry a user or a password before the host",
            Self::PlaceholderInAuthority => "the host and port are literal: no placeholder there",
            Self::Host => "the URL's host is empty or malformed",
            Self::Port => "the URL's port must be a number from 1 to 65535",
            Self::InvalidCharacter => {
                "the URL holds a character it cannot carry as written; percent-encode it"
            }
            Self::PartialSegment => "a placeholder fills a whole path segment, nothing else",
            Self::PlaceholderInQueryKey => "a query key is literal: no placeholder there",
            Self::PartialQueryValue => "a placeholder fills a whole query value, nothing else",
        })
    }
}

closed_enum! {
    /// The byte classes of the encoder: unreserved bytes pass, every other
    /// byte is percent-encoded.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ByteClass {
        /// `A-Za-z0-9-._~`: kept as is.
        Unreserved,
        /// Any other ASCII byte, `/`, `?`, `&`, `=`, `#`, `%` and space
        /// among them.
        Reserved,
        /// A byte of a multi-byte UTF-8 character.
        NonAscii,
    }
}

/// The class of one byte of a value.
pub fn byte_class(byte: u8) -> ByteClass {
    if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
        ByteClass::Unreserved
    } else if byte.is_ascii() {
        ByteClass::Reserved
    } else {
        ByteClass::NonAscii
    }
}

/// `value` with every byte outside `A-Za-z0-9-._~` as `%XX`.
///
/// ```
/// use officina_core::relay::url::encode;
/// assert_eq!(encode("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
/// assert_eq!(encode("é"), "%C3%A9");
/// ```
pub fn encode(value: &str) -> String {
    value
        .bytes()
        .fold(String::with_capacity(value.len()), |mut out, byte| {
            if byte_class(byte) == ByteClass::Unreserved {
                out.push(char::from(byte));
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
            out
        })
}

/// The URL schemes an HTTP action may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlScheme {
    Https,
    Http,
}

/// A path segment or query value: literal, or one placeholder filling it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Literal(String),
    Placeholder(String),
}

/// One `key` or `key=value` of the query.
#[derive(Debug, Clone, PartialEq, Eq)]
struct QueryItem {
    key: String,
    value: Option<Part>,
}

/// A parsed URL template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlTemplate {
    source: String,
    scheme: UrlScheme,
    /// `host[:port]` as written.
    authority: String,
    /// Lowercase, without the port; an IPv6 literal keeps its brackets.
    host: String,
    /// Absent when nothing follows the authority but a query or nothing.
    path: Option<Vec<Part>>,
    query: Option<Vec<QueryItem>>,
}

/// Why parameter values do not fill a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UrlRefusal {
    #[error("parameter {param} is missing")]
    MissingParameter { param: String },
    #[error("parameter {param}: {refusal}")]
    Value {
        param: String,
        refusal: ValueRefusal,
    },
}

/// A literal character allowed as written: unreserved, the sub-delimiters,
/// `:`, `@`, `/` and `?`; `%` only as the start of a `%XX` escape, which
/// [`check_literal`] reads.
fn is_literal_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-._~!$&'()*+,;=:@/?".contains(c)
}

fn check_literal(text: &str) -> Result<(), UrlIssue> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = char::from(bytes[i]);
        if c == '%' {
            let escape = bytes.get(i + 1..i + 3);
            if !escape.is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit)) {
                return Err(UrlIssue::InvalidCharacter);
            }
            i += 3;
            continue;
        }
        if !bytes[i].is_ascii() || !is_literal_char(c) {
            return Err(UrlIssue::InvalidCharacter);
        }
        i += 1;
    }
    Ok(())
}

/// The placeholder a whole part is, if it is one; `Err` when the part holds
/// a brace without being exactly one placeholder.
fn whole_placeholder(part: &str) -> Result<Option<&str>, ()> {
    if !part.contains(['{', '}']) {
        return Ok(None);
    }
    part.strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .filter(|name| !name.contains(['{', '}']))
        .map(Some)
        .ok_or(())
}

/// Every brace opens a placeholder that closes before the next one opens,
/// and names a parameter.
fn check_braces(text: &str) -> Result<(), UrlIssue> {
    let mut rest = text;
    while let Some(open) = rest.find(['{', '}']) {
        if rest[open..].starts_with('}') {
            return Err(UrlIssue::Braces);
        }
        let inner = &rest[open + 1..];
        let close = inner.find(['{', '}']).ok_or(UrlIssue::Braces)?;
        if inner[close..].starts_with('{') || !is_valid_param_name(&inner[..close]) {
            return Err(UrlIssue::Braces);
        }
        rest = &inner[close + 1..];
    }
    Ok(())
}

fn is_host_label(label: &str) -> bool {
    !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Splits `host[:port]` and checks both.
fn parse_authority(authority: &str) -> Result<String, UrlIssue> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']').ok_or(UrlIssue::Host)?;
        let inner = &rest[..end];
        if inner.is_empty()
            || !inner
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        {
            return Err(UrlIssue::Host);
        }
        let after = &rest[end + 1..];
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port),
            None if after.is_empty() => None,
            None => return Err(UrlIssue::Host),
        };
        (format!("[{}]", inner.to_ascii_lowercase()), port)
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        if !host.split('.').all(is_host_label) {
            return Err(UrlIssue::Host);
        }
        (host.to_ascii_lowercase(), port)
    };
    if let Some(port) = port {
        let valid = !port.is_empty()
            && port.chars().all(|c| c.is_ascii_digit())
            && port.parse::<u32>().is_ok_and(|p| (1..=65_535).contains(&p));
        if !valid {
            return Err(UrlIssue::Port);
        }
    }
    Ok(host)
}

/// A credential's `hosts` entry (D-37): a host as a URL template takes it,
/// without a port, lowercased; `None` when malformed, a wildcard included.
///
/// ```
/// use officina_core::relay::url::host_entry;
/// assert_eq!(host_entry("Monitoring.Example.com").as_deref(), Some("monitoring.example.com"));
/// assert_eq!(host_entry("*.example.com"), None);
/// assert_eq!(host_entry("example.com:443"), None);
/// ```
pub fn host_entry(text: &str) -> Option<String> {
    let has_port = if text.starts_with('[') {
        !text.ends_with(']')
    } else {
        text.contains(':')
    };
    if has_port {
        return None;
    }
    parse_authority(text).ok()
}

impl UrlTemplate {
    /// Parses a template; the first rule it breaks is the issue.
    ///
    /// ```
    /// use officina_core::relay::url::{UrlIssue, UrlTemplate};
    /// let t = UrlTemplate::parse("https://Monitoring.example.com/api/{check}?host={host}").unwrap();
    /// assert_eq!(t.host(), "monitoring.example.com");
    /// assert_eq!(UrlTemplate::parse("https://user01@example.com/").unwrap_err(), UrlIssue::Userinfo);
    /// assert_eq!(UrlTemplate::parse("https://example.com/v{n}").unwrap_err(), UrlIssue::PartialSegment);
    /// ```
    pub fn parse(source: &str) -> Result<UrlTemplate, UrlIssue> {
        let (scheme, rest) = source.split_once("://").ok_or(UrlIssue::Malformed)?;
        let scheme = match scheme {
            "https" => UrlScheme::Https,
            "http" => UrlScheme::Http,
            _ => return Err(UrlIssue::Scheme),
        };
        if rest.contains('#') {
            return Err(UrlIssue::Fragment);
        }
        check_braces(rest)?;
        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if authority.contains('@') {
            return Err(UrlIssue::Userinfo);
        }
        if authority.contains(['{', '}']) {
            return Err(UrlIssue::PlaceholderInAuthority);
        }
        let host = parse_authority(authority)?;
        let rest = &rest[authority_end..];
        let (path_text, query_text) = match rest.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (rest, None),
        };

        let mut path = None;
        if let Some(segments) = path_text.strip_prefix('/') {
            let mut parts = Vec::new();
            for segment in segments.split('/') {
                parts.push(match whole_placeholder(segment) {
                    Ok(Some(name)) => Part::Placeholder(name.to_owned()),
                    Ok(None) => {
                        check_literal(segment)?;
                        Part::Literal(segment.to_owned())
                    }
                    Err(()) => return Err(UrlIssue::PartialSegment),
                });
            }
            path = Some(parts);
        }

        let mut query = None;
        if let Some(query_text) = query_text {
            let mut items = Vec::new();
            for item in query_text.split('&') {
                let (key, value) = match item.split_once('=') {
                    Some((key, value)) => (key, Some(value)),
                    None => (item, None),
                };
                if key.contains(['{', '}']) {
                    return Err(UrlIssue::PlaceholderInQueryKey);
                }
                check_literal(key)?;
                let value = match value.map(whole_placeholder) {
                    None => None,
                    Some(Ok(Some(name))) => Some(Part::Placeholder(name.to_owned())),
                    Some(Ok(None)) => {
                        let literal = value.unwrap_or_default();
                        check_literal(literal)?;
                        Some(Part::Literal(literal.to_owned()))
                    }
                    Some(Err(())) => return Err(UrlIssue::PartialQueryValue),
                };
                items.push(QueryItem {
                    key: key.to_owned(),
                    value,
                });
            }
            query = Some(items);
        }
        Ok(UrlTemplate {
            source: source.to_owned(),
            scheme,
            authority: authority.to_owned(),
            host,
            path,
            query,
        })
    }

    /// The template as the floor wrote it.
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn scheme(&self) -> UrlScheme {
        self.scheme
    }

    /// The host, lowercase, without the port: what a credential's `hosts`
    /// are compared with (D-37).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The parameters the template's placeholders name.
    pub fn placeholders(&self) -> BTreeSet<&str> {
        let path = self.path.iter().flatten();
        let query = self.query.iter().flatten().filter_map(|i| i.value.as_ref());
        path.chain(query)
            .filter_map(|part| match part {
                Part::Placeholder(name) => Some(name.as_str()),
                Part::Literal(_) => None,
            })
            .collect()
    }

    /// The URL with every placeholder replaced by its value, checked by the
    /// AD-30 rules then percent-encoded. Parameters the template does not
    /// name are ignored; the caller refuses them.
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use officina_core::relay::url::UrlTemplate;
    /// let t = UrlTemplate::parse("https://example.com/api/{id}?q={q}").unwrap();
    /// let values = BTreeMap::from([("id".to_owned(), "a/b".to_owned()), ("q".to_owned(), "x&y=1".to_owned())]);
    /// assert_eq!(t.fill(&values).unwrap(), "https://example.com/api/a%2Fb?q=x%26y%3D1");
    /// ```
    pub fn fill(&self, values: &BTreeMap<String, String>) -> Result<String, UrlRefusal> {
        let filled = |part: &Part| -> Result<String, UrlRefusal> {
            match part {
                Part::Literal(text) => Ok(text.clone()),
                Part::Placeholder(param) => {
                    let value = values
                        .get(param)
                        .ok_or_else(|| UrlRefusal::MissingParameter {
                            param: param.clone(),
                        })?;
                    check_value_for(ValueTarget::Http, value).map_err(|refusal| {
                        UrlRefusal::Value {
                            param: param.clone(),
                            refusal,
                        }
                    })?;
                    Ok(encode(value))
                }
            }
        };
        let scheme = match self.scheme {
            UrlScheme::Https => "https",
            UrlScheme::Http => "http",
        };
        let mut url = format!("{scheme}://{}", self.authority);
        if let Some(path) = &self.path {
            for part in path {
                url.push('/');
                url.push_str(&filled(part)?);
            }
        }
        if let Some(query) = &self.query {
            url.push('?');
            for (index, item) in query.iter().enumerate() {
                if index > 0 {
                    url.push('&');
                }
                url.push_str(&item.key);
                if let Some(value) = &item.value {
                    url.push('=');
                    url.push_str(&filled(value)?);
                }
            }
        }
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_without_placeholders_fills_to_itself() {
        for source in [
            "https://example.com",
            "https://example.com/",
            "https://example.com:8443/api//v1/?a&b=&c=d",
            "http://[::1]:8080/x",
            "https://example.com?x=1",
            "https://example.com/a%20b",
        ] {
            let template = UrlTemplate::parse(source).unwrap();
            assert_eq!(template.fill(&BTreeMap::new()).unwrap(), source, "{source}");
        }
    }

    #[test]
    fn every_byte_is_classed() {
        for byte in 0..=u8::MAX {
            let class = byte_class(byte);
            let encoded = encode(&String::from_utf8_lossy(&[byte]));
            if class == ByteClass::Unreserved {
                assert_eq!(encoded.as_bytes(), [byte]);
            } else {
                assert!(encoded.starts_with('%'), "{byte}");
            }
        }
    }
}
