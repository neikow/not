//! Dates, views (smart lists), tags, order and migration from lists (ADR 0014).

use not_core::date::Date;
use not_core::{INBOX, Outline, SOMEDAY_VIEW, Schedule, Store, TODAY, Task, TaskSort, UPCOMING};
mod common;

use common::Log;

/// Sunday 8 November 2026.
fn today() -> Date {
    day("2026-11-08")
}

fn day(s: &str) -> Date {
    Date::parse(s).unwrap()
}

fn titles(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.title.as_str()).collect()
}

fn rows(outline: &Outline) -> Vec<String> {
    outline.open.iter().map(|r| r.task.title.clone()).collect()
}

fn open(s: &Store, view: &str) -> Vec<String> {
    rows(&s.view_outline(view, today()).unwrap())
}

fn add(s: &Store, view: &str, input: &str) -> String {
    s.quick_add(view, input, today(), &[]).unwrap()
}

fn find(s: &Store, title: &str) -> Task {
    s.all_tasks()
        .unwrap()
        .into_iter()
        .find(|t| t.title == title)
        .unwrap()
}

// ---- dates -------------------------------------------------------------------------------------

#[test]
fn quick_add_applies_what_it_parses() {
    let s = Store::open_in_memory().unwrap();
    let id = add(&s, INBOX, "Buy milk tomorrow 9am #Groceries #home !1 by fri");
    let task = s.task_by_id(&id).unwrap();
    assert_eq!(task.title, "Buy milk");
    assert_eq!(task.start.as_deref(), Some("2026-11-09"));
    assert_eq!(task.time, Some(9 * 60));
    assert_eq!(task.deadline.as_deref(), Some("2026-11-13"));
    assert_eq!(task.priority, 3);
    assert_eq!(task.tags, ["groceries", "home"]);

    // Dismissed tokens stay in the title; a title of only tokens keeps its words.
    s.quick_add(INBOX, "Read tomorrow", today(), &["tomorrow".into()])
        .unwrap();
    add(&s, INBOX, "today");
    assert_eq!(open(&s, INBOX), ["today", "Read tomorrow"]);

    // One undo step takes the whole quick add back.
    assert!(s.undo().unwrap());
    assert_eq!(open(&s, INBOX), ["Read tomorrow"]);
}

#[test]
fn start_dates_hide_tasks_until_then() {
    let s = Store::open_in_memory().unwrap();
    add(&s, INBOX, "now");
    add(&s, INBOX, "later fri");
    add(&s, INBOX, "maybe someday");
    add(&s, INBOX, "late fri by today");
    assert_eq!(
        open(&s, INBOX),
        ["late", "now"],
        "a deadline that has come shows anyway"
    );
    assert_eq!(open(&s, UPCOMING), ["late", "later"]);
    assert_eq!(open(&s, SOMEDAY_VIEW), ["maybe"]);
    assert_eq!(open(&s, TODAY), ["late"]);
    // On Friday it shows again.
    let fri = rows(&s.view_outline(INBOX, day("2026-11-13")).unwrap());
    assert_eq!(fri, ["late", "later", "now"]);
}

#[test]
fn schedules_and_priorities_can_be_changed_and_cleared() {
    let s = Store::open_in_memory().unwrap();
    let id = s.add_task(INBOX, "Pay rent", today()).unwrap();
    let parsed = s.parse_task("monthly on the 1st 10:00 by the 5th", today(), &[]);
    s.set_schedule(&id, &parsed.schedule).unwrap();
    s.set_priority(&id, 2).unwrap();
    let task = s.task_by_id(&id).unwrap();
    assert_eq!(
        (
            task.start.as_deref(),
            task.time,
            task.repeat.as_deref(),
            task.deadline.as_deref()
        ),
        (
            Some("2026-12-01"),
            Some(600),
            Some("FREQ=MONTHLY"),
            Some("2026-12-05")
        )
    );

    // Someday drops the start date and its time; the repeat stays on the deadline.
    s.set_schedule(
        &id,
        &Schedule {
            start: None,
            someday: true,
            ..parsed.schedule
        },
    )
    .unwrap();
    let task = s.task_by_id(&id).unwrap();
    assert!(task.someday && task.start.is_none() && task.time.is_none());
    assert_eq!(task.repeat.as_deref(), Some("FREQ=MONTHLY"));

    s.set_schedule(&id, &Schedule::default()).unwrap();
    s.set_priority(&id, 0).unwrap();
    let task = s.task_by_id(&id).unwrap();
    assert_eq!(
        (
            task.start,
            task.time,
            task.deadline,
            task.repeat,
            task.someday,
            task.priority
        ),
        (None, None, None, None, false, 0)
    );
    assert!(s.set_priority(&id, 4).is_err());
}

#[test]
fn completing_a_repeating_task_moves_it_on() {
    let s = Store::open_in_memory().unwrap();
    let id = s
        .quick_add(
            INBOX,
            "Water plants 1/11 every 2 days #home by 3/11",
            day("2026-11-01"),
            &[],
        )
        .unwrap();
    let sub = s.add_task(&id, "Balcony", today()).unwrap();
    s.set_notes(&id, "Use the green can").unwrap();
    s.set_completed(&sub, true, today()).unwrap();

    // Starts 1 Nov, done late on 8 Nov: next is after today on the 2-day rhythm; the deadline
    // keeps its two days' lead.
    s.set_completed(&id, true, today()).unwrap();
    let task = s.task_by_id(&id).unwrap();
    assert_eq!(task.start.as_deref(), Some("2026-11-09"));
    assert_eq!(task.deadline.as_deref(), Some("2026-11-11"));
    assert_eq!(task.completed_at, None);
    assert_eq!(
        s.task_by_id(&sub).unwrap().completed_at,
        None,
        "subtasks start over"
    );

    // This occurrence is kept as a completed copy, without the repeat.
    let outline = s.view_outline("#home", today()).unwrap();
    let done = &outline.completed[0].task;
    assert_eq!(
        (done.title.as_str(), done.start.as_deref(), done.repeat.as_deref()),
        ("Water plants", Some("2026-11-01"), None)
    );
    assert_eq!(done.notes, "Use the green can");
    assert_eq!(done.tags, ["home"]);

    assert!(s.undo().unwrap());
    assert_eq!(s.task_by_id(&id).unwrap().start.as_deref(), Some("2026-11-01"));
}

#[test]
fn repeats_from_completion_and_on_nth_weekdays() {
    let s = Store::open_in_memory().unwrap();
    let cut = s
        .quick_add(INBOX, "Haircut every 4 weeks after done", day("2026-10-01"), &[])
        .unwrap();
    s.set_completed(&cut, true, today()).unwrap();
    assert_eq!(s.task_by_id(&cut).unwrap().start.as_deref(), Some("2026-12-06"));

    let review = add(&s, INBOX, "Review every last friday");
    assert_eq!(
        s.task_by_id(&review).unwrap().start.as_deref(),
        Some("2026-11-27")
    );
    s.set_completed(&review, true, today()).unwrap();
    assert_eq!(
        s.task_by_id(&review).unwrap().start.as_deref(),
        Some("2026-12-25")
    );
}

#[test]
fn reschedule_moves_what_slipped_in_one_step() {
    let s = Store::open_in_memory().unwrap();
    let a = s.quick_add(INBOX, "a 3/11 9am", day("2026-11-01"), &[]).unwrap();
    let b = s
        .quick_add(INBOX, "b 5/11 by 20/11", day("2026-11-01"), &[])
        .unwrap();
    add(&s, INBOX, "c");
    let overdue: Vec<String> = s
        .view_outline(TODAY, today())
        .unwrap()
        .open
        .iter()
        .filter(|r| r.task.start.as_deref().is_some_and(|d| d < "2026-11-08"))
        .map(|r| r.task.id.clone())
        .collect();
    assert_eq!(overdue.len(), 2);
    s.reschedule(&overdue, Some(today())).unwrap();
    let (a, b) = (s.task_by_id(&a).unwrap(), s.task_by_id(&b).unwrap());
    assert_eq!(
        (a.start.as_deref(), a.time),
        (Some("2026-11-08"), Some(540)),
        "time kept"
    );
    assert_eq!(b.deadline.as_deref(), Some("2026-11-20"), "deadline kept");
    assert!(s.undo().unwrap());
    assert_eq!(s.task_by_id(&a.id).unwrap().start.as_deref(), Some("2026-11-03"));
    assert_eq!(s.task_by_id(&b.id).unwrap().start.as_deref(), Some("2026-11-05"));
}

#[test]
fn today_spans_tags_and_skips_done_parents() {
    let s = Store::open_in_memory().unwrap();
    add(&s, INBOX, "later next week");
    s.quick_add(INBOX, "overdue 3/11 #work", day("2026-11-01"), &[])
        .unwrap();
    add(&s, INBOX, "low today !3");
    add(&s, INBOX, "high today !1");
    add(&s, INBOX, "early today at 8 #work");
    add(&s, INBOX, "due by today");
    let done = add(&s, INBOX, "done today");
    s.set_completed(&done, true, today()).unwrap();
    let parent = add(&s, INBOX, "parent today");
    let child = s.add_task(&parent, "child", today()).unwrap();
    s.set_schedule(&child, &s.parse_task("today", today(), &[]).schedule)
        .unwrap();
    s.set_completed(&parent, true, today()).unwrap();

    assert_eq!(open(&s, TODAY), ["overdue", "early", "high", "low", "due"]);
    // A task added in Today starts today.
    let id = s.add_task(TODAY, "new", today()).unwrap();
    assert_eq!(s.task_by_id(&id).unwrap().start.as_deref(), Some("2026-11-08"));
}

#[test]
fn concurrent_schedule_edits_merge() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let id = add(&mac, INBOX, "Dentist");
    log.sync(&mac);
    log.sync(&phone);

    phone.set_priority(&id, 3).unwrap();
    mac.set_schedule(&id, &mac.parse_task("fri 10am", today(), &[]).schedule)
        .unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    for s in [&mac, &phone] {
        let task = s.task_by_id(&id).unwrap();
        assert_eq!(
            (task.start.as_deref(), task.time, task.priority),
            (Some("2026-11-13"), Some(600), 3)
        );
    }
}

// ---- views ---------------------------------------------------------------------------------------

#[test]
fn saved_views_filter_and_give_new_tasks_what_they_show() {
    let s = Store::open_in_memory().unwrap();
    add(&s, INBOX, "report #work !1");
    add(&s, INBOX, "slides #work");
    add(&s, INBOX, "milk #groceries");
    add(&s, INBOX, "bills #home !1");
    let urgent = s.create_view("Urgent work", "#work !1 or #home !1").unwrap();
    let work = s.create_view("Work", "#work").unwrap();
    assert!(s.create_view("Bad", "#work or").is_err());
    assert!(s.check_query("!7").is_err());
    assert_eq!(s.count_query("#work", today()).unwrap(), 2);

    assert_eq!(open(&s, &urgent), ["bills", "report"]);
    assert_eq!(open(&s, &work), ["slides", "report"]);
    assert_eq!(
        open(&s, INBOX),
        Vec::<String>::new(),
        "tagged tasks aren't in the Inbox"
    );

    let id = add(&s, &urgent, "deploy");
    let task = s.task_by_id(&id).unwrap();
    assert_eq!((task.tags.clone(), task.priority), (vec!["work".to_owned()], 3));
    // What's typed adds to the view's tags.
    let id = add(&s, &work, "call #phone");
    assert_eq!(s.task_by_id(&id).unwrap().tags, ["phone", "work"]);

    let views = s.views(today()).unwrap();
    let names: Vec<_> = views.iter().map(|v| (v.title.as_str(), v.open_count)).collect();
    assert_eq!(
        names,
        [
            ("Inbox", 0),
            ("Today", 0),
            ("Upcoming", 0),
            ("Someday", 0),
            ("Urgent work", 3),
            ("Work", 4)
        ]
    );
    s.move_view(&work, 0).unwrap();
    s.update_view(&urgent, "Fires", "!1").unwrap();
    let titles: Vec<_> = s.views(today()).unwrap().into_iter().map(|v| v.title).collect();
    assert_eq!(&titles[4..], ["Work", "Fires"]);
    s.delete_view(&work).unwrap();
    assert_eq!(s.views(today()).unwrap().len(), 5);
    assert!(s.undo().unwrap());
    assert_eq!(s.views(today()).unwrap().len(), 6);

    // A tag can be browsed without saving a view.
    assert_eq!(open(&s, "#groceries"), ["milk"]);
}

#[test]
fn subtasks_inherit_tags_and_show_under_their_parent() {
    let s = Store::open_in_memory().unwrap();
    let trip = add(&s, INBOX, "Trip #travel");
    s.add_task(&trip, "passport", today()).unwrap();
    let visa = s.add_task(&trip, "visa", today()).unwrap();
    let today_sub = s.add_task(&trip, "book today", today()).unwrap();
    s.set_schedule(&today_sub, &s.parse_task("today", today(), &[]).schedule)
        .unwrap();

    assert_eq!(open(&s, "#travel"), ["Trip", "passport", "visa", "book today"]);
    assert_eq!(open(&s, INBOX), Vec::<String>::new());
    // On its own when its parent doesn't show.
    assert_eq!(open(&s, TODAY), ["book today"]);

    // Out from under its parent it keeps the tags it had through it.
    assert!(s.outdent_task(&visa).unwrap());
    assert_eq!(s.task_by_id(&visa).unwrap().tags, ["travel"]);
    assert!(open(&s, "#travel").contains(&"visa".to_owned()));
}

#[test]
fn each_view_keeps_its_own_order() {
    let s = Store::open_in_memory().unwrap();
    let work = s.create_view("Work", "#work").unwrap();
    let c = add(&s, INBOX, "c #work");
    let b = add(&s, INBOX, "b #work !1");
    let a = add(&s, INBOX, "a #work");
    assert_eq!(open(&s, &work), ["a", "b", "c"], "newest first");
    s.move_in_view(&work, &a, None, today()).unwrap();
    assert_eq!(open(&s, &work), ["b", "c", "a"]);
    assert_eq!(
        open(&s, "#work"),
        ["a", "b", "c"],
        "another view's order is untouched"
    );

    // A task new to the view comes first.
    let d = add(&s, INBOX, "d #work");
    assert_eq!(open(&s, &work), ["d", "b", "c", "a"]);
    s.move_in_view(&work, &d, Some(&a), today()).unwrap();
    assert_eq!(open(&s, &work), ["b", "c", "d", "a"]);
    assert!(s.move_in_view(&work, &c, Some("nope"), today()).is_err());

    // Indenting goes under the task shown above, wherever it is in the tree.
    assert!(s.indent_in_view(&work, &d, today()).unwrap());
    assert_eq!(open(&s, &work), ["b", "c", "d", "a"]);
    assert_eq!(s.task_by_id(&d).unwrap().parent_id.as_deref(), Some(c.as_str()));
    assert!(s.outdent_in_view(&work, &d).unwrap());
    assert_eq!(open(&s, &work), ["b", "c", "d", "a"], "right after its parent");
    assert!(s.undo().unwrap());
    assert_eq!(
        s.task_by_id(&d).unwrap().parent_id.as_deref(),
        Some(c.as_str()),
        "one undo step"
    );
    let _ = b;
}

#[test]
fn views_sort_and_dragging_takes_over() {
    let s = Store::open_in_memory().unwrap();
    for input in [
        "walk dog",
        "Call mum fri !1",
        "bins tomorrow",
        "Taxes tomorrow 9am !3",
    ] {
        add(&s, INBOX, input);
    }
    let walk = find(&s, "walk dog");
    s.add_task(&walk.id, "z leash", today()).unwrap();
    s.add_task(&walk.id, "a treats", today()).unwrap();
    // Inbox hides what starts later; browse everything with `all`.
    let v = s.create_view("All", "untagged all").unwrap();
    assert_eq!(s.view_sort(&v), TaskSort::Manual);
    assert_eq!(
        open(&s, &v),
        ["Taxes", "bins", "Call mum", "walk dog", "z leash", "a treats"]
    );

    s.set_view_sort(&v, TaskSort::Due).unwrap();
    assert_eq!(
        open(&s, &v),
        ["Taxes", "bins", "Call mum", "walk dog", "z leash", "a treats"]
    );
    s.set_view_sort(&v, TaskSort::Priority).unwrap();
    assert_eq!(
        open(&s, &v),
        ["Call mum", "Taxes", "bins", "walk dog", "z leash", "a treats"]
    );
    s.set_view_sort(&v, TaskSort::Title).unwrap();
    assert_eq!(
        open(&s, &v),
        ["bins", "Call mum", "Taxes", "walk dog", "a treats", "z leash"]
    );

    // Dragging takes over the order shown, as one undo step.
    let bins = find(&s, "bins");
    s.move_in_view(&v, &bins.id, None, today()).unwrap();
    assert_eq!(s.view_sort(&v), TaskSort::Manual);
    assert_eq!(
        open(&s, &v),
        ["Call mum", "Taxes", "walk dog", "a treats", "z leash", "bins"]
    );
    assert!(s.undo().unwrap());
    assert_eq!(s.view_sort(&v), TaskSort::Title);

    assert_eq!(s.view_sort(TODAY), TaskSort::Due);
    assert_eq!(s.view_sort(UPCOMING), TaskSort::Due);
    assert!(s.set_view_sort(UPCOMING, TaskSort::Title).is_err());
    s.set_view_sort(INBOX, TaskSort::Title).unwrap();
    assert_eq!(s.views(today()).unwrap()[0].sort, TaskSort::Title);
}

#[test]
fn upcoming_lists_future_tasks_soonest_first() {
    let s = Store::open_in_memory().unwrap();
    for input in [
        "later next month",
        "dentist fri 10am",
        "bins tomorrow",
        "overdue yesterday",
        "undated",
        "call fri 9am",
        "report by 20/11",
    ] {
        add(&s, INBOX, input);
    }
    assert_eq!(open(&s, UPCOMING), ["bins", "call", "dentist", "report", "later"]);
}

// ---- tags ----------------------------------------------------------------------------------------

#[test]
fn tags_can_be_listed_renamed_and_deleted() {
    let s = Store::open_in_memory().unwrap();
    add(&s, INBOX, "a #work #urgent");
    let b = add(&s, INBOX, "b #work");
    add(&s, INBOX, "c #job");
    s.set_completed(&b, true, today()).unwrap();
    let v = s.create_view("Work", "#work !1 or -#work").unwrap();
    let tags: Vec<_> = s
        .tags()
        .unwrap()
        .into_iter()
        .map(|t| (t.name, t.open_count))
        .collect();
    assert_eq!(
        tags,
        [("job".into(), 1), ("urgent".into(), 1), ("work".into(), 1)]
    );

    s.rename_tag("work", "job").unwrap();
    assert_eq!(find(&s, "a").tags, ["job", "urgent"]);
    assert_eq!(find(&s, "c").tags, ["job"]);
    let query = s
        .views(today())
        .unwrap()
        .into_iter()
        .find(|x| x.id == v)
        .unwrap()
        .query;
    assert_eq!(query, "#job !1 or -#job");
    assert!(s.undo().unwrap());
    assert_eq!(find(&s, "a").tags, ["urgent", "work"]);

    s.delete_tag("#urgent").unwrap();
    assert_eq!(find(&s, "a").tags, ["work"]);
    s.set_tags(&find(&s, "a").id, &["Side Project".into(), "".into()])
        .unwrap();
    assert_eq!(find(&s, "a").tags, ["side-project"]);
}

// ---- archive, search, export ---------------------------------------------------------------------

#[test]
fn completed_tasks_move_to_the_archive_after_a_day() {
    let s = Store::open_in_memory().unwrap();
    let old = add(&s, INBOX, "old #home");
    s.add_task(&old, "old.1", today()).unwrap();
    let recent = add(&s, INBOX, "recent");
    add(&s, INBOX, "open #home");
    s.set_completed(&old, true, today()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    s.set_completed(&recent, true, today()).unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert_eq!(
        s.view_outline_at("#home", today(), now).unwrap().completed.len(),
        2
    );
    assert!(s.archive(now).unwrap().is_empty());

    let later = now + not_core::ARCHIVE_AFTER_MS + 1000;
    assert!(
        s.view_outline_at("#home", today(), later)
            .unwrap()
            .completed
            .is_empty()
    );
    assert_eq!(titles(&s.archive(later).unwrap()), ["recent", "old"]);

    s.set_completed(&old, false, today()).unwrap();
    assert_eq!(s.view_outline_at("#home", today(), later).unwrap().open.len(), 3);
    assert_eq!(s.clear_archive(later).unwrap(), 1);
    assert!(s.archive(later).unwrap().is_empty());
    assert!(s.undo().unwrap());
    assert_eq!(titles(&s.archive(later).unwrap()), ["recent"]);
}

#[test]
fn search_finds_tasks_by_title_notes_and_tags() {
    let s = Store::open_in_memory().unwrap();
    let milk = add(&s, INBOX, "Oat milk #groceries");
    let crepes = s.add_task(INBOX, "Crêpes for Sunday", today()).unwrap();
    s.set_notes(&crepes, "need MILK and eggs").unwrap();
    add(&s, INBOX, "Été plans");
    s.set_completed(&milk, true, today()).unwrap();

    assert_eq!(
        titles(&s.search_tasks("milk", 10).unwrap()),
        ["Crêpes for Sunday", "Oat milk"]
    );
    assert_eq!(
        titles(&s.search_tasks("milk eggs", 10).unwrap()),
        ["Crêpes for Sunday"]
    );
    assert_eq!(titles(&s.search_tasks("#groceries", 10).unwrap()), ["Oat milk"]);
    assert_eq!(titles(&s.search_tasks("été", 10).unwrap()), ["Été plans"]);
    assert!(s.search_tasks("  ", 10).unwrap().is_empty());
}

#[test]
fn export_and_backup_carry_everything() {
    let s = Store::open_in_memory().unwrap();
    s.create_view("Home", "#home").unwrap();
    add(&s, INBOX, "bins tomorrow 7am #home by fri");
    let json: serde_json::Value = serde_json::from_str(&s.export_json(today()).unwrap()).unwrap();
    assert_eq!(json["tasks"][0]["time"], "07:00");
    assert_eq!(json["tasks"][0]["tags"][0], "home");
    assert_eq!(json["tasks"][0]["deadline"], "2026-11-13");
    assert_eq!(json["views"][0]["query"], "#home");

    let fresh = Store::open_in_memory().unwrap();
    fresh.restore_backup(&s.backup().unwrap()).unwrap();
    assert_eq!(fresh.all_tasks().unwrap(), s.all_tasks().unwrap());
    assert!(fresh.restore_backup(b"hello").is_err());
}

#[test]
fn export_carries_habits_notes_and_workouts() {
    let s = Store::open_in_memory().unwrap();
    let run = s.create_habit("Run", 1, 0b0010101).unwrap();
    s.update_habit(&run, "Run", 2, 0b0010101, not_core::Period::Week)
        .unwrap();
    s.log_habit(&run, "2026-11-02", 1).unwrap();
    s.log_habit(&run, "2026-11-05", 2).unwrap();
    s.log_habit(&run, "2026-11-06", 1).unwrap();
    s.log_habit(&run, "2026-11-06", -1).unwrap();
    s.log_habit(&run, "2026-11-20", 1).unwrap(); // after `today`: not exported

    let trips = s.create_folder(None, "Trips").unwrap();
    let note = s.create_note(Some(&trips), "Lisbon\n- [ ] tram 28").unwrap();
    s.set_note_pinned(&note, true).unwrap();

    let w = s.create_workout("2026-11-07", "Push").unwrap();
    s.add_exercise(&w, "Bench press", "60kg x 8 x 3").unwrap();

    let json: serde_json::Value = serde_json::from_str(&s.export_json(today()).unwrap()).unwrap();
    assert_eq!(json["format"], "not-export");
    assert_eq!(json["exported_on"], "2026-11-08");

    let habit = &json["habits"][0];
    assert_eq!(habit["title"], "Run");
    assert_eq!(habit["goal"], 2);
    assert_eq!(habit["period"], "week");
    assert_eq!(habit["weekdays"], 0b1111111, "a weekly goal counts on any day");
    assert_eq!(
        habit["check_ins"],
        serde_json::json!([
            { "date": "2026-11-02", "count": 1 },
            { "date": "2026-11-05", "count": 2 },
        ]),
        "days with no check-ins left (an undone one too), future days left out"
    );

    assert_eq!(json["folders"][0]["title"], "Trips");
    let exported = &json["notes"][0];
    assert_eq!(exported["folder_id"], trips.as_str());
    assert_eq!(exported["title"], "Lisbon");
    assert_eq!(exported["pinned"], true);
    assert_eq!(exported["markdown"], "Lisbon\n- [ ] tram 28");

    let workout = &json["workouts"][0];
    assert_eq!(workout["date"], "2026-11-07");
    assert_eq!(workout["title"], "Push");
    assert_eq!(
        workout["exercises"],
        serde_json::json!([{ "name": "Bench press", "sets": "60kg x 8 x 3" }])
    );
}

// ---- migration -------------------------------------------------------------------------------------

/// A backup from before ADR 0014: Inbox "inbox task" (Inbox sorted by priority); list Groceries
/// (`3@5224642129793439165`) with open milk (subtask oat), eggs and completed bread; list "Work
/// Stuff" (`6@…`, sorted by title) with "report" (Fri, !1) and "agenda"; deleted list Old.
const LEGACY: &[u8] = include_bytes!("fixtures/lists-before-0014.backup");
const GROCERIES: &str = "3@5224642129793439165";
const WORK: &str = "6@5224642129793439165";

#[test]
fn lists_become_tags_and_saved_views() {
    let s = Store::open_in_memory().unwrap();
    s.restore_backup(LEGACY).unwrap();

    let views: Vec<_> = s
        .views(today())
        .unwrap()
        .into_iter()
        .filter(|v| !v.built_in)
        .map(|v| (v.id, v.title, v.query, v.sort))
        .collect();
    assert_eq!(
        views,
        [
            (
                GROCERIES.into(),
                "Groceries".into(),
                "#groceries".into(),
                TaskSort::Manual
            ),
            (
                WORK.into(),
                "Work Stuff".into(),
                "#work-stuff".into(),
                TaskSort::Title
            ),
        ],
        "same ids, so share links and widgets keep working"
    );
    assert_eq!(open(&s, GROCERIES), ["milk", "oat", "eggs"], "the list's order");
    let done: Vec<_> = s
        .view_outline(GROCERIES, today())
        .unwrap()
        .completed
        .into_iter()
        .map(|r| r.task.title)
        .collect();
    assert_eq!(done, ["bread"]);
    // Due dates became start dates, so a task due later waits in Upcoming until then.
    assert_eq!(open(&s, WORK), ["agenda"]);
    assert_eq!(find(&s, "report").start.as_deref(), Some("2026-11-13"));
    assert_eq!(open(&s, UPCOMING), ["report"]);
    assert_eq!(open(&s, INBOX), ["inbox task"]);
    assert_eq!(s.view_sort(INBOX), TaskSort::Priority);
    assert!(!s.all_tasks().unwrap().iter().any(|t| t.title == "gone"));
    assert_eq!(
        find(&s, "oat").tags,
        Vec::<String>::new(),
        "subtasks inherit instead"
    );
    assert!(!s.can_undo(), "migration isn't undoable");
}

#[test]
fn devices_migrating_at_once_converge() {
    let mut log = Log::default();
    let (mac, phone) = (Store::open_in_memory().unwrap(), Store::open_in_memory().unwrap());
    // Both get the old data (one of them through sync before migrating isn't possible: each
    // migrates on import), then sync.
    mac.restore_backup(LEGACY).unwrap();
    phone.restore_backup(LEGACY).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);
    assert_eq!(mac.all_tasks().unwrap(), phone.all_tasks().unwrap());
    let ids = |s: &Store| -> Vec<String> { s.views(today()).unwrap().into_iter().map(|v| v.id).collect() };
    assert_eq!(ids(&mac), ids(&phone));
    assert_eq!(ids(&mac).len(), 6, "one view per list, no duplicates");
    assert_eq!(open(&mac, GROCERIES), open(&phone, GROCERIES));
}

// ---- snooze, paste, progress, logbook, review ---------------------------------------------------

#[test]
fn snoozing_puts_tasks_off() {
    use not_core::Snooze;
    let s = Store::open_in_memory().unwrap();
    let a = add(&s, INBOX, "a today 9am");
    let b = add(&s, INBOX, "b");
    let ids = [a.clone(), b.clone()];
    let at = |id: &str| {
        let t = s.task_by_id(id).unwrap();
        (t.start.unwrap_or_default(), t.time)
    };
    // Sunday 10:10.
    s.snooze(&ids, Snooze::LaterToday, today(), 610).unwrap();
    assert_eq!(at(&a), ("2026-11-08".into(), Some(13 * 60 + 30)));
    assert_eq!(at(&b), ("2026-11-08".into(), Some(13 * 60 + 30)));
    s.snooze(&ids, Snooze::LaterToday, today(), 20 * 60).unwrap();
    assert_eq!(at(&a), ("2026-11-09".into(), Some(9 * 60)));
    s.snooze(std::slice::from_ref(&a), Snooze::ThisEvening, today(), 600)
        .unwrap();
    assert_eq!(at(&a), ("2026-11-08".into(), Some(19 * 60)));
    s.snooze(std::slice::from_ref(&a), Snooze::Tomorrow, today(), 600)
        .unwrap();
    assert_eq!(at(&a), ("2026-11-09".into(), Some(19 * 60)), "keeps the time");
    s.snooze(std::slice::from_ref(&b), Snooze::Weekend, today(), 600)
        .unwrap();
    assert_eq!(at(&b).0, "2026-11-14");
    s.snooze(std::slice::from_ref(&b), Snooze::Weekend, day("2026-11-14"), 600)
        .unwrap();
    assert_eq!(at(&b).0, "2026-11-21");
    s.snooze(std::slice::from_ref(&b), Snooze::NextWeek, today(), 600)
        .unwrap();
    assert_eq!(at(&b).0, "2026-11-09");
    assert!(s.undo().unwrap());
    assert_eq!(at(&b).0, "2026-11-21");
}

#[test]
fn pasted_lines_become_tasks_in_one_step() {
    let s = Store::open_in_memory().unwrap();
    let work = s.create_view("Work", "#work").unwrap();
    let text = "Plan:\n- [ ] draft fri\n  - outline\n  - sources\n- [x] kickoff\n\n3. ship !1\n   ";
    let tops = s.quick_add_lines(&work, text, today(), None).unwrap();
    assert_eq!(tops.len(), 4);
    assert_eq!(open(&s, &work), ["Plan:", "ship"]);
    let all: Vec<_> = s
        .view_outline("#work all", today())
        .unwrap()
        .open
        .into_iter()
        .map(|r| (r.depth, r.task.title))
        .collect();
    assert_eq!(
        all,
        [
            (0, "Plan:".into()),
            (0, "draft".into()),
            (1, "outline".into()),
            (1, "sources".into()),
            (0, "ship".into())
        ]
    );
    assert_eq!(find(&s, "draft").start.as_deref(), Some("2026-11-13"));
    assert_eq!(find(&s, "ship").priority, 3);
    assert!(find(&s, "kickoff").completed_at.is_some());
    assert_eq!(find(&s, "outline").tags, Vec::<String>::new(), "subtasks inherit");
    assert!(s.quick_add_lines(&work, " \n- ", today(), None).is_err());
    assert!(s.undo().unwrap());
    assert!(s.all_tasks().unwrap().is_empty());
}

#[test]
fn rows_count_their_subtasks() {
    let s = Store::open_in_memory().unwrap();
    let trip = add(&s, INBOX, "trip");
    for t in ["a", "b", "c"] {
        s.add_task(&trip, t, today()).unwrap();
    }
    s.set_completed(&find(&s, "b").id, true, today()).unwrap();
    let row = &s.view_outline(INBOX, today()).unwrap().open[0];
    assert_eq!((row.subtasks, row.subtasks_done), (3, 1));
}

#[test]
fn logbook_and_stale_tasks() {
    let s = Store::open_in_memory().unwrap();
    let a = add(&s, INBOX, "a");
    let b = add(&s, INBOX, "b");
    add(&s, INBOX, "c");
    add(&s, INBOX, "later fri");
    add(&s, INBOX, "maybe someday");
    s.set_completed(&a, true, today()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    s.set_completed(&b, true, today()).unwrap();
    assert_eq!(titles(&s.logbook(10).unwrap()), ["b", "a"]);
    assert_eq!(titles(&s.logbook(1).unwrap()), ["b"]);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!(s.stale_tasks(now, 30).unwrap().is_empty(), "just made");
    let month = now + 31 * 86_400_000;
    assert_eq!(titles(&s.stale_tasks(month, 30).unwrap()), ["c"]);
    s.rename(&find(&s, "c").id, "c2").unwrap();
    assert!(s.stale_tasks(now + 86_400_000, 30).unwrap().is_empty());
}
