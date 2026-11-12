//! Checklists in notes made into tasks, shown in the note as a live task block.

mod common;

use common::Log;
use not_core::date::Date;
use not_core::markdown::{TaskSource, task_block_markup, task_blocks};
use not_core::{INBOX, Store, TaskBlock};

fn today() -> Date {
    Date::parse("2026-11-12").unwrap()
}

/// The one block of a note's body.
fn block_of(store: &Store, note: &str) -> (TaskSource, TaskBlock) {
    let body = store.note_body(note).unwrap();
    let blocks = task_blocks(&body);
    assert_eq!(blocks.len(), 1, "one block in {body:?}");
    let source = blocks[0].source.clone();
    let block = store.task_block(&source).unwrap();
    (source, block)
}

/// Rows as (indent, title, done).
fn rows(block: &TaskBlock) -> Vec<(u32, String, bool)> {
    block
        .rows
        .iter()
        .map(|r| (r.depth, r.task.title.clone(), r.task.completed_at.is_some()))
        .collect()
}

/// For comparing with literals.
fn r(rows: &[(u32, &str, bool)]) -> Vec<(u32, String, bool)> {
    rows.iter().map(|&(d, t, done)| (d, t.to_owned(), done)).collect()
}

/// A note "Shopping" with three items (one ticked, one nested), made into a new list.
fn shopping(store: &Store) -> (String, String) {
    let note = store
        .create_note(
            None,
            "Shopping\nbefore\n- [ ] milk\n  - [x] oat\n- [ ] eggs\nafter",
        )
        .unwrap();
    let body = store.note_body(&note).unwrap();
    let moved = store
        .checklist_to_tasks(&note, &body, 15, body.find("eggs").unwrap() as u32, None, "")
        .unwrap();
    (note, moved.list_id)
}

// ---- making the block ----------------------------------------------------------------------

#[test]
fn a_new_list_block_takes_the_items_place() {
    let store = Store::open_in_memory().unwrap();
    let (note, list) = shopping(&store);
    let body = store.note_body(&note).unwrap();
    let markup = task_block_markup(&TaskSource::List(list.clone()));
    assert_eq!(
        body,
        format!("Shopping\nbefore\n{markup}\nafter"),
        "other lines stay, in place"
    );
    let (source, block) = block_of(&store, &note);
    assert_eq!(source, TaskSource::List(list.clone()));
    assert_eq!(block.title, "Shopping");
    assert_eq!(block.list_id, list);
    assert!(!block.gone);
    // Open first (with the ticked subtask under its parent), in note order.
    assert_eq!(
        rows(&block),
        r(&[(0, "milk", false), (1, "oat", true), (0, "eggs", false)])
    );
}

#[test]
fn tasks_added_to_an_existing_list_make_a_block_of_just_them() {
    let store = Store::open_in_memory().unwrap();
    let home = store.create_list("Home").unwrap();
    store.add_task(&home, "already there").unwrap();
    let note = store
        .create_note(None, "Chores\n- [ ] bins\n- [ ] dishes")
        .unwrap();
    let body = store.note_body(&note).unwrap();
    let moved = store
        .checklist_to_tasks(&note, &body, 0, body.len() as u32, Some(&home), "")
        .unwrap();
    assert_eq!(moved.list_id, home);
    let (source, block) = block_of(&store, &note);
    assert!(matches!(source, TaskSource::Tasks(ref ids) if ids.len() == 2));
    assert_eq!(block.list_id, home);
    assert_eq!(block.title, "", "picked tasks have no heading");
    assert_eq!(rows(&block), r(&[(0, "bins", false), (0, "dishes", false)]));
    // They lead the list, ahead of what was there.
    let titles: Vec<_> = store
        .outline(&home)
        .unwrap()
        .open
        .into_iter()
        .map(|r| r.task.title)
        .collect();
    assert_eq!(titles, ["bins", "dishes", "already there"]);
}

#[test]
fn the_inbox_works_as_a_target() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    store
        .checklist_to_tasks(&note, &body, 5, 5, Some(INBOX), "")
        .unwrap();
    let (_, block) = block_of(&store, &note);
    assert_eq!(block.list_id, INBOX);
    assert_eq!(rows(&block), r(&[(0, "tea", false)]));
}

#[test]
fn the_caret_lands_after_the_block_even_at_the_note_end() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    let moved = store
        .checklist_to_tasks(&note, &body, 5, 5, None, "Errands")
        .unwrap();
    let markup = task_block_markup(&TaskSource::List(moved.list_id.clone()));
    assert_eq!(
        moved.body,
        format!("Day\n{markup}\n"),
        "a line to type on after the block"
    );
    assert_eq!(moved.caret as usize, moved.body.encode_utf16().count());
    let list = store
        .lists()
        .unwrap()
        .into_iter()
        .find(|l| l.id == moved.list_id)
        .unwrap();
    assert_eq!(list.title, "Errands", "the title given wins over the suggestion");
}

#[test]
fn items_apart_are_gathered_into_one_block() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "N\n- [ ] a\ntext\n- [ ] b").unwrap();
    let body = store.note_body(&note).unwrap();
    let moved = store
        .checklist_to_tasks(&note, &body, 0, body.len() as u32, None, "")
        .unwrap();
    let markup = task_block_markup(&TaskSource::List(moved.list_id.clone()));
    assert_eq!(moved.body, format!("N\n{markup}\ntext"));
    let (_, block) = block_of(&store, &note);
    assert_eq!(rows(&block), r(&[(0, "a", false), (0, "b", false)]));
}

#[test]
fn nothing_to_make_is_refused_without_changes() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "N\nplain\n- [ ] ").unwrap();
    let body = store.note_body(&note).unwrap();
    assert!(
        store
            .checklist_to_tasks(&note, &body, 0, body.len() as u32, None, "")
            .is_err()
    );
    assert_eq!(store.note_body(&note).unwrap(), body);
    assert!(store.lists().unwrap().iter().all(|l| l.title != "N"));
}

#[test]
fn one_undo_restores_the_checklist_and_takes_the_tasks_away() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Day").unwrap();
    // Saves just before, as an editor makes them while typing.
    store.set_note_body(&note, "Day\n- [ ] ").unwrap();
    store.set_note_body(&note, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    let moved = store.checklist_to_tasks(&note, &body, 5, 5, None, "").unwrap();
    store.undo().unwrap();
    assert_eq!(store.note_body(&note).unwrap(), "Day\n- [ ] tea");
    assert!(store.lists().unwrap().iter().all(|l| l.id != moved.list_id));
    store.redo().unwrap();
    assert_eq!(store.note_body(&note).unwrap(), moved.body);
}

// ---- the block follows the tasks -----------------------------------------------------------

#[test]
fn ticks_renames_and_reopens_show_in_the_block() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = shopping(&store);
    let (_, block) = block_of(&store, &note);
    let milk = block.rows[0].task.id.clone();
    let oat = block.rows[1].task.id.clone();

    store.set_completed(&milk, true, today()).unwrap();
    store.rename(&oat, "oat milk").unwrap();
    // A done task moves to the completed part, with its subtasks.
    assert_eq!(
        rows(&block_of(&store, &note).1),
        r(&[(0, "eggs", false), (0, "milk", true), (1, "oat milk", true)])
    );
    store.set_completed(&milk, false, today()).unwrap();
    assert_eq!(
        rows(&block_of(&store, &note).1),
        r(&[(0, "milk", false), (1, "oat milk", true), (0, "eggs", false)])
    );
}

#[test]
fn subtasks_added_anywhere_show_in_the_block() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = shopping(&store);
    let eggs = block_of(&store, &note).1.rows[2].task.id.clone();
    let free_range = store.add_task(&eggs, "free range").unwrap();
    store.add_task(&free_range, "from the market").unwrap();
    assert_eq!(
        rows(&block_of(&store, &note).1),
        r(&[
            (0, "milk", false),
            (1, "oat", true),
            (0, "eggs", false),
            (1, "free range", false),
            (2, "from the market", false)
        ])
    );
}

#[test]
fn a_list_block_shows_tasks_added_to_the_list_later() {
    let store = Store::open_in_memory().unwrap();
    let (note, list) = shopping(&store);
    store.add_task(&list, "bread").unwrap();
    let titles: Vec<_> = rows(&block_of(&store, &note).1)
        .into_iter()
        .map(|r| r.1)
        .collect::<Vec<_>>();
    assert_eq!(
        titles,
        ["bread", "milk", "oat", "eggs"],
        "new tasks go first in a list"
    );
}

#[test]
fn a_picked_block_ignores_other_tasks_of_the_list() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    store
        .checklist_to_tasks(&note, &body, 5, 5, Some(INBOX), "")
        .unwrap();
    store.add_task(INBOX, "unrelated").unwrap();
    assert_eq!(rows(&block_of(&store, &note).1), r(&[(0, "tea", false)]));
}

#[test]
fn reordering_and_moving_tasks_show_in_the_block() {
    let store = Store::open_in_memory().unwrap();
    let (note, list) = shopping(&store);
    let block = block_of(&store, &note).1;
    let (milk, eggs) = (block.rows[0].task.id.clone(), block.rows[2].task.id.clone());
    store.move_task_before(&eggs, &list, Some(&milk)).unwrap();
    assert_eq!(
        rows(&block_of(&store, &note).1),
        r(&[(0, "eggs", false), (0, "milk", false), (1, "oat", true)])
    );
    // Made a subtask of milk.
    store.move_task(&eggs, &milk, 0).unwrap();
    let got = rows(&block_of(&store, &note).1);
    assert_eq!(got[0], (0, "milk".into(), false));
    assert!(got.contains(&(1, "eggs".into(), false)));
}

#[test]
fn a_picked_task_moved_to_another_list_still_shows() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Day\n- [ ] tea\n- [ ] cake").unwrap();
    let body = store.note_body(&note).unwrap();
    store
        .checklist_to_tasks(&note, &body, 0, body.len() as u32, Some(INBOX), "")
        .unwrap();
    let tea = block_of(&store, &note).1.rows[0].task.id.clone();
    let work = store.create_list("Work").unwrap();
    store.move_task(&tea, &work, 0).unwrap();
    let mut titles: Vec<_> = rows(&block_of(&store, &note).1)
        .into_iter()
        .map(|r| r.1)
        .collect();
    titles.sort();
    assert_eq!(titles, ["cake", "tea"]);
}

#[test]
fn deleted_tasks_leave_the_block_and_come_back_with_undo() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = shopping(&store);
    let milk = block_of(&store, &note).1.rows[0].task.id.clone();
    store.delete(&milk).unwrap();
    assert_eq!(
        rows(&block_of(&store, &note).1),
        r(&[(0, "eggs", false)]),
        "with its subtask"
    );
    store.undo().unwrap();
    assert_eq!(block_of(&store, &note).1.rows.len(), 3);
}

#[test]
fn a_deleted_list_or_all_picked_tasks_gone_leave_a_gone_block() {
    let store = Store::open_in_memory().unwrap();
    let (note, list) = shopping(&store);
    store.delete(&list).unwrap();
    let block = block_of(&store, &note).1;
    assert!(block.gone && block.rows.is_empty());

    let note = store.create_note(None, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    store
        .checklist_to_tasks(&note, &body, 5, 5, Some(INBOX), "")
        .unwrap();
    let tea = block_of(&store, &note).1.rows[0].task.id.clone();
    store.delete(&tea).unwrap();
    assert!(block_of(&store, &note).1.gone);
}

#[test]
fn a_renamed_list_renames_the_block() {
    let store = Store::open_in_memory().unwrap();
    let (note, list) = shopping(&store);
    store.rename(&list, "Market").unwrap();
    assert_eq!(block_of(&store, &note).1.title, "Market");
}

#[test]
fn completed_tasks_older_than_a_day_drop_out_like_in_the_list() {
    let store = Store::open_in_memory().unwrap();
    let (note, list) = shopping(&store);
    // The block uses the list's outline, so it shows what the list shows.
    let outline = store.outline(&list).unwrap();
    let in_list = outline.open.len() + outline.completed.len();
    assert_eq!(block_of(&store, &note).1.rows.len(), in_list);
}

// ---- the note around the block -------------------------------------------------------------

#[test]
fn editing_the_note_keeps_the_block() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = shopping(&store);
    let body = store.note_body(&note).unwrap();
    store
        .set_note_body(
            &note,
            &format!("{}\nmore below", body.replace("before", "earlier")),
        )
        .unwrap();
    assert_eq!(block_of(&store, &note).1.rows.len(), 3);
}

#[test]
fn removing_the_line_removes_the_block_but_not_the_tasks() {
    let store = Store::open_in_memory().unwrap();
    let (note, list) = shopping(&store);
    store.set_note_body(&note, "Shopping\nbefore\nafter").unwrap();
    assert!(task_blocks(&store.note_body(&note).unwrap()).is_empty());
    assert_eq!(
        store.outline(&list).unwrap().open.len(),
        3,
        "milk, oat, eggs stay"
    );
}

#[test]
fn previews_and_titles_skip_the_block_line() {
    let store = Store::open_in_memory().unwrap();
    let (note, _) = shopping(&store);
    let summary = store
        .notes(None)
        .unwrap()
        .into_iter()
        .find(|n| n.id == note)
        .unwrap();
    assert_eq!(
        (summary.title.as_str(), summary.preview.as_str()),
        ("Shopping", "before after")
    );
    let only = store
        .create_note(None, &task_block_markup(&TaskSource::List("1@2".into())))
        .unwrap();
    let summary = store
        .notes(None)
        .unwrap()
        .into_iter()
        .find(|n| n.id == only)
        .unwrap();
    assert_eq!(summary.title, "", "a note of just a block has no title line");
}

#[test]
fn checklists_made_with_per_line_links_read_as_plain_checklists() {
    let store = Store::open_in_memory().unwrap();
    let note = store
        .create_note(
            None,
            "Old\n- [x] tea <!--task:5@9-->\n- [ ] cake <!--task:6@9--> later",
        )
        .unwrap();
    assert_eq!(
        store.note_body(&note).unwrap(),
        "Old\n- [x] tea\n- [ ] cake later"
    );
    let summary = store
        .notes(None)
        .unwrap()
        .into_iter()
        .find(|n| n.id == note)
        .unwrap();
    assert_eq!(summary.preview, "tea cake later");
    // And can be made into tasks again.
    let body = store.note_body(&note).unwrap();
    assert!(
        store
            .checklist_to_tasks(&note, &body, 0, body.len() as u32, None, "")
            .is_ok()
    );
}

// ---- other devices ---------------------------------------------------------------------------

#[test]
fn the_block_and_its_tasks_sync_and_follow_edits_from_another_device() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let (note, _) = shopping(&mac);
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(rows(&block_of(&phone, &note).1), rows(&block_of(&mac, &note).1));

    // Ticked and given a subtask on the phone: the Mac's block shows both.
    let milk = block_of(&phone, &note).1.rows[0].task.id.clone();
    phone.add_task(&milk, "lactose free").unwrap();
    let eggs = block_of(&phone, &note)
        .1
        .rows
        .iter()
        .find(|r| r.task.title == "eggs")
        .unwrap()
        .task
        .id
        .clone();
    phone.set_completed(&eggs, true, today()).unwrap();
    log.sync(&phone);
    log.sync(&mac);
    let got = rows(&block_of(&mac, &note).1);
    assert!(got.contains(&(1, "lactose free".into(), false)));
    assert!(got.contains(&(0, "eggs".into(), true)));
    assert_eq!(rows(&block_of(&mac, &note).1), rows(&block_of(&phone, &note).1));
}

#[test]
fn a_block_whose_tasks_havent_arrived_yet_reads_as_gone_then_fills_in() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let (note, _) = shopping(&mac);
    log.sync(&mac);
    log.sync(&phone);
    // A block pointing at tasks the phone doesn't know: gone, not an error.
    let unknown = phone
        .task_block(&TaskSource::Tasks(vec!["99@99".into()]))
        .unwrap();
    assert!(unknown.gone);
    let unknown = phone.task_block(&TaskSource::List("99@99".into())).unwrap();
    assert!(unknown.gone);
    assert!(!block_of(&phone, &note).1.gone);
}

#[test]
fn tasks_added_from_a_block_show_in_it() {
    let store = Store::open_in_memory().unwrap();
    // A whole list: the source stays.
    let (note, list) = shopping(&store);
    let (source, _) = block_of(&store, &note);
    let (_, same) = store.add_task_from_block(&source, "bread").unwrap();
    assert_eq!(same, source);
    assert_eq!(
        rows(&store.task_block(&same).unwrap())[0],
        (0, "bread".into(), false)
    );
    assert_eq!(store.outline(&list).unwrap().open[0].task.title, "bread");

    // Picked tasks: the new one joins them, in the list they're in.
    let note = store.create_note(None, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    store
        .checklist_to_tasks(&note, &body, 5, 5, Some(INBOX), "")
        .unwrap();
    let (source, _) = block_of(&store, &note);
    let (id, grown) = store.add_task_from_block(&source, "cake").unwrap();
    assert!(matches!(&grown, TaskSource::Tasks(ids) if ids.contains(&id) && ids.len() == 2));
    assert_eq!(store.task_by_id(&id).unwrap().list_id, INBOX);
    let titles: Vec<_> = rows(&store.task_block(&grown).unwrap())
        .into_iter()
        .map(|r| r.1)
        .collect();
    assert_eq!(titles, ["cake", "tea"]);
    assert!(
        store.add_task_from_block(&source, "  ").is_err(),
        "an empty title is refused"
    );
    assert!(
        store
            .add_task_from_block(&TaskSource::List("9@9".into()), "x")
            .is_err(),
        "gone"
    );
}
