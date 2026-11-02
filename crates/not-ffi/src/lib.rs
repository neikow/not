//! uniffi surface of `not-core` for the Swift (macOS) and Kotlin (Android) apps.
//!
//! Naming avoids Swift's `Task`: records are `TaskList` / `TaskItem`.

use std::sync::{Arc, Mutex};

use not_core::{LiveSync, Store, SyncClient, SyncStatus};

uniffi::setup_scaffolding!();

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
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TaskItem {
    pub id: String,
    pub list_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    /// Unix millis.
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum SyncState {
    Syncing,
    Synced,
    Offline { reason: String },
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

    pub fn device_id(&self) -> String {
        self.store.device_id().to_owned()
    }

    // ---- commands ----

    pub fn create_list(&self, title: String) -> Result<String, NotError> {
        Ok(self.store.create_list(&title)?)
    }

    /// `parent_id` is a list, or a task for a subtask.
    pub fn add_task(&self, parent_id: String, title: String) -> Result<String, NotError> {
        Ok(self.store.add_task(&parent_id, &title)?)
    }

    pub fn rename(&self, id: String, title: String) -> Result<(), NotError> {
        Ok(self.store.rename(&id, &title)?)
    }

    pub fn set_completed(&self, id: String, completed: bool) -> Result<(), NotError> {
        Ok(self.store.set_completed(&id, completed)?)
    }

    pub fn move_task(&self, id: String, parent_id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_task(&id, &parent_id, index as usize)?)
    }

    pub fn delete_item(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete(&id)?)
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
            })
            .collect())
    }

    pub fn tasks(&self, list_id: String) -> Result<Vec<TaskItem>, NotError> {
        Ok(self
            .store
            .tasks(&list_id)?
            .into_iter()
            .map(|t| TaskItem {
                id: t.id,
                list_id: t.list_id,
                parent_id: t.parent_id,
                title: t.title,
                completed_at: t.completed_at,
            })
            .collect())
    }

    // ---- sync ----

    /// Starts (or restarts with new settings) background sync with the server.
    pub fn start_sync(&self, server_url: String, token: String) {
        let mut live = self.live.lock().unwrap();
        live.take();
        let observer = self.observer.clone();
        *live = Some(LiveSync::start(
            self.runtime.handle(),
            self.store.clone(),
            SyncClient::new(&server_url, &token),
            move |status| {
                if let Some(o) = current(&observer) {
                    o.on_sync_state(status.into());
                }
            },
        ));
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
}

impl From<SyncStatus> for SyncState {
    fn from(s: SyncStatus) -> Self {
        match s {
            SyncStatus::Syncing => Self::Syncing,
            SyncStatus::Synced => Self::Synced,
            SyncStatus::Offline(reason) => Self::Offline { reason },
        }
    }
}

/// Clones the observer out so callbacks never run while holding the lock.
fn current(observer: &SharedObserver) -> Option<Arc<dyn StoreObserver>> {
    observer.lock().unwrap().clone()
}
