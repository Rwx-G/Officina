//! The taskbar probe, gate 2 of AD-178 (story 4.1 AC 2): shows the overlay
//! badge the window will use, for 1, 9 and above nine, amber then in the
//! alert colour, at each probed size, five seconds each; then requests the
//! user's attention once while another window has focus, and once while the
//! probe has it. Run by the owner, by hand, at 100 % and 150 % display
//! scaling with a light and a dark taskbar; never by a test, never part of
//! a shipped binary.
//!
//! Usage: `cargo run -p officina-app --example taskbar_probe`. It opens one
//! blank window, with no collector, no single-instance lock and no floor:
//! it reads and writes nothing under the Officina home or the Claude config
//! dir. It prints what it shows as it shows it.

#[path = "../src/taskbar.rs"]
mod taskbar;

use std::thread::sleep;
use std::time::Duration;

use tauri::{UserAttentionType, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// How long each badge stays.
const SHOW: Duration = Duration::from_secs(5);
/// The time the owner has to focus another window before the flash.
const FOCUS_ELSEWHERE: Duration = Duration::from_secs(5);
/// How long the flash is watched before the probe takes focus back.
const WATCH_FLASH: Duration = Duration::from_secs(15);

fn badge(window: &WebviewWindow, count: u32, alert: bool, size: u32) {
    let image = taskbar::overlay_badge(count, alert, size);
    let shown = tauri::image::Image::new_owned(image.rgba, image.size, image.size);
    if let Err(e) = window.set_overlay_icon(Some(shown)) {
        eprintln!("taskbar_probe: the overlay could not be set: {e}");
    }
}

fn run(window: &WebviewWindow) {
    for size in taskbar::OVERLAY_SIZES {
        for alert in [false, true] {
            for count in [1, taskbar::MAX_SPELLED, taskbar::MAX_SPELLED + 1] {
                println!(
                    "{size} px, {}, \"{}\"",
                    if alert { "alert colour" } else { "amber" },
                    taskbar::badge_text(count)
                );
                badge(window, count, alert, size);
                sleep(SHOW);
            }
        }
    }
    let _ = window.set_overlay_icon(None);
    println!(
        "Focus another window now: the attention request comes in {} s.",
        FOCUS_ELSEWHERE.as_secs()
    );
    sleep(FOCUS_ELSEWHERE);
    println!("request_user_attention(Informational) while unfocused: the button should flash");
    let _ = window.request_user_attention(Some(UserAttentionType::Informational));
    sleep(WATCH_FLASH);
    println!("Focusing the probe: the flash should stop.");
    let _ = window.set_focus();
    sleep(Duration::from_secs(2));
    println!("request_user_attention(Informational) while focused: nothing should happen");
    let _ = window.request_user_attention(Some(UserAttentionType::Informational));
    sleep(SHOW);
    println!("Done.");
}

fn main() {
    let mut context = tauri::generate_context!();
    // The window of the app's configuration would load the board; the probe
    // opens its own blank one instead.
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .setup(|app| {
            let url = WebviewUrl::External("about:blank".parse()?);
            let window = WebviewWindowBuilder::new(app, "probe", url)
                .title("Officina taskbar probe")
                .inner_size(420.0, 200.0)
                .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                run(&window);
                handle.exit(0);
            });
            Ok(())
        })
        .run(context)
        .expect("failed to run the taskbar probe");
}
