use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use loro::{
    ContainerTrait, ExportMode, LoroDoc, LoroMap, LoroResult, LoroTree, Subscription, TreeID, TreeParentId,
    UndoManager, VersionVector,
};
use rusqlite::Connection;

use crate::projection::{
    self, COMPLETED_AT, DELETED_AT, Dirty, KIND, KIND_LIST, KIND_TASK, NOTES, Roots, TITLE,
};
use crate::{Error, Result, habits, storage};

pub const WORKSPACE: &str = "workspace";
const TASKS_TREE: &str = "tasks";
const HABITS_TREE: &str = "habits";
const HABIT_LOG: &str = "habit_log";

/// Id of the built-in Inbox list. It isn't a node: tasks at the root of the tree are in the Inbox,
/// so there is nothing to create and two fresh devices can't end up with an Inbox each.
pub const INBOX: &str = "inbox";
const INBOX_TITLE: &str = "Inbox";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub id: String,
    pub title: String,
    /// Open top-level tasks.
    pub open_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub list_id: String,
    /// Set for subtasks.
    pub parent_id: Option<String>,
    pub title: String,
    /// Markdown.
    pub notes: String,
    /// Unix millis.
    pub completed_at: Option<i64>,
}

/// A task in display order, with its nesting depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    pub task: Task,
    pub depth: u32,
    /// The task or one of its ancestors is completed.
    pub done: bool,
}

/// A list as shown: open tasks (with all their subtasks) in user order, then completed tasks,
/// most recently completed first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    pub open: Vec<OutlineRow>,
    pub completed: Vec<OutlineRow>,
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
    pub(crate) habits: LoroTree,
    pub(crate) habit_log: LoroMap,
    pub(crate) inner: Mutex<Inner>,
    dirty: Arc<Mutex<Dirty>>,
    /// This session's local edits; remote changes are never undone.
    history: Mutex<UndoManager>,
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
        let habits = doc.get_tree(HABITS_TREE);
        habits.enable_fractional_index(0);
        let habit_log = doc.get_map(HABIT_LOG);

        let tx = db.transaction()?;
        if storage::needs_compaction(&tx, WORKSPACE)? {
            storage::replace_with_snapshot(&tx, WORKSPACE, &doc.export(ExportMode::Snapshot)?)?;
        }
        if storage::ensure_read_model(&tx)? {
            projection::rebuild(&tasks, &tx)?;
            habits::rebuild(&habits, &habit_log, &tx)?;
        }
        let server_vv = storage::kv_get(&tx, SERVER_VV_KEY)?
            .map(|b| VersionVector::decode(&b))
            .transpose()?
            .unwrap_or_default();
        tx.commit()?;

        // Subscribe only after loading: stored blobs are already reflected in the read model.
        let dirty = Arc::new(Mutex::new(Dirty::default()));
        let subscription = {
            let dirty = dirty.clone();
            let roots = Roots {
                tasks: tasks.id(),
                habits: habits.id(),
                habit_log: habit_log.id(),
            };
            doc.subscribe_root(Arc::new(move |event| {
                dirty.lock().unwrap().record(&roots, &event);
            }))
        };

        // Created after loading, so stored history isn't undoable.
        let history = Mutex::new(UndoManager::new(&doc));

        Ok(Self {
            inner: Mutex::new(Inner {
                db,
                saved_vv: doc.oplog_vv(),
                server_vv,
            }),
            doc,
            tasks,
            habits,
            habit_log,
            dirty,
            history,
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

    /// Adds a task at the end of `parent`: a list, [`INBOX`], or a task (for a subtask).
    pub fn add_task(&self, parent: &str, title: &str) -> Result<String> {
        let title = clean_title(title)?;
        let parent = self.container(parent)?;
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

    /// Replaces the notes; the text CRDT merges concurrent edits from other devices.
    pub fn set_notes(&self, id: &str, notes: &str) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| {
            // Mergeable: two devices adding the first notes concurrently share one text.
            let text = tree.get_meta(id)?.ensure_mergeable_text(NOTES)?;
            text.update(notes, Default::default())
                .map_err(|e| Error::Invalid(format!("notes: {e}")))
        })
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

    /// Moves a task under `parent` (a list, [`INBOX`] or a task) to `index` among its sibling tasks.
    pub fn move_task(&self, id: &str, parent: &str, index: usize) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        let parent = self.container(parent)?;
        self.write(|tree| place(tree, id, parent, KIND_TASK, index))
    }

    /// Moves a task under `parent` right before its child `before`, or to the end when `None`.
    /// Anchoring on a sibling keeps drag-and-drop exact when some siblings (completed ones) are
    /// shown elsewhere.
    pub fn move_task_before(&self, id: &str, parent: &str, before: Option<&str>) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        let parent = self.container(parent)?;
        let before = before.map(|b| self.node(b, &[KIND_TASK])).transpose()?;
        if let Some(before) = before
            && self.tasks.parent(before) != Some(parent)
        {
            return Err(Error::Invalid("`before` must be a child of `parent`".into()));
        }
        self.write(|tree| {
            match before {
                Some(before) if before != id => tree.mov_before(id, before)?,
                Some(_) => {}
                None => tree.mov(id, parent)?,
            }
            Ok(())
        })
    }

    /// Moves a list to `index` among the other lists. The Inbox always comes first.
    pub fn move_list(&self, id: &str, index: usize) -> Result<()> {
        let id = self.node(id, &[KIND_LIST])?;
        self.write(|tree| place(tree, id, TreeParentId::Root, KIND_LIST, index))
    }

    /// Deletes a list or task together with everything under it. Undoable; see [`DELETED_AT`].
    pub fn delete(&self, id: &str) -> Result<()> {
        let id = self.node(id, &[KIND_LIST, KIND_TASK])?;
        self.write(|tree| Ok(tree.get_meta(id)?.insert(DELETED_AT, now_millis())?))
    }

    // ---- undo -----------------------------------------------------------------------------

    /// Reverts this session's last change (from any window or widget of this app). Returns false if
    /// there was nothing to undo.
    pub fn undo(&self) -> Result<bool> {
        self.step_history(UndoManager::undo)
    }

    pub fn redo(&self) -> Result<bool> {
        self.step_history(UndoManager::redo)
    }

    pub fn can_undo(&self) -> bool {
        self.history.lock().unwrap().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.lock().unwrap().can_redo()
    }

    // ---- queries (read model) -------------------------------------------------------------

    /// The Inbox, then the user's lists in order.
    pub fn lists(&self) -> Result<Vec<List>> {
        let inner = self.lock();
        let open = |list: &str| -> rusqlite::Result<u32> {
            inner
                .db
                .prepare_cached(OPEN_COUNT)?
                .query_row([list], |r| r.get(0))
        };
        let mut lists = vec![List {
            id: INBOX.into(),
            title: INBOX_TITLE.into(),
            open_count: open(INBOX)?,
        }];
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT id, title, ({}) FROM list ORDER BY position, id",
            OPEN_COUNT.replace("?1", "list.id")
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok(List {
                id: r.get(0)?,
                title: r.get(1)?,
                open_count: r.get(2)?,
            })
        })?;
        for row in rows {
            lists.push(row?);
        }
        Ok(lists)
    }

    /// All tasks of a list (or [`INBOX`]), top-level first, each level in user order.
    pub fn tasks(&self, list_id: &str) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(
            "SELECT id, list_id, parent_id, title, notes, completed_at FROM task
             WHERE list_id = ?1 ORDER BY parent_id IS NOT NULL, parent_id, position, id",
        )?;
        let rows = stmt.query_map([list_id], |r| {
            Ok(Task {
                id: r.get(0)?,
                list_id: r.get(1)?,
                parent_id: r.get(2)?,
                title: r.get(3)?,
                notes: r.get(4)?,
                completed_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The tasks of a list arranged for display. See [`Outline`].
    pub fn outline(&self, list_id: &str) -> Result<Outline> {
        let mut children: HashMap<Option<String>, Vec<Task>> = HashMap::new();
        // `tasks` returns each parent's children in user order.
        for task in self.tasks(list_id)? {
            children.entry(task.parent_id.clone()).or_default().push(task);
        }
        let (mut completed, open): (Vec<_>, Vec<_>) = children
            .remove(&None)
            .unwrap_or_default()
            .into_iter()
            .partition(|t| t.completed_at.is_some());
        completed.sort_by_key(|t| std::cmp::Reverse(t.completed_at));

        let mut outline = Outline::default();
        for task in open {
            push_subtree(&mut outline.open, &mut children, task, 0, false);
        }
        for task in completed {
            push_subtree(&mut outline.completed, &mut children, task, 0, true);
        }
        Ok(outline)
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

    fn step_history(&self, step: impl FnOnce(&mut UndoManager) -> LoroResult<bool>) -> Result<bool> {
        let (applied, changed) = {
            let mut inner = self.lock();
            let applied = step(&mut self.history.lock().unwrap());
            self.doc.commit();
            (applied, self.flush(&mut inner)?)
        };
        if changed {
            self.notify(ChangeOrigin::Local);
        }
        Ok(applied?)
    }

    fn write<T>(&self, mutate: impl FnOnce(&LoroTree) -> Result<T>) -> Result<T> {
        self.transact(|| mutate(&self.tasks))
    }

    /// Runs `mutate` as one Loro commit (one undo step), then persists and projects it.
    pub(crate) fn transact<T>(&self, mutate: impl FnOnce() -> Result<T>) -> Result<T> {
        let (out, changed) = {
            let mut inner = self.lock();
            let out = mutate();
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
        projection::apply(&self.tasks, &tx, dirty.tasks)?;
        if dirty.habits {
            habits::project_habits(&self.habits, &tx)?;
        }
        habits::project_log(&self.habit_log, &tx, &dirty.habit_log)?;
        tx.commit()?;
        *saved_vv = vv;
        Ok(true)
    }

    /// Where tasks can go: the Inbox (the tree root), a list, or a task.
    fn container(&self, id: &str) -> Result<TreeParentId> {
        if id == INBOX {
            return Ok(TreeParentId::Root);
        }
        Ok(TreeParentId::Node(self.node(id, &[KIND_LIST, KIND_TASK])?))
    }

    fn node(&self, id: &str, kinds: &[&str]) -> Result<TreeID> {
        if id == INBOX {
            return Err(Error::Invalid(
                "the Inbox is built in and can't be changed".into(),
            ));
        }
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

/// Open top-level tasks of list `?1`.
const OPEN_COUNT: &str =
    "SELECT count(*) FROM task WHERE list_id = ?1 AND parent_id IS NULL AND completed_at IS NULL";

fn push_subtree(
    rows: &mut Vec<OutlineRow>,
    children: &mut HashMap<Option<String>, Vec<Task>>,
    task: Task,
    depth: u32,
    ancestor_done: bool,
) {
    let done = ancestor_done || task.completed_at.is_some();
    let kids = children.remove(&Some(task.id.clone())).unwrap_or_default();
    rows.push(OutlineRow { task, depth, done });
    for kid in kids {
        push_subtree(rows, children, kid, depth + 1, done);
    }
}

/// Moves `id` under `parent` so that it ends up at `index` among siblings of the same kind. The root
/// holds both lists and Inbox tasks, so a plain child index would count the other kind too.
fn place(tree: &LoroTree, id: TreeID, parent: TreeParentId, kind: &str, index: usize) -> Result<()> {
    let siblings: Vec<TreeID> = tree
        .children(parent)
        .unwrap_or_default()
        .into_iter()
        .filter(|&s| s != id && projection::kind(tree, s).as_deref() == Some(kind))
        .collect();
    match (siblings.get(index), siblings.last()) {
        (Some(&next), _) => tree.mov_before(id, next)?,
        (None, Some(&last)) => tree.mov_after(id, last)?,
        (None, None) => tree.mov(id, parent)?,
    }
    Ok(())
}

pub(crate) fn clean_title(title: &str) -> Result<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::Invalid("title is empty".into()));
    }
    Ok(title.to_owned())
}

pub(crate) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
