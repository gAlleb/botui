//! Original BOTUI artwork. A cell holds two square pixels, so the emblem works
//! over SSH or in tmux without a font file, emoji, or terminal image protocol.
use super::Ink;
use ratatui::{
    Frame,
    layout::Position,
    style::{Color, Style},
};

const OWL: [&str; 24] = [
    "   f                 f   ",
    "   ff               ff   ",
    "   ftff    fff    fftf   ",
    "   ftffffffffffffffftf   ",
    "   ffffppppfffppppffff   ",
    "  fffpppppppfpppppppfff  ",
    "  ftpppiiipppppiiippptf  ",
    "  ftppip..ipppi.p.ipptf  ",
    "  ftppii..ipppi..iipptf  ",
    "  ftpppiiippappiiippptf  ",
    "  fttppppppaaappppppttf  ",
    "  fttftppppfafpppptfttf  ",
    "   tttfpppppppppppfttt   ",
    "   tttfppfpppppfppfttt   ",
    "    tttfpppfpfpppfttt    ",
    "     ttfpppppppppftt     ",
    "       aafffffffaa       ",
    " bbbbbbaa       aabbbbbb ",
    " bpppppppbbb bbbpppppppb ",
    " bppssssppppbppppssssppb ",
    " bppppppppppbppppppppppb ",
    " bppssssppppbppppssssppb ",
    " bbbbbbbbpppbpppbbbbbbbb ",
    "         bbbbbbb         ",
];
// Drawn as a bookplate: bracketed serifs, open counters and a single offset shadow.
const WORD: [&str; 11] = [
    "#######    #####    #########  #####  ###  #####",
    " ##   ##  ##   ##   ## ### ##   ##     #    ### ",
    " ##   ## ###   ###     ###      ##     #    ### ",
    " ##  ##  ##     ##     ###      ##     #    ### ",
    " #####   ##     ##     ###      ##     #    ### ",
    " ##  ##  ##     ##     ###      ##     #    ### ",
    " ##   ## ##     ##     ###      ##     #    ### ",
    " ##   ## ###   ###     ###      ##     #    ### ",
    " ##   ##  ##   ##      ###      ###   ##    ### ",
    "#######    #####      #####      #####     #####",
    "                                                ",
];
pub(super) const W: u16 = 25 + 4 + WORD[0].len() as u16 + 1;
pub(super) const H: u16 = OWL.len() as u16 / 2;
pub(super) const TYPE_X: u16 = 29;

fn pixels(
    f: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    pixel: impl Fn(u16, u16) -> Option<Color>,
) {
    for row in 0..h {
        for col in 0..w {
            let (ch, fg, bg) = match (pixel(col, 2 * row), pixel(col, 2 * row + 1)) {
                (None, None) => continue,
                (Some(top), Some(bottom)) if top == bottom => ('█', top, None),
                (Some(top), bottom) => ('▀', top, bottom),
                (None, Some(bottom)) => ('▄', bottom, None),
            };
            if let Some(cell) = f.buffer_mut().cell_mut(Position::new(x + col, y + row)) {
                let style = Style::new().fg(fg);
                cell.set_char(ch)
                    .set_style(bg.map_or(style, |c| style.bg(c)));
            }
        }
    }
}

pub(super) fn draw(f: &mut Frame, x: u16, y: u16, ink: &Ink) {
    let mono = ink.violet == Color::Reset;
    pixels(f, x, y, 25, H, |col, row| {
        let p = *OWL[row as usize].as_bytes().get(col as usize)?;
        if mono {
            return match p {
                b'f' | b'b' | b'i' | b'a' | b's' => Some(Color::Reset),
                _ => None,
            };
        }
        match p {
            b'f' => Some(ink.feathers),
            b't' => Some(ink.shade),
            b'a' | b'i' => Some(ink.amber),
            b'p' => Some(ink.paper),
            b'b' => Some(ink.aqua),
            b's' => Some(ink.shade),
            _ => None,
        }
    });
    let filled =
        |col: usize, row: usize| WORD.get(row).and_then(|l| l.as_bytes().get(col)) == Some(&b'#');
    pixels(f, x + TYPE_X, y + 1, W - TYPE_X, 6, |col, row| {
        let (col, row) = (col as usize, row as usize);
        if filled(col, row) {
            Some(ink.violet)
        } else if !mono && col > 0 && row > 0 && filled(col - 1, row - 1) {
            Some(ink.shade)
        } else {
            None
        }
    });
}
