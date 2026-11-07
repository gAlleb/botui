//! One settings and help surface for every source, including direct-file launches.
mod edit;
mod help;
use crate::{AppTerminal, appearance, settings::Settings, shell_theme};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Style, Stylize},
    widgets::{List, ListState, Paragraph},
};
use std::io;
#[derive(Clone, Copy, PartialEq)]
pub enum Context {
    Shelf,
    Local,
    Text,
    Scan,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Open {
    Settings,
    Help,
}
impl Open {
    pub fn key(code: KeyCode) -> Option<Self> {
        match code {
            KeyCode::F(2) => Some(Self::Settings),
            KeyCode::Char('?') | KeyCode::F(1) => Some(Self::Help),
            _ => None,
        }
    }
}
#[derive(Default)]
pub struct Buttons([Rect; 2]);
impl Buttons {
    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.0 = [Rect::default(); 2];
        let mut x = area.x;
        for (i, s) in ["[F2 Settings]", "[? Help]"].iter().enumerate() {
            let len = s.len() as u16;
            if area.height == 0 || x + len > area.right() {
                break;
            }
            let r = Rect::new(x, area.y, len, 1);
            self.0[i] = r;
            f.render_widget(Paragraph::new(*s).cyan(), r);
            x += len + 1;
        }
    }
    pub fn hit(&self, pos: Position) -> Option<Open> {
        self.0
            .iter()
            .position(|r| r.contains(pos))
            .map(|i| if i == 0 { Open::Settings } else { Open::Help })
    }
    pub fn event(&self, e: &Event) -> Option<Open> {
        match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => Open::key(k.code),
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                self.hit(Position::new(m.column, m.row))
            }
            _ => None,
        }
    }
}
const TABS: [&str; 5] = ["Appearance", "Reader", "Library", "BookOrbit", "Keys"];
fn rows(tab: usize, s: &Settings) -> Vec<String> {
    let pair = |k: &str, v: &str| format!(" {k:<23} {v}");
    let view = |grid| if grid { "Cover grid" } else { "List" };
    match tab {
        0 => vec![
            pair(
                "Interface theme",
                shell_theme::ALL[shell_theme::index(s.interface_theme.as_deref())].1,
            ),
            pair(
                "Background",
                if s.opaque_background {
                    "Opaque"
                } else {
                    "Transparent"
                },
            ),
            pair("Borders & covers", s.borders.label()),
            pair(
                "Solid terminal color",
                s.opaque_color
                    .as_deref()
                    .unwrap_or("Auto · terminal palette"),
            ),
        ],
        1 => vec![
            pair(
                "Reading theme",
                &crate::theme::Theme::from_id(s.theme.as_deref().unwrap_or("terminal")).name(),
            ),
            pair(
                "Reader view",
                if s.zen { "Zen" } else { "Contents sidebar" },
            ),
            pair(
                "Page width",
                &format!("{} cells", s.page_width.unwrap_or(78)),
            ),
            pair(
                "Column gap",
                &format!("{} cells", s.column_gap.unwrap_or(4)),
            ),
            pair(
                "Printed line breaks",
                if s.preserve_printed_lines {
                    "Preserve"
                } else {
                    "Reflow"
                },
            ),
            pair(
                "Illustrations",
                if s.hide_inline_images {
                    "Viewer only"
                } else {
                    "In text"
                },
            ),
            pair(
                "Default page layout",
                s.reading_layout.as_deref().unwrap_or("auto"),
            ),
            pair(
                "First-line indent",
                &format!("{} cells", s.paragraph_indent.min(8)),
            ),
            pair(
                "Contents width",
                &s.contents_width
                    .map_or_else(|| "Auto".into(), |n| format!("{n} cells")),
            ),
        ],
        2 => vec![
            pair(
                "Local folder",
                &s.library_root
                    .as_deref()
                    .map_or_else(|| "Default".into(), |p| p.display().to_string()),
            ),
            pair("Local shelf", view(s.local_grid)),
            pair("Reading statistics", "This device →"),
            pair(
                "Download limit",
                &if s.max_download_mib == 0 {
                    "Unlimited".into()
                } else {
                    format!("{} MiB", s.max_download_mib)
                },
            ),
            pair(
                "Sidebar width",
                &s.sidebar_width
                    .map_or_else(|| "Auto".into(), |n| format!("{n} cells")),
            ),
        ],
        3 => vec![
            pair(
                "Server",
                if s.bookorbit.server.is_empty() {
                    "Not set"
                } else {
                    &s.bookorbit.server
                },
            ),
            pair(
                "Username",
                if s.bookorbit.username.is_empty() {
                    "Not set"
                } else {
                    &s.bookorbit.username
                },
            ),
            pair(
                "Password",
                if s.bookorbit.password.is_some() {
                    "Stored · replace or clear"
                } else {
                    "Not set"
                },
            ),
            pair(
                "Password command",
                if s.bookorbit.password_command.is_some() {
                    "Configured · replace or clear"
                } else {
                    "Not set"
                },
            ),
            pair("Account shelf", view(!s.bookorbit.list_view)),
        ],
        _ => Vec::new(),
    }
}
pub fn show(term: &mut AppTerminal, start: Open, ctx: Context) -> io::Result<()> {
    let mut tab = if start == Open::Help { 4 } else { 0 };
    let mut state = ListState::default().with_selected(Some(0));
    let mut status = String::new();
    let mut help_offset = 0usize;
    let mut prefs = Settings::load()?;
    loop {
        let labels = rows(tab, &prefs);
        let guide = if tab == 4 {
            help::entries(ctx, &prefs)
        } else {
            vec![]
        };
        let mut help_max = 0;
        let mut tabs = [Rect::default(); 5];
        let mut back = Rect::default();
        let mut body = Rect::default();
        term.draw(|f| {
            let area = crate::centered(f.area(), 82, if tab==4{24}else{(labels.len() as u16+12).max(16)});
            let block = appearance::block().title(if tab == 4 {
                " Keyboard shortcuts "
            } else {
                " Settings "
            });
            let inner = block.inner(area);
            f.render_widget(block, area);
            if inner.height < 3 {
                crate::settings::paint_interface(f);
                return;
            }
            let compact = inner.width < 52;
            let mut x = inner.x;
            for (i, label) in TABS.iter().enumerate() {
                let s = if compact {
                    ["Style", "Read", "Lib", "Orbit", "Keys"][i]
                } else {
                    label
                };
                let w = (s.len() as u16 + 2).min(inner.right().saturating_sub(x));
                let r = Rect::new(x, inner.y, w, 1);
                tabs[i] = r;
                let style = if i == tab {
                    Style::new().cyan().reversed().bold()
                } else {
                    Style::new().cyan()
                };
                f.render_widget(Paragraph::new(format!(" {s} ")).style(style), r);
                x += w;
            }
            let gap = u16::from(inner.height >= 8);
            body = Rect::new(
                inner.x,
                inner.y + 1 + gap,
                inner.width,
                inner
                    .height
                    .saturating_sub(2 + gap + u16::from(!status.is_empty())),
            );
            if tab==4 { help_max=help::draw(f,body,&guide,&mut help_offset); } else {
            f.render_stateful_widget(
                List::new(labels.iter().map(String::as_str))
                    .highlight_style(Style::new().reversed()),
                body,
                &mut state,
            );
            }
            if tab<4 && body.height>=labels.len() as u16+4 {
                // Explain the selected control where the eye already is. Reserve the
                // last row for the same navigation hints on every tab.
                let note=match (tab,state.selected().unwrap_or(0)) {
                    (0,0)=>"Changes menus, the library and the dashboard. The reader has its own palette.",
                    (0,1)=>"Transparent lets the terminal show through. Opaque fills the page with the current palette.",
                    (0,2)=>"Choose the outline of panels, dialogs, covers and fallback book jackets.",
                    (0,_)=>"Auto uses your terminal’s background. Set a hex color to override Terminal paper.",
                    (1,0)=>"Preview light and dark book paper separately from the library interface.",
                    (1,1)=>"Zen shows just the page. Press Z while reading to bring the controls back.",
                    (1,2)=>"Preferred width of each text column, in terminal cells. A narrow window uses one page.",
                    (1,3)=>"Space between facing text pages. The W panel also lets you adjust this while reading.",
                    (1,4)=>"Reflow repairs words split across printed lines. Preserve keeps the book’s original breaks.",
                    (1,6)=>"Default for newly opened books: one page, a spread or continuous scroll.",
                    (1,8)=>"Width of the contents panel. Drag its right edge in a book, or enter cells here.",
                    (1,7)=>"Indent the first line of a text paragraph by 0–8 cells. Wrapped lines keep their left edge.",
                    (1,_)=>"Display embedded illustrations on the page or open them from I when you need them.",
                    (2,0)=>"Choose the library used by the folder browser. Searches also include its subfolders.",
                    (2,1)=>"Open local folders as a list or a grid of covers.",
                    (2,4)=>"Width of BookOrbit navigation. Drag its right edge, or enter cells here.",
                    (2,3)=>"Maximum download size for BookOrbit and OPDS. Enter MiB or 0 for unlimited.",
                    (2,_)=>"Editions, completed sessions and time spent in books on this device, across all sources.",
                    (3,0)|(3,1)=>"Used at your next sign-in. Saved credentials belong to this server and username.",
                    (3,2)=>"Store a password in your private config, or use a password command instead.",
                    (3,3)=>"Run your password manager at sign-in. BOTUI reads the first line of standard output.",
                    _=>"Open the BookOrbit shelf as a list or a grid of covers.",
                };
                let y=body.y+labels.len() as u16+1;
                f.render_widget(Paragraph::new(note).dim().wrap(ratatui::widgets::Wrap{trim:true}),Rect::new(body.x+1,y,body.width.saturating_sub(2),body.bottom()-y));
            }
            if !status.is_empty() {
                f.render_widget(
                    Paragraph::new(status.as_str()).yellow(),
                    Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
                );
            }
            back = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(10), 1);
            f.render_widget(Paragraph::new("[Esc Back]").cyan(), back);
            f.render_widget(
                Paragraph::new(if tab == 4 {
                    "  Tab Sections   ↑↓ Scroll"
                } else {
                    "  Tab Sections   ↑↓ Choose   Enter Edit"
                })
                .dim(),
                Rect::new(back.right(), back.y, inner.right() - back.right(), 1),
            );
            crate::settings::paint_interface(f);
        })?;
        let mut select = None;
        let mut opened = None;
        let delta = match crate::settings::read_event()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(2) => return Ok(()),
                KeyCode::Char('?') | KeyCode::F(1) => {
                    select = Some(4);
                    0
                }
                KeyCode::Tab | KeyCode::Right => {
                    select = Some((tab + 1) % 5);
                    0
                }
                KeyCode::BackTab | KeyCode::Left => {
                    select = Some((tab + 4) % 5);
                    0
                }
                KeyCode::Up | KeyCode::Char('k') => -1,
                KeyCode::Down | KeyCode::Char('j') => 1,
                KeyCode::PageUp => -(body.height as isize),
                KeyCode::PageDown => body.height as isize,
                KeyCode::Home => isize::MIN,
                KeyCode::End => isize::MAX,
                KeyCode::Enter | KeyCode::Char(' ') => {
                    opened = state.selected();
                    0
                }
                _ => continue,
            },
            Event::Mouse(m) => {
                let pos = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) if back.contains(pos) => return Ok(()),
                    MouseEventKind::Down(MouseButton::Left) => {
                        select = tabs.iter().position(|r| r.contains(pos));
                        if body.contains(pos) {
                            opened = Some(state.offset() + usize::from(m.row - body.y));
                        }
                        0
                    }
                    MouseEventKind::ScrollUp => -1,
                    MouseEventKind::ScrollDown => 1,
                    _ => continue,
                }
            }
            _ => continue,
        };
        if let Some(i) = select {
            tab = i;
            help_offset = 0;
            state = ListState::default().with_selected(Some(0));
            status.clear();
            continue;
        }
        if tab == 4 {
            help_offset = help_offset.saturating_add_signed(delta).min(help_max);
            continue;
        }
        if let Some(i) = opened.filter(|&i| i < labels.len() && tab != 4) {
            state.select(Some(i));
            status = match edit::setting(term, tab, i, &prefs) {
                Ok(_) => String::new(),
                Err(e) => e.to_string(),
            };
            prefs = Settings::load()?;
        } else {
            state.select(Some(
                state
                    .selected()
                    .unwrap_or(0)
                    .saturating_add_signed(delta)
                    .min(labels.len().saturating_sub(1)),
            ));
        }
    }
}
