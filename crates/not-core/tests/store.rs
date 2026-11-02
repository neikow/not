use not_core::{Credentials, INBOX, Store, Task};
use not_protocol::{LoggedUpdate, PullResponse};

/// In-memory stand-in for the server log. Unlike the real server it also returns a store's own
/// updates, which must be harmless.
#[derive(Default)]
struct Log(Vec<LoggedUpdate>);

impl Log {
    fn push(&mut self, store: &Store) {
        if let Some(out) = store.outgoing().unwrap() {
            for u in &out.request.updates {
                let seq = self.0.len() as u64 + 1;
                self.0.push(LoggedUpdate {
                    seq,
                    doc: u.doc.clone(),
                    data: u.data.clone(),
                });
            }
            store.ack_push(out).unwrap();
        }
    }

    fn pull(&self, store: &Store) {
        let since = store.pull_cursor().unwrap();
        let updates = self.0.iter().filter(|u| u.seq > since).cloned().collect();
        store
            .apply_pull(&PullResponse {
                updates,
                seq: self.0.len() as u64,
                more: false,
            })
            .unwrap();
    }

    fn sync(&mut self, store: &Store) {
        self.push(store);
        self.pull(store);
    }
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
    let a = s.add_task(&home, "a").unwrap();
    let b = s.add_task(&home, "b").unwrap();
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

    s.set_completed(&sub, true).unwrap();
    assert!(s.tasks(&work).unwrap()[1].completed_at.is_some());
    s.set_completed(&sub, false).unwrap();
    assert!(s.tasks(&work).unwrap()[1].completed_at.is_none());

    s.delete(&work).unwrap();
    assert!(s.tasks(&work).unwrap().is_empty());
    assert_eq!(list_titles(&s), ["Inbox", "Home"]);
}

#[test]
fn rejects_invalid_targets() {
    let s = Store::open_in_memory().unwrap();
    let list = s.create_list("Home").unwrap();
    assert!(s.set_completed(&list, true).is_err(), "lists can't be completed");
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

    let a = s.add_task(INBOX, "a").unwrap();
    let home = s.create_list("Home").unwrap();
    let b = s.add_task(INBOX, "b").unwrap();
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
    assert_eq!(titles(&s.tasks(&list).unwrap()), ["persisted", "second"]);
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
    phone.set_completed(&milk, true).unwrap();
    phone.add_task(INBOX, "bread").unwrap();

    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    let on_mac = mac.tasks(INBOX).unwrap();
    assert_eq!(on_mac, phone.tasks(INBOX).unwrap());
    assert_eq!(titles(&on_mac), ["oat milk", "bread"]);
    assert!(
        on_mac[0].completed_at.is_some(),
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
    s.set_completed(&milk, true).unwrap();

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
    mac.set_completed(&milk, true).unwrap();
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
    s.set_completed(&a2, true).unwrap();
    s.set_completed(&c, true).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    s.set_completed(&b, true).unwrap();

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
    let a = s.add_task(INBOX, "a").unwrap();
    let b = s.add_task(INBOX, "b").unwrap();
    let c = s.add_task(INBOX, "c").unwrap();
    s.set_completed(&b, true).unwrap();

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
