use super::*;
use crossterm::event::KeyEvent;

fn press(shelf: &mut Shelf, code: KeyCode) -> Option<Selection> {
    shelf.input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn page(number: u32, titles: &[&str]) -> BooksPage {
    BooksPage {
        page: number,
        size: 2,
        total: 6,
        items: titles
            .iter()
            .enumerate()
            .map(|(i, t)| botui_bookorbit::Book {
                id: u64::from(number) * 2 + i as u64,
                title: Some(t.to_string()),
                authors: vec![],
                subtitle: None,
                narrators: vec![],
                series_memberships: vec![],
                series_name: None,
                series_index: None,
                has_cover: None,
                progress: None,
                files: vec![botui_bookorbit::BookFile {
                    id: 300 + i as u64,
                    format: Some("pdf".into()),
                    role: "content".into(),
                    size_bytes: None,
                }],
            })
            .collect(),
    }
}

#[test]
fn q_returns_from_book_card_to_selected_shelf_before_home() {
    let mut shelf = Shelf::new(String::new(), String::new());
    shelf.complete(Loaded::Books(Scope::All, page(1, &["Book C", "Book D"])));
    shelf.current_mut().selection = ListState::default().with_selected(Some(1)).with_offset(1);
    press(&mut shelf, KeyCode::Enter);
    assert!(matches!(shelf.current().section, Section::Files(_)));
    assert!(press(&mut shelf, KeyCode::Char('q')).is_none());
    assert!(matches!(
        shelf.current().section,
        Section::Books {
            scope: Scope::All,
            ..
        }
    ));
    assert_eq!(shelf.current().selection.selected(), Some(1));
    assert_eq!(shelf.current().selection.offset(), 1);
    assert!(
        matches!(shelf.current().action(), Some(Command::Files(b)) if b.title.as_deref() == Some("Book D"))
    );
    assert!(press(&mut shelf, KeyCode::Char('q')).is_none());
    assert!(matches!(shelf.current().section, Section::Home));
    assert!(matches!(
        press(&mut shelf, KeyCode::Char('q')),
        Some(Selection::Quit)
    ));
}

#[test]
fn back_during_save_waits_and_keeps_accepted_progress_on_the_parent_shelf() {
    for key in [KeyCode::Char('q'), KeyCode::Esc] {
        let mut shelf = Shelf::new(String::new(), String::new());
        shelf.complete(Loaded::Books(Scope::All, page(0, &["Book A", "Book B"])));
        press(&mut shelf, KeyCode::Down);
        press(&mut shelf, KeyCode::Enter);
        let (send, receive) = std::sync::mpsc::channel();
        shelf.fetch("Saving position…", move |_| {
            receive.recv().unwrap();
            Ok(Loaded::Moved {
                saved: true,
                file: 301,
                percent: 42,
            })
        });
        shelf.pending.as_mut().unwrap().saving = true;
        press(&mut shelf, key);
        press(&mut shelf, key);
        assert!(shelf.pending.as_ref().is_some_and(|p| p.saving));
        assert!(matches!(shelf.current().section, Section::Files(_)));
        send.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while shelf.pending.is_some() {
            assert!(Instant::now() < deadline, "save did not complete");
            assert!(shelf.collect_results().is_none());
            std::thread::yield_now();
        }
        assert!(matches!(
            shelf.current().section,
            Section::Books {
                scope: Scope::All,
                ..
            }
        ));
        assert!(
            matches!(shelf.current().action(), Some(Command::Files(b)) if b.title.as_deref() == Some("Book B") && b.progress == Some(42))
        );
    }
}

#[test]
fn q_cancels_loading_without_skipping_the_visible_shelf() {
    let mut shelf = Shelf::new(String::new(), String::new());
    shelf.complete(Loaded::Books(Scope::All, page(0, &["Book A", "Book B"])));
    let (send, receive) = std::sync::mpsc::channel::<()>();
    shelf.fetch("Loading…", move |_| {
        let _ = receive.recv();
        Ok(Loaded::Collections(vec![]))
    });
    press(&mut shelf, KeyCode::Char('q'));
    drop(send);
    assert!(shelf.pending.is_none());
    assert!(matches!(
        shelf.current().section,
        Section::Books {
            scope: Scope::All,
            ..
        }
    ));
    press(&mut shelf, KeyCode::Char('q'));
    assert!(matches!(shelf.current().section, Section::Home));
}

#[test]
fn refreshing_a_shelf_keeps_the_selected_book_when_its_order_changes() {
    let mut previous = Pane::new(
        Section::Collections,
        "Before",
        book_rows(page(0, &["A", "B"]).items),
    );
    previous.selection = ListState::default().with_selected(Some(1)).with_offset(1);
    let mut items = page(0, &["A", "B"]).items;
    items.reverse();
    let mut current = Pane::new(Section::Collections, "After", book_rows(items));
    current.restore_position(previous);
    assert_eq!(current.selection.selected(), Some(0));
    assert!(matches!(current.action(), Some(Command::Files(b)) if b.title.as_deref() == Some("B")));
}

#[test]
fn library_pagination_and_empty_collections_return_to_selected_book_in_catalog() {
    let mut shelf = Shelf::new(String::new(), String::new());
    let library = Library {
        id: 47,
        name: "Переводы".into(),
        book_count: Some(6),
    };
    shelf.complete(Loaded::Books(Scope::All, page(1, &["Книга C", "Книга D"])));
    shelf.current_mut().move_by(1);
    shelf.complete(Loaded::Libraries(vec![library.clone()]));
    assert!(
        matches!(shelf.current().action(), Some(Command::Books(Scope::Library(l),0)) if l.id==47)
    );
    shelf.complete(Loaded::Books(
        Scope::Library(library.clone()),
        page(0, &["Первая", "Вторая"]),
    ));
    assert!(shelf.current().title.contains("6"));
    assert_eq!(shelf.current().rows.len(), 2);
    assert!(matches!(shelf.current().action(), Some(Command::Files(_))));
    shelf.back();
    assert!(matches!(shelf.current().section, Section::Libraries));
    shelf.complete(Loaded::Collections(vec![]));
    assert!(shelf.current().rows.is_empty());
    shelf.back();
    assert!(matches!(
        shelf.current().section,
        Section::Books {
            scope: Scope::All,
            page: 1,
            ..
        }
    ));
    assert!(
        matches!(shelf.current().action(), Some(Command::Files(b)) if b.title.as_deref()==Some("Книга D"))
    );
}
