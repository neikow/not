//! The surface the apps call: strings in and out, errors mapped, change callbacks, a real file.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use not_ffi::{NotError, NotStore, StoreObserver, SyncState, TokenKind};

/// Sunday 8 November 2026.
const TODAY: &str = "2026-11-08";

/// A fresh database file, removed when dropped.
struct TempDb(std::path::PathBuf);

impl TempDb {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("not-ffi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> String {
        self.0.join("not.db").to_string_lossy().into_owned()
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Counter(AtomicUsize);

impl StoreObserver for Counter {
    fn on_change(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn on_sync_state(&self, _: SyncState) {}
}

#[test]
fn quick_add_fields_cross_as_strings_and_survive_reopening() {
    let db = TempDb::new("reopen");
    let id = {
        let store = NotStore::new(db.path()).unwrap();
        let id = store
            .quick_add(
                not_ffi::inbox_id(),
                "Milk tomorrow 9am #groceries !1 by fri".into(),
                TODAY.into(),
                None,
                vec![],
            )
            .unwrap();
        let t = store.task_by_id(id.clone()).unwrap();
        assert_eq!(t.title, "Milk");
        assert_eq!(t.start.as_deref(), Some("2026-11-09"));
        assert_eq!(t.time, Some(9 * 60));
        assert_eq!(t.deadline.as_deref(), Some("2026-11-13"));
        assert_eq!(t.priority, 3, "!1 is the highest");
        assert_eq!(t.tags, ["groceries"]);
        id
    };
    let store = NotStore::new(db.path()).unwrap();
    assert_eq!(store.task_by_id(id).unwrap().title, "Milk", "kept on disk");
}

#[test]
fn observers_hear_every_change() {
    let db = TempDb::new("observer");
    let store = NotStore::new(db.path()).unwrap();
    let counter = Arc::new(Counter::default());
    store.set_observer(counter.clone());
    let id = store
        .add_task(not_ffi::inbox_id(), "a".into(), TODAY.into())
        .unwrap();
    let after_add = counter.0.load(Ordering::SeqCst);
    assert!(after_add >= 1);
    store.rename(id, "b".into()).unwrap();
    assert!(counter.0.load(Ordering::SeqCst) > after_add);
}

#[test]
fn bad_input_is_an_invalid_error_and_missing_ids_not_found() {
    let db = TempDb::new("errors");
    let store = NotStore::new(db.path()).unwrap();
    let id = store
        .add_task(not_ffi::inbox_id(), "a".into(), TODAY.into())
        .unwrap();

    let invalid = |r: Result<(), NotError>| matches!(r, Err(NotError::Invalid { .. }));
    assert!(invalid(
        store
            .add_task(not_ffi::inbox_id(), "x".into(), "8/11/2026".into())
            .map(|_| ())
    ));
    assert!(invalid(store.set_schedule(
        id.clone(),
        Some("2026-02-30".into()),
        None,
        None,
        false,
        None
    )));
    assert!(invalid(store.set_schedule(
        id.clone(),
        Some(TODAY.into()),
        None,
        None,
        false,
        Some("FREQ=SOMETIMES".into())
    )));
    assert!(invalid(store.rename(id.clone(), "   ".into())));
    assert!(matches!(
        store.task_by_id("999@1".into()),
        Err(NotError::NotFound { .. })
    ));
    assert!(matches!(
        not_ffi::due_label("soon".into(), None, TODAY.into()),
        Err(NotError::Invalid { .. })
    ));
}

#[test]
fn schedules_and_repeats_round_trip() {
    let db = TempDb::new("schedule");
    let store = NotStore::new(db.path()).unwrap();
    let id = store
        .add_task(not_ffi::inbox_id(), "Bins".into(), TODAY.into())
        .unwrap();
    store
        .set_schedule(
            id.clone(),
            Some("2026-11-09".into()),
            Some(7 * 60),
            None,
            false,
            Some("FREQ=WEEKLY;BYDAY=MO".into()),
        )
        .unwrap();
    let t = store.task_by_id(id.clone()).unwrap();
    assert_eq!(t.repeat.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO"));
    assert!(not_ffi::repeat_label(t.repeat.unwrap()).is_some());
    assert_eq!(
        not_ffi::due_label("2026-11-09".into(), t.time, TODAY.into()).unwrap(),
        "Tomorrow 7:00"
    );

    // Completing a repeat moves it on rather than finishing it.
    store
        .set_completed(id.clone(), true, "2026-11-09".into())
        .unwrap();
    let t = store.task_by_id(id).unwrap();
    assert_eq!(t.start.as_deref(), Some("2026-11-16"));
    assert_eq!(t.completed_at, None);
}

#[test]
fn parsed_tokens_use_utf16_offsets() {
    let db = TempDb::new("tokens");
    let store = NotStore::new(db.path()).unwrap();
    let input = "Café 🎉 tomorrow";
    let parsed = store
        .parse_task(input.into(), TODAY.into(), None, vec![])
        .unwrap();
    assert_eq!(parsed.title, "Café 🎉");
    let token = parsed.tokens.iter().find(|t| t.kind == TokenKind::Date).unwrap();
    let utf16: Vec<u16> = input.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&utf16[token.start as usize..token.end as usize]).unwrap(),
        "tomorrow",
        "the emoji counts as two units, as in Kotlin and NSString"
    );
    assert_eq!(parsed.start.as_deref(), Some("2026-11-09"));

    // Dismissing a token keeps its text in the title.
    let kept = store
        .parse_task(input.into(), TODAY.into(), None, vec![token.text.clone()])
        .unwrap();
    assert_eq!(kept.title, input);
    assert_eq!(kept.start, None);
}

#[test]
fn outlines_carry_depth_and_subtask_progress() {
    let db = TempDb::new("outline");
    let store = NotStore::new(db.path()).unwrap();
    let trip = store
        .add_task(not_ffi::inbox_id(), "Trip".into(), TODAY.into())
        .unwrap();
    let passport = store
        .add_task(trip.clone(), "Passport".into(), TODAY.into())
        .unwrap();
    store
        .add_task(trip.clone(), "Tickets".into(), TODAY.into())
        .unwrap();
    store.set_completed(passport, true, TODAY.into()).unwrap();

    let outline = store.view_outline(not_ffi::inbox_id(), TODAY.into()).unwrap();
    let rows: Vec<_> = outline
        .open
        .iter()
        .map(|r| (r.task.title.as_str(), r.depth, r.done))
        .collect();
    assert_eq!(rows[0], ("Trip", 0, false));
    assert!(rows.contains(&("Tickets", 1, false)));
    assert_eq!((outline.open[0].subtasks, outline.open[0].subtasks_done), (2, 1));
}

#[test]
fn backups_restore_into_another_store() {
    let a = TempDb::new("backup-a");
    let b = TempDb::new("backup-b");
    let from = NotStore::new(a.path()).unwrap();
    from.add_task(not_ffi::inbox_id(), "kept".into(), TODAY.into())
        .unwrap();
    let note = from.create_note(None, "Note\nbody".into()).unwrap();

    let to = NotStore::new(b.path()).unwrap();
    to.restore_backup(from.backup().unwrap()).unwrap();
    assert_eq!(to.search_tasks("kept".into(), 5).unwrap().len(), 1);
    assert_eq!(to.note_body(note).unwrap(), "Note\nbody");
    assert!(matches!(
        to.restore_backup(b"junk".to_vec()),
        Err(NotError::Invalid { .. })
    ));
    let json = to.export_json(TODAY.into()).unwrap();
    assert!(json.contains("\"kept\""));
}

#[test]
fn undo_and_redo_cross_the_boundary() {
    let db = TempDb::new("undo");
    let store = NotStore::new(db.path()).unwrap();
    assert!(!store.can_undo());
    let id = store
        .add_task(not_ffi::inbox_id(), "oops".into(), TODAY.into())
        .unwrap();
    store.delete_item(id.clone()).unwrap();
    assert!(store.can_undo());
    assert!(store.undo().unwrap());
    assert_eq!(store.task_by_id(id.clone()).unwrap().title, "oops");
    assert!(store.can_redo());
    assert!(store.redo().unwrap());
    assert!(
        store
            .view_outline(not_ffi::inbox_id(), TODAY.into())
            .unwrap()
            .open
            .is_empty()
    );
}

#[test]
fn timed_habits_switch_and_report_in_local_days() {
    let db = TempDb::new("timed");
    let store = NotStore::new(db.path()).unwrap();
    let rule = not_ffi::TimedRule {
        at_most_off: true,
        usual_on: true,
        nudge_minutes: 30,
    };
    let id = store
        .create_timed_habit("Retainer".into(), 120, not_ffi::every_day(), rule)
        .unwrap();
    // 2026-11-08 07:00 and 12:00 in UTC+1.
    let seven = 1_794_117_600_000 + 6 * 3_600_000;
    let noon = seven + 5 * 3_600_000;
    store
        .set_habit_state(id.clone(), true, seven, None, seven)
        .unwrap();
    store
        .set_habit_state(id.clone(), false, noon, None, noon)
        .unwrap();

    let now = noon + 20 * 60_000;
    let h = store.habits(TODAY.into(), now, 60).unwrap().remove(0);
    assert_eq!(h.habit.timed, Some(rule));
    let state = h.timed.unwrap();
    assert_eq!(
        (state.on, state.since, state.off_ms),
        (false, Some(noon), 20 * 60_000)
    );
    assert_eq!(h.count, 20);
    let segments = store.habit_timeline(id.clone(), TODAY.into(), now, 60).unwrap();
    assert_eq!(segments.len(), 2);
    assert!(segments[0].on && !segments[1].on);
    let days = store
        .habit_days(id.clone(), "2026-11-01".into(), TODAY.into(), now, 60)
        .unwrap();
    assert_eq!(days.len(), 1);
    assert!(matches!(
        store.log_habit(id, TODAY.into(), 1),
        Err(NotError::Invalid { .. })
    ));
}

#[test]
fn trash_restores_and_empties() {
    let db = TempDb::new("trash");
    let store = NotStore::new(db.path()).unwrap();
    let id = store
        .add_task(not_ffi::inbox_id(), "oops".into(), TODAY.into())
        .unwrap();
    store.delete_item(id.clone()).unwrap();
    let trash = store.trash().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(
        (trash[0].kind, trash[0].title.as_str()),
        (not_ffi::TrashKind::Task, "oops")
    );
    store.restore_from_trash(id.clone()).unwrap();
    assert!(store.trash().unwrap().is_empty());
    store.delete_item(id).unwrap();
    assert_eq!(store.empty_trash().unwrap(), 1);
    assert_eq!(store.purge_trash(i64::MAX / 2).unwrap(), 0);
}

#[test]
fn an_empty_list_offers_what_comes_later_and_what_new_tasks_get() {
    let db = TempDb::new("later");
    let store = NotStore::new(db.path()).unwrap();
    let add = |input: &str| {
        store
            .quick_add(not_ffi::inbox_id(), input.into(), TODAY.into(), None, vec![])
            .unwrap()
    };
    add("Plan trip fri #work");
    add("Write report #home");
    let later = store.view_later("#work".into(), TODAY.into()).unwrap();
    assert_eq!(
        later.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        ["Plan trip"]
    );
    assert_eq!(later[0].start.as_deref(), Some("2026-11-13"));

    let urgent = store.create_view("Urgent".into(), "#work !1".into()).unwrap();
    let d = store.view_defaults(urgent, TODAY.into()).unwrap();
    assert_eq!(
        (d.tags, d.priority, d.start, d.someday),
        (vec!["work".to_string()], 3, None, false)
    );
    assert!(store.view_defaults("bad id".into(), TODAY.into()).is_err());
}

#[test]
fn a_task_nests_under_another_and_comes_back_out() {
    let db = TempDb::new("nest");
    let store = NotStore::new(db.path()).unwrap();
    let inbox = not_ffi::inbox_id();
    let trip = store
        .add_task(inbox.clone(), "Trip".into(), TODAY.into())
        .unwrap();
    let flights = store
        .add_task(inbox.clone(), "Flights".into(), TODAY.into())
        .unwrap();
    store
        .nest_in_view(
            inbox.clone(),
            flights.clone(),
            Some(trip.clone()),
            None,
            TODAY.into(),
        )
        .unwrap();
    assert_eq!(
        store.task_by_id(flights.clone()).unwrap().parent_id,
        Some(trip.clone())
    );
    store
        .nest_in_view(inbox.clone(), flights.clone(), None, Some(trip), TODAY.into())
        .unwrap();
    let rows = store.view_outline(inbox, TODAY.into()).unwrap().open;
    assert_eq!(
        rows.iter().map(|r| r.task.title.as_str()).collect::<Vec<_>>(),
        ["Flights", "Trip"]
    );
}

#[test]
fn starter_tasks_take_a_string_date() {
    let db = TempDb::new("starter");
    let store = NotStore::new(db.path()).unwrap();
    assert_eq!(store.add_starter_tasks(TODAY.into()).unwrap(), 6);
    assert!(matches!(
        store.add_starter_tasks("tomorrow".into()),
        Err(NotError::Invalid { .. })
    ));
}

#[test]
fn imports_report_what_came_in_and_refuse_other_files() {
    let db = TempDb::new("import");
    let store = NotStore::new(db.path()).unwrap();
    let csv = "TYPE,CONTENT,DESCRIPTION,PRIORITY,INDENT,DATE\ntask,Milk @errands,,1,1,tomorrow\n";
    let report = store
        .import_tasks(csv.into(), "Inbox.csv".into(), TODAY.into(), 0)
        .unwrap();
    assert_eq!(report.source, not_ffi::ImportSource::Todoist);
    assert_eq!(report.added, 1);
    assert!(matches!(
        store.import_tasks("a,b\n1,2\n".into(), "x.csv".into(), TODAY.into(), 0),
        Err(NotError::Invalid { .. })
    ));
}

#[test]
fn whats_new_lists_releases_since_the_last_version_opened() {
    assert!(not_ffi::whats_new(String::new(), "0.2.1".into()).is_empty());
    let new = not_ffi::whats_new("0.2.0".into(), "0.2.1".into());
    assert_eq!(new[0].version, "0.2.1");
    assert!(!not_ffi::all_releases()[0].highlights.is_empty());
}
