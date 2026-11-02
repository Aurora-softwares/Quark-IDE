//! VT100 cell rendering for all native Quark terminals. The PTY owns bytes;
//! this presenter paints the parser's screen, including alternate-screen
//! contents, cell colours, inverse attributes, underlines and cursor position.
use naui::{Color, Painter, Point, Rect};

pub const CELL_WIDTH: f64 = 9.0;
pub const CELL_HEIGHT: f64 = 18.0;
const FONT_SIZE: f64 = 13.0;
const BG: (u8, u8, u8) = (17, 20, 26);
const FG: (u8, u8, u8) = (222, 228, 236);
const ANSI: [(u8, u8, u8); 16] = [
    (28, 33, 40), (232, 98, 106), (147, 207, 121), (231, 190, 98),
    (116, 163, 232), (198, 146, 232), (108, 198, 209), (222, 228, 236),
    (91, 100, 116), (255, 121, 133), (173, 232, 137), (255, 212, 125),
    (145, 189, 248), (220, 174, 255), (139, 222, 234), (255, 255, 255),
];

fn rgb(color: (u8, u8, u8)) -> Color {
    Color::rgb(color.0, color.1, color.2)
}

fn resolve(color: vt100::Color, default: (u8, u8, u8)) -> (u8, u8, u8) {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => (r, g, b),
        vt100::Color::Idx(index @ 0..=15) => ANSI[usize::from(index)],
        vt100::Color::Idx(index @ 16..=231) => {
            let cube = index - 16;
            let component = |value| if value == 0 { 0 } else { 55 + 40 * value };
            (
                component(cube / 36),
                component(cube / 6 % 6),
                component(cube % 6),
            )
        }
        vt100::Color::Idx(index) => {
            let gray = 8 + (index - 232) * 10;
            (gray, gray, gray)
        }
    }
}

/// Paint the actual VT grid instead of flattening it into a label. Blank
/// default cells are elided to limit WinUI Canvas rebuilding overhead.
pub fn paint(screen: &vt100::Screen, painter: &mut Painter) {
    painter.fill_rect(
        Rect::new(0.0, 0.0, painter.width(), painter.height()),
        rgb(BG),
    );
    let (rows, cols) = screen.size();
    let (cursor_row, cursor_col) = screen.cursor_position();
    let draw_cursor = !screen.hide_cursor() && screen.scrollback() == 0;
    for row in 0..rows.get() {
        let y = f64::from(row) * CELL_HEIGHT;
        if y >= painter.height() {
            break;
        }
        for col in 0..cols.get() {
            let x = f64::from(col) * CELL_WIDTH;
            if x >= painter.width() {
                break;
            }
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let (mut fg, mut bg) = (
                resolve(cell.fgcolor(), FG),
                resolve(cell.bgcolor(), BG),
            );
            if cell.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            if draw_cursor && row == cursor_row && col == cursor_col {
                std::mem::swap(&mut fg, &mut bg);
            }
            let width = CELL_WIDTH * if cell.is_wide() { 2.0 } else { 1.0 };
            if bg != BG {
                painter.fill_rect(Rect::new(x, y, width, CELL_HEIGHT), rgb(bg));
            }
            if cell.dim() {
                fg = (fg.0 / 2, fg.1 / 2, fg.2 / 2);
            } else if cell.bold() && matches!(cell.fgcolor(), vt100::Color::Default) {
                fg = ANSI[15];
            }
            if !cell.contents().is_empty() {
                painter.text(cell.contents(), Point::new(x, y + 1.0), FONT_SIZE, rgb(fg));
            }
            if cell.underline() {
                painter.fill_rect(
                    Rect::new(x, y + CELL_HEIGHT - 2.0, width, 1.0),
                    rgb(fg),
                );
            }
        }
    }
}

/// A native canvas tracks PTY rows and columns independently of word wrapping.
pub fn viewport_size(rows: u16, cols: u16) -> (f64, f64) {
    (
        f64::from(cols.max(10)) * CELL_WIDTH,
        f64::from(rows.max(2)) * CELL_HEIGHT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU16;

    fn parser() -> vt100::Parser {
        vt100::Parser::new(
            NonZeroU16::new(6).unwrap(),
            NonZeroU16::new(20).unwrap(),
            40,
        )
    }

    #[test]
    fn indexed_truecolor_and_cursor_create_independent_paint_commands() {
        let mut terminal = parser();
        terminal.process(b"\x1b[31mRed\x1b[38;5;196mBright\x1b[38;2;9;80;240mRGB");
        let screen = terminal.screen();
        assert_eq!(screen.cell(0, 0).unwrap().fgcolor(), vt100::Color::Idx(1));
        assert_eq!(screen.cell(0, 3).unwrap().fgcolor(), vt100::Color::Idx(196));
        assert_eq!(
            screen.cell(0, 9).unwrap().fgcolor(),
            vt100::Color::Rgb(9, 80, 240)
        );
        assert_eq!(resolve(vt100::Color::Idx(196), FG), (255, 0, 0));
        assert_eq!(resolve(vt100::Color::Idx(232), FG), (8, 8, 8));
        let mut painter = Painter::new(180.0, 108.0);
        paint(screen, &mut painter);
        assert!(painter.commands().len() > 7);
    }

    #[test]
    fn alternate_screen_unicode_cursor_and_resize_keep_vt_state() {
        let mut terminal = parser();
        terminal.process("normal☃".as_bytes());
        terminal.process(b"\x1b[?1049h\x1b[2;4H\x1b[1;4mX");
        assert!(terminal.screen().alternate_screen());
        assert_eq!(terminal.screen().cell(1, 3).unwrap().contents(), "X");
        assert_eq!(terminal.screen().cursor_position(), (1, 4));
        let mut painter = Painter::new(180.0, 108.0);
        paint(terminal.screen(), &mut painter);
        assert!(painter.commands().len() >= 3);
        terminal.process(b"\x1b[?1049l");
        assert!(!terminal.screen().alternate_screen());
        assert!(terminal.screen().contents().contains('☃'));
        terminal.set_size(
            NonZeroU16::new(10).unwrap(),
            NonZeroU16::new(40).unwrap(),
        );
        assert_eq!(viewport_size(10, 40), (360.0, 180.0));
    }
}
