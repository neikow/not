//! "Make tasks" on a note's checklist (ADR 0013): the items are copied into tasks once; the note
//! keeps its lines as plain checkboxes, and the two don't follow each other afterwards. Notes from
//! earlier versions (task blocks, per-line markers) open as plain checklists.

mod common;

use common::Log;
use not_core::date::Date;
use not_core::markdown::{TaskSource, task_block_markup};
use not_core::{INBOX, Store};

fn today() -> Date {
    Date::parse("2026-11-12").unwrap()
}

fn utf16(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn body(store: &Store, note: &str) -> String {
    store.note_body(note).unwrap()
}

/// A note with `text` whose whole checklist went into tasks tagged after its heading. Returns
/// (note, `#tag` to view them by).
fn make(store: &Store, text: &str) -> (String, String) {
    let note = store.create_note(None, text).unwrap();
    let moved = store
        .checklist_to_tasks(&note, text, 0, utf16(text), Some(""))
        .unwrap();
    (note, format!("#{}", moved.tag.unwrap()))
}

/// A view as (depth, title, done), open then completed.
fn list(store: &Store, id: &str) -> Vec<(u32, String, bool)> {
    let o = store.view_outline(id, today()).unwrap();
    [o.open, o.completed]
        .concat()
        .into_iter()
        .map(|r| (r.depth, r.task.title, r.task.completed_at.is_some()))
        .collect()
}

fn rows(rows: &[(u32, &str, bool)]) -> Vec<(u32, String, bool)> {
    rows.iter().map(|&(d, t, done)| (d, t.to_owned(), done)).collect()
}

fn task_id(store: &Store, list_id: &str, title: &str) -> String {
    let o = store.view_outline(list_id, today()).unwrap();
    [o.open, o.completed]
        .concat()
        .into_iter()
        .find(|r| r.task.title == title)
        .unwrap_or_else(|| panic!("no task {title:?} in {list_id}"))
        .task
        .id
}

const SHOPPING: &str = "Shopping\n## Groceries\n- [ ] milk\n  - [x] oat\n- [ ] eggs\nafter";

// ---- making tasks ------------------------------------------------------------------------------

#[test]
fn making_tasks_copies_the_items_and_leaves_the_note_as_it_was() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    assert_eq!(body(&store, &note), SHOPPING);
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "milk", false), (1, "oat", true), (0, "eggs", false)])
    );
    assert_eq!(list_id, "#groceries");
    assert!(store.tags().unwrap().iter().any(|t| t.name == "groceries"));
}

#[test]
fn a_typed_tag_wins_over_the_heading() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, SHOPPING).unwrap();
    let moved = store
        .checklist_to_tasks(&note, SHOPPING, 0, utf16(SHOPPING), Some("  Weekly shop "))
        .unwrap();
    assert_eq!(moved.tag.as_deref(), Some("weekly-shop"));
    assert_eq!(moved.count, 3);
}

#[test]
fn tasks_go_first_under_an_existing_tag_in_note_order() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_view("Errands", "#errands").unwrap();
    store.add_task(&errands, "old", today()).unwrap();
    let text = "Note\n- [ ] one\n- [ ] two";
    let note = store.create_note(None, text).unwrap();
    let moved = store
        .checklist_to_tasks(&note, text, 0, utf16(text), Some("errands"))
        .unwrap();
    assert_eq!(moved.tag.as_deref(), Some("errands"));
    assert_eq!(
        list(&store, &errands),
        rows(&[(0, "one", false), (0, "two", false), (0, "old", false)])
    );
}

#[test]
fn the_inbox_works_as_a_target() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] call mum";
    let note = store.create_note(None, text).unwrap();
    store
        .checklist_to_tasks(&note, text, 0, utf16(text), None)
        .unwrap();
    assert_eq!(list(&store, INBOX), rows(&[(0, "call mum", false)]));
}

#[test]
fn only_the_touched_items_are_made() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] a\n- [ ] b\n- [ ] c";
    let note = store.create_note(None, text).unwrap();
    let at = text.find('b').unwrap() as u32;
    let moved = store.checklist_to_tasks(&note, text, at, at, None).unwrap();
    assert_eq!(moved.count, 1);
    assert_eq!(list(&store, INBOX), rows(&[(0, "b", false)]));
}

#[test]
fn a_nested_item_alone_goes_to_the_top_level() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] a\n  - [ ] a1";
    let note = store.create_note(None, text).unwrap();
    let at = text.find("a1").unwrap() as u32;
    store.checklist_to_tasks(&note, text, at, at, None).unwrap();
    assert_eq!(list(&store, INBOX), rows(&[(0, "a1", false)]));
}

#[test]
fn deep_nesting_becomes_subtasks_of_subtasks() {
    let store = Store::open_in_memory().unwrap();
    let (_, list_id) = make(
        &store,
        "Trip\n- [ ] pack\n  - [ ] bag\n    - [x] socks\n  - [ ] charger\n- [ ] go",
    );
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "pack", false),
            (1, "bag", false),
            (2, "socks", true),
            (1, "charger", false),
            (0, "go", false),
        ])
    );
}

#[test]
fn what_the_editor_shows_is_saved_first() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Note\n- [ ] a").unwrap();
    let shown = "Note\nintro\n- [ ] a\n- [ ] b";
    store
        .checklist_to_tasks(&note, shown, 0, utf16(shown), None)
        .unwrap();
    assert_eq!(body(&store, &note), shown);
    assert_eq!(list(&store, INBOX).len(), 2);
}

#[test]
fn nothing_to_make_is_refused_without_changes() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\njust words";
    let note = store.create_note(None, text).unwrap();
    let shown = "Note\njust words, edited";
    assert!(
        store
            .checklist_to_tasks(&note, shown, 0, utf16(shown), Some(""))
            .is_err()
    );
    assert!(store.tags().unwrap().is_empty());
    assert!(store.all_tasks().unwrap().is_empty());
    assert_eq!(body(&store, &note), text, "nothing saved either");
}

#[test]
fn one_undo_takes_the_tasks_away_and_keeps_the_note() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    assert!(store.undo().unwrap());
    assert!(list(&store, &list_id).is_empty());
    assert_eq!(body(&store, &note), SHOPPING);
}

#[test]
fn making_tasks_again_makes_them_again() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] a";
    let note = store.create_note(None, text).unwrap();
    for _ in 0..2 {
        store
            .checklist_to_tasks(&note, text, 0, utf16(text), None)
            .unwrap();
    }
    assert_eq!(list(&store, INBOX).len(), 2, "a copy each time");
}

#[test]
fn offsets_are_utf16_and_titles_can_be_unicode() {
    let store = Store::open_in_memory().unwrap();
    let text = "😀 Trip\n- [ ] 🧳 bag\n- [ ] café";
    let note = store.create_note(None, text).unwrap();
    let at = utf16(text);
    store.checklist_to_tasks(&note, text, at, at, None).unwrap();
    assert_eq!(list(&store, INBOX), rows(&[(0, "café", false)]));
}

// ---- the note and the tasks go their own ways ----------------------------------------------------

#[test]
fn ticking_or_editing_the_note_leaves_the_tasks_alone() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let before = list(&store, &list_id);
    let edited = SHOPPING
        .replace("- [ ] milk", "- [x] whole milk")
        .replace("- [ ] eggs", "  - [ ] eggs\n- [ ] bread");
    store.set_note_body(&note, &edited).unwrap();
    assert_eq!(list(&store, &list_id), before);
}

#[test]
fn task_changes_leave_the_note_alone() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let eggs = task_id(&store, &list_id, "eggs");
    store.set_completed(&eggs, true, today()).unwrap();
    store
        .rename(&task_id(&store, &list_id, "milk"), "oat milk")
        .unwrap();
    store.add_task(&eggs, "large", today()).unwrap();
    store.add_task(&list_id, "bread", today()).unwrap();
    store.delete(&task_id(&store, &list_id, "oat")).unwrap();
    store.delete_tag(&list_id).unwrap();
    assert_eq!(body(&store, &note), SHOPPING);
}

#[test]
fn notes_and_tasks_sync_on_their_own() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(body(&phone, &note), SHOPPING);
    assert_eq!(list(&phone, &list_id).len(), 3);
    phone
        .set_note_body(&note, &SHOPPING.replace("- [ ] eggs", "- [x] eggs"))
        .unwrap();
    log.sync(&phone);
    log.sync(&mac);
    assert!(body(&mac, &note).contains("- [x] eggs"));
    assert!(
        mac.task_by_id(&task_id(&mac, &list_id, "eggs"))
            .unwrap()
            .completed_at
            .is_none()
    );
}

// ---- notes from earlier versions -------------------------------------------------------------

fn with_block(store: &Store, source: &TaskSource) -> String {
    let text = format!("Errands\nbefore\n{}\nafter", task_block_markup(source));
    store.create_note(None, &text).unwrap()
}

#[test]
fn a_list_block_opens_as_a_plain_checklist() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_view("Errands", "#errands").unwrap();
    let bank = store.add_task(&errands, "bank", today()).unwrap();
    let post = store.add_task(&errands, "post office", today()).unwrap();
    store.add_task(&post, "stamps", today()).unwrap();
    store.set_completed(&bank, true, today()).unwrap();
    let note = with_block(&store, &TaskSource::List(errands.clone()));
    let opened = store.open_note(&note).unwrap();
    assert_eq!(
        opened,
        "Errands\nbefore\n- [ ] post office\n  - [ ] stamps\n- [x] bank\nafter"
    );
    assert_eq!(body(&store, &note), opened);
    // Plain from now on.
    store.add_task(&errands, "pharmacy", today()).unwrap();
    store.set_completed(&post, true, today()).unwrap();
    assert_eq!(body(&store, &note), opened);
}

#[test]
fn a_block_of_picked_tasks_opens_as_their_lines_only() {
    let store = Store::open_in_memory().unwrap();
    store.add_task(INBOX, "other", today()).unwrap();
    let a = store.add_task(INBOX, "a", today()).unwrap();
    store.add_task(&a, "a.1", today()).unwrap();
    let note = with_block(&store, &TaskSource::Tasks(vec![a]));
    assert_eq!(
        store.open_note(&note).unwrap(),
        "Errands\nbefore\n- [ ] a\n  - [ ] a.1\nafter"
    );
}

#[test]
fn a_block_whose_tasks_are_gone_opens_without_its_line() {
    let store = Store::open_in_memory().unwrap();
    let note = with_block(&store, &TaskSource::List("99@99".into()));
    assert_eq!(store.open_note(&note).unwrap(), "Errands\nbefore\nafter");
}

#[test]
fn per_line_markers_are_taken_out_on_opening() {
    let store = Store::open_in_memory().unwrap();
    let tea = store.add_task(INBOX, "tea", today()).unwrap();
    let note = store
        .create_note(None, &format!("Note\n- [ ] tea <!--task:{tea}-->\n- [ ] plain"))
        .unwrap();
    assert_eq!(
        body(&store, &note),
        "Note\n- [ ] tea\n- [ ] plain",
        "read without markers"
    );
    assert_eq!(store.open_note(&note).unwrap(), "Note\n- [ ] tea\n- [ ] plain");
    store.set_completed(&tea, true, today()).unwrap();
    assert_eq!(body(&store, &note), "Note\n- [ ] tea\n- [ ] plain");
}

#[test]
fn opening_an_old_note_isnt_undone() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_view("Errands", "#errands").unwrap();
    let a = store.add_task(&errands, "a", today()).unwrap();
    let note = with_block(&store, &TaskSource::List(errands.clone()));
    store.rename(&a, "b").unwrap();
    let opened = store.open_note(&note).unwrap();
    // Undo takes the user's last step (the rename), not the update.
    assert!(store.undo().unwrap());
    assert_eq!(store.task_by_id(&a).unwrap().title, "a");
    assert_eq!(body(&store, &note), opened);
}

#[test]
fn opening_an_up_to_date_note_changes_nothing() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = make(&store, SHOPPING);
    let mut log = Log::default();
    log.push(&store);
    assert_eq!(store.open_note(&note).unwrap(), SHOPPING);
    assert!(store.outgoing().unwrap().is_none(), "no write");
}
