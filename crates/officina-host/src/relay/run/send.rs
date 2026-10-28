//! Sending an HTTP action's request (AD-36, D-35) and ending its run.

use std::io::Write;
use std::time::Instant;

use officina_core::relay::http::HttpPlan;
use officina_core::relay::journal::RunOutcome;
use officina_core::relay::run::{EXIT_NOT_STARTED, EXIT_TIMEOUT, RunPlan};

use super::journaling::{Response, end_run, http_finished};
use super::kept::Tee;
use crate::relay::credential::Secret;
use crate::relay::home::RelayHome;
use crate::relay::http::{self, HttpOutcome};

/// How an HTTP run ends (D-35): what the journal records, the line said on
/// stderr, and the exit code.
#[derive(Debug, PartialEq, Eq)]
struct HttpEnd {
    outcome: RunOutcome,
    response: Option<Response>,
    detail: Option<String>,
    said: Option<String>,
    code: i32,
}

/// The ending of `outcome` for `request`, sent by `action`: 0 for a 2xx
/// status whose body arrived whole or was cut at the cap, 1 for any other
/// status or a body that broke off (one line `officina: http <status>`),
/// 124 on timeout, 127 without a response; never 2.
fn http_end(outcome: HttpOutcome, request: &HttpPlan, action: &str) -> HttpEnd {
    match outcome {
        HttpOutcome::Response {
            status,
            body_bytes,
            body_sha256,
            cut,
            read_error,
        } => {
            let detail = match (&read_error, cut) {
                (Some(e), _) => Some(format!("the body broke off: {e}")),
                (None, true) => Some(format!(
                    "body cut at {} KiB, connection dropped",
                    request.max_body_bytes / 1024
                )),
                (None, false) => None,
            };
            let success = (200..300).contains(&status) && read_error.is_none();
            let said = (!success).then(|| match &read_error {
                Some(e) => format!("officina: http {status}, the body broke off: {e}"),
                None => format!("officina: http {status}"),
            });
            HttpEnd {
                outcome: RunOutcome::Http,
                response: Some(Response {
                    status,
                    body_bytes,
                    body_sha256,
                }),
                detail,
                said,
                code: i32::from(!success),
            }
        }
        HttpOutcome::Timeout => HttpEnd {
            outcome: RunOutcome::Timeout,
            response: None,
            detail: None,
            said: Some(format!(
                "officina: timeout: {action} got no complete response within {} s",
                request.timeout_s
            )),
            code: EXIT_TIMEOUT,
        },
        HttpOutcome::ConnectFailed { detail } => HttpEnd {
            outcome: RunOutcome::ConnectFailed,
            response: None,
            said: Some(format!(
                "officina: no response from {}: {detail}",
                request.host
            )),
            detail: Some(detail),
            code: EXIT_NOT_STARTED,
        },
    }
}

/// Sends an HTTP action's request (AD-36), writes the response to `out`,
/// says on `err` what did not succeed, ends the run ([`end_run`]) and
/// returns the exit code of [`http_end`].
pub(super) fn send_request(
    home: &RelayHome,
    plan: &RunPlan,
    request: &HttpPlan,
    run_id: &str,
    secret: Option<Secret>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let started = Instant::now();
    let mut tee = Tee::new(out, plan.keeps_output);
    let outcome = http::send(request, secret.as_ref(), &mut tee);
    drop(secret);
    let ended = http_end(outcome, request, &plan.action);
    if let Some(said) = &ended.said {
        let _ = writeln!(err, "{said}");
    }
    let line = http_finished(run_id, ended.outcome, started, ended.response, ended.detail);
    end_run(home, err, run_id, line, tee.kept());
    ended.code
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::relay::http::HttpMethod;

    fn request() -> HttpPlan {
        HttpPlan {
            method: HttpMethod::Get,
            url: "https://monitoring.example.com/api/health".to_owned(),
            host: "monitoring.example.com".to_owned(),
            credential: None,
            timeout_s: 5,
            max_body_bytes: 64 * 1024,
        }
    }

    fn response(status: u16, cut: bool, read_error: Option<&str>) -> HttpOutcome {
        HttpOutcome::Response {
            status,
            body_bytes: 2,
            body_sha256: "00".to_owned(),
            cut,
            read_error: read_error.map(str::to_owned),
        }
    }

    #[test]
    fn each_outcome_ends_with_its_code_and_line() {
        let ok = http_end(response(204, false, None), &request(), "health");
        assert_eq!((ok.code, ok.said, ok.detail), (0, None, None));
        let cut = http_end(response(200, true, None), &request(), "health");
        assert_eq!(cut.code, 0);
        assert_eq!(
            cut.detail.as_deref(),
            Some("body cut at 64 KiB, connection dropped")
        );
        let failed = http_end(response(503, false, None), &request(), "health");
        assert_eq!(
            (failed.code, failed.said.as_deref()),
            (1, Some("officina: http 503"))
        );
        let broke = http_end(response(200, false, Some("reset")), &request(), "health");
        assert_eq!(broke.code, 1);
        assert_eq!(broke.detail.as_deref(), Some("the body broke off: reset"));
        let timeout = http_end(HttpOutcome::Timeout, &request(), "health");
        assert_eq!(
            (timeout.code, timeout.outcome, timeout.response),
            (EXIT_TIMEOUT, RunOutcome::Timeout, None)
        );
        let refused = http_end(
            HttpOutcome::ConnectFailed {
                detail: "connection refused".to_owned(),
            },
            &request(),
            "health",
        );
        assert_eq!(
            (refused.code, refused.said.as_deref()),
            (
                EXIT_NOT_STARTED,
                Some("officina: no response from monitoring.example.com: connection refused")
            )
        );
    }
}
