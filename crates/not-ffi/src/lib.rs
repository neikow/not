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

    pub fn set_notes(&self, id: String, notes: String) -> Result<(), NotError> {
        Ok(self.store.set_notes(&id, &notes)?)
    }

    pub fn move_task(&self, id: String, parent_id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_task(&id, &parent_id, index as usize)?)
    }

    /// Moves a task under `parent_id` right before its child `before_id`, or to the end.
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

    /// Moves a list to `index` among the other lists (the Inbox stays first).
    pub fn move_list(&self, id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_list(&id, index as usize)?)
    }

    /// Deletes a list or task and everything under it. Undoable.
    pub fn delete_item(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete(&id)?)
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
