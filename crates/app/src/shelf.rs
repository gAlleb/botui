pub(crate) mod background;
mod covers;
mod gallery;
mod login;
mod opds;
mod rail;
mod view;

use botui_bookorbit::progress::{Move, Progress};
use botui_bookorbit::{Book, BookFile, BookOrbit, BooksPage, Collection, Library, Session};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    widgets::ListState,
};
use std::{
    io,
    sync::{Arc, atomic::AtomicBool, mpsc::TryRecvError},
    time::Duration,
};
use tempfile::TempPath;

use crate::AppTerminal;
use background::{Ticket, Worker};

pub(crate) enum Selection {
    Opds,
    Local,
    Book(TempPath, Option<Progress>),
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    All,
    Libraries,
    Collections,
}

impl Tab {
    const ALL: [Self; 3] = [Self::All, Self::Libraries, Self::Collections];
    fn adjacent(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(i + if forward { 1 } else { 2 }) % 3]
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Scope {
    All,
    Library(Library),
    Collection(Collection),
}
impl Scope {
    fn name(&self) -> &str {
        match self {
            Self::All => "Все книги",
            Self::Library(lib) => &lib.name,
            Self::Collection(c) => &c.name,
        }
    }
    fn tab(&self) -> Tab {
        match self {
            Self::All => Tab::All,
            Self::Library(_) => Tab::Libraries,
            Self::Collection(_) => Tab::Collections,
        }
    }
}

#[derive(Clone)]
enum Command {
    Opds,
    Feed(botui_opds::Address, bool),
    Connect,
    OpenTab(Tab),
    Books(Scope, u32),
    Files(Book),
    File(BookFile),
    Back,
    Local,
    Quit,
    NextPage,
    PrevPage,
    Info,
}

struct Row {
    label: String,
    action: Command,
}

impl Row {
    fn new(label: impl Into<String>, action: Command) -> Self {
        Self {
            label: single_line(label.into()),
            action,
        }
    }
}

enum Section {
    Feed {
        next: Option<botui_opds::Address>,
        prev: Option<botui_opds::Address>,
    },
    Home,
    Libraries,
    Collections,
    Books {
        scope: Scope,
        page: u32,
        has_next: bool,
    },
    Files(Book),
}

struct Pane {
    section: Section,
    title: String,
    rows: Vec<Row>,
    selection: ListState,
}
impl Pane {
    fn new(section: Section, title: impl Into<String>, rows: Vec<Row>) -> Self {
        let selection = ListState::default().with_selected((!rows.is_empty()).then_some(0));
        Self {
            section,
            title: single_line(title.into()),
            rows,
            selection,
        }
    }
    fn home(connected: bool) -> Self {
        let mut rows = vec![
            Row::new(
                if connected {
                    "Все книги"
                } else {
                    "Подключиться к BookOrbit"
                },
                Command::OpenTab(Tab::All),
            ),
            Row::new("Открыть локальный файл…", Command::Local),
            Row::new("Открыть каталог OPDS…", Command::Opds),
        ];
        if connected {
            rows.push(Row::new("Сменить подключение…", Command::Connect));
        }
        rows.push(Row::new("Выйти", Command::Quit));
        Self::new(Section::Home, "Библиотека", rows)
    }
    fn move_by(&mut self, delta: isize) {
        if let Some(i) = self.selection.selected() {
            self.selection.select(Some(
                i.saturating_add_signed(delta).min(self.rows.len() - 1),
            ));
        }
    }
    fn action(&self) -> Option<Command> {
        self.selection
            .selected()
            .and_then(|i| self.rows.get(i))
            .map(|r| r.action.clone())
    }
}

enum Loaded {
    Feed(Arc<botui_opds::Catalog>, botui_opds::Feed, bool),
    Login {
        server: String,
        username: String,
        session: Session,
    },
    Collections(Vec<Collection>),
    Libraries(Vec<Library>),
    Books(Scope, BooksPage),
    File(TempPath, Option<Progress>, Option<String>),
    Moved(bool),
    LoggedOut {
        quit: bool,
        warning: Option<String>,
    },
}

struct Pending {
    title: &'static str,
    closing: bool,
    work: Ticket<botui_bookorbit::Result<Loaded>>,
}

pub(crate) struct Shelf {
    is_opds: bool,
    links: opds::Links,
    catalog: Option<Arc<botui_opds::Catalog>>,
    server: String,
    username: String,
    session: Option<Arc<Session>>,
    stack: Vec<Pane>,
    login: Option<login::Login>,
    info: Option<String>,
    scan_file: Option<BookFile>,
    scan_button: Rect,
    notice: Option<String>,
    exit_warning: Option<String>,
    worker: Worker,
    pending: Option<Pending>,
    covers: covers::Covers,
    graphics: ratatui_image::picker::Picker,
    rail: rail::Rail,
    grid: Option<gallery::Grid>,
    pub(crate) grid_view: bool,
    view_button: Rect,
    list_area: Rect,
    back_button: Rect,
    file_button: Rect,
    tab_buttons: [Rect; 3],
}

const PAGE_SIZE: u32 = 24;

impl Shelf {
    pub fn new(server: String, username: String) -> Self {
        Self {
            server,
            username,
            is_opds: false,
            catalog: None,
            links: opds::Links::default(),
            session: None,
            stack: vec![Pane::home(false)],
            login: None,
            info: None,
            scan_file: None,
            scan_button: Rect::default(),
            notice: None,
            exit_warning: None,
            worker: Worker::new(),
            pending: None,
            covers: covers::Covers::new(),
            graphics: crate::cover::graphics(),
            rail: rail::Rail::default(),
            grid: None,
            grid_view: true,
            view_button: Rect::default(),
            list_area: Rect::default(),
            back_button: Rect::default(),
            file_button: Rect::default(),
            tab_buttons: [Rect::default(); 3],
        }
    }

    pub fn set_error(&mut self, message: String) {
        self.notice = Some(message);
    }
    pub fn take_exit_warning(&mut self) -> Option<String> {
        self.exit_warning.take()
    }

    fn current(&self) -> &Pane {
        self.stack.last().expect("home always present")
    }
    fn current_mut(&mut self) -> &mut Pane {
        self.stack.last_mut().expect("home always present")
    }

    fn fetch(
        &mut self,
        title: &'static str,
        task: impl FnOnce(&AtomicBool) -> botui_bookorbit::Result<Loaded> + Send + 'static,
    ) {
        self.pending = Some(Pending {
            title,
            closing: false,
            work: self.worker.start(task),
        });
    }

    fn login_screen(&mut self) {
        self.login = Some(login::Login::new(
            self.server.clone(),
            self.username.clone(),
        ));
    }

    fn submit(&mut self) {
        if self.is_opds {
            return self.submit_opds();
        }
        let Some(form) = &mut self.login else { return };
        if form.server.trim().is_empty()
            || form.username.trim().is_empty()
            || form.password.is_empty()
        {
            self.notice = Some("Введите адрес, логин и пароль.".into());
            return;
        }
        let server = form.server.trim().to_owned();
        let username = form.username.trim().to_owned();
        let password = std::mem::take(&mut form.password);
        form.focus_password();
        self.notice = None;
        self.fetch("Входим в BookOrbit…", move |cancel| {
            let api = BookOrbit::new(&server)?;
            let session = api.sign_in(&username, &password)?;
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                let _ = session.logout();
                return Err("Вход отменён.".into());
            }
            Ok(Loaded::Login {
                server,
                username,
                session,
            })
        });
    }

    fn view_available(&self) -> bool {
        matches!(
            self.current().section,
            Section::Feed { .. }
                | Section::Libraries
                | Section::Collections
                | Section::Books { .. }
        )
    }

    fn active_tab(&self) -> Tab {
        self.stack
            .iter()
            .rev()
            .find_map(|pane| match &pane.section {
                Section::Books { scope, .. } => Some(scope.tab()),
                Section::Libraries => Some(Tab::Libraries),
                Section::Collections => Some(Tab::Collections),
                _ => None,
            })
            .unwrap_or(Tab::All)
    }

    fn all_books_index(&self) -> Option<usize> {
        self.stack.iter().position(|p| {
            matches!(
                p.section,
                Section::Books {
                    scope: Scope::All,
                    ..
                }
            )
        })
    }

    fn open_tab(&mut self, tab: Tab) {
        let Some(session) = self.session.clone() else {
            self.login_screen();
            return;
        };
        match tab {
            Tab::All => {
                if let Some(i) = self.all_books_index() {
                    self.stack.truncate(i + 1);
                } else {
                    self.books(Scope::All, 0);
                }
            }
            Tab::Libraries => self.fetch("Загружаем библиотеки…", move |_| {
                session
                    .with_token(|api, token| api.libraries(token))
                    .map(Loaded::Libraries)
            }),
            Tab::Collections => self.fetch("Загружаем коллекции…", move |_| {
                session
                    .with_token(|api, token| api.collections(token))
                    .map(Loaded::Collections)
            }),
        }
    }

    fn show_directory(&mut self, section: Section, title: &str, rows: Vec<Row>) {
        // Back from either directory restores the original page and selected book.
        self.stack
            .truncate(self.all_books_index().map_or(1, |i| i + 1));
        self.stack.push(Pane::new(section, title, rows));
    }

    fn books(&mut self, scope: Scope, page: u32) {
        let Some(session) = self.session.clone() else {
            self.login_screen();
            return;
        };
        self.fetch("Загружаем книги…", move |_| {
            let result = session.with_token(|api, token| match &scope {
                Scope::All => api.all_books(token, page, PAGE_SIZE),
                Scope::Library(lib) => api.library_books(token, lib.id, page, PAGE_SIZE),
                Scope::Collection(c) => api.books(token, c.id, page, PAGE_SIZE),
            })?;
            Ok(Loaded::Books(scope, result))
        });
    }

    fn open_files(&mut self, book: Book) {
        let rows = book
            .files
            .iter()
            .cloned()
            .map(|file| Row::new(file_label(&file), Command::File(file)))
            .collect();
        self.stack.push(Pane::new(
            Section::Files(book.clone()),
            book.title.unwrap_or_else(|| "Без названия".into()),
            rows,
        ));
    }

    fn open_file(&mut self, file: BookFile) {
        if !file
            .format
            .as_deref()
            .and_then(botui_formats::FileKind::from_extension)
            .is_some_and(botui_formats::FileKind::readable)
        {
            self.scan_file = file
                .format
                .as_deref()
                .and_then(botui_formats::FileKind::from_extension)
                .filter(|k| !k.readable())
                .map(|_| file.clone());
            self.info = Some(format!(
                "{}\n\n{}\n\n{}",
                self.current().title,
                file_label(&file).trim(),
                if self.scan_file.is_some() {
                    "Можно открыть постраничный просмотр."
                } else {
                    "Просмотр этого формата пока недоступен."
                }
            ));
            return;
        }
        self.download(file);
    }
    fn request_covers(&mut self, ids: &[u64]) {
        for &id in ids {
            if let Some(session) = self.session.clone() {
                self.covers.request(id, move || {
                    session.with_token(|api, t| api.thumbnail(t, id))
                });
            } else if let Some(cat) = self.catalog.clone()
                && let Some(href) = self.links.covers.get(&id).cloned()
            {
                self.covers.request(id, move || cat.cover(href).map(Some));
            }
        }
    }
    fn download(&mut self, file: BookFile) {
        if let Some(cat) = self.catalog.clone()
            && let Some(href) = self.links.file(file.id)
        {
            let Some(kind) = file
                .format
                .as_deref()
                .and_then(botui_formats::FileKind::from_extension)
            else {
                return;
            };
            self.fetch("Получаем файл из OPDS…", move |cancel| {
                let mut f = tempfile::Builder::new()
                    .prefix("botui-opds-")
                    .suffix(&format!(".{}", kind.label().to_lowercase()))
                    .tempfile()?;
                cat.download(href, f.as_file_mut(), cancel)?;
                Ok(Loaded::File(f.into_temp_path(), None, None))
            });
            return;
        }
        let Some(session) = self.session.clone() else {
            self.login_screen();
            return;
        };
        self.fetch("Получаем файл книги…", move |cancel| {
            let path =
                session.with_token(|api, token| crate::cache::fetch(api, token, &file, cancel))?;
            let (base, note) = match session.with_token(|api, t| api.progress(t, file.id)) {
                Ok(p) => (Some(p), None),
                Err(_) => (
                    None,
                    Some("Позиция BookOrbit недоступна. Место сохраняется на устройстве.".into()),
                ),
            };
            Ok(Loaded::File(path, base, note))
        });
    }

    pub fn save_move(&mut self, base: Progress, change: Move) {
        if let Some(session) = self.session.clone() {
            self.fetch("Сохраняем место в BookOrbit…", move |_| {
                session
                    .with_token(|api, t| api.save_move(t, &base, &change))
                    .map(Loaded::Moved)
            });
        }
    }
    fn page(&mut self, forward: bool) {
        if let Section::Feed { next, prev } = &self.current().section {
            if let Some(href) = if forward { next } else { prev } {
                self.fetch_feed(href.clone(), true);
            }
            return;
        }
        if let Section::Books {
            scope,
            page,
            has_next,
        } = &self.current().section
        {
            let target = if forward && *has_next {
                page.checked_add(1)
            } else if !forward && *page > 0 {
                page.checked_sub(1)
            } else {
                None
            };
            if let Some(target) = target {
                self.books(scope.clone(), target)
            }
        }
    }

    fn back(&mut self) -> Option<Selection> {
        self.notice = None;
        if self.pending.take().is_some() {
            return None;
        }
        if self.is_opds && (self.login.is_some() || self.stack.len() == 2 && self.info.is_none()) {
            return Some(Selection::Quit);
        }
        if self.info.take().is_some() || self.login.take().is_some() {
            return None;
        }
        if self.stack.len() == 1 {
            return self.end_session(true);
        }
        self.stack.pop();
        None
    }

    fn end_session(&mut self, quit: bool) -> Option<Selection> {
        if self.pending.as_ref().is_some_and(|p| p.closing) {
            return None;
        }
        self.pending = None;
        self.covers.reset();
        self.rail = rail::Rail::default();
        let Some(session) = self.session.take() else {
            if quit {
                return Some(Selection::Quit);
            }
            self.login_screen();
            return None;
        };
        self.stack = vec![Pane::home(false)];
        // Logout must not wait behind a document download that is being cancelled.
        self.pending = Some(Pending {
            title: "Завершаем сеанс…",
            closing: true,
            work: Worker::new().start(move |_| {
                let warning = session
                    .logout()
                    .err()
                    .map(|err| format!("Не удалось завершить сеанс на сервере: {err}"));
                Ok(Loaded::LoggedOut { quit, warning })
            }),
        });
        None
    }

    fn apply(&mut self, cmd: Command) -> Option<Selection> {
        self.notice = None;
        match cmd {
            Command::Opds => return Some(Selection::Opds),
            Command::Feed(href, replace) => self.fetch_feed(href, replace),
            Command::Back => return self.back(),
            Command::Local => {
                self.pending = None;
                return Some(Selection::Local);
            }
            Command::Quit => return self.end_session(true),
            Command::Connect => return self.end_session(false),
            Command::OpenTab(tab) => self.open_tab(tab),
            Command::Books(c, p) => self.books(c, p),
            Command::Files(book) => self.open_files(book),
            Command::File(file) => self.open_file(file),
            Command::NextPage => self.page(true),
            Command::PrevPage => self.page(false),
            Command::Info => {
                self.info = None;
            }
        }
        None
    }

    fn complete(&mut self, loaded: Loaded) -> Option<Selection> {
        match loaded {
            Loaded::Feed(c, f, r) => self.show_feed(c, f, r),
            Loaded::Login {
                server,
                username,
                session,
            } => {
                self.server = server;
                self.username = username;
                self.login = None;
                self.session = Some(Arc::new(session));
                self.stack = vec![Pane::home(true)];
                if let Err(e) =
                    crate::connection::remember(&self.server, &self.username, !self.grid_view)
                {
                    self.notice = Some(format!("Не удалось запомнить адрес и логин: {e}"));
                }
                self.open_tab(Tab::All);
            }
            Loaded::Collections(items) => {
                let rows = items
                    .into_iter()
                    .map(|c| {
                        Row::new(
                            format!("{}   ·   {}", c.name, c.book_count),
                            Command::Books(Scope::Collection(c), 0),
                        )
                    })
                    .collect();
                self.show_directory(Section::Collections, "Коллекции", rows);
            }
            Loaded::Libraries(items) => {
                let rows = items
                    .into_iter()
                    .map(|lib| {
                        let label = lib.book_count.map_or_else(
                            || lib.name.clone(),
                            |count| format!("{}   ·   {count}", lib.name),
                        );
                        Row::new(label, Command::Books(Scope::Library(lib), 0))
                    })
                    .collect();
                self.show_directory(Section::Libraries, "Библиотеки", rows);
            }
            Loaded::Books(scope, books) => {
                let mut rows = books
                    .items
                    .into_iter()
                    .map(|b| {
                        let title = b.title.as_deref().unwrap_or("Без названия");
                        let mut formats = Vec::new();
                        for format in b.files.iter().filter_map(|file| file.format.as_deref()) {
                            let format = format.to_uppercase();
                            if !format.is_empty() && !formats.contains(&format) {
                                formats.push(format);
                            }
                        }
                        let mut label = match formats.is_empty() {
                            true => title.to_owned(),
                            false => format!("[{}]  {title}", formats.join(" / ")),
                        };
                        if !b.authors.is_empty() {
                            label.push_str("   ·   ");
                            label.push_str(&b.authors.join(", "));
                        }
                        Row::new(label, Command::Files(b))
                    })
                    .collect::<Vec<_>>();
                let pages = books.total.div_ceil(u64::from(books.size.max(1))).max(1);
                let displayed_page = u64::from(books.page) + 1;
                let has_next = displayed_page < pages;
                if books.page > 0 {
                    rows.push(Row::new("‹ Предыдущая страница", Command::PrevPage))
                }
                if has_next {
                    rows.push(Row::new("Следующая страница ›", Command::NextPage))
                }
                let title = format!("{}   ·   {}/{}", scope.name(), displayed_page, pages);
                if scope == Scope::All {
                    self.stack.truncate(1);
                } else if matches!(&self.current().section, Section::Books { scope: old,.. } if *old==scope)
                {
                    self.stack.pop();
                }
                self.stack.push(Pane::new(
                    Section::Books {
                        scope,
                        page: books.page,
                        has_next,
                    },
                    title,
                    rows,
                ));
            }
            Loaded::File(path, base, note) => {
                self.notice = note;
                return Some(Selection::Book(path, base));
            }
            Loaded::Moved(false) => {
                self.notice = Some(
                    "Место на сервере уже изменилось. Ваше место сохранено на устройстве.".into(),
                )
            }
            Loaded::Moved(true) => (),
            Loaded::LoggedOut { quit, warning } => {
                if quit {
                    self.exit_warning = warning;
                    return Some(Selection::Quit);
                }
                self.notice = warning;
                self.login_screen();
            }
        }
        None
    }

    fn collect_results(&mut self) -> Option<Selection> {
        self.covers.collect();
        self.rail.collect();
        let result = match self.pending.as_ref()?.work.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("Фоновый запрос был прерван.".into()),
        };
        self.pending = None;
        match result {
            Ok(loaded) => self.complete(loaded),
            Err(e) => {
                if botui_bookorbit::is_unauthorized(e.as_ref()) {
                    if self.login.is_some() {
                        self.notice = Some("Вход отклонён (401). Проверьте логин и пароль.".into());
                    } else {
                        self.notice = Some("Сессия истекла (401). Введите пароль ещё раз.".into());
                        self.session = None;
                        self.stack = vec![Pane::home(false)];
                        self.covers.reset();
                        self.rail = rail::Rail::default();
                        self.login_screen();
                        if let Some(form) = &mut self.login {
                            form.focus_password();
                        }
                    }
                } else if botui_bookorbit::is_forbidden(e.as_ref()) {
                    self.notice = Some("Нет доступа к запрошенным данным (403).".into());
                } else {
                    self.notice = Some(e.to_string());
                }
                None
            }
        }
    }

    fn input(&mut self, event: Event) -> Option<Selection> {
        let click = match &event {
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                Some(Position::new(m.column, m.row))
            }
            _ => None,
        };
        let key = match &event {
            Event::Key(k) if k.kind != KeyEventKind::Release => Some(k.code),
            _ => None,
        };
        if let Event::Key(k) = &event
            && k.kind != KeyEventKind::Release
            && k.code == KeyCode::Char('c')
            && k.modifiers.contains(KeyModifiers::CONTROL)
        {
            return self.apply(Command::Quit);
        }
        if self.pending.as_ref().is_some_and(|p| p.closing) {
            return None;
        }
        if self.pending.is_some() {
            if key == Some(KeyCode::Char('q')) {
                return self.end_session(true);
            }
            if key == Some(KeyCode::Char('o')) {
                return self.apply(Command::Local);
            }
            if click.is_some_and(|p| self.file_button.contains(p)) {
                return self.apply(Command::Local);
            }
            if key == Some(KeyCode::Esc) || click.is_some_and(|p| self.back_button.contains(p)) {
                return self.back();
            }
            return None;
        }
        if let Some(form) = &mut self.login {
            match form.input(event) {
                Some(login::Command::Back) => return self.back(),
                Some(login::Command::Submit) => self.submit(),
                None => {}
            }
            return None;
        }
        if self.info.is_some() {
            if (matches!(key, Some(KeyCode::Char('p' | 'P')))
                || click.is_some_and(|p| self.scan_button.contains(p)))
                && let Some(file) = self.scan_file.clone()
            {
                self.download(file);
                return None;
            }
            if matches!(
                key,
                Some(KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace)
            ) || click.is_some_and(|p| self.back_button.contains(p))
            {
                return self.apply(Command::Info);
            }
            return None;
        }
        if click.is_some_and(|p| self.back_button.contains(p)) {
            return self.back();
        }
        if click.is_some_and(|p| self.file_button.contains(p)) {
            return self.apply(Command::Local);
        }
        if self.session.is_some() {
            let tab = match key {
                Some(KeyCode::Char('1')) => Some(Tab::All),
                Some(KeyCode::Char('2')) => Some(Tab::Libraries),
                Some(KeyCode::Char('3')) => Some(Tab::Collections),
                Some(KeyCode::Tab) => Some(self.active_tab().adjacent(true)),
                Some(KeyCode::BackTab) => Some(self.active_tab().adjacent(false)),
                _ => click
                    .and_then(|pos| self.tab_buttons.iter().position(|r| r.contains(pos)))
                    .map(|i| Tab::ALL[i]),
            };
            if let Some(tab) = tab {
                return self.apply(Command::OpenTab(tab));
            }
        }
        if let Some(cmd) = click.and_then(|p| self.rail.hit(p)) {
            if matches!(cmd, Command::Books(..))
                && let Some(i) = self.all_books_index()
            {
                self.stack.truncate(i + 1);
            }
            return self.apply(cmd);
        }
        if self.view_available()
            && (matches!(key, Some(KeyCode::Char('v' | 'V')))
                || click.is_some_and(|pos| self.view_button.contains(pos)))
        {
            self.grid_view = !self.grid_view;
            if !self.is_opds
                && let Err(e) =
                    crate::connection::remember(&self.server, &self.username, !self.grid_view)
            {
                self.set_error(format!("Не удалось запомнить вид полки: {e}"));
            }
            return None;
        }
        if let Some(grid) = &self.grid {
            if let Some(pos) = click
                && self.list_area.contains(pos)
            {
                if let Some(i) = grid.hit(pos) {
                    self.current_mut().selection.select(Some(i));
                    let action = self.current().action()?;
                    return self.apply(action);
                }
                return None;
            }
            let step = grid.cols as isize;
            let delta = match (&event, key) {
                (_, Some(KeyCode::Left | KeyCode::Char('h'))) => -1,
                (_, Some(KeyCode::Right | KeyCode::Char('l'))) => 1,
                (_, Some(KeyCode::Up | KeyCode::Char('k'))) => -step,
                (_, Some(KeyCode::Down | KeyCode::Char('j'))) => step,
                (Event::Mouse(m), _) if self.list_area.contains(Position::new(m.column, m.row)) => {
                    match m.kind {
                        MouseEventKind::ScrollUp => -step,
                        MouseEventKind::ScrollDown => step,
                        _ => 0,
                    }
                }
                _ => 0,
            };
            if delta != 0 {
                self.current_mut().move_by(delta);
                return None;
            }
        }
        let list_area = self.list_area;
        let action = match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => Some(Command::Back),
                KeyCode::Char('q') => Some(Command::Quit),
                KeyCode::Char('o') => Some(Command::Local),
                KeyCode::Enter | KeyCode::Right => self.current().action(),
                KeyCode::PageDown
                    if matches!(
                        self.current().section,
                        Section::Books { .. } | Section::Feed { .. }
                    ) =>
                {
                    Some(Command::NextPage)
                }
                KeyCode::PageUp
                    if matches!(
                        self.current().section,
                        Section::Books { .. } | Section::Feed { .. }
                    ) =>
                {
                    Some(Command::PrevPage)
                }
                KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End => {
                    let delta = match k.code {
                        KeyCode::Home => isize::MIN,
                        KeyCode::End => isize::MAX,
                        KeyCode::PageUp => -(list_area.height as isize),
                        _ => list_area.height as isize,
                    };
                    self.current_mut().move_by(delta);
                    None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.current_mut().move_by(-1);
                    None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.current_mut().move_by(1);
                    None
                }
                _ => None,
            },
            Event::Mouse(m) if list_area.contains(Position::new(m.column, m.row)) => match m.kind {
                MouseEventKind::ScrollUp => {
                    self.current_mut().move_by(-1);
                    None
                }
                MouseEventKind::ScrollDown => {
                    self.current_mut().move_by(1);
                    None
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    let pane = self.current_mut();
                    let i = pane.selection.offset() + usize::from(m.row - list_area.y);
                    if i < pane.rows.len() {
                        pane.selection.select(Some(i));
                        pane.action()
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        };
        action.and_then(|cmd| self.apply(cmd))
    }

    pub fn choose(&mut self, term: &mut AppTerminal) -> io::Result<Selection> {
        loop {
            if let Some(choice) = self.collect_results() {
                return Ok(choice);
            }
            term.draw(|frame| self.draw(frame))?;
            if (self.pending.is_some() || self.covers.busy() || self.rail.busy())
                && !event::poll(Duration::from_millis(80))?
            {
                continue;
            }
            if let Some(choice) = self.input(event::read()?) {
                return Ok(choice);
            }
        }
    }
}

// A list item has exactly one terminal row, including metadata supplied by a server.
fn single_line(mut text: String) -> String {
    if text.chars().any(char::is_control) {
        text = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
    }
    text
}

fn file_label(file: &BookFile) -> String {
    let format = file
        .format
        .as_deref()
        .unwrap_or("неизвестный формат")
        .to_uppercase();
    let size = file.size_bytes.map(|bytes| {
        if bytes < 1024 {
            format!("{bytes} Б")
        } else if bytes < 1024 * 1024 {
            format!("{} КиБ", bytes.div_ceil(1024))
        } else {
            format!("{:.1} МиБ", bytes as f64 / 1048576.0)
        }
    });
    let role = match file.role.as_str() {
        "primary" => "основной",
        "supplement" => "дополнение",
        "cover" => "обложка",
        _ => "",
    };
    [
        Some(format),
        size,
        (!role.is_empty()).then(|| role.to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("   ·   ")
}

#[cfg(test)]
mod navigation_tests;
