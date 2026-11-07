pub(crate) mod bookmarks;
mod layout;
mod links;
mod mark_list;
mod navigation;
pub(crate) mod notes;
mod resources;
mod search;
pub(crate) mod theme_picker;
use navigation::{Edit, Kind};

use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use botui_core::{Document, LayoutMode, Pager, ReflowOptions};
use botui_store::{LocalState, Saved};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Modifier,
    widgets::{Block, Clear, Paragraph, Row, Table, TableState},
};

use crate::{AppTerminal, centered, theme::Theme};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Menu,
    Zen,
    Background,
    Borders,
    Sidebar(Edit),
    Focus,
    JumpBack,
    JumpForward,
    FindAgain(bool),
    SearchResults,
    Note,
    Export,
    Statistics,
    Resources,
    VisibleLinks,
    Follow(usize),
    ReturnLink,
    KeepLink,
    Settings,
    Help,
    Resume,
    Previous,
    Next,
    Single,
    Spread,
    Scroll,
    Lines(i8),
    Theme,
    Pick(theme_picker::Edit),
    Navigate(Kind),
    NavEdit(Edit),
    Mark,
    Bookmarks,
    Margin(i8),
    Gap(i8),
    Layout,
    Close,
    Up,
    Down,
    Resize,
}

const COMMANDS: [Action; 28] = [
    Action::Resume,
    Action::Previous,
    Action::Next,
    Action::Single,
    Action::Spread,
    Action::Scroll,
    Action::Theme,
    Action::Navigate(Kind::Contents),
    Action::Navigate(Kind::Search),
    Action::FindAgain(true),
    Action::FindAgain(false),
    Action::SearchResults,
    Action::Mark,
    Action::Bookmarks,
    Action::Layout,
    Action::Zen,
    Action::Background,
    Action::Borders,
    Action::Note,
    Action::Navigate(Kind::Notes),
    Action::Export,
    Action::Statistics,
    Action::Resources,
    Action::JumpBack,
    Action::JumpForward,
    Action::Settings,
    Action::Help,
    Action::Close,
];

struct Reader {
    zen: bool,
    opaque: bool,
    sidebar: navigation::Navigation,
    sidebar_rect: Rect,
    splitter: crate::splitter::Splitter,
    footer: [Rect; 6],
    remote_marked: bool,
    sidebar_focus: bool,
    links: links::Links,
    history: Vec<botui_core::Anchor>,
    future: Vec<botui_core::Anchor>,
    source: std::path::PathBuf,
    illustrations: Option<crate::shelf::covers::Covers>,
    doc: Document,
    pager: Pager,
    mode: LayoutMode,
    layout_panel: Option<layout::Panel>,
    page_width: u16,
    gap: u16,
    theme: Theme,
    palette: Option<theme_picker::Picker>,
    menu: Option<TableState>,
    menu_rows: Rect,
    nav: Option<navigation::Navigation>,
    search: search::Search,
    marks: Vec<botui_core::Anchor>,
    margin: i8,
    bindings: std::collections::HashMap<char, Action>,
    dirty: bool,
    finished: bool,
}

impl Reader {
    fn new(doc: Document, area: Rect, saved: Option<Saved>, source: &Path) -> Self {
        let settings = crate::settings::Settings::load().unwrap_or_default();
        let mode = if saved.is_none() && settings.reading_layout.as_deref() == Some("single") {
            LayoutMode::Single
        } else if (saved.is_none() && settings.reading_layout.as_deref() == Some("scroll"))
            || saved.as_ref().is_some_and(|s| s.scrolling)
        {
            LayoutMode::Scroll
        } else if saved.as_ref().is_some_and(|s| !s.spread) {
            LayoutMode::Single
        } else {
            LayoutMode::Spread
        };
        let margin = saved.as_ref().map_or(0, |s| s.margin);
        let page_width = settings
            .page_width
            .unwrap_or_else(|| 78u16.saturating_add_signed(-6 * i16::from(margin)))
            .clamp(32, 240);
        let gap = settings.column_gap.unwrap_or(4).min(32);
        let (text, mode_for_size, _) = layout::geometry(area, mode, page_width, gap, settings.zen);
        let mut pager = Pager::with_options(
            &doc,
            text.width,
            text.height,
            mode_for_size,
            ReflowOptions {
                repair_printed: !settings.preserve_printed_lines,
                inline_images: !settings.hide_inline_images,
                paragraph_indent: settings.paragraph_indent,
            },
        );
        let marks = saved.as_ref().map(|s| s.marks.clone()).unwrap_or_default();
        if let Some(state) = &saved {
            pager.seek(state.anchor);
        }
        let theme = settings
            .theme
            .as_deref()
            .or_else(|| saved.as_ref().map(|s| s.theme.as_str()))
            .map_or(Theme::TERMINAL, Theme::from_id);
        let links = links::Links::new(&doc);
        let sidebar = navigation::Navigation::new(Kind::Contents, &doc, pager.anchor());
        Self {
            zen: settings.zen,
            opaque: settings.opaque_background,
            sidebar,
            sidebar_rect: Rect::default(),
            splitter: crate::splitter::Splitter::new(
                crate::splitter::Pane::Contents,
                settings.contents_width,
            ),
            footer: [Rect::default(); 6],
            remote_marked: false,
            sidebar_focus: false,
            links,
            history: vec![],
            future: vec![],
            source: source.into(),
            illustrations: None,
            doc,
            pager,
            mode,
            page_width,
            gap,
            layout_panel: None,
            theme,
            palette: None,
            menu: None,
            menu_rows: Rect::default(),
            nav: None,
            search: Default::default(),
            marks,
            margin,
            bindings: Default::default(),
            dirty: false,
            finished: saved.as_ref().is_some_and(|s| s.finished),
        }
    }

    fn draw(&mut self, frame: &mut Frame, notice: Option<&str>) {
        let screen = frame.area();
        let mut page = screen;
        self.links.clear();
        if !self.zen || self.links.visiting() {
            page.height = page.height.saturating_sub(1);
        }
        (self.sidebar_rect, page) = self.splitter.layout(page, !self.zen && screen.width >= 78);
        if self.sidebar_rect.is_empty() {
            self.sidebar_focus = false;
        }
        let (text, actual, gap) =
            layout::geometry(page, self.mode, self.page_width, self.gap, self.zen);
        self.pager
            .relayout_with_gap(&self.doc, text.width, text.height, actual, gap);
        let mut style = self
            .palette
            .as_ref()
            .map_or(self.theme, |p| p.preview())
            .style();
        if !crate::settings::opaque() {
            style.bg = Some(ratatui::style::Color::Reset);
        }
        frame.render_widget(Block::new().style(style), screen);
        if let Some(p) = &mut self.illustrations {
            p.collect();
        }
        let mut figures = Vec::new();
        for c in 0..self.pager.columns() {
            let x = text
                .x
                .saturating_add(c as u16 * (self.pager.column_width() + self.pager.gutter()));
            for (y, line) in self
                .pager
                .lines(c)
                .iter()
                .take(usize::from(text.height))
                .enumerate()
            {
                let rect = Rect::new(
                    x + line.indent,
                    text.y + y as u16,
                    self.pager.column_width() - line.indent,
                    1,
                );
                if let Some(image) = line.image {
                    if let Some((id, area)) = figures.last_mut() as Option<&mut (usize, Rect)>
                        && *id == image
                        && area.x == rect.x
                        && area.bottom() == rect.y
                    {
                        area.height += 1;
                    } else {
                        figures.push((image, rect));
                    }
                    continue;
                }
                let mut para = Paragraph::new(self.search.line(line)).style(if line.table_header {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                });
                if line.heading {
                    para = para.centered().style(style.add_modifier(Modifier::BOLD));
                }
                frame.render_widget(para, rect);
                self.links.mark_line(frame, &self.doc, line, rect);
            }
        }
        for (id, area) in figures {
            self.links.image(area, id);
            let entry = &self.doc.references[id];
            if let botui_core::Target::Image(member) = &entry.target {
                let pictures = self
                    .illustrations
                    .get_or_insert_with(|| crate::shelf::covers::Covers::with_capacity(8));
                let member = member.clone();
                let path = self.source.clone();
                pictures.request_with(
                    id as u64,
                    move || Ok(botui_formats::illustration(&path, &member).ok()),
                    crate::cover::Picture::inline,
                );
                if let Some(picture) = pictures.image(id as u64) {
                    picture.draw(frame, area, &crate::cover::graphics());
                } else {
                    frame.render_widget(
                        Paragraph::new(format!("Illustration · {}", entry.label))
                            .centered()
                            .style(style.add_modifier(Modifier::DIM)),
                        Rect {
                            y: area.y + area.height / 2,
                            height: 1,
                            ..area
                        },
                    );
                }
            }
        }
        if !self.zen {
            if !self.sidebar_rect.is_empty() {
                if !self.sidebar_focus {
                    self.sidebar.follow(self.pager.anchor());
                }
                self.sidebar.draw_at(frame, style, self.sidebar_rect, false);
            }
            let foot = Rect::new(
                screen.x,
                screen.bottom().saturating_sub(1),
                screen.width,
                screen.height.min(1),
            );
            let mut right = foot.right();
            for (i, label) in [
                " Z Zen ",
                " O Contents ",
                " M Menu ",
                " F2 Settings ",
                " ? Help ",
                if self.links.has_visible() {
                    " F6 Links "
                } else {
                    ""
                },
            ]
            .into_iter()
            .enumerate()
            .rev()
            {
                let width = (label.len() as u16).min(right - foot.x);
                right -= width;
                let r = Rect::new(right, foot.y, width, foot.height);
                self.footer[i] = r;
                frame.render_widget(
                    Paragraph::new(label).style(style.add_modifier(Modifier::BOLD)),
                    r,
                );
            }
            let count = format!(
                " {}{} / {} ",
                if self.mode == LayoutMode::Scroll {
                    "Scroll · "
                } else {
                    ""
                },
                self.pager.page_number(),
                self.pager.page_count()
            );
            let cw = (count.len() as u16).min(right - foot.x);
            frame.render_widget(
                Paragraph::new(count).style(style),
                Rect::new(right - cw, foot.y, cw, foot.height),
            );
            frame.render_widget(
                Paragraph::new(format!(" {}", self.doc.title))
                    .style(style.add_modifier(Modifier::DIM)),
                Rect::new(foot.x, foot.y, right - cw - foot.x, foot.height),
            );
        }
        if self.links.visiting() {
            let footer = Rect::new(
                screen.x,
                screen.bottom().saturating_sub(1),
                screen.width.min(37),
                screen.height.min(1),
            );
            frame.render_widget(Clear, footer);
            self.links.controls(frame, footer, style);
        }
        if let Some(panel) = &mut self.layout_panel {
            panel.draw(
                frame,
                style,
                self.pager.column_width(),
                self.page_width,
                self.gap,
                (self.pager.columns() == 2).then(|| self.pager.gutter()),
            );
            return;
        }
        if let Some(picker) = &mut self.palette {
            picker.draw(frame);
            return;
        }
        if let Some(nav) = &mut self.nav {
            nav.draw(frame, style);
            return;
        }
        let Some(list) = &mut self.menu else { return };
        let popup = centered(
            screen,
            54,
            COMMANDS.len() as u16 + 3 + u16::from(notice.is_some()),
        );
        frame.render_widget(Clear, popup);
        let block = crate::appearance::block().title(" Reading ").style(style);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let header = inner.height.min(1);
        let status = u16::from(notice.is_some() && inner.height >= 4);
        let counter = format!("{}/{}", self.pager.page_number(), self.pager.page_count());
        let count_width = inner.width.min(counter.len() as u16);
        frame.render_widget(
            Paragraph::new(self.doc.title.as_str()),
            Rect::new(
                inner.x,
                inner.y,
                inner.width.saturating_sub(count_width + 1),
                header,
            ),
        );
        frame.render_widget(
            Paragraph::new(counter),
            Rect::new(inner.right() - count_width, inner.y, count_width, header),
        );
        self.menu_rows = Rect::new(
            inner.x,
            inner.y + header,
            inner.width,
            inner.height.saturating_sub(header + status),
        );
        let rows = COMMANDS.iter().map(|action| {
            let marked = matches!(
                (action, self.mode),
                (Action::Single, LayoutMode::Single)
                    | (Action::Spread, LayoutMode::Spread)
                    | (Action::Scroll, LayoutMode::Scroll)
            );
            let (label, shortcut): (std::borrow::Cow<'_, str>, _) = match action {
                Action::Resume => ("Resume".into(), "Esc"),
                Action::Previous => ("Previous page".into(), "←"),
                Action::Next => ("Next page".into(), "→"),
                Action::Single => ("Single page".into(), "1"),
                Action::Spread => ("Two-page spread".into(), "2"),
                Action::Scroll => ("Continuous scroll".into(), "3"),
                Action::Theme => (format!("Theme · {}", self.theme.name()).into(), "T"),
                Action::Navigate(Kind::Contents) => ("Contents".into(), "O"),
                Action::Navigate(Kind::Search) => ("Find in book".into(), "/"),
                Action::SearchResults => ("Reopen search results".into(), "F5"),
                Action::FindAgain(true) => ("Next search match".into(), "F3"),
                Action::FindAgain(false) => ("Previous search match".into(), "F4"),
                Action::Mark => (
                    (if self.remote_marked || self.marks.contains(&self.pager.anchor()) {
                        "Remove bookmark"
                    } else {
                        "Add bookmark"
                    })
                    .into(),
                    "B",
                ),
                Action::Bookmarks => ("Bookmarks".into(), "G"),
                Action::Layout => ("Page width & column gap".into(), "W"),
                Action::Zen => (
                    (if self.zen {
                        "Show contents sidebar"
                    } else {
                        "Enter zen mode"
                    })
                    .into(),
                    "Z",
                ),
                Action::Borders => (
                    format!(
                        "Borders · {}",
                        crate::appearance::BordersStyle::current().label()
                    )
                    .into(),
                    "U",
                ),
                Action::Background => (
                    format!(
                        "Background · {}",
                        if self.opaque { "Opaque" } else { "Transparent" }
                    )
                    .into(),
                    "F",
                ),
                Action::Note => ("Edit passage note".into(), "N"),
                Action::Navigate(Kind::Notes) => ("Reading notes".into(), "A"),
                Action::Export => ("Export notes to Markdown".into(), "E"),
                Action::Statistics => ("Reading statistics".into(), "S"),
                Action::Settings => ("Settings".into(), "F2"),
                Action::Help => ("Keyboard shortcuts".into(), "?"),
                Action::Resources => (
                    format!("Links & illustrations ({})", self.doc.references.len()).into(),
                    "I",
                ),
                Action::JumpBack => ("Jump back".into(), ","),
                Action::JumpForward => ("Jump forward".into(), "."),
                Action::Close => ("Close book".into(), "Q"),
                _ => unreachable!("visible reader menu command"),
            };
            Row::new([
                ratatui::text::Line::from(if marked { "✓" } else { "" }),
                ratatui::text::Line::from(label),
                ratatui::text::Line::from(shortcut).right_aligned(),
            ])
        });
        use ratatui::layout::Constraint;
        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(2),
                    Constraint::Fill(1),
                    Constraint::Length(4),
                ],
            )
            .row_highlight_style(style.add_modifier(Modifier::REVERSED)),
            self.menu_rows,
            list,
        );
        if let (1, Some(error)) = (status, notice) {
            frame.render_widget(
                Paragraph::new(error),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            );
        }
    }

    fn input(&self, event: Event) -> Option<Action> {
        match event {
            Event::Resize(..) => Some(Action::Resize),
            Event::Key(k)
                if k.kind != KeyEventKind::Release
                    && k.code == KeyCode::Char('c')
                    && k.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                Some(Action::Close)
            }
            _ if self.layout_panel.is_some() => self.layout_panel.as_ref()?.input(event),
            _ if self.nav.is_some() => self.nav.as_ref()?.input(event).map(Action::NavEdit),
            _ if self.palette.is_some() => self.palette.as_ref()?.input(event).map(Action::Pick),
            Event::Key(k)
                if k.kind != KeyEventKind::Release
                    && self.menu.is_none()
                    && !self.sidebar_rect.is_empty()
                    && k.code == KeyCode::Tab =>
            {
                Some(Action::Focus)
            }
            Event::Mouse(m)
                if self.menu.is_none()
                    && self.sidebar_rect.contains(Position::new(m.column, m.row)) =>
            {
                self.sidebar.input(Event::Mouse(m)).map(Action::Sidebar)
            }
            Event::Key(k)
                if self.sidebar_focus
                    && self.menu.is_none()
                    && matches!(
                        k.code,
                        KeyCode::Up
                            | KeyCode::Down
                            | KeyCode::PageUp
                            | KeyCode::PageDown
                            | KeyCode::Enter
                            | KeyCode::Home
                            | KeyCode::End
                    ) =>
            {
                self.sidebar.input(Event::Key(k)).map(Action::Sidebar)
            }
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char(',' | 'q' | 'Q')
                    if self.links.visiting() && self.menu.is_none() =>
                {
                    Some(Action::ReturnLink)
                }
                KeyCode::Enter if self.links.visiting() && self.menu.is_none() => {
                    Some(Action::KeepLink)
                }
                KeyCode::F(6) if self.menu.is_none() => Some(Action::VisibleLinks),
                KeyCode::F(2) => Some(Action::Settings),
                KeyCode::F(5) => Some(Action::SearchResults),
                KeyCode::F(4) => Some(Action::FindAgain(false)),
                KeyCode::F(3) => Some(Action::FindAgain(
                    !k.modifiers.contains(KeyModifiers::SHIFT),
                )),
                KeyCode::Char('?') | KeyCode::F(1) => Some(Action::Help),
                KeyCode::Char(c)
                    if self.menu.is_none()
                        && !k
                            .modifiers
                            .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
                        && self.bindings.contains_key(&c) =>
                {
                    self.bindings.get(&c).copied()
                }
                KeyCode::Char('n' | 'N') => Some(Action::Note),
                KeyCode::Char('i' | 'I') => Some(Action::Resources),
                KeyCode::Char('s' | 'S') => Some(Action::Statistics),
                KeyCode::Char('a' | 'A') => Some(Action::Navigate(Kind::Notes)),
                KeyCode::Char('e' | 'E') => Some(Action::Export),
                KeyCode::Char(',') => Some(Action::JumpBack),
                KeyCode::Char('.') => Some(Action::JumpForward),
                KeyCode::Char('u' | 'U') => Some(Action::Borders),
                KeyCode::Char('z' | 'Z') => Some(Action::Zen),
                KeyCode::Char('f' | 'F') => Some(Action::Background),
                KeyCode::Char('w' | 'W') => Some(Action::Layout),
                KeyCode::Char('{') => Some(Action::Gap(-1)),
                KeyCode::Char('}') => Some(Action::Gap(1)),
                KeyCode::Char('[') => Some(Action::Margin(-1)),
                KeyCode::Char(']') => Some(Action::Margin(1)),
                KeyCode::Char('o' | 'O') => Some(Action::Navigate(Kind::Contents)),
                KeyCode::Char('/') => Some(Action::Navigate(Kind::Search)),
                KeyCode::Char('g' | 'G') => Some(Action::Bookmarks),
                KeyCode::Char('b' | 'B') => Some(Action::Mark),
                KeyCode::Char('q' | 'Q') => Some(Action::Close),
                KeyCode::Char('m' | 'M') | KeyCode::Esc => Some(Action::Menu),
                KeyCode::Char('1') => Some(Action::Single),
                KeyCode::Char('2') => Some(Action::Spread),
                KeyCode::Char('3') => Some(Action::Scroll),
                KeyCode::Char('t' | 'T') => Some(Action::Theme),
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab if self.menu.is_some() => {
                    Some(Action::Up)
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab if self.menu.is_some() => {
                    Some(Action::Down)
                }
                KeyCode::Enter | KeyCode::Char(' ') if self.menu.is_some() => self
                    .menu
                    .as_ref()?
                    .selected()
                    .and_then(|i| COMMANDS.get(i).copied()),
                KeyCode::Tab | KeyCode::Enter => Some(Action::Menu),
                KeyCode::Down | KeyCode::Char('j') if self.mode == LayoutMode::Scroll => {
                    Some(Action::Lines(1))
                }
                KeyCode::Up | KeyCode::Char('k') if self.mode == LayoutMode::Scroll => {
                    Some(Action::Lines(-1))
                }
                KeyCode::Left
                | KeyCode::PageUp
                | KeyCode::Up
                | KeyCode::Char('h' | 'k')
                | KeyCode::Backspace => Some(Action::Previous),
                KeyCode::Right
                | KeyCode::PageDown
                | KeyCode::Down
                | KeyCode::Char('l' | 'j' | ' ') => Some(Action::Next),
                _ => None,
            },
            Event::Mouse(m)
                if self.menu.is_none()
                    && m.kind == MouseEventKind::Down(MouseButton::Left)
                    && self.links.hit(Position::new(m.column, m.row)).is_some() =>
            {
                self.links.hit(Position::new(m.column, m.row))
            }
            Event::Mouse(m)
                if !self.zen
                    && self.menu.is_none()
                    && m.kind == MouseEventKind::Down(MouseButton::Left)
                    && self
                        .footer
                        .iter()
                        .any(|r| r.contains(Position::new(m.column, m.row))) =>
            {
                self.footer
                    .iter()
                    .position(|r| r.contains(Position::new(m.column, m.row)))
                    .map(|i| {
                        [
                            Action::Zen,
                            Action::Navigate(Kind::Contents),
                            Action::Menu,
                            Action::Settings,
                            Action::Help,
                            Action::VisibleLinks,
                        ][i]
                    })
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) if self.menu.is_none() => {
                    Some(Action::Menu)
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if self.menu_rows.contains(Position::new(m.column, m.row)) {
                        let row =
                            self.menu.as_ref()?.offset() + usize::from(m.row - self.menu_rows.y);
                        COMMANDS.get(row).copied()
                    } else {
                        Some(Action::Resume)
                    }
                }
                MouseEventKind::ScrollUp
                    if self.mode == LayoutMode::Scroll && self.menu.is_none() =>
                {
                    Some(Action::Lines(-3))
                }
                MouseEventKind::ScrollDown
                    if self.mode == LayoutMode::Scroll && self.menu.is_none() =>
                {
                    Some(Action::Lines(3))
                }
                MouseEventKind::ScrollUp => Some(if self.menu.is_some() {
                    Action::Up
                } else {
                    Action::Previous
                }),
                MouseEventKind::ScrollDown => Some(if self.menu.is_some() {
                    Action::Down
                } else {
                    Action::Next
                }),
                _ => None,
            },
            _ => None,
        }
    }

    fn jump_to(&mut self, place: botui_core::Anchor) {
        if place != self.pager.anchor() {
            self.history.push(self.pager.anchor());
            self.future.clear();
            self.pager.seek(place);
        }
    }
    fn apply(&mut self, action: Action) {
        match action {
            Action::ReturnLink => {
                if let Some(a) = self.links.back() {
                    self.pager.seek(a);
                }
            }
            Action::KeepLink => {
                self.history.push(self.links.origin(self.pager.anchor()));
                self.future.clear();
                self.links.keep();
                self.dirty = true;
            }
            Action::JumpBack | Action::JumpForward => {
                let (from, to) = if action == Action::JumpBack {
                    (&mut self.history, &mut self.future)
                } else {
                    (&mut self.future, &mut self.history)
                };
                if let Some(a) = from.pop() {
                    to.push(self.pager.anchor());
                    self.pager.seek(a);
                    self.dirty = true;
                }
                self.menu = None;
            }
            Action::Focus => self.sidebar_focus = !self.sidebar_focus,
            Action::Sidebar(edit) => {
                self.sidebar_focus = true;
                if let Some(pos) = self.sidebar.edit(edit) {
                    self.jump_to(pos);
                    self.dirty = true;
                    self.sidebar_focus = false;
                }
            }
            Action::Zen => {
                self.zen = !self.zen;
                self.sidebar_focus = false;
                self.menu = None;
            }
            Action::Borders => {}
            Action::Background => {
                self.opaque = !self.opaque;
                self.menu = None;
            }
            Action::Navigate(kind) => {
                self.menu = None;
                let mut nav = navigation::Navigation::new(kind, &self.doc, self.pager.anchor());
                if kind == Kind::Search {
                    nav.index = Some(self.search.index(&self.pager, &self.doc));
                }
                self.nav = Some(nav);
            }
            Action::SearchResults => {
                self.apply(Action::Navigate(Kind::Search));
                if let Some(nav) = &mut self.nav {
                    self.search.restore(nav);
                }
            }
            Action::FindAgain(forward) => {
                self.menu = None;
                if let Some(place) = self.search.advance(self.pager.anchor(), forward) {
                    self.jump_to(place);
                    self.dirty = true;
                } else {
                    self.apply(Action::Navigate(Kind::Search));
                }
            }
            Action::NavEdit(Edit::Back) => self.nav = None,
            Action::NavEdit(e) => {
                if let Some(nav) = &mut self.nav
                    && let Some(place) = nav.edit(e)
                {
                    if let Edit::Choose(selected) = e
                        && nav.index.is_some()
                    {
                        self.search.select(
                            std::mem::take(&mut nav.query),
                            std::mem::take(&mut nav.hits),
                            selected,
                        );
                    }
                    self.jump_to(place);
                    self.dirty = true;
                    self.nav = None;
                }
            }
            Action::Layout => {
                self.menu = None;
                self.layout_panel = if self.layout_panel.is_some() {
                    None
                } else {
                    Some(layout::Panel::default())
                };
            }
            Action::Gap(d) => {
                self.gap = self.gap.saturating_add_signed(i16::from(d)).min(32);
            }
            Action::Margin(d) => {
                self.page_width = self
                    .pager
                    .column_width()
                    .saturating_add_signed(-2 * i16::from(d))
                    .clamp(32, 240);
                self.margin = (self.margin + d).clamp(-2, 4);
                self.dirty = true;
                self.menu = None;
            }
            Action::Menu => {
                self.menu = if self.menu.is_some() {
                    None
                } else {
                    Some(TableState::default().with_selected(Some(0)))
                };
            }
            Action::Resume => self.menu = None,
            Action::Up | Action::Down => {
                if let Some(list) = &mut self.menu {
                    let index = list.selected().unwrap_or(0);
                    let next = (index
                        + if action == Action::Down {
                            1
                        } else {
                            COMMANDS.len() - 1
                        })
                        % COMMANDS.len();
                    list.select(Some(next));
                }
            }
            Action::Previous | Action::Next | Action::Lines(_) => {
                let old = self.pager.anchor();
                let forward = match action {
                    Action::Lines(n) => {
                        self.pager.scroll(isize::from(n));
                        n > 0
                    }
                    Action::Next => {
                        if self.mode == LayoutMode::Scroll {
                            self.pager.scroll(self.pager.viewport() as isize);
                        } else {
                            self.pager.next();
                        }
                        true
                    }
                    _ => {
                        if self.mode == LayoutMode::Scroll {
                            self.pager.scroll(-(self.pager.viewport() as isize));
                        } else {
                            self.pager.previous();
                        }
                        false
                    }
                };
                self.dirty |= old != self.pager.anchor();
                if forward && !self.links.visiting() && self.pager.at_end() && !self.finished {
                    self.finished = true;
                    self.dirty = true;
                }
                self.menu = None;
            }
            Action::Single | Action::Spread | Action::Scroll => {
                let mode = match action {
                    Action::Single => LayoutMode::Single,
                    Action::Scroll => LayoutMode::Scroll,
                    _ => LayoutMode::Spread,
                };
                if self.mode != mode {
                    self.mode = mode;
                    self.dirty = true;
                }
                self.menu = None;
            }
            Action::Theme => {
                self.palette = Some(theme_picker::Picker::new(self.theme));
            }
            Action::Pick(theme_picker::Edit::Cancel) => self.palette = None,
            Action::Pick(theme_picker::Edit::Accept(theme)) => {
                self.dirty |= self.theme != theme;
                self.theme = theme;
                self.palette = None;
                self.menu = None;
            }
            Action::Pick(edit) => {
                if let Some(picker) = &mut self.palette {
                    picker.edit(edit);
                }
            }
            Action::Resources
            | Action::VisibleLinks
            | Action::Follow(_)
            | Action::Bookmarks
            | Action::Mark
            | Action::Settings
            | Action::Help
            | Action::Statistics
            | Action::Note
            | Action::Export
            | Action::Resize
            | Action::Close => {}
        }
    }

    fn snapshot(&self) -> Saved {
        Saved {
            anchor: self.links.origin(self.pager.anchor()),
            spread: self.mode == LayoutMode::Spread,
            scrolling: self.mode == LayoutMode::Scroll,
            scan_offset: 0,
            scan_appearance: Default::default(),
            finished: self.finished,
            theme: self.theme.id(),
            marks: self.marks.clone(),
            margin: self.margin,
        }
    }

    fn flush(&mut self, state: &mut Option<LocalState>, notices: &mut Vec<String>) {
        if !self.dirty {
            return;
        }
        if let Some(store) = state
            && let Err(e) = store.save_position(&self.snapshot())
        {
            notices.push(format!("Could not save reading position: {e}"));
            *state = None;
        }
        self.dirty = false;
    }
}

fn bindings() -> io::Result<std::collections::HashMap<char, Action>> {
    let raw = crate::settings::Settings::load()?.keys;
    raw.into_iter()
        .map(|(key, name)| {
            let action = match name.as_str() {
                "next" => Action::Next,
                "previous" => Action::Previous,
                "menu" => Action::Menu,
                "zen" => Action::Zen,
                "background" => Action::Background,
                "borders" => Action::Borders,
                "resources" => Action::Resources,
                "statistics" => Action::Statistics,
                "note" => Action::Note,
                "notes" => Action::Navigate(Kind::Notes),
                "export-notes" => Action::Export,
                "jump-back" => Action::JumpBack,
                "jump-forward" => Action::JumpForward,
                "theme" => Action::Theme,
                "contents" => Action::Navigate(Kind::Contents),
                "search" => Action::Navigate(Kind::Search),
                "search-results" => Action::SearchResults,
                "next-match" => Action::FindAgain(true),
                "previous-match" => Action::FindAgain(false),
                "bookmark" => Action::Mark,
                "bookmarks" => Action::Bookmarks,
                "single" => Action::Single,
                "spread" => Action::Spread,
                "scroll" => Action::Scroll,
                "page-layout" => Action::Layout,
                "wider-gap" => Action::Gap(1),
                "narrower-gap" => Action::Gap(-1),
                "wider-margins" | "narrower-page" => Action::Margin(1),
                "narrower-margins" | "wider-page" => Action::Margin(-1),
                "close" => Action::Close,
                _ => return Err(io::Error::other(format!("Unknown command {name}"))),
            };
            Ok((key, action))
        })
        .collect()
}

pub fn read(
    term: &mut AppTerminal,
    path: &Path,
    doc: Document,
    notices: &mut Vec<String>,
) -> io::Result<()> {
    let (_, percent) = read_server(term, path, doc, notices, None, None)?;
    if let Err(e) = botui_store::HistoryEntry::record(path, percent) {
        notices.push(e.to_string());
    }
    Ok(())
}
pub fn read_server(
    term: &mut AppTerminal,
    path: &Path,
    doc: Document,
    notices: &mut Vec<String>,
    base: Option<&botui_bookorbit::progress::Progress>,
    mut remote: Option<&mut bookmarks::ServerMarks>,
) -> io::Result<(Option<botui_bookorbit::progress::Move>, f64)> {
    let before = notices.len();
    let (mut state, mut saved) = match LocalState::for_file(path).and_then(|store| {
        let saved = store.load()?;
        Ok((Some(store), saved))
    }) {
        Ok(pair) => pair,
        Err(err) => {
            notices.push(format!("Local reading state unavailable: {err}"));
            (None, None)
        }
    };
    let size = term.size()?;
    if base.is_some_and(|p| p.percentage == 0. && p.text.cfi.is_none() && p.text.page.is_none())
        && let Some(local) = &mut saved
    {
        local.anchor = botui_core::Anchor::default();
        local.finished = false;
    }
    let mut reader = Reader::new(doc, Rect::new(0, 0, size.width, size.height), saved, path);
    match bindings() {
        Ok(map) => reader.bindings = map,
        Err(e) => notices.push(format!("Could not read key bindings: {e}")),
    }
    let mut can_sync = base.is_some();
    if let Some(p) = base {
        if let Some(cfi) = p.text.cfi.as_deref() {
            if let Some(a) = reader.doc.bookmark_anchor(cfi) {
                reader.pager.seek(a);
                reader.dirty = true;
            } else {
                can_sync = false;
                notices.push("BookOrbit position could not be located in this text.".into());
            }
        } else if p.text.page.is_some() || p.percentage > 0.0 {
            can_sync = false;
            notices.push("BookOrbit has no exact text location. Using the local position.".into());
        }
    }
    let opened = Instant::now();
    let start = reader.pager.anchor();
    let started_finished = reader.finished;
    let mut last_save = Instant::now();
    let result: io::Result<()> = (|| {
        loop {
            term.draw(|frame| {
                reader.draw(frame, notices[before..].last().map(String::as_str));
                crate::settings::paint_book(
                    frame,
                    reader
                        .palette
                        .as_ref()
                        .map_or(reader.theme, |p| p.preview()),
                )
            })?;
            let action = loop {
                let remaining = Duration::from_millis(600).saturating_sub(last_save.elapsed());
                if reader.dirty && remaining.is_zero() {
                    reader.flush(&mut state, notices);
                    last_save = Instant::now();
                }
                let images = reader.illustrations.as_ref().is_some_and(|p| p.busy());
                let timeout = reader
                    .dirty
                    .then_some(remaining)
                    .into_iter()
                    .chain(images.then_some(Duration::from_millis(70)))
                    .min();
                let Some(e) = crate::settings::read_event_for(timeout)? else {
                    if images {
                        break Action::Resize;
                    }
                    continue;
                };
                if reader.menu.is_none()
                    && reader.nav.is_none()
                    && reader.palette.is_none()
                    && reader.layout_panel.is_none()
                    && let Some(result) = reader.splitter.input(&e)
                {
                    if let Err(error) = result {
                        notices.push(error.to_string());
                    }
                    break Action::Resize;
                }
                if let Event::Paste(value) = &e
                    && let Some(nav) = reader.nav.as_mut()
                {
                    nav.paste(value);
                    break Action::Resize;
                }
                if let Some(action) = reader.input(e) {
                    break action;
                }
            };
            if matches!(
                action,
                Action::Note | Action::Export | Action::Navigate(Kind::Notes)
            ) {
                reader.menu = None;
                let action_result = (|| -> io::Result<()> {
                    let store = state
                        .as_ref()
                        .ok_or_else(|| io::Error::other("Local storage is unavailable"))?;
                    let notes = store.notes()?;
                    match action {
                        Action::Note => {
                            let at = reader.pager.anchor();
                            let initial = notes
                                .iter()
                                .find(|n| n.anchor() == at)
                                .map_or("", |n| &n.body);
                            if let Some(body) = crate::prompt::edit(
                                term,
                                "Passage note · clear to remove",
                                "[ Save ↵ ]",
                                initial,
                                |f| reader.draw(f, None),
                            )? {
                                store.edit_note(at, body)?;
                            }
                        }
                        Action::Export => {
                            let path = notes::export(&reader.doc.title, Some(&reader.doc), &notes)?;
                            notices.push(format!("Saved {}", path.display()));
                            reader.apply(Action::Menu);
                        }
                        _ => {
                            reader.apply(action);
                            if let Some(nav) = reader.nav.as_mut() {
                                nav.add_notes(&notes);
                            }
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = action_result {
                    notices.push(e.to_string());
                    reader.menu = Some(TableState::default().with_selected(Some(0)));
                }
                continue;
            }
            if matches!(action, Action::Settings | Action::Help) {
                let old = crate::settings::Settings::load()?;
                crate::preferences::show(
                    term,
                    if action == Action::Help {
                        crate::preferences::Open::Help
                    } else {
                        crate::preferences::Open::Settings
                    },
                    crate::preferences::Context::Text,
                )?;
                let new = crate::settings::Settings::load()?;
                reader.zen = new.zen;
                reader.splitter.preferred = new.contents_width;
                if new.reading_layout != old.reading_layout {
                    reader.mode = match new.reading_layout.as_deref() {
                        Some("scroll") => LayoutMode::Scroll,
                        Some("single") => LayoutMode::Single,
                        _ => LayoutMode::Spread,
                    };
                    reader.dirty = true;
                }

                reader.opaque = new.opaque_background;
                reader.menu = None;
                if new.theme != old.theme {
                    reader.theme = Theme::from_id(new.theme.as_deref().unwrap_or("terminal"));
                    reader.dirty = true;
                }
                if let Some(w) = new.page_width {
                    reader.page_width = w.clamp(32, 240);
                }
                if let Some(g) = new.column_gap {
                    reader.gap = g.min(32);
                }
                if old.preserve_printed_lines != new.preserve_printed_lines
                    || old.hide_inline_images != new.hide_inline_images
                    || old.paragraph_indent != new.paragraph_indent
                {
                    let focus = reader.pager.anchor();
                    let size = term.size()?;
                    reader.pager = Pager::with_options(
                        &reader.doc,
                        size.width,
                        size.height,
                        reader.mode,
                        ReflowOptions {
                            repair_printed: !new.preserve_printed_lines,
                            inline_images: !new.hide_inline_images,
                            paragraph_indent: new.paragraph_indent,
                        },
                    );
                    reader.pager.seek(focus);
                    if old.preserve_printed_lines != new.preserve_printed_lines {
                        reader.search = Default::default();
                    }
                }
                continue;
            }
            if matches!(
                action,
                Action::Resources | Action::VisibleLinks | Action::Follow(_)
            ) {
                reader.menu = None;
                let visible = reader.links.visible();
                let selected = match action {
                    Action::Follow(i) => Some(i),
                    Action::VisibleLinks => {
                        resources::choose(term, &reader.doc, reader.pager.anchor(), Some(&visible))?
                    }
                    _ => resources::choose(term, &reader.doc, reader.pager.anchor(), None)?,
                };
                if let Some(i) = selected {
                    let reference = reader.doc.references[i].clone();
                    match resources::open(term, path, &reference, |f| reader.draw(f, None)) {
                        Ok(Some(a)) => {
                            let dest =
                                reader
                                    .links
                                    .destination(reader.pager.anchor(), &reference, a);
                            reader.pager.seek(dest);
                        }
                        Ok(None) => (),
                        Err(e) => {
                            notices.push(e.to_string());
                            reader.menu = Some(TableState::default().with_selected(Some(0)));
                        }
                    }
                }
                continue;
            }
            if action == Action::Statistics {
                reader.menu = None;
                if let Err(e) = crate::stats::show(
                    term,
                    state.as_ref(),
                    opened,
                    if reader.finished && reader.pager.at_end() {
                        100.
                    } else {
                        reader.doc.text_percentage(reader.pager.anchor()).min(99.)
                    },
                ) {
                    notices.push(e.to_string());
                }
                continue;
            }
            if action == Action::Close {
                break;
            }
            if action == Action::Bookmarks {
                reader.menu = None;
                reader.manage_marks(term, remote.as_deref_mut(), state.as_ref())?;
                continue;
            }
            if action == Action::Mark {
                let at = reader.pager.anchor();
                let add = !reader.marks.contains(&at);
                let result = if add && let Some(m) = remote.as_deref_mut() {
                    m.toggle(&reader.doc, at).map_err(io::Error::other)
                } else {
                    reader.set_local_bookmark(state.as_ref(), at, add)
                };
                reader.menu = None;
                if let Err(error) = result {
                    crate::prompt::message(term, "Could not update bookmark", &error.to_string())?;
                }
                continue;
            }
            if action == Action::Menu {
                reader.remote_marked = remote.as_ref().is_some_and(|m| {
                    m.entries(&reader.doc)
                        .iter()
                        .any(|(_, a, _)| *a == Some(reader.pager.anchor()))
                });
            }
            if action == Action::Borders
                && let Err(e) = crate::appearance::cycle()
            {
                notices.push(e.to_string());
            }
            reader.apply(action);
            if matches!(
                action,
                Action::Zen
                    | Action::Background
                    | Action::Gap(_)
                    | Action::Margin(_)
                    | Action::Pick(theme_picker::Edit::Accept(_))
            ) {
                let result = crate::settings::Settings::update(|s| match action {
                    Action::Gap(_) => s.column_gap = Some(reader.gap),
                    Action::Margin(_) => s.page_width = Some(reader.page_width),
                    Action::Zen => s.zen = reader.zen,
                    Action::Background => s.opaque_background = reader.opaque,
                    _ => s.theme = Some(reader.theme.id()),
                });
                if let Err(e) = result {
                    notices.push(e.to_string());
                }
            }
        }
        Ok(())
    })();
    let origin = reader.links.origin(reader.pager.anchor());
    reader.pager.seek(origin);
    reader.flush(&mut state, notices);
    crate::stats::finish(state.as_ref(), opened, notices);
    result?;
    let a = reader.pager.anchor();
    let percent = if reader.finished && reader.pager.at_end() {
        100.
    } else {
        reader.doc.text_percentage(a).min(99.)
    };
    if can_sync && (a != start || reader.finished != started_finished) {
        if let Some(cfi) = reader.doc.cfi(a) {
            return Ok((
                Some(botui_bookorbit::progress::Move {
                    target: botui_bookorbit::progress::Target::Text(cfi),
                    percentage: percent,
                }),
                percent,
            ));
        }
        notices.push("Could not map this passage to BookOrbit. Saved locally.".into());
    }
    Ok((None, percent))
}
