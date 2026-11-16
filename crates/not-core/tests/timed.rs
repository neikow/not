//! Timed habits: on/off switches, goals in minutes, backdating, timeline edits, sync.

mod common;

use common::Log;
use not_core::date::Date;
use not_core::{Clock, EVERY_DAY, Period, Segment, Store, TimedRule};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
/// Paris in winter.
const OFFSET: i32 = 60;

/// Local `"YYYY-MM-DD HH:MM"` in UTC+1, as Unix millis.
fn at(s: &str) -> i64 {
    let (date, time) = s.split_once(' ').unwrap();
    let (h, m) = time.split_once(':').unwrap();
    let clock = Clock {
        now_ms: 0,
        offset_minutes: OFFSET,
    };
    clock.day_start(Date::parse(date).unwrap())
        + h.parse::<i64>().unwrap() * HOUR
        + m.parse::<i64>().unwrap() * MIN
}

fn clock(now: &str) -> Clock {
    Clock {
        now_ms: at(now),
        offset_minutes: OFFSET,
    }
}

/// Worn 22 h a day: at most 2 h off, normally on, a reminder after 30 min off.
const WEAR: TimedRule = TimedRule {
    at_most_off: true,
    usual_on: true,
    nudge_minutes: 30,
};

/// Reading: at least 30 min on, normally off.
const READ: TimedRule = TimedRule {
    at_most_off: false,
    usual_on: false,
    nudge_minutes: 0,
};

fn summary(s: &Store, date: &str, now: &str) -> not_core::HabitSummary {
    s.habits_at(date, clock(now)).unwrap().remove(0)
}

/// Sets the state from `from` on (open-ended), with now = `from`.
fn switch(s: &Store, id: &str, on: bool, from: &str) {
    s.set_habit_state(id, on, at(from), None, at(from)).unwrap();
}

fn timeline(s: &Store, id: &str, date: &str, now: &str) -> Vec<(String, String, bool)> {
    let fmt = |ms: i64| {
        let local = ms + i64::from(OFFSET) * MIN;
        let minutes = local.rem_euclid(24 * HOUR) / MIN;
        format!("{:02}:{:02}", minutes / 60, minutes % 60)
    };
    s.habit_timeline(id, date, clock(now))
        .unwrap()
        .into_iter()
        .map(|Segment { start, end, on }| (fmt(start), fmt(end), on))
        .collect()
}

fn seg(a: &str, b: &str, on: bool) -> (String, String, bool) {
    (a.into(), b.into(), on)
}

#[test]
fn time_adds_up_from_switches_and_untracked_time_counts_for_nothing() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();

    let fresh = summary(&s, "2026-11-02", "2026-11-02 08:00");
    let state = fresh.timed.unwrap();
    assert_eq!(
        (state.on, state.since, state.on_ms, state.off_ms),
        (false, None, 0, 0)
    );
    assert!(fresh.done, "nothing off yet: within the allowance");
    assert_eq!(fresh.habit.timed, Some(WEAR));
    assert_eq!(fresh.habit.goal, 120);
    assert_eq!(fresh.habit.period, Period::Day);

    switch(&s, &id, true, "2026-11-02 08:00");
    switch(&s, &id, false, "2026-11-02 12:00");
    switch(&s, &id, true, "2026-11-02 12:45");
    let noon = summary(&s, "2026-11-02", "2026-11-02 13:00");
    let state = noon.timed.unwrap();
    assert_eq!(state.on_ms, 4 * HOUR + 15 * MIN);
    assert_eq!(state.off_ms, 45 * MIN);
    assert_eq!((state.on, state.since), (true, Some(at("2026-11-02 12:45"))));
    assert_eq!(noon.count, 45, "minutes off count for an at-most habit");
    assert!(noon.done);
    assert_eq!(
        timeline(&s, &id, "2026-11-02", "2026-11-02 13:00"),
        [
            seg("08:00", "12:00", true),
            seg("12:00", "12:45", false),
            seg("12:45", "13:00", true)
        ]
    );

    // Off again for 1 h 30: 2 h 15 off in all, over the allowance.
    switch(&s, &id, false, "2026-11-02 18:00");
    let evening = summary(&s, "2026-11-02", "2026-11-02 19:30");
    assert_eq!(evening.count, 135);
    assert!(!evening.done);
}

#[test]
fn at_least_habits_count_time_on() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Read", 30, EVERY_DAY, READ).unwrap();
    switch(&s, &id, true, "2026-11-02 21:00");
    let mid = summary(&s, "2026-11-02", "2026-11-02 21:20");
    assert_eq!((mid.count, mid.done), (20, false));
    let done = summary(&s, "2026-11-02", "2026-11-02 21:31");
    assert_eq!((done.count, done.done), (31, true));
    switch(&s, &id, false, "2026-11-02 21:40");
    assert_eq!(summary(&s, "2026-11-02", "2026-11-02 23:00").count, 40);
}

#[test]
fn days_split_at_local_midnight() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Read", 60, EVERY_DAY, READ).unwrap();
    switch(&s, &id, true, "2026-11-02 23:00");
    switch(&s, &id, false, "2026-11-03 01:30");
    let now = "2026-11-03 09:00";
    assert_eq!(summary(&s, "2026-11-02", now).count, 60);
    assert_eq!(summary(&s, "2026-11-03", now).count, 90);
    assert_eq!(
        timeline(&s, &id, "2026-11-03", now),
        [seg("00:00", "01:30", true), seg("01:30", "09:00", false)]
    );
    let days = s
        .habit_days_at(&id, "2026-11-01", "2026-11-04", clock(now))
        .unwrap()
        .into_iter()
        .map(|d| (d.date, d.count))
        .collect::<Vec<_>>();
    assert_eq!(
        days,
        [("2026-11-02".into(), 60), ("2026-11-03".into(), 90)],
        "untracked and future days left out"
    );
    // The same moment is another local day elsewhere: UTC sees 22:00–00:30.
    let utc = Clock {
        now_ms: at(now),
        offset_minutes: 0,
    };
    assert_eq!(s.habits_at("2026-11-02", utc).unwrap()[0].count, 120);
}

#[test]
fn backdating_replaces_what_came_after() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    switch(&s, &id, true, "2026-11-02 08:00");
    // Tapped "off" at 10:00, then remembered it really came off at 09:40.
    switch(&s, &id, false, "2026-11-02 10:00");
    s.set_habit_state(&id, false, at("2026-11-02 09:40"), None, at("2026-11-02 10:01"))
        .unwrap();
    assert_eq!(
        timeline(&s, &id, "2026-11-02", "2026-11-02 10:30"),
        [seg("08:00", "09:40", true), seg("09:40", "10:30", false)]
    );
    assert_eq!(
        s.habit_switches(&id).unwrap().len(),
        2,
        "the 10:00 switch was replaced"
    );

    // "Back on since 10:20" while it's 10:30.
    s.set_habit_state(&id, true, at("2026-11-02 10:20"), None, at("2026-11-02 10:30"))
        .unwrap();
    let state = summary(&s, "2026-11-02", "2026-11-02 10:30").timed.unwrap();
    assert_eq!((state.on, state.off_ms), (true, 40 * MIN));

    // Saying "on since 08:00" again wipes out the break.
    s.set_habit_state(&id, true, at("2026-11-02 08:00"), None, at("2026-11-02 10:30"))
        .unwrap();
    assert_eq!(
        timeline(&s, &id, "2026-11-02", "2026-11-02 10:30"),
        [seg("08:00", "10:30", true)]
    );
}

#[test]
fn ranges_fix_the_past_and_keep_what_follows() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    switch(&s, &id, true, "2026-11-02 07:00");
    switch(&s, &id, false, "2026-11-02 12:00");
    switch(&s, &id, true, "2026-11-02 12:30");
    let now = "2026-11-02 20:00";
    let edit = |on: bool, from: &str, to: &str| {
        s.set_habit_state(&id, on, at(from), Some(at(to)), at(now))
            .unwrap();
    };

    // A forgotten break inside a stretch on.
    edit(false, "2026-11-02 15:00", "2026-11-02 15:20");
    assert_eq!(
        timeline(&s, &id, "2026-11-02", now),
        [
            seg("07:00", "12:00", true),
            seg("12:00", "12:30", false),
            seg("12:30", "15:00", true),
            seg("15:00", "15:20", false),
            seg("15:20", "20:00", true),
        ]
    );

    // Lunch really lasted until 12:50: the range ends after the old switch, which goes.
    edit(false, "2026-11-02 12:00", "2026-11-02 12:50");
    assert_eq!(
        timeline(&s, &id, "2026-11-02", now)[1..3],
        [seg("12:00", "12:50", false), seg("12:50", "15:00", true)]
    );

    // Was on through lunch after all: the break goes, neighbours merge.
    edit(true, "2026-11-02 12:00", "2026-11-02 12:50");
    assert_eq!(
        timeline(&s, &id, "2026-11-02", now)[..2],
        [seg("07:00", "15:00", true), seg("15:00", "15:20", false)]
    );

    // A range over the start of tracking moves the start; before it stays untracked.
    edit(true, "2026-11-02 06:00", "2026-11-02 07:30");
    assert_eq!(
        timeline(&s, &id, "2026-11-02", now)[0],
        seg("06:00", "15:00", true)
    );

    // A range ending now or later is open-ended: from 19:00 on, off.
    s.set_habit_state(
        &id,
        false,
        at("2026-11-02 19:00"),
        Some(at("2026-11-02 21:00")),
        at(now),
    )
    .unwrap();
    assert_eq!(
        *timeline(&s, &id, "2026-11-02", now).last().unwrap(),
        seg("19:00", "20:00", false)
    );
    // No stray switches pile up.
    let switches = s.habit_switches(&id).unwrap();
    assert!(
        switches.windows(2).all(|w| w[0].1 != w[1].1),
        "every switch changes the state: {switches:?}"
    );
}

#[test]
fn bad_edits_are_refused() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    let now = at("2026-11-02 12:00");
    assert!(
        s.set_habit_state(&id, true, now + 2 * MIN, None, now).is_err(),
        "future"
    );
    assert!(
        s.set_habit_state(&id, true, now - HOUR, Some(now - HOUR), now)
            .is_err(),
        "empty range"
    );
    assert!(
        s.set_habit_state(&id, true, now - HOUR, Some(now - 2 * HOUR), now)
            .is_err()
    );
    assert!(
        s.set_habit_state(&id, true, now + 30_000, None, now).is_ok(),
        "clock skew is fine"
    );

    let water = s.create_habit("Water", 3, EVERY_DAY).unwrap();
    assert!(
        s.set_habit_state(&water, true, now, None, now).is_err(),
        "not timed"
    );
    assert!(
        s.log_habit(&id, "2026-11-02", 1).is_err(),
        "timed habits aren't counted"
    );
    assert!(
        s.update_habit(&id, "Retainer", 2, EVERY_DAY, Period::Week)
            .is_err(),
        "nor turned into a count habit"
    );
    assert!(s.create_timed_habit("Zero", 0, EVERY_DAY, WEAR).is_err());
    assert!(
        s.create_timed_habit("Long", 24 * 60 + 1, EVERY_DAY, WEAR)
            .is_err()
    );
    assert!(s.create_timed_habit("Never", 60, 0, WEAR).is_err());
    assert!(s.create_timed_habit("  ", 60, EVERY_DAY, WEAR).is_err());
}

#[test]
fn toggle_flips_a_timed_habit_now() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Read", 30, EVERY_DAY, READ).unwrap();
    s.toggle_habit(&id, "2026-11-02").unwrap();
    let switches = s.habit_switches(&id).unwrap();
    assert_eq!(switches.len(), 1);
    assert!(switches[0].1, "first tap: on");
    s.toggle_habit(&id, "2026-11-02").unwrap();
    assert!(!s.habit_switches(&id).unwrap().last().unwrap().1);
}

#[test]
fn editing_the_rule_keeps_the_switches() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    switch(&s, &id, false, "2026-11-02 08:00");
    let rule = TimedRule {
        nudge_minutes: 0,
        ..WEAR
    };
    s.update_timed_habit(&id, "Aligners", 90, 0b001_1111, rule)
        .unwrap();
    let h = summary(&s, "2026-11-02", "2026-11-02 09:00");
    assert_eq!(
        (h.habit.title.as_str(), h.habit.goal, h.habit.days, h.habit.timed),
        ("Aligners", 90, 0b001_1111, Some(rule))
    );
    assert_eq!(h.count, 60);
    assert!(s.update_timed_habit(&id, "x", 0, EVERY_DAY, rule).is_err());
    let water = s.create_habit("Water", 3, EVERY_DAY).unwrap();
    assert!(s.update_timed_habit(&water, "x", 30, EVERY_DAY, rule).is_err());
}

#[test]
fn streaks_count_met_scheduled_days_since_tracking_started() {
    let s = Store::open_in_memory().unwrap();
    // Weekdays only.
    let id = s.create_timed_habit("Read", 30, 0b001_1111, READ).unwrap();
    // Thu 29 Oct, Fri 30 Oct: 45 min. Weekend untracked-ish (off). Mon 2 Nov: 10 min.
    for day in ["2026-10-29", "2026-10-30"] {
        switch(&s, &id, true, &format!("{day} 21:00"));
        switch(&s, &id, false, &format!("{day} 21:45"));
    }
    switch(&s, &id, true, "2026-11-02 21:00");
    switch(&s, &id, false, "2026-11-02 21:10");

    let fri = summary(&s, "2026-10-30", "2026-10-30 23:00");
    assert_eq!((fri.streak, fri.best_streak), (2, 2));
    // Monday still open in the morning: the streak stands, weekends don't count.
    let mon = summary(&s, "2026-11-02", "2026-11-02 08:00");
    assert_eq!((mon.streak, mon.due), (2, true));
    // Tuesday: Monday fell short.
    let tue = summary(&s, "2026-11-03", "2026-11-03 08:00");
    assert_eq!((tue.streak, tue.best_streak), (0, 2));
    assert_eq!(
        tue.week,
        [0, 45, 45, 0, 0, 10, 0],
        "minutes on, Wed 28 Oct .. Tue"
    );
    let sat = summary(&s, "2026-10-31", "2026-10-31 12:00");
    assert!(!sat.due, "not scheduled");
}

#[test]
fn switches_from_two_offline_devices_merge_in_time_order() {
    let mut log = Log::default();
    let phone = Store::open_in_memory().unwrap();
    let id = phone
        .create_timed_habit("Retainer", 120, EVERY_DAY, WEAR)
        .unwrap();
    switch(&phone, &id, true, "2026-11-02 07:00");
    log.sync(&phone);
    let mac = Store::open_in_memory().unwrap();
    log.sync(&mac);

    // Offline: the phone records lunch, the Mac a break in the afternoon.
    switch(&phone, &id, false, "2026-11-02 12:00");
    switch(&phone, &id, true, "2026-11-02 12:30");
    mac.set_habit_state(
        &id,
        false,
        at("2026-11-02 16:00"),
        Some(at("2026-11-02 16:15")),
        at("2026-11-02 17:00"),
    )
    .unwrap();
    log.sync(&phone);
    log.sync(&mac);
    log.sync(&phone);

    let now = "2026-11-02 17:00";
    let expected = [
        seg("07:00", "12:00", true),
        seg("12:00", "12:30", false),
        seg("12:30", "16:00", true),
        seg("16:00", "16:15", false),
        seg("16:15", "17:00", true),
    ];
    assert_eq!(timeline(&phone, &id, "2026-11-02", now), expected);
    assert_eq!(timeline(&mac, &id, "2026-11-02", now), expected);
    assert_eq!(summary(&mac, "2026-11-02", now).count, 45);
}

#[test]
fn concurrent_edits_of_the_same_moment_agree_everywhere() {
    let mut log = Log::default();
    let a = Store::open_in_memory().unwrap();
    let id = a.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    switch(&a, &id, true, "2026-11-02 07:00");
    log.sync(&a);
    let b = Store::open_in_memory().unwrap();
    log.sync(&b);
    // Both took it off at about the same time, offline.
    switch(&a, &id, false, "2026-11-02 12:00");
    switch(&b, &id, false, "2026-11-02 12:02");
    switch(&b, &id, true, "2026-11-02 12:40");
    log.sync(&a);
    log.sync(&b);
    log.sync(&a);
    let now = "2026-11-02 13:00";
    assert_eq!(
        timeline(&a, &id, "2026-11-02", now),
        timeline(&b, &id, "2026-11-02", now)
    );
    assert_eq!(
        timeline(&a, &id, "2026-11-02", now),
        [
            seg("07:00", "12:00", true),
            seg("12:00", "12:40", false),
            seg("12:40", "13:00", true)
        ]
    );
}

#[test]
fn undo_takes_back_a_switch_or_an_edit() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    switch(&s, &id, true, "2026-11-02 07:00");
    switch(&s, &id, false, "2026-11-02 12:00");
    assert!(s.undo().unwrap());
    assert_eq!(s.habit_switches(&id).unwrap().len(), 1);
    s.set_habit_state(
        &id,
        false,
        at("2026-11-02 09:00"),
        Some(at("2026-11-02 10:00")),
        at("2026-11-02 12:00"),
    )
    .unwrap();
    assert_eq!(s.habit_switches(&id).unwrap().len(), 3);
    assert!(s.undo().unwrap());
    assert_eq!(
        timeline(&s, &id, "2026-11-02", "2026-11-02 12:00"),
        [seg("07:00", "12:00", true)]
    );
}

#[test]
fn backups_and_exports_carry_timed_habits() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    switch(&s, &id, true, "2026-11-02 07:00");
    switch(&s, &id, false, "2026-11-02 12:00");

    let fresh = Store::open_in_memory().unwrap();
    fresh.restore_backup(&s.backup().unwrap()).unwrap();
    let now = "2026-11-02 13:00";
    assert_eq!(summary(&fresh, "2026-11-02", now), summary(&s, "2026-11-02", now));

    let json: serde_json::Value =
        serde_json::from_str(&s.export_json(Date::parse("2026-11-02").unwrap()).unwrap()).unwrap();
    let h = &json["habits"][0];
    assert_eq!(h["kind"], "timed");
    assert_eq!(h["goal_minutes"], 120);
    assert_eq!(h["aim"], "at most off");
    assert_eq!(h["usual"], "on");
    assert_eq!(h["nudge_minutes"], 30);
    assert_eq!(
        h["switches"],
        serde_json::json!([
            { "at": at("2026-11-02 07:00"), "on": true },
            { "at": at("2026-11-02 12:00"), "on": false },
        ])
    );
}

#[test]
fn deleting_a_timed_habit_and_undoing_keeps_its_history() {
    let s = Store::open_in_memory().unwrap();
    let id = s.create_timed_habit("Retainer", 120, EVERY_DAY, WEAR).unwrap();
    switch(&s, &id, true, "2026-11-02 07:00");
    s.delete_habit(&id).unwrap();
    assert!(
        s.habits_at("2026-11-02", clock("2026-11-02 08:00"))
            .unwrap()
            .is_empty()
    );
    assert!(
        s.set_habit_state(&id, false, at("2026-11-02 08:00"), None, at("2026-11-02 08:00"))
            .is_err()
    );
    assert!(s.undo().unwrap());
    assert_eq!(
        summary(&s, "2026-11-02", "2026-11-02 08:00").timed.unwrap().on_ms,
        HOUR
    );
}
