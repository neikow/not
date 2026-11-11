//! Workouts: each session is a root of the `workouts` tree, its exercises are the session's children
//! (the tree gives both a mergeable order). An exercise keeps its sets as the text the user typed,
//! one line per group — `24kg x 10 x 3`, `E 18kg x 10`, `24.8kg x (10 x 2 + 8)` — so nothing they
//! write is ever lost to parsing. [`parse_sets`] reads that text for display and statistics.
//!
//! Exercises are matched across sessions by name, ignoring case and extra spaces, for the
//! "last time" hint and history.

use loro::{LoroTree, TreeID, TreeParentId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::date::Date;
use crate::projection::{DELETED_AT, TITLE, meta_str};
use crate::store::{Store, clean_title, now_millis};
use crate::{Error, Result};

/// On a session: `YYYY-MM-DD`.
const DATE: &str = "date";
const CREATED_AT: &str = "created_at";
/// On an exercise.
const NAME: &str = "name";
const SETS: &str = "sets";
/// On an exercise: ticked off in this session (a bool).
const DONE: &str = "done";

/// Sets a single line may expand to; guards against `10 x 1000000`.
const MAX_SETS: usize = 50;

/// One line of an exercise's sets.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLine {
    /// As typed.
    pub raw: String,
    /// Read as sets; when false the line is shown as a plain note.
    pub parsed: bool,
    pub warmup: bool,
    /// `None` for bodyweight.
    pub weight: Option<f64>,
    /// `kg`, `lb`, or empty for bodyweight.
    pub unit: String,
    /// Reps of each set, in order: `10 x 3` is `[10, 10, 10]`. Empty for cardio.
    pub reps: Vec<u32>,
    /// Cardio (`500m 14min`, `11.6km 1h30`): metres covered; `unit` then holds the unit typed
    /// (`m`, `km` or `mi`).
    pub distance: Option<f64>,
    /// Cardio: seconds.
    pub duration: Option<u32>,
    /// Text after a quote (`28kg x 9 " spotted on the first`), shown beside the sets.
    pub note: Option<String>,
    /// Tidy form for display, e.g. `24 kg × 10 × 3`; the raw text when not parsed.
    pub label: String,
    /// Ticked off in this session: the line starts with `✓` (see [`DONE_MARK`]).
    pub done: bool,
}

/// Marks a set line as done: `✓ 24kg x 10`. Kept in the text so it syncs, survives edits and
/// reorders, and reads fine anywhere the text is shown.
pub const DONE_MARK: &str = "✓ ";

#[derive(Debug, Clone, PartialEq)]
pub struct WorkoutSummary {
    pub id: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// May be empty.
    pub title: String,
    pub exercise_names: Vec<String>,
    /// Working sets (warm-ups excluded).
    pub set_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Workout {
    pub id: String,
    pub date: String,
    pub title: String,
    pub exercises: Vec<WorkoutExercise>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkoutExercise {
    pub id: String,
    pub name: String,
    pub sets: String,
    pub lines: Vec<SetLine>,
    /// Ticked off for this session.
    pub done: bool,
    /// The latest earlier session with this exercise and some sets logged.
    pub previous: Option<ExerciseLog>,
}

/// One session's sets of an exercise, for history.
#[derive(Debug, Clone, PartialEq)]
pub struct ExerciseLog {
    pub workout_id: String,
    pub date: String,
    pub sets: String,
    pub lines: Vec<SetLine>,
    /// Heaviest working set.
    pub top_weight: Option<f64>,
    /// Σ weight × reps over working sets.
    pub volume: f64,
    pub set_count: u32,
    /// Cardio: metres over all lines.
    pub distance: f64,
    /// Cardio: seconds over all lines.
    pub duration: u32,
}

impl Store {
    pub fn create_workout(&self, date: &str, title: &str) -> Result<String> {
        let date = Date::parse(date)?;
        self.transact(|| {
            let id = self.workouts.create(TreeParentId::Root)?;
            let meta = self.workouts.get_meta(id)?;
            meta.insert(DATE, date.to_string())?;
            meta.insert(CREATED_AT, now_millis())?;
            if !title.trim().is_empty() {
                meta.insert(TITLE, title.trim())?;
            }
            Ok(id.to_string())
        })
    }

    /// A new session on `date` with the same title and exercises as `from`, without their sets: the
    /// "last time" hint shows them.
    pub fn repeat_workout(&self, from: &str, date: &str) -> Result<String> {
        let from = self.workout_node(from)?;
        let date = Date::parse(date)?;
        self.transact(|| {
            let source = self.workouts.get_meta(from)?;
            let id = self.workouts.create(TreeParentId::Root)?;
            let meta = self.workouts.get_meta(id)?;
            meta.insert(DATE, date.to_string())?;
            meta.insert(CREATED_AT, now_millis())?;
            if let Some(title) = meta_str(&source, TITLE) {
                meta.insert(TITLE, title)?;
            }
            for exercise in self.live_children(from) {
                let name = meta_str(&self.workouts.get_meta(exercise)?, NAME).unwrap_or_default();
                let copy = self.workouts.create(TreeParentId::Node(id))?;
                self.workouts.get_meta(copy)?.insert(NAME, name)?;
            }
            Ok(id.to_string())
        })
    }

    pub fn update_workout(&self, id: &str, date: &str, title: &str) -> Result<()> {
        let id = self.workout_node(id)?;
        let date = Date::parse(date)?.to_string();
        let title = title.trim();
        self.transact(|| {
            let meta = self.workouts.get_meta(id)?;
            if meta_str(&meta, DATE).as_deref() != Some(date.as_str()) {
                meta.insert(DATE, date)?;
            }
            match title {
                "" => meta.delete(TITLE)?,
                t if meta_str(&meta, TITLE).as_deref() != Some(t) => meta.insert(TITLE, t)?,
                _ => {}
            }
            Ok(())
        })
    }

    /// Undoable.
    pub fn delete_workout(&self, id: &str) -> Result<()> {
        let id = self.workout_node(id)?;
        self.transact(|| Ok(self.workouts.get_meta(id)?.insert(DELETED_AT, now_millis())?))
    }

    pub fn add_exercise(&self, workout: &str, name: &str, sets: &str) -> Result<String> {
        let workout = self.workout_node(workout)?;
        let name = clean_title(name)?;
        self.transact(|| {
            let id = self.workouts.create(TreeParentId::Node(workout))?;
            let meta = self.workouts.get_meta(id)?;
            meta.insert(NAME, name)?;
            if !sets.trim().is_empty() {
                meta.insert(SETS, sets.trim_end())?;
            }
            Ok(id.to_string())
        })
    }

    pub fn update_exercise(&self, id: &str, name: &str, sets: &str) -> Result<()> {
        let id = self.exercise_node(id)?;
        let name = clean_title(name)?;
        let sets = sets.trim_end();
        self.transact(|| {
            let meta = self.workouts.get_meta(id)?;
            if meta_str(&meta, NAME).as_deref() != Some(name.as_str()) {
                meta.insert(NAME, name)?;
            }
            if meta_str(&meta, SETS).unwrap_or_default() != sets {
                meta.insert(SETS, sets)?;
            }
            Ok(())
        })
    }

    /// Ticks an exercise off for this session, or back on.
    pub fn set_exercise_done(&self, id: &str, done: bool) -> Result<()> {
        let id = self.exercise_node(id)?;
        self.transact(|| {
            let meta = self.workouts.get_meta(id)?;
            if done {
                meta.insert(DONE, true)?;
            } else {
                meta.delete(DONE)?;
            }
            Ok(())
        })
    }

    pub fn delete_exercise(&self, id: &str) -> Result<()> {
        let id = self.exercise_node(id)?;
        self.transact(|| Ok(self.workouts.get_meta(id)?.insert(DELETED_AT, now_millis())?))
    }

    /// Moves an exercise to `index` among the session's other exercises.
    pub fn move_exercise(&self, id: &str, index: usize) -> Result<()> {
        let id = self.exercise_node(id)?;
        let Some(TreeParentId::Node(workout)) = self.workouts.parent(id) else {
            return Err(Error::NotFound(id.to_string()));
        };
        self.transact(|| {
            let siblings: Vec<TreeID> = self
                .live_children(workout)
                .into_iter()
                .filter(|&s| s != id)
                .collect();
            match (siblings.get(index), siblings.last()) {
                (Some(&next), _) => self.workouts.mov_before(id, next)?,
                (None, Some(&last)) => self.workouts.mov_after(id, last)?,
                (None, None) => {}
            }
            Ok(())
        })
    }

    /// Latest first.
    pub fn workouts(&self) -> Result<Vec<WorkoutSummary>> {
        let inner = self.lock();
        let mut stmt = inner
            .db
            .prepare_cached("SELECT id, date, title FROM workout ORDER BY date DESC, position DESC, id")?;
        let sessions = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut names = inner.db.prepare_cached(
            "SELECT name, set_count FROM workout_exercise WHERE workout_id = ?1 ORDER BY position, id",
        )?;
        sessions
            .into_iter()
            .map(|(id, date, title)| {
                let rows = names
                    .query_map([&id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(WorkoutSummary {
                    set_count: rows.iter().map(|(_, n)| n).sum(),
                    exercise_names: rows.into_iter().map(|(n, _)| n).collect(),
                    id,
                    date,
                    title,
                })
            })
            .collect()
    }

    pub fn workout(&self, id: &str) -> Result<Workout> {
        let inner = self.lock();
        let (date, title): (String, String) = inner
            .db
            .query_row("SELECT date, title FROM workout WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?
            .ok_or_else(|| Error::NotFound(id.to_owned()))?;
        let mut stmt = inner.db.prepare_cached(
            "SELECT id, name, name_key, sets, done FROM workout_exercise WHERE workout_id = ?1 ORDER BY position, id",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, bool>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let exercises = rows
            .into_iter()
            .map(|(eid, name, key, sets, done)| {
                Ok(WorkoutExercise {
                    id: eid,
                    name,
                    done,
                    lines: parse_sets(&sets),
                    sets,
                    previous: history(&inner.db, &self.same_exercise(&inner.db, &key)?, Some(&date), 1)?
                        .into_iter()
                        .next(),
                })
            })
            .collect::<Result<_>>()?;
        Ok(Workout {
            id: id.to_owned(),
            date,
            title,
            exercises,
        })
    }

    /// Sessions with this exercise, latest first, at most `limit`. Names linked to the same
    /// catalogue exercise count as one.
    pub fn exercise_history(&self, name: &str, limit: u32) -> Result<Vec<ExerciseLog>> {
        let inner = self.lock();
        let keys = self.same_exercise(&inner.db, &name_key(name))?;
        history(&inner.db, &keys, None, limit)
    }

    /// Working sets per primary muscle of catalogue-linked exercises in sessions dated
    /// `from..=to`, most trained first.
    pub fn muscle_sets(&self, from: &str, to: &str) -> Result<Vec<crate::MuscleSets>> {
        let (from, to) = (Date::parse(from)?.to_string(), Date::parse(to)?.to_string());
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(
            "SELECT e.name_key, sum(e.set_count) FROM workout_exercise e JOIN workout w ON w.id = e.workout_id
             WHERE w.date BETWEEN ?1 AND ?2 GROUP BY e.name_key",
        )?;
        let mut sets: std::collections::HashMap<String, u32> = Default::default();
        for row in stmt.query_map([from, to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))? {
            let (key, count) = row?;
            let Some(exercise) = self.resolve(&key).and_then(|id| crate::catalog::by_id(&id)) else {
                continue;
            };
            for muscle in exercise.muscles {
                *sets.entry(muscle).or_default() += count;
            }
        }
        let mut out: Vec<crate::MuscleSets> = sets
            .into_iter()
            .filter(|&(_, n)| n > 0)
            .map(|(muscle, sets)| crate::MuscleSets { muscle, sets })
            .collect();
        out.sort_by(|a, b| b.sets.cmp(&a.sets).then_with(|| a.muscle.cmp(&b.muscle)));
        Ok(out)
    }

    /// Name keys logged so far that mean the same exercise as `key` (linked to the same catalogue
    /// exercise), `key` included.
    fn same_exercise(&self, db: &Connection, key: &str) -> Result<Vec<String>> {
        let Some(id) = self.resolve(key) else {
            return Ok(vec![key.to_owned()]);
        };
        let mut stmt = db.prepare_cached("SELECT DISTINCT name_key FROM workout_exercise")?;
        let mut keys = vec![key.to_owned()];
        for k in stmt.query_map([], |r| r.get::<_, String>(0))? {
            let k = k?;
            if k != key && self.resolve(&k).as_deref() == Some(id.as_str()) {
                keys.push(k);
            }
        }
        Ok(keys)
    }

    /// Exercise names used so far, most recently used first, for suggestions.
    pub fn exercise_names(&self) -> Result<Vec<String>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(
            "SELECT e.name, e.name_key FROM workout_exercise e JOIN workout w ON w.id = e.workout_id
             ORDER BY w.date DESC, w.position DESC, e.position",
        )?;
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (name, key) = row?;
            if seen.insert(key) {
                out.push(name);
            }
        }
        Ok(out)
    }

    /// Reads a log kept as text — a `dd/mm` line starts a session, a line that reads as sets adds
    /// to the exercise above, any other line names an exercise — and adds it as sessions. Dates
    /// without a year take the latest one not after `today`. Returns how many sessions were added.
    pub fn import_workouts(&self, text: &str, today: &str) -> Result<u32> {
        let today = Date::parse(today)?;
        let sessions = parse_log(text, today);
        if sessions.is_empty() {
            return Ok(0);
        }
        self.transact(|| {
            let created = now_millis();
            for session in &sessions {
                let id = self.workouts.create(TreeParentId::Root)?;
                let meta = self.workouts.get_meta(id)?;
                meta.insert(DATE, session.date.to_string())?;
                meta.insert(CREATED_AT, created)?;
                for (name, sets) in &session.exercises {
                    let exercise = self.workouts.create(TreeParentId::Node(id))?;
                    let meta = self.workouts.get_meta(exercise)?;
                    meta.insert(NAME, name.as_str())?;
                    if !sets.is_empty() {
                        meta.insert(SETS, sets.as_str())?;
                    }
                }
            }
            Ok(sessions.len() as u32)
        })
    }

    fn workout_node(&self, id: &str) -> Result<TreeID> {
        let node = self.alive_workout_node(id)?;
        match self.workouts.parent(node) {
            Some(TreeParentId::Root) => Ok(node),
            _ => Err(Error::Invalid(format!("{id} is not a workout"))),
        }
    }

    fn exercise_node(&self, id: &str) -> Result<TreeID> {
        let node = self.alive_workout_node(id)?;
        match self.workouts.parent(node) {
            Some(TreeParentId::Node(_)) => Ok(node),
            _ => Err(Error::Invalid(format!("{id} is not an exercise"))),
        }
    }

    fn alive_workout_node(&self, id: &str) -> Result<TreeID> {
        let node = TreeID::try_from(id).map_err(|_| Error::Invalid(format!("bad id {id:?}")))?;
        if !crate::projection::is_alive(&self.workouts, node) {
            return Err(Error::NotFound(id.to_owned()));
        }
        Ok(node)
    }

    fn live_children(&self, parent: TreeID) -> Vec<TreeID> {
        self.workouts
            .children(TreeParentId::Node(parent))
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| crate::projection::is_alive(&self.workouts, c))
            .collect()
    }
}

/// Logs of exercises named by any of `keys` with sets, latest first; only sessions before `before`
/// when given.
fn history(db: &Connection, keys: &[String], before: Option<&str>, limit: u32) -> Result<Vec<ExerciseLog>> {
    let marks = vec!["?"; keys.len()].join(", ");
    let mut stmt = db.prepare(&format!(
        "SELECT w.id, w.date, e.sets FROM workout_exercise e JOIN workout w ON w.id = e.workout_id
         WHERE e.name_key IN ({marks}) AND e.sets != '' AND (?{b} IS NULL OR w.date < ?{b})
         ORDER BY w.date DESC, w.position DESC LIMIT ?{l}",
        b = keys.len() + 1,
        l = keys.len() + 2,
    ))?;
    let mut args: Vec<&dyn rusqlite::ToSql> = keys.iter().map(|k| k as &dyn rusqlite::ToSql).collect();
    args.push(&before);
    args.push(&limit);
    let rows = stmt.query_map(args.as_slice(), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    rows.map(|row| {
        let (workout_id, date, sets) = row?;
        let lines = parse_sets(&sets);
        let stats = Stats::of(&lines);
        Ok(ExerciseLog {
            workout_id,
            date,
            sets,
            lines,
            top_weight: stats.top_weight,
            volume: stats.volume,
            set_count: stats.set_count,
            distance: stats.distance,
            duration: stats.duration,
        })
    })
    .collect()
}

pub(crate) fn name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Default)]
struct Stats {
    top_weight: Option<f64>,
    volume: f64,
    set_count: u32,
    distance: f64,
    duration: u32,
}

impl Stats {
    fn of(lines: &[SetLine]) -> Self {
        let mut stats = Self::default();
        for line in lines.iter().filter(|l| l.parsed && !l.warmup) {
            stats.set_count += line.reps.len() as u32;
            stats.distance += line.distance.unwrap_or(0.0);
            stats.duration += line.duration.unwrap_or(0);
            if let Some(w) = line.weight {
                stats.top_weight = Some(stats.top_weight.map_or(w, |t: f64| t.max(w)));
                stats.volume += w * line.reps.iter().sum::<u32>() as f64;
            }
        }
        stats
    }
}

// ---- parsing ------------------------------------------------------------------------------------

/// Reads each non-empty line of an exercise's sets.
pub fn parse_sets(text: &str) -> Vec<SetLine> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(parse_line)
        .collect()
}

/// The same sets with each repeated set on a line of its own: `24kg x 10 x 3` becomes three
/// `24kg x 10` lines, `E 18kg x (10 + 8)` two warm-up lines. Notes stay on the last copy; cardio,
/// single sets and lines that aren't sets are kept as typed.
pub fn expand_sets(text: &str) -> String {
    let mut out = Vec::new();
    for raw in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let line = parse_line(raw);
        if !line.parsed || line.reps.len() < 2 {
            out.push(raw.to_owned());
            continue;
        }
        let weight = match line.weight {
            Some(w) => format!("{}{}", number(w), line.unit),
            None => "bw".to_owned(),
        };
        let last = line.reps.len() - 1;
        for (i, reps) in line.reps.iter().enumerate() {
            let mut set = format!(
                "{}{}{weight} x {reps}",
                if line.done { DONE_MARK } else { "" },
                if line.warmup { "E " } else { "" }
            );
            if i == last && let Some(note) = &line.note {
                set.push_str(&format!(" \" {note}"));
            }
            out.push(set);
        }
    }
    out.join("\n")
}

/// Quotes that start a note on a set line, straight or curly as keyboards type them.
const NOTE_QUOTES: [char; 7] = ['"', '\'', '“', '”', '‘', '’', '«'];

fn parse_line(raw: &str) -> SetLine {
    let Some(body) = raw.strip_prefix(['✓', '✔']).map(str::trim_start) else {
        return parse_unmarked(raw);
    };
    let mut line = parse_unmarked(body);
    if !line.parsed {
        line.label = body.to_owned();
    }
    line.raw = raw.to_owned();
    line.done = true;
    line
}

fn parse_unmarked(raw: &str) -> SetLine {
    // `28kg x 9 " spotted on the first`: sets, then a note. Only when the part before the quote
    // reads as sets, so a plain note with an apostrophe stays whole.
    if let Some(at) = raw.find(NOTE_QUOTES) {
        let head = raw[..at].trim();
        let note = raw[at..]
            .trim_start_matches(NOTE_QUOTES)
            .trim_end_matches(NOTE_QUOTES)
            .trim_end_matches('»')
            .trim();
        let mut line = parse_parts(raw, head);
        if line.parsed {
            line.note = (!note.is_empty()).then(|| note.to_owned());
            return line;
        }
        if head.is_empty() && !note.is_empty() {
            // A quoted line is a note on its own.
            line.label = note.to_owned();
            return line;
        }
    }
    parse_parts(raw, raw)
}

/// Reads `text` (all or part of the line `raw`) as sets or cardio.
fn parse_parts(raw: &str, text: &str) -> SetLine {
    let unparsed = || SetLine {
        raw: raw.to_owned(),
        parsed: false,
        warmup: false,
        weight: None,
        unit: String::new(),
        reps: Vec::new(),
        distance: None,
        duration: None,
        note: None,
        label: raw.to_owned(),
        done: false,
    };
    let (warmup, rest) = strip_warmup(text);
    if let Some((weight, unit, reps)) = tokenize(rest).and_then(|t| read_sets(&t)) {
        return SetLine {
            raw: raw.to_owned(),
            parsed: true,
            label: label(warmup, weight, &unit, &reps),
            warmup,
            weight,
            unit,
            reps,
            distance: None,
            duration: None,
            note: None,
            done: false,
        };
    }
    if let Some(cardio) = read_cardio(rest) {
        let body = cardio.label();
        return SetLine {
            raw: raw.to_owned(),
            parsed: true,
            warmup,
            weight: None,
            unit: cardio.unit.to_owned(),
            reps: Vec::new(),
            distance: cardio.metres,
            duration: cardio.seconds,
            note: None,
            label: if warmup {
                format!("Warm-up · {body}")
            } else {
                body
            },
            done: false,
        };
    }
    unparsed()
}

struct Cardio {
    metres: Option<f64>,
    /// As typed: `m`, `km` or `mi`; empty without a distance.
    unit: &'static str,
    seconds: Option<u32>,
}

impl Cardio {
    /// `500 m · 14 min · 2:48 /100 m`, `11.6 km · 1 h 30 · 7:46 /km`.
    fn label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(m) = self.metres {
            parts.push(match self.unit {
                "km" => format!("{} km", number(m / 1000.0)),
                "mi" => format!("{} mi", number(m / MILE)),
                _ => format!("{} m", number(m)),
            });
        }
        if let Some(s) = self.seconds {
            parts.push(duration_label(s));
        }
        if let (Some(m), Some(s)) = (self.metres, self.seconds)
            && m > 0.0
        {
            // Swims read per 100 m, runs and rides per km (or mile).
            let (per, unit) = match self.unit {
                "mi" => (MILE, "/mi"),
                "m" if m <= 5000.0 => (100.0, "/100 m"),
                _ => (1000.0, "/km"),
            };
            let pace = (f64::from(s) * per / m).round() as u32;
            parts.push(format!("{}:{:02} {unit}", pace / 60, pace % 60));
        }
        parts.join(" · ")
    }
}

const MILE: f64 = 1609.344;

fn duration_label(s: u32) -> String {
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    match (h, m, sec) {
        (0, 0, sec) => format!("{sec} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, sec) => format!("{m} min {sec} s"),
        (h, 0, _) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m:02}"),
    }
}

/// A distance and/or a duration in any order: `500m 14min`, `11,6 km en 1h30`, `45min`,
/// `5k 25:30`.
fn read_cardio(line: &str) -> Option<Cardio> {
    let lower = line.to_lowercase().replace('·', " ");
    // Glue a number to the unit after it ("14 min" → "14min"), and read decimal commas.
    let mut words: Vec<String> = Vec::new();
    for w in lower.split([' ', '\t', ';']).filter(|w| !w.is_empty()) {
        let w = w.trim_end_matches(',');
        match words.last_mut() {
            Some(prev)
                if prev.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',')
                    && w.starts_with(char::is_alphabetic) =>
            {
                prev.push_str(w)
            }
            _ => words.push(w.to_owned()),
        }
    }
    let mut cardio = Cardio {
        metres: None,
        unit: "",
        seconds: None,
    };
    for word in &words {
        let word = word.replace(',', ".");
        if matches!(word.as_str(), "en" | "in" | "@" | "-") {
            continue;
        }
        if cardio.metres.is_none()
            && let Some((metres, unit)) = distance(&word)
        {
            cardio.metres = Some(metres);
            cardio.unit = unit;
        } else if cardio.seconds.is_none()
            && let Some(s) = duration(&word)
        {
            cardio.seconds = Some(s);
        } else {
            return None;
        }
    }
    (cardio.metres.is_some() || cardio.seconds.is_some()).then_some(cardio)
}

/// Splits `11.6km` into (11.6, "km").
fn split_number(word: &str) -> Option<(f64, &str)> {
    let end = word
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(word.len());
    let n: f64 = word[..end].parse().ok()?;
    (n > 0.0).then_some((n, &word[end..]))
}

fn distance(word: &str) -> Option<(f64, &'static str)> {
    let (n, unit) = split_number(word)?;
    match unit {
        "m" => Some((n, "m")),
        "km" | "k" => Some((n * 1000.0, "km")),
        "mi" | "mile" | "miles" => Some((n * MILE, "mi")),
        _ => None,
    }
}

/// `14min`, `45s`, `1h30`, `1h30min`, `1h`, `14min30`, `25:30`, `1:02:03`.
fn duration(word: &str) -> Option<u32> {
    if word.contains(':') {
        let parts: Vec<u32> = word.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        return match parts[..] {
            [m, s] if s < 60 => Some(m * 60 + s),
            [h, m, s] if m < 60 && s < 60 => Some(h * 3600 + m * 60 + s),
            _ => None,
        };
    }
    let (n, rest) = split_number(word)?;
    if n.fract() != 0.0 {
        return None;
    }
    let n = n as u32;
    let trailing = |rest: &str, unit: u32| -> Option<u32> {
        if rest.is_empty() {
            return Some(0);
        }
        let (m, tail) = split_number(rest)?;
        (m.fract() == 0.0 && matches!(tail, "" | "min" | "mn" | "m" | "s" | "sec")).then_some(m as u32 * unit)
    };
    if let Some(rest) = rest.strip_prefix('h') {
        return Some(n * 3600 + trailing(rest, 60)?);
    }
    for unit in ["min", "mn", "mins"] {
        if let Some(rest) = rest.strip_prefix(unit)
            && (rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_digit()))
        {
            return Some(n * 60 + trailing(rest, 1)?);
        }
    }
    matches!(rest, "s" | "sec").then_some(n)
}

/// `E 18kg x 10`, `W: …`, `warm-up …`, `échauffement …`.
fn strip_warmup(line: &str) -> (bool, &str) {
    let word_end = line
        .find(|c: char| !(c.is_alphabetic() || c == '-'))
        .unwrap_or(line.len());
    let word = line[..word_end].to_lowercase();
    let after = &line[word_end..];
    let separated = after.starts_with([' ', ':', '\t']);
    let warm = matches!(
        word.as_str(),
        "e" | "w" | "wu" | "warmup" | "warm-up" | "échauffement" | "echauffement" | "échauf" | "echauf"
    );
    if warm && separated {
        (true, after.trim_start_matches([' ', ':', '\t']))
    } else {
        (false, line)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Unit(&'static str),
    Bodyweight,
    Times,
    Plus,
    Open,
    Close,
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => i += 1,
            'x' | 'X' | '×' | '*' => {
                // `x` only as an operator, not the start of a word.
                if c.is_alphabetic() && chars.get(i + 1).is_some_and(|n| n.is_alphabetic()) {
                    return None;
                }
                out.push(Tok::Times);
                i += 1;
            }
            '+' | ',' | ';' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '(' => {
                out.push(Tok::Open);
                i += 1;
            }
            ')' => {
                out.push(Tok::Close);
                i += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                // `24,8kg`: a decimal comma, only when a unit follows the digits.
                if chars.get(i) == Some(&',') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    let mut k = j;
                    while chars.get(k) == Some(&' ') {
                        k += 1;
                    }
                    if unit_at(&chars, k).is_some() {
                        i = j;
                    }
                }
                let text: String = chars[start..i].iter().collect::<String>().replace(',', ".");
                out.push(Tok::Num(text.parse().ok()?));
            }
            _ if c.is_alphabetic() => {
                if let Some((unit, len)) = unit_at(&chars, i) {
                    out.push(Tok::Unit(unit));
                    i += len;
                    continue;
                }
                let start = i;
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
                match word.as_str() {
                    "bw" | "pdc" => out.push(Tok::Bodyweight),
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// A weight unit starting at `i` and its length; it must end the word.
fn unit_at(chars: &[char], i: usize) -> Option<(&'static str, usize)> {
    for (spelling, unit) in [("kgs", "kg"), ("kg", "kg"), ("lbs", "lb"), ("lb", "lb")] {
        let len = spelling.chars().count();
        let Some(slice) = chars.get(i..i + len) else {
            continue;
        };
        let word: String = slice.iter().collect::<String>().to_lowercase();
        if word == spelling && !chars.get(i + len).is_some_and(|c| c.is_alphabetic()) {
            return Some((unit, len));
        }
    }
    None
}

/// `[weight x] reps`, where reps is `10`, `10 x 3`, `10, 10, 8`, `(10 x 2 + 8)` and so on.
fn read_sets(tokens: &[Tok]) -> Option<(Option<f64>, String, Vec<u32>)> {
    let (weight, unit, rest) = match tokens {
        [Tok::Num(w), Tok::Unit(u), Tok::Times, rest @ ..] => (Some(*w), (*u).to_owned(), rest),
        [Tok::Bodyweight, Tok::Times, rest @ ..] => (None, String::new(), rest),
        // No unit: `50 x 10 x 3` is a weight, `10 x 3` is reps times sets.
        [Tok::Num(w), Tok::Times, rest @ ..]
            if rest
                .iter()
                .any(|t| matches!(t, Tok::Times | Tok::Open | Tok::Plus)) =>
        {
            (Some(*w), "kg".to_owned(), rest)
        }
        _ => (None, String::new(), tokens),
    };
    let mut parser = Reps { tokens: rest, at: 0 };
    let reps = parser.sum()?;
    (parser.at == rest.len() && !reps.is_empty()).then_some((weight, unit, reps))
}

struct Reps<'a> {
    tokens: &'a [Tok],
    at: usize,
}

impl Reps<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at)
    }

    fn sum(&mut self) -> Option<Vec<u32>> {
        let mut out = self.term()?;
        while self.peek() == Some(&Tok::Plus) {
            self.at += 1;
            out.extend(self.term()?);
            if out.len() > MAX_SETS {
                return None;
            }
        }
        Some(out)
    }

    fn term(&mut self) -> Option<Vec<u32>> {
        let atom = match self.peek()? {
            Tok::Num(n) => {
                let n = whole(*n)?;
                self.at += 1;
                vec![n]
            }
            Tok::Open => {
                self.at += 1;
                let inner = self.sum()?;
                (self.peek() == Some(&Tok::Close)).then_some(())?;
                self.at += 1;
                inner
            }
            _ => return None,
        };
        if self.peek() != Some(&Tok::Times) {
            return Some(atom);
        }
        self.at += 1;
        let Some(Tok::Num(times)) = self.peek() else {
            return None;
        };
        let times = whole(*times)? as usize;
        self.at += 1;
        if times == 0 || atom.len() * times > MAX_SETS {
            return None;
        }
        Some(atom.iter().copied().cycle().take(atom.len() * times).collect())
    }
}

fn whole(n: f64) -> Option<u32> {
    (n.fract() == 0.0 && (1.0..=10_000.0).contains(&n)).then_some(n as u32)
}

fn number(n: f64) -> String {
    let s = format!("{n:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn label(warmup: bool, weight: Option<f64>, unit: &str, reps: &[u32]) -> String {
    let uniform = reps.iter().all(|r| *r == reps[0]);
    let list = || reps.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
    let body = match (weight, reps) {
        (Some(w), [one]) => format!("{} {unit} × {one}", number(w)),
        (Some(w), _) if uniform => format!("{} {unit} × {} × {}", number(w), reps[0], reps.len()),
        (Some(w), _) => format!("{} {unit} × {}", number(w), list()),
        (None, [one]) => format!("{one} reps"),
        (None, _) if uniform => format!("{} reps × {}", reps[0], reps.len()),
        (None, _) => format!("{} reps", list()),
    };
    if warmup {
        format!("Warm-up · {body}")
    } else {
        body
    }
}

struct LoggedSession {
    date: Date,
    exercises: Vec<(String, String)>,
}

fn parse_log(text: &str, today: Date) -> Vec<LoggedSession> {
    let mut sessions: Vec<LoggedSession> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(date) = log_date(line, today) {
            sessions.push(LoggedSession {
                date,
                exercises: Vec::new(),
            });
            continue;
        }
        let Some(session) = sessions.last_mut() else {
            continue;
        };
        // A quoted line is a note for the exercise above, not a new one.
        let reads_as_sets = parse_line(line).parsed || line.starts_with(NOTE_QUOTES);
        match session.exercises.last_mut() {
            Some((_, sets)) if reads_as_sets => {
                if !sets.is_empty() {
                    sets.push('\n');
                }
                sets.push_str(line);
            }
            _ => session.exercises.push((line.to_owned(), String::new())),
        }
    }
    sessions
}

/// `30/10`, `30/10/25` or `30/10/2025` (day first).
fn log_date(line: &str, today: Date) -> Option<Date> {
    let parts: Vec<&str> = line.split(['/', '.']).collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let day: i64 = parts[0].parse().ok()?;
    let month: i64 = parts[1].parse().ok()?;
    match parts.get(2) {
        Some(y) => {
            let y: i64 = y.parse().ok()?;
            Date::from_ymd(if y < 100 { 2000 + y } else { y }, month, day)
        }
        None => {
            let this_year = Date::from_ymd(today.year(), month, day)?;
            if this_year <= today {
                Some(this_year)
            } else {
                Date::from_ymd(today.year() - 1, month, day)
            }
        }
    }
}

// ---- projection ---------------------------------------------------------------------------------

pub fn rebuild(tree: &LoroTree, db: &Connection) -> Result<()> {
    db.execute_batch("DELETE FROM workout; DELETE FROM workout_exercise;")?;
    let mut insert_workout =
        db.prepare_cached("INSERT INTO workout (id, date, title, position) VALUES (?1, ?2, ?3, ?4)")?;
    let mut insert_exercise = db.prepare_cached(
        "INSERT INTO workout_exercise (id, workout_id, name, name_key, sets, position, set_count, done)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for session in tree.roots() {
        if !crate::projection::is_alive(tree, session) {
            continue;
        }
        let meta = tree.get_meta(session)?;
        let Some(date) = meta_str(&meta, DATE) else {
            continue;
        };
        insert_workout.execute(params![
            session.to_string(),
            date,
            meta_str(&meta, TITLE).unwrap_or_default(),
            tree.fractional_index(session).unwrap_or_default(),
        ])?;
        for exercise in tree.children(session).unwrap_or_default() {
            if !crate::projection::is_alive(tree, exercise) {
                continue;
            }
            let meta = tree.get_meta(exercise)?;
            let name = meta_str(&meta, NAME).unwrap_or_default();
            let sets = meta_str(&meta, SETS).unwrap_or_default();
            insert_exercise.execute(params![
                exercise.to_string(),
                session.to_string(),
                name,
                name_key(&name),
                sets,
                tree.fractional_index(exercise).unwrap_or_default(),
                Stats::of(&parse_sets(&sets)).set_count,
                matches!(
                    meta.get(DONE),
                    Some(loro::ValueOrContainer::Value(loro::LoroValue::Bool(true)))
                ),
            ])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(s: &str) -> SetLine {
        parse_line(s)
    }

    #[test]
    fn reads_the_usual_shorthand() {
        let l = line("66kg x 8 x 3");
        assert!(l.parsed && !l.warmup);
        assert_eq!(
            (l.weight, l.unit.as_str(), l.reps.clone()),
            (Some(66.0), "kg", vec![8, 8, 8])
        );
        assert_eq!(l.label, "66 kg × 8 × 3");

        let l = line("24.8kg x (10 x 2 + 8)");
        assert_eq!((l.weight, l.reps.clone()), (Some(24.8), vec![10, 10, 8]));
        assert_eq!(l.label, "24.8 kg × 10, 10, 8");

        let l = line("E 18kg x 10");
        assert!(l.warmup);
        assert_eq!((l.weight, l.reps.clone()), (Some(18.0), vec![10]));
        assert_eq!(l.label, "Warm-up · 18 kg × 10");

        assert_eq!(line("24,8 kg × 10, 10, 8").reps, vec![10, 10, 8]);
        assert_eq!(line("24,8kg x 10").weight, Some(24.8));
        assert_eq!(line("50 x 10 x 3").weight, Some(50.0));
        let bw = line("20 x 3");
        assert_eq!(
            (bw.weight, bw.reps.clone(), bw.label.as_str()),
            (None, vec![20, 20, 20], "20 reps × 3")
        );
        assert_eq!(line("bw x 12").label, "12 reps");
        assert_eq!(line("(10 + 8) x 2").reps, vec![10, 8, 10, 8]);
    }

    #[test]
    fn expands_repeated_sets_one_per_line() {
        assert_eq!(expand_sets("66kg x 8 x 3"), "66kg x 8\n66kg x 8\n66kg x 8");
        assert_eq!(expand_sets("E 18kg x (10 + 8)"), "E 18kg x 10\nE 18kg x 8");
        assert_eq!(expand_sets("24,8kg x 10 x 2 \" last one hard"), "24.8kg x 10\n24.8kg x 10 \" last one hard");
        assert_eq!(expand_sets("20 x 3"), "bw x 20\nbw x 20\nbw x 20");
        assert_eq!(expand_sets("26kg x 8\nfelt strong\n500m 14min"), "26kg x 8\nfelt strong\n500m 14min");
        // Each copy reads back as one set of the same weight and reps.
        for line in parse_sets(&expand_sets("24.8kg x (10 x 2 + 8)")) {
            assert_eq!((line.weight, line.reps.len()), (Some(24.8), 1));
        }
        assert_eq!(parse_sets(&expand_sets("20 x 3"))[0].reps, vec![20]);
    }

    #[test]
    fn done_sets_carry_a_mark() {
        let l = line("✓ 24kg x 10");
        assert!(l.done && l.parsed);
        assert_eq!((l.weight, l.label.as_str()), (Some(24.0), "24 kg × 10"));
        assert!(line("✔ E 18kg x 10").warmup);
        assert!(!line("24kg x 10").done);
        assert_eq!(expand_sets("✓ 24kg x 10 x 2"), "✓ 24kg x 10\n✓ 24kg x 10");
        let note = line("✓ felt strong");
        assert!(note.done && !note.parsed && note.label == "felt strong");
    }

    #[test]
    fn reads_cardio() {
        let l = line("500m 14min");
        assert!(l.parsed && l.reps.is_empty());
        assert_eq!((l.distance, l.duration), (Some(500.0), Some(840)));
        assert_eq!(l.label, "500 m · 14 min · 2:48 /100 m");

        let l = line("11.6km 1h30");
        assert_eq!((l.distance, l.duration), (Some(11_600.0), Some(5400)));
        assert_eq!(l.label, "11.6 km · 1 h 30 · 7:46 /km");

        assert_eq!(line("11,6 km en 1h30").duration, Some(5400));
        assert_eq!(line("45 min").label, "45 min");
        assert_eq!(line("5k 25:30").label, "5 km · 25 min 30 s · 5:06 /km");
        assert_eq!(line("1500m").label, "1500 m");
        assert_eq!(line("E 1km 6min").label, "Warm-up · 1 km · 6 min · 6:00 /km");
        assert!(!line("10 min felt good").parsed);
        // Cardio has no sets.
        assert_eq!(Stats::of(&parse_sets("500m 14min")).set_count, 0);
    }

    #[test]
    fn quotes_start_a_note() {
        let l = line("28kg x 9 \" aide au premier");
        assert!(l.parsed);
        assert_eq!(
            (l.reps.clone(), l.note.as_deref()),
            (vec![9], Some("aide au premier"))
        );
        assert_eq!(l.label, "28 kg × 9");
        assert_eq!(
            line("500m 14min ' eau froide").note.as_deref(),
            Some("eau froide")
        );
        assert_eq!(line("24kg x 10 “dur”").note.as_deref(), Some("dur"));
        assert_eq!(line("24kg x 10 \"").note, None);
        // An apostrophe in a plain note keeps it whole.
        let n = line("pas l'énergie");
        assert!(!n.parsed);
        assert_eq!(n.label, "pas l'énergie");
        let q = line("\" genou sensible");
        assert!(!q.parsed);
        assert_eq!(q.label, "genou sensible");
        // Notes don't change the stats.
        assert_eq!(Stats::of(&parse_sets("28kg x 9 \" aide")).set_count, 1);
    }

    #[test]
    fn keeps_anything_else_as_a_note() {
        for s in ["felt heavy", "Abdos", "66kg x", "x 3", "10 x 1000", "Extension"] {
            let l = line(s);
            assert!(!l.parsed, "{s}");
            assert_eq!(l.label, s);
        }
    }

    #[test]
    fn stats_skip_warmups() {
        let stats = Stats::of(&parse_sets("E 18kg x 10\n24kg x 10 x 3"));
        assert_eq!(
            (stats.top_weight, stats.volume, stats.set_count),
            (Some(24.0), 720.0, 3)
        );
    }

    #[test]
    fn reads_a_text_log() {
        let today = Date::parse("2026-11-09").unwrap();
        let log = parse_log(
            "30/10\n\nTirage vertical (large)\n66kg x 8 x 3\n\n02/11\nBench Haltères\nE 18kg x 10\n24kg x 10 x 3\n\nAbdos\n",
            today,
        );
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].date.to_string(), "2026-10-30");
        assert_eq!(
            log[0].exercises,
            vec![("Tirage vertical (large)".into(), "66kg x 8 x 3".into())]
        );
        let noted = parse_log("01/11\nDips\n50kg x 10 \" lent\n\" coude\n", today);
        assert_eq!(
            noted[0].exercises,
            vec![("Dips".into(), "50kg x 10 \" lent\n\" coude".into())]
        );
        assert_eq!(
            log[1].exercises,
            vec![
                ("Bench Haltères".into(), "E 18kg x 10\n24kg x 10 x 3".into()),
                ("Abdos".into(), String::new())
            ]
        );
        // A date later in the year than today is last year's.
        assert_eq!(log_date("24/12", today).unwrap().to_string(), "2025-12-24");
    }
}
