//! Trash: what was deleted (tasks, notes, folders, habits, workouts, exercises), to restore or
//! delete for good. Deletes are soft (`deleted_at`, ADR 0007), so the trash is read from the
//! trees themselves; deleting for good removes the node (and a habit's check-ins). Anything in
//! the trash longer than [TRASH_DAYS] days goes for good when the apps call
//! [`Store::purge_trash`]. Removed nodes stay in the Loro history until it's compacted.

use loro::{LoroTree, TreeID, TreeParentId};

use crate::notes::{BODY, KIND_NOTE};
use crate::projection::{DELETED_AT, KIND, KIND_TASK, TITLE, meta_i64, meta_str, meta_text};
use crate::store::Store;
use crate::{Error, Result};

/// How long deleted items wait in the trash.
pub const TRASH_DAYS: i64 = 30;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrashKind {
    Task,
    Note,
    Folder,
    Habit,
    Workout,
    Exercise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    pub id: String,
    pub kind: TrashKind,
    pub title: String,
    /// Unix millis.
    pub deleted_at: i64,
    /// Items deleted along with it: subtasks, the notes of a folder, a workout's exercises.
    pub inside: u32,
}

/// Which tree an item lives in.
#[derive(Clone, Copy)]
enum Tree {
    Tasks,
    Notes,
    Habits,
    Workouts,
}

impl Store {
    /// Deleted items, newest first. Only what was deleted itself: a folder lists once, not its notes.
    pub fn trash(&self) -> Result<Vec<TrashItem>> {
        let mut out = Vec::new();
        for (tree, which) in self.trees() {
            for id in tree.get_nodes(false).into_iter().map(|n| n.id) {
                let Some(deleted_at) = deleted_at(tree, id) else {
                    continue;
                };
                // Listed under the deleted ancestor instead.
                if ancestors(tree, id).any(|a| deleted_at_of(tree, a).is_some()) {
                    continue;
                }
                let Some(kind) = item_kind(tree, which, id) else {
                    continue;
                };
                out.push(TrashItem {
                    id: id.to_string(),
                    kind,
                    title: title(tree, kind, id),
                    deleted_at,
                    inside: descendants(tree, id),
                });
            }
        }
        out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then_with(|| a.id.cmp(&b.id)));
        Ok(out)
    }

    /// Puts a deleted item back where it was, with everything deleted along with it. One undo step.
    pub fn restore_from_trash(&self, id: &str) -> Result<()> {
        let (tree, node) = self.trashed(id)?;
        self.transact(|| Ok(tree.get_meta(node)?.delete(DELETED_AT)?))
    }

    /// Deletes an item in the trash for good (undoable until the app closes).
    pub fn delete_forever(&self, id: &str) -> Result<()> {
        let (tree, node) = self.trashed(id)?;
        let habit = std::ptr::eq(tree, &self.habits);
        self.transact(|| {
            if habit {
                self.forget_check_ins(node)?;
            }
            Ok(tree.delete(node)?)
        })
    }

    /// Deletes everything in the trash for good, in one undo step. Returns how many items.
    pub fn empty_trash(&self) -> Result<u32> {
        self.purge(|_| true)
    }

    /// Deletes for good what has been in the trash longer than [TRASH_DAYS] days as of `now`
    /// (Unix millis). The apps call this now and then; every device doing it is harmless.
    pub fn purge_trash(&self, now: i64) -> Result<u32> {
        self.purge(|item| now - item.deleted_at > TRASH_DAYS * DAY_MS)
    }

    fn purge(&self, which: impl Fn(&TrashItem) -> bool) -> Result<u32> {
        let doomed: Vec<TrashItem> = self.trash()?.into_iter().filter(|i| which(i)).collect();
        if doomed.is_empty() {
            return Ok(0);
        }
        self.transact(|| {
            for item in &doomed {
                let (tree, node) = self.trashed(&item.id)?;
                if item.kind == TrashKind::Habit {
                    self.forget_check_ins(node)?;
                }
                tree.delete(node)?;
            }
            Ok(doomed.len() as u32)
        })
    }

    fn trees(&self) -> [(&LoroTree, Tree); 4] {
        [
            (&self.tasks, Tree::Tasks),
            (&self.notes, Tree::Notes),
            (&self.habits, Tree::Habits),
            (&self.workouts, Tree::Workouts),
        ]
    }

    /// The tree and node of an item that is in the trash.
    fn trashed(&self, id: &str) -> Result<(&LoroTree, TreeID)> {
        let node = TreeID::try_from(id).map_err(|_| Error::Invalid(format!("bad id {id:?}")))?;
        for (tree, _) in self.trees() {
            if tree.contains(node) && !tree.is_node_deleted(&node).unwrap_or(true) {
                if deleted_at(tree, node).is_none() {
                    return Err(Error::Invalid("not in the trash".into()));
                }
                return Ok((tree, node));
            }
        }
        Err(Error::NotFound(id.to_owned()))
    }

    fn forget_check_ins(&self, habit: TreeID) -> Result<()> {
        let prefix = format!("{habit}:");
        for key in self
            .habit_log
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(|k| k.to_string())
            .collect::<Vec<_>>()
        {
            self.habit_log.delete(&key)?;
        }
        Ok(())
    }
}

fn deleted_at_of(tree: &LoroTree, id: TreeID) -> Option<i64> {
    tree.get_meta(id).ok().and_then(|m| meta_i64(&m, DELETED_AT))
}

/// When a live (not removed) node was soft-deleted.
fn deleted_at(tree: &LoroTree, id: TreeID) -> Option<i64> {
    if !tree.contains(id) || tree.is_node_deleted(&id).unwrap_or(true) {
        return None;
    }
    deleted_at_of(tree, id)
}

fn ancestors(tree: &LoroTree, id: TreeID) -> impl Iterator<Item = TreeID> + '_ {
    std::iter::successors(Some(id), move |&n| match tree.parent(n) {
        Some(TreeParentId::Node(p)) => Some(p),
        _ => None,
    })
    .skip(1)
}

fn descendants(tree: &LoroTree, id: TreeID) -> u32 {
    tree.children(id)
        .unwrap_or_default()
        .into_iter()
        .map(|c| 1 + descendants(tree, c))
        .sum()
}

fn item_kind(tree: &LoroTree, which: Tree, id: TreeID) -> Option<TrashKind> {
    let kind = tree.get_meta(id).ok().and_then(|m| meta_str(&m, KIND));
    Some(match which {
        Tree::Tasks => {
            // Old list nodes (before ADR 0014) aren't anything to restore.
            if kind.as_deref().is_some_and(|k| k != KIND_TASK) {
                return None;
            }
            TrashKind::Task
        }
        Tree::Notes if kind.as_deref() == Some(KIND_NOTE) => TrashKind::Note,
        Tree::Notes => TrashKind::Folder,
        Tree::Habits => TrashKind::Habit,
        Tree::Workouts if matches!(tree.parent(id), Some(TreeParentId::Node(_))) => TrashKind::Exercise,
        Tree::Workouts => TrashKind::Workout,
    })
}

fn title(tree: &LoroTree, kind: TrashKind, id: TreeID) -> String {
    let Ok(meta) = tree.get_meta(id) else {
        return String::new();
    };
    match kind {
        TrashKind::Note => crate::notes::title_and_preview(&meta_text(&meta, BODY).unwrap_or_default()).0,
        TrashKind::Workout => {
            let date = meta_str(&meta, "date").unwrap_or_default();
            match meta_str(&meta, TITLE).filter(|t| !t.is_empty()) {
                Some(t) => format!("{t} · {date}"),
                None => format!("Workout · {date}"),
            }
        }
        TrashKind::Exercise => meta_str(&meta, "name").unwrap_or_default(),
        _ => meta_str(&meta, TITLE).unwrap_or_default(),
    }
}
