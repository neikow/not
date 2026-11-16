//! Projects the `tasks` tree of the workspace doc into the `task` read-model table.
//!
//! Loro events tell us which nodes changed; only those rows (and, for structural changes, their
//! subtrees) are rewritten. [`Dirty`] also collects what changed in the other containers, which
//! their own modules project.

use std::collections::HashSet;

use loro::event::{ContainerDiff, Diff, DiffEvent};
use loro::{
    Container, ContainerID, Index, LoroMap, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer,
};
use rusqlite::{Connection, params};

use crate::Result;

pub const KIND: &str = "kind";
/// Lists from before ADR 0014; [`crate::views`] turns them into tags and saved views.
pub const KIND_LIST: &str = "list";
pub const KIND_TASK: &str = "task";
pub const TITLE: &str = "title";
pub const COMPLETED_AT: &str = "completed_at";
/// The start date, `YYYY-MM-DD` (named for when it was the only date); see [`crate::schedule`].
pub const DUE: &str = "due";
/// Minutes after midnight; only with [`DUE`].
pub const DUE_TIME: &str = "due_time";
/// `YYYY-MM-DD`.
pub const DEADLINE: &str = "deadline";
/// `true` when parked in Someday; never with [`DUE`].
pub const SOMEDAY: &str = "someday";
/// Mergeable map: tag → `true`.
pub const TAGS: &str = "tags";
/// Unix millis of the last edit made to the task itself (for the weekly review's stale tasks).
pub const TOUCHED_AT: &str = "touched_at";
/// 1 = low … 3 = high; absent = none.
pub const PRIORITY: &str = "priority";
/// RRULE subset; only with [`DUE`] or [`DEADLINE`].
pub const REPEAT: &str = "repeat";
/// `LoroText` with the task's markdown notes.
pub const NOTES: &str = "notes";
/// Soft delete: set on a list or task to hide it and everything under it. Nodes are never removed
/// from the tree, so undo (and a later trash) bring back the same node ids.
pub const DELETED_AT: &str = "deleted_at";

/// Ids of the workspace's root containers, to tell events apart.
pub struct Roots {
    pub tasks: ContainerID,
    pub habits: ContainerID,
    pub habit_log: ContainerID,
    pub notes: ContainerID,
    pub workouts: ContainerID,
}

#[derive(Debug, Default)]
pub struct Dirty {
    pub tasks: TreeDirty,
    /// Any habit definition changed; there are few, so they are re-projected together.
    pub habits: bool,
    /// `habit_log` keys whose counter changed.
    pub habit_log: HashSet<String>,
    pub notes: TreeDirty,
    /// Anything in the workouts tree; it is small and re-projected whole.
    pub workouts: bool,
}

impl Dirty {
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
            && !self.habits
            && self.habit_log.is_empty()
            && self.notes.is_empty()
            && !self.workouts
    }

    pub fn record(&mut self, roots: &Roots, event: &DiffEvent) {
        for change in &event.events {
            let under =
                |root: &ContainerID| change.target == root || change.path.iter().any(|(c, _)| c == root);
            if under(&roots.habits) {
                self.habits = true;
            } else if *change.target == roots.habit_log {
                if let Diff::Map(m) = &change.diff {
                    self.habit_log.extend(m.updated.keys().map(|k| k.to_string()));
                }
            } else if under(&roots.habit_log) {
                self.habit_log
                    .extend(change.path.windows(2).find_map(|pair| match pair {
                        [(parent, _), (_, Index::Key(key))] if *parent == roots.habit_log => {
                            Some(key.to_string())
                        }
                        _ => None,
                    }));
            } else if under(&roots.workouts) {
                self.workouts = true;
            } else if under(&roots.notes) {
                self.notes.record(&roots.notes, change);
            } else {
                self.tasks.record(&roots.tasks, change);
            }
        }
    }
}

/// Nodes of one tree to re-project.
#[derive(Debug, Default)]
pub struct TreeDirty {
    /// Created, moved, deleted or restored: the node and its whole subtree are re-projected.
    structure: HashSet<TreeID>,
    /// Only the node's meta map (or a container in it) changed.
    meta: HashSet<TreeID>,
}

impl TreeDirty {
    pub fn is_empty(&self) -> bool {
        self.structure.is_empty() && self.meta.is_empty()
    }

    pub fn into_parts(self) -> (HashSet<TreeID>, HashSet<TreeID>) {
        (self.structure, self.meta)
    }

    pub(crate) fn record(&mut self, tree: &ContainerID, change: &ContainerDiff<'_>) {
        if let Diff::Tree(diff) = &change.diff {
            if change.target == tree {
                self.structure.extend(diff.diff.iter().map(|item| item.target));
            }
            return;
        }
        // Each path entry is (container, its index within the previous container), so a node's
        // meta map appears as `(meta, Node(id))` right after `(tree, _)`.
        let Some(node) = change.path.windows(2).find_map(|pair| match pair {
            [(parent, _), (_, Index::Node(id))] if parent == tree => Some(*id),
            _ => None,
        }) else {
            return;
        };
        // Deleting or restoring changes what's visible below the node, too.
        let visibility = matches!(&change.diff, Diff::Map(m) if m.updated.contains_key(DELETED_AT));
        if visibility {
            self.structure.insert(node);
        } else {
            self.meta.insert(node);
        }
    }
}

pub fn apply(tree: &LoroTree, db: &Connection, dirty: TreeDirty) -> Result<()> {
    for id in &dirty.structure {
        project_subtree(tree, db, *id)?;
    }
    for id in dirty.meta.difference(&dirty.structure) {
        project_node(tree, db, *id)?;
    }
    Ok(())
}

pub fn rebuild(tree: &LoroTree, db: &Connection) -> Result<()> {
    db.execute_batch("DELETE FROM task;")?;
    for root in tree.roots() {
        project_subtree(tree, db, root)?;
    }
    Ok(())
}

/// In the tree and neither it nor an ancestor is soft-deleted.
pub fn is_alive(tree: &LoroTree, id: TreeID) -> bool {
    let mut current = id;
    loop {
        if !tree.contains(current) || tree.is_node_deleted(&current).unwrap_or(true) {
            return false;
        }
        if tree.get_meta(current).is_ok_and(|m| m.get(DELETED_AT).is_some()) {
            return false;
        }
        match tree.parent(current) {
            Some(TreeParentId::Node(parent)) => current = parent,
            _ => return true,
        }
    }
}

pub fn kind(tree: &LoroTree, id: TreeID) -> Option<String> {
    tree.get_meta(id).ok().and_then(|m| meta_str(&m, KIND))
}

fn project_subtree(tree: &LoroTree, db: &Connection, id: TreeID) -> Result<()> {
    project_node(tree, db, id)?;
    if is_alive(tree, id) {
        for child in tree.children(id).unwrap_or_default() {
            project_subtree(tree, db, child)?;
        }
    }
    Ok(())
}

fn project_node(tree: &LoroTree, db: &Connection, id: TreeID) -> Result<()> {
    if !is_alive(tree, id) {
        return remove(db, id);
    }
    let meta = tree.get_meta(id)?;
    let title = meta_str(&meta, TITLE).unwrap_or_default();
    let position = tree.fractional_index(id).unwrap_or_default();
    if meta_str(&meta, KIND).as_deref() != Some(KIND_TASK) {
        return Ok(());
    }
    let Some(parent_id) = locate_task(tree, id) else {
        // Under something that isn't a list or task (not possible through the public API).
        return remove(db, id);
    };
    db.execute(
        "INSERT INTO task (id, parent_id, title, notes, completed_at, position,
                           start, time, deadline, someday, priority, repeat, tags, touched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT (id) DO UPDATE SET
            parent_id = excluded.parent_id, title = excluded.title, notes = excluded.notes,
            completed_at = excluded.completed_at, position = excluded.position,
            start = excluded.start, time = excluded.time, deadline = excluded.deadline,
            someday = excluded.someday, priority = excluded.priority, repeat = excluded.repeat,
            tags = excluded.tags, touched_at = excluded.touched_at",
        params![
            id.to_string(),
            parent_id.map(|p| p.to_string()),
            title,
            meta_text(&meta, NOTES).unwrap_or_default(),
            meta_i64(&meta, COMPLETED_AT),
            position,
            meta_str(&meta, DUE),
            meta_i64(&meta, DUE_TIME),
            meta_str(&meta, DEADLINE),
            meta_bool(&meta, SOMEDAY),
            meta_i64(&meta, PRIORITY).unwrap_or(0),
            meta_str(&meta, REPEAT),
            tags_of(&meta).join("\n"),
            meta_i64(&meta, TOUCHED_AT),
        ],
    )?;
    Ok(())
}

/// A task's own tags, sorted.
pub fn tags_of(meta: &LoroMap) -> Vec<String> {
    let Some(ValueOrContainer::Container(Container::Map(tags))) = meta.get(TAGS) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    tags.for_each(|key, value| {
        if matches!(value, ValueOrContainer::Value(LoroValue::Bool(true))) {
            out.push(key.to_string());
        }
    });
    out.sort();
    out
}

/// The parent task of a subtask; `None` for a top-level task (at the root, or still in a list
/// from before ADR 0014). `None` overall when it is under something else.
fn locate_task(tree: &LoroTree, id: TreeID) -> Option<Option<TreeID>> {
    match tree.parent(id)? {
        TreeParentId::Root => Some(None),
        TreeParentId::Node(parent) => match kind(tree, parent).as_deref() {
            Some(KIND_TASK) => Some(Some(parent)),
            Some(KIND_LIST) => Some(None),
            _ => None,
        },
        _ => None,
    }
}

fn remove(db: &Connection, id: TreeID) -> Result<()> {
    let id = id.to_string();
    db.execute(
        "WITH RECURSIVE sub (id) AS (
             SELECT ?1 UNION ALL SELECT task.id FROM task JOIN sub ON task.parent_id = sub.id
         )
         DELETE FROM task WHERE id IN sub",
        [&id],
    )?;
    Ok(())
}

pub fn meta_str(meta: &LoroMap, key: &str) -> Option<String> {
    match meta.get(key)? {
        ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

pub fn meta_text(meta: &LoroMap, key: &str) -> Option<String> {
    match meta.get(key)? {
        ValueOrContainer::Container(Container::Text(text)) => Some(text.to_string()),
        _ => None,
    }
}

pub fn meta_bool(meta: &LoroMap, key: &str) -> bool {
    matches!(
        meta.get(key),
        Some(ValueOrContainer::Value(LoroValue::Bool(true)))
    )
}

pub fn meta_i64(meta: &LoroMap, key: &str) -> Option<i64> {
    match meta.get(key)? {
        ValueOrContainer::Value(LoroValue::I64(v)) => Some(v),
        _ => None,
    }
}
