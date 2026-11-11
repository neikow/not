//! uniffi surface of `not-core` for the Swift (macOS) and Kotlin (Android) apps.
//!
//! Naming avoids Swift's `Task`: records are `TaskList` / `TaskItem`.
//!
//! Network calls are `async` (Swift `async`, Kotlin `suspend`). They run on the store's own tokio
//! runtime; the foreign side only awaits the result.

use std::sync::{Arc, Mutex};

use not_core::{Credentials, LiveSync, Store, SyncClient, SyncStatus};

uniffi::setup_scaffolding!();

/// Id of the built-in Inbox list: always first in `lists()`, can't be renamed or deleted.
#[uniffi::export]
pub fn inbox_id() -> String {
    not_core::INBOX.into()
}

/// "Today", "Tomorrow 9:00", "Fri", "12 Nov" for a due date (`YYYY-MM-DD`) seen on `today`.
#[uniffi::export]
pub fn due_label(due: String, due_time: Option<u32>, today: String) -> Result<String, NotError> {
    Ok(not_core::due_label(date(&due)?, due_time, date(&today)?))
}

/// "Daily", "Every 2 weeks on Mon" for a task's `repeat` rule; `None` if it isn't one this app reads.
#[uniffi::export]
pub fn repeat_label(rule: String) -> Option<String> {
    not_core::Repeat::parse_rrule(&rule).ok().map(|r| r.label())
}

fn date(s: &str) -> Result<not_core::date::Date, NotError> {
    Ok(not_core::date::Date::parse(s)?)
}

fn schedule(
    due: Option<String>,
    due_time: Option<u32>,
    repeat: Option<String>,
) -> Result<not_core::Schedule, NotError> {
    Ok(not_core::Schedule {
        due: due.as_deref().map(date).transpose()?,
        time: due_time,
        repeat: repeat.as_deref().map(not_core::Repeat::parse_rrule).transpose()?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TokenKind {
    Date,
    Time,
    Repeat,
    Priority,
    List,
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
    pub due: Option<String>,
    pub due_time: Option<u32>,
    pub repeat: Option<String>,
    pub priority: u8,
    pub list_id: Option<String>,
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

#[derive(Debug, Clone, uniffi::Record)]
pub struct TaskList {
    pub id: String,
    pub title: String,
    /// Open top-level tasks, for badges.
    pub open_count: u32,
    pub sort: TaskSort,
}

/// How a list orders its open tasks; ties keep the manual order.
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
    pub list_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    /// Markdown.
    pub notes: String,
    /// Unix millis.
    pub completed_at: Option<i64>,
    /// `YYYY-MM-DD`; see `due_label`.
    pub due: Option<String>,
    /// Minutes after midnight.
    pub due_time: Option<u32>,
    /// 0 = none, 1 = low, 2 = medium, 3 = high.
    pub priority: u8,
    /// Repeat rule; see `repeat_label`.
    pub repeat: Option<String>,
}

/// A task in display order with its nesting depth.
#[derive(Debug, Clone, uniffi::Record)]
pub struct OutlineRow {
    pub task: TaskItem,
    pub depth: u32,
    /// The task or one of its ancestors is completed.
    pub done: bool,
}

/// A list as shown: open tasks with all their subtasks, then completed ones (latest first).
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

    pub fn create_list(&self, title: String) -> Result<String, NotError> {
        Ok(self.store.create_list(&title)?)
    }

    /// `parent_id` is a list (the task goes first), or a task for a subtask (it goes last).
    pub fn add_task(&self, parent_id: String, title: String) -> Result<String, NotError> {
        Ok(self.store.add_task(&parent_id, &title)?)
    }

    pub fn rename(&self, id: String, title: String) -> Result<(), NotError> {
        Ok(self.store.rename(&id, &title)?)
    }

    /// Completing a repeating task moves it to its next due date after `today` (`YYYY-MM-DD`).
    pub fn set_completed(&self, id: String, completed: bool, today: String) -> Result<(), NotError> {
        Ok(self.store.set_completed(&id, completed, date(&today)?)?)
    }

    /// Adds a task from quick-add input: "Buy milk tomorrow 9am #groceries !1". A `#list` overrides
    /// `parent_id`. Texts in `dismissed` (from `ParsedToken.text`) stay in the title. Without a date
    /// in the input the task is due on `default_due`, if given (adding from Today).
    pub fn quick_add(
        &self,
        parent_id: String,
        input: String,
        today: String,
        dismissed: Vec<String>,
        default_due: Option<String>,
    ) -> Result<String, NotError> {
        let default_due = default_due.as_deref().map(date).transpose()?;
        Ok(self
            .store
            .quick_add(&parent_id, &input, date(&today)?, &dismissed, default_due)?)
    }

    /// What `quick_add` would make of `input`, for live highlighting.
    pub fn parse_task(
        &self,
        input: String,
        today: String,
        dismissed: Vec<String>,
    ) -> Result<ParsedTask, NotError> {
        let p = self.store.parse_task(&input, date(&today)?, &dismissed)?;
        Ok(ParsedTask {
            title: p.title,
            due: p.schedule.due.map(|d| d.to_string()),
            due_time: p.schedule.time,
            repeat: p.schedule.repeat.map(|r| r.to_rrule()),
            priority: p.priority,
            list_id: p.list_id,
            tokens: p
                .tokens
                .into_iter()
                .map(|t| ParsedToken {
                    start: t.start,
                    end: t.end,
                    kind: match t.kind {
                        not_core::TokenKind::Date => TokenKind::Date,
                        not_core::TokenKind::Time => TokenKind::Time,
                        not_core::TokenKind::Repeat => TokenKind::Repeat,
                        not_core::TokenKind::Priority => TokenKind::Priority,
                        not_core::TokenKind::List => TokenKind::List,
                    },
                    text: t.text,
                })
                .collect(),
        })
    }

    /// No `due` clears the time and repeat too.
    pub fn set_schedule(
        &self,
        id: String,
        due: Option<String>,
        due_time: Option<u32>,
        repeat: Option<String>,
    ) -> Result<(), NotError> {
        Ok(self.store.set_schedule(&id, &schedule(due, due_time, repeat)?)?)
    }

    /// 0 = none … 3 = high.
    pub fn set_priority(&self, id: String, priority: u8) -> Result<(), NotError> {
        Ok(self.store.set_priority(&id, priority)?)
    }

    pub fn set_notes(&self, id: String, notes: String) -> Result<(), NotError> {
        Ok(self.store.set_notes(&id, &notes)?)
    }

    pub fn move_task(&self, id: String, parent_id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_task(&id, &parent_id, index as usize)?)
    }

    /// Moves a task under `parent_id` right before its child `before_id`, or to the end.
    /// How a list (or the Inbox) sorts its open tasks. Dragging a task in a sorted list switches it
    /// back to manual, starting from the order shown.
    pub fn set_list_sort(&self, list_id: String, sort: TaskSort) -> Result<(), NotError> {
        Ok(self.store.set_list_sort(&list_id, sort.into())?)
    }

    pub fn move_task_before(
        &self,
        id: String,
        parent_id: String,
        before_id: Option<String>,
    ) -> Result<(), NotError> {
        Ok(self
            .store
            .move_task_before(&id, &parent_id, before_id.as_deref())?)
    }

    /// Makes the task a subtask of the open task above it. False if there is none.
    pub fn indent_task(&self, id: String) -> Result<bool, NotError> {
        Ok(self.store.indent_task(&id)?)
    }

    /// Makes a subtask a sibling of its parent, right after it. False for top-level tasks.
    pub fn outdent_task(&self, id: String) -> Result<bool, NotError> {
        Ok(self.store.outdent_task(&id)?)
    }

    /// Moves a list to `index` among the other lists (the Inbox stays first).
    pub fn move_list(&self, id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_list(&id, index as usize)?)
    }

    /// Deletes a list or task and everything under it. Undoable.
    pub fn delete_item(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete(&id)?)
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
    pub fn toggle_habit(&self, id: String, date: String) -> Result<u32, NotError> {
        Ok(self.store.toggle_habit(&id, &date)?)
    }

    pub fn habits(&self, date: String) -> Result<Vec<HabitSummary>, NotError> {
        Ok(self
            .store
            .habits(&date)?
            .into_iter()
            .map(HabitSummary::from)
            .collect())
    }

    /// Days in `from..=to` with check-ins, for history grids.
    pub fn habit_days(&self, id: String, from: String, to: String) -> Result<Vec<HabitDay>, NotError> {
        Ok(self
            .store
            .habit_days(&id, &from, &to)?
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

    /// Replaces the markdown body; merges with concurrent edits from other devices.
    pub fn set_note_body(&self, id: String, body: String) -> Result<(), NotError> {
        Ok(self.store.set_note_body(&id, &body)?)
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

    pub fn lists(&self) -> Result<Vec<TaskList>, NotError> {
        Ok(self
            .store
            .lists()?
            .into_iter()
            .map(|l| TaskList {
                id: l.id,
                title: l.title,
                open_count: l.open_count,
                sort: l.sort.into(),
            })
            .collect())
    }

    pub fn tasks(&self, list_id: String) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .tasks(&list_id)?
            .into_iter()
            .map(TaskItem::from)
            .collect())
    }

    /// Open tasks due on or before `today` from every list, overdue first.
    pub fn due_tasks(&self, today: String) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .due_tasks(date(&today)?)?
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

    /// Open tasks due after `today` from every list, soonest first.
    pub fn upcoming_tasks(&self, today: String) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .upcoming_tasks(date(&today)?)?
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

    pub fn outline(&self, list_id: String) -> Result<Outline, NotError> {
        let outline = self.store.outline(&list_id)?;
        let rows = |rows: Vec<not_core::OutlineRow>| {
            rows.into_iter()
                .map(|r| OutlineRow {
                    task: r.task.into(),
                    depth: r.depth,
                    done: r.done,
                })
                .collect()
        };
        Ok(Outline {
            open: rows(outline.open),
            completed: rows(outline.completed),
        })
    }

    // ---- export ----

    /// Everything as readable JSON: lists and tasks, habits and check-ins, notes, workouts.
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
            list_id: t.list_id,
            parent_id: t.parent_id,
            title: t.title,
            notes: t.notes,
            completed_at: t.completed_at,
            due: t.due,
            due_time: t.due_time,
            priority: t.priority,
            repeat: t.repeat,
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
            },
            count: s.count,
            progress: s.progress,
            done: s.done,
            due: s.due,
            streak: s.streak,
            best_streak: s.best_streak,
            week: s.week,
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
