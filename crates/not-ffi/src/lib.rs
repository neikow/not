//! uniffi surface of `not-core` for the Swift (macOS) and Kotlin (Android) apps.
//!
//! Naming avoids Swift's `Task`: records are `TaskList` / `TaskItem`.
//!
//! Network calls are `async` (Swift `async`, Kotlin `suspend`). They run on the store's own tokio
//! runtime; the foreign side only awaits the result.

use std::sync::{Arc, Mutex};

use not_core::{Credentials, LiveSync, Store, SyncClient, SyncStatus};

uniffi::setup_scaffolding!();

/// Built-in views (ADR 0014). Inbox: untagged tasks.
#[uniffi::export]
pub fn inbox_id() -> String {
    not_core::INBOX.into()
}

/// Tasks starting or due on or before today.
#[uniffi::export]
pub fn today_view_id() -> String {
    not_core::TODAY.into()
}

/// Tasks starting or due after today; always by date.
#[uniffi::export]
pub fn upcoming_view_id() -> String {
    not_core::UPCOMING.into()
}

#[uniffi::export]
pub fn someday_view_id() -> String {
    not_core::SOMEDAY_VIEW.into()
}

/// "Today", "Tomorrow 9:00", "Fri", "12 Nov" for a date (`YYYY-MM-DD`) seen on `today`.
#[uniffi::export]
pub fn due_label(due: String, due_time: Option<u32>, today: String) -> Result<String, NotError> {
    Ok(not_core::due_label(date(&due)?, due_time, date(&today)?))
}

/// "Daily", "Every 2 weeks on Mon", "Monthly on the last Fri", "Every 3 days after done" for a
/// task's `repeat` rule; `None` if it isn't one this app reads.
#[uniffi::export]
pub fn repeat_label(rule: String) -> Option<String> {
    not_core::Repeat::parse_rrule(&rule).ok().map(|r| r.label())
}

/// A tag as stored: lowercase, no `#`, spaces as `-`. Empty when nothing is left.
#[uniffi::export]
pub fn normalize_tag(tag: String) -> String {
    not_core::normalize_tag(&tag)
}

fn date(s: &str) -> Result<not_core::date::Date, NotError> {
    Ok(not_core::date::Date::parse(s)?)
}

fn schedule(
    start: Option<String>,
    time: Option<u32>,
    deadline: Option<String>,
    someday: bool,
    repeat: Option<String>,
) -> Result<not_core::Schedule, NotError> {
    Ok(not_core::Schedule {
        start: start.as_deref().map(date).transpose()?,
        time,
        deadline: deadline.as_deref().map(date).transpose()?,
        someday,
        repeat: repeat.as_deref().map(not_core::Repeat::parse_rrule).transpose()?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TokenKind {
    /// The start date.
    Date,
    Deadline,
    Time,
    Repeat,
    Someday,
    Priority,
    Tag,
}

/// A recognised piece of quick-add input. Offsets are UTF-16 code units.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ParsedToken {
    pub start: u32,
    pub end: u32,
    pub kind: TokenKind,
    /// Pass back in `dismissed` to keep this text in the title.
    pub text: String,
}

/// What quick add will create from some input.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ParsedTask {
    pub title: String,
    pub start: Option<String>,
    pub time: Option<u32>,
    pub deadline: Option<String>,
    pub someday: bool,
    pub repeat: Option<String>,
    pub priority: u8,
    pub tags: Vec<String>,
    pub tokens: Vec<ParsedToken>,
}

/// Habit `days` mask for a daily habit. Bit 0 = Monday … bit 6 = Sunday.
#[uniffi::export]
pub fn every_day() -> u8 {
    not_core::EVERY_DAY
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MarkdownStyle {
    /// Level 1–3.
    Heading {
        level: u8,
    },
    Bold,
    Italic,
    Code,
    Strike,
    Quote,
    Bullet,
    Checkbox {
        checked: bool,
    },
    /// Text of a checked checklist item.
    Done,
    /// Markup characters, drawn dimmed.
    Syntax,
}

/// A range to style, in UTF-16 code units.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MarkdownSpan {
    pub start: u32,
    pub end: u32,
    pub style: MarkdownStyle,
}

/// Styling for live markdown editing: the text stays markdown, the editor styles it in place.
#[uniffi::export]
pub fn markdown_spans(text: String) -> Vec<MarkdownSpan> {
    use not_core::markdown::Style;
    not_core::markdown::spans(&text)
        .into_iter()
        .map(|s| MarkdownSpan {
            start: s.start,
            end: s.end,
            style: match s.style {
                Style::Heading(level) => MarkdownStyle::Heading { level },
                Style::Bold => MarkdownStyle::Bold,
                Style::Italic => MarkdownStyle::Italic,
                Style::Code => MarkdownStyle::Code,
                Style::Strike => MarkdownStyle::Strike,
                Style::Quote => MarkdownStyle::Quote,
                Style::Bullet => MarkdownStyle::Bullet,
                Style::Checkbox { checked } => MarkdownStyle::Checkbox { checked },
                Style::Done => MarkdownStyle::Done,
                Style::Syntax => MarkdownStyle::Syntax,
            },
        })
        .collect()
}

/// An image line in a note: UTF-16 range of the line and the image's id.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NoteImage {
    pub start: u32,
    pub end: u32,
    pub id: String,
}

/// The image lines (`![](not-image:<id>)` alone on a line) of a note, for the editor to draw.
#[uniffi::export]
pub fn markdown_images(text: String) -> Vec<NoteImage> {
    not_core::markdown::images(&text)
        .into_iter()
        .map(|i| NoteImage {
            start: i.start,
            end: i.end,
            id: i.id,
        })
        .collect()
}

/// The line to insert for an image added with `add_image`.
#[uniffi::export]
pub fn image_markup(id: String) -> String {
    not_core::markdown::image_markup(&id)
}

/// What "Make tasks" would take from a note: see [`note_checklist`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct NoteChecklist {
    /// Items found, in order.
    pub count: u32,
    /// Titles in order, for the picker's preview.
    pub titles: Vec<String>,
    /// For a new tag: the nearest heading above the items, else the note's title.
    pub suggested_title: String,
}

/// The checklist items (`- [ ] …`) on the lines the selection `start..end` (UTF-16) touches in
/// `body`; `None` when there are none, so "Make tasks" can hide.
#[uniffi::export]
pub fn note_checklist(body: String, start: u32, end: u32) -> Option<NoteChecklist> {
    not_core::checklist(&body, start, end).map(|c| NoteChecklist {
        count: c.items.len() as u32,
        titles: c.items.into_iter().map(|i| i.title).collect(),
        suggested_title: c.suggested_title,
    })
}

/// What making tasks of a checklist did: `count` new tasks tagged `tag` (`None`: in the Inbox);
/// the note is unchanged.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChecklistMoved {
    pub tag: Option<String>,
    pub count: u32,
}

/// Server URL and one-time code, as carried by a pairing QR code.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingLink {
    pub server_url: String,
    pub code: String,
}

/// The text to put in a pairing QR code: `not://pair?server=…&code=…`.
#[uniffi::export]
pub fn pairing_link(server_url: String, code: String) -> String {
    not_core::PairingLink { server_url, code }.to_string()
}

/// `None` if `text` (e.g. a scanned QR code) isn't a pairing link.
#[uniffi::export]
pub fn parse_pairing_link(text: String) -> Option<PairingLink> {
    not_core::PairingLink::parse(&text).map(|l| PairingLink {
        server_url: l.server_url,
        code: l.code,
    })
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
// Variant fields can't be called `message`: it would clash with Kotlin's `Throwable.message`.
pub enum NotError {
    #[error("not found: {detail}")]
    NotFound { detail: String },
    #[error("invalid: {detail}")]
    Invalid { detail: String },
    #[error("{detail}")]
    Internal { detail: String },
}

impl From<not_core::Error> for NotError {
    fn from(e: not_core::Error) -> Self {
        match e {
            not_core::Error::NotFound(detail) => Self::NotFound { detail },
            not_core::Error::Invalid(detail) => Self::Invalid { detail },
            other => Self::Internal {
                detail: other.to_string(),
            },
        }
    }
}

/// A view: built in (Inbox, Today, Upcoming, Someday) or a saved smart list.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    /// The filter; empty for built-in views. See `check_query`.
    pub query: String,
    pub built_in: bool,
    pub sort: TaskSort,
    /// Open top-level tasks, for badges.
    pub open_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TagInfo {
    pub name: String,
    /// Open tasks carrying it.
    pub open_count: u32,
}

/// Where `snooze` puts tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Snooze {
    /// Three hours from now, or tomorrow 9:00 late in the evening.
    LaterToday,
    /// Today 19:00 (tomorrow once past).
    ThisEvening,
    /// Tomorrow, keeping the time.
    Tomorrow,
    /// The coming Saturday.
    Weekend,
    NextWeek,
}

impl From<Snooze> for not_core::Snooze {
    fn from(s: Snooze) -> Self {
        match s {
            Snooze::LaterToday => Self::LaterToday,
            Snooze::ThisEvening => Self::ThisEvening,
            Snooze::Tomorrow => Self::Tomorrow,
            Snooze::Weekend => Self::Weekend,
            Snooze::NextWeek => Self::NextWeek,
        }
    }
}

/// How a view orders its open tasks; ties keep the manual order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TaskSort {
    /// Drag order; new tasks go first.
    Manual,
    Due,
    Priority,
    Title,
}

impl From<not_core::TaskSort> for TaskSort {
    fn from(s: not_core::TaskSort) -> Self {
        match s {
            not_core::TaskSort::Manual => Self::Manual,
            not_core::TaskSort::Due => Self::Due,
            not_core::TaskSort::Priority => Self::Priority,
            not_core::TaskSort::Title => Self::Title,
        }
    }
}

impl From<TaskSort> for not_core::TaskSort {
    fn from(s: TaskSort) -> Self {
        match s {
            TaskSort::Manual => Self::Manual,
            TaskSort::Due => Self::Due,
            TaskSort::Priority => Self::Priority,
            TaskSort::Title => Self::Title,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TaskItem {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    /// Markdown.
    pub notes: String,
    /// Unix millis.
    pub completed_at: Option<i64>,
    /// Start date `YYYY-MM-DD` (when it's planned; hidden from views before); see `due_label`.
    pub start: Option<String>,
    /// Minutes after midnight, on the start date.
    pub time: Option<u32>,
    /// `YYYY-MM-DD`.
    pub deadline: Option<String>,
    pub someday: bool,
    /// 0 = none, 1 = low, 2 = medium, 3 = high.
    pub priority: u8,
    /// Repeat rule; see `repeat_label`.
    pub repeat: Option<String>,
    /// Its own tags (subtasks also show in their parent's tag views).
    pub tags: Vec<String>,
}

/// A task in display order with its nesting depth.
#[derive(Debug, Clone, uniffi::Record)]
pub struct OutlineRow {
    pub task: TaskItem,
    pub depth: u32,
    /// The task or one of its ancestors is completed.
    pub done: bool,
    /// Direct subtasks, and how many of those are completed.
    pub subtasks: u32,
    pub subtasks_done: u32,
}

/// A view as shown: open tasks with all their subtasks, then completed ones (latest first).
#[derive(Debug, Clone, uniffi::Record)]
pub struct Outline {
    pub open: Vec<OutlineRow>,
    pub completed: Vec<OutlineRow>,
}

/// What a habit's goal counts over. Weeks run Monday to Sunday, months by the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum HabitPeriod {
    Day,
    Week,
    Month,
}

impl From<not_core::Period> for HabitPeriod {
    fn from(p: not_core::Period) -> Self {
        match p {
            not_core::Period::Day => Self::Day,
            not_core::Period::Week => Self::Week,
            not_core::Period::Month => Self::Month,
        }
    }
}

impl From<HabitPeriod> for not_core::Period {
    fn from(p: HabitPeriod) -> Self {
        match p {
            HabitPeriod::Day => Self::Day,
            HabitPeriod::Week => Self::Week,
            HabitPeriod::Month => Self::Month,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Habit {
    pub id: String,
    pub title: String,
    /// Check-ins per period.
    pub goal: u32,
    /// Scheduled weekdays: bit 0 = Monday … bit 6 = Sunday. All of them for weekly and monthly habits.
    pub days: u8,
    pub period: HabitPeriod,
    /// Set for timed habits: `goal` is then minutes per day.
    pub timed: Option<TimedRule>,
}

/// How a timed habit is judged and worded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct TimedRule {
    /// The goal caps time off ("at most 2 h off") instead of asking for time on.
    pub at_most_off: bool,
    /// Normally on (worn, fasting): offer Pause/Resume and watch the time off. Normally off
    /// (reading): Start/Stop, and watch the time on.
    pub usual_on: bool,
    /// Minutes out of the usual state before a reminder; 0 for none.
    pub nudge_minutes: u32,
}

impl From<TimedRule> for not_core::TimedRule {
    fn from(r: TimedRule) -> Self {
        Self {
            at_most_off: r.at_most_off,
            usual_on: r.usual_on,
            nudge_minutes: r.nudge_minutes,
        }
    }
}

impl From<not_core::TimedRule> for TimedRule {
    fn from(r: not_core::TimedRule) -> Self {
        Self {
            at_most_off: r.at_most_off,
            usual_on: r.usual_on,
            nudge_minutes: r.nudge_minutes,
        }
    }
}

/// A timed habit on one day, up to now when that day is today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct TimedState {
    /// On at the end of the day (or now); false before tracking starts.
    pub on: bool,
    /// Unix millis the current stretch started; `None` before the first switch.
    pub since: Option<i64>,
    /// Millis on and off that day; untracked time is neither.
    pub on_ms: i64,
    pub off_ms: i64,
}

/// One stretch of a day's timeline, in Unix millis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct TimeSegment {
    pub start: i64,
    pub end: i64,
    pub on: bool,
}

/// A habit as of one day (the user's today).
#[derive(Debug, Clone, uniffi::Record)]
pub struct HabitSummary {
    pub habit: Habit,
    /// Check-ins that day.
    pub count: u32,
    /// Check-ins in the period so far, that day included; `count` for daily habits.
    pub progress: u32,
    /// `progress` reached the goal.
    pub done: bool,
    /// Daily: that day is a scheduled weekday. Weekly/monthly: the goal now needs every day left.
    pub due: bool,
    /// Periods in a row that met the goal (scheduled days, weeks or months); the open one doesn't break it.
    pub streak: u32,
    pub best_streak: u32,
    /// Check-ins of the 7 days ending that day, oldest first.
    pub week: Vec<u32>,
    /// Timed habits: the state that day. `count`, `progress` and `week` are then the minutes
    /// that count toward the goal (on, or off for "at most" habits); `done` = goal met (for
    /// "at most": still within the allowance).
    pub timed: Option<TimedState>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HabitDay {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Folder {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    /// Below the top level, for indenting.
    pub depth: u32,
    /// Notes directly inside.
    pub note_count: u32,
    /// Notes keep the order they were dragged into, instead of latest edit first.
    pub manual_order: bool,
    pub sort: NoteSort,
}

/// How a folder (or All Notes) orders its notes, after the pinned ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NoteSort {
    Edited,
    Created,
    Title,
    /// Folders only.
    Manual,
}

impl From<not_core::NoteSort> for NoteSort {
    fn from(s: not_core::NoteSort) -> Self {
        match s {
            not_core::NoteSort::Edited => Self::Edited,
            not_core::NoteSort::Created => Self::Created,
            not_core::NoteSort::Title => Self::Title,
            not_core::NoteSort::Manual => Self::Manual,
        }
    }
}

impl From<NoteSort> for not_core::NoteSort {
    fn from(s: NoteSort) -> Self {
        match s {
            NoteSort::Edited => Self::Edited,
            NoteSort::Created => Self::Created,
            NoteSort::Title => Self::Title,
            NoteSort::Manual => Self::Manual,
        }
    }
}

/// One line of an exercise's sets, read from shorthand like `24kg x 10 x 3`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SetLine {
    pub raw: String,
    /// False: not read as sets; show `label` (the raw text) as a note.
    pub parsed: bool,
    pub warmup: bool,
    /// `None` for bodyweight.
    pub weight: Option<f64>,
    pub unit: String,
    /// Empty for cardio.
    pub reps: Vec<u32>,
    /// Cardio: metres (`unit` is then `m`, `km` or `mi` as typed).
    pub distance: Option<f64>,
    /// Cardio: seconds.
    pub duration: Option<u32>,
    /// Text after a quote on the line, shown beside the sets.
    pub note: Option<String>,
    /// Tidy form, e.g. `24 kg × 10 × 3`, `500 m · 14 min · 2:48 /100 m`.
    pub label: String,
    /// Ticked off in this session (the line starts with ✓).
    pub done: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct WorkoutSummary {
    pub id: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// May be empty.
    pub title: String,
    pub exercise_names: Vec<String>,
    /// Working sets, warm-ups excluded.
    pub set_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Workout {
    pub id: String,
    pub date: String,
    pub title: String,
    pub exercises: Vec<WorkoutExercise>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct WorkoutExercise {
    pub id: String,
    pub name: String,
    /// As typed, one line per group of sets.
    pub sets: String,
    pub lines: Vec<SetLine>,
    /// Ticked off for this session.
    pub done: bool,
    /// The latest earlier session with this exercise logged.
    pub previous: Option<ExerciseLog>,
}

#[derive(Debug, Clone, uniffi::Record)]
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

/// An exercise from wger's database (CC-BY-SA / CC0; credit wger and `author` where shown).
#[derive(Debug, Clone, uniffi::Record)]
pub struct CatalogExercise {
    pub id: String,
    pub name_en: String,
    pub name_fr: Option<String>,
    pub aliases: Vec<String>,
    pub category: String,
    pub muscles: Vec<String>,
    pub secondary_muscles: Vec<String>,
    pub equipment: Vec<String>,
    pub license: String,
    /// May be empty.
    pub author: String,
}

impl From<not_core::CatalogExercise> for CatalogExercise {
    fn from(e: not_core::CatalogExercise) -> Self {
        Self {
            id: e.id,
            name_en: e.name_en,
            name_fr: e.name_fr,
            aliases: e.aliases,
            category: e.category,
            muscles: e.muscles,
            secondary_muscles: e.secondary_muscles,
            equipment: e.equipment,
            license: e.license,
            author: e.author,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MuscleSets {
    pub muscle: String,
    pub sets: u32,
}

/// Puts each repeated set on its own line: `24kg x 10 x 3` becomes three `24kg x 10` lines.
#[uniffi::export]
pub fn expand_sets(text: String) -> String {
    not_core::expand_sets(&text)
}

/// Reads exercise sets shorthand, for live previews while typing.
#[uniffi::export]
pub fn parse_sets(text: String) -> Vec<SetLine> {
    not_core::parse_sets(&text)
        .into_iter()
        .map(SetLine::from)
        .collect()
}

/// A note without its body.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NoteSummary {
    pub id: String,
    pub folder_id: Option<String>,
    /// First line of the body, without markdown.
    pub title: String,
    /// Following text, shortened.
    pub preview: String,
    pub pinned: bool,
    /// Unix millis.
    pub updated_at: i64,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum SyncState {
    Syncing,
    Synced,
    Offline {
        reason: String,
    },
    /// The server rejected this device (revoked or server reset). Pair again.
    Unauthorized,
}

/// The pairing of this device. The token never leaves the core.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncInfo {
    pub server_url: String,
    pub device_id: i64,
    pub device_name: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingCode {
    /// Formatted for display, e.g. `7KQ2M-X9CHD`.
    pub code: String,
    /// Unix millis.
    pub expires_at: i64,
}

/// A link that shows one list in a browser, to anyone who has it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ListShare {
    pub id: i64,
    pub list_id: String,
    /// Visitors can tick tasks off and add new ones; else read only.
    pub can_edit: bool,
    pub url: String,
    /// Unix millis.
    pub created_at: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Device {
    pub id: i64,
    pub name: String,
    /// Unix millis.
    pub created_at: i64,
    /// Unix millis.
    pub last_seen_at: Option<i64>,
    /// This device.
    pub is_current: bool,
}

/// Implemented by the app. Called from background threads: hop to the UI thread yourself.
#[uniffi::export(with_foreign)]
pub trait StoreObserver: Send + Sync {
    /// The read model changed (local command or pulled updates). Re-query what is on screen.
    fn on_change(&self);
    fn on_sync_state(&self, state: SyncState);
}

type SharedObserver = Arc<Mutex<Option<Arc<dyn StoreObserver>>>>;

#[derive(uniffi::Object)]
pub struct NotStore {
    store: Arc<Store>,
    observer: SharedObserver,
    // Dropped before `runtime`: stopping sync aborts tasks on it.
    live: Mutex<Option<LiveSync>>,
    runtime: tokio::runtime::Runtime,
}

#[uniffi::export]
impl NotStore {
    #[uniffi::constructor]
    pub fn new(path: String) -> Result<Arc<Self>, NotError> {
        let store = Arc::new(Store::open(path)?);
        let observer = SharedObserver::default();
        store.add_listener({
            let observer = observer.clone();
            move |_| {
                if let Some(o) = current(&observer) {
                    o.on_change();
                }
            }
        });
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("not-sync")
            .enable_all()
            .build()
            .map_err(|e| NotError::Internal {
                detail: e.to_string(),
            })?;
        Ok(Arc::new(Self {
            store,
            observer,
            live: Mutex::new(None),
            runtime,
        }))
    }

    pub fn set_observer(&self, observer: Arc<dyn StoreObserver>) {
        *self.observer.lock().unwrap() = Some(observer);
    }

    // ---- commands ----

    /// Adds a subtask (last) when `target` is a task, else a task (first) in view `target`, with
    /// what that view gives new tasks (its tags, priority, Today's date…).
    pub fn add_task(&self, target: String, title: String, today: String) -> Result<String, NotError> {
        Ok(self.store.add_task(&target, &title, date(&today)?)?)
    }

    pub fn rename(&self, id: String, title: String) -> Result<(), NotError> {
        Ok(self.store.rename(&id, &title)?)
    }

    /// Completing a repeating task moves it to its next date after `today` (`YYYY-MM-DD`).
    pub fn set_completed(&self, id: String, completed: bool, today: String) -> Result<(), NotError> {
        Ok(self.store.set_completed(&id, completed, date(&today)?)?)
    }

    /// Adds a task from quick-add input ("Buy milk tomorrow 9am #groceries !1 by fri") to
    /// `target` as `add_task` does; typed tags add to the view's. Texts in `dismissed` (from
    /// `ParsedToken.text`) stay in the title. `minute` (minutes after midnight, now) lets "in 2
    /// hours" work.
    pub fn quick_add(
        &self,
        target: String,
        input: String,
        today: String,
        minute: Option<u32>,
        dismissed: Vec<String>,
    ) -> Result<String, NotError> {
        Ok(self
            .store
            .quick_add_at(&target, &input, date(&today)?, minute, &dismissed)?)
    }

    /// A task per line of pasted `text` (list markers dropped, indented lines as subtasks, `[x]`
    /// lines completed), each parsed like quick add. One undo step. Returns the top-level ids.
    pub fn quick_add_lines(
        &self,
        target: String,
        text: String,
        today: String,
        minute: Option<u32>,
    ) -> Result<Vec<String>, NotError> {
        Ok(self
            .store
            .quick_add_lines(&target, &text, date(&today)?, minute)?)
    }

    /// What `quick_add` would make of `input`, for live highlighting.
    pub fn parse_task(
        &self,
        input: String,
        today: String,
        minute: Option<u32>,
        dismissed: Vec<String>,
    ) -> Result<ParsedTask, NotError> {
        let p = not_core::parse_task_at(&input, date(&today)?, minute, &dismissed);
        Ok(ParsedTask {
            title: p.title,
            start: p.schedule.start.map(|d| d.to_string()),
            time: p.schedule.time,
            deadline: p.schedule.deadline.map(|d| d.to_string()),
            someday: p.schedule.someday,
            repeat: p.schedule.repeat.map(|r| r.to_rrule()),
            priority: p.priority,
            tags: p.tags,
            tokens: p
                .tokens
                .into_iter()
                .map(|t| ParsedToken {
                    start: t.start,
                    end: t.end,
                    kind: match t.kind {
                        not_core::TokenKind::Date => TokenKind::Date,
                        not_core::TokenKind::Deadline => TokenKind::Deadline,
                        not_core::TokenKind::Time => TokenKind::Time,
                        not_core::TokenKind::Repeat => TokenKind::Repeat,
                        not_core::TokenKind::Someday => TokenKind::Someday,
                        not_core::TokenKind::Priority => TokenKind::Priority,
                        not_core::TokenKind::Tag => TokenKind::Tag,
                    },
                    text: t.text,
                })
                .collect(),
        })
    }

    /// Start date (with `time`), deadline, Someday and repeat. A time needs a start date, a
    /// repeat a start date or deadline; a start date ends Someday.
    pub fn set_schedule(
        &self,
        id: String,
        start: Option<String>,
        time: Option<u32>,
        deadline: Option<String>,
        someday: bool,
        repeat: Option<String>,
    ) -> Result<(), NotError> {
        Ok(self
            .store
            .set_schedule(&id, &schedule(start, time, deadline, someday, repeat)?)?)
    }

    /// Sets the start date of several tasks in one undo step, keeping times and deadlines
    /// ("Move to today" for what slipped). `None` clears it.
    pub fn reschedule(&self, ids: Vec<String>, start: Option<String>) -> Result<(), NotError> {
        let start = start.as_deref().map(date).transpose()?;
        Ok(self.store.reschedule(&ids, start)?)
    }

    /// Puts tasks off to a snooze point, from `today` at `minute` (minutes after midnight). One
    /// undo step. Returns the new start date.
    pub fn snooze(
        &self,
        ids: Vec<String>,
        to: Snooze,
        today: String,
        minute: u32,
    ) -> Result<String, NotError> {
        let s = self.store.snooze(&ids, to.into(), date(&today)?, minute)?;
        Ok(s.start.map(|d| d.to_string()).unwrap_or_default())
    }

    /// "Still relevant" in the weekly review: the task counts as looked at now.
    pub fn keep_task(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.keep_task(&id)?)
    }

    /// 0 = none … 3 = high.
    pub fn set_priority(&self, id: String, priority: u8) -> Result<(), NotError> {
        Ok(self.store.set_priority(&id, priority)?)
    }

    /// Replaces a task's own tags (normalised).
    pub fn set_tags(&self, id: String, tags: Vec<String>) -> Result<(), NotError> {
        Ok(self.store.set_tags(&id, &tags)?)
    }

    pub fn set_notes(&self, id: String, notes: String) -> Result<(), NotError> {
        Ok(self.store.set_notes(&id, &notes)?)
    }

    /// Moves a task under `parent_id` (a task; `None` for top level) right before its child
    /// `before_id`, or to the end. For reordering subtasks; top-level order is `move_in_view`.
    pub fn move_task_before(
        &self,
        id: String,
        parent_id: Option<String>,
        before_id: Option<String>,
    ) -> Result<(), NotError> {
        Ok(self
            .store
            .move_task_before(&id, parent_id.as_deref(), before_id.as_deref())?)
    }

    /// Moves a task shown at the top level of `view` right before `before_id` (also shown there),
    /// or last. Each view keeps its own order; a sorted view switches to manual.
    pub fn move_in_view(
        &self,
        view: String,
        id: String,
        before_id: Option<String>,
        today: String,
    ) -> Result<(), NotError> {
        Ok(self
            .store
            .move_in_view(&view, &id, before_id.as_deref(), date(&today)?)?)
    }

    /// Makes the task a subtask of the open task shown above it in `view`. False if there is none.
    pub fn indent_in_view(&self, view: String, id: String, today: String) -> Result<bool, NotError> {
        Ok(self.store.indent_in_view(&view, &id, date(&today)?)?)
    }

    /// Makes a subtask a sibling of its parent, right after it (in `view` too), keeping the tags it
    /// had through it. False for top-level tasks.
    pub fn outdent_in_view(&self, view: String, id: String) -> Result<bool, NotError> {
        Ok(self.store.outdent_in_view(&view, &id)?)
    }

    /// Deletes a task and its subtasks. Undoable.
    pub fn delete_item(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete(&id)?)
    }

    // ---- views and tags ----

    /// Saves a smart list. The query must be valid (`check_query`).
    pub fn create_view(&self, title: String, query: String) -> Result<String, NotError> {
        Ok(self.store.create_view(&title, &query)?)
    }

    pub fn update_view(&self, id: String, title: String, query: String) -> Result<(), NotError> {
        Ok(self.store.update_view(&id, &title, &query)?)
    }

    /// Deletes a saved view (not its tasks). Undoable.
    pub fn delete_view(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_view(&id)?)
    }

    /// Moves a saved view to `index` among saved views.
    pub fn move_view(&self, id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_view(&id, index as usize)?)
    }

    /// How a view sorts its open tasks (Upcoming is always by date).
    pub fn set_view_sort(&self, id: String, sort: TaskSort) -> Result<(), NotError> {
        Ok(self.store.set_view_sort(&id, sort.into())?)
    }

    /// `None` if `query` is a valid filter, else what's wrong with it.
    pub fn check_query(&self, query: String) -> Option<String> {
        self.store.check_query(&query).err().map(|e| match e {
            not_core::Error::Invalid(m) => m,
            other => other.to_string(),
        })
    }

    /// How many open tasks `query` would show, for previewing a view being edited.
    pub fn count_query(&self, query: String, today: String) -> Result<u32, NotError> {
        Ok(self.store.count_query(&query, date(&today)?)?)
    }

    /// Renames a tag on every task and in saved views. One undo step.
    pub fn rename_tag(&self, old: String, new: String) -> Result<(), NotError> {
        Ok(self.store.rename_tag(&old, &new)?)
    }

    /// Takes a tag off every task. One undo step.
    pub fn delete_tag(&self, tag: String) -> Result<(), NotError> {
        Ok(self.store.delete_tag(&tag)?)
    }

    // ---- habits ----
    // Dates are the user's local calendar dates as `YYYY-MM-DD`.

    pub fn create_habit(&self, title: String, goal: u32, days: u8) -> Result<String, NotError> {
        Ok(self.store.create_habit(&title, goal, days)?)
    }

    pub fn update_habit(
        &self,
        id: String,
        title: String,
        goal: u32,
        days: u8,
        period: HabitPeriod,
    ) -> Result<(), NotError> {
        Ok(self.store.update_habit(&id, &title, goal, days, period.into())?)
    }

    pub fn move_habit(&self, id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_habit(&id, index as usize)?)
    }

    /// Undoable; check-ins are kept.
    pub fn delete_habit(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_habit(&id)?)
    }

    /// Adds (or with a negative `delta` removes) check-ins on `date`. Returns the new count.
    pub fn log_habit(&self, id: String, date: String, delta: i64) -> Result<u32, NotError> {
        Ok(self.store.log_habit(&id, &date, delta)?)
    }

    /// One tap: a check-in until the goal is met, then back to zero. Returns the new count.
    /// A timed habit flips now (returns 0).
    pub fn toggle_habit(&self, id: String, date: String) -> Result<u32, NotError> {
        Ok(self.store.toggle_habit(&id, &date)?)
    }

    /// A timed habit with a goal of `minutes` per scheduled day.
    pub fn create_timed_habit(
        &self,
        title: String,
        minutes: u32,
        days: u8,
        rule: TimedRule,
    ) -> Result<String, NotError> {
        Ok(self
            .store
            .create_timed_habit(&title, minutes, days, rule.into())?)
    }

    pub fn update_timed_habit(
        &self,
        id: String,
        title: String,
        minutes: u32,
        days: u8,
        rule: TimedRule,
    ) -> Result<(), NotError> {
        Ok(self
            .store
            .update_timed_habit(&id, &title, minutes, days, rule.into())?)
    }

    /// A timed habit was on (or off) from `from` until `to`, or from `from` on when `to` is
    /// `None`: a tap (`from` = now), backdating ("since 15 min ago") or a timeline fix. Unix
    /// millis; `now` is the device's clock. One undo step.
    pub fn set_habit_state(
        &self,
        id: String,
        on: bool,
        from: i64,
        to: Option<i64>,
        now: i64,
    ) -> Result<(), NotError> {
        Ok(self.store.set_habit_state(&id, on, from, to, now)?)
    }

    /// The tracked stretches of `date`, up to `now_ms` when it's today. `utc_offset_minutes`
    /// places the local day (UTC+1 = 60).
    pub fn habit_timeline(
        &self,
        id: String,
        date: String,
        now_ms: i64,
        utc_offset_minutes: i32,
    ) -> Result<Vec<TimeSegment>, NotError> {
        let clock = not_core::Clock {
            now_ms,
            offset_minutes: utc_offset_minutes,
        };
        Ok(self
            .store
            .habit_timeline(&id, &date, clock)?
            .into_iter()
            .map(|s| TimeSegment {
                start: s.start,
                end: s.end,
                on: s.on,
            })
            .collect())
    }

    /// Habits as of `date`; timed ones up to `now_ms`, with local days placed by
    /// `utc_offset_minutes` (UTC+1 = 60).
    pub fn habits(
        &self,
        date: String,
        now_ms: i64,
        utc_offset_minutes: i32,
    ) -> Result<Vec<HabitSummary>, NotError> {
        let clock = not_core::Clock {
            now_ms,
            offset_minutes: utc_offset_minutes,
        };
        Ok(self
            .store
            .habits_at(&date, clock)?
            .into_iter()
            .map(HabitSummary::from)
            .collect())
    }

    /// Days in `from..=to` with check-ins (timed habits: with tracked time, `count` in
    /// minutes), for history grids.
    pub fn habit_days(
        &self,
        id: String,
        from: String,
        to: String,
        now_ms: i64,
        utc_offset_minutes: i32,
    ) -> Result<Vec<HabitDay>, NotError> {
        let clock = not_core::Clock {
            now_ms,
            offset_minutes: utc_offset_minutes,
        };
        Ok(self
            .store
            .habit_days_at(&id, &from, &to, clock)?
            .into_iter()
            .map(|d| HabitDay {
                date: d.date,
                count: d.count,
            })
            .collect())
    }

    // ---- notes ----
    // `folder_id` `None` means the top level (for queries: all notes).

    pub fn create_folder(&self, parent_id: Option<String>, title: String) -> Result<String, NotError> {
        Ok(self.store.create_folder(parent_id.as_deref(), &title)?)
    }

    pub fn rename_folder(&self, id: String, title: String) -> Result<(), NotError> {
        Ok(self.store.rename_folder(&id, &title)?)
    }

    pub fn move_folder(&self, id: String, parent_id: Option<String>) -> Result<(), NotError> {
        Ok(self.store.move_folder(&id, parent_id.as_deref())?)
    }

    pub fn create_note(&self, folder_id: Option<String>, body: String) -> Result<String, NotError> {
        Ok(self.store.create_note(folder_id.as_deref(), &body)?)
    }

    /// Keeps an image (JPEG, PNG, WebP or GIF bytes, already downscaled) for notes; returns the id
    /// for [`image_markup`]. Syncs beside the doc.
    pub fn add_image(&self, bytes: Vec<u8>) -> Result<String, NotError> {
        Ok(self.store.add_image(&bytes)?)
    }

    /// An image's bytes; `None` until it has arrived from the server.
    pub fn image(&self, id: String) -> Result<Option<Vec<u8>>, NotError> {
        Ok(self.store.image(&id)?)
    }

    /// Makes tasks of the checklist items the selection touches in `body` (what the editor shows,
    /// saved first), tagged `tag` (empty: from the suggested title), or untagged (Inbox) when
    /// `None`. The note's lines stay as they are; one undo step.
    pub fn checklist_to_tasks(
        &self,
        note_id: String,
        body: String,
        start: u32,
        end: u32,
        tag: Option<String>,
    ) -> Result<ChecklistMoved, NotError> {
        let m = self
            .store
            .checklist_to_tasks(&note_id, &body, start, end, tag.as_deref())?;
        Ok(ChecklistMoved {
            tag: m.tag,
            count: m.count,
        })
    }

    /// Replaces the markdown body; merges with concurrent edits from other devices.
    pub fn set_note_body(&self, id: String, body: String) -> Result<(), NotError> {
        Ok(self.store.set_note_body(&id, &body)?)
    }

    /// The body for the editor to open (notes from earlier versions brought up to date first).
    pub fn open_note(&self, id: String) -> Result<String, NotError> {
        Ok(self.store.open_note(&id)?)
    }

    pub fn set_note_pinned(&self, id: String, pinned: bool) -> Result<(), NotError> {
        Ok(self.store.set_note_pinned(&id, pinned)?)
    }

    pub fn move_note(&self, id: String, folder_id: Option<String>) -> Result<(), NotError> {
        Ok(self.store.move_note(&id, folder_id.as_deref())?)
    }

    /// Places a note right before `before_id` in its folder, or last. A folder sorted by latest
    /// edit switches to manual order, keeping the order shown.
    pub fn move_note_before(&self, id: String, before_id: Option<String>) -> Result<(), NotError> {
        Ok(self.store.move_note_before(&id, before_id.as_deref())?)
    }

    /// Manual order starts from the order shown; `false` sorts by latest edit again.
    /// How `folder_id` (or All Notes, for `None`) sorts; Manual starts from the order shown.
    pub fn set_note_sort(&self, folder_id: Option<String>, sort: NoteSort) -> Result<(), NotError> {
        Ok(self.store.set_note_sort(folder_id.as_deref(), sort.into())?)
    }

    pub fn all_notes_sort(&self) -> NoteSort {
        self.store.all_notes_sort().into()
    }

    pub fn set_folder_manual_order(&self, id: String, manual: bool) -> Result<(), NotError> {
        Ok(self.store.set_folder_manual_order(&id, manual)?)
    }

    /// Deletes a note, or a folder with everything in it. Undoable.
    pub fn delete_note(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_note_or_folder(&id)?)
    }

    /// Depth-first, each level by title.
    pub fn folders(&self) -> Result<Vec<Folder>, NotError> {
        Ok(self
            .store
            .folders()?
            .into_iter()
            .map(|f| Folder {
                id: f.id,
                parent_id: f.parent_id,
                title: f.title,
                depth: f.depth,
                note_count: f.note_count,
                manual_order: f.manual_order,
                sort: f.sort.into(),
            })
            .collect())
    }

    /// Notes directly in `folder_id`, or all notes when `None`. Pinned first, then latest edit.
    pub fn notes(&self, folder_id: Option<String>) -> Result<Vec<NoteSummary>, NotError> {
        Ok(self
            .store
            .notes(folder_id.as_deref())?
            .into_iter()
            .map(NoteSummary::from)
            .collect())
    }

    /// Notes containing every word of `query`, ignoring case.
    pub fn search_notes(&self, query: String) -> Result<Vec<NoteSummary>, NotError> {
        Ok(self
            .store
            .search_notes(&query)?
            .into_iter()
            .map(NoteSummary::from)
            .collect())
    }

    pub fn note_body(&self, id: String) -> Result<String, NotError> {
        Ok(self.store.note_body(&id)?)
    }

    // ---- workouts ----
    // Dates are local calendar dates, `YYYY-MM-DD`.

    pub fn create_workout(&self, date: String, title: String) -> Result<String, NotError> {
        Ok(self.store.create_workout(&date, &title)?)
    }

    /// A new session on `date` with the same title and exercises as `from_id`, sets left empty.
    pub fn repeat_workout(&self, from_id: String, date: String) -> Result<String, NotError> {
        Ok(self.store.repeat_workout(&from_id, &date)?)
    }

    pub fn update_workout(&self, id: String, date: String, title: String) -> Result<(), NotError> {
        Ok(self.store.update_workout(&id, &date, &title)?)
    }

    /// Undoable.
    pub fn delete_workout(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_workout(&id)?)
    }

    pub fn add_exercise(&self, workout_id: String, name: String, sets: String) -> Result<String, NotError> {
        Ok(self.store.add_exercise(&workout_id, &name, &sets)?)
    }

    pub fn update_exercise(&self, id: String, name: String, sets: String) -> Result<(), NotError> {
        Ok(self.store.update_exercise(&id, &name, &sets)?)
    }

    /// Ticks an exercise off for this session (or back on).
    pub fn set_exercise_done(&self, id: String, done: bool) -> Result<(), NotError> {
        Ok(self.store.set_exercise_done(&id, done)?)
    }

    pub fn delete_exercise(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_exercise(&id)?)
    }

    pub fn move_exercise(&self, id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_exercise(&id, index as usize)?)
    }

    /// Latest first.
    pub fn workouts(&self) -> Result<Vec<WorkoutSummary>, NotError> {
        Ok(self
            .store
            .workouts()?
            .into_iter()
            .map(|w| WorkoutSummary {
                id: w.id,
                date: w.date,
                title: w.title,
                exercise_names: w.exercise_names,
                set_count: w.set_count,
            })
            .collect())
    }

    pub fn workout(&self, id: String) -> Result<Workout, NotError> {
        let w = self.store.workout(&id)?;
        Ok(Workout {
            id: w.id,
            date: w.date,
            title: w.title,
            exercises: w
                .exercises
                .into_iter()
                .map(|e| WorkoutExercise {
                    id: e.id,
                    name: e.name,
                    sets: e.sets,
                    lines: e.lines.into_iter().map(SetLine::from).collect(),
                    done: e.done,
                    previous: e.previous.map(ExerciseLog::from),
                })
                .collect(),
        })
    }

    /// Sessions with this exercise (matched by name, ignoring case), latest first.
    pub fn exercise_history(&self, name: String, limit: u32) -> Result<Vec<ExerciseLog>, NotError> {
        Ok(self
            .store
            .exercise_history(&name, limit)?
            .into_iter()
            .map(ExerciseLog::from)
            .collect())
    }

    /// Names used so far, most recent first.
    pub fn exercise_names(&self) -> Result<Vec<String>, NotError> {
        Ok(self.store.exercise_names()?)
    }

    /// Catalogue exercises matching `query` (English, French or alias; accents ignored).
    pub fn search_exercises(&self, query: String, limit: u32) -> Vec<CatalogExercise> {
        self.store
            .search_exercises(&query, limit as usize)
            .into_iter()
            .map(CatalogExercise::from)
            .collect()
    }

    /// The catalogue exercise a name is linked to, by hand or by matching its name.
    pub fn exercise_info(&self, name: String) -> Option<CatalogExercise> {
        self.store.exercise_info(&name).map(CatalogExercise::from)
    }

    /// Links a name to a catalogue exercise; `None` unlinks it.
    pub fn link_exercise(&self, name: String, catalog_id: Option<String>) -> Result<(), NotError> {
        Ok(self.store.link_exercise(&name, catalog_id.as_deref())?)
    }

    /// Working sets per primary muscle in sessions dated `from..=to`, most first.
    pub fn muscle_sets(&self, from: String, to: String) -> Result<Vec<MuscleSets>, NotError> {
        Ok(self
            .store
            .muscle_sets(&from, &to)?
            .into_iter()
            .map(|m| MuscleSets {
                muscle: m.muscle,
                sets: m.sets,
            })
            .collect())
    }

    /// Adds sessions from a text log (`dd/mm` lines, exercise names, set lines). Returns how many.
    pub fn import_workouts(&self, text: String, today: String) -> Result<u32, NotError> {
        Ok(self.store.import_workouts(&text, &today)?)
    }

    // ---- undo ----

    /// Reverts this session's last change on this device. False if there was nothing to undo.
    pub fn undo(&self) -> Result<bool, NotError> {
        Ok(self.store.undo()?)
    }

    pub fn redo(&self) -> Result<bool, NotError> {
        Ok(self.store.redo()?)
    }

    pub fn can_undo(&self) -> bool {
        self.store.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.store.can_redo()
    }

    // ---- queries ----

    /// Built-in views, then saved ones in order.
    pub fn views(&self, today: String) -> Result<Vec<TaskView>, NotError> {
        Ok(self
            .store
            .views(date(&today)?)?
            .into_iter()
            .map(|v| TaskView {
                id: v.id,
                title: v.title,
                query: v.query,
                built_in: v.built_in,
                sort: v.sort.into(),
                open_count: v.open_count,
            })
            .collect())
    }

    /// Every tag in use, by name.
    pub fn tags(&self) -> Result<Vec<TagInfo>, NotError> {
        Ok(self
            .store
            .tags()?
            .into_iter()
            .map(|t| TagInfo {
                name: t.name,
                open_count: t.open_count,
            })
            .collect())
    }

    pub fn task_by_id(&self, id: String) -> Result<TaskItem, NotError> {
        Ok(self.store.task_by_id(&id)?.into())
    }

    /// A task's direct subtasks, in order.
    pub fn subtasks(&self, id: String) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .subtasks(&id)?
            .into_iter()
            .map(TaskItem::from)
            .collect())
    }

    /// Open tasks with a start date and time, soonest first: what reminders are set for.
    pub fn timed_tasks(&self) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .timed_tasks()?
            .into_iter()
            .map(TaskItem::from)
            .collect())
    }

    /// Completed top-level tasks, latest first, at most `limit`.
    pub fn logbook(&self, limit: u32) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .logbook(limit)?
            .into_iter()
            .map(TaskItem::from)
            .collect())
    }

    /// Open undated tasks nobody has touched for `days`, oldest first (for the weekly review).
    pub fn stale_tasks(&self, days: u32) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .stale_tasks(now_millis(), days)?
            .into_iter()
            .map(TaskItem::from)
            .collect())
    }

    /// Tasks whose title or notes contain every word of `query`, open first. At most `limit`.
    pub fn search_tasks(&self, query: String, limit: u32) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .search_tasks(&query, limit)?
            .into_iter()
            .map(TaskItem::from)
            .collect())
    }

    /// Tasks completed over a day ago, from every list, latest first. Reopening one puts it back.
    pub fn archive(&self) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .archive(now_millis())?
            .into_iter()
            .map(TaskItem::from)
            .collect())
    }

    /// Deletes everything in the archive (one undo step). Returns how many tasks.
    pub fn clear_archive(&self) -> Result<u32, NotError> {
        Ok(self.store.clear_archive(now_millis())?)
    }

    /// A view's tasks as shown (`view` may also be a `#tag` query, to browse a tag).
    pub fn view_outline(&self, view: String, today: String) -> Result<Outline, NotError> {
        let outline = self.store.view_outline(&view, date(&today)?)?;
        let rows = |rows: Vec<not_core::OutlineRow>| {
            rows.into_iter()
                .map(|r| OutlineRow {
                    task: r.task.into(),
                    depth: r.depth,
                    done: r.done,
                    subtasks: r.subtasks,
                    subtasks_done: r.subtasks_done,
                })
                .collect()
        };
        Ok(Outline {
            open: rows(outline.open),
            completed: rows(outline.completed),
        })
    }

    // ---- export ----

    /// Everything as readable JSON: tasks and views, habits and check-ins, notes, workouts.
    pub fn export_json(&self, today: String) -> Result<String, NotError> {
        Ok(self.store.export_json(date(&today)?)?)
    }

    /// The whole workspace with its history, as one file for `restore_backup`.
    pub fn backup(&self) -> Result<Vec<u8>, NotError> {
        Ok(self.store.backup()?)
    }

    /// Merges a backup file in, like a sync: nothing on this device is lost or undone.
    pub fn restore_backup(&self, bytes: Vec<u8>) -> Result<(), NotError> {
        Ok(self.store.restore_backup(&bytes)?)
    }

    // ---- sync ----

    /// `None` until this device is paired.
    pub fn sync_info(&self) -> Result<Option<SyncInfo>, NotError> {
        Ok(self.store.credentials()?.as_ref().map(SyncInfo::from))
    }

    /// Starts (or restarts) background sync if this device is paired. Call once at launch.
    pub fn start_sync(&self) -> Result<(), NotError> {
        let mut live = self.live.lock().unwrap();
        live.take();
        let Some(credentials) = self.store.credentials()? else {
            return Ok(());
        };
        let observer = self.observer.clone();
        *live = Some(LiveSync::start(
            self.runtime.handle(),
            self.store.clone(),
            SyncClient::from_credentials(&credentials),
            move |status| {
                if let Some(o) = current(&observer) {
                    o.on_sync_state(status.into());
                }
            },
        ));
        Ok(())
    }

    pub fn stop_sync(&self) {
        self.live.lock().unwrap().take();
    }

    /// Sync immediately, e.g. when the app comes to the foreground. No-op if sync isn't started.
    pub fn sync_now(&self) {
        if let Some(live) = &*self.live.lock().unwrap() {
            live.sync_now();
        }
    }

    /// One full round (push, then pull) that completes before returning, for background work such
    /// as Android's WorkManager. No-op if this device isn't paired.
    pub async fn sync(&self) -> Result<(), NotError> {
        let Some(credentials) = self.store.credentials()? else {
            return Ok(());
        };
        let client = SyncClient::from_credentials(&credentials);
        let store = self.store.clone();
        self.on_runtime(async move { client.sync(&store).await.map(drop) })
            .await
    }

    // ---- pairing ----

    /// Pairs with a server using a code from `not-server pair` or another device's "Add device",
    /// then starts syncing. Replaces any previous pairing; local data is kept and merged.
    pub async fn pair(
        &self,
        server_url: String,
        code: String,
        device_name: String,
    ) -> Result<SyncInfo, NotError> {
        let credentials = self
            .on_runtime(async move { SyncClient::pair(&server_url, &code, &device_name).await })
            .await?;
        self.stop_sync();
        self.store.set_credentials(&credentials)?;
        self.start_sync()?;
        Ok(SyncInfo::from(&credentials))
    }

    /// Stops syncing and forgets the pairing. Local data stays. Also revokes this device on the
    /// server when reachable; otherwise revoke it later from another device.
    pub async fn unpair(&self) -> Result<(), NotError> {
        self.stop_sync();
        if let Some(credentials) = self.store.credentials()? {
            let client = SyncClient::from_credentials(&credentials);
            let _ = self
                .on_runtime(async move { client.revoke(credentials.device_id).await })
                .await;
        }
        Ok(self.store.clear_credentials()?)
    }

    /// A one-time code (valid 10 minutes) for pairing another device.
    pub async fn create_pairing_code(&self) -> Result<PairingCode, NotError> {
        let client = SyncClient::from_credentials(&self.paired()?);
        let code = self
            .on_runtime(async move { client.create_pairing_code().await })
            .await?;
        Ok(PairingCode {
            code: code.code,
            expires_at: code.expires_at,
        })
    }

    pub async fn devices(&self) -> Result<Vec<Device>, NotError> {
        let credentials = self.paired()?;
        let client = SyncClient::from_credentials(&credentials);
        let devices = self.on_runtime(async move { client.devices().await }).await?;
        Ok(devices
            .into_iter()
            .map(|d| Device {
                is_current: d.id == credentials.device_id,
                id: d.id,
                name: d.name,
                created_at: d.created_at,
                last_seen_at: d.last_seen_at,
            })
            .collect())
    }

    /// Unpairs another device. Revoking this device is the same as [`Self::unpair`].
    /// Share links of `list_id` (all lists for `None`), oldest first.
    pub async fn shares(&self, list_id: Option<String>) -> Result<Vec<ListShare>, NotError> {
        let credentials = self.paired()?;
        let client = SyncClient::from_credentials(&credentials);
        let shares = self.on_runtime(async move { client.shares().await }).await?;
        Ok(shares
            .into_iter()
            .filter(|s| list_id.as_ref().is_none_or(|l| *l == s.list_id))
            .map(|s| share(&credentials.server_url, s))
            .collect())
    }

    /// A new link to `list_id`; with `can_edit`, visitors can tick off and add tasks.
    pub async fn share_list(&self, list_id: String, can_edit: bool) -> Result<ListShare, NotError> {
        let credentials = self.paired()?;
        let client = SyncClient::from_credentials(&credentials);
        let created = self
            .on_runtime(async move { client.create_share(&list_id, can_edit).await })
            .await?;
        Ok(share(&credentials.server_url, created))
    }

    /// The link stops working at once.
    pub async fn revoke_share(&self, id: i64) -> Result<(), NotError> {
        let credentials = self.paired()?;
        let client = SyncClient::from_credentials(&credentials);
        self.on_runtime(async move { client.revoke_share(id).await })
            .await
    }

    pub async fn revoke_device(&self, id: i64) -> Result<(), NotError> {
        let credentials = self.paired()?;
        if id == credentials.device_id {
            return self.unpair().await;
        }
        let client = SyncClient::from_credentials(&credentials);
        self.on_runtime(async move { client.revoke(id).await }).await
    }
}

impl NotStore {
    fn paired(&self) -> Result<Credentials, NotError> {
        self.store.credentials()?.ok_or_else(|| NotError::Invalid {
            detail: "this device isn't paired with a server".into(),
        })
    }

    /// Runs `work` on the store's runtime (reqwest needs tokio). The returned future doesn't, so
    /// the Swift/Kotlin executors can poll it.
    async fn on_runtime<T: Send + 'static>(
        &self,
        work: impl Future<Output = not_core::Result<T>> + Send + 'static,
    ) -> Result<T, NotError> {
        match self.runtime.spawn(work).await {
            Ok(result) => Ok(result?),
            Err(e) => Err(NotError::Internal {
                detail: e.to_string(),
            }),
        }
    }
}

impl From<not_core::Task> for TaskItem {
    fn from(t: not_core::Task) -> Self {
        Self {
            id: t.id,
            parent_id: t.parent_id,
            title: t.title,
            notes: t.notes,
            completed_at: t.completed_at,
            start: t.start,
            time: t.time,
            deadline: t.deadline,
            someday: t.someday,
            priority: t.priority,
            repeat: t.repeat,
            tags: t.tags,
        }
    }
}

impl From<not_core::HabitSummary> for HabitSummary {
    fn from(s: not_core::HabitSummary) -> Self {
        Self {
            habit: Habit {
                id: s.habit.id,
                title: s.habit.title,
                goal: s.habit.goal,
                days: s.habit.days,
                period: s.habit.period.into(),
                timed: s.habit.timed.map(Into::into),
            },
            count: s.count,
            progress: s.progress,
            done: s.done,
            due: s.due,
            streak: s.streak,
            best_streak: s.best_streak,
            week: s.week,
            timed: s.timed.map(|t| TimedState {
                on: t.on,
                since: t.since,
                on_ms: t.on_ms,
                off_ms: t.off_ms,
            }),
        }
    }
}

impl From<not_core::NoteSummary> for NoteSummary {
    fn from(n: not_core::NoteSummary) -> Self {
        Self {
            id: n.id,
            folder_id: n.folder_id,
            title: n.title,
            preview: n.preview,
            pinned: n.pinned,
            updated_at: n.updated_at,
        }
    }
}

impl From<not_core::SetLine> for SetLine {
    fn from(l: not_core::SetLine) -> Self {
        Self {
            raw: l.raw,
            parsed: l.parsed,
            warmup: l.warmup,
            weight: l.weight,
            unit: l.unit,
            reps: l.reps,
            distance: l.distance,
            duration: l.duration,
            note: l.note,
            label: l.label,
            done: l.done,
        }
    }
}

impl From<not_core::ExerciseLog> for ExerciseLog {
    fn from(l: not_core::ExerciseLog) -> Self {
        Self {
            workout_id: l.workout_id,
            date: l.date,
            sets: l.sets,
            lines: l.lines.into_iter().map(SetLine::from).collect(),
            top_weight: l.top_weight,
            volume: l.volume,
            set_count: l.set_count,
            distance: l.distance,
            duration: l.duration,
        }
    }
}

impl From<&Credentials> for SyncInfo {
    fn from(c: &Credentials) -> Self {
        Self {
            server_url: c.server_url.clone(),
            device_id: c.device_id,
            device_name: c.device_name.clone(),
        }
    }
}

impl From<SyncStatus> for SyncState {
    fn from(s: SyncStatus) -> Self {
        match s {
            SyncStatus::Syncing => Self::Syncing,
            SyncStatus::Synced => Self::Synced,
            SyncStatus::Offline(reason) => Self::Offline { reason },
            SyncStatus::Unauthorized => Self::Unauthorized,
        }
    }
}

/// Clones the observer out so callbacks never run while holding the lock.
fn current(observer: &SharedObserver) -> Option<Arc<dyn StoreObserver>> {
    observer.lock().unwrap().clone()
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn share(server_url: &str, s: not_core::ShareInfo) -> ListShare {
    ListShare {
        url: not_core::share_url(server_url, &s.token),
        id: s.id,
        list_id: s.list_id,
        can_edit: s.can_edit,
        created_at: s.created_at,
    }
}
