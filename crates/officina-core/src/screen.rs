//! A minimal terminal screen (story 5.10, D-1020): `claude logs` prints a
//! background session's output as Claude Code painted it, a full repaint
//! from the top of the screen for every frame (E100), so removing the escape
//! sequences alone left each frame after the one before. [`render`] plays
//! the output on a screen instead and keeps what the screen shows at its
//! end.
//!
//! The output does not say the screen's size. Rows grow downward as the
//! output writes them, up to [`MAX_ROWS`], and a row wraps only past
//! [`MAX_COLUMNS`]: Claude Code lays out its own lines, so a guessed
//! narrower width would break every longer one, while a wider one at worst
//! leaves a row's tail on the full-width row before it. Every character
//! takes one column: a terminal gives a wide character two and a
//! combining one none, which only shifts a later cursor move on the same
//! row, and the repaints E100 captured write each row from its start.
//!
//! A line feed also returns to the row's start, as a terminal in newline
//! mode does: the capture puts a carriage return before every line feed,
//! so this changes nothing in it, and an output written with bare line
//! feeds still reads as lines.
//!
//! Interpreted: carriage return, line feed (and vertical tab, form feed),
//! backspace, tab; cursor position and movement (CUP, HVP, CUU, CUD, CUF,
//! CUB, CNL, CPL, CHA, HPA, HPR, VPA, VPR), save and restore (`ESC 7`,
//! `ESC 8`, SCOSC, SCORC); erase in display and line (ED, EL), erase,
//! insert and delete characters (ECH, ICH, DCH), insert and delete lines
//! (IL, DL); scrolling (SU, SD, IND, NEL, RI, the DECSTBM region); the
//! alternate screen (private modes 47, 1047, 1049); full reset (RIS).
//! Every other sequence, SGR included, is read and dropped, and no
//! control character reaches the text.

use std::collections::VecDeque;
use std::iter::Peekable;
use std::str::Chars;

/// The widest a row grows: a character written past it starts the next
/// row.
pub const MAX_COLUMNS: usize = 512;

/// The most rows the screen holds: a line feed on the last one drops the
/// first, as a terminal's scrollback limit does.
pub const MAX_ROWS: usize = 1_000;

/// The column every tab stop is a multiple of.
const TAB_STOP: usize = 8;

/// The most parameters a control sequence keeps; later ones are read and
/// dropped.
const MAX_PARAMS: usize = 16;

/// The largest parameter value kept; larger ones are read as it.
const MAX_PARAM: usize = 65_535;

/// What [`render`] made of a terminal output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// The final screen's rows from its first non-blank one to its last,
    /// each without its trailing spaces and ended by a newline; empty for
    /// a blank screen.
    pub text: String,
    /// How many escape sequences were read; none reaches the text.
    pub sequences: u32,
}

/// The screen `bytes` leave once played on a terminal, as text.
///
/// ```
/// use officina_core::screen::render;
/// // Two repaints from the top: the second replaces the first.
/// let out = b"\x1b[2J\x1b[H\x1b[1mthinking\x1b[0m\x1b[K\r\n\x1b[Hdone\x1b[K\r\n\x1b[K";
/// let rendered = render(out);
/// assert_eq!(rendered.text, "done\n");
/// assert_eq!(rendered.sequences, 8);
/// ```
pub fn render(bytes: &[u8]) -> Rendered {
    let text = String::from_utf8_lossy(bytes);
    let mut screen = Screen::default();
    let mut sequences = 0u32;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let introduces = matches!(
            c,
            '\u{1b}'
                | '\u{84}'
                | '\u{85}'
                | '\u{8d}'
                | '\u{90}'
                | '\u{98}'
                | '\u{9b}'
                | '\u{9d}'
                | '\u{9e}'
                | '\u{9f}'
        );
        if introduces {
            sequences = sequences.saturating_add(1);
        }
        match c {
            '\u{1b}' => screen.escape(&mut chars),
            '\u{9b}' => {
                if let Some(csi) = read_csi(&mut chars) {
                    screen.csi(&csi);
                }
            }
            '\u{9d}' => skip_string(&mut chars, true),
            '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => skip_string(&mut chars, false),
            '\u{84}' => screen.line_feed(),
            '\u{85}' => screen.next_line(),
            '\u{8d}' => screen.reverse_index(),
            c => screen.control_or_print(c),
        }
    }
    Rendered {
        text: screen.text(),
        sequences,
    }
}

type Row = Vec<char>;

#[derive(Debug, Clone, Copy, Default)]
struct Cursor {
    row: usize,
    col: usize,
    /// A character was written in the last column: the next one starts
    /// the next row, as a terminal's deferred wrap does.
    pending_wrap: bool,
}

#[derive(Debug, Default)]
struct Screen {
    main: VecDeque<Row>,
    alternate: VecDeque<Row>,
    on_alternate: bool,
    cursor: Cursor,
    saved: Cursor,
    /// The DECSTBM scroll region's first and last rows; `None` for the
    /// whole screen.
    region: Option<(usize, usize)>,
}

/// A control sequence as read: its parameters, missing ones as 0.
#[derive(Debug, Default)]
struct Csi {
    /// A private marker (`<`, `=`, `>`, `?`) came with it.
    private: bool,
    /// An intermediate byte came with it: no sequence interpreted has one.
    intermediate: bool,
    params: Vec<usize>,
    last: char,
}

impl Csi {
    /// Parameter `at`, or `default` when it is missing or 0.
    fn arg(&self, at: usize, default: usize) -> usize {
        match self.params.get(at) {
            Some(&value) if value > 0 => value,
            _ => default,
        }
    }
}

/// Reads a control sequence after its introducer, through its final byte;
/// `None` when the output ends first or an escape interrupts it, the
/// escape left to be read.
fn read_csi(chars: &mut Peekable<Chars<'_>>) -> Option<Csi> {
    let mut csi = Csi::default();
    let mut current: Option<usize> = None;
    while let Some(&c) = chars.peek() {
        if c == '\u{1b}' {
            return None;
        }
        chars.next();
        match c {
            '0'..='9' => {
                let digit = c as usize - '0' as usize;
                let value = current
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit);
                current = Some(value.min(MAX_PARAM));
            }
            ';' | ':' => {
                let value = current.take().unwrap_or(0);
                if csi.params.len() < MAX_PARAMS {
                    csi.params.push(value);
                }
            }
            '<'..='?' => csi.private = true,
            ' '..='/' => csi.intermediate = true,
            '@'..='~' => {
                if let Some(value) = current
                    && csi.params.len() < MAX_PARAMS
                {
                    csi.params.push(value);
                }
                csi.last = c;
                return Some(csi);
            }
            // A control character inside a sequence is dropped.
            _ => {}
        }
    }
    None
}

/// Skips a string sequence (OSC, DCS, SOS, PM, APC) to its terminator:
/// `ESC \` or ST, or BEL when `bel` ends it too (OSC).
fn skip_string(chars: &mut Peekable<Chars<'_>>, bel: bool) {
    while let Some(c) = chars.next() {
        match c {
            '\u{7}' if bel => return,
            '\u{9c}' => return,
            '\u{1b}' if chars.peek() == Some(&'\\') => {
                chars.next();
                return;
            }
            _ => {}
        }
    }
}

impl Screen {
    fn rows(&mut self) -> &mut VecDeque<Row> {
        if self.on_alternate {
            &mut self.alternate
        } else {
            &mut self.main
        }
    }

    /// Row `at`, the rows above it made blank when not written yet.
    fn row(&mut self, at: usize) -> &mut Row {
        let rows = self.rows();
        if rows.len() <= at {
            rows.resize_with(at + 1, Row::new);
        }
        &mut rows[at]
    }

    fn control_or_print(&mut self, c: char) {
        match c {
            '\r' => self.move_to(self.cursor.row, 0),
            '\n' | '\u{b}' | '\u{c}' => self.next_line(),
            '\u{8}' => self.move_to(self.cursor.row, self.cursor.col.saturating_sub(1)),
            '\t' => self.move_to(self.cursor.row, (self.cursor.col / TAB_STOP + 1) * TAB_STOP),
            c if c.is_control() => {}
            c => self.print(c),
        }
    }

    fn print(&mut self, c: char) {
        if self.cursor.pending_wrap {
            self.cursor.col = 0;
            self.line_feed();
        }
        let Cursor { row, col, .. } = self.cursor;
        let line = self.row(row);
        if line.len() <= col {
            line.resize(col + 1, ' ');
        }
        line[col] = c;
        if col + 1 == MAX_COLUMNS {
            self.cursor.pending_wrap = true;
        } else {
            self.cursor.col = col + 1;
        }
    }

    fn move_to(&mut self, row: usize, col: usize) {
        self.cursor = Cursor {
            row: row.min(MAX_ROWS - 1),
            col: col.min(MAX_COLUMNS - 1),
            pending_wrap: false,
        };
    }

    /// The scroll region, or the whole screen.
    fn scroll_bounds(&self) -> (usize, usize) {
        self.region.unwrap_or((0, MAX_ROWS - 1))
    }

    fn line_feed(&mut self) {
        self.cursor.pending_wrap = false;
        let row = self.cursor.row;
        match self.region {
            Some((top, bottom)) if row == bottom => self.scroll_up(top, bottom, 1),
            _ if row + 1 == MAX_ROWS => {
                self.rows().pop_front();
            }
            _ => self.cursor.row = row + 1,
        }
    }

    fn next_line(&mut self) {
        self.move_to(self.cursor.row, 0);
        self.line_feed();
    }

    fn reverse_index(&mut self) {
        self.cursor.pending_wrap = false;
        let (top, bottom) = self.scroll_bounds();
        if self.cursor.row == top {
            self.scroll_down(top, bottom, 1);
        } else {
            self.cursor.row = self.cursor.row.saturating_sub(1);
        }
    }

    /// Rows `top..=bottom` moved up by `n`: the first `n` leave and blank
    /// rows enter at the bottom. Rows past the last one written are blank,
    /// so only the written ones move.
    fn scroll_up(&mut self, top: usize, bottom: usize, n: usize) {
        let rows = self.rows();
        let end = rows.len().min(bottom + 1);
        if top >= end {
            return;
        }
        let span = &mut rows.make_contiguous()[top..end];
        let n = n.min(span.len());
        span.rotate_left(n);
        let len = span.len();
        span[len - n..].iter_mut().for_each(Row::clear);
    }

    /// Rows `top..=bottom` moved down by `n`: the last `n` leave and blank
    /// rows enter at the top.
    fn scroll_down(&mut self, top: usize, bottom: usize, n: usize) {
        let rows = self.rows();
        if rows.len() <= top {
            return;
        }
        let end = rows.len().saturating_add(n).min(bottom + 1);
        if rows.len() < end {
            rows.resize_with(end, Row::new);
        }
        let span = &mut rows.make_contiguous()[top..end];
        let n = n.min(span.len());
        span.rotate_right(n);
        span[..n].iter_mut().for_each(Row::clear);
    }

    /// Columns `from..to` of the cursor's row made blank.
    fn blank(&mut self, from: usize, to: usize) {
        let row = self.cursor.row;
        let Some(line) = self.rows().get_mut(row) else {
            return;
        };
        if to >= line.len() {
            line.truncate(from);
        } else {
            line[from..to].fill(' ');
        }
    }

    fn erase_display(&mut self, mode: usize) {
        let Cursor { row, col, .. } = self.cursor;
        match mode {
            0 => {
                let rows = self.rows();
                rows.truncate(row + 1);
                if let Some(line) = rows.get_mut(row) {
                    line.truncate(col);
                }
            }
            1 => {
                self.rows().iter_mut().take(row).for_each(Row::clear);
                self.blank(0, col + 1);
            }
            2 | 3 => self.rows().clear(),
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: usize) {
        let col = self.cursor.col;
        match mode {
            0 => self.blank(col, usize::MAX),
            1 => self.blank(0, col + 1),
            2 => self.blank(0, usize::MAX),
            _ => {}
        }
    }

    fn insert_chars(&mut self, n: usize) {
        let Cursor { row, col, .. } = self.cursor;
        if let Some(line) = self.rows().get_mut(row)
            && col < line.len()
        {
            let n = n.min(MAX_COLUMNS - col);
            line.splice(col..col, std::iter::repeat_n(' ', n));
            line.truncate(MAX_COLUMNS);
        }
    }

    fn delete_chars(&mut self, n: usize) {
        let Cursor { row, col, .. } = self.cursor;
        if let Some(line) = self.rows().get_mut(row)
            && col < line.len()
        {
            let end = col.saturating_add(n).min(line.len());
            line.drain(col..end);
        }
    }

    /// IL and DL: lines inserted or deleted at the cursor's row, inside the
    /// scroll region only, the cursor going to the row's start.
    fn insert_or_delete_lines(&mut self, n: usize, insert: bool) {
        let row = self.cursor.row;
        let (top, bottom) = self.scroll_bounds();
        if row < top || row > bottom {
            return;
        }
        if insert {
            self.scroll_down(row, bottom, n);
        } else {
            self.scroll_up(row, bottom, n);
        }
        self.move_to(row, 0);
    }

    fn set_region(&mut self, csi: &Csi) {
        let top = csi.arg(0, 1) - 1;
        let bottom = csi.arg(1, MAX_ROWS).min(MAX_ROWS) - 1;
        if top >= bottom {
            return;
        }
        self.region = (top > 0 || bottom < MAX_ROWS - 1).then_some((top, bottom));
        self.move_to(0, 0);
    }

    /// Private modes 47, 1047 and 1049: the alternate screen, entered
    /// blank; 1049 also saves the cursor on entering and restores it on
    /// leaving.
    fn switch_screen(&mut self, enter: bool, with_cursor: bool) {
        if enter == self.on_alternate {
            return;
        }
        if enter {
            if with_cursor {
                self.saved = self.cursor;
            }
            self.alternate.clear();
            self.on_alternate = true;
        } else {
            self.on_alternate = false;
            if with_cursor {
                self.cursor = self.saved;
            }
        }
    }

    fn escape(&mut self, chars: &mut Peekable<Chars<'_>>) {
        // An escape interrupted by another one is dropped, the next read
        // as itself.
        let Some(&next) = chars.peek() else {
            return;
        };
        if next == '\u{1b}' {
            return;
        }
        chars.next();
        match next {
            '[' => {
                if let Some(csi) = read_csi(chars) {
                    self.csi(&csi);
                }
            }
            ']' => skip_string(chars, true),
            'P' | 'X' | '^' | '_' => skip_string(chars, false),
            '7' => self.saved = self.cursor,
            '8' => self.cursor = self.saved,
            'D' => self.line_feed(),
            'E' => self.next_line(),
            'M' => self.reverse_index(),
            'c' => *self = Screen::default(),
            // Intermediates, then one final byte: a character set
            // designation and the like.
            ' '..='/' => {
                while chars.next_if(|c| matches!(c, ' '..='/')).is_some() {}
                chars.next();
            }
            _ => {}
        }
    }

    fn csi(&mut self, csi: &Csi) {
        if csi.intermediate {
            return;
        }
        if csi.private {
            if matches!(csi.last, 'h' | 'l') {
                for &mode in &csi.params {
                    if matches!(mode, 47 | 1047 | 1049) {
                        self.switch_screen(csi.last == 'h', mode == 1049);
                    }
                }
            }
            return;
        }
        let n = csi.arg(0, 1);
        let Cursor { row, col, .. } = self.cursor;
        match csi.last {
            'A' => self.move_to(row.saturating_sub(n), col),
            'B' | 'e' => self.move_to(row.saturating_add(n), col),
            'C' | 'a' => self.move_to(row, col.saturating_add(n)),
            'D' => self.move_to(row, col.saturating_sub(n)),
            'E' => self.move_to(row.saturating_add(n), 0),
            'F' => self.move_to(row.saturating_sub(n), 0),
            'G' | '`' => self.move_to(row, n - 1),
            'd' => self.move_to(n - 1, col),
            'H' | 'f' => self.move_to(n - 1, csi.arg(1, 1) - 1),
            'J' => self.erase_display(csi.arg(0, 0)),
            'K' => self.erase_line(csi.arg(0, 0)),
            'X' => self.blank(col, col.saturating_add(n)),
            '@' => self.insert_chars(n),
            'P' => self.delete_chars(n),
            'L' => self.insert_or_delete_lines(n, true),
            'M' => self.insert_or_delete_lines(n, false),
            'S' => {
                let (top, bottom) = self.scroll_bounds();
                self.scroll_up(top, bottom, n);
            }
            // With more parameters, `CSI T` is a mouse report, not SD.
            'T' if csi.params.len() <= 1 => {
                let (top, bottom) = self.scroll_bounds();
                self.scroll_down(top, bottom, n);
            }
            'r' => self.set_region(csi),
            's' if csi.params.is_empty() => self.saved = self.cursor,
            'u' if csi.params.is_empty() => self.cursor = self.saved,
            _ => {}
        }
    }

    /// The active screen's rows as [`Rendered::text`] holds them.
    fn text(&self) -> String {
        let rows = if self.on_alternate {
            &self.alternate
        } else {
            &self.main
        };
        let lines: Vec<String> = rows
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_owned())
            .collect();
        let Some(first) = lines.iter().position(|line| !line.is_empty()) else {
            return String::new();
        };
        let last = lines
            .iter()
            .rposition(|line| !line.is_empty())
            .unwrap_or(first);
        let mut out = String::new();
        for line in &lines[first..=last] {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(out: &str) -> String {
        render(out.as_bytes()).text
    }

    /// The capture of `claude logs` E100 recorded, its one home-relative
    /// path redacted: thirty repaints of a Haiku session answering with a
    /// box-drawn table.
    const CAPTURE: &[u8] = include_bytes!("../tests/data/logs/claude-2.1.316-repaints.ansi");

    #[test]
    fn the_capture_renders_its_final_screen_once() {
        let rendered = render(CAPTURE);
        let frames = CAPTURE.windows(3).filter(|w| *w == b"\x1b[H").count();
        assert!(frames > 1, "the capture repaints");
        assert_eq!(
            rendered.sequences as usize,
            CAPTURE.iter().filter(|b| **b == 0x1b).count()
        );
        let text = &rendered.text;
        assert!(!text.contains('\u{1b}') && !text.contains('\r'), "{text:?}");
        for once in [
            "Claude Code",
            "Answer with a markdown table",
            "│ Green  │ #008000  │",
            "└────────┴──────────┘",
            "done",
            "bypass permissions on",
        ] {
            assert_eq!(text.matches(once).count(), 1, "{once:?} in\n{text}");
        }
        // The spinner of the first frames was repainted over.
        assert!(!text.contains("Razzle-dazzling"), "{text}");
        let table: Vec<&str> = text
            .lines()
            .skip_while(|line| !line.contains('┌'))
            .take(9)
            .collect();
        assert_eq!(table.len(), 9, "{text}");
        assert!(table.iter().skip(1).all(|line| line.starts_with("  │")
            || line.starts_with("  ├")
            || line.starts_with("  └")));
        assert!(text.lines().all(|line| line.chars().count() <= MAX_COLUMNS));
    }

    #[test]
    fn repaint_frames_leave_the_last_one() {
        let frame = |spinner: &str, body: &str| {
            format!(
                "\u{1b}[H\u{1b}[91mbanner\u{1b}[K\r\n\u{1b}[K\r\n> prompt\u{1b}[K\r\n{body}\u{1b}[K\r\n\u{1b}[90m{spinner}\u{1b}[K\u{1b}[m\r\n\u{1b}[7mbypass permissions on\u{1b}[27m\u{1b}[K\u{1b}[4;3H"
            )
        };
        let mut out = String::from("\u{1b}[2J");
        for (at, spinner) in [
            "Determining… (1s)",
            "Determining… (2s)",
            "Determining… (3s)",
        ]
        .iter()
        .enumerate()
        {
            out.push_str(&frame(spinner, &"working ".repeat(at + 1)));
        }
        out.push_str(&frame("Brewed for 7s · done 21:06", "the answer"));
        assert_eq!(
            text(&out),
            "banner\n\n> prompt\nthe answer\nBrewed for 7s · done 21:06\nbypass permissions on\n"
        );
    }

    #[test]
    fn a_box_table_drawn_by_cursor_moves_is_whole() {
        // The borders first, then each cell's text placed by CUP and CHA.
        let out = concat!(
            "\u{1b}[1;1H┌─────┬─────┐",
            "\u{1b}[2;1H│     │     │",
            "\u{1b}[3;1H└─────┴─────┘",
            "\u{1b}[2;3Hab\u{1b}[9Gcd",
        );
        assert_eq!(text(out), "┌─────┬─────┐\n│ ab  │ cd  │\n└─────┴─────┘\n");
    }

    #[test]
    fn carriage_return_backspace_and_tab_move_the_cursor() {
        assert_eq!(text("50%\r75%\r100%\n"), "100%\n");
        assert_eq!(text("ab\u{8}c\n"), "ac\n");
        assert_eq!(text("a\tb\n"), "a       b\n");
        assert_eq!(text("a\u{7}\u{0}b\n"), "ab\n");
        // A vertical tab and a form feed are line feeds.
        assert_eq!(text("a\u{b}b\u{c}c"), "a\nb\nc\n");
    }

    #[test]
    fn relative_moves_and_their_clamps() {
        // CUU, CUD, CUF, CUB, CNL, CPL, HPR and VPR, from a known spot.
        assert_eq!(text("\u{1b}[3;5Hx\u{1b}[2Ay\n"), "     y\n\n    x\n");
        assert_eq!(text("x\u{1b}[2By\n"), "x\n\n y\n");
        assert_eq!(text("x\u{1b}[3Cy\u{1b}[2Dz\n"), "x  zy\n");
        assert_eq!(text("\u{1b}[3;4Hx\u{1b}[2Fy\u{1b}[Ez\n"), "y\nz\n   x\n");
        assert_eq!(text("x\u{1b}[2ay\u{1b}[ez\n"), "x  y\n    z\n");
        // Up from the first row and left from the first column stay there.
        assert_eq!(text("\u{1b}[9A\u{1b}[9Dx\n"), "x\n");
        // CHA, HPA and VPA are absolute; a missing or 0 parameter is 1.
        assert_eq!(text("abc\u{1b}[2Gx\u{1b}[`y\u{1b}[3dz\n"), "yxc\n\n z\n");
        assert_eq!(text("abc\u{1b}[0;0Hx\n"), "xbc\n");
    }

    #[test]
    fn save_and_restore_the_cursor() {
        assert_eq!(text("a\u{1b}7\u{1b}[3;1Hb\u{1b}8c\n"), "ac\n\nb\n");
        assert_eq!(text("a\u{1b}[s\u{1b}[3;1Hb\u{1b}[uc\n"), "ac\n\nb\n");
    }

    #[test]
    fn erase_in_display_and_line() {
        let screen = "111\r\n222\r\n333";
        assert_eq!(text(&format!("{screen}\u{1b}[2;2H\u{1b}[J")), "111\n2\n");
        assert_eq!(text(&format!("{screen}\u{1b}[2;2H\u{1b}[1J")), "  2\n333\n");
        for all in ["\u{1b}[2J", "\u{1b}[3J"] {
            assert_eq!(text(&format!("{screen}{all}")), "");
        }
        assert_eq!(text("abcd\u{1b}[3G\u{1b}[K"), "ab\n");
        assert_eq!(text("abcd\u{1b}[3G\u{1b}[1K"), "   d\n");
        assert_eq!(text("abcd\u{1b}[3G\u{1b}[2Kx"), "  x\n");
        assert_eq!(text("abcdef\u{1b}[2G\u{1b}[2X"), "a  def\n");
    }

    #[test]
    fn insert_and_delete_characters_and_lines() {
        assert_eq!(text("abcd\u{1b}[2G\u{1b}[2@"), "a  bcd\n");
        assert_eq!(text("abcd\u{1b}[2G\u{1b}[2P"), "ad\n");
        assert_eq!(text("1\r\n2\r\n3\u{1b}[2;1H\u{1b}[L"), "1\n\n2\n3\n");
        assert_eq!(text("1\r\n2\r\n3\u{1b}[1;1H\u{1b}[M"), "2\n3\n");
    }

    #[test]
    fn scrolling_inside_and_outside_a_region() {
        // A region of rows 2 to 3: a line feed on its last row scrolls it
        // only; the rows outside stay.
        let out = "top\r\n\u{1b}[2;3r\u{1b}[2;1Ha\r\nb\r\nc\u{1b}[4;1Hbottom";
        assert_eq!(text(out), "top\nb\nc\nbottom\n");
        // Reverse index on the region's first row scrolls it down.
        let out = "\u{1b}[1;1Htop\u{1b}[2;3r\u{1b}[2;1Ha\r\nb\u{1b}[4;1Hbottom\u{1b}[2;1H\u{1b}Mz";
        assert_eq!(text(out), "top\nz\na\nbottom\n");
        // SU and SD on the whole screen; IND and NEL feed lines.
        assert_eq!(text("1\r\n2\r\n3\u{1b}[S"), "2\n3\n");
        assert_eq!(text("1\r\n2\u{1b}[T\u{1b}[1;1Hz"), "z\n1\n2\n");
        assert_eq!(text("a\u{1b}Db\u{1b}Ec\n"), "a\n b\nc\n");
        // A mouse report spelled `CSI T` with parameters moves nothing.
        assert_eq!(text("1\u{1b}[1;2;3;4;5T"), "1\n");
    }

    #[test]
    fn the_alternate_screen_shows_while_it_is_on() {
        for mode in [47, 1047, 1049] {
            let enter = format!("\u{1b}[?{mode}h");
            let leave = format!("\u{1b}[?{mode}l");
            assert_eq!(text(&format!("main{enter}\u{1b}[Halt")), "alt\n", "{mode}");
            assert_eq!(
                text(&format!("main{enter}\u{1b}[Halt{leave}")),
                "main\n",
                "{mode}"
            );
            // Entered again, it starts blank.
            assert_eq!(
                text(&format!("{enter}old{leave}{enter}\u{1b}[2;1Hnew")),
                "new\n",
                "{mode}"
            );
        }
        // 1049 restores the cursor it saved.
        assert_eq!(text("ab\u{1b}[?1049h\u{1b}[5;5Hx\u{1b}[?1049lc"), "abc\n");
    }

    #[test]
    fn a_full_reset_clears_everything() {
        assert_eq!(text("old\u{1b}[2;3r\u{1b}cnew"), "new\n");
    }

    #[test]
    fn other_sequences_are_read_and_dropped() {
        let families = [
            "\u{1b}[38;5;208m",
            "\u{1b}[?25l",
            "\u{1b}[>1u",
            "\u{1b}[<u",
            "\u{1b}[1 q",
            "\u{1b}]0;window title\u{7}",
            "\u{1b}]8;;https://example.com\u{1b}\\",
            "\u{1b}P1$r0m\u{1b}\\",
            "\u{1b}X sos \u{1b}\\",
            "\u{1b}^ pm \u{9c}",
            "\u{1b}_ apc \u{1b}\\",
            "\u{1b}(B",
            "\u{1b}#8",
            "\u{1b}=",
            "\u{9b}1m",
            "\u{9d}0;title\u{7}",
            "\u{90}dcs\u{9c}",
        ];
        let out: String = families
            .iter()
            .enumerate()
            .map(|(at, seq)| format!("{seq}line {at}\r\n"))
            .collect();
        let rendered = render(out.as_bytes());
        let expected: String = (0..families.len())
            .map(|at| format!("line {at}\n"))
            .collect();
        assert_eq!(rendered.text, expected);
        assert_eq!(rendered.sequences as usize, families.len());
        // An unterminated string drops what follows it only; an escape
        // cut by another is dropped.
        assert_eq!(text("kept\u{1b}]0;never closed"), "kept\n");
        assert_eq!(text("a\u{1b}\u{1b}[1mb\n"), "ab\n");
        assert_eq!(text("a\u{1b}[12"), "a\n");
    }

    #[test]
    fn the_screen_stays_within_its_bounds() {
        let long = "x".repeat(MAX_COLUMNS + 3);
        assert_eq!(text(&long), format!("{}\nxxx\n", "x".repeat(MAX_COLUMNS)));
        let far = format!(
            "\u{1b}[{0};{0}Hz\u{1b}[99999C\u{1b}[99999Bw",
            MAX_PARAM * 10
        );
        let rendered = text(&far);
        assert_eq!(rendered, format!("{}w\n", " ".repeat(MAX_COLUMNS - 1)));
        let many = "line\r\n".repeat(MAX_ROWS + 10);
        let rendered = text(&many);
        assert_eq!(rendered.lines().count(), MAX_ROWS - 1);
    }

    #[test]
    fn invalid_utf8_and_blank_output() {
        assert_eq!(text(""), "");
        assert_eq!(text("   \r\n\u{1b}[K\r\n"), "");
        assert_eq!(render(b"a\xffb\n").text, "a\u{fffd}b\n");
    }
}
