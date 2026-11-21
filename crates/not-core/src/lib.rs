//! Shared core of `not`: Loro documents, SQLite persistence and read model, commands, sync.

mod catalog;
mod checklist;
pub mod date;
mod error;
mod export;
mod filter;
mod habits;
mod images;
mod import;
pub mod markdown;
mod notes;
mod parse;
mod projection;
mod schedule;
mod starter;
mod storage;
mod store;
mod sync;
mod timed;
mod trash;
mod views;
mod workouts;

pub use catalog::{CatalogExercise, MuscleSets};
pub use checklist::{Checklist, ChecklistItem, ChecklistMoved, checklist};
pub use error::{Error, Result};
pub use filter::Defaults as ViewDefaults;
pub use habits::{EVERY_DAY, Habit, HabitDay, HabitSummary, Period};
pub use import::{ImportReport, ImportSource};
pub use not_protocol::{IMAGE_SCHEME, PairingLink, ShareInfo, image_mime, is_image_id, share_url};
pub use notes::{Folder, NoteSort, NoteSummary};
pub use parse::{ParsedTask, Token, TokenKind, normalize_tag, parse_task, parse_task_at};
pub use schedule::{Freq, Repeat, Schedule, due_label};
pub use starter::STARTER_TAG;
pub use store::{
    ARCHIVE_AFTER_MS, ChangeOrigin, INBOX, ListenerId, Outline, OutlineRow, SOMEDAY_VIEW, Snooze, Store,
    TODAY, Task, TaskSort, UPCOMING, WORKSPACE,
};
pub use sync::{Credentials, Outgoing};
#[cfg(feature = "net")]
pub use sync::{DeviceInfo, LiveSync, PairingCode, SyncClient, SyncReport, SyncStatus};
pub use timed::{Clock, Segment, TimedRule, TimedState};
pub use trash::{TRASH_DAYS, TrashItem, TrashKind};
pub use views::{TagInfo, View};
pub use workouts::{
    DONE_MARK, ExerciseLog, SetLine, Workout, WorkoutExercise, WorkoutSummary, expand_sets, parse_sets,
};
