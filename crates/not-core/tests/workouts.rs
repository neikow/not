mod common;

use common::Log;
use not_core::Store;

#[test]
fn sessions_list_latest_first_with_last_time_hints() {
    let s = Store::open_in_memory().unwrap();
    let pull = s.create_workout("2026-10-30", "Pull").unwrap();
    s.add_exercise(&pull, "Tirage vertical (large)", "66kg x 8 x 3").unwrap();
    s.add_exercise(&pull, "Abdos", "").unwrap();
    let push = s.create_workout("2026-11-02", "").unwrap();
    s.add_exercise(&push, "Bench Haltères", "E 18kg x 10\n24kg x 10 x 3").unwrap();

    let list = s.workouts().unwrap();
    assert_eq!(list.iter().map(|w| w.date.as_str()).collect::<Vec<_>>(), ["2026-11-02", "2026-10-30"]);
    assert_eq!(list[0].set_count, 3);
    assert_eq!(list[1].exercise_names, ["Tirage vertical (large)", "Abdos"]);

    // Repeating copies names, not sets; the hint shows last time's.
    let again = s.repeat_workout(&pull, "2026-11-06").unwrap();
    let w = s.workout(&again).unwrap();
    assert_eq!(w.title, "Pull");
    assert_eq!(w.exercises.len(), 2);
    assert_eq!(w.exercises[0].sets, "");
    let previous = w.exercises[0].previous.as_ref().unwrap();
    assert_eq!((previous.date.as_str(), previous.top_weight, previous.set_count), ("2026-10-30", Some(66.0), 3));
    assert_eq!(previous.lines[0].label, "66 kg × 8 × 3");
    // Nothing was logged for Abdos, so there's no hint.
    assert!(w.exercises[1].previous.is_none());

    // Matching ignores case and spacing.
    s.update_exercise(&w.exercises[0].id, "tirage  vertical (LARGE)", "68kg x 8 x 3").unwrap();
    let history = s.exercise_history("Tirage vertical (large)", 10).unwrap();
    assert_eq!(history.iter().map(|h| h.top_weight).collect::<Vec<_>>(), [Some(68.0), Some(66.0)]);
    assert_eq!(s.exercise_names().unwrap()[0], "tirage  vertical (LARGE)");
}

#[test]
fn edits_are_undoable_and_sync() {
    let mut log = Log::default();
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();
    let w = mac.create_workout("2026-11-09", "Legs").unwrap();
    let squat = mac.add_exercise(&w, "Squat", "80kg x 5 x 5").unwrap();
    let lunge = mac.add_exercise(&w, "Lunge", "").unwrap();
    mac.move_exercise(&lunge, 0).unwrap();
    log.sync(&mac);
    log.sync(&phone);
    let names = |s: &Store| s.workout(&w).unwrap().exercises.into_iter().map(|e| e.name).collect::<Vec<_>>();
    assert_eq!(names(&phone), ["Lunge", "Squat"]);

    phone.delete_exercise(&squat).unwrap();
    assert_eq!(names(&phone), ["Lunge"]);
    phone.undo().unwrap();
    assert_eq!(names(&phone), ["Lunge", "Squat"]);

    mac.delete_workout(&w).unwrap();
    assert!(mac.workouts().unwrap().is_empty());
    mac.undo().unwrap();
    assert_eq!(mac.workouts().unwrap().len(), 1);
    assert!(mac.create_workout("2026-13-01", "").is_err());
    assert!(mac.add_exercise(&squat, "nested", "").is_err());
}

#[test]
fn imports_a_text_log() {
    let s = Store::open_in_memory().unwrap();
    let added = s
        .import_workouts(
            "30/10\n\nTirage vertical (large)\n66kg x 8 x 3\n\nStraight Arm Pulldown (double)\n24.8kg x (10 x 2 + 8)\n\n\n02/11\n\nShrugs\n24kg x 20\n24kg x 15 x 2\n\nAbdos\n",
            "2026-11-09",
        )
        .unwrap();
    assert_eq!(added, 2);
    let list = s.workouts().unwrap();
    assert_eq!(list[0].date, "2026-11-02");
    assert_eq!(list[0].set_count, 3);
    let w = s.workout(&list[1].id).unwrap();
    assert_eq!(w.exercises[1].lines[0].reps, [10, 10, 8]);
}

#[test]
fn linked_names_share_history_and_count_per_muscle() {
    let s = Store::open_in_memory().unwrap();
    let bench = s.search_exercises("Développé couché avec haltères", 1).remove(0);
    assert!(bench.muscles.iter().any(|m| m == "Chest"), "{bench:?}");

    // Matching a catalogue name links by itself, ignoring case and accents.
    assert_eq!(s.exercise_info("developpe couche avec halteres").map(|e| e.id), Some(bench.id.clone()));
    assert!(s.exercise_info("Bench Haltères").is_none());

    let old = s.create_workout("2026-11-02", "").unwrap();
    s.add_exercise(&old, "Bench Haltères", "24kg x 10 x 3").unwrap();
    let new = s.create_workout("2026-11-09", "").unwrap();
    s.add_exercise(&new, "Développé couché avec haltères", "").unwrap();
    assert!(s.workout(&new).unwrap().exercises[0].previous.is_none());

    s.link_exercise("Bench Haltères", Some(&bench.id)).unwrap();
    let previous = s.workout(&new).unwrap().exercises[0].previous.clone().unwrap();
    assert_eq!(previous.date, "2026-11-02");
    assert_eq!(s.exercise_history("Développé couché avec haltères", 10).unwrap().len(), 1);

    let muscles = s.muscle_sets("2026-11-02", "2026-11-08").unwrap();
    assert_eq!(muscles.iter().find(|m| m.muscle == "Chest").map(|m| m.sets), Some(3));

    // Unlinking by hand beats the name match.
    s.link_exercise("Bench Haltères", None).unwrap();
    assert!(s.exercise_info("Bench Haltères").is_none());
    assert!(s.link_exercise("x", Some("no-such-id")).is_err());
}
