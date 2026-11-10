//! Habits: definitions in the `habits` tree (flat; the tree only gives them a mergeable order) and
//! check-ins in `habit_log`, a map of counters keyed `habitId:YYYY-MM-DD`. Counters add up, so two
//! devices checking in offline on the same day count twice, and unchecking on one device while the
//! other checks in can't lose an increment.

use std::collections::BTreeMap;

use loro::{Container, LoroMap, LoroTree, TreeID, TreeParentId, ValueOrContainer};
use rusqlite::{Connection, params};

use crate::date::Date;
use crate::projection::{DELETED_AT, TITLE, meta_i64, meta_str};
use crate::store::{Store, clean_title, now_millis};
use crate::{Error, Result};

/// Check-ins per period it takes to complete a habit.
const GOAL: &str = "goal";
/// What the goal counts over: `"day"` (the default), `"week"` or `"month"`.
const PERIOD: &str = "period";
/// Scheduled weekdays as a bit mask, bit 0 = Monday … bit 6 = Sunday.
const DAYS: &str = "days";
pub const EVERY_DAY: u8 = 0b111_1111;

/// What a habit's goal counts over. Weeks run Monday to Sunday, months by the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    /// `goal` check-ins on each scheduled weekday.
    #[default]
    Day,
    /// `goal` check-ins over the week, on any days.
    Week,
    /// `goal` check-ins over the month, on any days.
    Month,
}

impl Period {
    fn as_str(self) -> &'static str {
        match self {
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
        }
    }

    /// Unknown values (from a newer app) read as daily.
    fn parse(s: &str) -> Self {
        match s {
            "week" => Period::Week,
            "month" => Period::Month,
            _ => Period::Day,
        }
    }

    /// The first day of the period `date` falls in.
    fn start(self, date: Date) -> Date {
        match self {
            Period::Day => date,
            Period::Week => date.monday(),
            Period::Month => date.add_days(1 - date.day()),
        }
    }

    /// The first day of the period after the one starting on `start`.
    fn next(self, start: Date) -> Date {
        match self {
            Period::Day => start.add_days(1),
            Period::Week => start.add_days(7),
            Period::Month => start.add_months(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Habit {
    pub id: String,
    pub title: String,
    pub goal: u32,
    /// Bit 0 = Monday … bit 6 = Sunday; [`EVERY_DAY`] for daily habits. Only daily habits are
    /// scheduled; weekly and monthly ones take [`EVERY_DAY`].
    pub days: u8,
    pub period: Period,
}

/// A habit as of a given day, for the habits screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitSummary {
    pub habit: Habit,
    /// Check-ins on that day.
    pub count: u32,
    /// Check-ins in the period so far, that day included; the same as `count` for daily habits.
    pub progress: u32,
    /// `progress` reached the goal.
    pub done: bool,
    /// Daily habits: the day is one of the scheduled weekdays. Weekly and monthly ones: the goal
    /// can only still be met by checking in on every day left in the period, that one included.
    pub due: bool,
    /// Periods in a row that met the goal (scheduled days for daily habits, else weeks or
    /// months), up to that day. The current one doesn't break it while it's still open.
    pub streak: u32,
    pub best_streak: u32,
    /// Check-ins on the 7 days ending with that day, oldest first.
    pub week: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitDay {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub count: u32,
}

impl Store {
    /// A daily habit; [`Store::update_habit`] makes it weekly or monthly.
    pub fn create_habit(&self, title: &str, goal: u32, days: u8) -> Result<String> {
        let title = clean_title(title)?;
        validate(goal, days)?;
        self.transact(|| {
            let id = self.habits.create(TreeParentId::Root)?;
            let meta = self.habits.get_meta(id)?;
            meta.insert(TITLE, title)?;
            meta.insert(GOAL, i64::from(goal))?;
            meta.insert(DAYS, i64::from(days))?;
            Ok(id.to_string())
        })
    }

    pub fn update_habit(&self, id: &str, title: &str, goal: u32, days: u8, period: Period) -> Result<()> {
        let title = clean_title(title)?;
        validate(goal, days)?;
        // A goal over the week or month can be met on any day.
        let days = if period == Period::Day { days } else { EVERY_DAY };
        let id = self.habit_node(id)?;
        self.transact(|| {
            let meta = self.habits.get_meta(id)?;
            // Only what changed, so concurrent edits of different fields both survive.
            if meta_str(&meta, TITLE).as_deref() != Some(title.as_str()) {
                meta.insert(TITLE, title)?;
            }
            if meta_i64(&meta, GOAL) != Some(i64::from(goal)) {
                meta.insert(GOAL, i64::from(goal))?;
            }
            if meta_i64(&meta, DAYS) != Some(i64::from(days)) {
                meta.insert(DAYS, i64::from(days))?;
            }
            if period_of(&meta) != period {
                meta.insert(PERIOD, period.as_str())?;
            }
            Ok(())
        })
    }

    pub fn move_habit(&self, id: &str, index: usize) -> Result<()> {
        let id = self.habit_node(id)?;
        self.transact(|| {
            let siblings: Vec<TreeID> = self
                .habits
                .roots()
                .into_iter()
                .filter(|&s| s != id && is_alive(&self.habits, s))
                .collect();
            match (siblings.get(index), siblings.last()) {
                (Some(&next), _) => self.habits.mov_before(id, next)?,
                (None, Some(&last)) => self.habits.mov_after(id, last)?,
                (None, None) => {}
            }
            Ok(())
        })
    }

    /// Hides the habit; its check-ins stay, so undo brings it back whole.
    pub fn delete_habit(&self, id: &str) -> Result<()> {
        let id = self.habit_node(id)?;
        self.transact(|| Ok(self.habits.get_meta(id)?.insert(DELETED_AT, now_millis())?))
    }

    /// Adds `delta` check-ins (negative to remove some) on `date`, never going below zero.
    /// Returns the new count.
    pub fn log_habit(&self, id: &str, date: &str, delta: i64) -> Result<u32> {
        let id = self.habit_node(id)?;
        let date = Date::parse(date)?;
        self.transact(|| {
            let counter = self.habit_log.ensure_mergeable_counter(&log_key(id, date))?;
            let current = counter.get_value().max(0.0);
            let delta = (delta as f64).max(-current);
            if delta != 0.0 {
                counter.increment(delta)?;
            }
            Ok((current + delta) as u32)
        })
    }

    /// One tap on a habit. Daily: a check-in until the day's goal is met, then back to zero.
    /// Weekly and monthly: the day's check-in, or taking it back.
    pub fn toggle_habit(&self, id: &str, date: &str) -> Result<u32> {
        let node = self.habit_node(id)?;
        let meta = self.habits.get_meta(node)?;
        let goal = match period_of(&meta) {
            Period::Day => meta_i64(&meta, GOAL).unwrap_or(1),
            Period::Week | Period::Month => 1,
        };
        let count = i64::from(self.habit_count(node, Date::parse(date)?));
        self.log_habit(id, date, if count >= goal { -count } else { 1 })
    }

    /// Habits in user order as of `date` (the user's today).
    pub fn habits(&self, date: &str) -> Result<Vec<HabitSummary>> {
        let date = Date::parse(date)?;
        let inner = self.lock();
        let mut stmt = inner
            .db
            .prepare_cached("SELECT id, title, goal, days, period FROM habit ORDER BY position, id")?;
        let habits = stmt
            .query_map([], |r| {
                Ok(Habit {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    goal: r.get(2)?,
                    days: r.get(3)?,
                    period: Period::parse(&r.get::<_, String>(4)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        habits
            .into_iter()
            .map(|habit| {
                let log = load_log(&inner.db, &habit.id)?;
                let at = |d: Date| log.get(&d).copied().unwrap_or(0);
                let (streak, best_streak) = streaks(&habit, &log, date);
                let start = habit.period.start(date);
                let progress = log.range(start..=date).map(|(_, &c)| c).sum();
                let due = match habit.period {
                    Period::Day => scheduled(habit.days, date),
                    Period::Week | Period::Month => {
                        let days_left = habit.period.next(start).days_since(date);
                        progress < habit.goal && days_left <= i64::from(habit.goal - progress)
                    }
                };
                Ok(HabitSummary {
                    count: at(date),
                    progress,
                    done: progress >= habit.goal,
                    due,
                    week: (0..7).rev().map(|back| at(date.add_days(-back))).collect(),
                    streak,
                    best_streak,
                    habit,
                })
            })
            .collect()
    }

    /// Days in `from..=to` with at least one check-in.
    pub fn habit_days(&self, id: &str, from: &str, to: &str) -> Result<Vec<HabitDay>> {
        let (from, to) = (Date::parse(from)?, Date::parse(to)?);
        let inner = self.lock();
        Ok(load_log(&inner.db, id)?
            .range(from..=to)
            .map(|(d, &count)| HabitDay {
                date: d.to_string(),
                count,
            })
            .collect())
    }

    fn habit_node(&self, id: &str) -> Result<TreeID> {
        let node = TreeID::try_from(id).map_err(|_| Error::Invalid(format!("bad id {id:?}")))?;
        if !self.habits.contains(node) || !is_alive(&self.habits, node) {
            return Err(Error::NotFound(id.to_owned()));
        }
        Ok(node)
    }

    fn habit_count(&self, id: TreeID, date: Date) -> u32 {
        counter_value(&self.habit_log, &log_key(id, date))
    }
}

fn validate(goal: u32, days: u8) -> Result<()> {
    if goal == 0 {
        return Err(Error::Invalid("goal must be at least 1".into()));
    }
    if days == 0 || days > EVERY_DAY {
        return Err(Error::Invalid("pick at least one weekday".into()));
    }
    Ok(())
}

fn log_key(id: TreeID, date: Date) -> String {
    format!("{id}:{date}")
}

fn scheduled(days: u8, date: Date) -> bool {
    days & (1 << date.weekday()) != 0
}

/// (current, best) streaks of periods that met the goal, as of `today`: scheduled days for daily
/// habits, else weeks or months.
fn streaks(habit: &Habit, log: &BTreeMap<Date, u32>, today: Date) -> (u32, u32) {
    let Some(&first) = log.keys().next() else {
        return (0, 0);
    };
    let period = habit.period;
    let (mut run, mut best) = (0, 0);
    let mut start = period.start(first);
    let current = period.start(today);
    while start <= current {
        let end = if start == current { today } else { period.next(start).add_days(-1) };
        if period != Period::Day || scheduled(habit.days, start) {
            let total: u32 = log.range(start..=end).map(|(_, &c)| c).sum();
            if total >= habit.goal {
                run += 1;
                best = u32::max(best, run);
            } else if start != current {
                // The current day, week or month is still open.
                run = 0;
            }
        }
        start = period.next(start);
    }
    (run, best)
}

fn period_of(meta: &LoroMap) -> Period {
    meta_str(meta, PERIOD).map_or(Period::Day, |p| Period::parse(&p))
}

fn load_log(db: &Connection, habit_id: &str) -> Result<BTreeMap<Date, u32>> {
    let mut stmt = db.prepare_cached("SELECT date, count FROM habit_log WHERE habit_id = ?1")?;
    let rows = stmt.query_map([habit_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?;
    let mut log = BTreeMap::new();
    for row in rows {
        let (date, count) = row?;
        log.insert(Date::parse(&date)?, count);
    }
    Ok(log)
}

fn counter_value(log: &LoroMap, key: &str) -> u32 {
    match log.get(key) {
        Some(ValueOrContainer::Container(Container::Counter(c))) => c.get_value().max(0.0) as u32,
        _ => 0,
    }
}

fn is_alive(tree: &LoroTree, id: TreeID) -> bool {
    !tree.is_node_deleted(&id).unwrap_or(true) && tree.get_meta(id).is_ok_and(|m| m.get(DELETED_AT).is_none())
}

// ---- projection ---------------------------------------------------------------------------------

/// Rewrites the `habit` table from the tree.
pub fn project_habits(tree: &LoroTree, db: &Connection) -> Result<()> {
    db.execute("DELETE FROM habit", [])?;
    let mut insert = db.prepare_cached(
        "INSERT INTO habit (id, title, goal, days, period, position) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for id in tree.roots() {
        if !is_alive(tree, id) {
            continue;
        }
        let meta = tree.get_meta(id)?;
        insert.execute(params![
            id.to_string(),
            meta_str(&meta, TITLE).unwrap_or_default(),
            meta_i64(&meta, GOAL).unwrap_or(1),
            meta_i64(&meta, DAYS).unwrap_or(i64::from(EVERY_DAY)),
            period_of(&meta).as_str(),
            tree.fractional_index(id).unwrap_or_default(),
        ])?;
    }
    Ok(())
}

/// Re-projects the given `habit_log` keys.
pub fn project_log<'a>(
    log: &LoroMap,
    db: &Connection,
    keys: impl IntoIterator<Item = &'a String>,
) -> Result<()> {
    for key in keys {
        let Some((habit, date)) = key.rsplit_once(':') else {
            continue;
        };
        match counter_value(log, key) {
            0 => db.execute(
                "DELETE FROM habit_log WHERE habit_id = ?1 AND date = ?2",
                params![habit, date],
            )?,
            count => db.execute(
                "INSERT INTO habit_log (habit_id, date, count) VALUES (?1, ?2, ?3)
                 ON CONFLICT (habit_id, date) DO UPDATE SET count = excluded.count",
                params![habit, date, count],
            )?,
        };
    }
    Ok(())
}

pub fn rebuild(tree: &LoroTree, log: &LoroMap, db: &Connection) -> Result<()> {
    db.execute("DELETE FROM habit_log", [])?;
    project_habits(tree, db)?;
    project_log(log, db, &log.keys().map(|k| k.to_string()).collect::<Vec<_>>())
}
