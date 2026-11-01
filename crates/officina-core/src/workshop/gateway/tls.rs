//! The ClientHello (AD-103 step 4, D-234): the server name a tunnel's
//! first TLS bytes name, read by hand from the record layer, so the
//! gateway needs no TLS crate.

use super::GatewayRefusal;

/// The most of a tunnel's first bytes read for the ClientHello.
pub const MAX_HELLO_BYTES: usize = 16 * 1024;

const RECORD_HANDSHAKE: u8 = 0x16;
const HANDSHAKE_CLIENT_HELLO: u8 = 0x01;
const EXT_SERVER_NAME: u16 = 0x0000;
const EXT_ENCRYPTED_CLIENT_HELLO: u16 = 0xfe0d;
const NAME_TYPE_HOST: u8 = 0x00;

/// What a ClientHello says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// The server name, as sent (the gateway compares it lowercased).
    pub server_name: String,
    /// An `encrypted_client_hello` extension is present (recorded, never
    /// refused, AD-103).
    pub ech: bool,
}

/// How far the bytes read so far go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelloRead {
    /// A complete ClientHello.
    Done(Hello),
    /// More bytes are needed, within [`MAX_HELLO_BYTES`].
    More,
    /// Not a ClientHello (`not_tls`), or one without a server name
    /// (`sni_missing`).
    Refused(GatewayRefusal),
}

/// A cursor over a byte slice; every read is bounds-checked.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn u24(&mut self) -> Option<usize> {
        self.take(3)
            .map(|b| (usize::from(b[0]) << 16) | (usize::from(b[1]) << 8) | usize::from(b[2]))
    }

    fn vec8(&mut self) -> Option<&'a [u8]> {
        let n = self.u8()?;
        self.take(usize::from(n))
    }

    fn vec16(&mut self) -> Option<&'a [u8]> {
        let n = self.u16()?;
        self.take(usize::from(n))
    }

    fn done(&self) -> bool {
        self.at == self.bytes.len()
    }
}

/// Reads the ClientHello at the start of `buf`, the tunnel's first bytes:
/// handshake records, possibly fragmented, joined until the handshake
/// message is whole. A record of another type, or a handshake that is not
/// a ClientHello, is `not_tls`; past [`MAX_HELLO_BYTES`] it is `not_tls`
/// too.
pub fn server_name(buf: &[u8]) -> HelloRead {
    let mut records = Reader::new(buf);
    let mut handshake: Vec<u8> = Vec::new();
    loop {
        if let Some(read) = whole_hello(&handshake) {
            return read;
        }
        let Some(header) = records.take(5) else {
            return more_or_not_tls(buf);
        };
        if header[0] != RECORD_HANDSHAKE || header[1] != 0x03 {
            return HelloRead::Refused(GatewayRefusal::NotTls);
        }
        let len = usize::from(u16::from_be_bytes([header[3], header[4]]));
        if len == 0 {
            return HelloRead::Refused(GatewayRefusal::NotTls);
        }
        let Some(fragment) = records.take(len) else {
            return more_or_not_tls(buf);
        };
        handshake.extend_from_slice(fragment);
    }
}

fn more_or_not_tls(buf: &[u8]) -> HelloRead {
    if buf.len() >= MAX_HELLO_BYTES {
        HelloRead::Refused(GatewayRefusal::NotTls)
    } else {
        HelloRead::More
    }
}

/// The ClientHello in `handshake` once its message is whole.
fn whole_hello(handshake: &[u8]) -> Option<HelloRead> {
    let mut r = Reader::new(handshake);
    let kind = r.u8()?;
    if kind != HANDSHAKE_CLIENT_HELLO {
        return Some(HelloRead::Refused(GatewayRefusal::NotTls));
    }
    let len = r.u24()?;
    let body = r.take(len)?;
    Some(match parse_body(body) {
        Some(read) => read,
        None => HelloRead::Refused(GatewayRefusal::NotTls),
    })
}

/// The body of a whole ClientHello; `None` when a length does not fit.
fn parse_body(body: &[u8]) -> Option<HelloRead> {
    let mut r = Reader::new(body);
    r.take(2)?; // legacy version
    r.take(32)?; // random
    r.vec8()?; // session id
    r.vec16()?; // cipher suites
    r.vec8()?; // compression methods
    if r.done() {
        return Some(HelloRead::Refused(GatewayRefusal::SniMissing));
    }
    let mut extensions = Reader::new(r.vec16()?);
    let mut names: Vec<String> = Vec::new();
    let mut ech = false;
    while !extensions.done() {
        let kind = extensions.u16()?;
        let data = extensions.vec16()?;
        match kind {
            EXT_SERVER_NAME => names.extend(host_names(data)?),
            EXT_ENCRYPTED_CLIENT_HELLO => ech = true,
            _ => {}
        }
    }
    Some(match names.as_slice() {
        [] => HelloRead::Refused(GatewayRefusal::SniMissing),
        [name] => HelloRead::Done(Hello {
            server_name: name.clone(),
            ech,
        }),
        // RFC 6066 allows one name of a type: two are not a ClientHello a
        // client sends.
        _ => HelloRead::Refused(GatewayRefusal::NotTls),
    })
}

fn host_names(data: &[u8]) -> Option<Vec<String>> {
    let mut outer = Reader::new(data);
    let mut list = Reader::new(outer.vec16()?);
    let mut names = Vec::new();
    while !list.done() {
        let kind = list.u8()?;
        let name = list.vec16()?;
        if kind == NAME_TYPE_HOST {
            names.push(String::from_utf8(name.to_vec()).ok()?);
        }
    }
    Some(names)
}

/// A ClientHello record naming `name`, for tests and the battery: TLS 1.2
/// framing, one cipher suite, a server name and, with `ech`, an empty
/// `encrypted_client_hello` extension.
pub fn client_hello(name: &str, ech: bool) -> Vec<u8> {
    client_hello_with(&[name], ech)
}

/// A ClientHello record with a `server_name` extension listing `names`
/// (none: no extension at all), for the refusal cases.
pub fn client_hello_with(names: &[&str], ech: bool) -> Vec<u8> {
    fn u16b(n: usize) -> [u8; 2] {
        u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes()
    }
    let mut extensions = Vec::new();
    if !names.is_empty() {
        let mut sni_list = Vec::new();
        for name in names {
            sni_list.push(NAME_TYPE_HOST);
            sni_list.extend(u16b(name.len()));
            sni_list.extend(name.as_bytes());
        }
        let mut sni = u16b(sni_list.len()).to_vec();
        sni.extend(sni_list);
        extensions.extend(EXT_SERVER_NAME.to_be_bytes());
        extensions.extend(u16b(sni.len()));
        extensions.extend(sni);
    }
    if ech {
        extensions.extend(EXT_ENCRYPTED_CLIENT_HELLO.to_be_bytes());
        extensions.extend([0, 0]);
    }
    let mut body = vec![0x03, 0x03];
    body.extend([0x5a; 32]);
    body.push(0); // no session id
    body.extend([0x00, 0x02, 0x13, 0x01]); // one suite
    body.extend([0x01, 0x00]); // null compression
    body.extend(u16b(extensions.len()));
    body.extend(extensions);
    let len = u32::try_from(body.len()).unwrap_or(0).to_be_bytes();
    let mut handshake = vec![HANDSHAKE_CLIENT_HELLO, len[1], len[2], len[3]];
    handshake.extend(body);
    let mut record = vec![RECORD_HANDSHAKE, 0x03, 0x01];
    record.extend(u16b(handshake.len()));
    record.extend(handshake);
    record
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_hello_parses_to_its_name() {
        for ech in [false, true] {
            let hello = client_hello("api.anthropic.com", ech);
            assert_eq!(
                server_name(&hello),
                HelloRead::Done(Hello {
                    server_name: "api.anthropic.com".into(),
                    ech
                })
            );
            for cut in 0..hello.len() {
                assert_eq!(server_name(&hello[..cut]), HelloRead::More, "cut at {cut}");
            }
        }
    }

    #[test]
    fn a_hello_split_across_records_parses() {
        let hello = client_hello("example.com", false);
        let handshake = &hello[5..];
        let (a, b) = handshake.split_at(10);
        let mut split = Vec::new();
        for part in [a, b] {
            split.extend([RECORD_HANDSHAKE, 0x03, 0x01]);
            split.extend(u16::try_from(part.len()).unwrap().to_be_bytes());
            split.extend(part);
        }
        assert!(
            matches!(server_name(&split), HelloRead::Done(h) if h.server_name == "example.com")
        );
    }

    #[test]
    fn no_name_and_two_names_are_refused() {
        assert_eq!(
            server_name(&client_hello_with(&[], false)),
            HelloRead::Refused(GatewayRefusal::SniMissing)
        );
        assert_eq!(
            server_name(&client_hello_with(
                &["a.example.com", "b.example.com"],
                false
            )),
            HelloRead::Refused(GatewayRefusal::NotTls)
        );
    }

    #[test]
    fn plain_http_is_not_tls() {
        assert_eq!(
            server_name(b"GET / HTTP/1.1\r\n\r\n"),
            HelloRead::Refused(GatewayRefusal::NotTls)
        );
    }
}
