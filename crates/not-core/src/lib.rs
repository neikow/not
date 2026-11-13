//! Shared core of `not`: Loro documents, SQLite persistence and read model, commands, sync.

mod catalog;
mod checklist;
pub mod date;
mod error;
mod export;
mod habits;
mod images;
pub mod markdown;
mod money;
mod money_parse;
mod notes;
mod parse;
mod projection;
mod schedule;
mod storage;
mod store;
mod sync;
mod workouts;

pub use catalog::{CatalogExercise, MuscleSets};
pub use checklist::{Checklist, ChecklistItem, ChecklistMoved, checklist};
pub use error::{Error, Result};
pub use habits::{EVERY_DAY, Habit, HabitDay, HabitSummary, Period};
pub use not_protocol::{IMAGE_SCHEME, PairingLink, ShareInfo, image_mime, is_image_id, share_url};
pub use money::{
    Account, AccountKind, Budget, Category, CategoryAmount, CategoryBudget, CategoryKind, MoneySettings, PeriodReport,
    Recurring, RecurringInput, Rollover, Split, Txn, TxnFilter, TxnInput, TxnKind, TxnSource, period_of,
};
pub use money_parse::{MoneyToken, MoneyTokenKind, ParsedMoney};
pub use notes::{Folder, NoteSort, NoteSummary};
pub use parse::{ParsedTask, Token, TokenKind, parse_task};
pub use schedule::{Freq, Repeat, Schedule, due_label};
pub use store::{
    ARCHIVE_AFTER_MS, ChangeOrigin, INBOX, List, ListenerId, Outline, OutlineRow, Store, Task, TaskSort,
    WORKSPACE,
};
pub use sync::{Credentials, Outgoing};
#[cfg(feature = "net")]
pub use sync::{DeviceInfo, LiveSync, PairingCode, SyncClient, SyncReport, SyncStatus};
pub use workouts::{
    DONE_MARK, ExerciseLog, SetLine, Workout, WorkoutExercise, WorkoutSummary, expand_sets, parse_sets,
};
