mod common;

use common::Log;
use not_core::{NoteSort, NoteSummary, Store};

fn titles(notes: &[NoteSummary]) -> Vec<&str> {
    notes.iter().map(|n| n.title.as_str()).collect()
}

#[test]
fn notes_derive_title_and_sort_pinned_first() {
    let store = Store::open_in_memory().unwrap();
    let work = store.create_folder(None, "Work").unwrap();
    let a = store
        .create_note(Some(&work), "# Standup\n- [ ] ship notes")
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    store.create_note(Some(&work), "Ideas\nmore").unwrap();
    store.create_note(None, "Loose").unwrap();

    assert_eq!(titles(&store.notes(Some(&work)).unwrap()), ["Ideas", "Standup"]);
    store.set_note_pinned(&a, true).unwrap();
    let notes = store.notes(Some(&work)).unwrap();
    assert_eq!(titles(&notes), ["Standup", "Ideas"]);
    assert_eq!(notes[0].preview, "☐ ship notes");
    assert_eq!(store.notes(None).unwrap().len(), 3);
    assert_eq!(store.folders().unwrap()[0].note_count, 2);
    assert_eq!(store.note_body(&a).unwrap(), "# Standup\n- [ ] ship notes");
}

#[test]
fn folders_nest_and_delete_with_their_notes() {
    let store = Store::open_in_memory().unwrap();
    let home = store.create_folder(None, "Home").unwrap();
    let recipes = store.create_folder(Some(&home), "Recipes").unwrap();
    store.create_folder(None, "Archive").unwrap();
    let soup = store.create_note(Some(&recipes), "Soup").unwrap();

    let folders: Vec<_> = store
        .folders()
        .unwrap()
        .into_iter()
        .map(|f| (f.title, f.depth))
        .collect();
    assert_eq!(
        folders,
        [("Archive".into(), 0), ("Home".into(), 0), ("Recipes".into(), 1)]
    );
    assert!(store.move_folder(&home, Some(&recipes)).is_err(), "no cycles");

    store.delete_note_or_folder(&home).unwrap();
    assert_eq!(store.folders().unwrap().len(), 1);
    assert!(store.notes(None).unwrap().is_empty());
    assert!(store.set_note_body(&soup, "x").is_err());

    store.undo().unwrap();
    assert_eq!(titles(&store.notes(None).unwrap()), ["Soup"]);
    store.move_note(&soup, None).unwrap();
    assert_eq!(store.notes(None).unwrap()[0].folder_id, None);
}

#[test]
fn concurrent_body_edits_merge() {
    let mut log = Log::default();
    let (mac, phone) = (Store::open_in_memory().unwrap(), Store::open_in_memory().unwrap());
    let id = mac.create_note(None, "Groceries\nmilk").unwrap();
    log.sync(&mac);
    log.sync(&phone);

    mac.set_note_body(&id, "Groceries\nmilk\neggs").unwrap();
    phone.set_note_body(&id, "Shopping\nmilk").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    let body = mac.note_body(&id).unwrap();
    assert_eq!(body, phone.note_body(&id).unwrap());
    assert_eq!(body, "Shopping\nmilk\neggs");
}

#[test]
fn search_matches_all_words() {
    let store = Store::open_in_memory().unwrap();
    store.create_note(None, "Trip to Lisbon\nbook hotel").unwrap();
    store.create_note(None, "Hotel loyalty numbers").unwrap();
    assert_eq!(
        titles(&store.search_notes("hotel LISBON").unwrap()),
        ["Trip to Lisbon"]
    );
    assert_eq!(store.search_notes("hotel").unwrap().len(), 2);
    assert_eq!(store.search_notes("100%").unwrap().len(), 0);
}

#[test]
fn dragging_switches_a_folder_to_manual_order() {
    let store = Store::open_in_memory().unwrap();
    let work = store.create_folder(None, "Work").unwrap();
    let mut ids = Vec::new();
    for title in ["a", "b", "c"] {
        ids.push(store.create_note(Some(&work), title).unwrap());
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(titles(&store.notes(Some(&work)).unwrap()), ["c", "b", "a"]);
    assert!(!store.folders().unwrap()[0].manual_order);

    // "a" goes before "b": the shown order (c, b, a) is kept around it.
    store.move_note_before(&ids[0], Some(&ids[1])).unwrap();
    assert!(store.folders().unwrap()[0].manual_order);
    assert_eq!(titles(&store.notes(Some(&work)).unwrap()), ["c", "a", "b"]);

    // Editing no longer reshuffles; new notes go on top.
    store.set_note_body(&ids[1], "b edited").unwrap();
    store.create_note(Some(&work), "d").unwrap();
    assert_eq!(
        titles(&store.notes(Some(&work)).unwrap()),
        ["d", "c", "a", "b edited"]
    );
    store.move_note_before(&ids[2], None).unwrap();
    assert_eq!(
        titles(&store.notes(Some(&work)).unwrap()),
        ["d", "a", "b edited", "c"]
    );

    // Undo goes back step by step; switching back sorts by edits again.
    store.set_folder_manual_order(&work, false).unwrap();
    assert_eq!(titles(&store.notes(Some(&work)).unwrap())[0], "d");
    assert!(!store.folders().unwrap()[0].manual_order);
    assert!(store.move_note_before(&ids[0], Some(&work)).is_err());
}

#[test]
fn notes_from_before_fractional_indexes_can_be_reordered() {
    // An older replica made its notes without positions.
    let old = loro::LoroDoc::new();
    let tree = old.get_tree("notes");
    let folder = tree.create(loro::TreeParentId::Root).unwrap();
    tree.get_meta(folder).unwrap().insert("kind", "folder").unwrap();
    tree.get_meta(folder).unwrap().insert("title", "Old").unwrap();
    let mut ids = Vec::new();
    for (i, title) in ["x", "y", "z"].into_iter().enumerate() {
        let n = tree.create(loro::TreeParentId::Node(folder)).unwrap();
        let meta = tree.get_meta(n).unwrap();
        meta.insert("kind", "note").unwrap();
        meta.insert("updated_at", i as i64).unwrap();
        meta.ensure_mergeable_text("body")
            .unwrap()
            .insert(0, title)
            .unwrap();
        ids.push(n.to_string());
    }
    old.commit();

    let store = Store::open_in_memory().unwrap();
    store
        .apply_pull(&not_protocol::PullResponse {
            updates: vec![not_protocol::LoggedUpdate {
                seq: 1,
                doc: "workspace".into(),
                data: old.export(loro::ExportMode::Snapshot).unwrap(),
            }],
            seq: 1,
            more: false,
        })
        .unwrap();
    let folder = folder.to_string();
    assert_eq!(titles(&store.notes(Some(&folder)).unwrap()), ["z", "y", "x"]);
    store.move_note_before(&ids[0], Some(&ids[2])).unwrap();
    assert_eq!(titles(&store.notes(Some(&folder)).unwrap()), ["x", "z", "y"]);
}

#[test]
fn manual_order_syncs() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let f = mac.create_folder(None, "F").unwrap();
    let a = mac.create_note(Some(&f), "a").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    mac.create_note(Some(&f), "b").unwrap();
    mac.move_note_before(&a, None).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(titles(&phone.notes(Some(&f)).unwrap()), ["b", "a"]);
    assert!(phone.folders().unwrap()[0].manual_order);
}

#[test]
fn folders_and_all_notes_sort_by_edit_creation_or_title() {
    let store = Store::open_in_memory().unwrap();
    let work = store.create_folder(None, "Work").unwrap();
    let pause = || std::thread::sleep(std::time::Duration::from_millis(3));
    let banana = store.create_note(Some(&work), "banana").unwrap();
    pause();
    store.create_note(Some(&work), "Apple").unwrap();
    pause();
    store.create_note(Some(&work), "cherry").unwrap();
    pause();
    store.set_note_body(&banana, "banana\nedited last").unwrap();

    assert_eq!(store.folders().unwrap()[0].sort, NoteSort::Edited);
    assert_eq!(
        titles(&store.notes(Some(&work)).unwrap()),
        ["banana", "cherry", "Apple"]
    );
    store.set_note_sort(Some(&work), NoteSort::Created).unwrap();
    assert_eq!(
        titles(&store.notes(Some(&work)).unwrap()),
        ["cherry", "Apple", "banana"]
    );
    store.set_note_sort(Some(&work), NoteSort::Title).unwrap();
    assert_eq!(store.folders().unwrap()[0].sort, NoteSort::Title);
    assert_eq!(
        titles(&store.notes(Some(&work)).unwrap()),
        ["Apple", "banana", "cherry"]
    );

    // By hand starts from what was shown.
    store.set_note_sort(Some(&work), NoteSort::Manual).unwrap();
    assert!(store.folders().unwrap()[0].manual_order);
    assert_eq!(
        titles(&store.notes(Some(&work)).unwrap()),
        ["Apple", "banana", "cherry"]
    );

    store.set_note_sort(None, NoteSort::Title).unwrap();
    assert_eq!(store.all_notes_sort(), NoteSort::Title);
    assert_eq!(titles(&store.notes(None).unwrap()), ["Apple", "banana", "cherry"]);
    assert!(store.set_note_sort(None, NoteSort::Manual).is_err());
}
