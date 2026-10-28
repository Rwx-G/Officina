//! The taskbar probe, gate 2 of AD-178 (story 4.1 AC 2, run 2 after D-523):
//! shows the overlay badges the window uses, at the one size the window
//! uses (the small icon's at the display scale): the waiting badge for 1, 9
//! and above nine, then the alert badge; then requests the user's attention
//! once while another window has focus, and once while the probe has it.
//! Run by the owner, by hand, with a light and a dark taskbar; never by a
//! test, never part of a shipped binary.
//!
//! Usage: `cargo run -p officina-app --example taskbar_probe`. Its window
//! says what the taskbar shows at each step and asks the owner what he sees,
//! so one run gives the whole entry: the answers, the display scale, the
//! Windows build and the Tauri version are printed at the end and appended
//! to `taskbar-probe-report.md` beside the probe's executable. It has no
//! collector, no single-instance lock and no floor: it reads and writes
//! nothing under the Officina home or the Claude config dir.

// The window uses the rest of the module (the title, the alert choice);
// the probe shows the badges only.
#[allow(dead_code)]
#[path = "../src/taskbar.rs"]
mod taskbar;

use std::fmt::Write as _;
use std::io::Write as _;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tauri::{UserAttentionType, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// How long each count stays before the next one, while a badge is rated.
const ROTATE: Duration = Duration::from_millis(1500);
/// The time the owner has to focus another window before the flash.
const FOCUS_ELSEWHERE: Duration = Duration::from_secs(5);
/// How long the probe waits for the owner to click its button back.
const COME_BACK: Duration = Duration::from_secs(120);
/// How long the taskbar is watched after the request made while focused.
const WATCH_FOCUSED: Duration = Duration::from_secs(5);

/// The page's buttons are links to this host: the probe cancels the
/// navigation and reads the answer from its query, so the page needs no
/// command and no capability.
const ANSWER_HOST: &str = "probe.invalid";

const STYLE: &str = "<style>\
body{font:15px/1.45 'Segoe UI',sans-serif;margin:20px;color:#222;background:#faf8f4}\
h1{font-size:18px;margin:0 0 10px}\
.choices{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px}\
.choices a{padding:8px 14px;border:1px solid #888;border-radius:6px;color:#222;text-decoration:none;background:#fff}\
.choices a:hover{background:#eee}\
.step{color:#666;font-size:13px}\
pre{white-space:pre-wrap;font-size:12px;background:#fff;border:1px solid #ccc;padding:8px}\
</style>";

const READABLE: &[(&str, &str)] = &[
    ("easy", "Readable at a glance"),
    ("effort", "Readable with effort"),
    ("no", "Not readable"),
    ("none", "No badge visible"),
];

enum Signal {
    Loaded,
    Answer(String),
}

struct Probe {
    window: WebviewWindow,
    signals: Receiver<Signal>,
}

impl Probe {
    fn render(&self, title: &str, body: &str, choices: &[(&str, &str)]) {
        let mut html = format!("{STYLE}<h1>{title}</h1>{body}<p class=\"choices\">");
        for (key, label) in choices {
            let _ = write!(
                html,
                "<a href=\"https://{ANSWER_HOST}/?a={key}\">{label}</a>"
            );
        }
        html.push_str("</p>");
        let html = serde_json::to_string(&html).expect("a string always serializes");
        if let Err(e) = self
            .window
            .eval(format!("document.body.innerHTML = {html};"))
        {
            eprintln!("taskbar_probe: the page could not be updated: {e}");
        }
    }

    /// Shows a step and waits for one of its choices, calling `tick` every
    /// [`ROTATE`] meanwhile; `None` once the window is gone.
    fn ask_ticking(
        &self,
        title: &str,
        body: &str,
        choices: &[(&str, &'static str)],
        mut tick: impl FnMut(),
    ) -> Option<&'static str> {
        self.render(title, body, choices);
        loop {
            match self.signals.recv_timeout(ROTATE) {
                Ok(Signal::Answer(key)) => {
                    if let Some((_, label)) = choices.iter().find(|(k, _)| *k == key) {
                        return Some(label);
                    }
                }
                Ok(Signal::Loaded) => {}
                Err(RecvTimeoutError::Timeout) => tick(),
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    fn ask(
        &self,
        title: &str,
        body: &str,
        choices: &[(&str, &'static str)],
    ) -> Option<&'static str> {
        self.ask_ticking(title, body, choices, || {})
    }

    fn focused(&self) -> bool {
        self.window.is_focused().unwrap_or(false)
    }
}

fn badge(window: &WebviewWindow, count: u32, alert: bool, size: u32) {
    let image = taskbar::overlay_badge(count, alert, size);
    let shown = tauri::image::Image::new_owned(image.rgba, image.size, image.size);
    if let Err(e) = window.set_overlay_icon(Some(shown)) {
        eprintln!("taskbar_probe: the overlay could not be set: {e}");
    }
}

struct BadgeRating {
    badge: &'static str,
    size: u32,
    readable: &'static str,
    distinct: Option<&'static str>,
}

/// Rates the waiting badge while it cycles through 1, 9 and 9+, then the
/// alert badge, at the size the window uses at this display scale.
fn rate_badges(probe: &Probe) -> Option<Vec<BadgeRating>> {
    let size = taskbar::overlay_size(probe.window.scale_factor().unwrap_or(1.0));
    let counts = [1, taskbar::MAX_SPELLED, taskbar::MAX_SPELLED + 1];
    let shown = std::cell::Cell::new(0);
    badge(&probe.window, counts[0], false, size);
    let cycle = || {
        shown.set((shown.get() + 1) % counts.len());
        badge(&probe.window, counts[shown.get()], false, size);
    };
    let body = format!(
        "<p class=\"step\">Badge 1 of 2</p>\
         <p>The probe's taskbar button now carries the <b>waiting badge</b> ({size} px, \
         amber), cycling through <b>1</b>, <b>9</b> and <b>9+</b> every 1.5 s.</p>\
         <p>Look at the taskbar button, not at this window: can you read the number?</p>"
    );
    let waiting = probe.ask_ticking("Waiting badge", &body, READABLE, cycle)?;
    badge(&probe.window, 1, true, size);
    let body = format!(
        "<p class=\"step\">Badge 2 of 2</p>\
         <p>The taskbar button now carries the <b>alert badge</b> ({size} px): the alert \
         colour with an exclamation mark instead of the count.</p>\
         <p>Can you see the exclamation mark?</p>"
    );
    let mark = probe.ask("Alert badge", &body, READABLE)?;
    let body =
        format!("<p>Same alert badge, {size} px. Is the colour clearly different from amber?</p>");
    let choices = [
        ("yes", "Clearly different"),
        ("barely", "Barely"),
        ("no", "Not different"),
    ];
    let distinct = probe.ask("Alert colour", &body, &choices)?;
    let _ = probe.window.set_overlay_icon(None);
    Some(vec![
        BadgeRating {
            badge: "waiting (1, 9, 9+)",
            size,
            readable: waiting,
            distinct: None,
        },
        BadgeRating {
            badge: "alert (!)",
            size,
            readable: mark,
            distinct: Some(distinct),
        },
    ])
}

struct FlashResult {
    unfocused: &'static str,
    came_back_after: Option<Duration>,
    focused: &'static str,
}

fn test_flash(probe: &Probe) -> Option<FlashResult> {
    let (unfocused, came_back_after) = loop {
        probe.ask(
            "Flash while unfocused",
            &format!(
                "<p>Click <b>Start</b>, then within {} s click another window (a terminal, \
                 the desktop). The probe then asks for attention: its taskbar button should \
                 flash until you click it.</p>",
                FOCUS_ELSEWHERE.as_secs()
            ),
            &[("start", "Start")],
        )?;
        sleep(FOCUS_ELSEWHERE);
        if probe.focused() {
            let retry = probe.ask(
                "The probe still had focus",
                "<p>No other window took focus, so the request would not show anything.</p>",
                &[("retry", "Try again"), ("skip", "Skip this test")],
            )?;
            if retry == "Try again" {
                continue;
            }
            break ("skipped", None);
        }
        let _ = probe
            .window
            .request_user_attention(Some(UserAttentionType::Informational));
        let asked = Instant::now();
        probe.render(
            "Watch the taskbar",
            "<p>The probe's taskbar button should be flashing now. Watch it for a few \
             seconds, then click it to come back here.</p>",
            &[],
        );
        let mut came_back = None;
        while asked.elapsed() < COME_BACK {
            if probe.focused() {
                came_back = Some(asked.elapsed());
                break;
            }
            sleep(Duration::from_millis(200));
        }
        let answer = probe.ask(
            "Did the button flash?",
            "<p>What did the taskbar button do between the request and your click?</p>",
            &[
                ("until", "Flashed until I clicked it"),
                ("stopped", "Flashed, then stopped on its own"),
                ("steady", "Lit up without flashing"),
                ("no", "Nothing"),
            ],
        )?;
        break (answer, came_back);
    };
    probe.ask(
        "Request while focused",
        "<p>Keep this window focused and click <b>Test</b>: the probe asks for attention \
         once more. Nothing should change on the taskbar.</p>",
        &[("test", "Test")],
    )?;
    let _ = probe
        .window
        .request_user_attention(Some(UserAttentionType::Informational));
    probe.render(
        "Watch the taskbar",
        &format!("<p>Watching for {} s.</p>", WATCH_FOCUSED.as_secs()),
        &[],
    );
    sleep(WATCH_FOCUSED);
    let focused = probe.ask(
        "Did anything change?",
        "<p>Did the taskbar button flash or light up after <b>Test</b>?</p>",
        &[
            ("nothing", "Nothing changed"),
            ("changed", "It flashed or lit up"),
        ],
    )?;
    Some(FlashResult {
        unfocused,
        came_back_after,
        focused,
    })
}

fn windows_build() -> String {
    Command::new("cmd")
        .args(["/c", "ver"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn report(
    run: u32,
    theme: &str,
    scale: Option<f64>,
    ratings: &[BadgeRating],
    flash: &FlashResult,
) -> String {
    let scale = scale.map_or("unknown".to_string(), |s| format!("{:.0} %", s * 100.0));
    let mut text = format!(
        "## Taskbar probe, run {run}\n\n\
         - Windows: {}\n- Tauri: {}\n- Display scaling: {scale}\n- Taskbar theme: {theme}\n\n\
         | Badge | Size | Readable | Colour clearly different from amber |\n|---|---|---|---|\n",
        windows_build(),
        tauri::VERSION,
    );
    for r in ratings {
        let _ = writeln!(
            text,
            "| {} | {} px | {} | {} |",
            r.badge,
            r.size,
            r.readable,
            r.distinct.unwrap_or("-"),
        );
    }
    let came_back = flash
        .came_back_after
        .map_or("no click within the wait".to_string(), |d| {
            format!("clicked back after {} s", d.as_secs())
        });
    let _ = write!(
        text,
        "\n- Request while unfocused: {} ({came_back})\n- Request while focused: {}\n",
        flash.unfocused, flash.focused
    );
    text
}

/// The report beside the probe's executable.
fn report_path() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("taskbar-probe-report.md")))
}

/// How many runs the report already holds, so a new run takes the next
/// number.
fn previous_runs() -> u32 {
    report_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or(0, |text| {
            let runs = text
                .lines()
                .filter(|l| l.starts_with("## Taskbar probe, run"))
                .count();
            u32::try_from(runs).unwrap_or(0)
        })
}

fn save(text: &str) -> String {
    let path = report_path();
    let Some(path) = path else {
        return "the report could not be saved (no executable path)".to_string();
    };
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| writeln!(file, "{text}"));
    match written {
        Ok(()) => format!("appended to {}", path.display()),
        Err(e) => format!("the report could not be saved to {}: {e}", path.display()),
    }
}

fn run(probe: &Probe) -> Option<()> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(Signal::Loaded) = probe.signals.recv_timeout(Duration::from_millis(200)) {
            break;
        }
    }
    for run in previous_runs() + 1.. {
        let theme = probe.ask(
            "Officina taskbar probe",
            "<p>The probe shows the waiting badge and the alert badge on its own taskbar \
             button, at the size the window uses at this display scale, then tests the \
             attention flash. Answer each step with what you see; the \
             report is written at the end.</p><p>Which taskbar theme is on now?</p>",
            &[("light", "Light taskbar"), ("dark", "Dark taskbar")],
        )?;
        let ratings = rate_badges(probe)?;
        let flash = test_flash(probe)?;
        let text = report(
            run,
            theme,
            probe.window.scale_factor().ok(),
            &ratings,
            &flash,
        );
        println!("{text}");
        let saved = save(&text);
        println!("{saved}");
        let shown = text.replace('&', "&amp;").replace('<', "&lt;");
        let again = probe.ask(
            "Done",
            &format!("<p>Report {saved}.</p><pre>{shown}</pre>"),
            &[
                ("again", "Run again (other taskbar theme)"),
                ("close", "Close"),
            ],
        )?;
        if again == "Close" {
            break;
        }
    }
    Some(())
}

fn main() {
    let mut context = tauri::generate_context!();
    // The window of the app's configuration would load the board; the probe
    // opens its own blank one instead.
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .setup(|app| {
            let (tx, signals) = mpsc::channel();
            let answers = tx.clone();
            let url = WebviewUrl::External("about:blank".parse()?);
            let window = WebviewWindowBuilder::new(app, "probe", url)
                .title("Officina taskbar probe")
                .inner_size(560.0, 420.0)
                .on_navigation(move |url| {
                    if url.host_str() != Some(ANSWER_HOST) {
                        return true;
                    }
                    if let Some((_, key)) = url.query_pairs().find(|(k, _)| k == "a") {
                        let _ = answers.send(Signal::Answer(key.into_owned()));
                    }
                    false
                })
                .on_page_load(move |_, _| {
                    let _ = tx.send(Signal::Loaded);
                })
                .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let probe = Probe { window, signals };
                let _ = run(&probe);
                handle.exit(0);
            });
            Ok(())
        })
        .run(context)
        .expect("failed to run the taskbar probe");
}
