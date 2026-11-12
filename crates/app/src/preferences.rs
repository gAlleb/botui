//! One settings and help surface for every source, including direct-file launches.
mod about;
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
        for (i, s) in ["[F2 Settings]", "[F1 Help]"].iter().enumerate() {
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
const TABS: [&str; 8] = [
    "Appearance",
    "Reader",
    "Library",
    "BookOrbit",
    "Keys",
    "About",
    "Legal",
    "Dictionaries",
];
const PARAGRAPH_SPACING: [&str; 4] = [
    "From book",
    "0 blank lines",
    "1 blank line",
    "2 blank lines",
];
fn spacing_index(prefs: &Settings) -> usize {
    prefs
        .paragraph_spacing
        .map_or(0, |n| 1 + usize::from(n.min(2)))
}
struct SettingRow {
    label: String,
    hint: &'static str,
}
fn rows(tab: usize, s: &Settings) -> Vec<SettingRow> {
    let pair = |k: &str, v: &str, hint| SettingRow {
        label: format!("{k}\t{v}"),
        hint,
    };
    let view = |grid| if grid { "Cover grid" } else { "List" };
    match tab {
        0 => vec![
            pair(
                "Interface theme",
                shell_theme::ALL[shell_theme::index(s.interface_theme.as_deref())].1,
                "Changes menus, the library and the dashboard. The reader has its own palette.",
            ),
            pair(
                "Background",
                if s.opaque_background {
                    "Opaque"
                } else {
                    "Transparent"
                },
                "Transparent lets the terminal show through. Opaque fills the page with the current palette.",
            ),
            pair(
                "Borders & covers",
                s.borders.label(),
                "Choose the outline of panels, dialogs, covers and fallback book jackets.",
            ),
            pair(
                "Solid terminal color",
                s.opaque_color
                    .as_deref()
                    .unwrap_or("Auto · terminal palette"),
                "Auto uses your terminal’s background. Set a hex color to override Terminal paper.",
            ),
        ],
        1 => vec![
            pair(
                "Reading theme",
                &crate::theme::Theme::from_id(s.theme.as_deref().unwrap_or("terminal")).name(),
                "Preview light and dark book paper separately from the library interface.",
            ),
            pair(
                "Reader view",
                if s.zen { "Zen" } else { "Contents sidebar" },
                "Zen shows just the page. Press Z while reading to bring the controls back.",
            ),
            pair(
                "Page width",
                &format!("{} cells", s.page_width.unwrap_or(78)),
                "Preferred width of each text column, in terminal cells. A narrow window uses one page.",
            ),
            pair(
                "Column gap",
                &format!("{} cells", s.column_gap.unwrap_or(4)),
                "Space between facing text pages. The Space l w panel also lets you adjust this while reading.",
            ),
            pair(
                "Printed line breaks",
                if s.preserve_printed_lines {
                    "Preserve"
                } else {
                    "Reflow"
                },
                "Reflow repairs words split across printed lines. Preserve keeps the book’s original breaks.",
            ),
            pair(
                "Illustrations",
                if s.hide_inline_images {
                    "Viewer only"
                } else {
                    "In text"
                },
                "Display embedded illustrations on the page or open them from I when you need them.",
            ),
            pair(
                "Default page layout",
                s.reading_layout.as_deref().unwrap_or("auto"),
                "Choose one page, a spread or continuous scroll for newly opened books.",
            ),
            pair(
                "First-line indent",
                &format!("{} cells", s.paragraph_indent.min(8)),
                "Indent the first line of a text paragraph by 0–8 cells. Wrapped lines keep their left edge.",
            ),
            pair(
                "Paragraph spacing",
                PARAGRAPH_SPACING[spacing_index(s)],
                "Press Enter to use the book’s spacing or choose 0, 1 or 2 blank lines between paragraphs.",
            ),
            pair(
                "Contents width",
                &s.contents_width
                    .map_or_else(|| "Auto".into(), |n| format!("{n} cells")),
                "Enter a width in cells, or drag the divider between the contents sidebar and the book text.",
            ),
            pair(
                "Zen progress",
                if s.zen_progress { "On" } else { "Off" },
                "Show the page and reading percentage at the bottom of zen mode. Toggle with %.",
            ),
            pair(
                "Mouse wheel step",
                &format!("{} line(s)", s.scroll_lines.unwrap_or(1).clamp(1, 12)),
                "Lines per wheel tick in continuous scroll, for text and PDF/DjVu. Arrow keys always move one line.",
            ),
            pair(
                "Space menu delay",
                &format!("{} ms", s.leader_delay_ms.unwrap_or(150)),
                "Delay before command hints appear. Space combinations always execute immediately. Set 0 for instant hints.",
            ),
            pair(
                "PDF / image theme",
                &s.page_theme_label(),
                "Original, Inverted or a named palette for PDF, DjVu and image-only EPUB. Space t previews and saves the same setting.",
            ),
            pair(
                "Comic background",
                &s.comic_theme().name(),
                "Background around CBZ/CBR pages. Artwork always keeps its original colors. Space t previews and saves this choice.",
            ),
        ],
        2 => vec![
            pair(
                "Local folder",
                &s.library_root
                    .as_deref()
                    .map_or_else(|| "Default".into(), |p| p.display().to_string()),
                "Choose the library used by the folder browser. Searches also include its subfolders.",
            ),
            pair(
                "Local shelf",
                view(s.local_grid),
                "Open local folders as a list or a grid of covers.",
            ),
            pair(
                "Reading statistics",
                "This device →",
                "Editions, completed sessions and time spent in books on this device, across all sources.",
            ),
            pair(
                "Download limit",
                &if s.max_download_mib == 0 {
                    "Unlimited".into()
                } else {
                    format!("{} MiB", s.max_download_mib)
                },
                "Maximum download size for BookOrbit and OPDS. Enter MiB or 0 for unlimited.",
            ),
            pair(
                "Sidebar width",
                &s.sidebar_width
                    .map_or_else(|| "Auto".into(), |n| format!("{n} cells")),
                "Enter a width in cells, or drag the divider between the navigation sidebar and the bookshelf.",
            ),
            pair(
                "OPDS catalogs",
                "Manage…",
                "Named catalogs, URLs and credentials saved in config.toml.",
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
                "Used at your next sign-in. Saved credentials belong to this server and username.",
            ),
            pair(
                "Username",
                if s.bookorbit.username.is_empty() {
                    "Not set"
                } else {
                    &s.bookorbit.username
                },
                "Used at your next sign-in. Saved credentials belong to this server and username.",
            ),
            pair(
                "Password",
                if s.bookorbit.password.is_some() {
                    "Stored · replace or clear"
                } else {
                    "Not set"
                },
                "Store a password in your private config, or use a password command instead.",
            ),
            pair(
                "Password command",
                if s.bookorbit.password_command.is_some() {
                    "Configured · replace or clear"
                } else {
                    "Not set"
                },
                "Run your password manager at sign-in. BOTUI reads the first line of standard output.",
            ),
            pair(
                "Account shelf",
                view(!s.bookorbit.list_view),
                "Open the BookOrbit shelf as a list or a grid of covers.",
            ),
        ],
        7 => crate::lookup::settings::rows(&crate::lookup::Config::from_settings(s))
            .into_iter()
            .map(|(label, hint)| SettingRow { label, hint })
            .collect(),
        _ => Vec::new(),
    }
}
pub fn show(term: &mut AppTerminal, start: Open, ctx: Context) -> io::Result<()> {
    let _palette = crate::settings::interface_dialogs();
    let mut tab = if start == Open::Help { 4 } else { 0 };
    let mut state = ListState::default().with_selected(Some(0));
    let mut status = String::new();
    let mut guide_view = help::Guide::default();
    let mut about_offset = 0usize;
    let mut prefs = Settings::load()?;
    loop {
        let labels = rows(tab, &prefs);
        let guide = if tab == 4 {
            help::entries(ctx, &prefs)
        } else {
            vec![]
        };
        let mut tabs = [Rect::default(); TABS.len()];
        let mut about_max = 0;
        let mut back = Rect::default();
        let mut body = Rect::default();
        term.draw(|f| {
            let area = crate::centered(f.area(), 106, 34);
            let block = appearance::block().title(if tab == 4 {
                " Keyboard shortcuts "
            } else if tab == 5 {
                " About BOTUI "
            } else if tab == 6 {
                " Legal notices "
            } else {
                " Settings "
            });
            let inner = block.inner(area).inner(ratatui::layout::Margin::new(1, 0));
            f.render_widget(block, area);
            if inner.height < 3 {
                crate::settings::paint_interface(f);
                return;
            }
            let compact = inner.width < 63;
            let mut x = inner.x;
            let mut y = inner.y;
            for i in [0, 1, 2, 3, 7, 4, 5, 6] {
                let label = TABS[i];
                let s = if compact {
                    [
                        "Style", "Read", "Lib", "Orbit", "Keys", "About", "Legal", "Words",
                    ][i]
                } else {
                    label
                };
                let w = (s.len() as u16 + 2).min(inner.width);
                if x + w > inner.right() {
                    x = inner.x;
                    y += 1;
                }
                if y >= inner.bottom().saturating_sub(1) {
                    break;
                }
                let r = Rect::new(x, y, w, 1);
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
                y + 1 + gap,
                inner.width,
                inner
                    .height
                    .saturating_sub(y - inner.y + 2 + gap + u16::from(!status.is_empty())),
            );
            match tab {
                6 => about_max = about::legal(f, body, &mut about_offset),
                5 => about_max = about::draw(f, body, &mut about_offset),
                4 => guide_view.draw(f, body, &guide),
                _ => {
                    f.render_stateful_widget(
                        List::new(crate::prompt::columns(
                            &labels
                                .iter()
                                .map(|row| row.label.clone())
                                .collect::<Vec<_>>(),
                            body.width,
                        ))
                        .highlight_style(Style::new().reversed()),
                        body,
                        &mut state,
                    );
                    if body.height >= labels.len() as u16 + 4
                        && let Some(row) = state.selected().and_then(|i| labels.get(i))
                    {
                        let y = body.y + labels.len() as u16 + 1;
                        f.render_widget(
                            Paragraph::new(row.hint)
                                .dim()
                                .wrap(ratatui::widgets::Wrap { trim: true }),
                            Rect::new(
                                body.x + 1,
                                y,
                                body.width.saturating_sub(2),
                                body.bottom() - y,
                            ),
                        );
                    }
                }
            }
            if !status.is_empty() {
                f.render_widget(
                    Paragraph::new(status.as_str()).yellow(),
                    Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
                );
            }
            let back_label = if tab == 4 && guide_view.filtering() {
                "[Esc Clear]"
            } else {
                "[Esc Back]"
            };
            back = Rect::new(
                inner.x,
                inner.bottom() - 1,
                inner.width.min(back_label.len() as u16),
                1,
            );
            f.render_widget(Paragraph::new(back_label).cyan(), back);
            f.render_widget(
                Paragraph::new(if matches!(tab, 5 | 6) {
                    "  Tab Sections   ↑↓ Scroll"
                } else if tab == 4 {
                    if inner.width >= 58 {
                        "  ↑↓ Scroll   [ / ] Section   Tab Switch tab"
                    } else {
                        "  ↑↓ Scroll  [ ] Section"
                    }
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
        let event = crate::settings::read_event()?;
        if tab == 4 && guide_view.filter_input(&event) {
            continue;
        }
        let delta = match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(2) => return Ok(()),
                KeyCode::Char('?') | KeyCode::F(1) => {
                    select = Some(4);
                    0
                }
                KeyCode::Tab | KeyCode::Right => {
                    let order = [0, 1, 2, 3, 7, 4, 5, 6];
                    select = Some(order[(order.iter().position(|&n| n == tab).unwrap() + 1) % 8]);
                    0
                }
                KeyCode::BackTab | KeyCode::Left => {
                    let order = [0, 1, 2, 3, 7, 4, 5, 6];
                    select = Some(order[(order.iter().position(|&n| n == tab).unwrap() + 7) % 8]);
                    0
                }
                KeyCode::Char('[' | ']') if tab == 4 => {
                    guide_view.section(k.code == KeyCode::Char(']'));
                    continue;
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
                    MouseEventKind::Down(MouseButton::Left) if back.contains(pos) => {
                        if tab == 4 && guide_view.filtering() {
                            guide_view.clear_filter();
                            continue;
                        }
                        return Ok(());
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if tab == 4 && guide_view.click(pos) {
                            continue;
                        }
                        select = tabs.iter().position(|r| r.contains(pos));
                        if body.contains(pos) && (tab < 4 || tab == 7) {
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
            guide_view = help::Guide::default();
            about_offset = 0;
            state = ListState::default().with_selected(Some(0));
            status.clear();
            continue;
        }
        if (4..=6).contains(&tab) {
            if matches!(tab, 5 | 6) {
                about_offset = about_offset.saturating_add_signed(delta).min(about_max);
            } else {
                guide_view.scroll(delta);
            }
            continue;
        }
        if let Some(i) = opened.filter(|&i| i < labels.len() && tab != 4) {
            state.select(Some(i));
            status = match if tab == 7 {
                crate::lookup::settings::open(term, i)
            } else if tab == 2 && i == 5 {
                crate::opds_settings::configure(term)
            } else {
                edit::setting(term, tab, i, &prefs)
            } {
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
