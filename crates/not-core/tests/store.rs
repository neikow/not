use not_core::{Credentials, INBOX, Store, Task, TaskSort};
mod common;

use common::Log;

fn today() -> not_core::date::Date {
    not_core::date::Date::parse("2026-11-08").unwrap()
}

fn titles(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.title.as_str()).collect()
}

fn list_titles(store: &Store) -> Vec<String> {
    store.lists().unwrap().into_iter().map(|l| l.title).collect()
}

fn credentials(server: &str) -> Credentials {
    Credentials {
        server_url: server.into(),
        token: "secret".into(),
        device_id: 1,
        device_name: "Mac".into(),
    }
}

#[test]
fn read_model_follows_commands() {
    let s = Store::open_in_memory().unwrap();
    let home = s.create_list("Home").unwrap();
    let work = s.create_list("Work").unwrap();
    // New tasks go first.
    let b = s.add_task(&home, "b").unwrap();
    let a = s.add_task(&home, "a").unwrap();
    let sub = s.add_task(&a, "a.1").unwrap();

    assert_eq!(list_titles(&s), ["Inbox", "Home", "Work"]);
    let tasks = s.tasks(&home).unwrap();
    assert_eq!(titles(&tasks), ["a", "b", "a.1"]);
    assert_eq!(tasks[2].parent_id.as_deref(), Some(a.as_str()));

    s.move_task(&b, &home, 0).unwrap();
    assert_eq!(titles(&s.tasks(&home).unwrap()), ["b", "a", "a.1"]);

    // Moving a parent carries its subtree to the new list.
    s.move_task(&a, &work, 0).unwrap();
    assert_eq!(titles(&s.tasks(&home).unwrap()), ["b"]);
    let moved = s.tasks(&work).unwrap();
    assert_eq!(titles(&moved), ["a", "a.1"]);
    assert!(moved.iter().all(|t| t.list_id == work));

    s.set_completed(&sub, true, today()).unwrap();
    assert!(s.tasks(&work).unwrap()[1].completed_at.is_some());
    s.set_completed(&sub, false, today()).unwrap();
    assert!(s.tasks(&work).unwrap()[1].completed_at.is_none());

    s.delete(&work).unwrap();
    assert!(s.tasks(&work).unwrap().is_empty());
    assert_eq!(list_titles(&s), ["Inbox", "Home"]);
}

#[test]
fn rejects_invalid_targets() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Home").unwrap();
    assert!(
        s.set_completed(&list, true, today()).is_err(),
        "lists can't be completed"
    );
    assert!(s.add_task("nope", "x").is_err());
    assert!(s.add_task(&list, "   ").is_err());
    assert!(s.rename(INBOX, "Mine").is_err());
    assert!(s.delete(INBOX).is_err());
    assert!(s.move_list(INBOX, 1).is_err());
}

#[test]
fn inbox_is_built_in() {
    let s = Store::open_in_memory().unwrap();
    assert_eq!(list_titles(&s), ["Inbox"], "exists without being created");

    let b = s.add_task(INBOX, "b").unwrap();
    let home = s.create_list("Home").unwrap();
    let a = s.add_task(INBOX, "a").unwrap();
    let sub = s.add_task(&a, "a.1").unwrap();
    let work = s.create_list("Work").unwrap();

    // Inbox tasks and lists share the tree root, but neither shows up as the other.
    assert_eq!(list_titles(&s), ["Inbox", "Home", "Work"]);
    let tasks = s.tasks(INBOX).unwrap();
    assert_eq!(titles(&tasks), ["a", "b", "a.1"]);
    assert!(tasks.iter().all(|t| t.list_id == INBOX));
    assert_eq!(tasks[2].parent_id.as_deref(), Some(a.as_str()));

    // Indexes count siblings of the same kind only.
    s.move_task(&b, INBOX, 0).unwrap();
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["b", "a", "a.1"]);
    s.move_task(&b, INBOX, 1).unwrap();
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["a", "b", "a.1"]);
    s.move_list(&work, 0).unwrap();
    assert_eq!(list_titles(&s), ["Inbox", "Work", "Home"]);
    s.move_list(&work, 5).unwrap();
    assert_eq!(list_titles(&s), ["Inbox", "Home", "Work"]);

    // Out of the Inbox and back, subtree included.
    s.move_task(&a, &home, 0).unwrap();
    assert_eq!(titles(&s.tasks(&home).unwrap()), ["a", "a.1"]);
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["b"]);
    s.move_task(&a, INBOX, 0).unwrap();
    assert!(s.tasks(&home).unwrap().is_empty());
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["a", "b", "a.1"]);

    s.delete(&sub).unwrap();
    s.delete(&home).unwrap();
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["a", "b"]);
}

#[test]
fn two_fresh_devices_share_one_inbox() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    mac.add_task(INBOX, "from mac").unwrap();
    phone.add_task(INBOX, "from phone").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    for s in [&mac, &phone] {
        assert_eq!(list_titles(s), ["Inbox"]);
    }
    // Concurrent appends land in some order, the same on both devices.
    let tasks = mac.tasks(INBOX).unwrap();
    assert_eq!(tasks, phone.tasks(INBOX).unwrap());
    let mut inbox = titles(&tasks);
    inbox.sort();
    assert_eq!(inbox, ["from mac", "from phone"]);
}

#[test]
fn state_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("not-core-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("reopen.db");
    let _ = std::fs::remove_file(&path);

    let list = {
        let s = Store::open(&path).unwrap();
        let list = s.create_list("Home").unwrap();
        s.add_task(&list, "persisted").unwrap();
        s.set_credentials(&credentials("https://a.example")).unwrap();
        list
    };
    let s = Store::open(&path).unwrap();
    assert_eq!(s.credentials().unwrap(), Some(credentials("https://a.example")));
    assert_eq!(titles(&s.tasks(&list).unwrap()), ["persisted"]);
    // Edits after reopen use a new peer id and still persist.
    s.add_task(&list, "second").unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(titles(&s.tasks(&list).unwrap()), ["second", "persisted"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn offline_edits_converge() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let milk = mac.add_task(INBOX, "milk").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(titles(&phone.tasks(INBOX).unwrap()), ["milk"]);

    // Both offline: mac renames, phone completes and adds a task.
    mac.rename(&milk, "oat milk").unwrap();
    phone.set_completed(&milk, true, today()).unwrap();
    phone.add_task(INBOX, "bread").unwrap();

    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    let on_mac = mac.tasks(INBOX).unwrap();
    assert_eq!(on_mac, phone.tasks(INBOX).unwrap());
    assert_eq!(titles(&on_mac), ["bread", "oat milk"]);
    assert!(
        on_mac[1].completed_at.is_some(),
        "per-field merge keeps both edits"
    );
}

#[test]
fn concurrent_moves_never_duplicate() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let a = mac.create_list("A").unwrap();
    let b = mac.create_list("B").unwrap();
    let c = mac.create_list("C").unwrap();
    let task = mac.add_task(&a, "wander").unwrap();
    log.sync(&mac);
    log.sync(&phone);

    mac.move_task(&task, &b, 0).unwrap();
    phone.move_task(&task, &c, 0).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    let count = |s: &Store| {
        [&a, &b, &c]
            .iter()
            .map(|l| s.tasks(l).unwrap().len())
            .sum::<usize>()
    };
    assert_eq!(count(&mac), 1);
    assert_eq!(count(&phone), 1);
    for l in [&a, &b, &c] {
        assert_eq!(mac.tasks(l).unwrap(), phone.tasks(l).unwrap());
    }
}

#[test]
fn deleting_a_list_while_other_device_adds_to_it() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let list = mac.create_list("Trip").unwrap();
    log.sync(&mac);
    log.sync(&phone);

    mac.delete(&list).unwrap();
    phone.add_task(&list, "passport").unwrap();
    log.sync(&mac);
    log.sync(&phone);

    assert_eq!(list_titles(&mac), ["Inbox"]);
    assert_eq!(list_titles(&phone), ["Inbox"]);
    assert!(phone.tasks(&list).unwrap().is_empty());
}

#[test]
fn pulled_ops_are_not_pushed_back() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let list = mac.create_list("Home").unwrap();
    mac.add_task(&list, "x").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert!(phone.outgoing().unwrap().is_none(), "phone only has server ops");
    log.sync(&mac);
    assert!(
        mac.outgoing().unwrap().is_none(),
        "own ops echoed back are still on the server"
    );
}

#[test]
fn new_pairing_resends_everything() {
    let mut old_server = Log::default();
    let mac = Store::open_in_memory().unwrap();
    mac.set_credentials(&credentials("https://old.example")).unwrap();
    mac.add_task(INBOX, "milk").unwrap();
    old_server.sync(&mac);
    assert!(mac.outgoing().unwrap().is_none());

    // Pairing again (new or reset server): the cursor and server version start over.
    mac.set_credentials(&credentials("https://new.example")).unwrap();
    assert_eq!(mac.pull_cursor().unwrap(), 0);
    let mut new_server = Log::default();
    new_server.sync(&mac);
    let phone = Store::open_in_memory().unwrap();
    new_server.sync(&phone);
    assert_eq!(titles(&phone.tasks(INBOX).unwrap()), ["milk"]);

    mac.clear_credentials().unwrap();
    assert_eq!(mac.credentials().unwrap(), None);
}

#[test]
fn listeners_see_change_origin() {
    use std::sync::{Arc, Mutex};

    use not_core::ChangeOrigin;

    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let id = phone.add_listener({
        let seen = seen.clone();
        move |origin| seen.lock().unwrap().push(origin)
    });

    let list = phone.create_list("Home").unwrap();
    mac.create_list("Work").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(*seen.lock().unwrap(), [ChangeOrigin::Local, ChangeOrigin::Remote]);

    // Rejected commands and empty pulls don't notify.
    assert!(phone.add_task(&list, "  ").is_err());
    log.sync(&phone);
    phone.remove_listener(id);
    phone.add_task(&list, "x").unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn undo_restores_a_deleted_subtree() {
    let s = Store::open_in_memory().unwrap();
    let parent = s.add_task(INBOX, "trip").unwrap();
    let child = s.add_task(&parent, "passport").unwrap();
    s.set_notes(&parent, "check visa").unwrap();
    assert!(!s.redo().unwrap(), "nothing to redo yet");

    s.delete(&parent).unwrap();
    assert!(s.tasks(INBOX).unwrap().is_empty());

    assert!(s.undo().unwrap());
    let tasks = s.tasks(INBOX).unwrap();
    assert_eq!(titles(&tasks), ["trip", "passport"]);
    assert_eq!(
        tasks[0].id, parent,
        "same identity, so other devices' edits still apply"
    );
    assert_eq!(tasks[1].id, child);
    assert_eq!(tasks[0].notes, "check visa");

    assert!(s.redo().unwrap());
    assert!(s.tasks(INBOX).unwrap().is_empty());
}

#[test]
fn each_command_is_one_undo_step() {
    let s = Store::open_in_memory().unwrap();
    let milk = s.add_task(INBOX, "milk").unwrap();
    s.rename(&milk, "oat milk").unwrap();
    s.set_completed(&milk, true, today()).unwrap();

    s.undo().unwrap();
    assert!(s.tasks(INBOX).unwrap()[0].completed_at.is_none());
    s.undo().unwrap();
    assert_eq!(s.tasks(INBOX).unwrap()[0].title, "milk");
    s.undo().unwrap();
    assert!(s.tasks(INBOX).unwrap().is_empty());
    assert!(!s.can_undo());
    assert!(s.can_redo());
}

#[test]
fn undo_never_reverts_other_devices() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let milk = mac.add_task(INBOX, "milk").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert!(!phone.can_undo(), "pulled changes aren't the phone's to undo");

    phone.rename(&milk, "oat milk").unwrap();
    mac.set_completed(&milk, true, today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert!(phone.undo().unwrap());
    let task = &phone.tasks(INBOX).unwrap()[0];
    assert_eq!(task.title, "milk", "the phone's rename is undone");
    assert!(task.completed_at.is_some(), "the mac's completion stays");
}

#[test]
fn concurrent_notes_merge() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let task = mac.add_task(INBOX, "trip").unwrap();
    log.sync(&mac);
    log.sync(&phone);

    // Both devices write the first notes for this task while offline.
    mac.set_notes(&task, "passport").unwrap();
    phone.set_notes(&task, "tickets").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    let notes = mac.tasks(INBOX).unwrap()[0].notes.clone();
    assert_eq!(notes, phone.tasks(INBOX).unwrap()[0].notes);
    assert!(
        notes.contains("passport") && notes.contains("tickets"),
        "{notes:?}"
    );

    // Later edits are character-level, so both survive too.
    mac.set_notes(&task, &format!("{notes}\nvisa")).unwrap();
    phone.set_notes(&task, &format!("hotel\n{notes}")).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    let merged = mac.tasks(INBOX).unwrap()[0].notes.clone();
    assert_eq!(merged, phone.tasks(INBOX).unwrap()[0].notes);
    assert!(
        merged.starts_with("hotel\n") && merged.ends_with("\nvisa"),
        "{merged:?}"
    );
}

#[test]
fn outline_puts_completed_tasks_last() {
    let s = Store::open_in_memory().unwrap();
    let a = s.add_task(INBOX, "a").unwrap();
    s.add_task(&a, "a.1").unwrap();
    let a2 = s.add_task(&a, "a.2").unwrap();
    let b = s.add_task(INBOX, "b").unwrap();
    let c = s.add_task(INBOX, "c").unwrap();
    s.add_task(&c, "c.1").unwrap();
    s.set_completed(&a2, true, today()).unwrap();
    s.set_completed(&c, true, today()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    s.set_completed(&b, true, today()).unwrap();

    let outline = s.outline(INBOX).unwrap();
    let view = |rows: &[not_core::OutlineRow]| -> Vec<(String, u32, bool)> {
        rows.iter()
            .map(|r| (r.task.title.clone(), r.depth, r.done))
            .collect()
    };
    assert_eq!(
        view(&outline.open),
        [
            ("a".into(), 0, false),
            ("a.1".into(), 1, false),
            ("a.2".into(), 1, true)
        ]
    );
    assert_eq!(
        view(&outline.completed),
        [
            ("b".into(), 0, true),
            ("c".into(), 0, true),
            ("c.1".into(), 1, true)
        ],
        "most recently completed first, subtasks of completed tasks count as done"
    );

    let counts: Vec<u32> = s.lists().unwrap().iter().map(|l| l.open_count).collect();
    assert_eq!(counts, [1], "only open top-level tasks count");
}

#[test]
fn move_before_anchors_on_a_sibling() {
    let s = Store::open_in_memory().unwrap();
    let c = s.add_task(INBOX, "c").unwrap();
    let b = s.add_task(INBOX, "b").unwrap();
    let a = s.add_task(INBOX, "a").unwrap();
    s.set_completed(&b, true, today()).unwrap();

    // Dragging c above a in the open section; b (completed) is shown elsewhere.
    s.move_task_before(&c, INBOX, Some(&a)).unwrap();
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["c", "a", "b"]);
    s.move_task_before(&c, INBOX, None).unwrap();
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["a", "b", "c"]);

    let sub = s.add_task(&a, "a.1").unwrap();
    assert!(
        s.move_task_before(&c, INBOX, Some(&sub)).is_err(),
        "anchor under another parent"
    );
}

// ---- schedules -------------------------------------------------------------------------------

fn day(s: &str) -> not_core::date::Date {
    not_core::date::Date::parse(s).unwrap()
}

#[test]
fn quick_add_applies_what_it_parses() {
    let s = Store::open_in_memory().unwrap();
    let groceries = s.create_list("Groceries").unwrap();
    let id = s
        .quick_add(INBOX, "Buy milk tomorrow 9am #gro !1", today(), &[], None)
        .unwrap();
    let task = &s.tasks(&groceries).unwrap()[0];
    assert_eq!(task.id, id);
    assert_eq!(task.title, "Buy milk");
    assert_eq!(task.due.as_deref(), Some("2026-11-09"));
    assert_eq!(task.due_time, Some(9 * 60));
    assert_eq!(task.priority, 3);
    assert_eq!(task.repeat, None);

    // Dismissed tokens stay in the title; a title of only tokens keeps its words.
    s.quick_add(INBOX, "Read tomorrow", today(), &["tomorrow".into()], None)
        .unwrap();
    s.quick_add(INBOX, "tomorrow", today(), &[], None).unwrap();
    let inbox = s.tasks(INBOX).unwrap();
    assert_eq!(titles(&inbox), ["tomorrow", "Read tomorrow"]);
    assert_eq!(inbox[0].due.as_deref(), Some("2026-11-09"));
    assert_eq!(inbox[1].due, None);

    // One undo step takes the whole quick add back.
    assert!(s.undo().unwrap());
    assert_eq!(titles(&s.tasks(INBOX).unwrap()), ["Read tomorrow"]);
}

#[test]
fn schedules_and_priorities_can_be_changed_and_cleared() {
    let s = Store::open_in_memory().unwrap();
    let id = s.add_task(INBOX, "Pay rent").unwrap();
    let parsed = s.parse_task("monthly on the 1st 10:00", today(), &[]).unwrap();
    s.set_schedule(&id, &parsed.schedule).unwrap();
    s.set_priority(&id, 2).unwrap();
    let task = &s.tasks(INBOX).unwrap()[0];
    assert_eq!(
        (task.due.as_deref(), task.due_time, task.repeat.as_deref()),
        (Some("2026-12-01"), Some(600), Some("FREQ=MONTHLY"))
    );
    assert_eq!(task.priority, 2);

    s.set_schedule(&id, &not_core::Schedule::default()).unwrap();
    s.set_priority(&id, 0).unwrap();
    let task = &s.tasks(INBOX).unwrap()[0];
    assert_eq!(
        (
            task.due.as_deref(),
            task.due_time,
            task.repeat.as_deref(),
            task.priority
        ),
        (None, None, None, 0)
    );
    assert!(s.set_priority(&id, 4).is_err());
}

#[test]
fn completing_a_repeating_task_moves_it_on() {
    let s = Store::open_in_memory().unwrap();
    let id = s
        .quick_add(INBOX, "Water plants every 2 days", day("2026-11-01"), &[], None)
        .unwrap();
    let sub = s.add_task(&id, "Balcony").unwrap();
    s.set_notes(&id, "Use the green can").unwrap();
    s.set_completed(&sub, true, today()).unwrap();

    // Due 1 Nov, completed late on 8 Nov: the next one is after today, on the 2-day rhythm.
    s.set_completed(&id, true, today()).unwrap();
    let outline = s.outline(INBOX).unwrap();
    let open: Vec<_> = outline.open.iter().map(|r| &r.task).collect();
    assert_eq!(open[0].title, "Water plants");
    assert_eq!(open[0].due.as_deref(), Some("2026-11-09"));
    assert_eq!(open[0].completed_at, None);
    assert_eq!(open[1].title, "Balcony");
    assert_eq!(open[1].completed_at, None, "subtasks start over");

    // This occurrence is kept as a completed copy, without the repeat.
    let done = &outline.completed[0].task;
    assert_eq!(
        (done.title.as_str(), done.due.as_deref(), done.repeat.as_deref()),
        ("Water plants", Some("2026-11-01"), None)
    );
    assert_eq!(done.notes, "Use the green can");
    assert!(done.completed_at.is_some());

    assert!(s.undo().unwrap());
    let open = s.outline(INBOX).unwrap();
    assert_eq!(open.completed.len(), 0);
    assert_eq!(open.open[0].task.due.as_deref(), Some("2026-11-01"));
}

#[test]
fn due_tasks_span_lists() {
    let s = Store::open_in_memory().unwrap();
    let work = s.create_list("Work").unwrap();
    s.quick_add(INBOX, "later next week", today(), &[], None).unwrap();
    s.quick_add(&work, "overdue 3/11", day("2026-11-01"), &[], None)
        .unwrap();
    s.quick_add(INBOX, "low today !3", today(), &[], None).unwrap();
    s.quick_add(INBOX, "high today !1", today(), &[], None).unwrap();
    s.quick_add(&work, "early today at 8", today(), &[], None)
        .unwrap();
    let done = s.quick_add(INBOX, "done today", today(), &[], None).unwrap();
    s.set_completed(&done, true, today()).unwrap();
    let parent = s.quick_add(INBOX, "parent", today(), &[], None).unwrap();
    let child = s.add_task(&parent, "child").unwrap();
    s.set_schedule(&child, &s.parse_task("today", today(), &[]).unwrap().schedule)
        .unwrap();
    s.set_completed(&parent, true, today()).unwrap();

    let due = s.due_tasks(today()).unwrap();
    assert_eq!(titles(&due), ["overdue", "early", "high", "low"]);
}

#[test]
fn concurrent_schedule_edits_merge() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let id = mac.quick_add(INBOX, "Dentist", today(), &[], None).unwrap();
    log.sync(&mac);
    log.sync(&phone);

    phone.set_priority(&id, 3).unwrap();
    mac.set_schedule(&id, &mac.parse_task("fri 10am", today(), &[]).unwrap().schedule)
        .unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    for s in [&mac, &phone] {
        let task = &s.tasks(INBOX).unwrap()[0];
        assert_eq!(
            (task.due.as_deref(), task.due_time, task.priority),
            (Some("2026-11-13"), Some(600), 3)
        );
    }
}

#[test]
fn indent_and_outdent_tasks() {
    let s = Store::open_in_memory().unwrap();
    let b = s.add_task(INBOX, "b").unwrap();
    let done = s.add_task(INBOX, "done").unwrap();
    let a = s.add_task(INBOX, "a").unwrap();
    s.set_completed(&done, true, today()).unwrap();

    assert!(!s.indent_task(&a).unwrap());
    // Skips the completed task above, which isn't shown among open ones.
    assert!(s.indent_task(&b).unwrap());
    let outline = s.outline(INBOX).unwrap();
    let open: Vec<_> = outline
        .open
        .iter()
        .map(|r| (r.task.title.as_str(), r.depth))
        .collect();
    assert_eq!(open, [("a", 0), ("b", 1)]);

    assert!(s.outdent_task(&b).unwrap());
    assert!(!s.outdent_task(&b).unwrap());
    let open: Vec<_> = s
        .outline(INBOX)
        .unwrap()
        .open
        .iter()
        .map(|r| r.task.title.clone())
        .collect();
    assert_eq!(open, ["a", "b"]);
}

#[test]
fn new_tasks_go_first_and_subtasks_last() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Home").unwrap();
    let a = s.add_task(&list, "a").unwrap();
    s.quick_add(&list, "b", today(), &[], None).unwrap();
    s.add_task(&a, "a.1").unwrap();
    s.add_task(&a, "a.2").unwrap();
    assert_eq!(titles(&s.tasks(&list).unwrap()), ["b", "a", "a.1", "a.2"]);
}

#[test]
fn lists_sort_their_open_tasks() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Home").unwrap();
    for input in [
        "walk dog",
        "Call mum fri !1",
        "bins tomorrow",
        "Taxes tomorrow 9am !3",
    ] {
        s.quick_add(&list, input, today(), &[], None).unwrap();
    }
    let a = s
        .tasks(&list)
        .unwrap()
        .into_iter()
        .find(|t| t.title == "walk dog")
        .unwrap();
    s.add_task(&a.id, "z leash").unwrap();
    s.add_task(&a.id, "a treats").unwrap();
    let open = |s: &Store| -> Vec<String> {
        s.outline(&list)
            .unwrap()
            .open
            .into_iter()
            .map(|r| r.task.title)
            .collect()
    };

    assert_eq!(s.lists().unwrap()[1].sort, TaskSort::Manual);
    assert_eq!(
        open(&s),
        ["Taxes", "bins", "Call mum", "walk dog", "z leash", "a treats"]
    );

    s.set_list_sort(&list, TaskSort::Due).unwrap();
    assert_eq!(s.lists().unwrap()[1].sort, TaskSort::Due);
    assert_eq!(
        open(&s),
        ["Taxes", "bins", "Call mum", "walk dog", "z leash", "a treats"]
    );
    s.set_list_sort(&list, TaskSort::Priority).unwrap();
    assert_eq!(
        open(&s),
        ["Call mum", "Taxes", "bins", "walk dog", "z leash", "a treats"],
        "!1 is high"
    );
    s.set_list_sort(&list, TaskSort::Title).unwrap();
    assert_eq!(
        open(&s),
        ["bins", "Call mum", "Taxes", "walk dog", "a treats", "z leash"]
    );

    // Dragging takes over the order shown, as one undo step.
    let bins = s
        .tasks(&list)
        .unwrap()
        .into_iter()
        .find(|t| t.title == "bins")
        .unwrap();
    s.move_task_before(&bins.id, &list, None).unwrap();
    assert_eq!(s.list_sort(&list), TaskSort::Manual);
    assert_eq!(
        open(&s),
        ["Call mum", "Taxes", "walk dog", "a treats", "z leash", "bins"]
    );
    assert!(s.undo().unwrap());
    assert_eq!(s.list_sort(&list), TaskSort::Title);
    assert_eq!(
        open(&s),
        ["bins", "Call mum", "Taxes", "walk dog", "a treats", "z leash"]
    );

    // The Inbox sorts too.
    s.set_list_sort(INBOX, TaskSort::Title).unwrap();
    assert_eq!(s.lists().unwrap()[0].sort, TaskSort::Title);
}

#[test]
fn completed_tasks_move_to_the_archive_after_a_day() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Home").unwrap();
    let old = s.add_task(&list, "old").unwrap();
    s.add_task(&old, "old.1").unwrap();
    let recent = s.add_task(INBOX, "recent").unwrap();
    s.add_task(&list, "open").unwrap();
    s.set_completed(&old, true, today()).unwrap();
    // Completion times are millis: keep the two apart so "latest first" is decided by them.
    std::thread::sleep(std::time::Duration::from_millis(3));
    s.set_completed(&recent, true, today()).unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert_eq!(
        s.outline_at(&list, now).unwrap().completed.len(),
        2,
        "old and its subtask, still today"
    );
    assert!(s.archive(now).unwrap().is_empty());

    let later = now + not_core::ARCHIVE_AFTER_MS + 1000;
    assert!(s.outline_at(&list, later).unwrap().completed.is_empty());
    assert_eq!(titles(&s.archive(later).unwrap()), ["recent", "old"]);

    // Reopening brings it back to its list.
    s.set_completed(&old, false, today()).unwrap();
    assert_eq!(s.outline_at(&list, later).unwrap().open.len(), 3);
    assert_eq!(titles(&s.archive(later).unwrap()), ["recent"]);

    assert_eq!(s.clear_archive(later).unwrap(), 1);
    assert!(s.archive(later).unwrap().is_empty());
    assert!(s.undo().unwrap());
    assert_eq!(titles(&s.archive(later).unwrap()), ["recent"]);
}

#[test]
fn upcoming_lists_future_tasks_soonest_first() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Home").unwrap();
    for input in [
        "later next month",
        "dentist fri 10am",
        "bins tomorrow",
        "overdue yesterday",
        "undated",
        "call fri 9am",
    ] {
        s.quick_add(&list, input, today(), &[], None).unwrap();
    }
    assert_eq!(
        titles(&s.upcoming_tasks(today()).unwrap()),
        ["bins", "call", "dentist", "later"]
    );
}

#[test]
fn search_finds_tasks_by_title_and_notes() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Groceries").unwrap();
    let milk = s.add_task(&list, "Oat milk").unwrap();
    let crepes = s.add_task(INBOX, "Crêpes for Sunday").unwrap();
    s.set_notes(&crepes, "need MILK and eggs").unwrap();
    s.add_task(INBOX, "Été plans").unwrap();
    s.set_completed(&milk, true, today()).unwrap();

    assert_eq!(
        titles(&s.search_tasks("milk", 10).unwrap()),
        ["Crêpes for Sunday", "Oat milk"],
        "open first"
    );
    assert_eq!(
        titles(&s.search_tasks("milk eggs", 10).unwrap()),
        ["Crêpes for Sunday"]
    );
    assert_eq!(titles(&s.search_tasks("été", 10).unwrap()), ["Été plans"]);
    assert!(s.search_tasks("  ", 10).unwrap().is_empty());
    assert_eq!(s.search_tasks("milk", 1).unwrap().len(), 1);
}

#[test]
fn export_and_backup_carry_everything() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Home").unwrap();
    s.quick_add(&list, "bins tomorrow 7am", today(), &[], None)
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&s.export_json(today()).unwrap()).unwrap();
    assert_eq!(json["lists"][1]["title"], "Home");
    assert_eq!(json["lists"][1]["tasks"][0]["due_time"], "07:00");

    let backup = s.backup().unwrap();
    let fresh = Store::open_in_memory().unwrap();
    fresh.restore_backup(&backup).unwrap();
    assert_eq!(titles(&fresh.tasks(&list).unwrap()), ["bins"]);
    assert!(fresh.restore_backup(b"hello").is_err());
}

#[test]
fn checklist_lines_become_linked_tasks_in_one_undo_step() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Shopping\n- [ ] milk\n  - [x] oat\n- [ ] eggs\nthanks").unwrap();
    let body = store.note_body(&note).unwrap();
    let end = body.find("eggs").unwrap() as u32;

    // Into a new list, named after the note; the lines stay, linked.
    let moved = store.checklist_to_tasks(&note, &body, 10, end, None, "").unwrap();
    assert_eq!(moved.count, 3);
    let list = store.lists().unwrap().into_iter().find(|l| l.id == moved.list_id).unwrap();
    assert_eq!(list.title, "Shopping");
    let outline = store.outline(&moved.list_id).unwrap();
    let open: Vec<_> = outline.open.iter().map(|r| (r.task.title.as_str(), r.depth)).collect();
    assert_eq!(open, [("milk", 0), ("oat", 1), ("eggs", 0)], "order and nesting kept");
    assert!(outline.open[1].task.completed_at.is_some(), "ticked items come done");
    let milk = &outline.open[0].task.id;
    let shown = store.note_body(&note).unwrap();
    assert_eq!(shown, moved.body);
    assert!(shown.starts_with(&format!("Shopping\n- [ ] milk <!--task:{milk}-->\n")));
    assert!(shown.ends_with("-->\nthanks"));
    assert!(store.checklist_to_tasks(&note, &shown, 0, shown.len() as u32, None, "").is_err(), "linked once");
    let (title, preview) = store.notes(None).unwrap().into_iter().map(|n| (n.title, n.preview)).next().unwrap();
    assert_eq!((title.as_str(), preview.as_str()), ("Shopping", "milk oat eggs thanks"), "links hidden");

    // One undo puts the lines back as they were and takes the list away.
    store.undo().unwrap();
    assert_eq!(store.note_body(&note).unwrap(), body);
    assert!(store.lists().unwrap().iter().all(|l| l.id != moved.list_id));
}

#[test]
fn linked_lines_and_tasks_follow_each_other() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    store.checklist_to_tasks(&note, &body, 5, 5, Some(INBOX), "").unwrap();
    let tea = store.tasks(INBOX).unwrap()[0].id.clone();

    // The task ticked and renamed in the list: the note shows it.
    store.set_completed(&tea, true, not_core::date::Date::parse("2026-11-12").unwrap()).unwrap();
    store.rename(&tea, "green tea").unwrap();
    assert_eq!(store.note_body(&note).unwrap(), format!("Day\n- [x] green tea <!--task:{tea}-->"));

    // Reopened and renamed in the note: the task follows, in one undo step with the text.
    store
        .set_note_body(&note, &format!("Day\n- [ ] mint tea <!--task:{tea}-->\nmore"))
        .unwrap();
    let task = store.task_by_id(&tea).unwrap();
    assert_eq!((task.title.as_str(), task.completed_at), ("mint tea", None));
    store.undo().unwrap();
    let task = store.task_by_id(&tea).unwrap();
    assert_eq!(task.title, "green tea");
    assert!(task.completed_at.is_some());

    // Editing other lines leaves the task alone.
    let shown = store.note_body(&note).unwrap();
    store.set_note_body(&note, &format!("{shown}\nmore")).unwrap();
    assert_eq!(store.task_by_id(&tea).unwrap().title, "green tea");

    // A deleted task's line stays as written.
    store.delete(&tea).unwrap();
    assert!(store.note_body(&note).unwrap().contains("- [x] green tea <!--task:"));
}

#[test]
fn undo_after_checklist_move_following_body_saves() {
    let store = Store::open_in_memory().unwrap();
    let note = store.create_note(None, "Day").unwrap();
    store.set_note_body(&note, "Day\n- [ ] ").unwrap();
    store.set_note_body(&note, "Day\n- [ ] tea").unwrap();
    let body = store.note_body(&note).unwrap();
    store.checklist_to_tasks(&note, &body, 5, 5, Some(INBOX), "").unwrap();
    assert!(store.note_body(&note).unwrap().contains("tea <!--task:"));
    store.undo().unwrap();
    assert_eq!(store.note_body(&note).unwrap(), "Day\n- [ ] tea");
    assert!(store.tasks(INBOX).unwrap().is_empty());
}
