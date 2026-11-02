mod common;

use common::Log;
use not_core::{NoteSummary, Store};

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
    assert_eq!(notes[0].preview, "ship notes");
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
