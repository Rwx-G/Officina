//! The upstream proxy of each allow-list entry (D-611): the floor's
//! `upstream_proxy` setting applied to what the host read of Windows' proxy
//! configuration. Pure: the host reads, core decides.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::floor::UpstreamProxy;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a run's routes came from (D-611), as doctor names it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ProxySource {
        /// An explicit `host:port` in the floor.
        Floor,
        /// A PAC file Windows names.
        Pac,
        /// Windows' automatic detection.
        AutoDetect,
        /// Windows' static proxy and its bypass list.
        Static,
        /// No proxy: `"none"`, or `"system"` with nothing configured.
        None,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the routes could not be decided (D-611); a launch refuses with
    /// it, naming the entry.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum ProxyRefusal {
        /// The answer names a SOCKS or HTTPS proxy, or does not parse.
        ProxyUnsupported,
        /// The PAC file could not be fetched or evaluated.
        ProxyUnresolved,
    }
}

/// What the PAC evaluation answered for one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacAnswer {
    /// The PAC's answer, as `FindProxyForURL` returns it.
    Answer(String),
    /// The PAC file could not be fetched or evaluated.
    Failed,
    /// Automatic detection found no PAC file.
    NoPac,
}

/// What the host read of Windows' proxy configuration for the current user
/// (`WinHttpGetIEProxyConfigForCurrentUser`'s fields), with the PAC's
/// answer per allow-list entry when a PAC applies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemProxy {
    pub auto_detect: bool,
    pub pac_url: Option<String>,
    /// `host:port`, or per scheme (`https=host:port;http=...`).
    pub static_proxy: Option<String>,
    /// `;`-separated patterns, `<local>` among them.
    pub bypass: Option<String>,
    /// The PAC's answer per entry, when one was evaluated.
    pub pac_answers: BTreeMap<String, PacAnswer>,
}

/// How one entry leaves: direct, or through `host:port`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    Direct,
    Proxy(String),
}

/// Every entry's route and where they came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Routes {
    pub source: ProxySource,
    pub routes: BTreeMap<String, Route>,
}

impl Routes {
    /// The gateway's `--route` arguments (D-611): the proxied entries only.
    pub fn gateway_routes(&self) -> BTreeMap<String, String> {
        self.routes
            .iter()
            .filter_map(|(entry, route)| match route {
                Route::Proxy(proxy) => Some((entry.clone(), proxy.clone())),
                Route::Direct => None,
            })
            .collect()
    }
}

/// A refusal and the entry it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRefusal {
    pub cause: ProxyRefusal,
    pub entry: String,
}

/// The URL the PAC is asked about for `entry` (D-611): `https://<host>/`,
/// a `*.` entry on its suffix, an approximation the README states.
pub fn pac_url(entry: &str) -> String {
    format!("https://{}/", entry.strip_prefix("*.").unwrap_or(entry))
}

/// The route a PAC answer gives (D-611): its first element; `DIRECT`, or
/// `PROXY host:port`; `SOCKS`, `HTTPS` or anything else is unsupported.
pub fn pac_route(answer: &str) -> Result<Route, ProxyRefusal> {
    let first = answer.split(';').map(str::trim).find(|e| !e.is_empty());
    let Some(first) = first else {
        return Ok(Route::Direct);
    };
    let mut words = first.split_whitespace();
    match (
        words.next().map(str::to_ascii_uppercase).as_deref(),
        words.next(),
    ) {
        (Some("DIRECT"), None) => Ok(Route::Direct),
        (Some("PROXY"), Some(target)) if is_host_port(target) => {
            Ok(Route::Proxy(target.to_owned()))
        }
        _ => Err(ProxyRefusal::ProxyUnsupported),
    }
}

/// WinHTTP's proxy list for a URL (`WinHttpGetProxyForUrl`'s
/// `lpszProxy`, entries separated by `;` or spaces, each
/// `[<scheme>=][<scheme>://]host:port`) as the PAC answer [`pac_route`]
/// reads: its first entry, a `socks` scheme as `SOCKS`, a TLS connection to
/// the proxy (`https://`) as `HTTPS`, both refused there, anything else
/// as `PROXY host:port`.
///
/// ```
/// use officina_core::workshop::proxy::winhttp_answer;
/// assert_eq!(winhttp_answer("proxy.example.com:8080;backup.example.com:80"), "PROXY proxy.example.com:8080");
/// assert_eq!(winhttp_answer("socks=s.example.com:1080"), "SOCKS s.example.com:1080");
/// ```
pub fn winhttp_answer(list: &str) -> String {
    let Some(first) = list
        .split([';', ' ', '\t'])
        .map(str::trim)
        .find(|e| !e.is_empty())
    else {
        return "DIRECT".to_owned();
    };
    let (scheme, rest) = match first.split_once('=') {
        Some((scheme, rest)) => (Some(scheme.to_ascii_lowercase()), rest),
        None => (None, first),
    };
    let (url_scheme, target) = match rest.split_once("://") {
        Some((scheme, target)) => (Some(scheme.to_ascii_lowercase()), target),
        None => (None, rest),
    };
    let target = target.trim_end_matches('/');
    let socks = [&scheme, &url_scheme]
        .into_iter()
        .flatten()
        .any(|s| s.starts_with("socks"));
    if socks {
        format!("SOCKS {target}")
    } else if url_scheme.as_deref() == Some("https") {
        format!("HTTPS {target}")
    } else {
        format!("PROXY {target}")
    }
}

fn is_host_port(text: &str) -> bool {
    text.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty()
            && port.parse::<u16>().is_ok_and(|p| p > 0)
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    })
}

/// The static proxy for HTTPS: `host:port`, or the `https=` element of a
/// per-scheme list; `None` when the list names no proxy for HTTPS.
fn https_proxy(static_proxy: &str) -> Option<String> {
    let elements: Vec<&str> = static_proxy.split(';').map(str::trim).collect();
    if elements.iter().any(|e| e.contains('=')) {
        return elements
            .iter()
            .find_map(|e| e.strip_prefix("https="))
            .map(str::to_owned);
    }
    elements
        .first()
        .filter(|e| !e.is_empty())
        .map(|e| (*e).to_owned())
}

/// True when `entry` bypasses the static proxy (WinINet's reading of the
/// bypass list): `<local>` for a name without a dot, `*` patterns matched
/// case-insensitively against the host; a `*.` entry is matched on its
/// suffix.
pub fn bypassed(entry: &str, bypass: &str) -> bool {
    let host = entry
        .strip_prefix("*.")
        .unwrap_or(entry)
        .to_ascii_lowercase();
    bypass.split(';').map(str::trim).any(|pattern| {
        let pattern = pattern.to_ascii_lowercase();
        if pattern == "<local>" {
            return !host.contains('.');
        }
        glob(&pattern, &host)
    })
}

/// `*` matches any run of characters, everything else itself.
fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    true
}

/// Every entry's route for `setting` (D-611): `"none"` direct; an explicit
/// `host:port` for every entry; `"system"` the PAC's answers when a PAC
/// applies, else the static proxy and its bypass list, else direct.
pub fn routes(
    setting: &UpstreamProxy,
    allow: &[String],
    system: &SystemProxy,
) -> Result<Routes, RouteRefusal> {
    let all = |route: Route| allow.iter().map(|e| (e.clone(), route.clone())).collect();
    match setting {
        UpstreamProxy::Direct => {
            return Ok(Routes {
                source: ProxySource::None,
                routes: all(Route::Direct),
            });
        }
        UpstreamProxy::Explicit(target) => {
            return Ok(Routes {
                source: ProxySource::Floor,
                routes: all(Route::Proxy(target.clone())),
            });
        }
        UpstreamProxy::System => {}
    }
    if system.pac_url.is_some() || system.auto_detect {
        let source = if system.pac_url.is_some() {
            ProxySource::Pac
        } else {
            ProxySource::AutoDetect
        };
        let mut routes = BTreeMap::new();
        let mut found_pac = false;
        for entry in allow {
            let route = match system.pac_answers.get(entry) {
                Some(PacAnswer::Answer(answer)) => {
                    found_pac = true;
                    pac_route(answer).map_err(|cause| RouteRefusal {
                        cause,
                        entry: entry.clone(),
                    })?
                }
                Some(PacAnswer::NoPac) if system.pac_url.is_none() => Route::Direct,
                Some(PacAnswer::NoPac | PacAnswer::Failed) | None => {
                    return Err(RouteRefusal {
                        cause: ProxyRefusal::ProxyUnresolved,
                        entry: entry.clone(),
                    });
                }
            };
            routes.insert(entry.clone(), route);
        }
        // Auto-detect finding no PAC means direct, as Windows does; the
        // static proxy, when set, then applies.
        if found_pac || system.static_proxy.is_none() {
            let source = if found_pac { source } else { ProxySource::None };
            return Ok(Routes { source, routes });
        }
    }
    let Some(proxy) = system.static_proxy.as_deref().and_then(https_proxy) else {
        return Ok(Routes {
            source: ProxySource::None,
            routes: all(Route::Direct),
        });
    };
    let bypass = system.bypass.as_deref().unwrap_or("");
    Ok(Routes {
        source: ProxySource::Static,
        routes: allow
            .iter()
            .map(|e| {
                let route = if bypassed(e, bypass) {
                    Route::Direct
                } else {
                    Route::Proxy(proxy.clone())
                };
                (e.clone(), route)
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow() -> Vec<String> {
        vec![
            "api.anthropic.com".into(),
            "index.crates.io".into(),
            "*.pkg.example.com".into(),
        ]
    }

    #[test]
    fn none_is_direct() {
        let r = routes(&UpstreamProxy::Direct, &allow(), &SystemProxy::default()).unwrap();
        assert_eq!(r.source, ProxySource::None);
        assert!(r.routes.values().all(|route| *route == Route::Direct));
        assert!(r.gateway_routes().is_empty());
    }

    #[test]
    fn an_explicit_proxy_routes_every_entry() {
        let setting = UpstreamProxy::Explicit("proxy.example.com:8080".into());
        let r = routes(&setting, &allow(), &SystemProxy::default()).unwrap();
        assert_eq!(r.source, ProxySource::Floor);
        assert_eq!(r.gateway_routes().len(), allow().len());
    }

    #[test]
    fn a_bypassed_host_is_direct() {
        let system = SystemProxy {
            static_proxy: Some("proxy.example.com:8080".into()),
            bypass: Some("*.crates.io;<local>".into()),
            ..SystemProxy::default()
        };
        let r = routes(&UpstreamProxy::System, &allow(), &system).unwrap();
        assert_eq!(r.source, ProxySource::Static);
        assert_eq!(r.routes["index.crates.io"], Route::Direct);
        assert_eq!(
            r.routes["api.anthropic.com"],
            Route::Proxy("proxy.example.com:8080".into())
        );
        assert!(bypassed("intranet", "<local>"));
    }

    #[test]
    fn the_first_proxy_element_wins() {
        assert_eq!(
            pac_route("PROXY a.example.com:3128; PROXY b.example.com:3128; DIRECT"),
            Ok(Route::Proxy("a.example.com:3128".into()))
        );
        assert_eq!(pac_route("DIRECT"), Ok(Route::Direct));
    }

    #[test]
    fn winhttp_lists_map_to_pac_routes() {
        let route = |list: &str| pac_route(&winhttp_answer(list));
        assert_eq!(
            route("proxy.example.com:8080"),
            Ok(Route::Proxy("proxy.example.com:8080".into()))
        );
        assert_eq!(
            route("http://proxy.example.com:8080/ backup.example.com:80"),
            Ok(Route::Proxy("proxy.example.com:8080".into()))
        );
        assert_eq!(
            route("https=proxy.example.com:3128"),
            Ok(Route::Proxy("proxy.example.com:3128".into())),
            "a per-scheme entry names a plain proxy"
        );
        for refused in [
            "socks=s.example.com:1080",
            "socks5://s.example.com:1080",
            "https://p.example.com:443",
        ] {
            assert_eq!(
                route(refused),
                Err(ProxyRefusal::ProxyUnsupported),
                "{refused}"
            );
        }
        assert_eq!(route(""), Ok(Route::Direct));
    }

    #[test]
    fn socks_is_unsupported() {
        for answer in [
            "SOCKS proxy.example.com:1080",
            "HTTPS proxy.example.com:443",
            "PROXY",
            "garbage",
        ] {
            assert_eq!(
                pac_route(answer),
                Err(ProxyRefusal::ProxyUnsupported),
                "{answer}"
            );
        }
        let system = SystemProxy {
            pac_url: Some("http://wpad.example.com/proxy.pac".into()),
            pac_answers: allow()
                .into_iter()
                .map(|e| (e, PacAnswer::Answer("SOCKS5 s.example.com:1080".into())))
                .collect(),
            ..SystemProxy::default()
        };
        let refused = routes(&UpstreamProxy::System, &allow(), &system).unwrap_err();
        assert_eq!(refused.cause, ProxyRefusal::ProxyUnsupported);
        assert_eq!(refused.entry, "api.anthropic.com");
    }

    #[test]
    fn a_wildcard_entry_uses_its_suffix() {
        assert_eq!(pac_url("*.pkg.example.com"), "https://pkg.example.com/");
        assert_eq!(pac_url("api.anthropic.com"), "https://api.anthropic.com/");
        assert!(bypassed("*.pkg.example.com", "pkg.example.com"));
    }

    #[test]
    fn auto_detect_without_a_pac_is_direct_and_a_failed_pac_is_unresolved() {
        let no_pac = SystemProxy {
            auto_detect: true,
            pac_answers: allow().into_iter().map(|e| (e, PacAnswer::NoPac)).collect(),
            ..SystemProxy::default()
        };
        let r = routes(&UpstreamProxy::System, &allow(), &no_pac).unwrap();
        assert!(r.routes.values().all(|route| *route == Route::Direct));
        let failed = SystemProxy {
            pac_url: Some("http://wpad.example.com/proxy.pac".into()),
            ..SystemProxy::default()
        };
        assert_eq!(
            routes(&UpstreamProxy::System, &allow(), &failed)
                .unwrap_err()
                .cause,
            ProxyRefusal::ProxyUnresolved
        );
    }

    // ---- the `proxy` section of tests/vectors/workshop.json (AC 2) ----

    fn case(
        name: &str,
        setting: UpstreamProxy,
        system: SystemProxy,
    ) -> (String, UpstreamProxy, SystemProxy) {
        (name.to_owned(), setting, system)
    }

    fn answers(answer: PacAnswer) -> BTreeMap<String, PacAnswer> {
        allow().into_iter().map(|e| (e, answer.clone())).collect()
    }

    fn cases() -> Vec<(String, UpstreamProxy, SystemProxy)> {
        let pac = Some("http://wpad.example.com/proxy.pac".to_owned());
        vec![
            case("none", UpstreamProxy::Direct, SystemProxy::default()),
            case(
                "an explicit proxy",
                UpstreamProxy::Explicit("proxy.example.com:8080".into()),
                SystemProxy::default(),
            ),
            case(
                "system with nothing configured",
                UpstreamProxy::System,
                SystemProxy::default(),
            ),
            case(
                "a static proxy with a bypass list",
                UpstreamProxy::System,
                SystemProxy {
                    static_proxy: Some(
                        "http=web.example.com:80;https=proxy.example.com:8443".into(),
                    ),
                    bypass: Some("*.crates.io;<local>".into()),
                    ..SystemProxy::default()
                },
            ),
            case(
                "a PAC answering a proxy then direct",
                UpstreamProxy::System,
                SystemProxy {
                    pac_url: pac.clone(),
                    pac_answers: answers(PacAnswer::Answer(
                        "PROXY pac.example.com:3128; DIRECT".into(),
                    )),
                    ..SystemProxy::default()
                },
            ),
            case(
                "automatic detection finding a PAC",
                UpstreamProxy::System,
                SystemProxy {
                    auto_detect: true,
                    pac_answers: answers(PacAnswer::Answer("DIRECT".into())),
                    ..SystemProxy::default()
                },
            ),
            case(
                "automatic detection finding none",
                UpstreamProxy::System,
                SystemProxy {
                    auto_detect: true,
                    pac_answers: answers(PacAnswer::NoPac),
                    ..SystemProxy::default()
                },
            ),
            case(
                "a PAC answering SOCKS",
                UpstreamProxy::System,
                SystemProxy {
                    pac_url: pac.clone(),
                    pac_answers: answers(PacAnswer::Answer("SOCKS socks.example.com:1080".into())),
                    ..SystemProxy::default()
                },
            ),
            case(
                "a PAC that cannot be fetched",
                UpstreamProxy::System,
                SystemProxy {
                    pac_url: pac,
                    pac_answers: answers(PacAnswer::Failed),
                    ..SystemProxy::default()
                },
            ),
        ]
    }

    fn built_cases() -> serde_json::Value {
        let cases: Vec<serde_json::Value> = cases()
            .into_iter()
            .map(|(name, setting, system)| {
                let expect = match routes(&setting, &allow(), &system) {
                    Ok(routes) => {
                        serde_json::json!({"source": routes.source, "routes": routes.routes})
                    }
                    Err(refusal) => {
                        serde_json::json!({"refused": refusal.cause, "entry": refusal.entry})
                    }
                };
                serde_json::json!({
                    "name": name,
                    "setting": setting.setting(),
                    "system": {
                        "auto_detect": system.auto_detect,
                        "pac_url": system.pac_url,
                        "static_proxy": system.static_proxy,
                        "bypass": system.bypass,
                    },
                    "expect": expect,
                })
            })
            .collect();
        serde_json::json!({ "allow": allow(), "cases": cases })
    }

    fn vectors_file() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../../tests/vectors/workshop.json")).expect("json")
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "a test's write mode for a reviewed change of the golden file"
    )]
    fn write_if_asked(value: &serde_json::Value) -> bool {
        if std::env::var("OFFICINA_WRITE_VECTORS").is_ok_and(|v| v == "1") {
            let path = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/vectors/workshop.json"
            );
            let mut file: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            file["proxy"] = value.clone();
            std::fs::write(path, serde_json::to_string_pretty(&file).unwrap() + "\n").unwrap();
            return true;
        }
        false
    }

    #[test]
    fn proxy_cases_match() {
        let built = built_cases();
        if write_if_asked(&built) {
            return;
        }
        assert_eq!(vectors_file()["proxy"], built);
    }

    #[test]
    fn every_refusal_and_source_has_a_case() {
        let file = vectors_file();
        let cases = file["proxy"]["cases"].as_array().expect("cases");
        for source in ProxySource::ALL {
            assert!(
                cases
                    .iter()
                    .any(|c| c["expect"]["source"] == serde_json::json!(source)),
                "{source:?} has no case"
            );
        }
        for refusal in ProxyRefusal::ALL {
            assert!(
                cases
                    .iter()
                    .any(|c| c["expect"]["refused"] == serde_json::json!(refusal)),
                "{refusal:?} has no case"
            );
        }
    }
}
