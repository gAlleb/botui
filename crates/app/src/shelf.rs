pub(crate) mod background;
pub(crate) mod covers;
pub(crate) mod gallery;
mod login;
mod native;
mod opds;
mod rail;
mod view;

use crate::catalog::{Book, BookFile};
use botui_bookorbit::progress::{Move, Progress};
use botui_bookorbit::{BookOrbit, BooksPage, Collection, Library, Session};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use native::{book_rows, fetch_books};
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
    Downloads,
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
    Reading,
    Recent,
}

impl Tab {
    const ALL: [Self; 5] = [
        Self::All,
        Self::Libraries,
        Self::Collections,
        Self::Reading,
        Self::Recent,
    ];
    fn adjacent(self, forward: bool) -> Self {
        let i = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(i + if forward { 1 } else { Self::ALL.len() - 1 }) % Self::ALL.len()]
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Scope {
    All,
    Library(Library),
    Collection(Collection),
    Reading,
    Recent,
    Search(String),
}
impl Scope {
    fn name(&self) -> &str {
        match self {
            Self::All => "All books",
            Self::Reading => "Currently reading",
            Self::Recent => "Recently added",
            Self::Search(q) => q,
            Self::Library(lib) => &lib.name,
            Self::Collection(c) => &c.name,
        }
    }
    fn tab(&self) -> Tab {
        match self {
            Self::Reading => Tab::Reading,
            Self::Recent => Tab::Recent,
            Self::Search(_) | Self::All => Tab::All,
            Self::Library(_) => Tab::Libraries,
            Self::Collection(_) => Tab::Collections,
        }
    }
}

#[derive(Clone)]
enum Command {
    Downloads,
    Background,
    Opds,
    Feed(botui_opds::Address),
    Connect,
    OpenTab(Tab),
    Books(Scope, u32),
    Files(Book),
    File(BookFile),
    Back,
    Local,
    Quit,
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
        visited: std::collections::HashSet<botui_opds::Address>,
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

enum Tail {
    Books(BooksPage),
    Feed(botui_opds::Address, Box<botui_opds::Feed>),
}

struct Pane {
    section: Section,
    title: String,
    rows: Vec<Row>,
    tail: Option<Ticket<botui_bookorbit::Result<Tail>>>,
    tail_error: bool,
    selection: ListState,
}
impl Pane {
    fn new(section: Section, title: impl Into<String>, rows: Vec<Row>) -> Self {
        let selection = ListState::default().with_selected((!rows.is_empty()).then_some(0));
        Self {
            section,
            tail: None,
            tail_error: false,
            title: single_line(title.into()),
            rows,
            selection,
        }
    }
    fn home(connected: bool) -> Self {
        let mut rows = vec![
            Row::new(
                if connected {
                    "All books"
                } else {
                    "Connect to BookOrbit"
                },
                Command::OpenTab(Tab::All),
            ),
            Row::new("Browse local library…", Command::Local),
            Row::new("Open OPDS catalog…", Command::Opds),
            Row::new("Offline downloads", Command::Downloads),
            Row::new("Toggle opaque background   F", Command::Background),
        ];
        if connected {
            rows.push(Row::new("Change connection…", Command::Connect));
        }
        rows.push(Row::new("Quit", Command::Quit));
        Self::new(Section::Home, "Library", rows)
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
    Feed(
        Arc<botui_opds::Catalog>,
        Box<botui_opds::Feed>,
        botui_opds::Address,
    ),
    Login {
        server: String,
        username: String,
        session: Session,
        offline_notice: Option<String>,
    },
    Collections(Vec<Collection>),
    Libraries(Vec<Library>),
    Books(Scope, BooksPage),
    File(TempPath, Option<Progress>, Option<String>),
    NativeFile(
        TempPath,
        Option<Progress>,
        Option<String>,
        Option<crate::reader::bookmarks::ServerMarks>,
    ),
    Kept,
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
    pub(crate) bookmarks: Option<crate::reader::bookmarks::ServerMarks>,
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
    keep_button: Rect,
    tab_buttons: [Rect; 5],
    search_button: Rect,
}

const PAGE_SIZE: u32 = 24;

impl Shelf {
    pub fn new(server: String, username: String) -> Self {
        Self {
            server,
            username,
            is_opds: false,
            bookmarks: None,
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
            keep_button: Rect::default(),
            tab_buttons: [Rect::default(); 5],
            search_button: Rect::default(),
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
            self.notice = Some("Enter the address, username and password.".into());
            return;
        }
        let server = form.server.trim().to_owned();
        let username = form.username.trim().to_owned();
        let password = std::mem::take(&mut form.password);
        form.focus_password();
        self.notice = None;
        self.fetch("Signing in to BookOrbit…", move |cancel| {
            let api = BookOrbit::new(&server)?;
            let session = api.sign_in(&username, &password)?;
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                let _ = session.logout();
                return Err("Sign-in cancelled.".into());
            }
            let offline_notice = crate::offline::sync(
                &session,
                &crate::offline::account(&server, &username),
                cancel,
            )
            .unwrap_or_else(|e| Some(format!("Offline progress is pending: {e}")));
            Ok(Loaded::Login {
                offline_notice,
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
            Tab::Reading => self.books(Scope::Reading, 0),
            Tab::Recent => self.books(Scope::Recent, 0),
            Tab::All => {
                if let Some(i) = self.all_books_index() {
                    self.stack.truncate(i + 1);
                } else {
                    self.books(Scope::All, 0);
                }
            }
            Tab::Libraries => self.fetch("Loading libraries…", move |_| {
                session
                    .with_token(|api, token| api.libraries(token))
                    .map(Loaded::Libraries)
            }),
            Tab::Collections => self.fetch("Loading collections…", move |_| {
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
        self.fetch("Loading books…", move |_| {
            let result = fetch_books(&session, &scope, page)?;
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
            book.title.unwrap_or_else(|| "Untitled".into()),
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
                    "Open the original pages to read this book."
                } else {
                    "No reader available for this format."
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
        self.transfer(file, false);
    }
    fn transfer(&mut self, file: BookFile, keep: bool) {
        let title = self.current().title.clone();
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
            self.fetch("Downloading from OPDS…", move |cancel| {
                let mut f = tempfile::Builder::new()
                    .prefix("botui-opds-")
                    .suffix(&format!(".{}", kind.label().to_lowercase()))
                    .tempfile()?;
                cat.download(href, f.as_file_mut(), cancel)?;
                if keep {
                    crate::cache::keep(f.path(), &title, kind.label(), None, cancel)?;
                    Ok(Loaded::Kept)
                } else {
                    Ok(Loaded::File(f.into_temp_path(), None, None))
                }
            });
            return;
        }
        let Some(session) = self.session.clone() else {
            self.login_screen();
            return;
        };
        let book_id = match &self.current().section {
            Section::Files(b) => b.id,
            _ => return,
        };
        let account = crate::offline::account(&self.server, &self.username);
        self.fetch("Downloading book…", move |cancel| {
            let path =
                session.with_token(|api, token| crate::cache::fetch(api, token, &file, cancel))?;
            if keep {
                let dest = crate::cache::keep(
                    &path,
                    &title,
                    file.format.as_deref().unwrap_or(""),
                    Some((&account, file.id)),
                    cancel,
                )?;
                if let Ok(p) = session.with_token(|api, t| api.progress(t, file.id)) {
                    crate::offline::associate(&dest, account, p)?;
                }
                return Ok(Loaded::Kept);
            }
            let (base, note) = match session.with_token(|api, t| api.progress(t, file.id)) {
                Ok(p) => (Some(p), None),
                Err(_) => (
                    None,
                    Some("BookOrbit progress unavailable. Saving on this device.".into()),
                ),
            };
            let (marks, note) = if file
                .format
                .as_deref()
                .and_then(botui_formats::FileKind::from_extension)
                .is_some_and(botui_formats::FileKind::readable)
            {
                match crate::reader::bookmarks::ServerMarks::load(session.clone(), book_id, file.id)
                {
                    Ok(m) => (Some(m), note),
                    Err(_) => (
                        None,
                        Some(
                            [
                                note.as_deref(),
                                Some("Server bookmarks unavailable. Showing local bookmarks."),
                            ]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(" "),
                        ),
                    ),
                }
            } else {
                (None, note)
            };
            Ok(Loaded::NativeFile(path, base, note, marks))
        });
    }

    pub fn save_move(&mut self, base: Progress, change: Move) {
        if let Some(session) = self.session.clone() {
            self.fetch("Saving position to BookOrbit…", move |_| {
                session
                    .with_token(|api, t| api.save_move(t, &base, &change))
                    .map(Loaded::Moved)
            });
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
            title: "Signing out…",
            closing: true,
            work: Worker::new().start(move |_| {
                let warning = session
                    .logout()
                    .err()
                    .map(|err| format!("Could not sign out on the server: {err}"));
                Ok(Loaded::LoggedOut { quit, warning })
            }),
        });
        None
    }

    fn apply(&mut self, cmd: Command) -> Option<Selection> {
        self.notice = None;
        match cmd {
            Command::Background => {
                if let Err(e) = crate::settings::Settings::update(|s| {
                    s.opaque_background = !s.opaque_background
                }) {
                    self.notice = Some(e.to_string());
                }
            }
            Command::Downloads => return Some(Selection::Downloads),
            Command::Opds => return Some(Selection::Opds),
            Command::Feed(href) => self.fetch_feed(href),
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
            Command::Info => {
                self.info = None;
            }
        }
        None
    }

    fn complete(&mut self, loaded: Loaded) -> Option<Selection> {
        match loaded {
            Loaded::Feed(c, f, r) => self.show_feed(c, *f, r),
            Loaded::Login {
                offline_notice,
                server,
                username,
                session,
            } => {
                self.notice = offline_notice;
                self.server = server;
                self.username = username;
                self.login = None;
                self.session = Some(Arc::new(session));
                self.stack = vec![Pane::home(true)];
                if let Err(e) =
                    crate::connection::remember(&self.server, &self.username, !self.grid_view)
                {
                    self.notice = Some(format!("Could not save address and username: {e}"));
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
                self.show_directory(Section::Collections, "Collections", rows);
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
                self.show_directory(Section::Libraries, "Libraries", rows);
            }
            Loaded::Books(scope, books) => {
                let has_next = more_books(&books);
                let title = format!("{}  ·  {}", scope.name(), books.total);
                let rows = book_rows(books.items);
                if scope == Scope::All {
                    self.stack.truncate(1);
                } else if matches!(scope, Scope::Reading | Scope::Recent | Scope::Search(_)) {
                    self.stack
                        .truncate(self.all_books_index().map_or(1, |i| i + 1));
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
            Loaded::NativeFile(path, base, note, marks) => {
                self.bookmarks = marks;
                self.notice = note;
                return Some(Selection::Book(path, base));
            }
            Loaded::File(path, base, note) => {
                self.notice = note;
                return Some(Selection::Book(path, base));
            }
            Loaded::Kept => self.notice = Some("Saved in Offline downloads".into()),
            Loaded::Moved(false) => {
                self.notice = Some(
                    "Another device updated this book. This device kept its local position.".into(),
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

    fn viewport(&self) -> usize {
        self.grid
            .as_ref()
            .map_or(self.list_area.height as usize, |g| g.len())
            .max(1)
    }
    fn prefetch(&mut self) {
        let pane = self.current();
        let end = pane
            .selection
            .selected()
            .unwrap_or(0)
            .max(pane.selection.offset())
            .saturating_add(self.viewport() + 4);
        if pane.tail_error || pane.tail.is_some() || end < pane.rows.len() {
            return;
        }
        let ticket = match &pane.section {
            Section::Books {
                scope,
                page,
                has_next: true,
            } => {
                let Some(session) = self.session.clone() else {
                    return;
                };
                let next = page + 1;
                let scope = scope.clone();
                self.worker
                    .start(move |_| fetch_books(&session, &scope, next).map(Tail::Books))
            }
            Section::Feed {
                next: Some(url),
                visited,
            } if !visited.contains(url) => {
                let Some(cat) = self.catalog.clone() else {
                    return;
                };
                let url = url.clone();
                self.worker
                    .start(move |_| cat.feed(url.clone()).map(|f| Tail::Feed(url, Box::new(f))))
            }
            _ => return,
        };
        self.current_mut().tail = Some(ticket);
    }
    fn collect_tail(&mut self) {
        let Some(job) = &self.current().tail else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(_) => Err("Catalog request interrupted".into()),
        };
        let pane = self.current_mut();
        pane.tail = None;
        match result {
            Ok(Tail::Feed(url, feed)) => {
                let rows = self.feed_rows(feed.items);
                let pane = self.current_mut();
                if let Section::Feed { next, visited } = &mut pane.section {
                    visited.insert(url);
                    *next = feed.next;
                    pane.rows.extend(rows);
                }
            }
            Ok(Tail::Books(books)) => {
                if let Section::Books { page, has_next, .. } = &mut pane.section {
                    *has_next = more_books(&books) && books.page == *page + 1;
                    if books.page == *page + 1 {
                        *page = books.page;
                        pane.rows.extend(book_rows(books.items));
                    }
                }
            }
            Err(e) => {
                pane.tail_error = true;
                self.notice = Some(format!("{e} · R to retry"));
            }
        }
    }

    fn collect_results(&mut self) -> Option<Selection> {
        self.collect_tail();
        self.covers.collect();
        self.rail.collect();
        let result = match self.pending.as_ref()?.work.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("Background request interrupted.".into()),
        };
        self.pending = None;
        match result {
            Ok(loaded) => self.complete(loaded),
            Err(e) => {
                if botui_bookorbit::is_unauthorized(e.as_ref()) {
                    if self.login.is_some() {
                        self.notice =
                            Some("Sign-in declined (401). Check your credentials.".into());
                    } else {
                        self.notice = Some("Session expired (401). Sign in again.".into());
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
                    self.notice = Some("Access denied (403).".into());
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
        if matches!(key, Some(KeyCode::Char('f' | 'F'))) {
            return self.apply(Command::Background);
        }
        if (matches!(key, Some(KeyCode::Char('d' | 'D')))
            || click.is_some_and(|p| self.keep_button.contains(p)))
            && let Some(Command::File(file)) = self.current().action()
        {
            self.transfer(file, true);
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
                Some(KeyCode::Char('4')) => Some(Tab::Reading),
                Some(KeyCode::Char('5')) => Some(Tab::Recent),
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
                self.set_error(format!("Could not save shelf view: {e}"));
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
        let viewport = self.viewport() as isize;
        let action = match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => Some(Command::Back),
                KeyCode::Char('q') => Some(Command::Quit),
                KeyCode::Char('o') => Some(Command::Local),
                KeyCode::Enter | KeyCode::Right => self.current().action(),
                KeyCode::Char('r') if self.current().tail_error => {
                    self.current_mut().tail_error = false;
                    self.notice = None;
                    None
                }
                KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End => {
                    let delta = match k.code {
                        KeyCode::Home => isize::MIN,
                        KeyCode::End => isize::MAX,
                        KeyCode::PageUp => -viewport,
                        _ => viewport,
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
            term.draw(|frame| {
                self.draw(frame);
                crate::settings::paint(frame)
            })?;
            self.prefetch();
            if (self.pending.is_some()
                || self.current().tail.is_some()
                || self.covers.busy()
                || self.rail.busy())
                && !event::poll(Duration::from_millis(80))?
            {
                continue;
            }
            let e = event::read()?;
            if (self.session.is_some() || self.links.search.is_some())
                && self.pending.is_none()
                && self.info.is_none()
                && self.login.is_none()
                && (matches!(&e, Event::Key(k) if k.kind!=KeyEventKind::Release && k.code==KeyCode::Char('/'))
                    || matches!(&e, Event::Mouse(m) if m.kind==MouseEventKind::Down(MouseButton::Left) && self.search_button.contains(Position::new(m.column,m.row))))
            {
                let old = match &self.current().section {
                    Section::Books {
                        scope: Scope::Search(q),
                        ..
                    } => q.clone(),
                    _ => String::new(),
                };
                if let Some(q) = crate::prompt::edit(
                    term,
                    if self.is_opds {
                        "Search OPDS catalog"
                    } else {
                        "Search all books · title or author"
                    },
                    "[ Search ↵ ]",
                    &old,
                    |f| self.draw(f),
                )? {
                    if self.is_opds {
                        if !q.is_empty() {
                            self.search_feed(q);
                        }
                    } else if q.is_empty() {
                        self.open_tab(Tab::All)
                    } else {
                        self.books(Scope::Search(q), 0)
                    }
                }
                continue;
            }
            if let Some(choice) = self.input(e) {
                return Ok(choice);
            }
        }
    }
}

fn more_books(books: &BooksPage) -> bool {
    !books.items.is_empty()
        && (u64::from(books.page) + 1) * u64::from(books.size.max(1)) < books.total
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
        .unwrap_or("unknown format")
        .to_uppercase();
    let size = file.size_bytes.map(|bytes| {
        if bytes < 1024 {
            format!("{bytes} B")
        } else if bytes < 1024 * 1024 {
            format!("{} KiB", bytes.div_ceil(1024))
        } else {
            format!("{:.1} MiB", bytes as f64 / 1048576.0)
        }
    });
    let role = match file.role.as_str() {
        "primary" => "primary",
        "supplement" => "supplement",
        "cover" => "cover",
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
