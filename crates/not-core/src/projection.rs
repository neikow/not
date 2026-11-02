//! Projects the `tasks` tree of the workspace doc into the `list` / `task` read-model tables.
//!
//! Loro events tell us which nodes changed; only those rows (and, for structural changes, their
//! subtrees) are rewritten.

use std::collections::HashSet;

use loro::event::{Diff, DiffEvent};
use loro::{ContainerID, Index, LoroMap, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer};
use rusqlite::{Connection, params};

use crate::Result;

pub const KIND: &str = "kind";
pub const KIND_LIST: &str = "list";
pub const KIND_TASK: &str = "task";
pub const TITLE: &str = "title";
pub const COMPLETED_AT: &str = "completed_at";

#[derive(Debug, Default)]
pub struct Dirty {
    /// Created, moved or deleted: the node and its whole subtree are re-projected.
    structure: HashSet<TreeID>,
    /// Only the node's meta map changed.
    meta: HashSet<TreeID>,
}

impl Dirty {
    pub fn is_empty(&self) -> bool {
        self.structure.is_empty() && self.meta.is_empty()
    }

    pub fn record(&mut self, tree: &ContainerID, event: &DiffEvent) {
        for change in &event.events {
            if let Diff::Tree(diff) = &change.diff {
                if change.target == tree {
                    self.structure.extend(diff.diff.iter().map(|item| item.target));
                }
                continue;
            }
            // Each path entry is (container, its index within the previous container), so a node's
            // meta map appears as `(meta, Node(id))` right after `(tree, _)`.
            let node = change.path.windows(2).find_map(|pair| match pair {
                [(parent, _), (_, Index::Node(id))] if parent == tree => Some(*id),
                _ => None,
            });
            self.meta.extend(node);
        }
    }
}

pub fn apply(tree: &LoroTree, db: &Connection, dirty: Dirty) -> Result<()> {
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

pub fn is_alive(tree: &LoroTree, id: TreeID) -> bool {
    tree.contains(id) && !tree.is_node_deleted(&id).unwrap_or(true)
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
                // Not under any list (shouldn't happen through the public API): keep it out of the UI.
                return remove(db, id);
            };
            db.execute(
                "INSERT INTO task (id, list_id, parent_id, title, completed_at, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (id) DO UPDATE SET
                    list_id = excluded.list_id, parent_id = excluded.parent_id, title = excluded.title,
                    completed_at = excluded.completed_at, position = excluded.position",
                params![
                    id.to_string(),
                    list_id.to_string(),
                    parent_id.map(|p| p.to_string()),
                    title,
                    meta_i64(&meta, COMPLETED_AT),
                    position
                ],
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Returns the owning list and, for subtasks, the parent task.
fn locate_task(tree: &LoroTree, id: TreeID) -> Option<(TreeID, Option<TreeID>)> {
    let TreeParentId::Node(parent) = tree.parent(id)? else {
        return None;
    };
    let mut current = parent;
    loop {
        match kind(tree, current).as_deref() {
            Some(KIND_LIST) => {
                let parent_task = (current != parent).then_some(parent);
                return Some((current, parent_task));
            }
            Some(KIND_TASK) => match tree.parent(current)? {
                TreeParentId::Node(next) => current = next,
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

pub fn meta_i64(meta: &LoroMap, key: &str) -> Option<i64> {
    match meta.get(key)? {
        ValueOrContainer::Value(LoroValue::I64(v)) => Some(v),
        _ => None,
    }
}
