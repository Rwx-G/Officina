//! The taskbar overlay badge (FR-11, AD-174): a disc carrying the waiting
//! count, drawn in code so no asset can drift from the palette. Pure: the
//! window sets the image, the tests compare pixels without one.

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

#[cfg(test)]
mod tests {
    use super::*;

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
