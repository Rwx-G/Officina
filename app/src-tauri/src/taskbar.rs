//! The taskbar overlay badge, the window title and the attention request
//! (FR-11, AD-174 as amended by gate 2, D-523): an amber disc carrying the
//! waiting count, or a disc in the alert colour carrying an exclamation
//! mark, so the alert never rests on colour alone; drawn in code so no
//! asset can drift from the palette. Pure: the window sets the image, the
//! title and the flash; the tests compare pixels and words without one.

use std::collections::BTreeSet;

use officina_core::board::Board;
use officina_core::scene::Zone;

/// The overlay's side at 100 % display scaling: Windows draws the overlay
/// at the small-icon size, so a larger image is only scaled down and blurs
/// (gate 2, D-523).
pub const BASE_SIZE: u32 = 16;

/// The overlay's side in pixels for a window at `scale_factor`: the small
/// icon's size at that scaling (16 px at 100 %, 24 px at 150 %).
pub fn overlay_size(scale_factor: f64) -> u32 {
    let scaled = (f64::from(BASE_SIZE) * scale_factor).round();
    if scaled.is_finite() && scaled > f64::from(BASE_SIZE) {
        // A display scale stays far below u32::MAX pixels.
        scaled as u32
    } else {
        BASE_SIZE
    }
}

/// The disc's fill while someone waits: the light `--color-attention`.
pub const AMBER: [u8; 3] = [0xF5, 0xB7, 0x31];
/// The count on amber: the light `--color-attention-text`.
pub const AMBER_TEXT: [u8; 3] = [0x2A, 0x1F, 0x05];
/// The disc's fill while anyone is in alert: the prototype v2's light
/// `--color-alert`.
pub const ALERT: [u8; 3] = [0xB0, 0x3A, 0x22];
/// The mark on the alert colour: the prototype v2's light `--color-on-alert`.
pub const ALERT_TEXT: [u8; 3] = [0xFF, 0xFF, 0xFF];
/// The rim around the disc, today's dot's: the light `--color-text`.
pub const RIM: [u8; 3] = [0x2A, 0x24, 0x1C];

/// The small `+` beside a `9` above nine: 3 by 3 cells, a row per byte,
/// centred on the digit's height, one cell after it.
const PLUS: [u8; 3] = [0b010, 0b111, 0b010];

/// The highest count the badge spells; above it, it reads `9+`.
pub const MAX_SPELLED: u32 = 9;

/// Bold glyphs 5 cells wide and 7 high, one row per byte, the leftmost cell
/// the highest of the five bits (the owner's choice on gate 2's run 2,
/// D-523 as amended).
const GLYPH_W: u32 = 5;
const GLYPH_H: u32 = 7;

fn glyph(c: char) -> [u8; 7] {
    match c {
        '0' => [
            0b01110, 0b11011, 0b11011, 0b11011, 0b11011, 0b11011, 0b01110,
        ],
        '1' => [
            0b00110, 0b01110, 0b00110, 0b00110, 0b00110, 0b00110, 0b01111,
        ],
        '2' => [
            0b01110, 0b11011, 0b00011, 0b00110, 0b01100, 0b11000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00011, 0b00011, 0b01110, 0b00011, 0b00011, 0b11110,
        ],
        '4' => [
            0b00110, 0b01110, 0b11110, 0b10110, 0b11111, 0b00110, 0b00110,
        ],
        '5' => [
            0b11111, 0b11000, 0b11110, 0b00011, 0b00011, 0b11011, 0b01110,
        ],
        '6' => [
            0b01110, 0b11000, 0b11000, 0b11110, 0b11011, 0b11011, 0b01110,
        ],
        '7' => [
            0b11111, 0b00011, 0b00110, 0b00110, 0b01100, 0b01100, 0b01100,
        ],
        '8' => [
            0b01110, 0b11011, 0b11011, 0b01110, 0b11011, 0b11011, 0b01110,
        ],
        '9' => [
            0b01110, 0b11011, 0b11011, 0b01111, 0b00011, 0b00110, 0b01100,
        ],
        _ => [0; 7],
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

/// `value` scaled from [`BASE_SIZE`] to `size`, rounded, at least 1.
fn scaled(value: u32, size: u32) -> u32 {
    ((value * size + BASE_SIZE / 2) / BASE_SIZE).max(1)
}

/// Paints the cells set in `rows` (`width` bits each, the leftmost first)
/// at `(x0, y0)`, each cell `cell` pixels square, in `ink`.
fn paint(image: &mut Overlay, rows: &[u8], width: u32, x0: u32, y0: u32, cell: u32, ink: [u8; 3]) {
    for (row, bits) in rows.iter().enumerate() {
        for column in 0..width {
            if bits & (1 << (width - 1 - column)) == 0 {
                continue;
            }
            for dy in 0..cell {
                for dx in 0..cell {
                    let (x, y) = (x0 + column * cell + dx, y0 + row as u32 * cell + dy);
                    if x < image.size && y < image.size {
                        let at = ((y * image.size + x) * 4) as usize;
                        image.rgba[at..at + 4].copy_from_slice(&opaque(ink));
                    }
                }
            }
        }
    }
}

/// Fills the rectangle `(x0, y0, w, h)` with `ink`.
fn fill(image: &mut Overlay, x0: u32, y0: u32, w: u32, h: u32, ink: [u8; 3]) {
    for y in y0..(y0 + h).min(image.size) {
        for x in x0..(x0 + w).min(image.size) {
            let at = ((y * image.size + x) * 4) as usize;
            image.rgba[at..at + 4].copy_from_slice(&opaque(ink));
        }
    }
}

/// The rim's width: one pixel at 16 px.
fn rim(size: u32) -> u32 {
    (size / BASE_SIZE).max(1)
}

/// The disc of `size` pixels in `body`, inside a one-pixel rim (at 16 px).
fn disc(size: u32, body: [u8; 3]) -> Overlay {
    let center = (size as f32 - 1.0) / 2.0;
    let outer = size as f32 / 2.0 - 0.5;
    let inner = outer - rim(size) as f32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            let pixel = if d <= inner {
                opaque(body)
            } else if d <= outer {
                opaque(RIM)
            } else {
                [0, 0, 0, 0]
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    Overlay { size, rgba }
}

/// The badge at `size` pixels: with `alert`, a disc in the alert colour
/// carrying a bold `!` (the count stays in the title and the band); else an
/// amber disc carrying `count` up to [`MAX_SPELLED`] in a bold 5 x 7 font,
/// one pixel per cell at 16 px, and above nine a small `+` beside the `9`
/// (gate 2, D-523 as amended).
pub fn overlay_badge(count: u32, alert: bool, size: u32) -> Overlay {
    if alert {
        let mut image = disc(size, ALERT);
        let (stroke, bar, gap) = (scaled(2, size), scaled(7, size), scaled(1, size));
        let height = bar + gap + stroke;
        let (x0, y0) = ((size - stroke) / 2, (size - height) / 2);
        fill(&mut image, x0, y0, stroke, bar, ALERT_TEXT);
        fill(&mut image, x0, y0 + bar + gap, stroke, stroke, ALERT_TEXT);
        return image;
    }
    let mut image = disc(size, AMBER);
    let digit = char::from_digit(count.min(MAX_SPELLED), 10).unwrap_or('9');
    let plus = count > MAX_SPELLED;
    // One cell per pixel at 16 px, scaled with the size, or the largest
    // cell below it whose text keeps its corners inside the rim.
    let center = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 / 2.0 - 0.5 - rim(size) as f32;
    let layout = |cell: u32| {
        let plus_cell = if plus { cell } else { 0 };
        let width = GLYPH_W * cell + if plus { 4 * plus_cell } else { 0 };
        let height = GLYPH_H * cell;
        let (x0, y0) = ((size - width.min(size)) / 2, (size - height.min(size)) / 2);
        let corners = [
            (x0, y0),
            (x0 + width - 1, y0),
            (x0, y0 + height - 1),
            (x0 + width - 1, y0 + height - 1),
        ];
        let fits = corners.iter().all(|&(x, y)| {
            ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt() <= radius
        });
        (fits, x0, y0, plus_cell)
    };
    let cell = (1..=scaled(1, size))
        .rev()
        .find(|&c| layout(c).0)
        .unwrap_or(1);
    let (_, x0, y0, plus_cell) = layout(cell);
    paint(&mut image, &glyph(digit), GLYPH_W, x0, y0, cell, AMBER_TEXT);
    if plus {
        paint(
            &mut image,
            &PLUS,
            3,
            x0 + GLYPH_W * cell + plus_cell,
            y0 + 2 * plus_cell,
            plus_cell,
            AMBER_TEXT,
        );
    }
    image
}

/// The overlay for `board` at `size`: none when nothing waits and nobody
/// is in alert; else the waiting badge with its count, or the alert badge
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

    /// The display scales Windows offers most, as overlay sizes.
    fn sizes() -> Vec<u32> {
        [1.0, 1.25, 1.5, 1.75, 2.0]
            .into_iter()
            .map(overlay_size)
            .collect()
    }

    /// The bounding box `(x, y, w, h)` of the pixels painted in `rgb`.
    fn bbox(image: &Overlay, rgb: [u8; 3]) -> (u32, u32, u32, u32) {
        let painted: Vec<(u32, u32)> = (0..image.size)
            .flat_map(|y| (0..image.size).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let at = ((y * image.size + x) * 4) as usize;
                image.rgba[at..at + 4] == opaque(rgb)
            })
            .collect();
        let xs = painted.iter().map(|p| p.0);
        let ys = painted.iter().map(|p| p.1);
        let (x0, x1) = (xs.clone().min().unwrap(), xs.max().unwrap());
        let (y0, y1) = (ys.clone().min().unwrap(), ys.max().unwrap());
        (x0, y0, x1 - x0 + 1, y1 - y0 + 1)
    }

    /// D-523: the overlay is the small icon's size at the window's scale.
    #[test]
    fn the_overlay_size_follows_the_display_scale() {
        assert_eq!(overlay_size(1.0), BASE_SIZE);
        assert_eq!(overlay_size(1.5), BASE_SIZE * 3 / 2);
        assert_eq!(overlay_size(2.0), BASE_SIZE * 2);
        assert_eq!(overlay_size(1.25), 20);
        for odd in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(overlay_size(odd), BASE_SIZE, "{odd}");
        }
    }

    /// D-523 as amended: at 16 px each digit is its glyph, one pixel per
    /// cell, every lit cell drawn; 9+ is the 9 and the whole `+`; every
    /// badge's ink stays inside the rim at every scale.
    #[test]
    fn digits_are_drawn_in_the_bold_font_inside_the_rim() {
        let lit = |rows: &[u8]| rows.iter().map(|r| r.count_ones() as usize).sum::<usize>();
        for count in 1..=MAX_SPELLED {
            let image = overlay_badge(count, false, BASE_SIZE);
            let digit = char::from_digit(count, 10).unwrap();
            assert_eq!(count_of(&image, AMBER_TEXT), lit(&glyph(digit)), "{count}");
            let (_, _, w, h) = bbox(&image, AMBER_TEXT);
            assert!(w <= GLYPH_W && h == GLYPH_H, "{count}: {w} x {h}");
        }
        let nine_plus = overlay_badge(MAX_SPELLED + 1, false, BASE_SIZE);
        assert_eq!(
            count_of(&nine_plus, AMBER_TEXT),
            lit(&glyph('9')) + lit(&PLUS),
            "the + is drawn whole"
        );
        let (_, _, w, _) = bbox(&nine_plus, AMBER_TEXT);
        assert_eq!(w, GLYPH_W + 1 + 3);
        for size in sizes() {
            for count in [1, MAX_SPELLED, MAX_SPELLED + 1] {
                for alert in [false, true] {
                    let image = overlay_badge(count, alert, size);
                    let ink = if alert { ALERT_TEXT } else { AMBER_TEXT };
                    let (x, y, w, h) = bbox(&image, ink);
                    let center = (size as f32 - 1.0) / 2.0;
                    let radius = size as f32 / 2.0 - 0.5 - rim(size) as f32;
                    for (cx, cy) in [
                        (x, y),
                        (x + w - 1, y),
                        (x, y + h - 1),
                        (x + w - 1, y + h - 1),
                    ] {
                        let d =
                            ((cx as f32 - center).powi(2) + (cy as f32 - center).powi(2)).sqrt();
                        assert!(
                            d <= radius,
                            "{count} {alert} at {size}: ink outside the disc"
                        );
                    }
                }
            }
        }
    }

    /// D-523: the alert badge carries a bold mark, the same whatever the
    /// count, two pixels wide and ten high at 16 px, so the alert never
    /// rests on colour alone.
    #[test]
    fn the_alert_badge_shows_a_mark_not_the_count() {
        for size in sizes() {
            assert_eq!(
                overlay_badge(1, true, size),
                overlay_badge(7, true, size),
                "{size}"
            );
            assert_ne!(
                overlay_badge(1, true, size),
                overlay_badge(1, false, size),
                "{size}"
            );
        }
        let (_, _, w, h) = bbox(&overlay_badge(3, true, BASE_SIZE), ALERT_TEXT);
        assert_eq!((w, h), (2, 10));
    }

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
        for size in sizes() {
            assert_eq!(overlay_for(&board(0, 0), size), None);
            assert_eq!(
                overlay_for(&board(3, 0), size),
                Some(overlay_badge(3, false, size))
            );
        }
    }

    /// Story 5.18 AC 9 (D-565, review F40): the orchestrator, waiting or
    /// not, leaves the title and the overlay as the author's waits make
    /// them; proposals are no board input, so they cannot reach either.
    #[test]
    fn the_orchestrator_leaves_the_title_and_the_overlay_unchanged() {
        let home = r"C:\Users\user01\.officina";
        for waiting in [0, 2] {
            let plain = board(waiting, 0);
            for status in ["busy", "waiting", "idle"] {
                let mut agents: Vec<serde_json::Value> = (0..waiting)
                    .map(|i| {
                        serde_json::json!({"pid": 100 + i, "sessionId": format!("s{i}"), "kind": "interactive",
                            "status": "waiting", "cwd": "C:/work/calm", "startedAt": 1})
                    })
                    .collect();
                agents.push(serde_json::json!({"pid": 9, "sessionId": "orch", "name": "officina-orchestrator",
                    "kind": "interactive", "status": status, "cwd": format!("{home}/orchestrator/workdir"), "startedAt": 1}));
                let with = build_board(
                    &BoardInput {
                        officina_home: Some(home.to_owned()),
                        ..input(&agents)
                    },
                    NOW,
                );
                assert!(with.orchestrator.is_some(), "{status}");
                assert_eq!(window_title(&with), window_title(&plain), "{status}");
                for size in sizes() {
                    assert_eq!(
                        overlay_for(&with, size),
                        overlay_for(&plain, size),
                        "{status}"
                    );
                }
            }
        }
    }

    /// Story 6.20 AC 2 (AD-55, D-618): live workshop runs leave the title
    /// and the overlay as the author's waits make them; the live-run count
    /// is the Orchestrator view switch's only.
    #[test]
    fn the_window_title_never_counts_workshop_runs() {
        use officina_core::workshop::run::RunState;
        use officina_core::workshop::view::BoardRun;
        let live: Vec<BoardRun> = RunState::ALL
            .iter()
            .copied()
            .filter(|state| state.is_live())
            .enumerate()
            .map(|(i, state)| BoardRun {
                id: format!("{i:032x}"),
                template: "bump".into(),
                project: "calm".into(),
                name: format!("bump-{i}"),
                state,
                state_at_ms: NOW,
                started_at_ms: NOW,
            })
            .collect();
        assert!(!live.is_empty());
        for waiting in [0, 2] {
            let plain = board(waiting, 0);
            let agents: Vec<serde_json::Value> = (0..waiting)
                .map(|i| {
                    serde_json::json!({"pid": 100 + i, "sessionId": format!("s{i}"), "kind": "interactive",
                        "status": "waiting", "cwd": "C:/work/calm", "startedAt": 1})
                })
                .collect();
            let with = build_board(
                &BoardInput {
                    workshop_runs: live.clone(),
                    ..input(&agents)
                },
                NOW,
            );
            assert_eq!(with.workshop_live as usize, live.len());
            assert_eq!(window_title(&with), window_title(&plain));
            for size in sizes() {
                assert_eq!(overlay_for(&with, size), overlay_for(&plain, size));
            }
        }
    }

    #[test]
    fn the_badge_turns_to_the_alert_colour() {
        for size in sizes() {
            assert_eq!(
                overlay_for(&board(4, 1), size),
                Some(overlay_badge(4, true, size))
            );
        }
    }

    /// D-494: a stale board keeps the alert badge, and the title says the
    /// alert is the last one known.
    #[test]
    fn a_last_known_alert_keeps_the_alert_colour() {
        let alerted = board(2, 1);
        let stale = stale_after(&alerted, 2, 1);
        assert_eq!(stale.alert_count, 1);
        for size in sizes() {
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
    /// draws its own text in the ink colour at every display scale.
    #[test]
    fn overlay_badge_draws_each_count() {
        for size in sizes() {
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

    /// AD-174: the alert badge fills the disc with the alert colour and its
    /// mark, and no amber is left.
    #[test]
    fn overlay_badge_uses_the_alert_colour() {
        for size in sizes() {
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
        for size in sizes() {
            assert_eq!(
                overlay_badge(MAX_SPELLED + 1, false, size),
                overlay_badge(MAX_SPELLED + 40, false, size)
            );
        }
    }
}
