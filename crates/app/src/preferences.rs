//! One settings and help surface for every source, including direct-file launches.
mod about;
mod book;
pub(crate) use book::Kind as BookKind;
mod edit;
mod help;
use crate::{AppTerminal, appearance, settings::Settings, shell_theme};
use botui_core::PaginationMode;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
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
    pub const LABELS: [&str; 2] = ["[F2 Settings]", "[F1/? Help]"];

    pub fn draw(&mut self, f: &mut Frame, area: Rect) -> u16 {
        self.0 = [Rect::default(); 2];
        let mut right = area.x;
        for (i, s) in Self::LABELS.into_iter().enumerate() {
            let x = right.saturating_add(u16::from(i != 0));
            let len = s.len() as u16;
            if area.height == 0 || area.right().saturating_sub(x) < len {
                break;
            }
            let r = Rect::new(x, area.y, len, 1);
            self.0[i] = r;
            f.render_widget(Paragraph::new(s).cyan(), r);
            right = r.right();
        }
        right
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
const SYNCTEX_MODES: [&str; 2] = ["Custom command", "Neovim · reuse window"];
const PAGINATION_MODES: [PaginationMode; 2] = [PaginationMode::Chapter, PaginationMode::KeepWord];
fn pagination_label(mode: PaginationMode) -> &'static str {
    match mode {
        PaginationMode::Chapter => "From chapter start",
        PaginationMode::KeepWord => "Keep first word",
    }
}
fn spacing_index(prefs: &Settings) -> usize {
    prefs
        .paragraph_spacing
        .map_or(0, |n| 1 + usize::from(n.min(2)))
}
enum Item {
    Global(usize),
    Book(book::Field),
}
struct SettingRow {
    section: Option<&'static str>,
    label: String,
    hint: &'static str,
}
impl SettingRow {
    fn section(mut self, name: &'static str) -> Self {
        self.section = Some(name);
        self
    }
}

fn rows(tab: usize, s: &Settings) -> Vec<SettingRow> {
    let pair = |k: &str, v: &str, hint| SettingRow {
        section: None,
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
        1 => {
            let mut rows = vec![
                pair(
                    "Reader view",
                    if s.zen { "Zen" } else { "Contents sidebar" },
                    "Zen shows just the page. Press Z while reading to bring the controls back.",
                ).section("Reading controls"),
                pair(
                    "Default page layout",
                    s.reading_layout.as_deref().unwrap_or("auto"),
                    "Choose one page, a spread or continuous scroll for newly opened books.",
                ),
                pair(
                    "Contents width",
                    &s.contents_width
                        .map_or_else(|| "Auto".into(), |n| format!("{n} cells")),
                    "Enter a width in cells, or drag the divider between the contents sidebar and the book text.",
                ),
                pair(
                    "Zen frame & progress",
                    if s.zen_progress { "On" } else { "Off" },
                    "Show the reading frame, page and percentage in zen mode. Toggle with Space p.",
                ),
                pair(
                    "Full-height Zen",
                    if s.zen_full_height { "On" } else { "Off" },
                    "Use every terminal row in Zen. Selection controls overlay the bottom row. Toggle with Space Z.",
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
                    "Reading theme",
                    &crate::theme::Theme::from_id(s.theme.as_deref().unwrap_or("terminal")).name(),
                    "Preview light and dark book paper separately from the library interface.",
                ).section("Text books"),
                pair(
                    "Page width",
                    &book::width_label(s.page_width.unwrap_or_default()),
                    "Preferred width of each text column, in terminal cells. A narrow window uses one page.",
                ),
                pair(
                    "Column gap",
                    &format!("{} cells", s.column_gap.unwrap_or(4)),
                    "Space between facing text pages. The Space l w panel also lets you adjust this while reading.",
                ),
                pair(
                    "Text alignment",
                    s.text_alignment.label(),
                    "Align text, headings and display formulas inside the current page width.",
                ),
                pair(
                    "Heading rules",
                    if s.heading_rules { "On" } else { "Off" },
                    "Double line under H1 and single line under H2.",
                ),
                pair(
                    "First-line indent",
                    &format!("{} cells", s.paragraph_indent.min(8)),
                    "Indent the first line by 0–8 cells. Centered EPUB paragraphs keep the book's indent.",
                ),
                pair(
                    "Compact blocks",
                    if s.compact_spacing() { "On" } else { "Off" },
                    "On keeps book spacing in addresses, centered/right-aligned and blank paragraphs. Off applies Paragraph spacing there too.",
                ),
                pair(
                    "Paragraph spacing",
                    PARAGRAPH_SPACING[spacing_index(s)],
                    "Use the book’s spacing or choose 0, 1 or 2 blank lines between paragraphs.",
                ),
                pair(
                    "Word hyphenation (EN/RU)",
                    if s.hyphenation { "On" } else { "Off" },
                    "Use dictionary hyphens in English and Russian prose.",
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
                    "Pagination",
                    pagination_label(s.pagination_mode),
                    "In Single and Spread, use pages from the start of the chapter or keep the current word at the top.",
                ),
                pair(
                    "PDF reading theme",
                    &s.page_theme().name(),
                    "Palette for PDF, DjVu and image-only EPUB margins and dialogs.",
                ).section("PDF & DjVu"),
                pair(
                    "PDF page colors",
                    s.page_colors().label(),
                    "Keep document colors, invert the original page, or apply the reading theme to paper and ink.",
                ),
                pair(
                    "Comic background",
                    &s.comic_theme().name(),
                    "Background around CBZ/CBR pages. Artwork always keeps its original colors. Space t previews and saves this choice.",
                ).section("Comics"),
                pair(
                    "SyncTeX mode",
                    SYNCTEX_MODES
                        [usize::from(matches!(s.synctex_mode, crate::synctex::Mode::Neovim))],
                    "Custom runs your editor command. Neovim opens source and PDF windows when needed, then reuses them for jumps both ways.",
                ).section("SyncTeX"),
            ];
            if matches!(s.synctex_mode, crate::synctex::Mode::Neovim) {
                rows.extend([
                    pair(
                        "SyncTeX Neovim terminal",
                        &if s.synctex_nvim_terminal.is_empty() {
                            "Auto".into()
                        } else {
                            crate::synctex::editor_command(&s.synctex_nvim_terminal)
                        },
                        "Neovim mode: terminal for the first editor window only, e.g. kitty, alacritty -e or foot. Leave blank for Auto.",
                    ),
                    pair(
                        "SyncTeX BOTUI terminal",
                        &if s.synctex_botui_terminal.is_empty() {
                            "Auto".into()
                        } else {
                            crate::synctex::editor_command(&s.synctex_botui_terminal)
                        },
                        "Neovim mode: terminal for the first PDF window only. Set independently from Neovim; blank means Auto. Open windows are reused.",
                    ),
                ]);
            } else {
                rows.push(pair(
                    "SyncTeX editor command",
                    &if s.synctex_editor.is_empty() {
                        "Show source location".into()
                    } else {
                        crate::synctex::editor_command(&s.synctex_editor)
                    },
                    "Used in Custom command mode; kept when switching modes. Use {file}, {line} and optionally {column}. Blank shows the source location.",
                ));
            }
            rows
        }
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
            .map(|(label, hint)| SettingRow {
                label,
                hint,
                section: None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
pub fn show(term: &mut AppTerminal, start: Open, ctx: Context) -> io::Result<()> {
    show_book(term, start, ctx, None)
}

pub fn show_book(
    term: &mut AppTerminal,
    start: Open,
    ctx: Context,
    edition: Option<(&botui_store::LocalState, BookKind)>,
) -> io::Result<()> {
    let _palette = crate::settings::interface_dialogs();
    let mut tab = if start == Open::Help { 4 } else { 0 };
    let mut state = ListState::default().with_selected(Some(0));
    let mut status = String::new();
    let mut guide_view = help::Guide::default();
    let mut about_offset = 0usize;
    let mut prefs = Settings::load()?;
    let book_open = matches!(ctx, Context::Text | Context::Scan);
    let mut local = crate::settings::book::BookSettings::load(edition.map(|(store, _)| store))?;
    loop {
        let global = rows(tab, &prefs);
        let (items, labels): (Vec<_>, Vec<_>) = if tab == 1
            && let Some((_, kind)) = edition
        {
            book::rows(kind, &local, &prefs, global).into_iter().unzip()
        } else if tab == 1 && book_open {
            (Vec::new(), Vec::new())
        } else {
            global
                .into_iter()
                .enumerate()
                .map(|(i, row)| (Item::Global(i), row))
                .unzip()
        };
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
            } else if tab == 1 && book_open && edition.is_none() {
                " Book settings unavailable · cannot access local reading state "
            } else if tab == 1 && !book_open {
                " Settings · defaults for all books "
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
                    appearance::CURRENT_SECTION
                } else {
                    appearance::ACTION
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
                    let columns = crate::prompt::columns(
                        &labels
                            .iter()
                            .map(|row| row.label.clone())
                            .collect::<Vec<_>>(),
                        body.width,
                    );
                    let selected = state.selected();
                    let items = labels
                        .iter()
                        .zip(columns)
                        .enumerate()
                        .map(|(i, (row, label))| {
                            let mut lines = Vec::with_capacity(2);
                            if let Some(section) = row.section {
                                lines.push(ratatui::text::Line::styled(
                                    section,
                                    appearance::CURRENT_SECTION,
                                ));
                            }
                            let mut line = ratatui::text::Line::raw(label);
                            if selected == Some(i) {
                                line = line.style(appearance::SELECTION);
                            }
                            lines.push(line);
                            ratatui::widgets::ListItem::new(lines)
                        });
                    let rows =
                        labels.len() + labels.iter().filter(|row| row.section.is_some()).count();
                    let hint_space = if tab == 1 && body.height >= 12 { 2 } else { 0 };
                    let mut list_area = body;
                    list_area.height = list_area.height.saturating_sub(hint_space);
                    f.render_stateful_widget(List::new(items), list_area, &mut state);
                    if (hint_space > 0 || body.height >= rows as u16 + 4)
                        && let Some(row) = state.selected().and_then(|i| labels.get(i))
                    {
                        let y = (body.y + rows as u16 + 1).min(list_area.bottom());
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
            let footer_x = back.right();
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
                Rect::new(footer_x, back.y, inner.right() - footer_x, 1),
            );
            crate::settings::paint_interface(f);
        })?;
        let mut select = None;
        let mut opened = None;
        let event = if tab == 4 && guide_view.filtering() {
            crate::settings::read_event()?
        } else {
            crate::settings::read_dialog()?
        };
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
                            let mut y = body.y;
                            for (i, row) in labels.iter().enumerate().skip(state.offset()) {
                                y += u16::from(row.section.is_some());
                                if y == m.row
                                    && (tab != 1 || body.height < 12 || y < body.bottom() - 2)
                                {
                                    opened = Some(i);
                                    break;
                                }
                                y += 1;
                            }
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
            let result = match items[i] {
                Item::Book(field) => match edition {
                    Some((store, _)) => book::edit(term, store, field, &prefs),
                    None => Err(io::Error::other("Book settings are unavailable")),
                },
                Item::Global(index) => match (tab, index) {
                    (7, _) => crate::lookup::settings::open(term, index),
                    (2, 5) => crate::opds_settings::configure(term),
                    _ => edit::setting(term, tab, index, &prefs),
                },
            };
            status = result.err().map(|err| err.to_string()).unwrap_or_default();
            prefs = Settings::load()?;
            local = crate::settings::book::BookSettings::load(edition.map(|(store, _)| store))?;
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
