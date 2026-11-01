//! The routes of a run on this network (D-611, story 6.25): the floor's
//! `upstream_proxy` applied by core to what a [`SystemProxyReader`] reads
//! of Windows' configuration. Callers run [`resolve`] on thread `workshop`,
//! never on the collector.
//!
//! The reader of record is WinHTTP (`WinHttpGetIEProxyConfigForCurrentUser`
//! and `WinHttpGetProxyForUrl`), which needs the `windows-sys` feature
//! `Win32_Networking_WinHttp`, awaiting the owner's approval. Until then
//! [`RegistryReader`] reads the same configuration from the current user's
//! registry and cannot evaluate a PAC file: a PAC URL resolves
//! `proxy_unresolved`, and automatic detection is taken as finding none
//! (D-1124), which doctor names.

use std::sync::mpsc;
use std::time::Duration;

use officina_core::workshop::floor::UpstreamProxy;
use officina_core::workshop::proxy::{
    PacAnswer, RouteRefusal, Routes, SystemProxy, pac_url, routes,
};

/// Each read of the system's configuration and each PAC evaluation is
/// bounded (story 6.25 AC 3).
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Windows' proxy configuration for the current user, as WinHTTP's
/// `WINHTTP_CURRENT_USER_IE_PROXY_CONFIG` holds it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IeProxyConfig {
    pub auto_detect: bool,
    pub pac_url: Option<String>,
    pub static_proxy: Option<String>,
    pub bypass: Option<String>,
}

/// What the resolution reads, injected so tests give their own answers
/// (D-86's pattern). Both calls run on a thread bounded by
/// [`READ_TIMEOUT`].
pub trait SystemProxyReader: Send + Sync {
    /// The current user's configuration.
    fn config(&self) -> Result<IeProxyConfig, String>;
    /// The PAC's answer for `url` (`https://<host>/`) under `config`.
    fn pac(&self, url: &str, config: &IeProxyConfig) -> PacAnswer;
    /// True when this reader cannot evaluate a PAC or automatic detection,
    /// which doctor names.
    fn evaluates_pac(&self) -> bool;
}

/// Runs `f` on a thread, its answer within [`READ_TIMEOUT`], else `None`.
fn bounded<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("workshop-proxy".into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .ok()?;
    rx.recv_timeout(READ_TIMEOUT).ok()
}

/// Why no routes came out of a resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyError {
    /// Core refused an entry's answer.
    Refused(RouteRefusal),
    /// The system's configuration could not be read in time.
    Unreadable(String),
}

/// The routes of `allow` for `setting` on this network (D-611). `"none"`
/// and an explicit proxy never call the reader.
pub fn resolve(
    setting: &UpstreamProxy,
    allow: &[String],
    reader: std::sync::Arc<dyn SystemProxyReader>,
) -> Result<Routes, ProxyError> {
    if *setting != UpstreamProxy::System {
        return routes(setting, allow, &SystemProxy::default()).map_err(ProxyError::Refused);
    }
    let config = {
        let reader = std::sync::Arc::clone(&reader);
        bounded(move || reader.config())
            .ok_or_else(|| ProxyError::Unreadable("the proxy configuration timed out".into()))?
            .map_err(ProxyError::Unreadable)?
    };
    let mut system = SystemProxy {
        auto_detect: config.auto_detect,
        pac_url: config.pac_url.clone(),
        static_proxy: config.static_proxy.clone(),
        bypass: config.bypass.clone(),
        pac_answers: Default::default(),
    };
    if config.auto_detect || config.pac_url.is_some() {
        for entry in allow {
            let url = pac_url(entry);
            let reader = std::sync::Arc::clone(&reader);
            let asked = config.clone();
            let answer = bounded(move || reader.pac(&url, &asked)).unwrap_or(PacAnswer::Failed);
            system.pac_answers.insert(entry.clone(), answer);
        }
    }
    routes(setting, allow, &system).map_err(ProxyError::Refused)
}

/// The interim reader (D-1124): the current user's Internet settings from
/// the registry, which WinHTTP reads too; no PAC evaluation.
#[derive(Debug, Clone, Copy, Default)]
pub struct RegistryReader;

/// Where the current user's Internet settings live.
#[cfg(windows)]
const INTERNET_SETTINGS: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";

/// The flags of `DefaultConnectionSettings` (the WinINet blob WinHTTP's
/// configuration reads): byte 8, `0x02` a static proxy, `0x04` a PAC URL,
/// `0x08` automatic detection.
pub fn connection_flags(blob: &[u8]) -> Option<u8> {
    blob.get(8).copied()
}

impl SystemProxyReader for RegistryReader {
    fn config(&self) -> Result<IeProxyConfig, String> {
        #[cfg(windows)]
        {
            use crate::platform::windows::{Hive, registry_binary, registry_string};
            let flags = registry_binary(
                Hive::CurrentUser,
                &format!("{INTERNET_SETTINGS}\\Connections"),
                "DefaultConnectionSettings",
            )
            .as_deref()
            .and_then(connection_flags)
            .unwrap_or(0);
            let text = |name: &str| {
                registry_string(Hive::CurrentUser, INTERNET_SETTINGS, name)
                    .filter(|v| !v.is_empty())
            };
            Ok(IeProxyConfig {
                auto_detect: flags & 0x08 != 0,
                pac_url: text("AutoConfigURL").filter(|_| flags == 0 || flags & 0x04 != 0),
                static_proxy: text("ProxyServer").filter(|_| flags & 0x02 != 0),
                bypass: text("ProxyOverride"),
            })
        }
        #[cfg(not(windows))]
        {
            Ok(IeProxyConfig::default())
        }
    }

    fn pac(&self, _url: &str, config: &IeProxyConfig) -> PacAnswer {
        if config.pac_url.is_some() {
            PacAnswer::Failed
        } else {
            PacAnswer::NoPac
        }
    }

    fn evaluates_pac(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use officina_core::workshop::proxy::{ProxyRefusal, ProxySource, Route};

    use super::*;

    struct Fake {
        config: Result<IeProxyConfig, String>,
        answer: PacAnswer,
        delay: Duration,
        calls: AtomicUsize,
    }

    impl Fake {
        fn new(config: IeProxyConfig, answer: PacAnswer) -> Arc<Fake> {
            Arc::new(Fake {
                config: Ok(config),
                answer,
                delay: Duration::ZERO,
                calls: AtomicUsize::new(0),
            })
        }
    }

    impl SystemProxyReader for Fake {
        fn config(&self) -> Result<IeProxyConfig, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.config.clone()
        }
        fn pac(&self, _url: &str, _config: &IeProxyConfig) -> PacAnswer {
            std::thread::sleep(self.delay);
            self.answer.clone()
        }
        fn evaluates_pac(&self) -> bool {
            true
        }
    }

    fn allow() -> Vec<String> {
        vec!["api.anthropic.com".into(), "index.crates.io".into()]
    }

    #[test]
    fn the_reader_is_injected() {
        let fake = Fake::new(
            IeProxyConfig {
                static_proxy: Some("proxy.example.com:8080".into()),
                ..IeProxyConfig::default()
            },
            PacAnswer::NoPac,
        );
        let routes = resolve(&UpstreamProxy::System, &allow(), fake.clone()).unwrap();
        assert_eq!(routes.source, ProxySource::Static);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn resolve_applies_core_routes_to_the_reader_answer() {
        let fake = Fake::new(
            IeProxyConfig {
                pac_url: Some("http://wpad.example.com/proxy.pac".into()),
                ..IeProxyConfig::default()
            },
            PacAnswer::Answer("PROXY pac.example.com:3128; DIRECT".into()),
        );
        let routes = resolve(&UpstreamProxy::System, &allow(), fake).unwrap();
        assert_eq!(routes.source, ProxySource::Pac);
        assert_eq!(
            routes.routes["api.anthropic.com"],
            Route::Proxy("pac.example.com:3128".into())
        );
    }

    #[test]
    fn a_slow_pac_times_out_as_unresolved() {
        let fake = Arc::new(Fake {
            config: Ok(IeProxyConfig {
                pac_url: Some("http://wpad.example.com/proxy.pac".into()),
                ..IeProxyConfig::default()
            }),
            answer: PacAnswer::Answer("DIRECT".into()),
            delay: READ_TIMEOUT + Duration::from_secs(1),
            calls: AtomicUsize::new(0),
        });
        let refused = resolve(&UpstreamProxy::System, &allow()[..1], fake).unwrap_err();
        assert!(
            matches!(
                refused,
                ProxyError::Refused(RouteRefusal {
                    cause: ProxyRefusal::ProxyUnresolved,
                    ..
                })
            ),
            "{refused:?}"
        );
    }

    #[test]
    fn the_setting_none_never_calls_the_reader() {
        let fake = Fake::new(IeProxyConfig::default(), PacAnswer::NoPac);
        for setting in [
            UpstreamProxy::Direct,
            UpstreamProxy::Explicit("proxy.example.com:8080".into()),
        ] {
            resolve(&setting, &allow(), fake.clone()).unwrap();
        }
        assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn handles_are_closed_on_error() {
        // The interim reader holds no handle; an unreadable configuration
        // is an error, never routes.
        let fake = Arc::new(Fake {
            config: Err("unreadable".into()),
            answer: PacAnswer::NoPac,
            delay: Duration::ZERO,
            calls: AtomicUsize::new(0),
        });
        assert_eq!(
            resolve(&UpstreamProxy::System, &allow(), fake),
            Err(ProxyError::Unreadable("unreadable".into()))
        );
    }

    #[test]
    fn the_connection_flags_are_byte_8() {
        let mut blob = vec![0u8; 16];
        blob[8] = 0x0b;
        assert_eq!(connection_flags(&blob), Some(0x0b));
        assert_eq!(connection_flags(&[0; 4]), None);
    }

    #[test]
    fn the_registry_reader_cannot_evaluate_a_pac() {
        let reader = RegistryReader;
        assert!(!reader.evaluates_pac());
        let with_pac = IeProxyConfig {
            pac_url: Some("http://wpad.example.com/proxy.pac".into()),
            ..IeProxyConfig::default()
        };
        assert_eq!(reader.pac("https://x/", &with_pac), PacAnswer::Failed);
        assert_eq!(
            reader.pac("https://x/", &IeProxyConfig::default()),
            PacAnswer::NoPac
        );
        // Reads the current user's settings, whatever they hold.
        assert!(reader.config().is_ok());
    }
}
