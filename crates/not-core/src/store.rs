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

use crate::date::Date;
use crate::parse::{self, ParsedTask};
use crate::projection::{
    self, COMPLETED_AT, DELETED_AT, DUE, DUE_TIME, Dirty, KIND, KIND_LIST, KIND_TASK, NOTES, PRIORITY,
    REPEAT, Roots, TITLE, meta_i64, meta_str, meta_text,
};
use crate::schedule::{Repeat, Schedule};
use crate::{Error, Result, habits, notes, storage, workouts};

pub const WORKSPACE: &str = "workspace";
const TASKS_TREE: &str = "tasks";
const HABITS_TREE: &str = "habits";
const HABIT_LOG: &str = "habit_log";
const NOTES_TREE: &str = "notes";
const WORKOUTS_TREE: &str = "workouts";
const EXERCISE_LINKS: &str = "exercise_links";
/// List id (or [`INBOX`]) → how its open tasks are sorted; absent means [`TaskSort::Manual`].
const TASK_SORT: &str = "task_sort";
/// `"all"` → how All Notes sorts (see [`crate::NoteSort`]); folders keep theirs on the folder.
const NOTE_SORT: &str = "note_sort";

/// Completed tasks stay in their list this long, then move to the archive.
pub const ARCHIVE_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

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
    pub sort: TaskSort,
}

/// How a list orders its open tasks (each level of subtasks too). Ties keep the manual order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskSort {
    /// The order they were dragged into; new tasks go first.
    #[default]
    Manual,
    /// Soonest due first, undated last; then by time and priority.
    Due,
    /// Highest priority first, then soonest due.
    Priority,
    /// Alphabetical by title.
    Title,
}

impl TaskSort {
    fn key(self) -> Option<&'static str> {
        match self {
            Self::Manual => None,
            Self::Due => Some("due"),
            Self::Priority => Some("priority"),
            Self::Title => Some("title"),
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "due" => Self::Due,
            "priority" => Self::Priority,
            "title" => Self::Title,
            _ => Self::Manual,
        }
    }

    fn compare(self, a: &Task, b: &Task) -> std::cmp::Ordering {
        // Undated (or no time) sorts after dated.
        let due = |t: &Task| (t.due.is_none(), t.due.clone(), t.due_time.is_none(), t.due_time);
        let priority = |t: &Task| std::cmp::Reverse(t.priority);
        match self {
            Self::Manual => std::cmp::Ordering::Equal,
            Self::Due => due(a).cmp(&due(b)).then(priority(a).cmp(&priority(b))),
            Self::Priority => priority(a).cmp(&priority(b)).then(due(a).cmp(&due(b))),
            Self::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        }
    }
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
    /// `YYYY-MM-DD`.
    pub due: Option<String>,
    /// Minutes after midnight.
    pub due_time: Option<u32>,
    /// 0 = none, 1 = low, 2 = medium, 3 = high.
    pub priority: u8,
    /// RRULE subset, see [`Repeat`].
    pub repeat: Option<String>,
}

/// A task in display order, with its nesting depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    pub task: Task,
    pub depth: u32,
    /// The task or one of its ancestors is completed.
    pub done: bool,
}

/// A list as shown: open tasks (with all their subtasks) in user order, then tasks completed in the
/// last [`ARCHIVE_AFTER_MS`], most recently completed first. Older ones are in [`Store::archive`].
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
    pub(crate) tasks: LoroTree,
    pub(crate) habits: LoroTree,
    pub(crate) habit_log: LoroMap,
    pub(crate) notes: LoroTree,
    pub(crate) workouts: LoroTree,
    /// The user's exercise name keys → catalogue ids; see [`crate::catalog`].
    pub(crate) exercise_links: LoroMap,
    task_sort: LoroMap,
    pub(crate) note_sort: LoroMap,
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
        let notes = doc.get_tree(NOTES_TREE);
        notes.enable_fractional_index(0);
        let workouts = doc.get_tree(WORKOUTS_TREE);
        workouts.enable_fractional_index(0);
        let exercise_links = doc.get_map(EXERCISE_LINKS);
        let task_sort = doc.get_map(TASK_SORT);
        let note_sort = doc.get_map(NOTE_SORT);

        let tx = db.transaction()?;
        if storage::needs_compaction(&tx, WORKSPACE)? {
            storage::replace_with_snapshot(&tx, WORKSPACE, &doc.export(ExportMode::Snapshot)?)?;
        }
        if storage::ensure_read_model(&tx)? {
            projection::rebuild(&tasks, &tx)?;
            habits::rebuild(&habits, &habit_log, &tx)?;
            notes::rebuild(&notes, &tx)?;
            workouts::rebuild(&workouts, &tx)?;
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
                notes: notes.id(),
                workouts: workouts.id(),
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
            notes,
            workouts,
            exercise_links,
            task_sort,
            note_sort,
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

    /// Adds a task to `parent`: first in a list or [`INBOX`], last under a task (a subtask, so
    /// subtasks typed one after another keep their order).
    pub fn add_task(&self, parent: &str, title: &str) -> Result<String> {
        let title = clean_title(title)?;
        let parent = self.container(parent)?;
        self.write(|tree| {
            let id = create_task_node(tree, parent)?;
            let meta = tree.get_meta(id)?;
            meta.insert(KIND, KIND_TASK)?;
            meta.insert(TITLE, title)?;
            Ok(id.to_string())
        })
    }

    /// Adds a task with a schedule and priority in one step (one undo step), placed as in
    /// [`Store::add_task`].
    pub fn add_task_with(
        &self,
        parent: &str,
        title: &str,
        schedule: &Schedule,
        priority: u8,
    ) -> Result<String> {
        let title = clean_title(title)?;
        let parent = self.container(parent)?;
        self.write(|tree| {
            let id = create_task_node(tree, parent)?;
            let meta = tree.get_meta(id)?;
            meta.insert(KIND, KIND_TASK)?;
            meta.insert(TITLE, title)?;
            write_schedule(&meta, schedule)?;
            write_priority(&meta, priority)?;
            Ok(id.to_string())
        })
    }

    /// Parses `input` (see [`parse::parse_task`]) and adds the task to the list it names, or else to
    /// `parent`, due on `default_due` unless the input says when. Returns the new task's id.
    pub fn quick_add(
        &self,
        parent: &str,
        input: &str,
        today: Date,
        dismissed: &[String],
        default_due: Option<Date>,
    ) -> Result<String> {
        let mut parsed = self.parse_task(input, today, dismissed)?;
        parsed.schedule.due = parsed.schedule.due.or(default_due);
        // A title made only of tokens ("tomorrow") keeps its words rather than being refused.
        let title = if parsed.title.is_empty() {
            input
        } else {
            &parsed.title
        };
        let parent = parsed.list_id.as_deref().unwrap_or(parent);
        self.add_task_with(parent, title, &parsed.schedule, parsed.priority)
    }

    /// [`parse::parse_task`] against this store's lists.
    pub fn parse_task(&self, input: &str, today: Date, dismissed: &[String]) -> Result<ParsedTask> {
        let lists: Vec<_> = self.lists()?.into_iter().map(|l| (l.id, l.title)).collect();
        Ok(parse::parse_task(input, today, &lists, dismissed))
    }

    /// Sets or (with no due date) clears when the task is due and how it repeats.
    pub fn set_schedule(&self, id: &str, schedule: &Schedule) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| write_schedule(&tree.get_meta(id)?, schedule))
    }

    /// 0 = none … 3 = high.
    pub fn set_priority(&self, id: &str, priority: u8) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| write_priority(&tree.get_meta(id)?, priority))
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

    /// Completes or reopens a task. Completing a repeating task logs this occurrence as a completed
    /// copy and moves the task on to its next due date after `today`, with its subtasks reopened.
    pub fn set_completed(&self, id: &str, completed: bool, today: Date) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| complete_in(tree, id, completed, today))
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
    ///
    /// In a sorted list, dragging means taking over the order: the list switches to
    /// [`TaskSort::Manual`], starting from the order shown, in the same undo step.
    pub fn move_task_before(&self, id: &str, parent: &str, before: Option<&str>) -> Result<()> {
        let task = self.task(id)?;
        let id = self.node(id, &[KIND_TASK])?;
        let parent = self.container(parent)?;
        let before = before.map(|b| self.node(b, &[KIND_TASK])).transpose()?;
        if let Some(before) = before
            && self.tasks.parent(before) != Some(parent)
        {
            return Err(Error::Invalid("`before` must be a child of `parent`".into()));
        }
        // Only within its own list; a move to another list takes that list's order.
        let freeze = match self.list_sort(&task.list_id) {
            TaskSort::Manual => None,
            sort if self.list_of(parent)? == task.list_id => Some(self.sorted_children(&task.list_id, sort)?),
            _ => None,
        };
        self.write(|tree| {
            if let Some(children) = freeze {
                self.task_sort.delete(&task.list_id)?;
                for (parent, kids) in children {
                    let parent = match parent {
                        Some(p) => TreeParentId::Node(tree_id(&p)?),
                        None => self.container(&task.list_id)?,
                    };
                    // Each to the front, last first, so they end up first in the order shown.
                    for kid in kids.iter().rev().filter(|k| k.completed_at.is_none()) {
                        tree.mov_to(tree_id(&kid.id)?, parent, 0)?;
                    }
                }
            }
            match before {
                Some(before) if before != id => tree.mov_before(id, before)?,
                Some(_) => {}
                None => tree.mov(id, parent)?,
            }
            Ok(())
        })
    }

    /// Makes a task the last subtask of the open sibling right above it. Returns false when there is
    /// none (it is the first).
    pub fn indent_task(&self, id: &str) -> Result<bool> {
        let id = self.node(id, &[KIND_TASK])?;
        let parent = self
            .tasks
            .parent(id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        let siblings = self.tasks.children(parent).unwrap_or_default();
        let Some(at) = siblings.iter().position(|&s| s == id) else {
            return Ok(false);
        };
        let open_task = |s: TreeID| {
            projection::is_alive(&self.tasks, s)
                && projection::kind(&self.tasks, s).as_deref() == Some(KIND_TASK)
                && self
                    .tasks
                    .get_meta(s)
                    .is_ok_and(|m| meta_i64(&m, COMPLETED_AT).is_none())
        };
        let Some(&above) = siblings[..at].iter().rev().find(|&&s| open_task(s)) else {
            return Ok(false);
        };
        self.write(|tree| Ok(tree.mov(id, TreeParentId::Node(above))?))?;
        Ok(true)
    }

    /// Makes a subtask a sibling of its parent task, right after it. Returns false for top-level
    /// tasks.
    pub fn outdent_task(&self, id: &str) -> Result<bool> {
        let id = self.node(id, &[KIND_TASK])?;
        let Some(TreeParentId::Node(parent)) = self.tasks.parent(id) else {
            return Ok(false);
        };
        if projection::kind(&self.tasks, parent).as_deref() != Some(KIND_TASK) {
            return Ok(false);
        }
        self.write(|tree| Ok(tree.mov_after(id, parent)?))?;
        Ok(true)
    }

    /// Moves a list to `index` among the other lists. The Inbox always comes first.
    pub fn move_list(&self, id: &str, index: usize) -> Result<()> {
        let id = self.node(id, &[KIND_LIST])?;
        self.write(|tree| place(tree, id, TreeParentId::Root, KIND_LIST, index))
    }

    /// How a list (or [`INBOX`]) sorts its open tasks.
    pub fn set_list_sort(&self, list_id: &str, sort: TaskSort) -> Result<()> {
        if list_id != INBOX {
            self.node(list_id, &[KIND_LIST])?;
        }
        self.transact(|| {
            match sort.key() {
                Some(key) => self.task_sort.insert(list_id, key)?,
                None => self.task_sort.delete(list_id)?,
            }
            Ok(())
        })
    }

    pub fn list_sort(&self, list_id: &str) -> TaskSort {
        match self.task_sort.get(list_id) {
            Some(loro::ValueOrContainer::Value(v)) => {
                v.as_string().map_or(TaskSort::Manual, |s| TaskSort::from_key(s))
            }
            _ => TaskSort::Manual,
        }
    }

    /// Deletes a list or task together with everything under it. Undoable; see [`DELETED_AT`].
    pub fn delete(&self, id: &str) -> Result<()> {
        let id = self.node(id, &[KIND_LIST, KIND_TASK])?;
        self.write(|tree| Ok(tree.get_meta(id)?.insert(DELETED_AT, now_millis())?))
    }

    /// Imports Loro bytes from outside sync (a backup), persisting and projecting them.
    pub(crate) fn import_local(&self, bytes: &[u8]) -> Result<()> {
        let (result, changed) = {
            let mut inner = self.lock();
            let result = self.doc.import(bytes).map(|_| ()).map_err(Error::from);
            (result, self.flush(&mut inner)?)
        };
        if changed {
            self.notify(ChangeOrigin::Remote);
        }
        result
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
            sort: self.list_sort(INBOX),
        }];
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT id, title, ({}) FROM list ORDER BY position, id",
            OPEN_COUNT.replace("?1", "list.id")
        ))?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            Ok(List {
                sort: self.list_sort(&id),
                id,
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
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t
             WHERE list_id = ?1 ORDER BY parent_id IS NOT NULL, parent_id, position, id"
        ))?;
        let rows = stmt.query_map([list_id], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Open tasks due on or before `today`, from every list: overdue first, then by time and
    /// priority. Subtasks of completed tasks are left out.
    pub fn due_tasks(&self, today: Date) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t LEFT JOIN task p ON p.id = t.parent_id
             WHERE t.completed_at IS NULL AND t.due IS NOT NULL AND t.due <= ?1 AND p.completed_at IS NULL
             ORDER BY t.due, t.due_time IS NULL, t.due_time, t.priority DESC, t.position, t.id"
        ))?;
        let rows = stmt.query_map([today.to_string()], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Tasks from every list whose title or notes contain each word of `query` (ignoring case):
    /// open ones first, then completed ones latest first. At most `limit`.
    pub fn search_tasks(&self, query: &str, limit: u32) -> Result<Vec<Task>> {
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t
             ORDER BY t.completed_at IS NOT NULL, t.completed_at DESC, t.due IS NULL, t.due, t.position, t.id"
        ))?;
        // Filtered here rather than with LIKE, which only folds ASCII case ("Été" vs "été").
        let mut found = Vec::new();
        for task in stmt.query_map([], task_row)? {
            let task = task?;
            let haystack = format!("{}\n{}", task.title, task.notes).to_lowercase();
            if words.iter().all(|w| haystack.contains(w.as_str())) {
                found.push(task);
                if found.len() as u32 >= limit {
                    break;
                }
            }
        }
        Ok(found)
    }

    /// Open tasks due after `today`, from every list, soonest first, then by time and priority.
    /// Subtasks of completed tasks are left out.
    pub fn upcoming_tasks(&self, today: Date) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t LEFT JOIN task p ON p.id = t.parent_id
             WHERE t.completed_at IS NULL AND t.due IS NOT NULL AND t.due > ?1 AND p.completed_at IS NULL
             ORDER BY t.due, t.due_time IS NULL, t.due_time, t.priority DESC, t.position, t.id"
        ))?;
        let rows = stmt.query_map([today.to_string()], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The tasks of a list arranged for display. See [`Outline`].
    pub fn outline(&self, list_id: &str) -> Result<Outline> {
        self.outline_at(list_id, now_millis())
    }

    /// [`Store::outline`] as of `now` (Unix millis).
    pub fn outline_at(&self, list_id: &str, now: i64) -> Result<Outline> {
        let mut children = self.sorted_children(list_id, self.list_sort(list_id))?;
        let (mut completed, open): (Vec<_>, Vec<_>) = children
            .remove(&None)
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.completed_at.is_none_or(|at| at > now - ARCHIVE_AFTER_MS))
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

    /// Top-level tasks from every list completed more than [`ARCHIVE_AFTER_MS`] before `now`,
    /// most recently completed first. Their subtasks go with them.
    pub fn archive(&self, now: i64) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t
             WHERE t.parent_id IS NULL AND t.completed_at IS NOT NULL AND t.completed_at <= ?1
             ORDER BY t.completed_at DESC, t.id"
        ))?;
        let rows = stmt.query_map([now - ARCHIVE_AFTER_MS], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Deletes everything in the archive as of `now`, in one undo step. Returns how many tasks.
    pub fn clear_archive(&self, now: i64) -> Result<u32> {
        let ids = self
            .archive(now)?
            .into_iter()
            .map(|t| tree_id(&t.id))
            .collect::<Result<Vec<_>>>()?;
        let deleted_at = now_millis();
        self.write(|tree| {
            for &id in &ids {
                tree.get_meta(id)?.insert(DELETED_AT, deleted_at)?;
            }
            Ok(ids.len() as u32)
        })
    }

    /// A list's tasks grouped by parent task (`None` for top-level), each group in `sort` order.
    fn sorted_children(&self, list_id: &str, sort: TaskSort) -> Result<HashMap<Option<String>, Vec<Task>>> {
        let mut children: HashMap<Option<String>, Vec<Task>> = HashMap::new();
        // `tasks` returns each parent's children in user order, which the stable sort keeps for ties.
        for task in self.tasks(list_id)? {
            children.entry(task.parent_id.clone()).or_default().push(task);
        }
        if sort != TaskSort::Manual {
            for kids in children.values_mut() {
                kids.sort_by(|a, b| sort.compare(a, b));
            }
        }
        Ok(children)
    }

    /// One task by id, from any list.
    pub fn task_by_id(&self, id: &str) -> Result<Task> {
        self.task(id)
    }

    fn task(&self, id: &str) -> Result<Task> {
        let inner = self.lock();
        let mut stmt = inner
            .db
            .prepare_cached(&format!("SELECT {TASK_COLUMNS} FROM task t WHERE id = ?1"))?;
        match stmt.query_row([id], task_row) {
            Ok(task) => Ok(task),
            Err(rusqlite::Error::QueryReturnedNoRows) => Err(Error::NotFound(id.to_owned())),
            Err(e) => Err(e.into()),
        }
    }

    /// The list a container (the root, a list or a task) belongs to.
    fn list_of(&self, container: TreeParentId) -> Result<String> {
        match container {
            TreeParentId::Node(node) if projection::kind(&self.tasks, node).as_deref() == Some(KIND_LIST) => {
                Ok(node.to_string())
            }
            TreeParentId::Node(node) => Ok(self.task(&node.to_string())?.list_id),
            _ => Ok(INBOX.into()),
        }
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
        notes::apply(&self.notes, &tx, dirty.notes)?;
        if dirty.workouts {
            workouts::rebuild(&self.workouts, &tx)?;
        }
        tx.commit()?;
        *saved_vv = vv;
        Ok(true)
    }

    /// Where tasks can go: the Inbox (the tree root), a list, or a task.
    pub(crate) fn container(&self, id: &str) -> Result<TreeParentId> {
        if id == INBOX {
            return Ok(TreeParentId::Root);
        }
        Ok(TreeParentId::Node(self.node(id, &[KIND_LIST, KIND_TASK])?))
    }

    pub(crate) fn node(&self, id: &str, kinds: &[&str]) -> Result<TreeID> {
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

const TASK_COLUMNS: &str =
    "t.id, t.list_id, t.parent_id, t.title, t.notes, t.completed_at, t.due, t.due_time, t.priority, t.repeat";

fn task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        list_id: r.get(1)?,
        parent_id: r.get(2)?,
        title: r.get(3)?,
        notes: r.get(4)?,
        completed_at: r.get(5)?,
        due: r.get(6)?,
        due_time: r.get(7)?,
        priority: r.get(8)?,
        repeat: r.get(9)?,
    })
}

fn read_schedule(meta: &LoroMap) -> Schedule {
    let due = meta_str(meta, DUE).and_then(|d| Date::parse(&d).ok());
    Schedule {
        due,
        time: due.and(meta_i64(meta, DUE_TIME)).map(|t| t as u32),
        repeat: due
            .and(meta_str(meta, REPEAT))
            .and_then(|r| Repeat::parse_rrule(&r).ok()),
    }
}

/// A time or repeat rule only means something with a due date; without one all three go.
fn write_schedule(meta: &LoroMap, schedule: &Schedule) -> Result<()> {
    let Some(due) = schedule.due else {
        for key in [DUE, DUE_TIME, REPEAT] {
            meta.delete(key)?;
        }
        return Ok(());
    };
    meta.insert(DUE, due.to_string())?;
    match schedule.time {
        Some(t) if t < 24 * 60 => meta.insert(DUE_TIME, i64::from(t))?,
        Some(t) => return Err(Error::Invalid(format!("time {t} is past midnight"))),
        None => meta.delete(DUE_TIME)?,
    }
    match &schedule.repeat {
        Some(r) => meta.insert(REPEAT, r.to_rrule())?,
        None => meta.delete(REPEAT)?,
    }
    Ok(())
}

fn write_priority(meta: &LoroMap, priority: u8) -> Result<()> {
    match priority {
        0 => meta.delete(PRIORITY)?,
        1..=3 => meta.insert(PRIORITY, i64::from(priority))?,
        _ => return Err(Error::Invalid(format!("priority {priority} is not 0-3"))),
    }
    Ok(())
}

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

/// [`Store::set_completed`] inside a transaction (for edits that change more than one thing).
pub(crate) fn complete_in(tree: &LoroTree, id: TreeID, completed: bool, today: Date) -> Result<()> {
    let meta = tree.get_meta(id)?;
    if !completed {
        meta.delete(COMPLETED_AT)?;
        return Ok(());
    }
    let schedule = read_schedule(&meta);
    let (Some(due), Some(repeat), None) = (schedule.due, schedule.repeat, meta.get(COMPLETED_AT)) else {
        meta.insert(COMPLETED_AT, now_millis())?;
        return Ok(());
    };
    let parent = tree.parent(id).ok_or_else(|| Error::NotFound(id.to_string()))?;
    let copy = tree.create(parent)?;
    let done = tree.get_meta(copy)?;
    done.insert(KIND, KIND_TASK)?;
    done.insert(TITLE, meta_str(&meta, TITLE).unwrap_or_default())?;
    write_schedule(
        &done,
        &Schedule {
            repeat: None,
            ..schedule
        },
    )?;
    write_priority(&done, meta_i64(&meta, PRIORITY).unwrap_or(0) as u8)?;
    if let Some(notes) = meta_text(&meta, NOTES).filter(|n| !n.is_empty()) {
        done.ensure_mergeable_text(NOTES)?
            .update(&notes, Default::default())
            .map_err(|e| Error::Invalid(format!("notes: {e}")))?;
    }
    done.insert(COMPLETED_AT, now_millis())?;

    let mut next = repeat.next_after(due);
    while next <= today {
        next = repeat.next_after(next);
    }
    meta.insert(DUE, next.to_string())?;
    let mut stack = tree.children(id).unwrap_or_default();
    while let Some(child) = stack.pop() {
        tree.get_meta(child)?.delete(COMPLETED_AT)?;
        stack.extend(tree.children(child).unwrap_or_default());
    }
    Ok(())
}

/// New tasks go first in a list (or the Inbox) and last under a task.
pub(crate) fn create_task_node(tree: &LoroTree, parent: TreeParentId) -> Result<TreeID> {
    let under_task =
        matches!(parent, TreeParentId::Node(p) if projection::kind(tree, p).as_deref() == Some(KIND_TASK));
    Ok(if under_task {
        tree.create(parent)?
    } else {
        tree.create_at(parent, 0)?
    })
}

fn tree_id(id: &str) -> Result<TreeID> {
    TreeID::try_from(id).map_err(|_| Error::Invalid(format!("bad id {id:?}")))
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
