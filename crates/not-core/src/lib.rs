//! Shared core of `not`: Loro documents, SQLite persistence and read model, commands, sync.

mod error;
mod projection;
mod storage;
mod store;
mod sync;

pub use error::{Error, Result};
pub use store::{ChangeOrigin, List, ListenerId, Store, Task, WORKSPACE};
pub use sync::Outgoing;
#[cfg(feature = "net")]
pub use sync::{LiveSync, SyncClient, SyncReport, SyncStatus};
