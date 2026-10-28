//! Sends an HTTP action's request (AD-36, D-35, D-36) with `ureq` over
//! rustls and the platform verifier, so the Windows store, an enterprise CA
//! included, decides trust and no root set is bundled. The crypto provider
//! is ring, given to ureq here: without one ureq would panic at the first
//! request. No redirect is
//! followed, no proxy is used (the environment's proxy variables are
//! ignored), the whole exchange is bounded by the plan's timeout, the body
//! is read up to the plan's cap and the connection is then dropped; `HEAD`
//! reads no body. The request headers are never logged.

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::Duration;

use officina_core::relay::StreamHash;
use officina_core::relay::http::{HttpMethod, HttpPlan};
use ureq::tls::{RootCerts, TlsConfig};

use super::credential::Secret;

/// The `User-Agent` of every request.
pub const USER_AGENT: &str = concat!("officina/", env!("CARGO_PKG_VERSION"));

const CHUNK: usize = 8192;

/// How the request ended (D-35, D-36).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpOutcome {
    /// A response came. `body_bytes` and `body_sha256` cover the bytes passed
    /// on; `cut` when the body went past the cap and the connection was
    /// dropped; `read_error` when the body broke off for another reason.
    Response {
        status: u16,
        body_bytes: u64,
        body_sha256: String,
        cut: bool,
        read_error: Option<String>,
    },
    /// The timeout ran out, before or during the response.
    Timeout,
    /// No response: name resolution, connection or TLS failure.
    ConnectFailed { detail: String },
}

fn agent(plan: &HttpPlan) -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .max_idle_connections(0)
        .timeout_global(Some(Duration::from_secs(u64::from(plan.timeout_s))))
        .user_agent(USER_AGENT)
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .unversioned_rustls_crypto_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .build(),
        )
        .build()
        .new_agent()
}

fn is_timeout(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::Timeout(_) => true,
        ureq::Error::Io(e) => e.kind() == io::ErrorKind::TimedOut,
        _ => false,
    }
}

fn io_is_timeout(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::TimedOut
        || error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<ureq::Error>())
            .is_some_and(is_timeout)
}

/// Sends `plan`, the credential's header added when the plan carries one
/// and `secret` is given, and writes `HTTP <status>` then the body, up to
/// the cap, to `out`; a cut body ends with a marker line. The secret goes
/// into the request header and nowhere else.
pub fn send(plan: &HttpPlan, secret: Option<&Secret>, out: &mut dyn Write) -> HttpOutcome {
    let agent = agent(plan);
    let headers = secret.map_or_else(Vec::new, |s| plan.credential_headers(s.expose()));
    let response = match plan.method {
        HttpMethod::Get => {
            let mut request = agent.get(&plan.url);
            for (name, value) in &headers {
                request = request.header(name, value);
            }
            request.call()
        }
        HttpMethod::Head => {
            let mut request = agent.head(&plan.url);
            for (name, value) in &headers {
                request = request.header(name, value);
            }
            request.call()
        }
    };
    drop(headers);
    let response = match response {
        Ok(response) => response,
        Err(error) if is_timeout(&error) => return HttpOutcome::Timeout,
        Err(error) => {
            return HttpOutcome::ConnectFailed {
                detail: error.to_string(),
            };
        }
    };
    let status = response.status();
    let _ = writeln!(
        out,
        "HTTP {} {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or_default()
    );
    let mut hash = StreamHash::new();
    let mut passed: u64 = 0;
    let mut cut = false;
    let mut read_error = None;
    if plan.method == HttpMethod::Get {
        let mut reader = response.into_body().into_reader();
        let mut buf = vec![0u8; CHUNK];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let room = usize::try_from(plan.max_body_bytes - passed)
                        .unwrap_or(usize::MAX)
                        .min(n);
                    let kept = &buf[..room];
                    hash.update(kept);
                    passed += room as u64;
                    let _ = out.write_all(kept);
                    if room < n {
                        cut = true;
                        break;
                    }
                }
                Err(e) if io_is_timeout(&e) => {
                    let _ = out.flush();
                    return HttpOutcome::Timeout;
                }
                Err(e) => {
                    read_error = Some(e.to_string());
                    break;
                }
            }
        }
        // Dropping the reader closes the connection: nothing past the cap
        // is read.
        drop(reader);
        if cut {
            let _ = writeln!(
                out,
                "\n[officina: body cut at {} KiB, connection dropped]",
                plan.max_body_bytes / 1024
            );
        }
    }
    let _ = out.flush();
    HttpOutcome::Response {
        status: status.as_u16(),
        body_bytes: passed,
        body_sha256: hash.hex(),
        cut,
        read_error,
    }
}
