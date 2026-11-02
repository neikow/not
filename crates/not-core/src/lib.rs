//! Shared core of `not`: Loro documents, SQLite persistence and read model, commands, sync.

mod date;
mod error;
mod habits;
pub mod markdown;
mod notes;
mod projection;
mod storage;
mod store;
mod sync;

pub use error::{Error, Result};
pub use habits::{EVERY_DAY, Habit, HabitDay, HabitSummary};
pub use not_protocol::PairingLink;
pub use notes::{Folder, NoteSummary};
pub use store::{ChangeOrigin, INBOX, List, ListenerId, Outline, OutlineRow, Store, Task, WORKSPACE};
pub use sync::{Credentials, Outgoing};
#[cfg(feature = "net")]
pub use sync::{DeviceInfo, LiveSync, PairingCode, SyncClient, SyncReport, SyncStatus};
