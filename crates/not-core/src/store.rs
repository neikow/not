use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use loro::{
    ContainerTrait, ExportMode, LoroDoc, LoroTree, Subscription, TreeID, TreeParentId, VersionVector,
};
use rusqlite::Connection;

use crate::projection::{self, COMPLETED_AT, Dirty, KIND, KIND_LIST, KIND_TASK, TITLE};
use crate::{Error, Result, storage};

pub const WORKSPACE: &str = "workspace";
const TASKS_TREE: &str = "tasks";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub list_id: String,
    /// Set for subtasks.
    pub parent_id: Option<String>,
    pub title: String,
    /// Unix millis.
    pub completed_at: Option<i64>,
}

/// Where a change to the store came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOrigin {
    /// A command on this device.
    Local,
    /// Updates pulled from the server.
    Remote,
}

pub type Listener = Arc<dyn Fn(ChangeOrigin) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

/// One device's replica: the Loro workspace doc plus its SQLite persistence and read model.
///
/// All mutations go through [`Store::write`], which commits the Loro transaction, appends the update
/// blob and refreshes the read model in a single SQLite transaction.
pub struct Store {
    pub(crate) doc: LoroDoc,
    tasks: LoroTree,
    pub(crate) inner: Mutex<Inner>,
    dirty: Arc<Mutex<Dirty>>,
    pub(crate) device_id: String,
    listeners: Mutex<Vec<(ListenerId, Listener)>>,
    next_listener: AtomicU64,
    _subscription: Subscription,
}

pub(crate) struct Inner {
    pub(crate) db: Connection,
    /// Version already persisted to `doc_update`.
    saved_vv: VersionVector,
    /// Version the server is known to hold; pushes export everything after it.
    pub(crate) server_vv: VersionVector,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut db: Connection) -> Result<Self> {
        storage::init(&db)?;

        let doc = LoroDoc::new();
        for blob in storage::load_doc(&db, WORKSPACE)? {
            doc.import(&blob)?;
        }
        let tasks = doc.get_tree(TASKS_TREE);
        tasks.enable_fractional_index(0);

        let tx = db.transaction()?;
        if storage::needs_compaction(&tx, WORKSPACE)? {
            storage::replace_with_snapshot(&tx, WORKSPACE, &doc.export(ExportMode::Snapshot)?)?;
        }
        if storage::ensure_read_model(&tx)? {
            projection::rebuild(&tasks, &tx)?;
        }
        let device_id = match storage::kv_get(&tx, "device_id")? {
            Some(id) => String::from_utf8(id).map_err(|e| Error::Invalid(e.to_string()))?,
            None => {
                let id = format!("{:016x}", doc.peer_id());
                storage::kv_set(&tx, "device_id", id.as_bytes())?;
                id
            }
        };
        let server_vv = storage::kv_get(&tx, SERVER_VV_KEY)?
            .map(|b| VersionVector::decode(&b))
            .transpose()?
            .unwrap_or_default();
        tx.commit()?;

        // Subscribe only after loading: stored blobs are already reflected in the read model.
        let dirty = Arc::new(Mutex::new(Dirty::default()));
        let subscription = {
            let dirty = dirty.clone();
            let tree_id = tasks.id();
            doc.subscribe_root(Arc::new(move |event| {
                dirty.lock().unwrap().record(&tree_id, &event);
            }))
        };

        Ok(Self {
            inner: Mutex::new(Inner {
                db,
                saved_vv: doc.oplog_vv(),
                server_vv,
            }),
            doc,
            tasks,
            dirty,
            device_id,
            listeners: Mutex::default(),
            next_listener: AtomicU64::new(0),
            _subscription: subscription,
        })
    }

    // ---- commands -------------------------------------------------------------------------

    pub fn create_list(&self, title: &str) -> Result<String> {
        let title = clean_title(title)?;
        self.write(|tree| {
            let id = tree.create(TreeParentId::Root)?;
            let meta = tree.get_meta(id)?;
            meta.insert(KIND, KIND_LIST)?;
            meta.insert(TITLE, title)?;
            Ok(id.to_string())
        })
    }

    /// Adds a task at the end of `parent`, which is a list or (for a subtask) a task.
    pub fn add_task(&self, parent: &str, title: &str) -> Result<String> {
        let title = clean_title(title)?;
        let parent = self.node(parent, &[KIND_LIST, KIND_TASK])?;
        self.write(|tree| {
            let id = tree.create(parent)?;
            let meta = tree.get_meta(id)?;
            meta.insert(KIND, KIND_TASK)?;
            meta.insert(TITLE, title)?;
            Ok(id.to_string())
        })
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<()> {
        let title = clean_title(title)?;
        let id = self.node(id, &[KIND_LIST, KIND_TASK])?;
        self.write(|tree| Ok(tree.get_meta(id)?.insert(TITLE, title)?))
    }

    pub fn set_completed(&self, id: &str, completed: bool) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| {
            let meta = tree.get_meta(id)?;
            if completed {
                meta.insert(COMPLETED_AT, now_millis())?;
            } else {
                meta.delete(COMPLETED_AT)?;
            }
            Ok(())
        })
    }

    /// Moves a task under `parent` (a list or a task) at `index` among its siblings.
    pub fn move_task(&self, id: &str, parent: &str, index: usize) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        let parent = self.node(parent, &[KIND_LIST, KIND_TASK])?;
        self.write(|tree| Ok(tree.mov_to(id, parent, index)?))
    }

    pub fn move_list(&self, id: &str, index: usize) -> Result<()> {
        let id = self.node(id, &[KIND_LIST])?;
        self.write(|tree| Ok(tree.mov_to(id, TreeParentId::Root, index)?))
    }

    /// Deletes a list or task together with everything under it.
    pub fn delete(&self, id: &str) -> Result<()> {
        let id = self.node(id, &[KIND_LIST, KIND_TASK])?;
        self.write(|tree| Ok(tree.delete(id)?))
    }

    // ---- queries (read model) -------------------------------------------------------------

    pub fn lists(&self) -> Result<Vec<List>> {
        let inner = self.lock();
        let mut stmt = inner
            .db
            .prepare_cached("SELECT id, title FROM list ORDER BY position, id")?;
        let rows = stmt.query_map([], |r| {
            Ok(List {
                id: r.get(0)?,
                title: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// All tasks of a list, top-level first, each level in user order.
    pub fn tasks(&self, list_id: &str) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(
            "SELECT id, list_id, parent_id, title, completed_at FROM task
             WHERE list_id = ?1 ORDER BY parent_id IS NOT NULL, parent_id, position, id",
        )?;
        let rows = stmt.query_map([list_id], |r| {
            Ok(Task {
                id: r.get(0)?,
                list_id: r.get(1)?,
                parent_id: r.get(2)?,
                title: r.get(3)?,
                completed_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ---- change notifications -------------------------------------------------------------

    /// Called after every committed change, outside the store lock, on the thread that made it.
    pub fn add_listener(&self, listener: impl Fn(ChangeOrigin) + Send + Sync + 'static) -> ListenerId {
        let id = ListenerId(self.next_listener.fetch_add(1, Ordering::Relaxed));
        self.listeners.lock().unwrap().push((id, Arc::new(listener)));
        id
    }

    pub fn remove_listener(&self, id: ListenerId) {
        self.listeners.lock().unwrap().retain(|(l, _)| *l != id);
    }

    pub(crate) fn notify(&self, origin: ChangeOrigin) {
        let listeners: Vec<Listener> = self
            .listeners
            .lock()
            .unwrap()
            .iter()
            .map(|(_, l)| l.clone())
            .collect();
        for listener in listeners {
            listener(origin);
        }
    }

    // ---- internals ------------------------------------------------------------------------

    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap()
    }

    fn write<T>(&self, mutate: impl FnOnce(&LoroTree) -> Result<T>) -> Result<T> {
        let (out, changed) = {
            let mut inner = self.lock();
            let out = mutate(&self.tasks);
            // Commit and flush even on error so the read model never lags behind applied ops.
            self.doc.commit();
            (out, self.flush(&mut inner)?)
        };
        if changed {
            self.notify(ChangeOrigin::Local);
        }
        out
    }

    /// Persists ops not yet saved and applies pending read-model changes, atomically.
    /// Returns whether anything changed.
    pub(crate) fn flush(&self, inner: &mut Inner) -> Result<bool> {
        let dirty = std::mem::take(&mut *self.dirty.lock().unwrap());
        let vv = self.doc.oplog_vv();
        if vv == inner.saved_vv && dirty.is_empty() {
            return Ok(false);
        }
        let Inner { db, saved_vv, .. } = inner;
        let tx = db.transaction()?;
        if vv != *saved_vv {
            let update = self.doc.export(ExportMode::updates(saved_vv))?;
            storage::append_update(&tx, WORKSPACE, &update)?;
        }
        projection::apply(&self.tasks, &tx, dirty)?;
        tx.commit()?;
        *saved_vv = vv;
        Ok(true)
    }

    fn node(&self, id: &str, kinds: &[&str]) -> Result<TreeID> {
        let tree_id = TreeID::try_from(id).map_err(|_| Error::Invalid(format!("bad id {id:?}")))?;
        if !projection::is_alive(&self.tasks, tree_id) {
            return Err(Error::NotFound(id.to_owned()));
        }
        match projection::kind(&self.tasks, tree_id) {
            Some(kind) if kinds.contains(&kind.as_str()) => Ok(tree_id),
            kind => Err(Error::Invalid(format!(
                "{id} is {kind:?}, expected one of {kinds:?}"
            ))),
        }
    }
}

pub(crate) const SERVER_VV_KEY: &str = "server_vv:workspace";

fn clean_title(title: &str) -> Result<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::Invalid("title is empty".into()));
    }
    Ok(title.to_owned())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
