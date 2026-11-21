//! Importing other apps' exports: Todoist project CSVs and TickTick backups.

use not_core::date::Date;
use not_core::{INBOX, ImportSource, Store, Task};

fn today() -> Date {
    Date::parse("2026-11-21").unwrap()
}

fn find(s: &Store, title: &str) -> Task {
    s.all_tasks()
        .unwrap()
        .into_iter()
        .find(|t| t.title == title)
        .unwrap_or_else(|| panic!("no task {title:?}"))
}

const TODOIST: &str = "\u{feff}TYPE,CONTENT,DESCRIPTION,PRIORITY,INDENT,AUTHOR,RESPONSIBLE,DATE,DATE_LANG,TIMEZONE,DURATION,DURATION_UNIT,DEADLINE,DEADLINE_LANG
task,Buy milk @errands,Oat if they have it,4,1,Me (1),,every mon,en,Europe/Paris,,,,
note,\"Remember the bags, \"\"the big ones\"\"\",,,,Me (1),,,,,,,,
task,Plan the trip,,1,1,Me (1),,Nov 25 10:00,en,Europe/Paris,,,2026-12-01,en
task,Book trains,,2,2,Me (1),,,en,Europe/Paris,,,,
section,Later,,,,,,,,,,,,
task,Read about Lisbon,,1,1,Me (1),,someday maybe,en,Europe/Paris,,,,
";

#[test]
fn a_todoist_project_becomes_a_tag_with_its_dates_and_subtasks() {
    let s = Store::open_in_memory().unwrap();
    let report = s
        .import_tasks(TODOIST, "Exports/Trip Ideas.csv", today(), 60)
        .unwrap();
    assert_eq!(report.source, ImportSource::Todoist);
    assert_eq!(report.added, 4);

    let milk = find(&s, "Buy milk");
    assert_eq!(milk.tags, ["errands", "trip-ideas"]);
    assert_eq!(milk.priority, 3, "Todoist's 4 is p1");
    assert!(milk.repeat.is_some(), "every mon repeats");
    assert_eq!(
        milk.notes, "Oat if they have it\n\nRemember the bags, \"the big ones\"",
        "the comment joins the description"
    );

    let trip = find(&s, "Plan the trip");
    assert_eq!(trip.start.as_deref(), Some("2026-11-25"));
    assert_eq!(trip.time, Some(600));
    assert_eq!(trip.deadline.as_deref(), Some("2026-12-01"));

    let trains = find(&s, "Book trains");
    assert_eq!(
        trains.parent_id.as_deref(),
        Some(trip.id.as_str()),
        "INDENT 2 nests"
    );
    assert!(trains.tags.is_empty(), "it inherits the project tag");
    assert_eq!(trains.priority, 1);

    let lisbon = find(&s, "Read about Lisbon");
    assert_eq!(lisbon.start, None);
    assert!(
        lisbon.notes.contains("Todoist date: someday maybe"),
        "kept when not understood"
    );

    let order: Vec<String> = s
        .view_outline("#trip-ideas all", today())
        .unwrap()
        .open
        .iter()
        .filter(|r| r.depth == 0)
        .map(|r| r.task.title.clone())
        .collect();
    assert_eq!(
        order,
        ["Buy milk", "Plan the trip", "Read about Lisbon"],
        "in the file's order"
    );

    assert!(s.undo().unwrap());
    assert!(s.all_tasks().unwrap().is_empty(), "one undo step");
}

#[test]
fn the_todoist_inbox_stays_untagged() {
    let s = Store::open_in_memory().unwrap();
    s.import_tasks(TODOIST, "Inbox.csv", today(), 0).unwrap();
    assert_eq!(find(&s, "Buy milk").tags, ["errands"]);
    assert!(
        s.view_outline(INBOX, today())
            .unwrap()
            .open
            .iter()
            .any(|r| r.task.title == "Read about Lisbon")
    );
}

const TICKTICK: &str = r#""Date: 2026-11-21+0000"
"Version: 7.1"
"Status:
0 Normal
1 Completed
2 Archived"
"Folder Name","List Name","Title","Kind","Tags","Content","Is Check list","Start Date","Due Date","Reminder","Repeat","Priority","Status","Created Time","Completed Time","Order","Timezone","Is All Day","Is Floating","Column Name","Column Order","View Mode","taskId","parentId"
"","Inbox","Call the dentist","TEXT","health","Ask about Tuesday","N","2026-11-20T23:00:00+0000","2026-11-20T23:00:00+0000","","","5","0","2026-11-01T10:00:00+0000","","-1","Europe/Paris","true","false","","","list","1",""
"Home","Chores","Water the plants","TEXT","","","N","2026-11-22T07:30:00+0000","2026-11-22T07:30:00+0000","","RRULE:FREQ=WEEKLY;INTERVAL=1;BYDAY=SA","1","0","2026-11-01T10:00:00+0000","","-2","Europe/Paris","false","false","","","list","2",""
"Home","Chores","Kitchen","TEXT","","","N","","","","","0","0","2026-11-01T10:00:00+0000","","-3","Europe/Paris","false","false","","","list","3","2"
"Home","Chores","Old thing","TEXT","","","N","","","","","0","2","2026-11-01T10:00:00+0000","2026-11-02T10:00:00+0000","-4","Europe/Paris","false","false","","","list","4",""
"#;

#[test]
fn a_ticktick_backup_keeps_lists_times_repeats_and_subtasks() {
    let s = Store::open_in_memory().unwrap();
    let report = s
        .import_tasks(TICKTICK, "TickTick-backup.csv", today(), 60)
        .unwrap();
    assert_eq!(report.source, ImportSource::TickTick);
    assert_eq!((report.added, report.skipped_done), (3, 1));

    let dentist = find(&s, "Call the dentist");
    assert_eq!(dentist.tags, ["health"], "the Inbox list isn't a tag");
    assert_eq!(dentist.start.as_deref(), Some("2026-11-21"), "all day, at UTC+1");
    assert_eq!(dentist.time, None);
    assert_eq!(dentist.priority, 3);
    assert_eq!(dentist.notes, "Ask about Tuesday");

    let plants = find(&s, "Water the plants");
    assert_eq!(plants.tags, ["chores"]);
    assert_eq!(plants.start.as_deref(), Some("2026-11-22"));
    assert_eq!(plants.time, Some(8 * 60 + 30));
    assert!(plants.repeat.is_some());

    let kitchen = find(&s, "Kitchen");
    assert_eq!(kitchen.parent_id.as_deref(), Some(plants.id.as_str()));
    assert!(kitchen.tags.is_empty());
}

#[test]
fn other_files_are_refused_and_change_nothing() {
    let s = Store::open_in_memory().unwrap();
    let err = s
        .import_tasks("name,amount\nrent,900\n", "budget.csv", today(), 0)
        .unwrap_err();
    assert!(err.to_string().contains("Todoist or TickTick"), "{err}");
    assert!(s.all_tasks().unwrap().is_empty());
}
