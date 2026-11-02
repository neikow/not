mod common;

use common::Log;
use not_core::{EVERY_DAY, HabitDay, Store};

const MON: &str = "2026-11-02";
const WEEKDAYS: u8 = 0b001_1111;

fn summary(store: &Store, date: &str) -> not_core::HabitSummary {
    store.habits(date).unwrap().remove(0)
}

#[test]
fn toggle_counts_up_to_the_goal_then_resets() {
    let store = Store::open_in_memory().unwrap();
    let water = store.create_habit("Water", 3, EVERY_DAY).unwrap();
    let counts: Vec<u32> = (0..4).map(|_| store.toggle_habit(&water, MON).unwrap()).collect();
    assert_eq!(counts, [1, 2, 3, 0]);
    assert_eq!(store.log_habit(&water, MON, -5).unwrap(), 0, "never below zero");
    assert_eq!(store.log_habit(&water, MON, 2).unwrap(), 2);
    let s = summary(&store, MON);
    assert_eq!((s.count, s.due, s.streak), (2, true, 0));
    assert_eq!(s.week, [0, 0, 0, 0, 0, 0, 2]);
}

#[test]
fn streaks_skip_unscheduled_days_and_wait_for_today() {
    let store = Store::open_in_memory().unwrap();
    let run = store.create_habit("Run", 1, WEEKDAYS).unwrap();
    // Wed..Fri of the week before, then Mon. The weekend in between isn't scheduled.
    for day in ["2026-10-28", "2026-10-29", "2026-10-30", MON] {
        store.toggle_habit(&run, day).unwrap();
    }
    assert_eq!(summary(&store, MON).streak, 4);
    // Tuesday not done yet: the streak still stands.
    let tue = summary(&store, "2026-11-03");
    assert_eq!((tue.streak, tue.count), (4, 0));
    // Wednesday: Tuesday was missed.
    let wed = summary(&store, "2026-11-04");
    assert_eq!((wed.streak, wed.best_streak), (0, 4));
    let sat = summary(&store, "2026-10-31");
    assert!(!sat.due);
}

#[test]
fn offline_check_ins_add_up() {
    let mut log = Log::default();
    let (mac, phone) = (Store::open_in_memory().unwrap(), Store::open_in_memory().unwrap());
    let water = mac.create_habit("Water", 8, EVERY_DAY).unwrap();
    log.sync(&mac);
    log.sync(&phone);

    mac.log_habit(&water, MON, 2).unwrap();
    phone.log_habit(&water, MON, 3).unwrap();
    phone.update_habit(&water, "Drink water", 8, EVERY_DAY).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    log.sync(&mac);

    for store in [&mac, &phone] {
        let s = summary(store, MON);
        assert_eq!((s.habit.title.as_str(), s.count), ("Drink water", 5));
    }
    assert_eq!(
        mac.habit_days(&water, "2026-11-01", "2026-11-30").unwrap(),
        [HabitDay {
            date: MON.into(),
            count: 5
        }]
    );
}

#[test]
fn delete_and_undo_keep_history() {
    let store = Store::open_in_memory().unwrap();
    let read = store.create_habit("Read", 1, EVERY_DAY).unwrap();
    store.toggle_habit(&read, MON).unwrap();
    store.delete_habit(&read).unwrap();
    assert!(store.habits(MON).unwrap().is_empty());
    assert!(store.toggle_habit(&read, MON).is_err());
    store.undo().unwrap();
    assert_eq!(summary(&store, MON).count, 1);
}

#[test]
fn habits_survive_reopen_and_keep_order() {
    let dir = std::env::temp_dir().join(format!("not-habits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("store.db");
    {
        let store = Store::open(&path).unwrap();
        let a = store.create_habit("A", 1, EVERY_DAY).unwrap();
        store.create_habit("B", 1, EVERY_DAY).unwrap();
        store.move_habit(&a, 1).unwrap();
        store.toggle_habit(&a, MON).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let habits = store.habits(MON).unwrap();
    let titles: Vec<_> = habits.iter().map(|h| h.habit.title.as_str()).collect();
    assert_eq!(titles, ["B", "A"]);
    assert_eq!(habits[1].count, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_bad_input() {
    let store = Store::open_in_memory().unwrap();
    assert!(store.create_habit("x", 0, EVERY_DAY).is_err());
    assert!(store.create_habit("x", 1, 0).is_err());
    let id = store.create_habit("x", 1, EVERY_DAY).unwrap();
    assert!(store.toggle_habit(&id, "2026-02-30").is_err());
    assert!(store.toggle_habit("nope", MON).is_err());
}
