//! Search the signed-in catalog with the same matching rules as the local shelf.
//! Only one metadata page is in flight. Leaving the search pauses pagination.
use super::{
    Command, Directory, Scope, Session, Shelf,
    background::{Ticket, Worker},
    more_books, native, single_line,
};
use crate::{
    catalog::Book,
    live_search::{Item, Search, Source},
};
use botui_bookorbit::{BooksPage, browse::DirectoryPage};
use std::{
    io,
    sync::{Arc, mpsc::TryRecvError},
    time::{Duration, Instant},
};

enum Page {
    Books(BooksPage),
    Groups(DirectoryPage),
}

pub(super) struct Catalog {
    session: Arc<Session>,
    directory: Option<Directory>,
    worker: Worker,
    next: Option<u32>,
    pending: Option<Ticket<botui_bookorbit::Result<Page>>>,
}
impl Source for Catalog {
    type Key = u64;
    type Action = Command;

    fn row_label<'a>(action: &Command, label: &'a str, width: u16) -> std::borrow::Cow<'a, str> {
        if let Command::Books(Scope::Group(_, group), _) = action {
            super::view::directory_line(
                label,
                &group.book_count.map(|n| n.to_string()).unwrap_or_default(),
                width,
            )
            .into()
        } else {
            label.into()
        }
    }
    fn busy(&self) -> bool {
        self.next.is_some() || self.pending.is_some()
    }
    fn collect(&mut self, batch: &mut Vec<Item<u64, Command>>) -> io::Result<()> {
        if let Some(ticket) = &self.pending {
            let result = match ticket.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => Err("Catalog request interrupted".into()),
            };
            self.pending = None;
            match result.map_err(|e| io::Error::other(e.to_string()))? {
                Page::Books(page) => {
                    self.next = more_books(&page).then(|| page.page + 1);
                    batch.extend(page.items.into_iter().map(book_item));
                }
                Page::Groups(page) => {
                    self.next = native::next_group_page(&page);
                    let directory = self.directory.expect("directory response");
                    for group in page.items {
                        let name = single_line(group.name.clone());
                        batch.push(Item {
                            key: group.id,
                            action: Command::Books(Scope::Group(directory, group), 0),
                            text: name.clone(),
                            label: name.clone(),
                            name,
                        });
                    }
                }
            }
        }
        if let Some(page) = self.next.take() {
            let session = self.session.clone();
            let directory = self.directory;
            self.pending = Some(self.worker.start(move |_| {
                session.with_token(|api, token| {
                    const SIZE: u32 = 100;
                    match directory {
                        Some(d) => api.directory(token, d, "", page, SIZE).map(Page::Groups),
                        None => api.all_books(token, page, SIZE).map(Page::Books),
                    }
                })
            }));
        }
        Ok(())
    }
}
fn book_item(book: botui_bookorbit::Book) -> Item<u64, Command> {
    let text = book
        .title
        .iter()
        .chain(&book.subtitle)
        .chain(&book.authors)
        .chain(&book.series_name)
        .chain(book.series_memberships.iter().map(|s| &s.series_name))
        .chain(&book.narrators)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    let book = Book::from(book);
    let name = single_line(book.title.clone().unwrap_or_else(|| "Untitled".into()));
    let label = single_line(
        [
            name.clone(),
            book.authors.join(", "),
            book.series.clone().unwrap_or_default(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  ·  "),
    );
    Item {
        key: book.id,
        name,
        label,
        text,
        action: Command::Files(book),
    }
}

pub(super) struct Panel {
    results: Search<Catalog>,
    opened: Instant,
    active: bool,
}
impl Shelf {
    pub(super) fn start_search(&mut self, session: Arc<Session>) {
        let pane = self.current_mut();
        if let Some(panel) = &mut pane.live_search
            && !panel.results.failed()
            && panel.opened.elapsed() < Duration::from_secs(60)
        {
            panel.results.reset_query();
            panel.active = true;
            return;
        }
        let directory = match pane.section {
            super::Section::Groups { directory, .. } => Some(directory),
            _ => None,
        };
        let title = match directory {
            None => "Search all books · BookOrbit",
            Some(Directory::Authors) => "Search authors · BookOrbit",
            Some(Directory::Series) => "Search series · BookOrbit",
        };
        let source = Catalog {
            session,
            directory,
            worker: Worker::new(),
            next: Some(0),
            pending: None,
        };
        pane.live_search = Some(Box::new(Panel {
            results: Search::new(source, title),
            opened: Instant::now(),
            active: true,
        }));
    }

    pub(super) fn handle_search(&mut self, term: &mut crate::AppTerminal) -> io::Result<bool> {
        if self.pending.is_some() || self.login.is_some() {
            return Ok(false);
        }
        if !self
            .current()
            .live_search
            .as_ref()
            .is_some_and(|p| p.active)
        {
            return Ok(false);
        }
        if let Some(notice) = self.notice.take() {
            crate::prompt::message(term, "BookOrbit", &notice)?;
        }
        let mut panel = self.current_mut().live_search.take().unwrap();
        let action = panel.results.choose(term, |frame| self.draw(frame))?;
        panel.active = action.is_some();
        self.current_mut().live_search = Some(panel);
        if let Some(cmd) = action {
            self.apply(cmd);
        }
        Ok(true)
    }
}
