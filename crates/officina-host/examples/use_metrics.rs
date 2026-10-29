//! The use metrics of PRD section 7 (story 5.19 AC 4): per UTC day, the
//! work orders started, the answers sent and confirmed by source, and the
//! author's decisions beside the review's and the rule's, counted by
//! `officina_core::orders::metrics::per_day` over the relay journal read as
//! `officina relay --json` reads it. Read-only.
//!
//! Usage: `cargo run -p officina-host --example use_metrics -- [days]`,
//! with `OFFICINA_HOME` set as for the window when it is not the default.
//! `days` defaults to the journal's retention. Exit 1 without an Officina
//! home.

use std::process::ExitCode;

use officina_core::orders::metrics::{BySource, per_day};
use officina_core::relay::journal::JOURNAL_RETENTION_DAYS;
use officina_host::files::now_ms;
use officina_host::relay::home::RelayHome;
use officina_host::relay::journal::read_days;

fn share(confirmed: u32, sent: u32) -> String {
    if sent == 0 {
        "none sent".to_owned()
    } else {
        format!(
            "{confirmed} of {sent} ({:.0} %)",
            f64::from(confirmed) * 100.0 / f64::from(sent)
        )
    }
}

fn main() -> ExitCode {
    let days = match std::env::args().nth(1).map(|arg| arg.parse::<i64>()) {
        None => JOURNAL_RETENTION_DAYS,
        Some(Ok(days)) if days > 0 => days,
        Some(_) => {
            eprintln!("usage: use_metrics [days]");
            return ExitCode::from(64);
        }
    };
    let Some(home) = RelayHome::from_env() else {
        eprintln!("no Officina home: set OFFICINA_HOME");
        return ExitCode::FAILURE;
    };
    let (lines, corrupt) = read_days(&home, now_ms(), days);
    let used = per_day(&lines);
    println!(
        "day         orders  sent(author/proposal)  confirmed(author/proposal)  decisions(author/auto)"
    );
    let mut sent = BySource::default();
    let mut confirmed = BySource::default();
    for day in &used {
        println!(
            "{}  {:>6}  {:>10}/{:<10}  {:>14}/{:<11}  {:>11}/{}",
            day.day,
            day.work_orders_started,
            day.answers_sent.author,
            day.answers_sent.proposal,
            day.answers_confirmed.author,
            day.answers_confirmed.proposal,
            day.author_decisions,
            day.auto_decisions
        );
        sent.author += day.answers_sent.author;
        sent.proposal += day.answers_sent.proposal;
        confirmed.author += day.answers_confirmed.author;
        confirmed.proposal += day.answers_confirmed.proposal;
    }
    println!();
    println!(
        "Use: {} days with a work order started or an answer sent, of the last {days} (the PRD's target: 10 working days)",
        used.iter().filter(|day| day.used()).count()
    );
    println!(
        "Delivery share: author {}; proposal {}",
        share(confirmed.author, sent.author),
        share(confirmed.proposal, sent.proposal)
    );
    if corrupt > 0 {
        println!("{corrupt} journal lines could not be read and are not counted");
    }
    ExitCode::SUCCESS
}
