use super::*;

#[test]
fn back_visits_parents_and_preserves_selection_and_scroll_before_leaving_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for name in ["A", "B", "C", "C/Child"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    let mut picker = Picker::new(root.into()).unwrap();
    picker.list = ListState::default().with_selected(Some(2)).with_offset(1);
    picker.load(&root.join("C")).unwrap();
    picker.load(&root.join("C/Child")).unwrap();
    assert!(!picker.back().unwrap());
    assert_eq!(picker.dir, root.join("C"));
    assert_eq!(
        picker.entries[picker.list.selected().unwrap()].path,
        root.join("C/Child")
    );
    assert!(!picker.back().unwrap());
    assert_eq!(picker.dir, root);
    assert_eq!(picker.list.selected(), Some(2));
    assert_eq!(picker.list.offset(), 1);
    assert!(picker.back().unwrap());
    assert_eq!(picker.dir, root);
}

#[test]
fn returning_from_a_local_shelf_restores_the_folder_even_if_its_order_changed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("B")).unwrap();
    let mut picker = Picker::new(root.into()).unwrap();
    picker.browse(LocalView::Recent).unwrap();
    fs::create_dir(root.join("A")).unwrap();
    assert!(!picker.back().unwrap());
    assert_eq!(
        picker.entries[picker.list.selected().unwrap()].path,
        root.join("B")
    );
    assert!(matches!(picker.view, LocalView::Directory));
}
