pub(crate) mod bookmarks;
mod navigation;
pub(crate) mod notes;
mod resources;
mod theme_picker;
use navigation::{Edit, Kind};

use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use botui_core::{Document, LayoutMode, Pager};
use botui_store::{LocalState, Saved};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Modifier,
    widgets::{Block, Clear, List, ListState, Paragraph},
};

use crate::{AppTerminal, centered, theme::Theme};

fn text_area(area: Rect, mode: LayoutMode, margin: i8) -> Rect {
    let side = (area.width / 22)
        .min(8)
        .saturating_add_signed(2 * i16::from(margin))
        .min(area.width.saturating_sub(12) / 2);
    let vertical = (area.height / 13).min(2);
    let width = area.width.saturating_sub(side * 2).min(match mode {
        LayoutMode::Single => 78u16.saturating_add_signed(-6 * i16::from(margin)),
        LayoutMode::Spread => 160u16.saturating_add_signed(-12 * i16::from(margin)),
    });
    centered(area, width, area.height.saturating_sub(vertical * 2))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Menu,
    Zen,
    Background,
    Sidebar(Edit),
    Focus,
    JumpBack,
    JumpForward,
    Note,
    Export,
    Statistics,
    Resources,
    Resume,
    Previous,
    Next,
    Single,
    Spread,
    Theme,
    Pick(theme_picker::Edit),
    Navigate(Kind),
    NavEdit(Edit),
    Mark,
    Margin(i8),
    Close,
    Up,
    Down,
    Resize,
}

const COMMANDS: [Action; 22] = [
    Action::Resume,
    Action::Previous,
    Action::Next,
    Action::Single,
    Action::Spread,
    Action::Theme,
    Action::Navigate(Kind::Contents),
    Action::Navigate(Kind::Search),
    Action::Mark,
    Action::Navigate(Kind::Marks),
    Action::Margin(-1),
    Action::Margin(1),
    Action::Zen,
    Action::Background,
    Action::Note,
    Action::Navigate(Kind::Notes),
    Action::Export,
    Action::Statistics,
    Action::Resources,
    Action::JumpBack,
    Action::JumpForward,
    Action::Close,
];

struct Reader {
    zen: bool,
    opaque: bool,
    sidebar: navigation::Navigation,
    sidebar_rect: Rect,
    footer: [Rect; 3],
    remote_marked: bool,
    sidebar_focus: bool,
    history: Vec<botui_core::Anchor>,
    future: Vec<botui_core::Anchor>,
    doc: Document,
    pager: Pager,
    mode: LayoutMode,
    area: Rect,
    theme: Theme,
    palette: Option<theme_picker::Picker>,
    menu: Option<ListState>,
    menu_rows: Rect,
    nav: Option<navigation::Navigation>,
    marks: Vec<botui_core::Anchor>,
    margin: i8,
    bindings: std::collections::HashMap<char, Action>,
    dirty: bool,
}

impl Reader {
    fn new(doc: Document, area: Rect, saved: Option<Saved>) -> Self {
        let settings = crate::settings::Settings::load().unwrap_or_default();
        let mode = if saved.as_ref().is_some_and(|s| !s.spread) {
            LayoutMode::Single
        } else {
            LayoutMode::Spread
        };
        let margin = saved.as_ref().map_or(0, |s| s.margin);
        let text = text_area(area, mode, margin);
        let mut pager = Pager::with_reflow(
            &doc,
            text.width,
            text.height,
            mode,
            !settings.preserve_printed_lines,
        );
        let marks = saved.as_ref().map(|s| s.marks.clone()).unwrap_or_default();
        let theme = if let Some(state) = saved {
            pager.seek(state.anchor);
            Theme::from_id(&state.theme)
        } else {
            settings
                .theme
                .as_deref()
                .map_or(Theme::TERMINAL, Theme::from_id)
        };
        let sidebar = navigation::Navigation::new(Kind::Contents, &doc, pager.anchor(), &[]);
        Self {
            zen: settings.zen,
            opaque: settings.opaque_background,
            sidebar,
            sidebar_rect: Rect::default(),
            footer: [Rect::default(); 3],
            remote_marked: false,
            sidebar_focus: false,
            history: vec![],
            future: vec![],
            doc,
            pager,
            mode,
            area: text,
            theme,
            palette: None,
            menu: None,
            menu_rows: Rect::default(),
            nav: None,
            marks,
            margin,
            bindings: Default::default(),
            dirty: false,
        }
    }

    fn draw(&mut self, frame: &mut Frame, notice: Option<&str>) {
        let screen = frame.area();
        let mut page = screen;
        self.sidebar_rect = Rect::default();
        if !self.zen {
            page.height = page.height.saturating_sub(1);
            if screen.width >= 78 {
                let w = (screen.width / 5).clamp(22, 32);
                self.sidebar_rect = Rect::new(page.x, page.y, w, page.height);
                page.x += w + 1;
                page.width -= w + 1;
            } else {
                self.sidebar_focus = false;
            }
        }
        let pair = self.mode == LayoutMode::Spread && page.width >= 76;
        let pane_width = if pair {
            (page.width - 4) / 2
        } else {
            page.width
        };
        let pane = Rect {
            width: pane_width,
            ..page
        };
        let text = text_area(pane, LayoutMode::Single, self.margin);
        let logical_width = if pair { 2 * text.width + 4 } else { text.width };
        let actual = if pair && text.width >= 32 {
            LayoutMode::Spread
        } else {
            LayoutMode::Single
        };
        let text = if pair && actual == LayoutMode::Single {
            text_area(page, LayoutMode::Single, self.margin)
        } else {
            text
        };
        self.pager.relayout(
            &self.doc,
            if actual == LayoutMode::Spread {
                logical_width
            } else {
                text.width
            },
            text.height,
            actual,
        );
        self.area = text;
        let style = self
            .palette
            .as_ref()
            .map_or(self.theme, |p| p.preview())
            .style();
        frame.render_widget(Block::new().style(style), screen);
        for c in 0..self.pager.columns() {
            let x = text.x.saturating_add(c as u16 * (pane_width + 4));
            for (y, line) in self
                .pager
                .lines(c)
                .iter()
                .take(usize::from(text.height))
                .enumerate()
            {
                let rect = Rect::new(x, text.y + y as u16, self.pager.column_width(), 1);
                let mut para = Paragraph::new(line.text.as_str()).style(style);
                if line.heading {
                    para = para.centered().style(style.add_modifier(Modifier::BOLD));
                }
                frame.render_widget(para, rect);
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
            for (i, label) in [" Z Zen ", " O Contents ", " M Menu "]
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
                " {} / {} ",
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
        if let Some(picker) = &mut self.palette {
            picker.draw(frame);
            return;
        }
        if let Some(nav) = &mut self.nav {
            nav.draw(frame, style);
            return;
        }
        let Some(list) = &mut self.menu else { return };
        let popup = centered(screen, 54, 24 + u16::from(notice.is_some()));
        frame.render_widget(Clear, popup);
        let block = Block::bordered().title(" Reading ").style(style);
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
        let single = if self.mode == LayoutMode::Single {
            "✓"
        } else {
            " "
        };
        let wide = if self.mode == LayoutMode::Spread {
            "✓"
        } else {
            " "
        };
        let entries = [
            "   Resume                 Esc".to_owned(),
            "   Previous page           ←".into(),
            "   Next page               →".into(),
            format!(" {single} Single page             1"),
            format!(" {wide} Two-page spread         2"),
            format!("   Theme: {}     T", self.theme.name()),
            "   Contents                O".into(),
            "   Find in book            /".into(),
            format!(
                "   {}          B",
                if self.remote_marked || self.marks.contains(&self.pager.anchor()) {
                    "Remove bookmark"
                } else {
                    "Add bookmark"
                }
            ),
            "   Bookmarks               G".into(),
            "   Narrower margins        [".into(),
            "   Wider margins           ]".into(),
            format!(
                "   {}                   Z",
                if self.zen {
                    "Show contents sidebar"
                } else {
                    "Enter zen mode"
                }
            ),
            format!(
                "   Background: {}       F",
                if self.opaque { "Opaque" } else { "Terminal" }
            ),
            "   Edit passage note       N".into(),
            "   Reading notes           A".into(),
            "   Export notes to Markdown E".into(),
            "   Reading statistics      S".into(),
            format!(
                "   Links & illustrations ({})  I",
                self.doc.references.len()
            ),
            "   Jump back              ,".into(),
            "   Jump forward           .".into(),
            "   Close book              Q".into(),
        ];
        frame.render_stateful_widget(
            List::new(entries).highlight_style(style.add_modifier(Modifier::REVERSED)),
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
                KeyCode::Char('z' | 'Z') => Some(Action::Zen),
                KeyCode::Char('f' | 'F') => Some(Action::Background),
                KeyCode::Char('[') => Some(Action::Margin(-1)),
                KeyCode::Char(']') => Some(Action::Margin(1)),
                KeyCode::Char('o' | 'O') => Some(Action::Navigate(Kind::Contents)),
                KeyCode::Char('/') => Some(Action::Navigate(Kind::Search)),
                KeyCode::Char('g' | 'G') => Some(Action::Navigate(Kind::Marks)),
                KeyCode::Char('b' | 'B') => Some(Action::Mark),
                KeyCode::Char('q' | 'Q') => Some(Action::Close),
                KeyCode::Char('m' | 'M' | '?') | KeyCode::Esc => Some(Action::Menu),
                KeyCode::Char('1') => Some(Action::Single),
                KeyCode::Char('2') => Some(Action::Spread),
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
                    .map(|i| [Action::Zen, Action::Navigate(Kind::Contents), Action::Menu][i])
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
            Action::Background => {
                self.opaque = !self.opaque;
                self.menu = None;
            }
            Action::Navigate(kind) => {
                self.menu = None;
                let mut nav =
                    navigation::Navigation::new(kind, &self.doc, self.pager.anchor(), &self.marks);
                if kind == Kind::Search {
                    nav.index = Some(self.pager.search_index(&self.doc));
                }
                self.nav = Some(nav);
            }
            Action::NavEdit(Edit::Back) => self.nav = None,
            Action::NavEdit(e) => {
                if let Some(place) = self.nav.as_mut().and_then(|n| n.edit(e)) {
                    self.jump_to(place);
                    self.dirty = true;
                    self.nav = None;
                }
            }
            Action::Margin(d) => {
                self.margin = (self.margin + d).clamp(-2, 4);
                self.dirty = true;
                self.menu = None;
            }
            Action::Mark => {
                let a = self.pager.anchor();
                match self.marks.binary_search(&a) {
                    Ok(i) => {
                        self.marks.remove(i);
                    }
                    Err(i) if self.marks.len() < 2048 => self.marks.insert(i, a),
                    _ => (),
                }
                self.dirty = true;
                self.menu = None;
            }
            Action::Menu => {
                self.menu = if self.menu.is_some() {
                    None
                } else {
                    Some(ListState::default().with_selected(Some(0)))
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
            Action::Previous | Action::Next => {
                let anchor = self.pager.anchor();
                if action == Action::Next {
                    self.pager.next()
                } else {
                    self.pager.previous()
                }
                self.dirty |= anchor != self.pager.anchor();
                self.menu = None;
            }
            Action::Single | Action::Spread => {
                let mode = if action == Action::Single {
                    LayoutMode::Single
                } else {
                    LayoutMode::Spread
                };
                if self.mode != mode {
                    self.mode = mode;
                    // Force layout even when both modes use an identical viewport.
                    self.area = Rect::default();
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
            | Action::Statistics
            | Action::Note
            | Action::Export
            | Action::Resize
            | Action::Close => {}
        }
    }

    fn flush(&mut self, state: &mut Option<LocalState>, notices: &mut Vec<String>) {
        if !self.dirty {
            return;
        }
        if let Some(store) = state {
            let snapshot = Saved {
                anchor: self.pager.anchor(),
                spread: self.mode == LayoutMode::Spread,
                theme: self.theme.id(),
                marks: self.marks.clone(),
                margin: self.margin,
            };
            if let Err(e) = store.save(&snapshot) {
                notices.push(format!("Could not save reading position: {e}"));
                *state = None;
            }
        }
        self.dirty = false;
    }
}

fn bindings() -> io::Result<std::collections::HashMap<char, Action>> {
    use std::{collections::HashMap, fs::File, io::Read};
    let path = crate::connection::settings_file()?.with_file_name("reader-keys.json");
    let f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e),
    };
    if f.metadata()?.len() > 16384 {
        return Err(io::Error::other("reader-keys.json is too large"));
    }
    let raw: HashMap<char, String> = serde_json::from_reader(f.take(16385))?;
    raw.into_iter()
        .map(|(key, name)| {
            let action = match name.as_str() {
                "next" => Action::Next,
                "previous" => Action::Previous,
                "menu" => Action::Menu,
                "zen" => Action::Zen,
                "background" => Action::Background,
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
                "bookmark" => Action::Mark,
                "bookmarks" => Action::Navigate(Kind::Marks),
                "single" => Action::Single,
                "spread" => Action::Spread,
                "wider-margins" => Action::Margin(1),
                "narrower-margins" => Action::Margin(-1),
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
    let (mut state, saved) = match LocalState::for_file(path).and_then(|store| {
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
    let mut reader = Reader::new(doc, Rect::new(0, 0, size.width, size.height), saved);
    match bindings() {
        Ok(map) => reader.bindings = map,
        Err(e) => notices.push(format!("Could not read key bindings: {e}")),
    }
    let mut can_sync = base.is_some();
    if let Some(p) = base {
        if let Some(cfi) = p.text.cfi.as_deref() {
            if let Some(a) = reader.doc.resolve_cfi(cfi) {
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
    let mut last_save = Instant::now();
    let result: io::Result<()> = (|| {
        loop {
            term.draw(|frame| {
                reader.draw(frame, notices[before..].last().map(String::as_str));
                crate::settings::paint(frame)
            })?;
            let action = loop {
                if reader.dirty {
                    let remaining = Duration::from_millis(600).saturating_sub(last_save.elapsed());
                    if remaining.is_zero() || !event::poll(remaining)? {
                        reader.flush(&mut state, notices);
                        last_save = Instant::now();
                    }
                }
                let e = event::read()?;
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
                    reader.menu = Some(ListState::default().with_selected(Some(0)));
                }
                continue;
            }
            if action == Action::Resources {
                reader.menu = None;
                match resources::open(term, path, &reader.doc, reader.pager.anchor()) {
                    Ok(Some(a)) => {
                        reader.jump_to(a);
                        reader.dirty = true;
                    }
                    Ok(None) => (),
                    Err(e) => {
                        notices.push(e.to_string());
                        reader.menu = Some(ListState::default().with_selected(Some(0)));
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
                    reader.doc.text_percentage(reader.pager.anchor()),
                ) {
                    notices.push(e.to_string());
                }
                continue;
            }
            if action == Action::Close {
                break;
            }
            if action == Action::Mark
                && !reader.marks.contains(&reader.pager.anchor())
                && let Some(m) = remote.as_deref_mut()
            {
                match m.toggle(&reader.doc, reader.pager.anchor()) {
                    Ok(()) => reader.menu = None,
                    Err(e) => {
                        notices.push(format!("Could not update bookmark: {e}"));
                        reader.menu = Some(ListState::default().with_selected(Some(0)));
                    }
                }
                continue;
            }
            if action == Action::Menu {
                reader.remote_marked = remote.as_ref().is_some_and(|m| {
                    m.entries(&reader.doc)
                        .iter()
                        .any(|(a, _)| *a == Some(reader.pager.anchor()))
                });
            }
            reader.apply(action);
            if action == Action::Navigate(Kind::Marks)
                && let (Some(m), Some(nav)) = (remote.as_deref(), reader.nav.as_mut())
            {
                nav.add_server_marks(m.entries(&reader.doc));
            }
            if matches!(
                action,
                Action::Zen | Action::Background | Action::Pick(theme_picker::Edit::Accept(_))
            ) {
                let result = crate::settings::Settings::update(|s| match action {
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
    reader.flush(&mut state, notices);
    crate::stats::finish(state.as_ref(), opened, notices);
    result?;
    let a = reader.pager.anchor();
    let percent = reader.doc.text_percentage(a);
    if can_sync && a != start {
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
