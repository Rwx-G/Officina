//! HTTP actions as pure functions (AD-36, D-35 to D-37): the request a
//! validated action and its parameter values make, and the one header a
//! credential adds, only for a host the credential is bound to. The host
//! reads the secret and sends the request.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::argv::ArgvRefusal;
use super::catalogue::{Action, Credential};
use super::params::{self, ValueTarget};
use super::url::UrlRefusal;

/// The longest an HTTP request waits, whatever the action's `timeout_s`
/// (AD-36).
pub const MAX_HTTP_TIMEOUT_S: u32 = 60;

closed_enum! {
    #[serde(rename_all = "UPPERCASE")]
    /// The methods an HTTP action may send: reads only (AD-36).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum HttpMethod {
        Get,
        Head,
    }
}

impl HttpMethod {
    /// The method as sent on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Head => "HEAD",
        }
    }
}

/// The credential a request carries: which one, and the header it fills.
/// The secret itself is never part of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialBinding {
    pub name: String,
    pub header: String,
    pub scheme: String,
}

/// What passed every check of an HTTP action: the request to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPlan {
    pub method: HttpMethod,
    /// The template filled with the percent-encoded values.
    pub url: String,
    /// The URL's host, lowercase, without the port.
    pub host: String,
    /// Present only when the action names a credential bound to `host`.
    pub credential: Option<CredentialBinding>,
    /// `min(timeout_s, MAX_HTTP_TIMEOUT_S)`.
    pub timeout_s: u32,
    /// `max_output_kib` in bytes: the body is read up to it, then dropped.
    pub max_body_bytes: u64,
}

impl HttpPlan {
    /// The headers the credential adds, given its secret: one
    /// `<header>: <scheme> <secret>`, or none when the plan carries no
    /// credential. Built at the last moment, so the secret lives only in
    /// the request.
    pub fn credential_headers(&self, secret: &str) -> Vec<(String, String)> {
        self.credential
            .iter()
            .map(|binding| {
                (
                    binding.header.clone(),
                    format!("{} {secret}", binding.scheme),
                )
            })
            .collect()
    }
}

/// True when `host` is one of `hosts` (D-37): exact, case-insensitive, port
/// ignored (the URL's host never carries one), no wildcard.
///
/// ```
/// use officina_core::relay::http::host_is_bound;
/// let hosts = ["monitoring.example.com".to_owned()];
/// assert!(host_is_bound(&hosts, "Monitoring.Example.COM"));
/// assert!(!host_is_bound(&hosts, "api.monitoring.example.com"));
/// ```
pub fn host_is_bound(hosts: &[String], host: &str) -> bool {
    hosts.iter().any(|bound| bound.eq_ignore_ascii_case(host))
}

/// Checks `params` against an HTTP action (every declared one present, none
/// extra, each accepted) and builds its request; the credential is bound
/// only when the URL's host is in its `hosts` (D-37). `None` for a process
/// action.
pub fn build(
    action: &Action,
    params: &BTreeMap<String, String>,
    credentials: &BTreeMap<String, Credential>,
) -> Option<Result<HttpPlan, ArgvRefusal>> {
    let spec = action.http.as_ref()?;
    Some((|| {
        if let Some(param) = action.params.keys().find(|p| !params.contains_key(*p)) {
            return Err(ArgvRefusal::MissingParameter {
                param: param.clone(),
            });
        }
        if let Some(param) = params.keys().find(|p| !action.params.contains_key(*p)) {
            return Err(ArgvRefusal::UnknownParameter {
                param: param.clone(),
            });
        }
        for (name, param) in &action.params {
            let value = params.get(name).map_or("", String::as_str);
            params::accept_for(param, ValueTarget::Http, value).map_err(|refusal| {
                ArgvRefusal::Value {
                    param: name.clone(),
                    refusal,
                }
            })?;
        }
        let url = spec.url.fill(params).map_err(|refusal| match refusal {
            UrlRefusal::MissingParameter { param } => ArgvRefusal::MissingParameter { param },
            UrlRefusal::Value { param, refusal } => ArgvRefusal::Value { param, refusal },
        })?;
        let host = spec.url.host().to_owned();
        let credential = spec
            .credential
            .as_ref()
            .and_then(|name| credentials.get(name))
            .filter(|credential| host_is_bound(&credential.hosts, &host))
            .map(|credential| CredentialBinding {
                name: credential.name.clone(),
                header: credential.header.clone(),
                scheme: credential.scheme.clone(),
            });
        Ok(HttpPlan {
            method: spec.method,
            url,
            host,
            credential,
            timeout_s: action.timeout_s.min(MAX_HTTP_TIMEOUT_S),
            max_body_bytes: u64::from(action.max_output_kib) * 1024,
        })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::catalogue::action_from_value;
    use crate::relay::params::ValueRefusal;

    fn action(url: &str, credential: Option<&str>) -> Action {
        let credential = credential.map_or(String::new(), |c| format!("credential = \"{c}\"\n"));
        let text = format!(
            "name = \"health\"\nkind = \"http\"\nclass = \"free\"\nmethod = \"GET\"\nurl = \"{url}\"\n{credential}timeout_s = 300\nmax_output_kib = 2\n[params.check]\nvalues = [\"health\", \"a b\"]\n"
        );
        action_from_value(&toml::from_str(&text).unwrap()).unwrap()
    }

    fn credential(hosts: &[&str]) -> BTreeMap<String, Credential> {
        BTreeMap::from([(
            "monitoring".to_owned(),
            Credential {
                name: "monitoring".to_owned(),
                hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
                header: "Authorization".to_owned(),
                scheme: "Bearer".to_owned(),
            },
        )])
    }

    fn params(check: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("check".to_owned(), check.to_owned())])
    }

    #[test]
    fn plan_fills_the_url_and_bounds_the_wait_and_body() {
        let plan = build(
            &action("https://monitoring.example.com/{check}", None),
            &params("a b"),
            &BTreeMap::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.url, "https://monitoring.example.com/a%20b");
        assert_eq!(plan.method, HttpMethod::Get);
        assert_eq!(plan.timeout_s, MAX_HTTP_TIMEOUT_S);
        assert_eq!(plan.max_body_bytes, 2048);
        assert!(plan.credential_headers("s").is_empty());
        assert_eq!(
            build(
                &action("https://monitoring.example.com/{check}", None),
                &params("nope"),
                &BTreeMap::new()
            ),
            Some(Err(ArgvRefusal::Value {
                param: "check".to_owned(),
                refusal: ValueRefusal::NotInValues
            }))
        );
    }

    #[test]
    fn header_only_for_a_bound_host() {
        let bound = build(
            &action("https://monitoring.example.com/{check}", Some("monitoring")),
            &params("health"),
            &credential(&["monitoring.example.com"]),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            bound.credential_headers("s3cret"),
            [("Authorization".to_owned(), "Bearer s3cret".to_owned())]
        );

        let elsewhere = build(
            &action("https://other.example.com/{check}", Some("monitoring")),
            &params("health"),
            &credential(&["monitoring.example.com"]),
        )
        .unwrap()
        .unwrap();
        assert_eq!(elsewhere.credential, None);
        assert!(elsewhere.credential_headers("s3cret").is_empty());

        let undeclared = build(
            &action("https://monitoring.example.com/{check}", Some("monitoring")),
            &params("health"),
            &BTreeMap::new(),
        )
        .unwrap()
        .unwrap();
        assert!(undeclared.credential_headers("s3cret").is_empty());
    }

    #[test]
    fn host_match_is_exact_and_case_insensitive() {
        let hosts = ["monitoring.example.com".to_owned(), "[::1]".to_owned()];
        for bound in ["monitoring.example.com", "MONITORING.example.COM", "[::1]"] {
            assert!(host_is_bound(&hosts, bound), "{bound}");
        }
        for not_bound in [
            "example.com",
            "api.monitoring.example.com",
            "monitoring.example.com.",
            "monitoring.example.co",
            "xmonitoring.example.com",
            "",
        ] {
            assert!(!host_is_bound(&hosts, not_bound), "{not_bound}");
        }
        let with_port = build(
            &action(
                "https://Monitoring.example.com:8443/{check}",
                Some("monitoring"),
            ),
            &params("health"),
            &credential(&["monitoring.example.com"]),
        )
        .unwrap()
        .unwrap();
        assert!(
            with_port.credential.is_some(),
            "the port is ignored, the host's case too"
        );
    }

    #[test]
    fn a_process_action_has_no_http_plan() {
        let process = action_from_value(
            &toml::from_str("name = \"probe\"\nclass = \"free\"\nprogram = \"C:/x/tool.exe\"\n")
                .unwrap(),
        )
        .unwrap();
        assert!(build(&process, &BTreeMap::new(), &BTreeMap::new()).is_none());
    }
}
