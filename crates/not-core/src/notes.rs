//! Notes: the `notes` tree holds folders and notes. A note's markdown body is a mergeable `LoroText` in
//! its meta map, so concurrent edits merge character by character. Titles and previews are derived
//! from the body (first line, then the next), like Apple Notes.
//!
//! Bodies live in the workspace doc for now rather than in per-note docs: one doc keeps sync and undo
//! simple, and a single user's notes are small. See `docs/adr/0008-notes-in-the-workspace-doc.md`.

use loro::{LoroTree, TreeID, TreeParentId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::projection::{DELETED_AT, KIND, TITLE, TreeDirty, is_alive, kind, meta_i64, meta_str, meta_text};
use crate::store::{Store, clean_title, now_millis};
use crate::{Error, Result};

const KIND_FOLDER: &str = "folder";
const KIND_NOTE: &str = "note";
const BODY: &str = "body";
const PINNED: &str = "pinned";
const CREATED_AT: &str = "created_at";
const UPDATED_AT: &str = "updated_at";
/// On a folder: `"manual"` keeps its notes in the order they were dragged into; absent sorts them by
/// latest edit.
const ORDER: &str = "order";
const ORDER_MANUAL: &str = "manual";
const PREVIEW_CHARS: usize = 140;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    /// Depth below the top level, for indenting a flat list.
    pub depth: u32,
    /// Notes directly in the folder.
    pub note_count: u32,
    /// Notes keep the order they were dragged into, instead of latest edit first.
    pub manual_order: bool,
}

/// A note without its body, for lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: String,
    /// `None` for notes outside any folder.
    pub folder_id: Option<String>,
    pub title: String,
    pub preview: String,
    pub pinned: bool,
    /// Unix millis.
    pub updated_at: i64,
}

impl Store {
    /// A folder at the top level, or inside `parent`.
    pub fn create_folder(&self, parent: Option<&str>, title: &str) -> Result<String> {
        let title = clean_title(title)?;
        let parent = self.notes_parent(parent)?;
        self.transact(|| {
            let id = self.notes.create(parent)?;
            let meta = self.notes.get_meta(id)?;
            meta.insert(KIND, KIND_FOLDER)?;
            meta.insert(TITLE, title)?;
            Ok(id.to_string())
        })
    }

    pub fn rename_folder(&self, id: &str, title: &str) -> Result<()> {
        let title = clean_title(title)?;
        let id = self.notes_node(id, KIND_FOLDER)?;
        self.transact(|| Ok(self.notes.get_meta(id)?.insert(TITLE, title)?))
    }

    /// Moves a folder (with everything in it) into `parent`, or to the top level.
    pub fn move_folder(&self, id: &str, parent: Option<&str>) -> Result<()> {
        let node = self.notes_node(id, KIND_FOLDER)?;
        let parent = self.notes_parent(parent)?;
        self.transact(|| match self.notes.mov(node, parent) {
            Ok(()) => Ok(()),
            // Into itself or one of its subfolders.
            Err(loro::LoroError::TreeError(_)) => {
                Err(Error::Invalid("a folder can't go inside itself".into()))
            }
            Err(e) => Err(e.into()),
        })
    }

    /// A note in `folder` (or outside any folder) starting with `body`.
    pub fn create_note(&self, folder: Option<&str>, body: &str) -> Result<String> {
        let parent = self.notes_parent(folder)?;
        self.transact(|| {
            let id = self.notes.create(parent)?;
            let meta = self.notes.get_meta(id)?;
            meta.insert(KIND, KIND_NOTE)?;
            let now = now_millis();
            meta.insert(CREATED_AT, now)?;
            meta.insert(UPDATED_AT, now)?;
            if !body.is_empty() {
                meta.ensure_mergeable_text(BODY)?.insert(0, body)?;
            }
            // New notes go on top of a hand-sorted folder, as they do in one sorted by edits.
            if let TreeParentId::Node(folder) = parent
                && self.is_manual(folder)
                && let Some(&first) = self.note_children(parent).iter().find(|&&n| n != id)
            {
                self.notes.mov_before(id, first)?;
            }
            Ok(id.to_string())
        })
    }

    /// Replaces the body; the text CRDT merges it with concurrent edits from other devices.
    pub fn set_note_body(&self, id: &str, body: &str) -> Result<()> {
        let id = self.notes_node(id, KIND_NOTE)?;
        self.transact(|| {
            let meta = self.notes.get_meta(id)?;
            let text = meta.ensure_mergeable_text(BODY)?;
            if text.to_string() == body {
                return Ok(());
            }
            text.update(body, Default::default())
                .map_err(|e| Error::Invalid(format!("note body: {e}")))?;
            meta.insert(UPDATED_AT, now_millis())?;
            Ok(())
        })
    }

    pub fn set_note_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        let id = self.notes_node(id, KIND_NOTE)?;
        self.transact(|| {
            let meta = self.notes.get_meta(id)?;
            if pinned {
                meta.insert(PINNED, true)?;
            } else {
                meta.delete(PINNED)?;
            }
            Ok(())
        })
    }

    pub fn move_note(&self, id: &str, folder: Option<&str>) -> Result<()> {
        let id = self.notes_node(id, KIND_NOTE)?;
        let parent = self.notes_parent(folder)?;
        self.transact(|| Ok(self.notes.mov(id, parent)?))
    }

    /// Places a note right before `before` (another note in the same folder), or last when `None`.
    /// A folder sorted by latest edit switches to manual order first, keeping what was on screen.
    pub fn move_note_before(&self, id: &str, before: Option<&str>) -> Result<()> {
        let id = self.notes_node(id, KIND_NOTE)?;
        let parent = self.notes.parent(id).ok_or_else(|| Error::NotFound(id.to_string()))?;
        let TreeParentId::Node(folder) = parent else {
            return Err(Error::Invalid("only notes in a folder can be reordered".into()));
        };
        let before = before.map(|b| self.notes_node(b, KIND_NOTE)).transpose()?;
        if before.is_some_and(|b| self.notes.parent(b) != Some(parent)) {
            return Err(Error::Invalid("`before` must be in the same folder".into()));
        }
        self.transact(|| {
            if !self.is_manual(folder) {
                self.freeze_order(folder)?;
            }
            match before {
                Some(before) if before != id => self.notes.mov_before(id, before)?,
                Some(_) => {}
                None => {
                    if let Some(&last) = self.note_children(parent).iter().rev().find(|&&n| n != id) {
                        self.notes.mov_after(id, last)?;
                    }
                }
            }
            Ok(())
        })
    }

    /// Sorts a folder's notes by hand (`true`, starting from the order shown) or by latest edit.
    pub fn set_folder_manual_order(&self, id: &str, manual: bool) -> Result<()> {
        let folder = self.notes_node(id, KIND_FOLDER)?;
        if self.is_manual(folder) == manual {
            return Ok(());
        }
        self.transact(|| {
            if manual {
                self.freeze_order(folder)
            } else {
                Ok(self.notes.get_meta(folder)?.delete(ORDER)?)
            }
        })
    }

    /// Lays the folder's notes out in the tree in the order they are shown now (latest edit first),
    /// and marks it manual. Runs inside a transaction.
    fn freeze_order(&self, folder: TreeID) -> Result<()> {
        // Same order as the read model's (pinned, then latest edit), from the doc: the database is
        // locked while a transaction runs.
        let mut shown = self.note_children(TreeParentId::Node(folder));
        let sort_key = |&n: &TreeID| {
            let meta = self.notes.get_meta(n).ok();
            let pinned = meta.as_ref().is_some_and(|m| {
                matches!(m.get(PINNED), Some(loro::ValueOrContainer::Value(loro::LoroValue::Bool(true))))
            });
            let edited = meta
                .as_ref()
                .and_then(|m| meta_i64(m, UPDATED_AT).or(meta_i64(m, CREATED_AT)))
                .unwrap_or(0);
            (std::cmp::Reverse(pinned), std::cmp::Reverse(edited), n.to_string())
        };
        shown.sort_by_cached_key(sort_key);
        let mut previous: Option<TreeID> = None;
        for id in shown {
            match previous {
                Some(p) => self.notes.mov_after(id, p)?,
                None => match self.note_children(TreeParentId::Node(folder)).first() {
                    Some(&first) if first != id => self.notes.mov_before(id, first)?,
                    _ => {}
                },
            }
            previous = Some(id);
        }
        self.notes.get_meta(folder)?.insert(ORDER, ORDER_MANUAL)?;
        Ok(())
    }

    fn is_manual(&self, folder: TreeID) -> bool {
        self.notes
            .get_meta(folder)
            .is_ok_and(|m| meta_str(&m, ORDER).as_deref() == Some(ORDER_MANUAL))
    }

    /// Live notes (not folders) directly under `parent`, in tree order.
    fn note_children(&self, parent: TreeParentId) -> Vec<TreeID> {
        self.notes
            .children(parent)
            .unwrap_or_default()
            .into_iter()
            .filter(|&n| is_alive(&self.notes, n) && kind(&self.notes, n).as_deref() == Some(KIND_NOTE))
            .collect()
    }

    /// Deletes a note, or a folder with everything in it. Undoable.
    pub fn delete_note_or_folder(&self, id: &str) -> Result<()> {
        let node = self.note_id(id)?;
        if !is_alive(&self.notes, node) {
            return Err(Error::NotFound(id.to_owned()));
        }
        self.transact(|| Ok(self.notes.get_meta(node)?.insert(DELETED_AT, now_millis())?))
    }

    /// Folders depth-first, each level by title.
    pub fn folders(&self) -> Result<Vec<Folder>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(
            "SELECT id, parent_id, title, (SELECT count(*) FROM note WHERE folder_id = folder.id), manual
             FROM folder ORDER BY title COLLATE NOCASE, id",
        )?;
        let all = stmt
            .query_map([], |r| {
                Ok(Folder {
                    id: r.get(0)?,
                    parent_id: r.get(1)?,
                    title: r.get(2)?,
                    depth: 0,
                    note_count: r.get(3)?,
                    manual_order: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::with_capacity(all.len());
        push_folders(&all, None, 0, &mut out);
        Ok(out)
    }

    /// Notes in `folder` (not its subfolders), or all notes when `None`. Pinned first, then by hand
    /// in manual folders, else most recently edited.
    pub fn notes(&self, folder: Option<&str>) -> Result<Vec<NoteSummary>> {
        let inner = self.lock();
        let mut stmt = inner.db.prepare_cached(
            "SELECT id, folder_id, title, preview, pinned, updated_at FROM note
             WHERE ?1 IS NULL OR folder_id = ?1
             ORDER BY pinned DESC,
                CASE WHEN (SELECT manual FROM folder WHERE id = ?1) THEN position END,
                updated_at DESC, id",
        )?;
        let rows = stmt.query_map([folder], note_summary)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Notes whose text contains every word of `query`, ignoring case.
    pub fn search_notes(&self, query: &str) -> Result<Vec<NoteSummary>> {
        let words: Vec<String> = query
            .split_whitespace()
            .map(|w| format!("%{}%", w.replace(['%', '_', '\\'], "")))
            .collect();
        if words.is_empty() {
            return self.notes(None);
        }
        let filter = vec!["body LIKE ?"; words.len()].join(" AND ");
        let inner = self.lock();
        let mut stmt = inner.db.prepare(&format!(
            "SELECT id, folder_id, title, preview, pinned, updated_at FROM note
             WHERE {filter} ORDER BY pinned DESC, updated_at DESC, id"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(&words), note_summary)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn note_body(&self, id: &str) -> Result<String> {
        let inner = self.lock();
        inner
            .db
            .query_row("SELECT body FROM note WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::NotFound(id.to_owned()))
    }

    fn note_id(&self, id: &str) -> Result<TreeID> {
        TreeID::try_from(id).map_err(|_| Error::Invalid(format!("bad id {id:?}")))
    }

    fn notes_node(&self, id: &str, expected: &str) -> Result<TreeID> {
        let node = self.note_id(id)?;
        if !is_alive(&self.notes, node) {
            return Err(Error::NotFound(id.to_owned()));
        }
        match kind(&self.notes, node) {
            Some(k) if k == expected => Ok(node),
            k => Err(Error::Invalid(format!("{id} is {k:?}, expected {expected}"))),
        }
    }

    fn notes_parent(&self, folder: Option<&str>) -> Result<TreeParentId> {
        Ok(match folder {
            None => TreeParentId::Root,
            Some(id) => TreeParentId::Node(self.notes_node(id, KIND_FOLDER)?),
        })
    }
}

fn note_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
    Ok(NoteSummary {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        title: r.get(2)?,
        preview: r.get(3)?,
        pinned: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

fn push_folders(all: &[Folder], parent: Option<&str>, depth: u32, out: &mut Vec<Folder>) {
    for folder in all.iter().filter(|f| f.parent_id.as_deref() == parent) {
        out.push(Folder {
            depth,
            ..folder.clone()
        });
        push_folders(all, Some(&folder.id), depth + 1, out);
    }
}

/// Title and preview: the first non-empty line and what follows it, without markdown markers.
pub fn title_and_preview(body: &str) -> (String, String) {
    let mut lines = body.lines().map(plain).filter(|l| !l.is_empty());
    let title = lines.next().unwrap_or_default();
    let mut preview = String::new();
    for line in lines {
        if !preview.is_empty() {
            preview.push(' ');
        }
        preview.push_str(&line);
        if preview.chars().count() >= PREVIEW_CHARS {
            break;
        }
    }
    let preview = preview.chars().take(PREVIEW_CHARS).collect();
    (title.chars().take(200).collect(), preview)
}

/// A line without leading block markers (`#`, `>`, list bullets, checkboxes) or emphasis markers.
fn plain(line: &str) -> String {
    let mut rest = line.trim();
    loop {
        let before = rest;
        for marker in ["- [ ] ", "- [x] ", "- [X] ", "* ", "- ", "+ ", "> "] {
            rest = rest.strip_prefix(marker).unwrap_or(rest);
        }
        rest = rest.trim_start_matches('#').trim_start();
        if rest == before {
            break;
        }
    }
    rest.replace(['*', '`', '~'], "").trim().to_owned()
}

// ---- projection ---------------------------------------------------------------------------------

pub fn apply(tree: &LoroTree, db: &Connection, dirty: TreeDirty) -> Result<()> {
    let (structure, meta) = dirty.into_parts();
    for id in &structure {
        project_subtree(tree, db, *id)?;
    }
    for id in meta.difference(&structure) {
        project_node(tree, db, *id)?;
    }
    Ok(())
}

pub fn rebuild(tree: &LoroTree, db: &Connection) -> Result<()> {
    db.execute_batch("DELETE FROM folder; DELETE FROM note;")?;
    for root in tree.roots() {
        project_subtree(tree, db, root)?;
    }
    Ok(())
}

fn project_subtree(tree: &LoroTree, db: &Connection, id: TreeID) -> Result<()> {
    project_node(tree, db, id)?;
    if is_alive(tree, id) {
        for child in tree.children(id).unwrap_or_default() {
            project_subtree(tree, db, child)?;
        }
    } else {
        remove_subtree(db, id)?;
    }
    Ok(())
}

fn project_node(tree: &LoroTree, db: &Connection, id: TreeID) -> Result<()> {
    let key = id.to_string();
    if !is_alive(tree, id) {
        db.execute("DELETE FROM folder WHERE id = ?1", [&key])?;
        db.execute("DELETE FROM note WHERE id = ?1", [&key])?;
        return Ok(());
    }
    let meta = tree.get_meta(id)?;
    let parent = match tree.parent(id) {
        Some(TreeParentId::Node(p)) => Some(p.to_string()),
        _ => None,
    };
    match meta_str(&meta, KIND).as_deref() {
        Some(KIND_FOLDER) => {
            db.execute(
                "INSERT INTO folder (id, parent_id, title, manual) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (id) DO UPDATE SET
                    parent_id = excluded.parent_id, title = excluded.title, manual = excluded.manual",
                params![
                    key,
                    parent,
                    meta_str(&meta, TITLE).unwrap_or_default(),
                    meta_str(&meta, ORDER).as_deref() == Some(ORDER_MANUAL)
                ],
            )?;
        }
        Some(KIND_NOTE) => {
            let body = meta_text(&meta, BODY).unwrap_or_default();
            let (title, preview) = title_and_preview(&body);
            let pinned = matches!(
                meta.get(PINNED),
                Some(loro::ValueOrContainer::Value(loro::LoroValue::Bool(true)))
            );
            db.execute(
                "INSERT INTO note (id, folder_id, title, preview, body, pinned, updated_at, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (id) DO UPDATE SET
                    folder_id = excluded.folder_id, title = excluded.title, preview = excluded.preview,
                    body = excluded.body, pinned = excluded.pinned, updated_at = excluded.updated_at,
                    position = excluded.position",
                params![
                    key,
                    parent,
                    title,
                    preview,
                    body,
                    pinned,
                    meta_i64(&meta, UPDATED_AT)
                        .or(meta_i64(&meta, CREATED_AT))
                        .unwrap_or(0),
                    tree.fractional_index(id).unwrap_or_default()
                ],
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Removes rows below a deleted folder (the tree still holds them, hidden).
fn remove_subtree(db: &Connection, id: TreeID) -> Result<()> {
    db.execute(
        "WITH RECURSIVE sub (id) AS (
             SELECT ?1 UNION ALL SELECT folder.id FROM folder JOIN sub ON folder.parent_id = sub.id
         )
         DELETE FROM note WHERE folder_id IN sub",
        [id.to_string()],
    )?;
    db.execute(
        "WITH RECURSIVE sub (id) AS (
             SELECT ?1 UNION ALL SELECT folder.id FROM folder JOIN sub ON folder.parent_id = sub.id
         )
         DELETE FROM folder WHERE id IN sub",
        [id.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::title_and_preview;

    #[test]
    fn titles_skip_markdown() {
        assert_eq!(
            title_and_preview("\n# Trip *plans*\n\n- [ ] book **hotel**\n> pack\n"),
            ("Trip plans".into(), "book hotel pack".into())
        );
        assert_eq!(title_and_preview(""), (String::new(), String::new()));
    }
}
