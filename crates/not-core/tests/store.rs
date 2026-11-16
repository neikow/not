use not_core::{Credentials, INBOX, Store, Task};
mod common;

use common::Log;

fn today() -> not_core::date::Date {
    not_core::date::Date::parse("2026-11-08").unwrap()
}

fn titles(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.title.as_str()).collect()
}

/// Every task, top-level first.
fn all(s: &Store) -> Vec<Task> {
    s.all_tasks().unwrap()
}

/// Titles of a view's open rows, subtasks included.
fn open(s: &Store, view: &str) -> Vec<String> {
    s.view_outline(view, today())
        .unwrap()
        .open
        .into_iter()
        .map(|r| r.task.title)
        .collect()
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
    // New tasks go first, subtasks last.
    let b = s.add_task(INBOX, "b", today()).unwrap();
    let a = s.add_task(INBOX, "a", today()).unwrap();
    let sub = s.add_task(&a, "a.1", today()).unwrap();
    let tasks = all(&s);
    assert_eq!(titles(&tasks), ["a", "b", "a.1"]);
    assert_eq!(tasks[2].parent_id.as_deref(), Some(a.as_str()));

    s.move_task_before(&b, None, Some(&a)).unwrap();
    assert_eq!(titles(&all(&s)), ["b", "a", "a.1"]);
    // A parent carries its subtree.
    s.move_task_before(&a, Some(&b), None).unwrap();
    assert_eq!(open(&s, INBOX), ["b", "a", "a.1"]);
    assert!(
        s.move_task_before(&b, Some(&sub), None).is_err(),
        "not under its own subtree"
    );

    s.set_completed(&sub, true, today()).unwrap();
    assert!(s.task_by_id(&sub).unwrap().completed_at.is_some());
    s.set_completed(&sub, false, today()).unwrap();
    assert!(s.task_by_id(&sub).unwrap().completed_at.is_none());

    s.delete(&b).unwrap();
    assert!(all(&s).is_empty(), "the subtree goes with it");
}

#[test]
fn rejects_invalid_targets() {
    let s = Store::open_in_memory().unwrap();
    assert!(s.add_task("nope", "x", today()).is_err());
    assert!(s.add_task(INBOX, "   ", today()).is_err());
    assert!(s.rename(INBOX, "Mine").is_err());
    assert!(s.delete(INBOX).is_err());
    assert!(s.set_completed(INBOX, true, today()).is_err());
}

#[test]
fn two_fresh_devices_share_one_inbox() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    mac.add_task(INBOX, "from mac", today()).unwrap();
    phone.add_task(INBOX, "from phone", today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    // Concurrent appends land in some order, the same on both devices.
    let tasks = all(&mac);
    assert_eq!(tasks, all(&phone));
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

    let view = {
        let s = Store::open(&path).unwrap();
        let view = s.create_view("Home", "#home").unwrap();
        s.add_task(&view, "persisted", today()).unwrap();
        s.set_credentials(&credentials("https://a.example")).unwrap();
        view
    };
    let s = Store::open(&path).unwrap();
    assert_eq!(s.credentials().unwrap(), Some(credentials("https://a.example")));
    assert_eq!(open(&s, &view), ["persisted"]);
    // Edits after reopen use a new peer id and still persist.
    s.add_task(&view, "second", today()).unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(open(&s, &view), ["second", "persisted"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn offline_edits_converge() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let milk = mac.add_task(INBOX, "milk", today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(titles(&all(&phone)), ["milk"]);

    // Both offline: mac renames and tags, phone completes and adds a task.
    mac.rename(&milk, "oat milk").unwrap();
    mac.set_tags(&milk, &["shop".into()]).unwrap();
    phone.set_completed(&milk, true, today()).unwrap();
    phone.set_tags(&milk, &["food".into()]).unwrap();
    phone.add_task(INBOX, "bread", today()).unwrap();

    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    let on_mac = all(&mac);
    assert_eq!(on_mac, all(&phone));
    assert_eq!(
        on_mac[1].tags,
        ["food", "shop"],
        "tags added concurrently both stay"
    );
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

    let a = mac.add_task(INBOX, "A", today()).unwrap();
    let b = mac.add_task(INBOX, "B", today()).unwrap();
    let task = mac.add_task(INBOX, "wander", today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);

    mac.move_task_before(&task, Some(&a), None).unwrap();
    phone.move_task_before(&task, Some(&b), None).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    assert_eq!(all(&mac), all(&phone));
    assert_eq!(all(&mac).iter().filter(|t| t.title == "wander").count(), 1);
}

#[test]
fn deleting_a_task_while_other_device_adds_under_it() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let trip = mac.add_task(INBOX, "Trip", today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);

    mac.delete(&trip).unwrap();
    phone.add_task(&trip, "passport", today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);

    assert!(all(&mac).is_empty());
    assert!(all(&phone).is_empty());
}

#[test]
fn pulled_ops_are_not_pushed_back() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    mac.add_task(INBOX, "x", today()).unwrap();
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
    mac.add_task(INBOX, "milk", today()).unwrap();
    old_server.sync(&mac);
    assert!(mac.outgoing().unwrap().is_none());

    // Pairing again (new or reset server): the cursor and server version start over.
    mac.set_credentials(&credentials("https://new.example")).unwrap();
    assert_eq!(mac.pull_cursor().unwrap(), 0);
    let mut new_server = Log::default();
    new_server.sync(&mac);
    let phone = Store::open_in_memory().unwrap();
    new_server.sync(&phone);
    assert_eq!(titles(&all(&phone)), ["milk"]);

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

    phone.add_task(INBOX, "Home", today()).unwrap();
    mac.create_view("Work", "#work").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert_eq!(*seen.lock().unwrap(), [ChangeOrigin::Local, ChangeOrigin::Remote]);

    // Rejected commands and empty pulls don't notify.
    assert!(phone.add_task(INBOX, "  ", today()).is_err());
    log.sync(&phone);
    phone.remove_listener(id);
    phone.add_task(INBOX, "x", today()).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn undo_restores_a_deleted_subtree() {
    let s = Store::open_in_memory().unwrap();
    let parent = s.add_task(INBOX, "trip", today()).unwrap();
    let child = s.add_task(&parent, "passport", today()).unwrap();
    s.set_notes(&parent, "check visa").unwrap();
    assert!(!s.redo().unwrap(), "nothing to redo yet");

    s.delete(&parent).unwrap();
    assert!(all(&s).is_empty());

    assert!(s.undo().unwrap());
    let tasks = all(&s);
    assert_eq!(titles(&tasks), ["trip", "passport"]);
    assert_eq!(
        tasks[0].id, parent,
        "same identity, so other devices' edits still apply"
    );
    assert_eq!(tasks[1].id, child);
    assert_eq!(tasks[0].notes, "check visa");

    assert!(s.redo().unwrap());
    assert!(all(&s).is_empty());
}

#[test]
fn each_command_is_one_undo_step() {
    let s = Store::open_in_memory().unwrap();
    let milk = s.add_task(INBOX, "milk", today()).unwrap();
    s.rename(&milk, "oat milk").unwrap();
    s.set_completed(&milk, true, today()).unwrap();

    s.undo().unwrap();
    assert!(all(&s)[0].completed_at.is_none());
    s.undo().unwrap();
    assert_eq!(all(&s)[0].title, "milk");
    s.undo().unwrap();
    assert!(all(&s).is_empty());
    assert!(!s.can_undo());
    assert!(s.can_redo());
}

#[test]
fn undo_never_reverts_other_devices() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let milk = mac.add_task(INBOX, "milk", today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert!(!phone.can_undo(), "pulled changes aren't the phone's to undo");

    phone.rename(&milk, "oat milk").unwrap();
    mac.set_completed(&milk, true, today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    assert!(phone.undo().unwrap());
    let task = &all(&phone)[0];
    assert_eq!(task.title, "milk", "the phone's rename is undone");
    assert!(task.completed_at.is_some(), "the mac's completion stays");
}

#[test]
fn concurrent_notes_merge() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let task = mac.add_task(INBOX, "trip", today()).unwrap();
    log.sync(&mac);
    log.sync(&phone);

    // Both devices write the first notes for this task while offline.
    mac.set_notes(&task, "passport").unwrap();
    phone.set_notes(&task, "tickets").unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    let notes = all(&mac)[0].notes.clone();
    assert_eq!(notes, all(&phone)[0].notes);
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
    let merged = all(&mac)[0].notes.clone();
    assert_eq!(merged, all(&phone)[0].notes);
    assert!(
        merged.starts_with("hotel\n") && merged.ends_with("\nvisa"),
        "{merged:?}"
    );
}

#[test]
fn outline_puts_completed_tasks_last() {
    let s = Store::open_in_memory().unwrap();
    let a = s.add_task(INBOX, "a", today()).unwrap();
    s.add_task(&a, "a.1", today()).unwrap();
    let a2 = s.add_task(&a, "a.2", today()).unwrap();
    let b = s.add_task(INBOX, "b", today()).unwrap();
    let c = s.add_task(INBOX, "c", today()).unwrap();
    s.add_task(&c, "c.1", today()).unwrap();
    s.set_completed(&a2, true, today()).unwrap();
    s.set_completed(&c, true, today()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    s.set_completed(&b, true, today()).unwrap();

    let outline = s.view_outline(INBOX, today()).unwrap();
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

    let inbox = &s.views(today()).unwrap()[0];
    assert_eq!(
        (inbox.id.as_str(), inbox.open_count),
        (INBOX, 1),
        "only open top-level tasks count"
    );
}

#[test]
fn move_before_anchors_on_a_sibling() {
    let s = Store::open_in_memory().unwrap();
    let p = s.add_task(INBOX, "p", today()).unwrap();
    let a = s.add_task(&p, "a", today()).unwrap();
    let b = s.add_task(&p, "b", today()).unwrap();
    let c = s.add_task(&p, "c", today()).unwrap();
    s.set_completed(&b, true, today()).unwrap();

    // Dragging c above a among open subtasks; b (completed) is shown done.
    s.move_task_before(&c, Some(&p), Some(&a)).unwrap();
    assert_eq!(open(&s, INBOX), ["p", "c", "a", "b"]);
    s.move_task_before(&c, Some(&p), None).unwrap();
    assert_eq!(open(&s, INBOX), ["p", "a", "b", "c"]);

    let sub = s.add_task(&a, "a.1", today()).unwrap();
    assert!(
        s.move_task_before(&c, Some(&p), Some(&sub)).is_err(),
        "anchor under another parent"
    );
}
