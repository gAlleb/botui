//! Typeset the BOTUI wordmark in terminal half-blocks alongside the shared owl.
use super::Ink;
use ratatui::{
    Frame,
    layout::Position,
    style::{Color, Style},
};

// One design for all terminals. Each symbol is one square of the owl, upright
// on the terminal grid. The right eye and ear sit higher than the left pair.
const OWL: [&[u8; 22]; 32] = [
    b"................ff....",
    b"..............ffff....",
    b".............ffssf....",
    b".ff.....ffffffffssf...",
    b".fff...ffffffffffff...",
    b".fsffffffppffaaaafff..",
    b".fsfffppppffaaaaaasf..",
    b".ffffaaaapppaaddHasf..",
    b"fffpaaaaaappaadddasff.",
    b"ffspaaddHappaadddasff.",
    b"ffspaadddapapaaaapsff.",
    b".fspaadddaaaappppffs..",
    b".fsspaaaapfafpppffss..",
    b".fvvvfpppppfppppfvvvv.",
    b"..vvvffpppppppppfvvvv.",
    b".vvvvffpppppppppfvvvv.",
    b".vvvvsppfppfppfpvvvvv.",
    b".vvvssppppppppppssvvv.",
    b"..vvssfppfppfppffssv..",
    b"....sssppppppppssss...",
    b".....sssspppppssss....",
    b"......aaassssaaas.....",
    b".bbbbbaaabbbbaaabbbb..",
    b"bbbbbbbbbbbbbbbbbbbb..",
    b"bbbbaaaaaaaaaafffaaa..",
    b"bbbbaaaaaaaaaafafaaa..",
    b".bbbbbbbbbbbbbbbbbbb..",
    b"...vvvvvvvvvvvvvvvvvvv",
    b"..vvvvvaaaaaaaaaaaaaa.",
    b"..vvvvvaaaaaaaaaaaaaa.",
    b"..vvvvvaaaaaaaaaaaaaa.",
    b"...vvvvvvvvvvvvvvvvvvv",
];
// Drawn as a bookplate: bracketed serifs, open counters and a single offset shadow.
const WORD: [&str; 11] = [
    "#########       #####     ###########  ####   ####  #######",
    "  ##    ##     ##   ##    ##  ###  ##   ##     ##     ###  ",
    "  ##     ##   ##     ##   #   ###   #   ##     ##     ###  ",
    "  ##     ##  ##       ##      ###       ##     ##     ###  ",
    "  ########   ##       ##      ###       ##     ##     ###  ",
    "  ########   ##       ##      ###       ##     ##     ###  ",
    "  ##     ##  ##       ##      ###       ##     ##     ###  ",
    "  ##     ##  ##       ##      ###       ##     ##     ###  ",
    "  ##     ##   ##     ##       ###       ##     ##     ###  ",
    "  ##    ##     ##   ##        ###        ##   ##      ###  ",
    "#########       #####       #######       #####     #######",
];
pub(super) const H: u16 = (OWL.len() / 2) as u16;
pub(super) const TYPE_X: u16 = 29;
// At 80 columns keep the serifs and open counters by narrowing repeated strokes.
const NARROW_SKIP: [usize; 11] = [1, 6, 12, 18, 25, 31, 34, 38, 45, 55, 58];
const FULL: u16 = TYPE_X + WORD[0].len() as u16 + 1;
pub(super) const MIN: u16 = FULL - NARROW_SKIP.len() as u16;

pub(super) fn width(available: u16) -> u16 {
    if available >= FULL { FULL } else { MIN }
}

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

pub(super) fn draw(f: &mut Frame, area: ratatui::layout::Rect, ink: &Ink) {
    let (x, y) = (area.x, area.y);
    let mono = ink.violet == Color::Reset;
    let w = OWL[0].len() as u16;
    pixels(f, x + (25 - w) / 2, y, w, H, |col, row| {
        let role = OWL[row as usize][col as usize];
        if mono {
            return matches!(role, b'f' | b'a' | b'b' | b'v' | b'H').then_some(Color::Reset);
        }
        match role {
            b'f' => Some(ink.feathers),
            b'p' => Some(ink.paper),
            b'a' => Some(ink.amber),
            b'b' => Some(ink.aqua),
            b'v' => Some(ink.violet),
            b's' => Some(ink.shade),
            b'd' => Some(ink.pupil),
            b'H' => Some(ink.glint),
            _ => None,
        }
    });
    let columns: Vec<_> = (0..WORD[0].len())
        .filter(|i| area.width >= FULL || !NARROW_SKIP.contains(i))
        .collect();
    let filled = |col: usize, row: usize| {
        columns
            .get(col)
            .zip(WORD.get(row))
            .is_some_and(|(&x, line)| line.as_bytes()[x] == b'#')
    };
    pixels(
        f,
        x + TYPE_X,
        area.bottom() - 11,
        area.width - TYPE_X,
        6,
        |col, row| {
            let (col, row) = (col as usize, row as usize);
            if filled(col, row) {
                Some(ink.violet)
            } else if !mono && col > 0 && row > 0 && filled(col - 1, row - 1) {
                Some(ink.shade)
            } else {
                None
            }
        },
    );
}
