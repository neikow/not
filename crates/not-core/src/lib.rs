//! Shared core of `not`: Loro documents, SQLite persistence and read model, commands, sync.

mod catalog;
pub mod date;
mod error;
mod habits;
pub mod markdown;
mod notes;
mod parse;
mod projection;
mod schedule;
mod storage;
mod store;
mod sync;
mod workouts;

pub use catalog::{CatalogExercise, MuscleSets};
pub use error::{Error, Result};
pub use habits::{EVERY_DAY, Habit, HabitDay, HabitSummary};
pub use not_protocol::PairingLink;
pub use notes::{Folder, NoteSummary};
pub use parse::{ParsedTask, Token, TokenKind, parse_task};
pub use schedule::{Freq, Repeat, Schedule, due_label};
pub use store::{ChangeOrigin, INBOX, List, ListenerId, Outline, OutlineRow, Store, Task, WORKSPACE};
pub use sync::{Credentials, Outgoing};
pub use workouts::{ExerciseLog, SetLine, Workout, WorkoutExercise, WorkoutSummary, parse_sets};
#[cfg(feature = "net")]
pub use sync::{DeviceInfo, LiveSync, PairingCode, SyncClient, SyncReport, SyncStatus};
