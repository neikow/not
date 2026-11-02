//! Shared core of `not`: Loro documents, SQLite persistence and read model, commands, sync.

mod error;
mod projection;
mod storage;
mod store;
mod sync;

pub use error::{Error, Result};
pub use not_protocol::PairingLink;
pub use store::{ChangeOrigin, INBOX, List, ListenerId, Outline, OutlineRow, Store, Task, WORKSPACE};
pub use sync::{Credentials, Outgoing};
#[cfg(feature = "net")]
pub use sync::{DeviceInfo, LiveSync, PairingCode, SyncClient, SyncReport, SyncStatus};
