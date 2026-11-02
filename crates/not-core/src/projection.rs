//! Projects the `tasks` tree of the workspace doc into the `list` / `task` read-model tables.
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
use crate::store::INBOX;

pub const KIND: &str = "kind";
pub const KIND_LIST: &str = "list";
pub const KIND_TASK: &str = "task";
pub const TITLE: &str = "title";
pub const COMPLETED_AT: &str = "completed_at";
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
}

#[derive(Debug, Default)]
pub struct Dirty {
    pub tasks: TreeDirty,
    /// Any habit definition changed; there are few, so they are re-projected together.
    pub habits: bool,
    /// `habit_log` keys whose counter changed.
    pub habit_log: HashSet<String>,
}

impl Dirty {
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty() && !self.habits && self.habit_log.is_empty()
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

    fn record(&mut self, tree: &ContainerID, change: &ContainerDiff<'_>) {
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
    db.execute_batch("DELETE FROM list; DELETE FROM task;")?;
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
    match meta_str(&meta, KIND).as_deref() {
        Some(KIND_LIST) => {
            db.execute(
                "INSERT INTO list (id, title, position) VALUES (?1, ?2, ?3)
                 ON CONFLICT (id) DO UPDATE SET title = excluded.title, position = excluded.position",
                params![id.to_string(), title, position],
            )?;
        }
        Some(KIND_TASK) => {
            let Some((list_id, parent_id)) = locate_task(tree, id) else {
                // Under something that isn't a list or task (not possible through the public API).
                return remove(db, id);
            };
            db.execute(
                "INSERT INTO task (id, list_id, parent_id, title, notes, completed_at, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (id) DO UPDATE SET
                    list_id = excluded.list_id, parent_id = excluded.parent_id, title = excluded.title,
                    notes = excluded.notes, completed_at = excluded.completed_at,
                    position = excluded.position",
                params![
                    id.to_string(),
                    list_id,
                    parent_id.map(|p| p.to_string()),
                    title,
                    meta_text(&meta, NOTES).unwrap_or_default(),
                    meta_i64(&meta, COMPLETED_AT),
                    position
                ],
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Returns the owning list id ([`INBOX`] for tasks outside any list) and, for subtasks, the parent
/// task.
fn locate_task(tree: &LoroTree, id: TreeID) -> Option<(String, Option<TreeID>)> {
    let parent = match tree.parent(id)? {
        TreeParentId::Root => return Some((INBOX.into(), None)),
        TreeParentId::Node(parent) => parent,
        _ => return None,
    };
    let mut current = parent;
    loop {
        match kind(tree, current).as_deref() {
            Some(KIND_LIST) => {
                let parent_task = (current != parent).then_some(parent);
                return Some((current.to_string(), parent_task));
            }
            Some(KIND_TASK) => match tree.parent(current)? {
                TreeParentId::Node(next) => current = next,
                TreeParentId::Root => return Some((INBOX.into(), Some(parent))),
                _ => return None,
            },
            _ => return None,
        }
    }
}

fn remove(db: &Connection, id: TreeID) -> Result<()> {
    let id = id.to_string();
    db.execute("DELETE FROM list WHERE id = ?1", [&id])?;
    db.execute("DELETE FROM task WHERE list_id = ?1", [&id])?;
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

pub fn meta_i64(meta: &LoroMap, key: &str) -> Option<i64> {
    match meta.get(key)? {
        ValueOrContainer::Value(LoroValue::I64(v)) => Some(v),
        _ => None,
    }
}
