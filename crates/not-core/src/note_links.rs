//! Checklist lines in notes linked to tasks (ADR 0012). "Make tasks" leaves the lines where they
//! are and links each to the task made from it with a Loro rich-text mark (`task` = the task id) on
//! the line's characters. The mark is invisible in the text the editors show, so typing,
//! autocorrect and keyboards can't break it the way hidden markup broke: it sticks to the
//! characters that are still there, follows them through merges, and is undone with them.
//!
//! - A line is linked when it is a checklist item and one of its characters carries the mark of a
//!   live task; when several lines claim one task (a line split in two), the first one has it.
//!   Marks are never taken off: a deleted task's line just reads as a plain item, and undoing the
//!   delete links it again.
//! - Editing a linked line's box or title ticks, reopens or renames its task
//!   ([`Store::set_note_body`]).
//! - After every local change the linked notes catch up with their tasks
//!   ([`Store::sync_note_links`]): ticks and titles, and new open subtasks of linked tasks (and new
//!   tasks of a list made from the note) come in as linked lines. Each note remembers the tasks it
//!   has seen, so a line deleted from the note doesn't come back.
//!
//! Remote changes aren't caught up on import: the device that made them did that in the same
//! commit, so doing it again could put a line in twice.

use std::collections::{HashMap, HashSet};

use loro::{
    ExpandType, LoroDoc, LoroMap, LoroText, LoroValue, StyleConfig, StyleConfigMap, TextDelta, TreeID,
    TreeParentId, ValueOrContainer,
};

use crate::date::Date;
use crate::notes::{BODY, KIND_NOTE};
use crate::projection::{self, COMPLETED_AT, KIND_LIST, KIND_TASK, TITLE, meta_str};
use crate::store::{Store, complete_in};
use crate::{INBOX, Result};

/// The mark key on a linked line's characters; its value is the task id.
pub(crate) const LINK: &str = "task";
/// Note meta: task id → true for every task the note has shown (or chose not to).
pub(crate) const SEEN: &str = "task_seen";
/// Note meta: list id → true for lists made from the note, whose new tasks come into it.
pub(crate) const LISTS: &str = "task_lists";

/// Text typed at a linked line's edges stays out of the link; text typed inside it joins it.
pub(crate) fn configure(doc: &LoroDoc) {
    let mut styles = StyleConfigMap::new();
    styles.insert(
        LINK.into(),
        StyleConfig {
            expand: ExpandType::None,
        },
    );
    doc.config_text_style(styles);
}

/// A checklist item's parts. Offsets are in chars from the line's start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// The leading spaces and tabs.
    pub indent: String,
    pub done: bool,
    /// Trimmed.
    pub title: String,
    /// The ` ` or `x` inside the box.
    pub box_at: usize,
    /// Right after the box's `]`.
    pub after_box: usize,
}

impl Item {
    pub fn width(&self) -> usize {
        indent_width(&self.indent)
    }
}

pub(crate) fn indent_width(indent: &str) -> usize {
    indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// A checklist line: a `-`, `*`, `+` or `1.` marker, a space, then `[ ]` or `[x]`, then nothing or
/// a space and the title.
pub(crate) fn parse_item(line: &str) -> Option<Item> {
    let rest = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - rest.len()];
    let marker = if rest.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || !rest[digits..].starts_with(['.', ')']) {
            return None;
        }
        digits + 1
    };
    let rest = rest[marker..].strip_prefix(' ')?;
    let done = match rest.get(..3)? {
        "[ ]" => false,
        "[x]" | "[X]" => true,
        _ => return None,
    };
    let title = &rest[3..];
    if !(title.is_empty() || title.starts_with(' ')) {
        return None;
    }
    // All ASCII up to the title, so byte counts are char counts from the indent on.
    let start = indent.chars().count() + marker + 1;
    Some(Item {
        indent: indent.to_owned(),
        done,
        title: title.trim().to_owned(),
        box_at: start + 1,
        after_box: start + 3,
    })
}

/// A line of a note's text with the task ids marked on it.
#[derive(Debug, Clone)]
pub(crate) struct Line {
    /// Char offsets in the text; `end` is before the newline.
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub item: Option<Item>,
    /// Task ids marked on the line's characters, first seen first.
    pub ids: Vec<String>,
    /// The same ids, the one marking most of the title first: when edits leave a line with the
    /// marks of two tasks, its title says which task it is.
    pub ranked: Vec<String>,
    /// Each character's id, while reading.
    chars: Vec<Option<String>>,
}

pub(crate) fn lines_of(text: &LoroText) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut current = Line {
        start: 0,
        end: 0,
        text: String::new(),
        item: None,
        ids: Vec::new(),
        ranked: Vec::new(),
        chars: Vec::new(),
    };
    let mut at = 0usize;
    for delta in text.to_delta() {
        let TextDelta::Insert { insert, attributes } = delta else {
            continue;
        };
        let id = attributes.as_ref().and_then(|a| match a.get(LINK) {
            Some(LoroValue::String(s)) => Some(s.to_string()),
            _ => None,
        });
        for c in insert.chars() {
            if c == '\n' {
                current.end = at;
                let next = Line {
                    start: at + 1,
                    end: at + 1,
                    text: String::new(),
                    item: None,
                    ids: Vec::new(),
                    ranked: Vec::new(),
                    chars: Vec::new(),
                };
                lines.push(std::mem::replace(&mut current, next));
            } else {
                current.text.push(c);
                current.chars.push(id.clone());
                if let Some(id) = &id
                    && !current.ids.contains(id)
                {
                    current.ids.push(id.clone());
                }
            }
            at += 1;
        }
    }
    current.end = at;
    lines.push(current);
    for line in &mut lines {
        line.item = parse_item(&line.text);
        let title_from = line.item.as_ref().map_or(0, |i| i.after_box);
        let in_title = |id: &String| {
            line.chars
                .iter()
                .skip(title_from)
                .filter(|c| c.as_ref() == Some(id))
                .count()
        };
        let mut ranked: Vec<(usize, usize, String)> = line
            .ids
            .iter()
            .enumerate()
            .map(|(i, id)| (in_title(id), i, id.clone()))
            .collect();
        ranked.sort_by_key(|(count, i, _)| (std::cmp::Reverse(*count), *i));
        line.ranked = ranked.into_iter().map(|(_, _, id)| id).collect();
        line.chars = Vec::new();
    }
    lines
}

/// The task each line is linked to (see the module docs), by line index.
pub(crate) fn links(lines: &[Line], alive: impl Fn(&str) -> bool) -> Vec<Option<String>> {
    let mut claimed = HashSet::new();
    lines
        .iter()
        .map(|line| {
            line.item.as_ref()?;
            let id = line
                .ranked
                .iter()
                .find(|id| !claimed.contains(*id) && alive(id))?
                .clone();
            claimed.insert(id.clone());
            Some(id)
        })
        .collect()
}

/// Each line's checklist and parent item: lines in one checklist share a number, and an item's
/// parent is the nearest item above it with a smaller indent. Blank and indented lines continue a
/// checklist; other text ends it.
pub(crate) fn structure(lines: &[Line]) -> (Vec<usize>, Vec<Option<usize>>) {
    let mut blocks = vec![0; lines.len()];
    let mut parents = vec![None; lines.len()];
    let mut block = 0;
    let mut levels: Vec<(usize, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        match &line.item {
            Some(item) => {
                let width = item.width();
                while levels.last().is_some_and(|&(w, _)| w >= width) {
                    levels.pop();
                }
                parents[i] = levels.last().map(|&(_, p)| p);
                levels.push((width, i));
            }
            None if !line.text.trim().is_empty() && !line.text.starts_with([' ', '\t']) => {
                levels.clear();
                block += 1;
            }
            None => {}
        }
        blocks[i] = block;
    }
    (blocks, parents)
}

/// Links the characters in `range` to task `id` only, dropping marks they carried before.
pub(crate) fn relink(text: &LoroText, range: std::ops::Range<usize>, id: &str) -> Result<()> {
    if range.is_empty() {
        return Ok(());
    }
    text.unmark(range.clone(), LINK)?;
    text.mark(range, LINK, id)?;
    Ok(())
}

/// Where a linked line sits in its checklist (see [`Store::link_nesting`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Nesting {
    Top,
    /// Under the line linked to this task.
    Under(String),
    /// Under an item that isn't linked.
    Plain,
}

/// A linked line, as [`Store::note_links`] reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteLink {
    /// Index of the line in the body (split on `\n`).
    pub line: u32,
    pub task_id: String,
}

fn map_has(map: &Option<LoroMap>, key: &str) -> bool {
    map.as_ref().is_some_and(|m| m.get(key).is_some())
}

pub(crate) fn sub_map(meta: &LoroMap, key: &str) -> Option<LoroMap> {
    match meta.get(key)? {
        ValueOrContainer::Container(c) => c.into_map().ok(),
        ValueOrContainer::Value(_) => None,
    }
}

fn single_line(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Store {
    /// The task node for a linked id, if it's a live task.
    pub(crate) fn live_task(&self, id: &str) -> Option<TreeID> {
        let node = TreeID::try_from(id).ok()?;
        (projection::is_alive(&self.tasks, node)
            && projection::kind(&self.tasks, node).as_deref() == Some(KIND_TASK))
        .then_some(node)
    }

    fn task_state(&self, node: TreeID) -> (bool, String) {
        let meta = self.tasks.get_meta(node).ok();
        let done = meta.as_ref().is_some_and(|m| m.get(COMPLETED_AT).is_some());
        let title = meta.and_then(|m| meta_str(&m, TITLE)).unwrap_or_default();
        (done, title)
    }

    pub(crate) fn note_text(&self, note: TreeID) -> Option<LoroText> {
        match self.notes.get_meta(note).ok()?.get(BODY)? {
            ValueOrContainer::Container(c) => c.into_text().ok(),
            ValueOrContainer::Value(_) => None,
        }
    }

    pub(crate) fn note_lines(&self, text: &LoroText) -> (Vec<Line>, Vec<Option<String>>) {
        let lines = lines_of(text);
        let links = links(&lines, |id| self.live_task(id).is_some());
        (lines, links)
    }

    /// Each linked task's (ticked, title) as the text says.
    pub(crate) fn link_states(&self, text: &LoroText) -> HashMap<String, (bool, String)> {
        let (lines, links) = self.note_lines(text);
        lines
            .into_iter()
            .zip(links)
            .filter_map(|(line, id)| {
                let item = line.item?;
                Some((id?, (item.done, item.title)))
            })
            .collect()
    }

    /// The linked lines of a note.
    pub fn note_links(&self, note_id: &str) -> Result<Vec<NoteLink>> {
        let note = self.notes_node(note_id, KIND_NOTE)?;
        let Some(text) = self.note_text(note) else {
            return Ok(Vec::new());
        };
        let (_, links) = self.note_lines(&text);
        Ok(links
            .into_iter()
            .enumerate()
            .filter_map(|(i, id)| {
                Some(NoteLink {
                    line: i as u32,
                    task_id: id?,
                })
            })
            .collect())
    }

    /// Where each linked line sits in the note's nesting: under which linked line's task, at its
    /// checklist's top level, or under a plain (unlinked) item.
    pub(crate) fn link_nesting(&self, text: &LoroText) -> HashMap<String, Nesting> {
        let (lines, links) = self.note_lines(text);
        let (_, parents) = structure(&lines);
        links
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let nesting = match parents[i] {
                    None => Nesting::Top,
                    Some(p) => match &links[p] {
                        Some(parent) => Nesting::Under(parent.clone()),
                        None => Nesting::Plain,
                    },
                };
                Some((id.clone()?, nesting))
            })
            .collect()
    }

    /// Moves the tasks whose linked lines were indented or outdented between `before` and now:
    /// under the task of the linked line they now sit under (after their nearest linked sibling
    /// above), or out to their checklist's top level (after their nearest linked sibling above,
    /// else after the task they were under). Nesting under a plain item moves nothing.
    pub(crate) fn push_nesting_edits(
        &self,
        before: &HashMap<String, Nesting>,
        text: &LoroText,
    ) -> Result<()> {
        let after = self.link_nesting(text);
        let (lines, links) = self.note_lines(text);
        let (blocks, parents) = structure(&lines);
        let tree = &self.tasks;
        for (i, id) in links.iter().enumerate() {
            let Some(id) = id else { continue };
            let (Some(old), Some(new)) = (before.get(id), after.get(id)) else {
                continue;
            };
            if old == new || *new == Nesting::Plain {
                continue;
            }
            let Some(task) = self.live_task(id) else { continue };
            let sibling =
                |j: &usize| lines[*j].item.is_some() && blocks[*j] == blocks[i] && parents[*j] == parents[i];
            let linked = |j: usize| {
                links[j]
                    .as_deref()
                    .and_then(|id| self.live_task(id))
                    .filter(|&t| t != task)
            };
            let above = (0..i).rev().filter(sibling).find_map(linked);
            let moved = match new {
                Nesting::Under(parent) => {
                    let Some(parent) = self.live_task(parent) else {
                        continue;
                    };
                    match above.filter(|&s| tree.parent(s) == Some(TreeParentId::Node(parent))) {
                        Some(s) => tree.mov_after(task, s),
                        None => match (i + 1..lines.len())
                            .filter(sibling)
                            .find_map(linked)
                            .filter(|&s| tree.parent(s) == Some(TreeParentId::Node(parent)))
                        {
                            Some(s) => tree.mov_before(task, s),
                            None => tree.mov(task, TreeParentId::Node(parent)),
                        },
                    }
                }
                Nesting::Top => match above {
                    Some(s) => tree.mov_after(task, s),
                    None => {
                        // Out from under its parent: right after the top-level task above it.
                        let mut top = task;
                        while let Some(TreeParentId::Node(p)) = tree.parent(top)
                            && projection::kind(tree, p).as_deref() == Some(KIND_TASK)
                        {
                            top = p;
                        }
                        if top == task {
                            continue;
                        }
                        tree.mov_after(task, top)
                    }
                },
                Nesting::Plain => continue,
            };
            // A line indented under its own subtask can't move its task there: it stays put.
            if let Err(e) = moved
                && !matches!(e, loro::LoroError::TreeError(_))
            {
                return Err(e.into());
            }
        }
        Ok(())
    }

    /// Lines moved within the note (cut and pasted, or reordered) lose their marks with the old
    /// characters. An unlinked item whose title is that of a task whose line went in the same save
    /// takes its link back, rather than becoming a new task.
    pub(crate) fn relink_moved_lines(
        &self,
        before: &HashMap<String, (bool, String)>,
        text: &LoroText,
    ) -> Result<()> {
        let now = self.link_states(text);
        let mut lost: Vec<(&String, &String)> = before
            .iter()
            .filter(|(id, _)| !now.contains_key(*id) && self.live_task(id).is_some())
            .map(|(id, (_, title))| (id, title))
            .collect();
        if lost.is_empty() {
            return Ok(());
        }
        lost.sort();
        let (lines, links) = self.note_lines(text);
        for (i, line) in lines.iter().enumerate() {
            let Some(item) = &line.item else { continue };
            if links[i].is_some() || item.title.is_empty() {
                continue;
            }
            if let Some(k) = lost.iter().position(|(_, title)| **title == item.title) {
                let (id, _) = lost.remove(k);
                relink(text, line.start..line.end, id)?;
            }
        }
        Ok(())
    }

    /// Ticks, reopens and renames the tasks whose linked lines changed from `before` to what
    /// `text` holds now. An emptied title leaves the task's title alone.
    pub(crate) fn push_link_edits(
        &self,
        before: &HashMap<String, (bool, String)>,
        text: &LoroText,
        today: Date,
    ) -> Result<()> {
        for (id, (done, title)) in self.link_states(text) {
            let Some(old) = before.get(&id) else { continue };
            let Some(node) = self.live_task(&id) else { continue };
            let (task_done, task_title) = self.task_state(node);
            if *old != (done, title.clone()) {
                if done != old.0 && done != task_done {
                    complete_in(&self.tasks, node, done, today)?;
                }
                if title != old.1 && !title.is_empty() && title != task_title {
                    self.tasks.get_meta(node)?.insert(TITLE, single_line(&title))?;
                }
            }
        }
        Ok(())
    }

    /// Makes tasks of checklist items typed among linked ones (Return at a linked line, a line put
    /// in between, a linked line split in two): each goes next to its nearest linked sibling, or
    /// under its linked parent as a subtask, and its line is linked. Items with an empty title wait
    /// until they have one; a line still marked for a task that is gone stays plain.
    pub(crate) fn link_new_items(&self, note: TreeID, text: &LoroText) -> Result<()> {
        enum Place {
            After(TreeID),
            Before(TreeID),
            Under(TreeID),
        }
        // One at a time: each new task is an anchor for the next.
        for _ in 0..10_000 {
            let (lines, links) = self.note_lines(text);
            let (blocks, parents) = structure(&lines);
            let linked = |j: usize| links[j].as_deref().and_then(|id| self.live_task(id));
            let mut found = None;
            for (i, line) in lines.iter().enumerate() {
                let Some(item) = &line.item else { continue };
                if links[i].is_some()
                    || item.title.is_empty()
                    || line.ids.iter().any(|id| self.live_task(id).is_none())
                {
                    continue;
                }
                let sibling = |j: &usize| {
                    lines[*j].item.is_some() && blocks[*j] == blocks[i] && parents[*j] == parents[i]
                };
                let place = if let Some(t) = (0..i).rev().filter(sibling).find_map(linked) {
                    Place::After(t)
                } else if let Some(t) = (i + 1..lines.len()).filter(sibling).find_map(linked) {
                    Place::Before(t)
                } else if let Some(t) = parents[i].and_then(linked) {
                    Place::Under(t)
                } else {
                    continue;
                };
                found = Some((i, place));
                break;
            }
            let Some((i, place)) = found else { return Ok(()) };
            let tree = &self.tasks;
            let id = match place {
                Place::After(t) | Place::Before(t) => {
                    let parent = tree
                        .parent(t)
                        .ok_or_else(|| crate::Error::NotFound(t.to_string()))?;
                    let id = tree.create(parent)?;
                    match place {
                        Place::After(_) => tree.mov_after(id, t)?,
                        _ => tree.mov_before(id, t)?,
                    }
                    id
                }
                Place::Under(p) => tree.create(TreeParentId::Node(p))?,
            };
            let item = lines[i].item.as_ref().expect("an item");
            let meta = tree.get_meta(id)?;
            meta.insert(crate::projection::KIND, KIND_TASK)?;
            meta.insert(TITLE, single_line(&item.title))?;
            if item.done {
                meta.insert(COMPLETED_AT, crate::store::now_millis())?;
            }
            let id = id.to_string();
            // A split line's second half still carries the first half's mark: this one replaces it.
            relink(text, lines[i].start..lines[i].end, &id)?;
            self.notes
                .get_meta(note)?
                .ensure_mergeable_map(SEEN)?
                .insert(&id, true)?;
        }
        Ok(())
    }

    /// Brings every linked note up to date with its tasks; runs inside each local transaction.
    pub(crate) fn sync_note_links(&self) -> Result<()> {
        for note in self.notes.nodes() {
            if !projection::is_alive(&self.notes, note)
                || projection::kind(&self.notes, note).as_deref() != Some(KIND_NOTE)
            {
                continue;
            }
            let Ok(meta) = self.notes.get_meta(note) else {
                continue;
            };
            if sub_map(&meta, SEEN).is_none() {
                continue;
            }
            self.sync_note(note, &meta)?;
        }
        Ok(())
    }

    fn sync_note(&self, note: TreeID, meta: &LoroMap) -> Result<()> {
        let Some(text) = self.note_text(note) else {
            return Ok(());
        };
        // Ticks and titles, from the last line up so earlier offsets hold.
        let (lines, links) = self.note_lines(&text);
        for (line, id) in lines.iter().zip(&links).rev() {
            let (Some(item), Some(id)) = (&line.item, id) else {
                continue;
            };
            let Some(node) = self.live_task(id) else { continue };
            let (done, title) = self.task_state(node);
            let title = single_line(&title);
            let mut end = line.end;
            let rename = !item.title.is_empty() && item.title != title && !title.is_empty();
            if rename {
                let from = line.start + item.after_box;
                text.splice(from, line.end - from, &format!(" {title}"))?;
                end = from + 1 + title.chars().count();
            }
            if item.done != done {
                text.splice(line.start + item.box_at, 1, if done { "x" } else { " " })?;
            }
            // The edit may have taken the marked characters: mark the whole line again.
            if rename || item.done != done {
                text.mark(line.start..end, LINK, id.as_str())?;
            }
        }

        let seen = meta.ensure_mergeable_map(SEEN)?;
        for id in links.iter().flatten() {
            if seen.get(id).is_none() {
                seen.insert(id, true)?;
            }
        }
        let lists: Vec<TreeParentId> = sub_map(meta, LISTS)
            .map(|m| m.keys().map(|k| k.to_string()).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| {
                if id == INBOX {
                    return Some(TreeParentId::Root);
                }
                let node = TreeID::try_from(id.as_str()).ok()?;
                (projection::is_alive(&self.tasks, node)
                    && projection::kind(&self.tasks, node).as_deref() == Some(KIND_LIST))
                .then_some(TreeParentId::Node(node))
            })
            .collect();

        // New open tasks under linked tasks or in the note's lists, one at a time (each may bring
        // its own subtasks along).
        loop {
            let (lines, links) = self.note_lines(&text);
            let line_of: HashMap<&str, usize> = links
                .iter()
                .enumerate()
                .filter_map(|(i, id)| Some((id.as_deref()?, i)))
                .collect();
            let seen = Some(seen.clone());
            let mut parents: Vec<TreeParentId> = links
                .iter()
                .flatten()
                .filter_map(|id| self.live_task(id).map(TreeParentId::Node))
                .collect();
            parents.extend(lists.iter().copied());
            let mut found = None;
            'parents: for parent in parents {
                for child in self.tasks.children(parent).unwrap_or_default() {
                    let id = child.to_string();
                    if line_of.contains_key(id.as_str())
                        || map_has(&seen, &id)
                        || self.live_task(&id).is_none()
                    {
                        continue;
                    }
                    let (done, title) = self.task_state(child);
                    if done {
                        // Never shown: it isn't brought in when reopened either.
                        seen.as_ref().unwrap().insert(&id, true)?;
                        continue;
                    }
                    found = Some((parent, child, single_line(&title)));
                    break 'parents;
                }
            }
            let Some((parent, child, title)) = found else {
                break;
            };
            let id = child.to_string();
            seen.as_ref().unwrap().insert(&id, true)?;
            self.insert_task_line(&text, &lines, &line_of, parent, child, &title)?;
        }
        Ok(())
    }

    /// Puts a linked line for `child` (under `parent`) where it belongs among the note's lines:
    /// after its nearest linked sibling above (and that one's nested lines), else right under its
    /// linked parent, else before its nearest linked sibling below, else at the note's end.
    fn insert_task_line(
        &self,
        text: &LoroText,
        lines: &[Line],
        line_of: &HashMap<&str, usize>,
        parent: TreeParentId,
        child: TreeID,
        title: &str,
    ) -> Result<()> {
        // A list shows its completed tasks after the open ones; subtasks stay in their order.
        let in_list = !matches!(parent, TreeParentId::Node(p) if projection::kind(&self.tasks, p).as_deref() == Some(KIND_TASK));
        let siblings: Vec<TreeID> = self
            .tasks
            .children(parent)
            .unwrap_or_default()
            .into_iter()
            .filter(|&s| self.live_task(&s.to_string()).is_some())
            .filter(|&s| !in_list || s == child || !self.task_state(s).0)
            .collect();
        let at = siblings.iter().position(|&s| s == child).unwrap_or(0);
        let linked = |s: &TreeID| line_of.get(s.to_string().as_str()).copied();
        let indent_of = |i: usize| {
            lines[i]
                .item
                .as_ref()
                .map(|it| it.indent.clone())
                .unwrap_or_default()
        };

        enum Place {
            After(usize),
            Before(usize),
            End,
        }
        let (place, indent) = if let Some(k) = siblings[..at].iter().rev().find_map(linked) {
            // Past the sibling's nested lines.
            let width = indent_width(&indent_of(k));
            let mut end = k;
            while let Some(next) = lines.get(end + 1) {
                let lead: String = next
                    .text
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                if next.text.trim().is_empty() || indent_width(&lead) <= width {
                    break;
                }
                end += 1;
            }
            (Place::After(end), indent_of(k))
        } else if let TreeParentId::Node(p) = parent
            && let Some(&k) = line_of.get(p.to_string().as_str())
        {
            (Place::After(k), format!("{}  ", indent_of(k)))
        } else if let Some(k) = siblings[at + 1..].iter().find_map(linked) {
            (Place::Before(k), indent_of(k))
        } else {
            (Place::End, String::new())
        };

        let line = format!("{indent}- [ ] {title}");
        let len = line.chars().count();
        let id = child.to_string();
        let start = match place {
            Place::After(k) => {
                text.insert(lines[k].end, &format!("\n{line}"))?;
                lines[k].end + 1
            }
            Place::Before(k) => {
                text.insert(lines[k].start, &format!("{line}\n"))?;
                lines[k].start
            }
            Place::End => {
                let total = text.len_unicode();
                let last_empty = lines.last().is_some_and(|l| l.text.is_empty());
                if total == 0 || last_empty {
                    text.insert(total, &line)?;
                    total
                } else {
                    text.insert(total, &format!("\n{line}"))?;
                    total + 1
                }
            }
        };
        text.mark(start..start + len, LINK, id.as_str())?;
        Ok(())
    }
}

// ---- saving a body without moving marks onto the wrong lines ---------------------------------------

/// Replaces `text` with `new` line by line, so links stay on their lines. A plain text diff can
/// line up an inserted line with the inside of a linked one (`- [ ] a` → `- [ ] b\n- [ ] a` keeps
/// `- [ ] ` and `a`, inserting `b\n- [ ] ` in between), and text inserted inside a mark takes it.
/// Here unchanged lines are left alone, a changed line is edited in place (common start and end
/// kept), and new lines go in whole at line boundaries, unmarked.
pub(crate) fn apply_body(text: &LoroText, new: &str) -> Result<()> {
    let old = text.to_string();
    if old == new {
        return Ok(());
    }
    let a: Vec<&str> = old.split('\n').collect();
    let b: Vec<&str> = new.split('\n').collect();
    let mut starts = Vec::with_capacity(a.len());
    let mut total = 0;
    for line in &a {
        starts.push(total);
        total += line.chars().count() + 1;
    }
    let total = total - 1;
    let steps = align(&a, &b);
    // Last step first, so earlier offsets hold.
    for step in steps.into_iter().rev() {
        match step {
            Step::Keep(i, j) => {
                if a[i] != b[j] {
                    splice_line(text, starts[i], a[i], b[j])?;
                }
            }
            Step::Del(i) => {
                // With the newline before it (the first line: after it), so deleting several
                // lines in a row never takes one newline twice.
                let len = a[i].chars().count();
                let (from, to) = if i > 0 {
                    (starts[i] - 1, starts[i] + len)
                } else if a.len() > 1 {
                    (0, starts[1])
                } else {
                    (0, len)
                };
                text.delete(from, to - from)?;
            }
            Step::Ins(j, before) => {
                let (at, s) = if before < a.len() {
                    (starts[before], format!("{}\n", b[j]))
                } else {
                    (total, format!("\n{}", b[j]))
                };
                text.insert(at, &s)?;
                let len = s.chars().count();
                if len > 0 {
                    text.unmark(at..at + len, LINK)?;
                }
            }
        }
    }
    // Exact by construction; should it ever not be, the text still ends up as asked.
    if text.to_string() != new {
        debug_assert!(
            false,
            "line edit of {old:?} into {new:?} gave {:?}",
            text.to_string()
        );
        text.update(new, Default::default())
            .map_err(|e| crate::Error::Invalid(format!("note body: {e}")))?;
    }
    Ok(())
}

/// One line's edit: keep the common start and end, replace the middle.
fn splice_line(text: &LoroText, start: usize, old: &str, new: &str) -> Result<()> {
    let o: Vec<char> = old.chars().collect();
    let n: Vec<char> = new.chars().collect();
    let prefix = o.iter().zip(&n).take_while(|(x, y)| x == y).count();
    let max_suffix = o.len().min(n.len()) - prefix;
    let suffix = o
        .iter()
        .rev()
        .zip(n.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    let middle: String = n[prefix..n.len() - suffix].iter().collect();
    text.splice(start + prefix, o.len() - prefix - suffix, &middle)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Old line `.0` becomes new line `.1` (the same, or edited in place).
    Keep(usize, usize),
    Del(usize),
    /// New line `.0` goes in before old line `.1` (`a.len()`: at the end).
    Ins(usize, usize),
}

/// Lines in common (longest common subsequence of equal lines), and within each stretch between
/// them the changed lines paired by likeness, in order.
fn align(a: &[&str], b: &[&str]) -> Vec<Step> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let max_suf = a.len().min(b.len()) - pre;
    let suf = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suf)
        .take_while(|(x, y)| x == y)
        .count();
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    // Equal lines in the middle; a very long middle is taken as one change.
    let mut matches = Vec::new();
    if am.len() * bm.len() <= 4_000_000 {
        let mut dp = vec![vec![0u32; bm.len() + 1]; am.len() + 1];
        for i in (0..am.len()).rev() {
            for j in (0..bm.len()).rev() {
                dp[i][j] = if am[i] == bm[j] {
                    dp[i + 1][j + 1] + 1
                } else {
                    dp[i + 1][j].max(dp[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < am.len() && j < bm.len() {
            if am[i] == bm[j] {
                matches.push((pre + i, pre + j));
                i += 1;
                j += 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let mut steps: Vec<Step> = (0..pre).map(|i| Step::Keep(i, i)).collect();
    let (mut oi, mut nj) = (pre, pre);
    let ends = matches
        .into_iter()
        .chain(std::iter::once((a.len() - suf, b.len() - suf)));
    for (mi, mj) in ends {
        steps.extend(pair_changed(a, b, oi..mi, nj..mj));
        if mi < a.len() - suf {
            steps.push(Step::Keep(mi, mj));
        }
        oi = mi + 1;
        nj = mj + 1;
    }
    steps.extend((0..suf).map(|k| Step::Keep(a.len() - suf + k, b.len() - suf + k)));
    steps
}

/// How alike two lines are, 0 to 1: checklist items with the same title are alike whatever
/// their indent or box; otherwise the share of the longer line in a common start and end.
fn likeness(x: &str, y: &str) -> f64 {
    if let (Some(p), Some(q)) = (parse_item(x), parse_item(y))
        && !p.title.is_empty()
        && p.title == q.title
    {
        return 1.0;
    }
    let (x, y) = (x.trim_start(), y.trim_start());
    // An empty line has nothing to keep: filling or emptying one is an edit in place.
    if x.is_empty() || y.is_empty() {
        return 0.5;
    }
    let (o, n): (Vec<char>, Vec<char>) = (x.chars().collect(), y.chars().collect());
    let longest = o.len().max(n.len());
    if longest == 0 {
        return 1.0;
    }
    let prefix = o.iter().zip(&n).take_while(|(p, q)| p == q).count();
    let max_suffix = o.len().min(n.len()) - prefix;
    let suffix = o
        .iter()
        .rev()
        .zip(n.iter().rev())
        .take(max_suffix)
        .take_while(|(p, q)| p == q)
        .count();
    (prefix + suffix) as f64 / longest as f64
}

/// The changed old lines `olds` and new lines `news` between two lines in common: pairs of alike
/// lines (best total likeness, in order) are edited in place, the rest deleted or inserted.
fn pair_changed(
    a: &[&str],
    b: &[&str],
    olds: std::ops::Range<usize>,
    news: std::ops::Range<usize>,
) -> Vec<Step> {
    let (d, n) = (olds.len(), news.len());
    if d == 0 {
        return news.map(|j| Step::Ins(j, olds.end)).collect();
    }
    if n == 0 {
        return olds.map(Step::Del).collect();
    }
    const ALIKE: f64 = 0.5;
    let score = |i: usize, j: usize| {
        let s = likeness(a[olds.start + i], b[news.start + j]);
        (s >= ALIKE).then_some(s)
    };
    // Too many to compare: pair in order.
    if d * n > 250_000 {
        let mut steps = Vec::new();
        for k in 0..d.max(n) {
            match (k < d, k < n) {
                (true, true) => steps.push(Step::Keep(olds.start + k, news.start + k)),
                (true, false) => steps.push(Step::Del(olds.start + k)),
                (false, true) => steps.push(Step::Ins(news.start + k, olds.end)),
                _ => {}
            }
        }
        return steps;
    }
    let mut dp = vec![vec![0f64; n + 1]; d + 1];
    for i in (0..d).rev() {
        for j in (0..n).rev() {
            let mut best = dp[i + 1][j].max(dp[i][j + 1]);
            if let Some(s) = score(i, j) {
                best = best.max(dp[i + 1][j + 1] + s);
            }
            dp[i][j] = best;
        }
    }
    let mut raw = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < d || j < n {
        if i < d
            && j < n
            && let Some(s) = score(i, j)
            && (dp[i][j] - (dp[i + 1][j + 1] + s)).abs() < 1e-9
        {
            raw.push(Step::Keep(olds.start + i, news.start + j));
            i += 1;
            j += 1;
        } else if i < d && (j == n || dp[i][j] == dp[i + 1][j]) {
            raw.push(Step::Del(olds.start + i));
            i += 1;
        } else {
            raw.push(Step::Ins(news.start + j, olds.start + i));
            j += 1;
        }
    }
    // Between alike pairs, what's left is paired in order too (a line typed over keeps its place
    // and link); only extra new lines go in whole.
    let mut steps = Vec::new();
    let mut run: Vec<Step> = Vec::new();
    let flush = |run: &mut Vec<Step>, steps: &mut Vec<Step>, next_old: usize| {
        let dels: Vec<usize> = run
            .iter()
            .filter_map(|s| if let Step::Del(i) = s { Some(*i) } else { None })
            .collect();
        let ins: Vec<usize> = run
            .iter()
            .filter_map(|s| if let Step::Ins(j, _) = s { Some(*j) } else { None })
            .collect();
        let paired = dels.len().min(ins.len());
        for k in 0..paired {
            steps.push(Step::Keep(dels[k], ins[k]));
        }
        steps.extend(dels[paired..].iter().map(|&i| Step::Del(i)));
        steps.extend(ins[paired..].iter().map(|&j| Step::Ins(j, next_old)));
        run.clear();
    };
    for step in raw {
        match step {
            Step::Keep(i, _) => {
                flush(&mut run, &mut steps, i);
                steps.push(step);
            }
            _ => run.push(step),
        }
    }
    flush(&mut run, &mut steps, olds.end);
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(old: &str, new: &str) -> String {
        let doc = LoroDoc::new();
        configure(&doc);
        let text = doc.get_text("t");
        text.insert(0, old).unwrap();
        apply_body(&text, new).unwrap();
        text.to_string()
    }

    #[test]
    fn applies_any_edit_exactly() {
        let cases = [
            ("", "a"),
            ("a", ""),
            ("a\nb\nc", "a\nc"),
            ("a\nb\nc", "c"),
            ("a\nb\nc", "a"),
            ("a\nb", "x\na\nb"),
            ("a\nb", "a\nb\nx"),
            ("a\nb", "a\nb\n"),
            ("a\nb\n", "a\nb"),
            ("- [ ] a\n- [ ] b", "- [ ] b\n- [ ] a"),
            ("- [ ] a", "- [ ] plain\n  - [ ] a"),
            ("x\n\n\ny", "\n\nx\ny\n\n"),
            ("😀 a\nb", "😀 ab\nc😀"),
            ("one\ntwo\nthree\nfour", "zero\none\n2\nthree\nfive"),
            ("a", ""),
            ("", "\n"),
            ("\n", ""),
            ("a\nb\nc\nd", "d\nc\nb\na"),
            ("- [ ] a\n  - [x] b", "  - [x] b\n- [ ] a\n- [ ] a"),
        ];
        for (old, new) in cases {
            assert_eq!(run(old, new), new, "{old:?} → {new:?}");
        }
    }

    #[test]
    fn pairs_lines_by_title_and_inserts_new_ones_whole() {
        let a = ["- [ ] a"];
        let b = ["- [ ] plain", "  - [ ] a"];
        assert_eq!(align(&a, &b), vec![Step::Ins(0, 0), Step::Keep(0, 1)]);
        let a = ["- [ ] a", "- [ ] c"];
        let b = ["- [ ] a", "- [ ] b", "- [ ] c"];
        assert_eq!(
            align(&a, &b),
            vec![Step::Keep(0, 0), Step::Ins(1, 1), Step::Keep(1, 2)]
        );
        // A tick is an edit in place.
        assert_eq!(align(&["- [ ] milk"], &["- [x] milk"]), vec![Step::Keep(0, 0)]);
    }
}
