use super::{
    engine::{self, Remote},
    mapping::Mapper,
};
use botui_bookorbit::{
    Result,
    annotations::{Annotation, New},
};
use botui_core::{Anchor, Block, Document, Run, TextRange};
use botui_store::{HighlightColor, LocalState};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

#[derive(Default)]
struct Server {
    items: RefCell<Vec<Annotation>>,
    next: Cell<u64>,
    fail_get: Cell<bool>,
    lose_post: Cell<bool>,
    creates: Cell<usize>,
}
impl Remote for Server {
    fn list(&self) -> Result<Vec<Annotation>> {
        if self.fail_get.get() {
            Err("offline".into())
        } else {
            Ok(self.items.borrow().clone())
        }
    }
    fn create(&self, new: &New) -> Result<Annotation> {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.creates.set(self.creates.get() + 1);
        let r = Annotation {
            id,
            cfi: new.cfi.clone(),
            pdf: new.pdf.clone(),
            jump_file_id: Some(new.book_file_id),
            text: new.text.clone(),
            color: new.color.clone(),
            style: new.style.clone(),
            note: new.note.clone(),
            updated_at: None,
        };
        self.items.borrow_mut().push(r.clone());
        if self.lose_post.replace(false) {
            Err("response lost after commit".into())
        } else {
            Ok(r)
        }
    }
    fn edit(&self, id: u64, note: &str, color: &str) -> Result<Annotation> {
        let mut items = self.items.borrow_mut();
        let a = items.iter_mut().find(|a| a.id == id).unwrap();
        a.note = Some(note.into());
        a.color = color.into();
        Ok(a.clone())
    }
    fn delete(&self, id: u64) -> Result<()> {
        self.items.borrow_mut().retain(|r| r.id != id);
        Ok(())
    }
}
fn document() -> Document {
    let mut doc = Document {
        blocks: vec![Block {
            text: "a😀bc def".into(),
            heading: false,
        }],
        ..Default::default()
    };
    doc.locations.0.insert(
        0,
        vec![Run {
            byte: 0,
            end: 11,
            utf16: 0,
            path: "/6/2!/4/2/1".into(),
        }],
    );
    doc
}
fn fixture() -> (tempfile::TempDir, LocalState, Mapper, Server) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("book.epub");
    std::fs::write(&file, b"fixture").unwrap();
    let store = LocalState::in_directory(&file, &dir.path().join("state"), None).unwrap();
    (
        dir,
        store,
        Mapper::Text(Arc::new(document())),
        Server::default(),
    )
}
fn add(store: &LocalState, id: Option<&str>, body: &str) {
    store
        .annotate(
            id,
            TextRange::new(Anchor { block: 0, byte: 1 }, Anchor { block: 0, byte: 7 }),
            "😀bc".into(),
            Some(HighlightColor::Blue),
            body.into(),
        )
        .unwrap();
}
fn sync(store: &LocalState, mapper: &mut Mapper, server: &Server) -> engine::Report {
    engine::sync(store, "test", 9, mapper, server, false).unwrap()
}
#[test]
fn annotations_create_edit_color_delete_and_remote_import_in_both_directions() {
    let (_dir, store, mut mapper, server) = fixture();
    add(&store, None, "local");
    sync(&store, &mut mapper, &server);
    assert_eq!(server.items.borrow().len(), 1);
    assert_eq!(server.items.borrow()[0].note.as_deref(), Some("local"));
    let id = store.notes().unwrap()[0].id.clone();
    add(&store, Some(&id), "edited");
    sync(&store, &mut mapper, &server);
    assert_eq!(server.items.borrow()[0].note.as_deref(), Some("edited"));
    server.items.borrow_mut()[0].note = Some("browser".into());
    server.items.borrow_mut()[0].color = "#F472B6".into();
    sync(&store, &mut mapper, &server);
    let note = store.notes().unwrap().remove(0);
    assert_eq!(note.body, "browser");
    assert_eq!(note.color, Some(HighlightColor::Pink));
    // Browser style/color are not normalized by a comment edit.
    server.items.borrow_mut()[0].style = "underline".into();
    server.items.borrow_mut()[0].color = "#FF3300".into();
    sync(&store, &mut mapper, &server);
    let n = store.notes().unwrap().remove(0);
    store
        .annotate(
            Some(&n.id),
            n.range().unwrap(),
            n.quote,
            n.color,
            "keep exact style".into(),
        )
        .unwrap();
    sync(&store, &mut mapper, &server);
    assert_eq!(server.items.borrow()[0].style, "underline");
    assert_eq!(server.items.borrow()[0].color, "#FF3300");
    store.delete_note(&id).unwrap();
    sync(&store, &mut mapper, &server);
    assert!(server.items.borrow().is_empty());
    add(&store, None, "second");
    sync(&store, &mut mapper, &server);
    server.items.borrow_mut().clear();
    sync(&store, &mut mapper, &server);
    assert!(store.notes().unwrap().is_empty());
    let new = New {
        book_file_id: 9,
        cfi: document().range_cfi(TextRange::new(
            Anchor { block: 0, byte: 1 },
            Anchor { block: 0, byte: 7 },
        )),
        pdf: None,
        text: "😀bc".into(),
        color: "#4ADE80".into(),
        style: "highlight".into(),
        note: Some("remote new".into()),
    };
    server.create(&new).unwrap();
    sync(&store, &mut mapper, &server);
    assert_eq!(store.notes().unwrap()[0].body, "remote new");
    assert_eq!(server.items.borrow().len(), 1);
}
#[test]
fn offline_conflicts_and_ambiguous_posts_keep_data_without_duplicates() {
    let (_dir, store, mut mapper, server) = fixture();
    add(&store, None, "local");
    server.fail_get.set(true);
    assert!(engine::sync(&store, "test", 9, &mut mapper, &server, false).is_err());
    assert_eq!(store.notes().unwrap().len(), 1);
    server.fail_get.set(false);
    server.lose_post.set(true);
    assert!(engine::sync(&store, "test", 9, &mut mapper, &server, false).is_err());
    sync(&store, &mut mapper, &server);
    assert_eq!(server.creates.get(), 1);
    let id = store.notes().unwrap()[0].id.clone();
    add(&store, Some(&id), "device edit");
    server.items.borrow_mut()[0].note = Some("web edit".into());
    assert_eq!(sync(&store, &mut mapper, &server).conflicts, 1);
    let notes = store.notes().unwrap();
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().any(|n| n.body == "device edit"));
    assert!(notes.iter().any(|n| n.body == "web edit"));
    assert_eq!(server.items.borrow().len(), 2);
    sync(&store, &mut mapper, &server);
    assert_eq!(server.items.borrow().len(), 2);
    // A deletion never discards an independently changed remote comment.
    store.delete_note(&id).unwrap();
    server
        .items
        .borrow_mut()
        .iter_mut()
        .find(|a| a.note.as_deref() == Some("web edit"))
        .unwrap()
        .note = Some("web again".into());
    assert_eq!(sync(&store, &mut mapper, &server).conflicts, 1);
    assert!(store.notes().unwrap().iter().any(|n| n.body == "web again"));
}
#[test]
fn unknown_edition_or_unresolved_range_stays_remote_and_failed_get_never_deletes() {
    let (_dir, store, mut mapper, server) = fixture();
    add(&store, None, "saved");
    sync(&store, &mut mapper, &server);
    let before = store.notes().unwrap();
    server.fail_get.set(true);
    assert!(engine::sync(&store, "test", 9, &mut mapper, &server, false).is_err());
    assert_eq!(store.notes().unwrap(), before);
    server.fail_get.set(false);
    server.items.borrow_mut()[0].jump_file_id = Some(999);
    assert!(sync(&store, &mut mapper, &server).unlocated > 0);
    assert_eq!(store.notes().unwrap(), before);
}

#[test]
fn edits_made_while_a_post_is_in_flight_are_not_acknowledged_as_sent() {
    struct During<'a> {
        server: &'a Server,
        store: &'a LocalState,
    }
    impl Remote for During<'_> {
        fn list(&self) -> Result<Vec<Annotation>> {
            self.server.list()
        }
        fn create(&self, new: &New) -> Result<Annotation> {
            let id = self.store.notes()?.remove(0).id;
            add(self.store, Some(&id), "typed during upload");
            self.server.create(new)
        }
        fn edit(&self, id: u64, note: &str, color: &str) -> Result<Annotation> {
            self.server.edit(id, note, color)
        }
        fn delete(&self, id: u64) -> Result<()> {
            self.server.delete(id)
        }
    }
    let (_dir, store, mut mapper, server) = fixture();
    add(&store, None, "sent snapshot");
    engine::sync(
        &store,
        "test",
        9,
        &mut mapper,
        &During {
            server: &server,
            store: &store,
        },
        false,
    )
    .unwrap();
    assert_eq!(store.notes().unwrap()[0].body, "typed during upload");
    assert_eq!(
        server.items.borrow()[0].note.as_deref(),
        Some("sent snapshot")
    );
    sync(&store, &mut mapper, &server);
    assert_eq!(
        server.items.borrow()[0].note.as_deref(),
        Some("typed during upload")
    );
    assert_eq!(server.creates.get(), 1);
}
#[test]
fn pdf_geometry_round_trips_points_and_normalized_boxes() {
    use botui_fixed::text::{PageText, Word};
    use botui_store::PdfRegion;
    let (_dir, store, _, server) = fixture();
    let page = PageText {
        size: [200., 400.],
        text: "Hello world".into(),
        words: vec![
            Word {
                bytes: 0..5,
                bounds: [0.1, 0.2, 0.3, 0.25],
                line: 0,
                block: 0,
            },
            Word {
                bytes: 6..11,
                bounds: [0.4, 0.2, 0.6, 0.25],
                line: 0,
                block: 0,
            },
        ],
    };
    let mut mapper = Mapper::Pdf {
        path: "unused".into(),
        pages: [(2, page)].into(),
    };
    let range = TextRange::new(Anchor { block: 1, byte: 0 }, Anchor { block: 1, byte: 11 });
    store
        .annotate_regions(
            None,
            range,
            "Hello world".into(),
            Some(HighlightColor::Yellow),
            "PDF note".into(),
            vec![PdfRegion {
                page: 2,
                bounds: [0.1, 0.2, 0.6, 0.25],
            }],
        )
        .unwrap();
    sync(&store, &mut mapper, &server);
    let pdf = server.items.borrow()[0].pdf.clone().unwrap();
    assert_eq!(pdf.page, 1);
    assert_eq!(pdf.rect.x, 20.);
    assert_eq!(pdf.rect.y, 80.);
    assert_eq!(pdf.rect.height, 20.);
    server.items.borrow_mut()[0].note = Some("PDF web comment".into());
    sync(&store, &mut mapper, &server);
    let note = &store.notes().unwrap()[0];
    assert_eq!(note.body, "PDF web comment");
    assert_eq!(note.range(), Some(range));
    assert_eq!(note.pdf_regions[0].bounds, [0.1, 0.2, 0.6, 0.25]);
}
