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
    /// Reps of each set, in order: `10 x 3` is `[10, 10, 10]`.
    pub reps: Vec<u32>,
    /// Tidy form for display, e.g. `24 kg × 10 × 3`; the raw text when not parsed.
    pub label: String,
}

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
            let siblings: Vec<TreeID> = self.live_children(workout).into_iter().filter(|&s| s != id).collect();
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
        let mut stmt = inner.db.prepare_cached(
            "SELECT id, date, title FROM workout ORDER BY date DESC, position DESC, id",
        )?;
        let sessions = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
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
            "SELECT id, name, name_key, sets FROM workout_exercise WHERE workout_id = ?1 ORDER BY position, id",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let exercises = rows
            .into_iter()
            .map(|(eid, name, key, sets)| {
                Ok(WorkoutExercise {
                    id: eid,
                    name,
                    lines: parse_sets(&sets),
                    sets,
                    previous: history(&inner.db, &key, Some(&date), 1)?.into_iter().next(),
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

    /// Sessions with this exercise (matched by name), latest first, at most `limit`.
    pub fn exercise_history(&self, name: &str, limit: u32) -> Result<Vec<ExerciseLog>> {
        let inner = self.lock();
        history(&inner.db, &name_key(name), None, limit)
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

/// Logs of exercise `key` with sets, latest first; only sessions before `before` when given.
fn history(db: &Connection, key: &str, before: Option<&str>, limit: u32) -> Result<Vec<ExerciseLog>> {
    let mut stmt = db.prepare_cached(
        "SELECT w.id, w.date, e.sets FROM workout_exercise e JOIN workout w ON w.id = e.workout_id
         WHERE e.name_key = ?1 AND e.sets != '' AND (?2 IS NULL OR w.date < ?2)
         ORDER BY w.date DESC, w.position DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![key, before, limit], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
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
        })
    })
    .collect()
}

fn name_key(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

#[derive(Default)]
struct Stats {
    top_weight: Option<f64>,
    volume: f64,
    set_count: u32,
}

impl Stats {
    fn of(lines: &[SetLine]) -> Self {
        let mut stats = Self::default();
        for line in lines.iter().filter(|l| l.parsed && !l.warmup) {
            stats.set_count += line.reps.len() as u32;
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
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(parse_line).collect()
}

fn parse_line(raw: &str) -> SetLine {
    let unparsed = || SetLine {
        raw: raw.to_owned(),
        parsed: false,
        warmup: false,
        weight: None,
        unit: String::new(),
        reps: Vec::new(),
        label: raw.to_owned(),
    };
    let (warmup, rest) = strip_warmup(raw);
    let Some(tokens) = tokenize(rest) else {
        return unparsed();
    };
    let Some((weight, unit, reps)) = read_sets(&tokens) else {
        return unparsed();
    };
    let label = label(warmup, weight, &unit, &reps);
    SetLine {
        raw: raw.to_owned(),
        parsed: true,
        warmup,
        weight,
        unit,
        reps,
        label,
    }
}

/// `E 18kg x 10`, `W: …`, `warm-up …`, `échauffement …`.
fn strip_warmup(line: &str) -> (bool, &str) {
    let word_end = line.find(|c: char| !(c.is_alphabetic() || c == '-')).unwrap_or(line.len());
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
        let Some(slice) = chars.get(i..i + len) else { continue };
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
        [Tok::Num(w), Tok::Times, rest @ ..] if rest.iter().any(|t| matches!(t, Tok::Times | Tok::Open | Tok::Plus)) => {
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
    if warmup { format!("Warm-up · {body}") } else { body }
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
        let reads_as_sets = parse_line(line).parsed;
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
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
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
            if this_year <= today { Some(this_year) } else { Date::from_ymd(today.year() - 1, month, day) }
        }
    }
}

// ---- projection ---------------------------------------------------------------------------------

pub fn rebuild(tree: &LoroTree, db: &Connection) -> Result<()> {
    db.execute_batch("DELETE FROM workout; DELETE FROM workout_exercise;")?;
    let mut insert_workout =
        db.prepare_cached("INSERT INTO workout (id, date, title, position) VALUES (?1, ?2, ?3, ?4)")?;
    let mut insert_exercise = db.prepare_cached(
        "INSERT INTO workout_exercise (id, workout_id, name, name_key, sets, position, set_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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
        assert_eq!((l.weight, l.unit.as_str(), l.reps.clone()), (Some(66.0), "kg", vec![8, 8, 8]));
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
        assert_eq!((bw.weight, bw.reps.clone(), bw.label.as_str()), (None, vec![20, 20, 20], "20 reps × 3"));
        assert_eq!(line("bw x 12").label, "12 reps");
        assert_eq!(line("(10 + 8) x 2").reps, vec![10, 8, 10, 8]);
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
        assert_eq!((stats.top_weight, stats.volume, stats.set_count), (Some(24.0), 720.0, 3));
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
        assert_eq!(log[0].exercises, vec![("Tirage vertical (large)".into(), "66kg x 8 x 3".into())]);
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
