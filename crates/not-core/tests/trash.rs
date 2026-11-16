//! Trash: listing what was deleted, restoring it, deleting for good, the 30-day purge.

mod common;

use common::Log;
use not_core::date::Date;
use not_core::{EVERY_DAY, INBOX, Store, TRASH_DAYS, TrashKind};

const DAY: i64 = 24 * 60 * 60 * 1000;

fn today() -> Date {
    Date::parse("2026-11-08").unwrap()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn kinds(s: &Store) -> Vec<(TrashKind, String, u32)> {
    s.trash()
        .unwrap()
        .into_iter()
        .map(|i| (i.kind, i.title, i.inside))
        .collect()
}

#[test]
fn a_deleted_task_lists_once_with_its_subtasks_and_comes_back_whole() {
    let s = Store::open_in_memory().unwrap();
    let trip = s.add_task(INBOX, "Trip", today()).unwrap();
    s.add_task(&trip, "Passport", today()).unwrap();
    s.add_task(&trip, "Tickets", today()).unwrap();
    s.add_task(INBOX, "Milk", today()).unwrap();
    assert!(s.trash().unwrap().is_empty());

    s.delete(&trip).unwrap();
    assert_eq!(kinds(&s), [(TrashKind::Task, "Trip".into(), 2)]);
    assert_eq!(s.view_outline(INBOX, today()).unwrap().open.len(), 1);

    s.restore_from_trash(&trip).unwrap();
    assert!(s.trash().unwrap().is_empty());
    let titles: Vec<_> = s
        .view_outline(INBOX, today())
        .unwrap()
        .open
        .into_iter()
        .map(|r| r.task.title)
        .collect();
    assert_eq!(
        titles,
        ["Milk", "Trip", "Passport", "Tickets"],
        "back where it was (new tasks go on top)"
    );
}

#[test]
fn a_deleted_folder_takes_its_notes_along() {
    let s = Store::open_in_memory().unwrap();
    let trips = s.create_folder(None, "Trips").unwrap();
    s.create_note(Some(&trips), "Lisbon\ntram").unwrap();
    s.create_note(Some(&trips), "Rome").unwrap();
    let loose = s.create_note(None, "# Shopping\nmilk").unwrap();
    s.delete_note_or_folder(&trips).unwrap();
    s.delete_note_or_folder(&loose).unwrap();
    let items = kinds(&s);
    assert_eq!(items.len(), 2);
    assert!(items.contains(&(TrashKind::Folder, "Trips".into(), 2)));
    assert!(
        items.contains(&(TrashKind::Note, "Shopping".into(), 0)),
        "titled from the body"
    );

    s.restore_from_trash(&trips).unwrap();
    assert_eq!(s.notes(Some(&trips)).unwrap().len(), 2);
}

#[test]
fn habits_and_workouts_go_to_the_trash_too() {
    let s = Store::open_in_memory().unwrap();
    let run = s.create_habit("Run", 1, EVERY_DAY).unwrap();
    let push = s.create_workout("2026-11-07", "Push").unwrap();
    let bench = s.add_exercise(&push, "Bench", "60kg x 8").unwrap();
    let legs = s.create_workout("2026-11-06", "").unwrap();
    s.delete_habit(&run).unwrap();
    s.delete_exercise(&bench).unwrap();
    s.delete_workout(&legs).unwrap();
    let items = kinds(&s);
    assert!(items.contains(&(TrashKind::Habit, "Run".into(), 0)));
    assert!(items.contains(&(TrashKind::Exercise, "Bench".into(), 0)));
    assert!(items.contains(&(TrashKind::Workout, "Workout · 2026-11-06".into(), 0)));
}

#[test]
fn deleting_for_good_removes_it_and_a_habits_check_ins() {
    let s = Store::open_in_memory().unwrap();
    let run = s.create_habit("Run", 1, EVERY_DAY).unwrap();
    s.log_habit(&run, "2026-11-07", 1).unwrap();
    s.delete_habit(&run).unwrap();
    s.delete_forever(&run).unwrap();
    assert!(s.trash().unwrap().is_empty());
    assert!(s.habits("2026-11-08").unwrap().is_empty());
    let json: serde_json::Value = serde_json::from_str(&s.export_json(today()).unwrap()).unwrap();
    assert!(json["habits"].as_array().unwrap().is_empty());
    assert!(s.restore_from_trash(&run).is_err(), "nothing left to restore");

    // Undo still saves a slip of the finger, until the app closes.
    assert!(s.undo().unwrap());
    assert_eq!(kinds(&s), [(TrashKind::Habit, "Run".into(), 0)]);
}

#[test]
fn only_trashed_items_can_be_restored_or_deleted_for_good() {
    let s = Store::open_in_memory().unwrap();
    let milk = s.add_task(INBOX, "Milk", today()).unwrap();
    assert!(s.restore_from_trash(&milk).is_err(), "not in the trash");
    assert!(
        s.delete_forever(&milk).is_err(),
        "live items aren't deleted for good"
    );
    assert!(s.restore_from_trash("999@1").is_err());
    assert!(s.delete_forever("nope").is_err());
}

#[test]
fn the_trash_empties_itself_after_thirty_days() {
    let s = Store::open_in_memory().unwrap();
    let a = s.add_task(INBOX, "a", today()).unwrap();
    let b = s.add_task(INBOX, "b", today()).unwrap();
    s.delete(&a).unwrap();
    s.delete(&b).unwrap();
    assert_eq!(
        s.purge_trash(now() + (TRASH_DAYS - 1) * DAY).unwrap(),
        0,
        "not yet"
    );
    assert_eq!(s.trash().unwrap().len(), 2);
    assert_eq!(s.purge_trash(now() + (TRASH_DAYS + 1) * DAY).unwrap(), 2);
    assert!(s.trash().unwrap().is_empty());
    assert_eq!(s.purge_trash(now() + 100 * DAY).unwrap(), 0, "idempotent");
}

#[test]
fn emptying_the_trash_is_one_undo_step() {
    let s = Store::open_in_memory().unwrap();
    for t in ["a", "b", "c"] {
        let id = s.add_task(INBOX, t, today()).unwrap();
        s.delete(&id).unwrap();
    }
    assert_eq!(s.empty_trash().unwrap(), 3);
    assert!(s.trash().unwrap().is_empty());
    assert!(s.undo().unwrap());
    assert_eq!(s.trash().unwrap().len(), 3);
}

#[test]
fn trash_changes_sync_and_purging_twice_is_harmless() {
    let mut log = Log::default();
    let phone = Store::open_in_memory().unwrap();
    let mac = Store::open_in_memory().unwrap();
    let keep = phone.add_task(INBOX, "keep", today()).unwrap();
    let gone = phone.add_task(INBOX, "gone", today()).unwrap();
    phone.delete(&keep).unwrap();
    phone.delete(&gone).unwrap();
    log.sync(&phone);
    log.sync(&mac);
    assert_eq!(mac.trash().unwrap().len(), 2);

    mac.restore_from_trash(&keep).unwrap();
    // Both purge offline, at about the same time.
    phone.delete_forever(&gone).unwrap();
    mac.delete_forever(&gone).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    for s in [&phone, &mac] {
        assert!(s.trash().unwrap().is_empty());
        let open: Vec<_> = s
            .view_outline(INBOX, today())
            .unwrap()
            .open
            .into_iter()
            .map(|r| r.task.title)
            .collect();
        assert_eq!(open, ["keep"]);
    }
}
