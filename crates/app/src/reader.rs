mod navigation;
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

const COMMANDS: [Action; 13] = [
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
    Action::Close,
];

struct Reader {
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
        let mode = if saved.as_ref().is_some_and(|s| !s.spread) {
            LayoutMode::Single
        } else {
            LayoutMode::Spread
        };
        let margin = saved.as_ref().map_or(0, |s| s.margin);
        let text = text_area(area, mode, margin);
        let mut pager = Pager::new(&doc, text.width, text.height, mode);
        let marks = saved.as_ref().map(|s| s.marks.clone()).unwrap_or_default();
        let theme = if let Some(state) = saved {
            pager.seek(state.anchor);
            Theme::from_id(&state.theme)
        } else {
            Theme::TERMINAL
        };
        Self {
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
        let text = text_area(screen, self.mode, self.margin);
        if text != self.area {
            self.pager
                .relayout(&self.doc, text.width, text.height, self.mode);
            self.area = text;
        }
        let style = self
            .palette
            .as_ref()
            .map_or(self.theme, |p| p.preview())
            .style();
        frame.render_widget(Block::new().style(style), screen);
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
                let rect = Rect::new(x, text.y + y as u16, self.pager.column_width(), 1);
                let mut para = Paragraph::new(line.text.as_str()).style(style);
                if line.heading {
                    para = para.centered().style(style.add_modifier(Modifier::BOLD));
                }
                frame.render_widget(para, rect);
            }
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
        let popup = centered(screen, 48, 17 + u16::from(notice.is_some()));
        frame.render_widget(Clear, popup);
        let block = Block::bordered().title(" Чтение ").style(style);
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
            "   Продолжить чтение       Esc".to_owned(),
            "   Предыдущая страница     ←".into(),
            "   Следующая страница      →".into(),
            format!(" {single} Одна страница            1"),
            format!(" {wide} Разворот                2"),
            format!("   Тема: {}     T", self.theme.name()),
            "   Оглавление              O".into(),
            "   Поиск по книге          /".into(),
            format!(
                "   {}          B",
                if self.marks.contains(&self.pager.anchor()) {
                    "Убрать закладку"
                } else {
                    "Поставить закладку"
                }
            ),
            "   Закладки                G".into(),
            "   Уменьшить поля          [".into(),
            "   Увеличить поля          ]".into(),
            "   Закрыть книгу           Q".into(),
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

    fn apply(&mut self, action: Action) {
        match action {
            Action::Navigate(kind) => {
                self.menu = None;
                self.nav = Some(navigation::Navigation::new(
                    kind,
                    &self.doc,
                    self.pager.anchor(),
                    &self.marks,
                ));
            }
            Action::NavEdit(Edit::Back) => self.nav = None,
            Action::NavEdit(e) => {
                if let Some(place) = self.nav.as_mut().and_then(|n| n.edit(&self.doc, e)) {
                    self.pager.seek(place);
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
            Action::Resize | Action::Close => (),
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
                notices.push(format!("Не удалось сохранить место чтения: {e}"));
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
        return Err(io::Error::other("Слишком большой reader-keys.json"));
    }
    let raw: HashMap<char, String> = serde_json::from_reader(f.take(16385))?;
    raw.into_iter()
        .map(|(key, name)| {
            let action = match name.as_str() {
                "next" => Action::Next,
                "previous" => Action::Previous,
                "menu" => Action::Menu,
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
                _ => return Err(io::Error::other(format!("Неизвестная команда {name}"))),
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
    read_server(term, path, doc, notices, None).map(|_| ())
}
pub fn read_server(
    term: &mut AppTerminal,
    path: &Path,
    doc: Document,
    notices: &mut Vec<String>,
    base: Option<&botui_bookorbit::progress::Progress>,
) -> io::Result<Option<botui_bookorbit::progress::Move>> {
    let before = notices.len();
    let (mut state, saved) = match LocalState::for_file(path).and_then(|store| {
        let saved = store.load()?;
        Ok((Some(store), saved))
    }) {
        Ok(pair) => pair,
        Err(err) => {
            notices.push(format!("Локальное состояние недоступно: {err}"));
            (None, None)
        }
    };
    let size = term.size()?;
    let mut reader = Reader::new(doc, Rect::new(0, 0, size.width, size.height), saved);
    match bindings() {
        Ok(map) => reader.bindings = map,
        Err(e) => notices.push(format!("Не удалось прочитать клавиши читалки: {e}")),
    }
    let mut can_sync = base.is_some();
    if let Some(p) = base {
        if let Some(cfi) = p.text.cfi.as_deref() {
            if let Some(a) = reader.doc.resolve_cfi(cfi) {
                reader.pager.seek(a);
                reader.dirty = true;
            } else {
                can_sync = false;
                notices.push(
                    "Адрес BookOrbit не найден в тексте. Серверное место останется прежним.".into(),
                );
            }
        } else if p.text.page.is_some() || p.percentage > 0.0 {
            can_sync = false;
            notices.push(
                "На сервере нет точного адреса текста. Используется локальная позиция.".into(),
            );
        }
    }
    let start = reader.pager.anchor();
    let mut last_save = Instant::now();
    let result: io::Result<()> = (|| {
        loop {
            term.draw(|frame| reader.draw(frame, notices.get(before).map(String::as_str)))?;
            let action = loop {
                if reader.dirty {
                    let remaining = Duration::from_millis(600).saturating_sub(last_save.elapsed());
                    if remaining.is_zero() || !event::poll(remaining)? {
                        reader.flush(&mut state, notices);
                        last_save = Instant::now();
                    }
                }
                if let Some(action) = reader.input(event::read()?) {
                    break action;
                }
            };
            if action == Action::Close {
                break;
            }
            reader.apply(action);
        }
        Ok(())
    })();
    reader.flush(&mut state, notices);
    result?;
    let a = reader.pager.anchor();
    if can_sync && a != start {
        if let Some(cfi) = reader.doc.cfi(a) {
            return Ok(Some(botui_bookorbit::progress::Move {
                target: botui_bookorbit::progress::Target::Text(cfi),
                percentage: reader.doc.text_percentage(a),
            }));
        }
        notices.push(
            "Этот фрагмент не удалось связать с адресом BookOrbit. Место сохранено локально."
                .into(),
        );
    }
    Ok(None)
}
