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
    self, COMPLETED_AT, DEADLINE, DELETED_AT, DUE, DUE_TIME, Dirty, KIND, KIND_TASK, NOTES, PRIORITY, REPEAT,
    Roots, SOMEDAY, TAGS, TITLE, TOUCHED_AT, meta_bool, meta_i64, meta_str, meta_text,
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
/// View id → how its open tasks are sorted; absent means the view's default (see
/// [`Store::view_sort`]).
const TASK_SORT: &str = "task_sort";
/// Saved views (smart lists): id → mergeable map; see [`crate::views`].
const VIEWS: &str = "views";
/// View id → mergeable map of task id → number: the order tasks were dragged into in that view.
const VIEW_ORDER: &str = "view_order";
/// `"all"` → how All Notes sorts (see [`crate::NoteSort`]); folders keep theirs on the folder.
const NOTE_SORT: &str = "note_sort";

/// Completed tasks stay in their list this long; after that only the Logbook and views naming
/// `done` show them.
pub const ARCHIVE_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

/// Built-in views (ADR 0014). Inbox: untagged tasks.
pub const INBOX: &str = "inbox";
/// Tasks starting or due on or before today.
pub const TODAY: &str = "today";
/// Tasks starting or due after today.
pub const UPCOMING: &str = "upcoming";
pub const SOMEDAY_VIEW: &str = "someday";

/// How a view orders its open tasks (each level of subtasks too). Ties keep the manual order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskSort {
    /// The order they were dragged into; new tasks go first.
    #[default]
    Manual,
    /// Soonest first by start date, else deadline; undated last; then by time and priority.
    Due,
    /// Highest priority first, then soonest due.
    Priority,
    /// Alphabetical by title.
    Title,
}

impl TaskSort {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Due => "due",
            Self::Priority => "priority",
            Self::Title => "title",
        }
    }

    pub(crate) fn from_key(key: &str) -> Self {
        match key {
            "due" => Self::Due,
            "priority" => Self::Priority,
            "title" => Self::Title,
            _ => Self::Manual,
        }
    }

    pub(crate) fn compare(self, a: &Task, b: &Task) -> std::cmp::Ordering {
        // Undated (or no time) sorts after dated.
        let due = |t: &Task| {
            let date = t.start.clone().or_else(|| t.deadline.clone());
            (date.is_none(), date, t.time.is_none(), t.time)
        };
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
    /// Set for subtasks.
    pub parent_id: Option<String>,
    pub title: String,
    /// Markdown.
    pub notes: String,
    /// Unix millis.
    pub completed_at: Option<i64>,
    /// Start date, `YYYY-MM-DD`: when the user plans to do it. Hidden from views before then.
    pub start: Option<String>,
    /// Minutes after midnight, on the start date.
    pub time: Option<u32>,
    /// `YYYY-MM-DD`.
    pub deadline: Option<String>,
    pub someday: bool,
    /// 0 = none, 1 = low, 2 = medium, 3 = high.
    pub priority: u8,
    /// RRULE subset, see [`Repeat`].
    pub repeat: Option<String>,
    /// Its own tags, sorted (subtasks also count their ancestors' in views).
    pub tags: Vec<String>,
}

/// A task in display order, with its nesting depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    pub task: Task,
    pub depth: u32,
    /// The task or one of its ancestors is completed.
    pub done: bool,
    /// Its direct subtasks, and how many of them are completed.
    pub subtasks: u32,
    pub subtasks_done: u32,
}

/// A view as shown: open tasks (with all their subtasks) in its order, then tasks completed in the
/// last [`ARCHIVE_AFTER_MS`] (any age in a view naming `done`), most recently completed first.
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
    pub(crate) task_sort: LoroMap,
    pub(crate) views: LoroMap,
    pub(crate) view_order: LoroMap,
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

    /// [`Store::open`] for a database encrypted with `key` (SQLCipher, in builds that have it:
    /// the server's). A plain SQLite build would ignore the key, so only the server calls this.
    pub fn open_with_key(path: impl AsRef<Path>, key: &str) -> Result<Self> {
        let db = Connection::open(path)?;
        db.pragma_update(None, "key", key)?;
        // A wrong key (or a plain file) only shows on the first read.
        db.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
        Self::from_connection(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(db: Connection) -> Result<Self> {
        let store = Self::load(db)?;
        store.migrate_lists()?;
        Ok(store)
    }

    fn load(mut db: Connection) -> Result<Self> {
        storage::init(&db)?;

        let doc = LoroDoc::new();
        // Notes saved by a version that linked checklist lines to tasks carry `task` text marks;
        // they mean nothing now, but Loro needs to know how text typed next to one behaves.
        let mut styles = loro::StyleConfigMap::new();
        styles.insert(
            "task".into(),
            loro::StyleConfig {
                expand: loro::ExpandType::None,
            },
        );
        doc.config_text_style(styles);
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
        let views = doc.get_map(VIEWS);
        let view_order = doc.get_map(VIEW_ORDER);
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
        let history = {
            let mut history = UndoManager::new(&doc);
            history.add_exclude_origin_prefix(crate::checklist::MIGRATE_ORIGIN);
            Mutex::new(history)
        };

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
            views,
            view_order,
            note_sort,
            dirty,
            history,
            listeners: Mutex::default(),
            next_listener: AtomicU64::new(0),
            _subscription: subscription,
        })
    }

    // ---- commands -------------------------------------------------------------------------

    /// Adds a task: a subtask (last) when `target` is a task, else a top-level task (first) with
    /// what view `target` gives new tasks (see [`crate::views`]).
    pub fn add_task(&self, target: &str, title: &str, today: Date) -> Result<String> {
        self.add_task_with(target, title, &Schedule::default(), 0, &[], today)
    }

    /// Adds a task with a schedule, priority and tags in one step (one undo step), placed as in
    /// [`Store::add_task`]. What the view gives new tasks fills in what these leave empty; tags add up.
    pub fn add_task_with(
        &self,
        target: &str,
        title: &str,
        schedule: &Schedule,
        priority: u8,
        tags: &[String],
        today: Date,
    ) -> Result<String> {
        let title = clean_title(title)?;
        let (parent, defaults) = self.target(target, today)?;
        let mut schedule = *schedule;
        let mut priority = priority;
        let mut all_tags: Vec<String> = Vec::new();
        if let Some(d) = defaults {
            if schedule.start.is_none() && schedule.deadline.is_none() && !schedule.someday {
                schedule.start = d.start;
                schedule.someday = d.someday;
            }
            if priority == 0 {
                priority = d.priority;
            }
            all_tags = d.tags;
        }
        for tag in tags
            .iter()
            .map(|t| parse::normalize_tag(t))
            .filter(|t| !t.is_empty())
        {
            if !all_tags.contains(&tag) {
                all_tags.push(tag);
            }
        }
        self.write(|tree| {
            let id = create_task_node(tree, parent)?;
            fill_task(&tree.get_meta(id)?, &title, &schedule, priority, &all_tags)?;
            Ok(id.to_string())
        })
    }

    /// Adds a task per non-empty line of `text` (pasted), as [`Store::quick_add_at`] would, in
    /// their order; list markers (`-`, `*`, `•`, `1.`, `- [ ]`) go, `- [x]` lines start completed,
    /// and indented lines become subtasks of the line above. One undo step. Returns the ids of the
    /// top-level ones.
    pub fn quick_add_lines(
        &self,
        target: &str,
        text: &str,
        today: Date,
        minute: Option<u32>,
    ) -> Result<Vec<String>> {
        let lines = paste_lines(text);
        if lines.is_empty() {
            return Err(Error::Invalid("nothing to add".into()));
        }
        let (parent, defaults) = self.target(target, today)?;
        let defaults = defaults.unwrap_or_default();
        self.write(|tree| {
            let mut levels: Vec<(usize, TreeID)> = Vec::new();
            let mut tops = Vec::new();
            let mut last_top: Option<TreeID> = None;
            for line in &lines {
                while levels.last().is_some_and(|&(indent, _)| indent >= line.indent) {
                    levels.pop();
                }
                let parsed = parse::parse_task_at(&line.text, today, minute, &[]);
                let title = if parsed.title.is_empty() {
                    line.text.clone()
                } else {
                    parsed.title
                };
                let mut schedule = parsed.schedule;
                let mut tags = parsed.tags;
                let mut priority = parsed.priority;
                let id = match levels.last() {
                    Some(&(_, above)) => tree.create(TreeParentId::Node(above))?,
                    None => {
                        if schedule.start.is_none() && schedule.deadline.is_none() && !schedule.someday {
                            schedule.start = defaults.start;
                            schedule.someday = defaults.someday;
                        }
                        if priority == 0 {
                            priority = defaults.priority;
                        }
                        for t in &defaults.tags {
                            if !tags.contains(t) {
                                tags.push(t.clone());
                            }
                        }
                        let id = create_task_node(tree, parent)?;
                        if let Some(prev) = last_top {
                            tree.mov_after(id, prev)?;
                        }
                        last_top = Some(id);
                        tops.push(id.to_string());
                        id
                    }
                };
                let meta = tree.get_meta(id)?;
                fill_task(&meta, &clean_title(&title)?, &schedule, priority, &tags)?;
                if line.done {
                    meta.insert(COMPLETED_AT, now_millis())?;
                }
                levels.push((line.indent, id));
            }
            Ok(tops)
        })
    }

    /// Parses `input` (see [`parse::parse_task`]) and adds the task as [`Store::add_task_with`] does.
    /// Returns the new task's id.
    pub fn quick_add(&self, target: &str, input: &str, today: Date, dismissed: &[String]) -> Result<String> {
        self.quick_add_at(target, input, today, None, dismissed)
    }

    /// [`Store::quick_add`] knowing the time of day (minutes after midnight), for "in 2 hours".
    pub fn quick_add_at(
        &self,
        target: &str,
        input: &str,
        today: Date,
        minute: Option<u32>,
        dismissed: &[String],
    ) -> Result<String> {
        let parsed = parse::parse_task_at(input, today, minute, dismissed);
        // A title made only of tokens ("tomorrow") keeps its words rather than being refused.
        let title = if parsed.title.is_empty() {
            input
        } else {
            &parsed.title
        };
        self.add_task_with(
            target,
            title,
            &parsed.schedule,
            parsed.priority,
            &parsed.tags,
            today,
        )
    }

    /// [`parse::parse_task`].
    pub fn parse_task(&self, input: &str, today: Date, dismissed: &[String]) -> ParsedTask {
        parse::parse_task(input, today, dismissed)
    }

    /// Sets when the task starts, its time, deadline, Someday and repeat. A time needs a start
    /// date; a repeat needs a start date or deadline; a start date ends Someday.
    pub fn set_schedule(&self, id: &str, schedule: &Schedule) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| write_schedule(&tree.get_meta(id)?, schedule))
    }

    /// Puts tasks off (one undo step) to a [`Snooze`] point, relative to `today` and the time of day
    /// `minute`. Returns where they went.
    pub fn snooze(&self, ids: &[String], to: Snooze, today: Date, minute: u32) -> Result<Schedule> {
        let (start, time) = to.resolve(today, minute);
        let ids = ids
            .iter()
            .map(|id| self.node(id, &[KIND_TASK]))
            .collect::<Result<Vec<_>>>()?;
        self.write(|tree| {
            let mut last = Schedule::default();
            for &id in &ids {
                let meta = tree.get_meta(id)?;
                let current = read_schedule(&meta);
                last = Schedule {
                    start: Some(start),
                    time: time.or(current.time),
                    someday: false,
                    ..current
                };
                write_schedule(&meta, &last)?;
            }
            Ok(last)
        })
    }

    /// Sets the start date of several tasks at once (one undo step), keeping their time, deadline
    /// and repeat; `None` clears it. For moving what slipped past its day to today.
    pub fn reschedule(&self, ids: &[String], start: Option<Date>) -> Result<()> {
        let ids = ids
            .iter()
            .map(|id| self.node(id, &[KIND_TASK]))
            .collect::<Result<Vec<_>>>()?;
        self.write(|tree| {
            for &id in &ids {
                let meta = tree.get_meta(id)?;
                let schedule = Schedule {
                    start,
                    someday: false,
                    ..read_schedule(&meta)
                };
                write_schedule(&meta, &schedule)?;
            }
            Ok(())
        })
    }

    /// Marks a task as looked at now, so the weekly review stops asking about it for a while.
    pub fn keep_task(&self, id: &str) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| touch(&tree.get_meta(id)?))
    }

    /// 0 = none … 3 = high.
    pub fn set_priority(&self, id: &str, priority: u8) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| write_priority(&tree.get_meta(id)?, priority))
    }

    /// Replaces a task's own tags (normalised; empty ones dropped).
    pub fn set_tags(&self, id: &str, tags: &[String]) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        let tags: Vec<String> = tags
            .iter()
            .map(|t| parse::normalize_tag(t))
            .filter(|t| !t.is_empty())
            .collect();
        self.write(|tree| write_tags(&tree.get_meta(id)?, &tags))
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<()> {
        let title = clean_title(title)?;
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| {
            let meta = tree.get_meta(id)?;
            touch(&meta)?;
            Ok(meta.insert(TITLE, title)?)
        })
    }

    /// Replaces the notes; the text CRDT merges concurrent edits from other devices.
    pub fn set_notes(&self, id: &str, notes: &str) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| {
            // Mergeable: two devices adding the first notes concurrently share one text.
            let meta = tree.get_meta(id)?;
            touch(&meta)?;
            let text = meta.ensure_mergeable_text(NOTES)?;
            text.update(notes, Default::default())
                .map_err(|e| Error::Invalid(format!("notes: {e}")))
        })
    }

    /// Completes or reopens a task. Completing a repeating task logs this occurrence as a completed
    /// copy and moves the task on to its next date after `today`, with its subtasks reopened.
    pub fn set_completed(&self, id: &str, completed: bool, today: Date) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        self.write(|tree| complete_in(tree, id, completed, today))
    }

    /// Moves a task under `parent` (a task; `None` for top level) right before its child `before`,
    /// or to the end when `None`. Anchoring on a sibling keeps drag-and-drop exact when some
    /// siblings (completed ones) are shown elsewhere. Top-level order within a view is
    /// [`Store::move_in_view`]'s.
    pub fn move_task_before(&self, id: &str, parent: Option<&str>, before: Option<&str>) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
        let parent = match parent {
            Some(p) => TreeParentId::Node(self.node(p, &[KIND_TASK])?),
            None => TreeParentId::Root,
        };
        let before = before.map(|b| self.node(b, &[KIND_TASK])).transpose()?;
        if let Some(before) = before
            && self.tasks.parent(before) != Some(parent)
        {
            return Err(Error::Invalid("`before` must be a child of `parent`".into()));
        }
        if let TreeParentId::Node(p) = parent
            && (p == id || self.is_ancestor(id, p))
        {
            return Err(Error::Invalid("a task can't go under itself".into()));
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

    /// Makes a task the last subtask of the open sibling right above it. Returns false when there is
    /// none (it is the first). For top-level tasks in a view, see [`Store::indent_in_view`].
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

    /// Makes a subtask a sibling of its parent task, right after it, keeping the tags it had
    /// through its parent. Returns false for top-level tasks.
    pub fn outdent_task(&self, id: &str) -> Result<bool> {
        self.outdent(id, None)
    }

    /// Deletes a task together with its subtasks. Undoable; see [`DELETED_AT`].
    pub fn delete(&self, id: &str) -> Result<()> {
        let id = self.node(id, &[KIND_TASK])?;
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
            self.migrate_lists()?;
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

    /// Every task (top-level first, each level in tree order).
    pub fn all_tasks(&self) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t ORDER BY parent_id IS NOT NULL, parent_id, position, id"
        ))?;
        let rows = stmt.query_map([], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Open tasks with a start date and a time, soonest first, for reminders. Subtasks of
    /// completed tasks are left out.
    pub fn timed_tasks(&self) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t LEFT JOIN task p ON p.id = t.parent_id
             WHERE t.completed_at IS NULL AND t.start IS NOT NULL AND t.time IS NOT NULL
               AND p.completed_at IS NULL
             ORDER BY t.start, t.time, t.id"
        ))?;
        let rows = stmt.query_map([], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Tasks whose title or notes contain each word of `query` (ignoring case): open ones first,
    /// then completed ones latest first. At most `limit`.
    pub fn search_tasks(&self, query: &str, limit: u32) -> Result<Vec<Task>> {
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t
             ORDER BY t.completed_at IS NOT NULL, t.completed_at DESC, t.start IS NULL, t.start, t.position, t.id"
        ))?;
        // Filtered here rather than with LIKE, which only folds ASCII case ("Été" vs "été").
        let mut found = Vec::new();
        for task in stmt.query_map([], task_row)? {
            let task = task?;
            let haystack = format!("{}\n{}\n{}", task.title, task.notes, task.tags.join(" ")).to_lowercase();
            if words.iter().all(|w| haystack.contains(w.trim_start_matches('#'))) {
                found.push(task);
                if found.len() as u32 >= limit {
                    break;
                }
            }
        }
        Ok(found)
    }

    /// Open top-level tasks without a start date, not in Someday, that nobody has edited for
    /// `days` as of `now` (tasks from before edits were dated count as stale), oldest first: what
    /// the weekly review asks about.
    pub fn stale_tasks(&self, now: i64, days: u32) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t
             WHERE t.parent_id IS NULL AND t.completed_at IS NULL AND t.start IS NULL AND t.someday = 0
               AND coalesce(t.touched_at, 0) <= ?1
             ORDER BY coalesce(t.touched_at, 0), t.position, t.id"
        ))?;
        let rows = stmt.query_map([now - i64::from(days) * 86_400_000], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Top-level tasks completed, latest first, at most `limit`: the logbook. Apps group them by
    /// local day.
    pub fn logbook(&self, limit: u32) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t
             WHERE t.parent_id IS NULL AND t.completed_at IS NOT NULL
             ORDER BY t.completed_at DESC, t.id LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// One task by id.
    pub fn task_by_id(&self, id: &str) -> Result<Task> {
        self.task(id)
    }

    /// A task's direct subtasks, in order.
    pub fn subtasks(&self, id: &str) -> Result<Vec<Task>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(&format!(
            "SELECT {TASK_COLUMNS} FROM task t WHERE t.parent_id = ?1 ORDER BY t.position, t.id"
        ))?;
        let rows = stmt.query_map([id], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub(crate) fn task(&self, id: &str) -> Result<Task> {
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

    /// Whether `ancestor` is above `id` in the tree.
    pub(crate) fn is_ancestor(&self, ancestor: TreeID, id: TreeID) -> bool {
        let mut current = id;
        while let Some(TreeParentId::Node(parent)) = self.tasks.parent(current) {
            if parent == ancestor {
                return true;
            }
            current = parent;
        }
        false
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

    pub(crate) fn write<T>(&self, mutate: impl FnOnce(&LoroTree) -> Result<T>) -> Result<T> {
        self.transact(|| mutate(&self.tasks))
    }

    /// Runs `mutate` as one Loro commit (one undo step), then persists and projects it.
    pub(crate) fn transact<T>(&self, mutate: impl FnOnce() -> Result<T>) -> Result<T> {
        self.transact_with_origin("", mutate)
    }

    /// [`Store::transact`] with a commit origin (undo skips some origins).
    pub(crate) fn transact_with_origin<T>(
        &self,
        origin: &str,
        mutate: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let (out, changed) = {
            let mut inner = self.lock();
            let out = mutate();
            // Commit and flush even on error so the read model never lags behind applied ops.
            if origin.is_empty() {
                self.doc.commit();
            } else {
                self.doc.commit_with(loro::CommitOptions::new().origin(origin));
            }
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

    pub(crate) fn node(&self, id: &str, kinds: &[&str]) -> Result<TreeID> {
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

pub(crate) const TASK_COLUMNS: &str = "t.id, t.parent_id, t.title, t.notes, t.completed_at, t.start, t.time, \
     t.deadline, t.someday, t.priority, t.repeat, t.tags";

pub(crate) fn task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let tags: String = r.get(11)?;
    Ok(Task {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        title: r.get(2)?,
        notes: r.get(3)?,
        completed_at: r.get(4)?,
        start: r.get(5)?,
        time: r.get(6)?,
        deadline: r.get(7)?,
        someday: r.get(8)?,
        priority: r.get(9)?,
        repeat: r.get(10)?,
        tags: tags
            .split('\n')
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect(),
    })
}

pub(crate) fn read_schedule(meta: &LoroMap) -> Schedule {
    let date = |key| meta_str(meta, key).and_then(|d| Date::parse(&d).ok());
    let start = date(DUE);
    let deadline = date(DEADLINE);
    Schedule {
        start,
        time: start.and(meta_i64(meta, DUE_TIME)).map(|t| t as u32),
        deadline,
        someday: start.is_none() && meta_bool(meta, SOMEDAY),
        repeat: start
            .or(deadline)
            .and(meta_str(meta, REPEAT))
            .and_then(|r| Repeat::parse_rrule(&r).ok()),
    }
}

/// A time only means something with a start date, a repeat with a start date or deadline;
/// Someday only without a start date.
pub(crate) fn write_schedule(meta: &LoroMap, schedule: &Schedule) -> Result<()> {
    touch(meta)?;
    let set_date = |key, date: Option<Date>| match date {
        Some(d) => meta.insert(key, d.to_string()),
        None => meta.delete(key),
    };
    set_date(DUE, schedule.start)?;
    set_date(DEADLINE, schedule.deadline)?;
    match schedule.time.filter(|_| schedule.start.is_some()) {
        Some(t) if t < 24 * 60 => meta.insert(DUE_TIME, i64::from(t))?,
        Some(t) => return Err(Error::Invalid(format!("time {t} is past midnight"))),
        None => meta.delete(DUE_TIME)?,
    }
    match schedule.repeat.filter(|_| schedule.anchor().is_some()) {
        Some(r) => meta.insert(REPEAT, r.to_rrule())?,
        None => meta.delete(REPEAT)?,
    }
    if schedule.someday && schedule.start.is_none() {
        meta.insert(SOMEDAY, true)?;
    } else {
        meta.delete(SOMEDAY)?;
    }
    Ok(())
}

/// Replaces the tag set; tags are already normalised.
/// Marks the task as edited now.
fn touch(meta: &LoroMap) -> Result<()> {
    Ok(meta.insert(TOUCHED_AT, now_millis())?)
}

pub(crate) fn write_tags(meta: &LoroMap, tags: &[String]) -> Result<()> {
    touch(meta)?;
    let current = projection::tags_of(meta);
    if current.is_empty() && tags.is_empty() {
        return Ok(());
    }
    // Mergeable: two devices tagging a task for the first time concurrently share one map.
    let map = meta.ensure_mergeable_map(TAGS)?;
    for old in current.iter().filter(|t| !tags.contains(t)) {
        map.delete(old)?;
    }
    for tag in tags {
        map.insert(tag, true)?;
    }
    Ok(())
}

fn write_priority(meta: &LoroMap, priority: u8) -> Result<()> {
    touch(meta)?;
    match priority {
        0 => meta.delete(PRIORITY)?,
        1..=3 => meta.insert(PRIORITY, i64::from(priority))?,
        _ => return Err(Error::Invalid(format!("priority {priority} is not 0-3"))),
    }
    Ok(())
}

pub(crate) fn push_subtree(
    rows: &mut Vec<OutlineRow>,
    children: &mut HashMap<Option<String>, Vec<Task>>,
    task: Task,
    depth: u32,
    ancestor_done: bool,
) {
    let done = ancestor_done || task.completed_at.is_some();
    let kids = children.remove(&Some(task.id.clone())).unwrap_or_default();
    rows.push(OutlineRow {
        task,
        depth,
        done,
        subtasks: kids.len() as u32,
        subtasks_done: kids.iter().filter(|k| k.completed_at.is_some()).count() as u32,
    });
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
    let (Some(anchor), Some(repeat), None) = (schedule.anchor(), schedule.repeat, meta.get(COMPLETED_AT))
    else {
        meta.insert(COMPLETED_AT, now_millis())?;
        return Ok(());
    };
    let parent = tree.parent(id).ok_or_else(|| Error::NotFound(id.to_string()))?;
    let copy = tree.create(parent)?;
    let done = tree.get_meta(copy)?;
    copy_task(&meta, &done, Some(schedule))?;
    done.insert(OCCURRENCE_OF, id.to_string())?;
    done.insert(COMPLETED_AT, now_millis())?;

    // Start date and deadline move on together, keeping the days between them; dated subtasks
    // move with them.
    let next = repeat.next_for_completion(anchor, today);
    let shift = next.days_since(anchor);
    write_schedule(&meta, &shifted(schedule, shift))?;
    start_over(tree, id, copy, shift)?;
    Ok(())
}

/// Meta a task's logged occurrence carries: the id of the repeating task it was done from.
/// Under a repeating parent, such a copy belongs to the parent's occurrence, not to its next one.
const OCCURRENCE_OF: &str = "occurrence_of";

/// A schedule `days` later, start and deadline together.
fn shifted(schedule: Schedule, days: i64) -> Schedule {
    Schedule {
        start: schedule.start.map(|d| d.add_days(days)),
        deadline: schedule.deadline.map(|d| d.add_days(days)),
        ..schedule
    }
}

/// Copies what a logged occurrence keeps of a task: title, priority, tags, notes, and with
/// `schedule` its dates (without the repeat).
fn copy_task(from: &LoroMap, to: &LoroMap, schedule: Option<Schedule>) -> Result<()> {
    to.insert(KIND, KIND_TASK)?;
    to.insert(TITLE, meta_str(from, TITLE).unwrap_or_default())?;
    if let Some(schedule) = schedule {
        write_schedule(
            to,
            &Schedule {
                repeat: None,
                ..schedule
            },
        )?;
    }
    write_priority(to, meta_i64(from, PRIORITY).unwrap_or(0) as u8)?;
    write_tags(to, &projection::tags_of(from))?;
    if let Some(notes) = meta_text(from, NOTES).filter(|n| !n.is_empty()) {
        to.ensure_mergeable_text(NOTES)?
            .update(&notes, Default::default())
            .map_err(|e| Error::Invalid(format!("notes: {e}")))?;
    }
    Ok(())
}

/// The subtasks of a repeating task that just moved on: each is copied under the logged
/// occurrence `log` as it was (done or not), then reopened, its dates `shift` days later.
/// Occurrences logged by repeating subtasks move to `log` instead: they belong to this round.
/// Deleted subtasks stay as they are.
fn start_over(tree: &LoroTree, from: TreeID, log: TreeID, shift: i64) -> Result<()> {
    for child in tree.children(from).unwrap_or_default() {
        let meta = tree.get_meta(child)?;
        if meta.get(DELETED_AT).is_some() || meta_str(&meta, KIND).as_deref() != Some(KIND_TASK) {
            continue;
        }
        if meta.get(OCCURRENCE_OF).is_some() {
            tree.mov(child, TreeParentId::Node(log))?;
            continue;
        }
        let copy = tree.create(TreeParentId::Node(log))?;
        let logged = tree.get_meta(copy)?;
        let schedule = read_schedule(&meta);
        copy_task(&meta, &logged, Some(schedule))?;
        if let Some(at) = meta_i64(&meta, COMPLETED_AT) {
            logged.insert(COMPLETED_AT, at)?;
            meta.delete(COMPLETED_AT)?;
        }
        // A repeating subtask keeps its own rhythm.
        if schedule.repeat.is_none() && (schedule.start.is_some() || schedule.deadline.is_some()) {
            write_schedule(&meta, &shifted(schedule, shift))?;
        }
        start_over(tree, child, copy, shift)?;
    }
    Ok(())
}

/// New tasks go first at the top level and last under a task.
pub(crate) fn create_task_node(tree: &LoroTree, parent: TreeParentId) -> Result<TreeID> {
    let under_task =
        matches!(parent, TreeParentId::Node(p) if projection::kind(tree, p).as_deref() == Some(KIND_TASK));
    Ok(if under_task {
        tree.create(parent)?
    } else {
        tree.create_at(parent, 0)?
    })
}

pub(crate) fn tree_id(id: &str) -> Result<TreeID> {
    TreeID::try_from(id).map_err(|_| Error::Invalid(format!("bad id {id:?}")))
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

/// Where [`Store::snooze`] puts tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snooze {
    /// Three hours from now (rounded to the half hour), or tomorrow at 9:00 late in the evening.
    LaterToday,
    /// Today at 19:00, or tomorrow evening once past it.
    ThisEvening,
    /// Tomorrow, keeping the time.
    Tomorrow,
    /// The coming Saturday (the next one on a weekend).
    Weekend,
    /// Next Monday.
    NextWeek,
}

impl Snooze {
    fn resolve(self, today: Date, minute: u32) -> (Date, Option<u32>) {
        const EVENING: u32 = 19 * 60;
        match self {
            Self::LaterToday => {
                let later = (minute + 180).div_ceil(30) * 30;
                if later < 22 * 60 {
                    (today, Some(later))
                } else {
                    (today.add_days(1), Some(9 * 60))
                }
            }
            Self::ThisEvening if minute < EVENING => (today, Some(EVENING)),
            Self::ThisEvening => (today.add_days(1), Some(EVENING)),
            Self::Tomorrow => (today.add_days(1), None),
            Self::Weekend => {
                let days = (5 + 7 - i64::from(today.weekday())) % 7;
                (today.add_days(if days == 0 { 7 } else { days }), None)
            }
            Self::NextWeek => (today.monday().add_days(7), None),
        }
    }
}

/// Title, schedule, priority and tags of a new task, in its meta map.
pub(crate) fn fill_task(
    meta: &LoroMap,
    title: &str,
    schedule: &Schedule,
    priority: u8,
    tags: &[String],
) -> Result<()> {
    meta.insert(KIND, KIND_TASK)?;
    meta.insert(TITLE, title)?;
    write_schedule(meta, schedule)?;
    write_priority(meta, priority)?;
    write_tags(meta, tags)
}

struct PastedLine {
    indent: usize,
    text: String,
    done: bool,
}

/// The lines of pasted text worth a task each, with list markers taken off.
fn paste_lines(text: &str) -> Vec<PastedLine> {
    text.lines()
        .filter_map(|raw| {
            let trimmed = raw.trim_start();
            // Tabs count as four spaces, so tab- and space-indented pastes nest the same.
            let indent: usize = raw[..raw.len() - trimmed.len()]
                .chars()
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum();
            let mut rest = trimmed.trim_end();
            let mut done = false;
            for marker in ["- ", "* ", "+ ", "• ", "– "] {
                if let Some(r) = rest.strip_prefix(marker) {
                    rest = r.trim_start();
                    break;
                }
            }
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits > 0
                && let Some(r) = rest[digits..]
                    .strip_prefix(". ")
                    .or_else(|| rest[digits..].strip_prefix(") "))
            {
                rest = r.trim_start();
            }
            for (box_, ticked) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
                if let Some(r) = rest.strip_prefix(box_) {
                    rest = r.trim_start();
                    done = ticked;
                }
            }
            let bare_marker = matches!(rest, "-" | "*" | "+" | "•" | "–" | "[ ]" | "[x]" | "[X]");
            (!rest.is_empty() && !bare_marker).then(|| PastedLine {
                indent,
                text: rest.to_owned(),
                done,
            })
        })
        .collect()
}
