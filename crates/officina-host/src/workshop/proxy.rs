//! The routes of a run on this network (D-611, story 6.25): the floor's
//! `upstream_proxy` applied by core to what a [`SystemProxyReader`] reads
//! of Windows' configuration. Callers run [`resolve`] on thread `workshop`,
//! never on the collector.
//!
//! The reader is WinHTTP ([`WinHttpReader`]:
//! `WinHttpGetIEProxyConfigForCurrentUser` and `WinHttpGetProxyForUrl`),
//! which evaluates PAC files and automatic detection as Windows does
//! (D-611, D-1136); it replaced the interim registry reader of D-1124.

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

/// The reader of record (D-611, D-1136): WinHTTP, which reads the current
/// user's configuration (`WinHttpGetIEProxyConfigForCurrentUser`) and
/// fetches and evaluates a PAC file or runs automatic detection
/// (`WinHttpGetProxyForUrl`) as Windows does. Off Windows (CI) there is no
/// system configuration: every route is direct, and it says it evaluates
/// nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinHttpReader;

impl SystemProxyReader for WinHttpReader {
    fn config(&self) -> Result<IeProxyConfig, String> {
        #[cfg(windows)]
        {
            let config =
                crate::platform::windows::winhttp_ie_config().map_err(|e| e.to_string())?;
            Ok(IeProxyConfig {
                auto_detect: config.auto_detect,
                pac_url: config.pac_url,
                static_proxy: config.proxy,
                bypass: config.bypass,
            })
        }
        #[cfg(not(windows))]
        {
            Ok(IeProxyConfig::default())
        }
    }

    fn pac(&self, url: &str, config: &IeProxyConfig) -> PacAnswer {
        #[cfg(windows)]
        {
            use crate::platform::windows::{WinHttpProxyAnswer, winhttp_proxy_for_url};
            let timeout_ms = i32::try_from(READ_TIMEOUT.as_millis()).unwrap_or(i32::MAX);
            match winhttp_proxy_for_url(
                url,
                config.auto_detect,
                config.pac_url.as_deref(),
                timeout_ms,
            ) {
                WinHttpProxyAnswer::Direct => PacAnswer::Answer("DIRECT".to_owned()),
                WinHttpProxyAnswer::Named(list) => {
                    PacAnswer::Answer(officina_core::workshop::proxy::winhttp_answer(&list))
                }
                WinHttpProxyAnswer::NoPac => PacAnswer::NoPac,
                WinHttpProxyAnswer::Failed(_) => PacAnswer::Failed,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (url, config);
            PacAnswer::NoPac
        }
    }

    fn evaluates_pac(&self) -> bool {
        cfg!(windows)
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
        // WinHTTP's handles are closed by their guard on every path in
        // platform/windows.rs; an unreadable configuration is an error, never routes.
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

    /// Story 6.25 AC 1: the WinHTTP reader evaluates PAC files on Windows
    /// and reads the current user's configuration, whatever it holds; a
    /// PAC file it cannot fetch is `failed`, never a route.
    #[test]
    fn the_winhttp_reader_evaluates_pac_files() {
        let reader = WinHttpReader;
        assert_eq!(reader.evaluates_pac(), cfg!(windows));
        assert!(reader.config().is_ok());
        let unreachable = IeProxyConfig {
            pac_url: Some("http://127.0.0.1:9/officina-test.pac".into()),
            ..IeProxyConfig::default()
        };
        let answer = reader.pac("https://api.anthropic.com/", &unreachable);
        if cfg!(windows) {
            assert_eq!(answer, PacAnswer::Failed);
        } else {
            assert_eq!(answer, PacAnswer::NoPac);
        }
        assert_eq!(
            reader.pac("https://api.anthropic.com/", &IeProxyConfig::default()),
            PacAnswer::NoPac,
            "no PAC file and no detection asks nothing"
        );
    }
}
