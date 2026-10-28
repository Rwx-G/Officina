//! The taskbar overlay badge, the window title and the attention request
//! (FR-11, AD-174): a disc carrying the waiting count, drawn in code so no
//! asset can drift from the palette, amber or in the alert colour. Pure:
//! the window sets the image, the title and the flash; the tests compare
//! pixels and words without one.

use std::collections::BTreeSet;

use officina_core::board::Board;
use officina_core::scene::Zone;

/// The sizes the Slice 0 probe shows (AD-178 gate 2); the window uses the
/// one the gate finds legible.
pub const OVERLAY_SIZES: [u32; 3] = [16, 24, 32];

/// The disc's fill while someone waits: the light `--color-attention`.
pub const AMBER: [u8; 3] = [0xF5, 0xB7, 0x31];
/// The count on amber: the light `--color-attention-text`.
pub const AMBER_TEXT: [u8; 3] = [0x2A, 0x1F, 0x05];
/// The disc's fill while anyone is in alert: the prototype v2's light
/// `--color-alert`.
pub const ALERT: [u8; 3] = [0xB0, 0x3A, 0x22];
/// The count on the alert colour: the prototype v2's light `--color-on-alert`.
pub const ALERT_TEXT: [u8; 3] = [0xFF, 0xFF, 0xFF];
/// The rim around the disc, today's dot's: the light `--color-text`.
pub const RIM: [u8; 3] = [0x2A, 0x24, 0x1C];

/// The highest count the badge spells; above it, it reads `9+`.
pub const MAX_SPELLED: u32 = 9;

/// Glyphs 3 pixels wide and 5 high, one row per byte, bit 2 leftmost.
const GLYPH_W: u32 = 3;
const GLYPH_H: u32 = 5;

fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b011, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => [0; 5],
    }
}

/// What the badge spells for `count`: the count up to [`MAX_SPELLED`],
/// then `9+`.
pub fn badge_text(count: u32) -> String {
    if count > MAX_SPELLED {
        format!("{MAX_SPELLED}+")
    } else {
        count.to_string()
    }
}

/// A square RGBA image, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub size: u32,
    pub rgba: Vec<u8>,
}

fn opaque(rgb: [u8; 3]) -> [u8; 4] {
    [rgb[0], rgb[1], rgb[2], 0xFF]
}

/// The badge for `count` waiting at `size` pixels: a disc of amber, or of
/// the alert colour when `alert`, inside a dark rim, with [`badge_text`] in
/// a bitmap font scaled to the size. The rim is an eighth of the size, as
/// today's 16 px dot's two pixels.
pub fn overlay_badge(count: u32, alert: bool, size: u32) -> Overlay {
    let (fill, ink) = if alert {
        (ALERT, ALERT_TEXT)
    } else {
        (AMBER, AMBER_TEXT)
    };
    let center = (size as f32 - 1.0) / 2.0;
    let outer = size as f32 / 2.0 - 0.5;
    let inner = outer - (size / 8).max(1) as f32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            let pixel = if d <= inner {
                opaque(fill)
            } else if d <= outer {
                opaque(RIM)
            } else {
                [0, 0, 0, 0]
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    let mut image = Overlay { size, rgba };
    let text: Vec<char> = badge_text(count).chars().collect();
    let glyphs = text.len() as u32;
    let scale = (size.saturating_sub(4) / 10).max(1);
    let width = (glyphs * GLYPH_W + glyphs.saturating_sub(1)) * scale;
    let height = GLYPH_H * scale;
    let (x0, y0) = (
        size.saturating_sub(width) / 2,
        size.saturating_sub(height) / 2,
    );
    for (i, c) in text.iter().enumerate() {
        let left = x0 + i as u32 * (GLYPH_W + 1) * scale;
        for (row, bits) in glyph(*c).iter().enumerate() {
            for column in 0..GLYPH_W {
                if bits & (0b100 >> column) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let (x, y) = (left + column * scale + dx, y0 + row as u32 * scale + dy);
                        if x < size && y < size {
                            let at = ((y * size + x) * 4) as usize;
                            image.rgba[at..at + 4].copy_from_slice(&opaque(ink));
                        }
                    }
                }
            }
        }
    }
    image
}

/// The overlay for `board` at `size`: none when nothing waits and nobody
/// is in alert; else the badge with the waiting count, in the alert colour
/// while any card is in alert, a last known alert on unknown data included
/// (D-494).
pub fn overlay_for(board: &Board, size: u32) -> Option<Overlay> {
    let alert = board.alert_count > 0;
    (board.waiting_count > 0 || alert)
        .then(|| overlay_badge(board.waiting_count.max(board.alert_count), alert, size))
}

/// True when an alerted card's zone is unknown: its alert is the last one
/// known, kept while the data cannot say more (D-494).
fn alert_on_unknown_data(board: &Board) -> bool {
    board
        .columns
        .all()
        .filter_map(|card| card.scene)
        .any(|scene| scene.alert.is_some() && scene.zone == Zone::Unknown)
}

/// The window's title, which Windows also shows on the taskbar button:
/// `Officina`, then what waits, what is in alert and, while an alert rests
/// on unknown data, that it does (AD-174 as amended, D-494).
pub fn window_title(board: &Board) -> String {
    let mut parts = Vec::new();
    if board.waiting_count > 0 {
        parts.push(format!("{} waiting", board.waiting_count));
    }
    if board.alert_count > 0 {
        parts.push(format!("{} in alert", board.alert_count));
        if alert_on_unknown_data(board) {
            parts.push("data unknown".to_owned());
        }
    }
    if parts.is_empty() {
        "Officina".to_owned()
    } else {
        format!("Officina ({})", parts.join(", "))
    }
}

/// The keys of the cards in alert on `board`, unknown ones included.
pub fn alerted_keys(board: &Board) -> BTreeSet<String> {
    board
        .columns
        .all()
        .filter(|card| card.scene.is_some_and(|s| s.alert.is_some()))
        .map(|card| card.key.clone())
        .collect()
}

/// True when `board` holds an alerted key that `previous`, the alerted keys
/// of the last board emitted, did not: the taskbar button flashes once for
/// it (AD-174, D-475). A key kept alerted, unknown or not, never again.
pub fn alerts_to_announce(previous: &BTreeSet<String>, board: &Board) -> bool {
    alerted_keys(board)
        .iter()
        .any(|key| !previous.contains(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::agents::parse_agents;
    use officina_core::board::{BoardInput, ProjectRule, Staleness, build_board};
    use officina_core::floor::Priority;

    const NOW: i64 = 1_793_102_400_000;

    /// `waiting` sessions waiting with no known start; the first `alerted`
    /// of them on a high-priority project, so in alert.
    fn board(waiting: u32, alerted: u32) -> Board {
        let agents: Vec<serde_json::Value> = (0..waiting)
            .map(|i| {
                let project = if i < alerted { "hot" } else { "calm" };
                serde_json::json!({"pid": 100 + i, "sessionId": format!("s{i}"), "kind": "interactive",
                    "status": "waiting", "cwd": format!("C:/work/{project}"), "startedAt": 1})
            })
            .collect();
        build_board(&input(&agents), NOW)
    }

    fn input(agents: &[serde_json::Value]) -> BoardInput {
        BoardInput {
            agents: Some(parse_agents(&serde_json::to_string(agents).unwrap()).unwrap()),
            projects: ["hot", "calm"]
                .into_iter()
                .map(|name| ProjectRule {
                    name: name.into(),
                    path: format!("c:/work/{name}"),
                    color: None,
                    priority: if name == "hot" {
                        Priority::High
                    } else {
                        Priority::Normal
                    },
                })
                .collect(),
            ..Default::default()
        }
    }

    /// The same sessions, the agents snapshot failing: every card unknown,
    /// the previous board's alerts kept.
    fn stale_after(previous: &Board, waiting: u32, alerted: u32) -> Board {
        let fresh = board(waiting, alerted);
        let agents: Vec<serde_json::Value> = (0..waiting)
            .map(|i| {
                let project = if i < alerted { "hot" } else { "calm" };
                serde_json::json!({"pid": 100 + i, "sessionId": format!("s{i}"), "kind": "interactive",
                    "status": "waiting", "cwd": format!("C:/work/{project}"), "startedAt": 1})
            })
            .collect();
        let stale = BoardInput {
            stale: Some(Staleness {
                since_ms: NOW,
                error: "claude agents failed".into(),
            }),
            previous_scene: officina_core::scene::states_by_key(&previous.columns),
            ..input(&agents)
        };
        let board = build_board(&stale, NOW + 1_000);
        assert_eq!(board.waiting_count, fresh.waiting_count);
        board
    }

    #[test]
    fn no_overlay_without_waiting() {
        for size in OVERLAY_SIZES {
            assert_eq!(overlay_for(&board(0, 0), size), None);
            assert_eq!(
                overlay_for(&board(3, 0), size),
                Some(overlay_badge(3, false, size))
            );
        }
    }

    #[test]
    fn the_badge_turns_to_the_alert_colour() {
        for size in OVERLAY_SIZES {
            assert_eq!(
                overlay_for(&board(4, 1), size),
                Some(overlay_badge(4, true, size))
            );
        }
    }

    /// D-494: a stale board keeps the alert colour, and the title says the
    /// alert is the last one known.
    #[test]
    fn a_last_known_alert_keeps_the_alert_colour() {
        let alerted = board(2, 1);
        let stale = stale_after(&alerted, 2, 1);
        assert_eq!(stale.alert_count, 1);
        for size in OVERLAY_SIZES {
            assert_eq!(
                overlay_for(&stale, size),
                Some(overlay_badge(2, true, size))
            );
        }
    }

    #[test]
    fn the_title_counts_alerts() {
        assert_eq!(window_title(&board(0, 0)), "Officina");
        assert_eq!(window_title(&board(3, 0)), "Officina (3 waiting)");
        assert_eq!(
            window_title(&board(3, 2)),
            "Officina (3 waiting, 2 in alert)"
        );
    }

    #[test]
    fn the_title_marks_a_last_known_alert() {
        let stale = stale_after(&board(3, 2), 3, 2);
        assert_eq!(
            window_title(&stale),
            "Officina (3 waiting, 2 in alert, data unknown)"
        );
    }

    #[test]
    fn a_new_alerted_key_is_announced() {
        let calm = board(2, 0);
        let alerted = board(2, 1);
        assert!(alerts_to_announce(&alerted_keys(&calm), &alerted));
    }

    #[test]
    fn a_known_key_is_not_announced_again() {
        let alerted = board(2, 1);
        assert!(!alerts_to_announce(&alerted_keys(&alerted), &board(3, 1)));
    }

    #[test]
    fn a_key_that_leaves_and_returns_is_announced_again() {
        let alerted = board(2, 1);
        let gone = board(0, 0);
        assert!(!alerts_to_announce(&alerted_keys(&alerted), &gone));
        assert!(alerts_to_announce(&alerted_keys(&gone), &alerted));
    }

    #[test]
    fn the_first_board_with_alerts_is_announced() {
        assert!(alerts_to_announce(&BTreeSet::new(), &board(2, 1)));
        assert!(!alerts_to_announce(&BTreeSet::new(), &board(2, 0)));
    }

    #[test]
    fn a_key_kept_alerted_while_unknown_is_not_announced() {
        let alerted = board(2, 1);
        let stale = stale_after(&alerted, 2, 1);
        assert!(!alerts_to_announce(&alerted_keys(&alerted), &stale));
    }

    /// The custom properties of the light theme's `:root` block of
    /// `app.css`, the only committed copy of the tokens.
    fn light_token(name: &str) -> [u8; 3] {
        let css = include_str!("../../src/app.css");
        let root = &css[css.find(":root {").expect(":root")..];
        let root = &root[..root.find("\n}").expect("end of :root")];
        let at = root
            .find(&format!("{name}:"))
            .unwrap_or_else(|| panic!("{name} in app.css"));
        let value = root[at + name.len() + 1..].trim_start();
        let hex = value.trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        [byte(0), byte(2), byte(4)]
    }

    /// AD-174, `derived-not-transcribed`: the overlay's colours are the
    /// light theme's tokens.
    #[test]
    fn overlay_colours_equal_the_light_tokens() {
        assert_eq!(AMBER, light_token("--color-attention"));
        assert_eq!(AMBER_TEXT, light_token("--color-attention-text"));
        assert_eq!(ALERT, light_token("--color-alert"));
        assert_eq!(ALERT_TEXT, light_token("--color-on-alert"));
        assert_eq!(RIM, light_token("--color-text"));
    }

    fn count_of(image: &Overlay, rgb: [u8; 3]) -> usize {
        image
            .rgba
            .chunks(4)
            .filter(|p| *p == opaque(rgb).as_slice())
            .count()
    }

    /// AD-174: every count from 1 to the last spelled one, and above it,
    /// draws its own text in the ink colour at every probed size, inside the
    /// disc.
    #[test]
    fn overlay_badge_draws_each_count() {
        for size in OVERLAY_SIZES {
            let mut seen = Vec::new();
            for count in 1..=MAX_SPELLED + 1 {
                let image = overlay_badge(count, false, size);
                assert_eq!(image.rgba.len(), (size * size * 4) as usize);
                assert!(count_of(&image, AMBER_TEXT) > 0, "{count} at {size}");
                assert!(count_of(&image, AMBER) > 0, "{count} at {size}");
                assert_eq!(image.rgba[3], 0, "the corner is transparent");
                assert!(!seen.contains(&image), "{count} at {size} repeats a count");
                seen.push(image);
            }
        }
    }

    /// AD-174: the alert state fills the disc with the alert colour and its
    /// ink, and no amber is left.
    #[test]
    fn overlay_badge_uses_the_alert_colour() {
        for size in OVERLAY_SIZES {
            let image = overlay_badge(3, true, size);
            assert!(count_of(&image, ALERT) > 0, "{size}");
            assert!(count_of(&image, ALERT_TEXT) > 0, "{size}");
            assert_eq!(count_of(&image, AMBER), 0, "{size}");
            assert!(count_of(&image, RIM) > 0, "{size}");
            assert_ne!(image, overlay_badge(3, false, size));
        }
    }

    /// FR-11: above the last spelled count the badge reads `9+`, the same
    /// image whatever the count.
    #[test]
    fn nine_plus_above_nine() {
        assert_eq!(badge_text(MAX_SPELLED), MAX_SPELLED.to_string());
        assert_eq!(badge_text(MAX_SPELLED + 1), format!("{MAX_SPELLED}+"));
        for size in OVERLAY_SIZES {
            assert_eq!(
                overlay_badge(MAX_SPELLED + 1, false, size),
                overlay_badge(MAX_SPELLED + 40, false, size)
            );
        }
    }
}
