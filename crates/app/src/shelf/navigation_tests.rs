use super::*;

fn page(number: u32, titles: &[&str]) -> BooksPage {
    BooksPage {
        page: number,
        size: 2,
        total: 6,
        items: titles
            .iter()
            .enumerate()
            .map(|(i, t)| Book {
                id: u64::from(number) * 2 + i as u64,
                title: Some(t.to_string()),
                authors: vec![],
                has_cover: None,
                progress: None,
                files: vec![BookFile {
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
    shelf.complete(Loaded::Books(
        Scope::Library(library),
        page(1, &["Третья", "Четвёртая"]),
    ));
    assert!(shelf.current().title.contains("2/3"));
    assert!(shelf.current().rows[0].label.contains("[PDF]"));
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
