//! Checklist lines in notes linked to the tasks made from them (ADR 0012): making them, edits in the
//! note reaching the tasks, task changes reaching the note, new tasks and subtasks coming in, sync
//! between devices, undo, and notes from earlier versions.

mod common;

use common::Log;
use not_core::date::Date;
use not_core::markdown::{TaskSource, task_block_markup};
use not_core::{Freq, INBOX, Repeat, Schedule, Store};

fn today() -> Date {
    Date::parse("2026-11-12").unwrap()
}

/// A note with `body` whose whole checklist went into a new list. Returns (note, list).
fn make(store: &Store, body: &str) -> (String, String) {
    let note = store.create_note(None, body).unwrap();
    let moved = store
        .checklist_to_tasks(&note, body, 0, utf16(body), None, "", today())
        .unwrap();
    (note, moved.list_id)
}

fn utf16(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn body(store: &Store, note: &str) -> String {
    store.note_body(note).unwrap()
}

/// Edits the note's body as an editor would: replaces `from` with `to` (first occurrence).
fn edit(store: &Store, note: &str, from: &str, to: &str) {
    let b = body(store, note);
    assert!(b.contains(from), "{from:?} not in {b:?}");
    store
        .set_note_body(note, &b.replacen(from, to, 1), today())
        .unwrap();
}

/// Linked lines as (line text, task title).
fn linked(store: &Store, note: &str) -> Vec<(String, String)> {
    let b = body(store, note);
    let lines: Vec<&str> = b.split('\n').collect();
    store
        .note_links(note)
        .unwrap()
        .into_iter()
        .map(|l| {
            let title = store.task_by_id(&l.task_id).unwrap().title;
            (lines[l.line as usize].to_owned(), title)
        })
        .collect()
}

fn linked_lines(store: &Store, note: &str) -> Vec<String> {
    linked(store, note).into_iter().map(|(l, _)| l).collect()
}

/// A list as (depth, title, done), open then completed.
fn list(store: &Store, id: &str) -> Vec<(u32, String, bool)> {
    let o = store.outline(id).unwrap();
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
    let o = store.outline(list_id).unwrap();
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
fn making_tasks_keeps_the_lines_and_links_each() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    assert_eq!(body(&store, &note), SHOPPING, "the text is left as it was");
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "milk", false), (1, "oat", true), (0, "eggs", false)])
    );
    assert_eq!(
        linked(&store, &note),
        vec![
            ("- [ ] milk".into(), "milk".into()),
            ("  - [x] oat".into(), "oat".into()),
            ("- [ ] eggs".into(), "eggs".into()),
        ]
    );
    let lists = store.lists().unwrap();
    assert!(lists.iter().any(|l| l.id == list_id && l.title == "Groceries"));
}

#[test]
fn a_typed_list_title_wins_over_the_heading() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, SHOPPING).unwrap();
    let moved = store
        .checklist_to_tasks(
            &note,
            SHOPPING,
            0,
            utf16(SHOPPING),
            None,
            "  Weekly shop ",
            today(),
        )
        .unwrap();
    assert!(
        store
            .lists()
            .unwrap()
            .iter()
            .any(|l| l.id == moved.list_id && l.title == "Weekly shop")
    );
    assert_eq!(moved.count, 3);
}

#[test]
fn tasks_go_first_in_an_existing_list_in_note_order() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_list("Errands").unwrap();
    store.add_task(&errands, "old").unwrap();
    let text = "Note\n- [ ] one\n- [ ] two";
    let note = store.create_note(None, text).unwrap();
    let moved = store
        .checklist_to_tasks(&note, text, 0, utf16(text), Some(&errands), "", today())
        .unwrap();
    assert_eq!(moved.list_id, errands);
    assert_eq!(
        list(&store, &errands),
        rows(&[(0, "one", false), (0, "two", false), (0, "old", false)])
    );
    assert_eq!(linked_lines(&store, &note), ["- [ ] one", "- [ ] two"]);
}

#[test]
fn the_inbox_works_as_a_target() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] call mum";
    let note = store.create_note(None, text).unwrap();
    store
        .checklist_to_tasks(&note, text, 0, utf16(text), Some(INBOX), "", today())
        .unwrap();
    assert_eq!(list(&store, INBOX), rows(&[(0, "call mum", false)]));
    assert_eq!(linked_lines(&store, &note), ["- [ ] call mum"]);
}

#[test]
fn only_the_touched_items_are_made_and_linked() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] a\n- [ ] b\n- [ ] c";
    let note = store.create_note(None, text).unwrap();
    let at = text.find("b").unwrap() as u32;
    let moved = store
        .checklist_to_tasks(&note, text, at, at, Some(INBOX), "", today())
        .unwrap();
    assert_eq!(moved.count, 1);
    assert_eq!(linked_lines(&store, &note), ["- [ ] b"]);
}

#[test]
fn what_the_editor_shows_is_saved_first() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Note\n- [ ] a").unwrap();
    // Typed but not saved yet.
    let shown = "Note\nintro\n- [ ] a\n- [ ] b";
    store
        .checklist_to_tasks(&note, shown, 0, utf16(shown), Some(INBOX), "", today())
        .unwrap();
    assert_eq!(body(&store, &note), shown);
    assert_eq!(linked_lines(&store, &note), ["- [ ] a", "- [ ] b"]);
}

#[test]
fn linked_lines_arent_offered_again() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = make(&store, SHOPPING);
    let b = body(&store, &note);
    assert!(store.note_checklist(&note, &b, 0, utf16(&b)).unwrap().is_none());
    // A new item is, alone.
    let b = format!("{b}\n- [ ] bread");
    store.set_note_body(&note, &b, today()).unwrap();
    let c = store.note_checklist(&note, &b, 0, utf16(&b)).unwrap().unwrap();
    assert_eq!(
        c.items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        ["bread"]
    );
}

#[test]
fn making_tasks_again_takes_items_apart_from_the_linked_ones() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    // After plain text: a checklist of its own, not linked until made into tasks.
    edit(&store, &note, "after", "after\n- [ ] bread\n  - [ ] rye");
    assert_eq!(linked(&store, &note).len(), 3);
    let b = body(&store, &note);
    let moved = store
        .checklist_to_tasks(&note, &b, 0, utf16(&b), Some(&list_id), "", today())
        .unwrap();
    assert_eq!(moved.count, 2);
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "bread", false),
            (1, "rye", false),
            (0, "milk", false),
            (1, "oat", true),
            (0, "eggs", false),
        ])
    );
    assert_eq!(linked(&store, &note).len(), 5);
    assert_eq!(body(&store, &note).matches("bread").count(), 1);
}

#[test]
fn nothing_to_make_is_refused_without_tasks() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\njust words";
    let note = store.create_note(None, text).unwrap();
    assert!(
        store
            .checklist_to_tasks(&note, text, 0, utf16(text), None, "", today())
            .is_err()
    );
    assert_eq!(store.lists().unwrap().len(), 1, "only the Inbox");
    assert!(store.tasks(INBOX).unwrap().is_empty());
}

#[test]
fn one_undo_takes_the_tasks_and_links_away() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    assert!(store.undo().unwrap());
    assert!(store.lists().unwrap().iter().all(|l| l.id != list_id));
    assert_eq!(body(&store, &note), SHOPPING);
    assert!(store.note_links(&note).unwrap().is_empty());
    // Redo makes the list and tasks again, but as new ones (Loro recreates undone tree nodes with
    // new ids), so the lines read as plain and can be made into tasks again.
    assert!(store.redo().unwrap());
    assert_eq!(
        store
            .lists()
            .unwrap()
            .iter()
            .filter(|l| l.title == "Groceries")
            .count(),
        1
    );
    assert!(store.note_links(&note).unwrap().is_empty());
    let b = body(&store, &note);
    assert_eq!(
        store
            .note_checklist(&note, &b, 0, utf16(&b))
            .unwrap()
            .unwrap()
            .items
            .len(),
        3
    );
}

#[test]
fn offsets_are_utf16_and_titles_can_be_unicode() {
    let store = Store::open_in_memory().unwrap();
    let text = "😀 Trip\n- [ ] 🧳 bag\n- [ ] café";
    let note = store.create_note(None, text).unwrap();
    let at = text.encode_utf16().count() as u32; // the end, on "café"
    store
        .checklist_to_tasks(&note, text, at, at, Some(INBOX), "", today())
        .unwrap();
    assert_eq!(linked(&store, &note), vec![("- [ ] café".into(), "café".into())]);
    let id = store.note_links(&note).unwrap()[0].task_id.clone();
    store.rename(&id, "crème brûlée").unwrap();
    assert_eq!(body(&store, &note), "😀 Trip\n- [ ] 🧳 bag\n- [ ] crème brûlée");
}

#[test]
fn numbered_starred_and_tabbed_items_link_and_tick() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n1. [ ] one\n* [X] two\n\t+ [ ] three";
    let (note, list_id) = make(&store, text);
    assert_eq!(linked(&store, &note).len(), 3);
    store
        .set_completed(&task_id(&store, &list_id, "one"), true, today())
        .unwrap();
    store
        .set_completed(&task_id(&store, &list_id, "two"), false, today())
        .unwrap();
    store
        .set_completed(&task_id(&store, &list_id, "three"), true, today())
        .unwrap();
    assert_eq!(body(&store, &note), "Note\n1. [x] one\n* [ ] two\n\t+ [x] three");
}

// ---- the note's edits reach the tasks ----------------------------------------------------------

#[test]
fn ticking_a_box_completes_its_task_and_unticking_reopens_it() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] milk", "- [x] milk");
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "milk"))
            .unwrap()
            .completed_at
            .is_some()
    );
    edit(&store, &note, "  - [x] oat", "  - [ ] oat");
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "oat"))
            .unwrap()
            .completed_at
            .is_none()
    );
    assert_eq!(linked(&store, &note).len(), 3, "still linked after ticks");
}

#[test]
fn renaming_a_line_renames_its_task() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs", "- [ ] a dozen eggs");
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "milk", false), (1, "oat", true), (0, "a dozen eggs", false)])
    );
}

#[test]
fn typing_on_at_a_lines_end_keeps_the_link() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let milk = task_id(&store, &list_id, "milk");
    // Letter by letter, as a keyboard sends it: each is a save of its own.
    let mut line = "- [ ] milk".to_owned();
    for c in " 2% x".chars() {
        let next = format!("{line}{c}");
        edit(&store, &note, &format!("{line}\n"), &format!("{next}\n"));
        line = next;
    }
    assert_eq!(store.task_by_id(&milk).unwrap().title, "milk 2% x");
    assert_eq!(linked(&store, &note)[0].0, "- [ ] milk 2% x");
}

#[test]
fn retyping_a_whole_title_keeps_the_link() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs", "- [ ] ");
    // An emptied title leaves the task's title alone…
    assert!(store.task_by_id(&task_id(&store, &list_id, "eggs")).is_ok());
    assert_eq!(
        body(&store, &note),
        SHOPPING.replace("- [ ] eggs", "- [ ] "),
        "…and isn't put back while typing"
    );
    edit(&store, &note, "- [ ] \n", "- [ ] butter\n");
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "milk", false), (1, "oat", true), (0, "butter", false)])
    );
}

#[test]
fn indenting_or_typing_before_a_line_keeps_the_link() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs", "  - [ ] eggs");
    edit(&store, &note, "## Groceries\n", "## Groceries\nfirst:\n");
    assert_eq!(linked(&store, &note).len(), 3);
    edit(&store, &note, "  - [ ] eggs", "  - [x] eggs");
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "eggs"))
            .unwrap()
            .completed_at
            .is_some()
    );
}

#[test]
fn edits_elsewhere_in_the_note_leave_tasks_alone() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let before = list(&store, &list_id);
    edit(&store, &note, "after", "after all\n- [x] not a task");
    assert_eq!(list(&store, &list_id), before);
    assert_eq!(linked(&store, &note).len(), 3);
}

#[test]
fn a_note_edit_and_the_task_change_are_one_undo_step() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] milk", "- [x] whole milk");
    assert!(store.undo().unwrap());
    assert_eq!(body(&store, &note), SHOPPING);
    let milk = store.task_by_id(&task_id(&store, &list_id, "milk")).unwrap();
    assert!(milk.completed_at.is_none());
}

#[test]
fn ticking_a_repeating_task_moves_it_on_and_the_box_clears() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let milk = task_id(&store, &list_id, "milk");
    store
        .set_schedule(
            &milk,
            &Schedule {
                due: Some(today()),
                time: None,
                repeat: Some(Repeat {
                    freq: Freq::Daily,
                    interval: 1,
                    weekdays: 0,
                }),
            },
        )
        .unwrap();
    edit(&store, &note, "- [ ] milk", "- [x] milk");
    let task = store.task_by_id(&milk).unwrap();
    assert!(task.completed_at.is_none());
    assert_eq!(task.due.as_deref(), Some("2026-11-13"));
    assert!(
        body(&store, &note).contains("- [ ] milk"),
        "the box clears for the next time"
    );
    // The completed copy of this time isn't brought into the note.
    assert_eq!(body(&store, &note).matches("milk").count(), 1);
}

#[test]
fn deleting_a_line_leaves_the_task_and_it_doesnt_come_back() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs\n", "");
    assert!(store.task_by_id(&task_id(&store, &list_id, "eggs")).is_ok());
    // Other changes don't bring the line back.
    store.add_task(INBOX, "unrelated").unwrap();
    store.rename(&task_id(&store, &list_id, "eggs"), "eggs!").unwrap();
    assert!(!body(&store, &note).contains("eggs"));
    assert_eq!(linked(&store, &note).len(), 2);
}

#[test]
fn a_split_line_keeps_the_link_on_its_first_half_and_the_rest_becomes_a_task() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    // Return in the middle of "eggs", the list continuing.
    edit(&store, &note, "- [ ] eggs", "- [ ] eg\n- [ ] gs");
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "milk", false),
            (1, "oat", true),
            (0, "eg", false),
            (0, "gs", false)
        ])
    );
    assert_eq!(linked(&store, &note).len(), 4);
    // Taking the first half out leaves the second with its own task.
    edit(&store, &note, "- [ ] eg\n", "");
    edit(&store, &note, "- [ ] gs", "- [x] gs");
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "gs"))
            .unwrap()
            .completed_at
            .is_some()
    );
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "eg"))
            .unwrap()
            .completed_at
            .is_none()
    );
}

#[test]
fn a_line_turned_into_plain_text_unlinks_until_its_box_comes_back() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs", "eggs");
    assert_eq!(linked(&store, &note).len(), 2);
    assert!(store.task_by_id(&task_id(&store, &list_id, "eggs")).is_ok());
    edit(&store, &note, "\neggs", "\n- [ ] eggs");
    // The box is new text, but the title kept its mark.
    assert_eq!(linked(&store, &note).len(), 3);
}

// ---- the tasks' changes reach the note -------------------------------------------------------

#[test]
fn completing_and_reopening_a_task_ticks_its_line() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let eggs = task_id(&store, &list_id, "eggs");
    store.set_completed(&eggs, true, today()).unwrap();
    assert!(body(&store, &note).contains("- [x] eggs"));
    store.set_completed(&eggs, false, today()).unwrap();
    assert_eq!(body(&store, &note), SHOPPING);
}

#[test]
fn renaming_a_task_renames_its_line_and_it_stays_linked() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let eggs = task_id(&store, &list_id, "eggs");
    store.rename(&eggs, "free range eggs").unwrap();
    assert!(body(&store, &note).contains("\n- [ ] free range eggs\n"));
    store.rename(&eggs, "eggs").unwrap();
    assert_eq!(body(&store, &note), SHOPPING);
    edit(&store, &note, "- [ ] eggs", "- [x] eggs");
    assert!(store.task_by_id(&eggs).unwrap().completed_at.is_some());
}

#[test]
fn a_rename_keeps_whatever_follows_the_box() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ]    spaced   ");
    store
        .rename(&task_id(&store, &list_id, "spaced"), "tight")
        .unwrap();
    assert_eq!(body(&store, &note), "Note\n- [ ] tight");
}

#[test]
fn a_deleted_task_unlinks_its_line_and_undo_links_it_again() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let eggs = task_id(&store, &list_id, "eggs");
    store.delete(&eggs).unwrap();
    assert_eq!(body(&store, &note), SHOPPING, "the line stays as plain text");
    assert_eq!(linked(&store, &note).len(), 2);
    edit(&store, &note, "- [ ] eggs", "- [x] eggs");
    assert!(store.undo().unwrap(), "undo the tick");
    assert!(store.undo().unwrap(), "undo the delete");
    assert_eq!(linked(&store, &note).len(), 3);
    store.rename(&eggs, "duck eggs").unwrap();
    assert!(body(&store, &note).contains("- [ ] duck eggs"));
}

#[test]
fn a_deleted_list_unlinks_every_line() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store.delete(&list_id).unwrap();
    assert!(store.note_links(&note).unwrap().is_empty());
    assert_eq!(body(&store, &note), SHOPPING);
}

#[test]
fn a_new_subtask_comes_in_under_its_parent() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let eggs = task_id(&store, &list_id, "eggs");
    store.add_task(&eggs, "free range").unwrap();
    store.add_task(&eggs, "large").unwrap();
    assert_eq!(
        body(&store, &note),
        "Shopping\n## Groceries\n- [ ] milk\n  - [x] oat\n- [ ] eggs\n  - [ ] free range\n  - [ ] large\nafter"
    );
    assert_eq!(linked(&store, &note).len(), 5);
    // A subtask after a ticked one goes after it.
    store.add_task(&task_id(&store, &list_id, "milk"), "soy").unwrap();
    assert!(body(&store, &note).contains("- [ ] milk\n  - [x] oat\n  - [ ] soy\n- [ ] eggs"));
}

#[test]
fn a_subtask_goes_past_its_siblings_nested_lines() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(
        &store,
        "Note\n- [ ] a\n  - [ ] b\n    - [ ] c\n    some notes\n- [ ] d",
    );
    store.add_task(&task_id(&store, &list_id, "a"), "e").unwrap();
    assert_eq!(
        body(&store, &note),
        "Note\n- [ ] a\n  - [ ] b\n    - [ ] c\n    some notes\n  - [ ] e\n- [ ] d"
    );
}

#[test]
fn a_subtask_with_subtasks_comes_in_whole() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] trip");
    let trip = task_id(&store, &list_id, "trip");
    // Made on another device in one go, arriving as one change: here, in one store, step by step.
    let pack = store.add_task(&trip, "pack").unwrap();
    store.add_task(&pack, "socks").unwrap();
    store.add_task(&pack, "charger").unwrap();
    assert_eq!(
        body(&store, &note),
        "Note\n- [ ] trip\n  - [ ] pack\n    - [ ] socks\n    - [ ] charger"
    );
}

#[test]
fn tasks_added_to_a_list_made_from_the_note_come_in_first() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store.add_task(&list_id, "bread").unwrap();
    assert_eq!(
        body(&store, &note),
        "Shopping\n## Groceries\n- [ ] bread\n- [ ] milk\n  - [x] oat\n- [ ] eggs\nafter"
    );
    assert_eq!(linked(&store, &note).len(), 4);
}

#[test]
fn a_task_moved_within_the_list_comes_in_after_its_sibling_above() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] b\n- [ ] c");
    let x = store.add_task(INBOX, "x").unwrap();
    store.move_task(&x, &list_id, 2).unwrap();
    assert_eq!(body(&store, &note), "Note\n- [ ] a\n- [ ] b\n- [ ] x\n- [ ] c");
}

#[test]
fn tasks_added_to_an_existing_list_stay_out() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_list("Errands").unwrap();
    let text = "Note\n- [ ] one";
    let note = store.create_note(None, text).unwrap();
    store
        .checklist_to_tasks(&note, text, 0, utf16(text), Some(&errands), "", today())
        .unwrap();
    store.add_task(&errands, "two").unwrap();
    assert_eq!(body(&store, &note), text);
    // Subtasks of its own tasks still come in.
    store
        .add_task(&task_id(&store, &errands, "one"), "one.a")
        .unwrap();
    assert_eq!(body(&store, &note), "Note\n- [ ] one\n  - [ ] one.a");
}

#[test]
fn with_every_linked_line_gone_new_list_tasks_go_at_the_end() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\nend");
    edit(&store, &note, "- [ ] a\n", "");
    store.add_task(&list_id, "b").unwrap();
    assert_eq!(body(&store, &note), "Note\nend\n- [ ] b");
    // Ending on a newline: no blank line in between.
    edit(&store, &note, "- [ ] b", "");
    store.add_task(&list_id, "c").unwrap();
    assert_eq!(body(&store, &note), "Note\nend\n- [ ] c");
}

#[test]
fn undoing_a_new_subtask_takes_its_line_too() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store
        .add_task(&task_id(&store, &list_id, "eggs"), "large")
        .unwrap();
    assert!(store.undo().unwrap());
    assert_eq!(body(&store, &note), SHOPPING);
}

#[test]
fn a_deleted_note_is_left_alone() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store.delete_note_or_folder(&note).unwrap();
    store.rename(&task_id(&store, &list_id, "eggs"), "x").unwrap();
    store.add_task(&list_id, "y").unwrap();
    assert!(store.undo().unwrap());
    assert!(store.undo().unwrap());
    assert!(store.undo().unwrap(), "undo the delete");
    assert_eq!(body(&store, &note), SHOPPING);
}

#[test]
fn one_task_change_reaches_every_note_linking_it() {
    let store = Store::open_in_memory().unwrap();
    let (a, list_id) = make(&store, SHOPPING);
    let (b, _) = make(&store, "Other\n- [ ] tea");
    store.rename(&task_id(&store, &list_id, "eggs"), "EGGS").unwrap();
    assert!(body(&store, &a).contains("- [ ] EGGS"));
    assert_eq!(body(&store, &b), "Other\n- [ ] tea");
}

#[test]
fn unrelated_changes_dont_touch_linked_notes() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = make(&store, SHOPPING);
    let updated = store.notes(None).unwrap()[0].updated_at;
    store.add_task(INBOX, "x").unwrap();
    store.create_habit("water", 1, 0b111_1111).unwrap();
    assert_eq!(body(&store, &note), SHOPPING);
    assert_eq!(store.notes(None).unwrap()[0].updated_at, updated);
}

#[test]
fn notes_without_links_are_plain_text() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] a";
    let note = store.create_note(None, text).unwrap();
    store.add_task(INBOX, "a").unwrap();
    store.set_note_body(&note, "Note\n- [x] a", today()).unwrap();
    assert!(store.tasks(INBOX).unwrap()[0].completed_at.is_none());
    assert!(store.note_links(&note).unwrap().is_empty());
}

// ---- sync ----------------------------------------------------------------------------------

#[test]
fn links_sync_and_both_ways_work_on_the_other_device() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(linked(&phone, &note).len(), 3);

    // The phone ticks in the note; the Mac sees the task done.
    edit(&phone, &note, "- [ ] eggs", "- [x] eggs");
    log.sync(&phone);
    log.sync(&mac);
    assert!(
        mac.task_by_id(&task_id(&mac, &list_id, "eggs"))
            .unwrap()
            .completed_at
            .is_some()
    );
    assert!(body(&mac, &note).contains("- [x] eggs"));

    // The Mac renames the task; the phone's note follows.
    mac.rename(&task_id(&mac, &list_id, "milk"), "oat milk").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert!(body(&phone, &note).contains("- [ ] oat milk"));
    assert_eq!(body(&phone, &note), body(&mac, &note));
}

#[test]
fn a_synced_subtask_isnt_put_in_twice() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    mac.add_task(&task_id(&mac, &list_id, "eggs"), "large").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    // The phone's next change of its own catches up again: the line is already there.
    phone.add_task(INBOX, "x").unwrap();
    edit(&phone, &note, "after", "after!");
    log.sync(&phone);
    log.sync(&mac);
    for store in [&mac, &phone] {
        assert_eq!(body(store, &note).matches("large").count(), 1);
    }
    assert_eq!(body(&mac, &note), body(&phone, &note));
}

#[test]
fn concurrent_edits_of_the_note_and_the_task_settle() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    // Offline at once: the Mac renames eggs, the phone ticks milk in the note.
    mac.rename(&task_id(&mac, &list_id, "eggs"), "eggs (6)").unwrap();
    edit(&phone, &note, "- [ ] milk", "- [x] milk");
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    for store in [&mac, &phone] {
        let b = body(store, &note);
        assert!(b.contains("- [x] milk"), "{b}");
        assert!(b.contains("- [ ] eggs (6)"), "{b}");
        assert_eq!(linked(store, &note).len(), 3);
    }
}

#[test]
fn subtasks_added_on_two_devices_at_once_both_come_in_once() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    let eggs = task_id(&mac, &list_id, "eggs");
    mac.add_task(&eggs, "from mac").unwrap();
    phone.add_task(&eggs, "from phone").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    for store in [&mac, &phone] {
        let b = body(store, &note);
        assert_eq!(b.matches("from mac").count(), 1, "{b}");
        assert_eq!(b.matches("from phone").count(), 1, "{b}");
        assert_eq!(linked(store, &note).len(), 5);
    }
}

// ---- notes from earlier versions -------------------------------------------------------------

/// A note holding a task block of the earlier version, as it was saved then.
fn with_block(store: &Store, list_id: &str, source: TaskSource) -> String {
    let text = format!("Errands\nbefore\n{}\nafter", task_block_markup(&source));
    let note = store.create_note(None, &text).unwrap();
    let _ = list_id;
    note
}

#[test]
fn a_list_block_opens_as_linked_lines_and_keeps_taking_new_tasks() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_list("Errands").unwrap();
    let post = store.add_task(&errands, "post office").unwrap();
    store.add_task(&post, "stamps").unwrap();
    let bank = store.add_task(&errands, "bank").unwrap();
    store.set_completed(&bank, true, today()).unwrap();
    let note = with_block(&store, &errands, TaskSource::List(errands.clone()));
    let opened = store.open_note(&note).unwrap();
    assert_eq!(
        opened,
        "Errands\nbefore\n- [ ] post office\n  - [ ] stamps\n- [x] bank\nafter"
    );
    assert_eq!(body(&store, &note), opened);
    assert_eq!(linked(&store, &note).len(), 3);
    store.add_task(&errands, "pharmacy").unwrap();
    assert!(body(&store, &note).contains("before\n- [ ] pharmacy\n- [ ] post office"));
}

#[test]
fn a_block_of_picked_tasks_opens_as_their_lines_only() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_list("Errands").unwrap();
    store.add_task(&errands, "other").unwrap();
    let a = store.add_task(&errands, "a").unwrap();
    let note = with_block(&store, &errands, TaskSource::Tasks(vec![a.clone()]));
    assert_eq!(store.open_note(&note).unwrap(), "Errands\nbefore\n- [ ] a\nafter");
    store.add_task(&errands, "new").unwrap();
    assert!(!body(&store, &note).contains("new"));
    store.add_task(&a, "a.1").unwrap();
    assert!(body(&store, &note).contains("- [ ] a\n  - [ ] a.1\nafter"));
}

#[test]
fn a_block_whose_tasks_are_gone_opens_without_its_line() {
    let store = Store::open_in_memory().unwrap();
    let note = with_block(&store, "", TaskSource::List("99@99".into()));
    assert_eq!(store.open_note(&note).unwrap(), "Errands\nbefore\nafter");
}

#[test]
fn per_line_markers_open_as_links() {
    let store = Store::open_in_memory().unwrap();
    let tea = store.add_task(INBOX, "tea").unwrap();
    let note = store
        .create_note(None, &format!("Note\n- [ ] tea <!--task:{tea}-->\n- [ ] plain"))
        .unwrap();
    assert_eq!(
        body(&store, &note),
        "Note\n- [ ] tea\n- [ ] plain",
        "read with markers taken out"
    );
    assert_eq!(store.open_note(&note).unwrap(), "Note\n- [ ] tea\n- [ ] plain");
    assert_eq!(linked(&store, &note), vec![("- [ ] tea".into(), "tea".into())]);
    store.set_completed(&tea, true, today()).unwrap();
    assert_eq!(body(&store, &note), "Note\n- [x] tea\n- [ ] plain");
}

#[test]
fn opening_an_old_note_isnt_undone() {
    let store = Store::open_in_memory().unwrap();
    let errands = store.create_list("Errands").unwrap();
    store.add_task(&errands, "a").unwrap();
    let note = with_block(&store, &errands, TaskSource::List(errands.clone()));
    store.rename(&errands, "Chores").unwrap();
    let opened = store.open_note(&note).unwrap();
    // Undo takes the user's last step (the rename), not the update.
    assert!(store.undo().unwrap());
    assert!(store.lists().unwrap().iter().any(|l| l.title == "Errands"));
    assert_eq!(body(&store, &note), opened);
    assert_eq!(linked(&store, &note).len(), 1);
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

// ---- more edits and odd cases ------------------------------------------------------------------

#[test]
fn links_survive_closing_and_opening_the_store() {
    let dir = std::env::temp_dir().join(format!("not-links-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("db.sqlite");
    let (note, list_id) = {
        let store = Store::open(&path).unwrap();
        make(&store, SHOPPING)
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(linked(&store, &note).len(), 3);
    // The mark still keeps out of text typed at a line's end after reopening.
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\nnew line\n");
    store.rename(&task_id(&store, &list_id, "eggs"), "EGGS").unwrap();
    assert!(body(&store, &note).contains("- [ ] EGGS\nnew line\n"));
    drop(store);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_pasted_copy_of_a_line_isnt_linked() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = make(&store, SHOPPING);
    edit(&store, &note, "after", "after\n- [ ] milk");
    assert_eq!(linked(&store, &note).len(), 3);
    assert_eq!(body(&store, &note).matches("- [ ] milk").count(), 2);
}

#[test]
fn joining_two_linked_lines_leaves_one_link_and_both_tasks() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] b");
    // Backspace at the start of the second line.
    edit(&store, &note, "- [ ] a\n- [ ] b", "- [ ] a- [ ] b");
    let links = linked(&store, &note);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].0, "- [ ] a- [ ] b");
    assert_eq!(links[0].1, "a- [ ] b", "the linked task took the line's title");
    assert_eq!(store.tasks(&list_id).unwrap().len(), 2, "no task deleted");
}

#[test]
fn deleting_the_whole_checklist_and_undoing_brings_the_links_back() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store.set_note_body(&note, "Shopping", today()).unwrap();
    assert!(store.note_links(&note).unwrap().is_empty());
    assert_eq!(list(&store, &list_id).len(), 3, "the tasks stay");
    assert!(store.undo().unwrap());
    assert_eq!(body(&store, &note), SHOPPING);
    assert_eq!(linked(&store, &note).len(), 3);
}

#[test]
fn a_task_moved_to_another_list_stays_linked() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let eggs = task_id(&store, &list_id, "eggs");
    store.move_task(&eggs, INBOX, 0).unwrap();
    store.set_completed(&eggs, true, today()).unwrap();
    assert!(body(&store, &note).contains("- [x] eggs"));
    // And other Inbox tasks don't come in.
    store.add_task(INBOX, "inbox task").unwrap();
    assert!(!body(&store, &note).contains("inbox task"));
}

#[test]
fn a_task_moved_under_another_linked_task_keeps_its_line_where_it_is() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store.indent_task(&task_id(&store, &list_id, "eggs")).unwrap();
    assert_eq!(body(&store, &note), SHOPPING);
    assert_eq!(linked(&store, &note).len(), 3);
}

#[test]
fn clearing_the_archive_leaves_ticked_lines_unlinked() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store
        .set_completed(&task_id(&store, &list_id, "eggs"), true, today())
        .unwrap();
    let later = not_core::ARCHIVE_AFTER_MS * 2
        + std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
    assert_eq!(store.clear_archive(later).unwrap(), 1, "eggs");
    assert_eq!(body(&store, &note), SHOPPING.replace("- [ ] eggs", "- [x] eggs"));
    assert_eq!(linked_lines(&store, &note), ["- [ ] milk", "  - [x] oat"]);
}

#[test]
fn a_line_deleted_on_one_device_while_the_other_adds_a_subtask_settles() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    mac.add_task(&task_id(&mac, &list_id, "eggs"), "large").unwrap();
    edit(&phone, &note, "- [ ] eggs\n", "");
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    assert_eq!(body(&mac, &note), body(&phone, &note));
    // The subtask's line made it; the parent's is gone. Both tasks stay.
    let b = body(&mac, &note);
    assert!(b.contains("  - [ ] large"), "{b}");
    assert!(!b.contains("- [ ] eggs"), "{b}");
    assert_eq!(
        mac.outline(&list_id).unwrap().open.len(),
        4,
        "milk, oat, eggs, large"
    );
}

#[test]
fn a_long_checklist_links_and_follows() {
    let store = Store::open_in_memory().unwrap();
    let items: Vec<String> = (0..200).map(|i| format!("- [ ] item {i}")).collect();
    let text = format!("Big\n{}", items.join("\n"));
    let started = std::time::Instant::now();
    let (note, list_id) = make(&store, &text);
    assert_eq!(linked(&store, &note).len(), 200);
    for i in (0..200).step_by(20) {
        store
            .set_completed(&task_id(&store, &list_id, &format!("item {i}")), true, today())
            .unwrap();
    }
    assert_eq!(body(&store, &note).matches("- [x]").count(), 10);
    assert!(started.elapsed().as_secs() < 5, "took {:?}", started.elapsed());
}

#[test]
fn ticking_in_the_note_completes_subtasks_on_their_own() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "  - [x] oat", "  - [ ] oat");
    edit(&store, &note, "- [ ] milk", "- [x] milk");
    // The parent is done; its subtask keeps its own state, as in the list.
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "oat"))
            .unwrap()
            .completed_at
            .is_none()
    );
    assert!(body(&store, &note).contains("  - [ ] oat"));
}

#[test]
fn several_lines_changed_in_one_save_all_reach_their_tasks() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let b = body(&store, &note)
        .replace("- [ ] milk", "- [x] milk")
        .replace("  - [x] oat", "  - [ ] oat milk")
        .replace("- [ ] eggs", "- [x] eggs!");
    store.set_note_body(&note, &b, today()).unwrap();
    for (title, done) in [("milk", true), ("oat milk", false), ("eggs!", true)] {
        assert_eq!(
            store
                .task_by_id(&task_id(&store, &list_id, title))
                .unwrap()
                .completed_at
                .is_some(),
            done,
            "{title}"
        );
    }
    assert!(store.undo().unwrap());
    assert_eq!(body(&store, &note), SHOPPING, "one undo step for all of it");
}

// ---- items typed among linked ones become tasks --------------------------------------------------

#[test]
fn return_at_a_linked_line_makes_the_next_task() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\n- [ ] butter\n");
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "milk", false),
            (1, "oat", true),
            (0, "eggs", false),
            (0, "butter", false)
        ])
    );
    assert_eq!(linked_lines(&store, &note).last().unwrap(), "- [ ] butter");
    // At the very end of the note too.
    let (note, list_id) = make(&store, "Note\n- [ ] a");
    store
        .set_note_body(&note, "Note\n- [ ] a\n- [ ] b", today())
        .unwrap();
    assert_eq!(list(&store, &list_id), rows(&[(0, "a", false), (0, "b", false)]));
}

#[test]
fn a_line_put_in_between_goes_in_between() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] c");
    edit(&store, &note, "- [ ] a\n", "- [ ] a\n- [ ] b\n");
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "a", false), (0, "b", false), (0, "c", false)])
    );
    // Above the first one: before it.
    edit(&store, &note, "Note\n", "Note\n- [ ] first\n");
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "first", false),
            (0, "a", false),
            (0, "b", false),
            (0, "c", false)
        ])
    );
}

#[test]
fn an_indented_line_becomes_a_subtask_in_its_place() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    // The first subtask of eggs, then one before oat under milk.
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\n  - [ ] large\n");
    edit(&store, &note, "- [ ] milk\n", "- [ ] milk\n  - [ ] whole\n");
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "milk", false),
            (1, "whole", false),
            (1, "oat", true),
            (0, "eggs", false),
            (1, "large", false),
        ])
    );
    assert_eq!(linked(&store, &note).len(), 5);
    // Deeper: under a subtask.
    edit(
        &store,
        &note,
        "  - [ ] large\n",
        "  - [ ] large\n    - [ ] brown\n",
    );
    let large = task_id(&store, &list_id, "large");
    assert_eq!(
        store
            .task_by_id(&task_id(&store, &list_id, "brown"))
            .unwrap()
            .parent_id
            .as_deref(),
        Some(large.as_str())
    );
}

#[test]
fn an_empty_item_waits_for_its_title_and_typing_on_renames_it() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\n- [ ] \n");
    assert_eq!(list(&store, &list_id).len(), 3, "no task for an empty item");
    edit(&store, &note, "- [ ] \n", "- [ ] bu\n");
    edit(&store, &note, "- [ ] bu\n", "- [ ] butter\n");
    assert_eq!(
        list(&store, &list_id)
            .iter()
            .filter(|r| r.1.starts_with("bu"))
            .count(),
        1
    );
    assert!(store.task_by_id(&task_id(&store, &list_id, "butter")).is_ok());
}

#[test]
fn a_ticked_new_item_makes_a_completed_task() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\n- [x] salt\n");
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "salt"))
            .unwrap()
            .completed_at
            .is_some()
    );
}

#[test]
fn several_lines_pasted_in_become_tasks_in_order() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(
        &store,
        &note,
        "- [ ] eggs\n",
        "- [ ] eggs\n- [ ] bread\n  - [ ] rye\n  - [ ] spelt\n- [ ] jam\n",
    );
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "milk", false),
            (1, "oat", true),
            (0, "eggs", false),
            (0, "bread", false),
            (1, "rye", false),
            (1, "spelt", false),
            (0, "jam", false),
        ])
    );
    assert_eq!(linked(&store, &note).len(), 7);
}

#[test]
fn items_after_plain_text_or_in_unlinked_notes_stay_plain() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "after", "after\n- [ ] not mine");
    assert_eq!(list(&store, &list_id).len(), 3);
    let plain = store.create_note(None, "Plain\n- [ ] a").unwrap();
    store
        .set_note_body(&plain, "Plain\n- [ ] a\n- [ ] b", today())
        .unwrap();
    assert!(store.note_links(&plain).unwrap().is_empty());
    assert!(store.tasks(INBOX).unwrap().is_empty());
}

#[test]
fn blank_lines_inside_the_checklist_dont_end_it() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n\n- [ ] b");
    edit(&store, &note, "\n\n- [ ] b", "\n\n- [ ] b\n- [ ] c");
    assert_eq!(list(&store, &list_id).len(), 3);
}

#[test]
fn a_deleted_tasks_line_doesnt_make_a_new_task() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    store.delete(&task_id(&store, &list_id, "eggs")).unwrap();
    edit(&store, &note, "after", "after!");
    assert_eq!(list(&store, &list_id).len(), 2, "eggs stays deleted");
    // Typing on the line doesn't revive it either.
    edit(&store, &note, "- [ ] eggs", "- [ ] eggs and ham");
    assert_eq!(list(&store, &list_id).len(), 2);
}

#[test]
fn a_typed_task_and_its_line_are_one_undo_step() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\n- [ ] butter\n");
    assert!(store.undo().unwrap());
    assert_eq!(body(&store, &note), SHOPPING);
    assert_eq!(list(&store, &list_id).len(), 3);
}

#[test]
fn typed_tasks_in_a_list_made_from_the_note_arent_put_in_twice() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] milk\n", "- [ ] milk\n- [ ] cream\n");
    store.add_task(INBOX, "anything").unwrap();
    assert_eq!(body(&store, &note).matches("cream").count(), 1);
    assert_eq!(list(&store, &list_id).len(), 4);
}

#[test]
fn typed_tasks_join_the_inbox_when_their_siblings_are_there() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] call mum";
    let note = store.create_note(None, text).unwrap();
    store
        .checklist_to_tasks(&note, text, 0, utf16(text), Some(INBOX), "", today())
        .unwrap();
    edit(&store, &note, "- [ ] call mum", "- [ ] call mum\n- [ ] call dad");
    assert_eq!(
        list(&store, INBOX),
        rows(&[(0, "call mum", false), (0, "call dad", false)])
    );
}

#[test]
fn a_typed_task_syncs_once_with_its_link() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    edit(&phone, &note, "- [ ] eggs\n", "- [ ] eggs\n- [ ] butter\n");
    log.sync(&phone);
    log.sync(&mac);
    mac.add_task(INBOX, "x").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    for store in [&mac, &phone] {
        assert_eq!(body(store, &note).matches("butter").count(), 1);
        assert_eq!(list(store, &list_id).len(), 4);
        assert_eq!(linked(store, &note).len(), 4);
    }
    mac.rename(&task_id(&mac, &list_id, "butter"), "salted butter")
        .unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert!(body(&phone, &note).contains("- [ ] salted butter"));
}

#[test]
fn a_linked_lines_task_moved_elsewhere_takes_new_siblings_along() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    let eggs = task_id(&store, &list_id, "eggs");
    store.move_task(&eggs, INBOX, 0).unwrap();
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\n- [ ] ham\n");
    assert!(store.tasks(INBOX).unwrap().iter().any(|t| t.title == "ham"));
}

// ---- nesting changed in the note moves the tasks -------------------------------------------------

/// The parent task's title of `title` in `list_id`, if it is a subtask.
fn parent_of(store: &Store, list_id: &str, title: &str) -> Option<String> {
    let task = store.task_by_id(&task_id(store, list_id, title)).unwrap();
    task.parent_id.map(|p| store.task_by_id(&p).unwrap().title)
}

#[test]
fn indenting_a_linked_line_makes_its_task_a_subtask_and_outdenting_takes_it_back() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs", "  - [ ] eggs");
    assert_eq!(parent_of(&store, &list_id, "eggs").as_deref(), Some("milk"));
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "milk", false), (1, "oat", true), (1, "eggs", false)]),
        "after the subtask above it"
    );
    edit(&store, &note, "  - [ ] eggs", "- [ ] eggs");
    assert_eq!(parent_of(&store, &list_id, "eggs"), None);
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "milk", false), (1, "oat", true), (0, "eggs", false)])
    );
    assert_eq!(linked(&store, &note).len(), 3);
}

#[test]
fn outdenting_a_first_subtask_puts_it_after_its_old_parent() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n  - [ ] a1\n  - [ ] a2\n- [ ] b");
    edit(&store, &note, "  - [ ] a1", "- [ ] a1");
    assert_eq!(parent_of(&store, &list_id, "a1"), None);
    // In the note a2 now sits under a1, so it follows.
    assert_eq!(parent_of(&store, &list_id, "a2").as_deref(), Some("a1"));
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "a", false),
            (0, "a1", false),
            (1, "a2", false),
            (0, "b", false)
        ])
    );
}

#[test]
fn indenting_a_line_with_its_subtasks_moves_them_together() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] b\n  - [ ] b1");
    edit(&store, &note, "- [ ] b\n  - [ ] b1", "  - [ ] b\n    - [ ] b1");
    assert_eq!(parent_of(&store, &list_id, "b").as_deref(), Some("a"));
    assert_eq!(parent_of(&store, &list_id, "b1").as_deref(), Some("b"));
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "a", false), (1, "b", false), (2, "b1", false)])
    );
}

#[test]
fn indenting_a_parent_alone_follows_the_note() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] b\n  - [ ] b1");
    // Only b moves in: b1 now lines up with b, so both are a's subtasks.
    edit(&store, &note, "- [ ] b\n", "  - [ ] b\n");
    assert_eq!(parent_of(&store, &list_id, "b").as_deref(), Some("a"));
    assert_eq!(parent_of(&store, &list_id, "b1").as_deref(), Some("a"));
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "a", false), (1, "b", false), (1, "b1", false)])
    );
}

#[test]
fn two_levels_at_once_and_tabs_count() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n  - [ ] a1\n- [ ] c");
    edit(&store, &note, "- [ ] c", "\t- [ ] c");
    assert_eq!(parent_of(&store, &list_id, "c").as_deref(), Some("a1"));
}

#[test]
fn indenting_under_a_plain_item_moves_nothing() {
    let store = Store::open_in_memory().unwrap();
    let text = "Note\n- [ ] a";
    let note = store.create_note(None, text).unwrap();
    store
        .checklist_to_tasks(&note, text, 0, utf16(text), Some(INBOX), "", today())
        .unwrap();
    // A plain item above, outside the group, then a indented under it.
    store
        .set_note_body(&note, "Note\n- [ ] plain\n  - [ ] a", today())
        .unwrap();
    let tasks = store.tasks(INBOX).unwrap();
    assert!(tasks.iter().all(|t| t.parent_id.is_none()));
    assert!(
        !tasks.iter().any(|t| t.title == "plain"),
        "a plain parent isn't made a task"
    );
}

#[test]
fn an_indent_and_a_tick_in_one_save_are_one_undo_step() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs", "  - [x] eggs");
    let eggs = store.task_by_id(&task_id(&store, &list_id, "eggs")).unwrap();
    assert!(eggs.completed_at.is_some());
    assert_eq!(parent_of(&store, &list_id, "eggs").as_deref(), Some("milk"));
    assert!(store.undo().unwrap());
    assert_eq!(body(&store, &note), SHOPPING);
    assert_eq!(parent_of(&store, &list_id, "eggs"), None);
    assert!(
        store
            .task_by_id(&task_id(&store, &list_id, "eggs"))
            .unwrap()
            .completed_at
            .is_none()
    );
}

#[test]
fn moving_lines_without_changing_their_nesting_moves_nothing() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] b");
    store
        .set_note_body(&note, "Note\n- [ ] b\n- [ ] a", today())
        .unwrap();
    assert_eq!(
        list(&store, &list_id),
        rows(&[(0, "a", false), (0, "b", false)]),
        "the list keeps its order"
    );
}

#[test]
fn a_typed_item_indented_later_becomes_a_subtask() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, SHOPPING);
    edit(&store, &note, "- [ ] eggs\n", "- [ ] eggs\n- [ ] large\n");
    assert_eq!(parent_of(&store, &list_id, "large"), None);
    edit(&store, &note, "- [ ] large", "  - [ ] large");
    assert_eq!(parent_of(&store, &list_id, "large").as_deref(), Some("eggs"));
    assert_eq!(list(&store, &list_id).len(), 4, "moved, not made again");
}

#[test]
fn an_indent_on_one_device_nests_the_task_on_the_other() {
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let mut log = Log::default();
    let (note, list_id) = make(&mac, SHOPPING);
    log.sync(&mac);
    log.sync(&phone);
    edit(&phone, &note, "- [ ] eggs", "  - [ ] eggs");
    log.sync(&phone);
    log.sync(&mac);
    assert_eq!(parent_of(&mac, &list_id, "eggs").as_deref(), Some("milk"));
    assert_eq!(body(&mac, &note), body(&phone, &note));
}

#[test]
fn a_subtask_line_moved_under_another_parent_follows() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n  - [ ] x\n- [ ] b");
    // Cut "x" from under a, paste it under b: the cut takes its link along only if the editor
    // keeps the characters, which a cut and paste doesn't — so it's a typed item under b.
    store
        .set_note_body(&note, "Note\n- [ ] a\n- [ ] b\n  - [ ] x", today())
        .unwrap();
    let xs: Vec<_> = list(&store, &list_id)
        .into_iter()
        .filter(|r| r.1 == "x")
        .collect();
    assert!(!xs.is_empty());
    assert!(linked(&store, &note).iter().any(|(l, _)| l == "  - [ ] x"));
}

// ---- lines keep their own tasks ------------------------------------------------------------------

#[test]
fn a_line_put_in_between_leaves_the_other_tasks_as_they_were() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] c");
    let (a, c) = (task_id(&store, &list_id, "a"), task_id(&store, &list_id, "c"));
    edit(&store, &note, "- [ ] a\n", "- [ ] a\n- [ ] b\n");
    assert_eq!(task_id(&store, &list_id, "a"), a);
    assert_eq!(task_id(&store, &list_id, "c"), c, "c is still the same task");
    // Typed at the start of a linked line, as a keyboard would: still a new task above it.
    edit(&store, &note, "- [ ] c", "- [ ] b2\n- [ ] c");
    assert_eq!(task_id(&store, &list_id, "c"), c);
    assert_eq!(
        list(&store, &list_id),
        rows(&[
            (0, "a", false),
            (0, "b", false),
            (0, "b2", false),
            (0, "c", false)
        ])
    );
}

#[test]
fn reordered_lines_keep_their_tasks() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n- [ ] b\n- [ ] c");
    let ids: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|t| task_id(&store, &list_id, t))
        .collect();
    store
        .set_note_body(&note, "Note\n- [ ] c\n- [ ] a\n- [ ] b", today())
        .unwrap();
    assert_eq!(linked(&store, &note).len(), 3);
    assert_eq!(list(&store, &list_id).len(), 3, "no task made twice");
    for (t, id) in ["a", "b", "c"].iter().zip(&ids) {
        assert_eq!(&task_id(&store, &list_id, t), id);
    }
    // And a moved line still ticks its own task.
    edit(&store, &note, "- [ ] c", "- [x] c");
    assert!(store.task_by_id(&ids[2]).unwrap().completed_at.is_some());
}

#[test]
fn a_line_cut_and_pasted_under_another_moves_its_task() {
    let store = Store::open_in_memory().unwrap();
    let (note, list_id) = make(&store, "Note\n- [ ] a\n  - [ ] x\n- [ ] b");
    let x = task_id(&store, &list_id, "x");
    store
        .set_note_body(&note, "Note\n- [ ] a\n- [ ] b\n  - [ ] x", today())
        .unwrap();
    assert_eq!(task_id(&store, &list_id, "x"), x, "the same task");
    assert_eq!(parent_of(&store, &list_id, "x").as_deref(), Some("b"));
    assert_eq!(list(&store, &list_id).len(), 3);
}

/// Random edits of a linked checklist: the text always ends up as typed, every task has at most
/// one line, and the tasks the note made are never more than its items.
#[test]
fn random_edits_keep_the_note_and_tasks_consistent() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let words = ["milk", "eggs", "tea", "jam", "rye", "oat", "", "salt"];
    for round in 0..40 {
        let store = Store::open_in_memory().unwrap();
        let (note, list_id) = make(&store, SHOPPING);
        for _ in 0..25 {
            let b = body(&store, &note);
            let mut lines: Vec<String> = b.split('\n').map(str::to_owned).collect();
            let at = 1 + next(lines.len().max(2) - 1).min(lines.len() - 1);
            let word = words[next(words.len())];
            let indent = ["", "  ", "    "][next(3)];
            let tick = [" ", "x"][next(2)];
            match next(7) {
                0 => lines.insert(at, format!("{indent}- [{tick}] {word}")),
                1 if lines.len() > 2 => {
                    lines.remove(at);
                }
                2 => lines[at] = lines[at].replacen("[ ]", "[x]", 1),
                3 => lines[at] = format!("  {}", lines[at]),
                4 => lines[at] = lines[at].trim_start().to_owned(),
                5 => lines[at].push_str(word),
                _ if lines.len() > 2 => {
                    let line = lines.remove(at);
                    let to = 1 + next(lines.len() - 1);
                    lines.insert(to, line);
                }
                _ => {}
            }
            let new = lines.join("\n");
            store.set_note_body(&note, &new, today()).unwrap();
            // Unrelated changes catch the note up with its tasks; that may retick or retitle.
            let now = body(&store, &note);
            assert_eq!(
                now.split('\n').count(),
                new.split('\n').count(),
                "round {round}: lines kept"
            );
            let links = store.note_links(&note).unwrap();
            let mut ids: Vec<_> = links.iter().map(|l| l.task_id.clone()).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), links.len(), "round {round}: a task on two lines");
            let items = now.split('\n').filter(|l| l.contains("- [")).count();
            assert!(links.len() <= items);
            // Every linked line shows its task as it is.
            let lines: Vec<&str> = now.split('\n').collect();
            for l in &links {
                let task = store.task_by_id(&l.task_id).unwrap();
                let line = lines[l.line as usize];
                assert_eq!(
                    line.contains("[x]"),
                    task.completed_at.is_some(),
                    "round {round}: {line:?}"
                );
                if !line.trim_end().ends_with(']') {
                    assert!(
                        line.ends_with(&task.title),
                        "round {round}: {line:?} vs {:?}",
                        task.title
                    );
                }
            }
        }
        let _ = list_id;
    }
}
