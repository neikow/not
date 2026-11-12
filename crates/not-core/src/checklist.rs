//! Checklists in notes made into tasks. "Make tasks" turns the checklist items a selection touches
//! (`- [ ] milk`, `  - [x] eggs`) into tasks (ticked ones completed, indented ones subtasks) and
//! links each line to its task; the lines stay where they are, as ordinary checklist lines. See
//! [`crate::note_links`] for how linked lines and tasks keep up with each other, and
//! `docs/adr/0012-linked-checklist-lines.md`.
//!
//! Offsets are UTF-16 code units, as in [`crate::markdown`].

use loro::{TreeID, TreeParentId};

use crate::date::Date;
use crate::markdown::{TaskSource, task_blocks};
use crate::note_links::{LINK, LISTS, SEEN, parse_item};
use crate::notes::{BODY, KIND_NOTE, UPDATED_AT};
use crate::projection::{COMPLETED_AT, KIND, KIND_LIST, KIND_TASK, TITLE};
use crate::store::{OutlineRow, Store, create_task_node, now_millis};
use crate::{Error, INBOX, Result};

const LINK_OPEN: &str = "<!--task:";
const LINK_CLOSE: &str = "-->";

/// Commits that bring old notes up to date carry this origin, so undo skips them.
pub(crate) const MIGRATE_ORIGIN: &str = "not-migrate";

/// Per-line task links of an earlier version (` <!--task:ID-->` at a line's end): the line
/// without one (and the space before it), and the id. [`Store::open_note`] turns them into links.
pub fn split_link(line: &str) -> (String, Option<&str>) {
    let Some(open) = line.find(LINK_OPEN) else {
        return (line.to_owned(), None);
    };
    let rest = &line[open + LINK_OPEN.len()..];
    let Some(close) = rest.find(LINK_CLOSE) else {
        return (line.to_owned(), None);
    };
    let id = &rest[..close];
    if id.is_empty() || id.contains(char::is_whitespace) {
        return (line.to_owned(), None);
    }
    let before = line[..open].strip_suffix(' ').unwrap_or(&line[..open]);
    (format!("{before}{}", &rest[close + LINK_CLOSE.len()..]), Some(id))
}

/// `body` without per-line task links of an earlier version.
pub(crate) fn strip_legacy_links(body: &str) -> String {
    if !body.contains(LINK_OPEN) {
        return body.to_owned();
    }
    body.split('\n')
        .map(|l| split_link(l).0)
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub title: String,
    pub done: bool,
    /// 0 = top level; each level is under the item before it with a smaller depth.
    pub depth: u32,
}

/// Where a new task goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Under {
    /// The list picked.
    Top,
    /// The task made from the item at this index.
    Item(usize),
    /// The task the line at this index is already linked to.
    Linked(usize),
}

/// The checklist items a selection touches (not already linked to tasks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checklist {
    pub items: Vec<ChecklistItem>,
    /// For a new list: the nearest heading above the items, else the note's title.
    pub suggested_title: String,
    /// Index of each item's line in the body (split on `\n`).
    lines: Vec<usize>,
    under: Vec<Under>,
}

/// What [`Store::checklist_to_tasks`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistMoved {
    pub list_id: String,
    pub count: u32,
}

/// The checklist items on the lines `start..end` touches (a caret touches its own line). `None`
/// when there are none.
pub fn checklist(body: &str, start: u32, end: u32) -> Option<Checklist> {
    checklist_skipping(body, start, end, &[])
}

/// [`checklist`] leaving out the lines whose index is in `linked`; an item nested under a linked
/// line goes under that line's task.
fn checklist_skipping(body: &str, start: u32, end: u32, linked: &[usize]) -> Option<Checklist> {
    let (start, end) = (start.min(end), start.max(end));
    let lines: Vec<&str> = body.split('\n').collect();
    let parsed: Vec<_> = lines.iter().map(|l| parse_item(l)).collect();
    let mut items = Vec::new();
    let mut item_lines = Vec::new();
    let mut under = Vec::new();
    // Open levels: (indent width, line index), to find each item's parent.
    let mut levels: Vec<(usize, usize)> = Vec::new();
    let mut offset = 0u32; // UTF-16 offset of the line's start in `body`
    for (index, line) in lines.iter().enumerate() {
        let len = line.encode_utf16().count() as u32;
        let touched = offset <= end && start <= offset + len;
        offset += len + 1;
        let Some(item) = &parsed[index] else {
            // Anything but an item (or a blank line) ends the checklist.
            if !line.trim().is_empty() {
                levels.clear();
            }
            continue;
        };
        let width = item.width();
        while levels.last().is_some_and(|&(w, _)| w >= width) {
            levels.pop();
        }
        let parent = levels.last().map(|&(_, i)| i);
        let depth = levels.len() as u32;
        levels.push((width, index));
        if !touched || linked.contains(&index) || item.title.is_empty() {
            continue;
        }
        under.push(match parent {
            Some(p) if linked.contains(&p) => Under::Linked(p),
            Some(p) => match item_lines.iter().position(|&l| l == p) {
                Some(i) => Under::Item(i),
                None => Under::Top,
            },
            None => Under::Top,
        });
        items.push(ChecklistItem {
            title: item.title.clone(),
            done: item.done,
            depth,
        });
        item_lines.push(index);
    }
    let first = *item_lines.first()?;
    Some(Checklist {
        suggested_title: suggested_title(&lines, first),
        items,
        lines: item_lines,
        under,
    })
}

fn suggested_title(lines: &[&str], first_item: usize) -> String {
    let heading = |line: &str| {
        let hashes = line.bytes().take_while(|&b| b == b'#').count();
        (1..=6)
            .contains(&hashes)
            .then(|| line[hashes..].strip_prefix(' '))
            .flatten()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    };
    if let Some(title) = lines[..first_item].iter().rev().find_map(|l| heading(l)) {
        return title;
    }
    lines
        .iter()
        .map(|l| l.trim_start_matches('#').trim())
        .find(|l| !l.is_empty() && parse_item(l).is_none())
        .map_or_else(|| "Checklist".to_owned(), str::to_owned)
}

/// What an old note needs to become linked lines: per line index, the lines replacing it.
struct Migration {
    /// (line index, new lines as (text, task id)).
    blocks: Vec<(usize, Vec<(String, String)>)>,
    /// (line index, the line without its marker, task id).
    markers: Vec<(usize, String, String)>,
    lists: Vec<String>,
}

impl Store {
    /// The checklist items `start..end` touches in `body` (what the editor shows for the note)
    /// that "Make tasks" would take: lines already linked to tasks are left out.
    pub fn note_checklist(
        &self,
        note_id: &str,
        body: &str,
        start: u32,
        end: u32,
    ) -> Result<Option<Checklist>> {
        let linked = self.linked_line_indexes(note_id)?;
        Ok(checklist_skipping(body, start, end, &linked))
    }

    fn linked_line_indexes(&self, note_id: &str) -> Result<Vec<usize>> {
        Ok(self
            .note_links(note_id)?
            .into_iter()
            .map(|l| l.line as usize)
            .collect())
    }

    /// Makes tasks of the checklist items `start..end` touches in `body` (what the editor shows
    /// for note `note_id`, saved first), in list `list_id` ([`crate::INBOX`] works too) or a new
    /// list titled `new_list` when `list_id` is `None`. They keep their order and go first in the
    /// list; an item nested under a line already linked becomes a subtask of that line's task.
    /// Each line stays and is linked to its task. A new list's later tasks come into the note too.
    /// One undo step.
    #[allow(clippy::too_many_arguments)]
    pub fn checklist_to_tasks(
        &self,
        note_id: &str,
        body: &str,
        start: u32,
        end: u32,
        list_id: Option<&str>,
        new_list: &str,
        today: Date,
    ) -> Result<ChecklistMoved> {
        let note = self.notes_node(note_id, KIND_NOTE)?;
        let target = match list_id {
            Some(id) => Some(self.container(id)?),
            None => None,
        };
        self.transact(|| {
            self.write_note_body(note, body, today)?;
            let text = self
                .note_text(note)
                .ok_or_else(|| Error::Invalid("no checklist items selected".into()))?;
            let (lines, links) = self.note_lines(&text);
            let linked: Vec<usize> = (0..links.len()).filter(|&i| links[i].is_some()).collect();
            let checklist = checklist_skipping(body, start, end, &linked)
                .ok_or_else(|| Error::Invalid("no checklist items selected".into()))?;
            let new_title = match new_list.trim() {
                "" => checklist.suggested_title.clone(),
                t => t.to_owned(),
            };
            let tree = &self.tasks;
            let (parent, list_id) = match target {
                Some(parent) => (parent, list_id.unwrap_or_default().to_owned()),
                None => {
                    let id = tree.create(TreeParentId::Root)?;
                    let meta = tree.get_meta(id)?;
                    meta.insert(KIND, KIND_LIST)?;
                    meta.insert(TITLE, new_title.as_str())?;
                    (TreeParentId::Node(id), id.to_string())
                }
            };
            let now = now_millis();
            let mut made: Vec<TreeID> = Vec::new();
            let mut last_top = None;
            for (i, item) in checklist.items.iter().enumerate() {
                let id = match checklist.under[i] {
                    Under::Item(p) => create_task_node(tree, TreeParentId::Node(made[p]))?,
                    Under::Linked(line) => {
                        let task = links[line]
                            .as_deref()
                            .and_then(|id| self.live_task(id))
                            .ok_or_else(|| Error::NotFound("linked task".into()))?;
                        create_task_node(tree, TreeParentId::Node(task))?
                    }
                    Under::Top => {
                        let id = create_task_node(tree, parent)?;
                        if let Some(prev) = last_top {
                            tree.mov_after(id, prev)?;
                        }
                        last_top = Some(id);
                        id
                    }
                };
                let meta = tree.get_meta(id)?;
                meta.insert(KIND, KIND_TASK)?;
                meta.insert(TITLE, item.title.as_str())?;
                if item.done {
                    meta.insert(COMPLETED_AT, now)?;
                }
                made.push(id);
            }
            let meta = self.notes.get_meta(note)?;
            let seen = meta.ensure_mergeable_map(SEEN)?;
            for (i, id) in made.iter().enumerate() {
                let id = id.to_string();
                let line = &lines[checklist.lines[i]];
                crate::note_links::relink(&text, line.start..line.end, &id)?;
                seen.insert(&id, true)?;
            }
            if target.is_none() {
                meta.ensure_mergeable_map(LISTS)?.insert(&list_id, true)?;
            }
            Ok(ChecklistMoved {
                list_id,
                count: made.len() as u32,
            })
        })
    }

    /// The body for the editor to open. Notes from earlier versions are brought up to date first:
    /// task block lines (`![](not-tasks:…)`) become linked lines of the tasks they showed, and
    /// per-line ` <!--task:ID-->` markers become links. That step isn't undoable.
    pub fn open_note(&self, note_id: &str) -> Result<String> {
        let note = self.notes_node(note_id, KIND_NOTE)?;
        let Some(text) = self.note_text(note) else {
            return Ok(String::new());
        };
        let body = text.to_string();
        let blocks = task_blocks(&body);
        if blocks.is_empty() && !body.contains(LINK_OPEN) {
            return Ok(body);
        }
        // Read what the blocks show before writing (the outline reads the read model).
        let line_index = |start: u32| {
            let mut offset = 0u32;
            for (i, l) in body.split('\n').enumerate() {
                if offset == start {
                    return i;
                }
                offset += l.encode_utf16().count() as u32 + 1;
            }
            usize::MAX
        };
        let mut plan = Migration {
            blocks: Vec::new(),
            markers: Vec::new(),
            lists: Vec::new(),
        };
        for block in &blocks {
            if let TaskSource::List(id) = &block.source {
                plan.lists.push(id.clone());
            }
            let rows = self.block_rows(&block.source)?;
            let lines = rows
                .into_iter()
                .map(|r| {
                    let indent = "  ".repeat(r.depth as usize);
                    let tick = if r.task.completed_at.is_some() { 'x' } else { ' ' };
                    let title = r.task.title.split_whitespace().collect::<Vec<_>>().join(" ");
                    (format!("{indent}- [{tick}] {title}"), r.task.id)
                })
                .collect();
            plan.blocks.push((line_index(block.start), lines));
        }
        for (i, line) in body.split('\n').enumerate() {
            if let (stripped, Some(id)) = split_link(line) {
                plan.markers.push((i, stripped, id.to_owned()));
            }
        }
        self.transact_with_origin(MIGRATE_ORIGIN, || {
            let meta = self.notes.get_meta(note)?;
            let seen = meta.ensure_mergeable_map(SEEN)?;
            for id in &plan.lists {
                meta.ensure_mergeable_map(LISTS)?.insert(id, true)?;
            }
            // Last line first, so earlier lines' offsets hold.
            let mut edits: Vec<(usize, Vec<(String, String)>)> = plan.blocks.clone();
            edits.extend(
                plan.markers
                    .iter()
                    .map(|(i, s, id)| (*i, vec![(s.clone(), id.clone())])),
            );
            edits.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
            let lines = crate::note_links::lines_of(&text);
            for (index, new_lines) in edits {
                let Some(line) = lines.get(index) else { continue };
                let joined = new_lines
                    .iter()
                    .map(|(t, _)| t.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                if new_lines.is_empty() {
                    // A block whose tasks are gone: the line goes, with its newline.
                    let (from, to) = if line.end < text.len_unicode() {
                        (line.start, line.end + 1)
                    } else {
                        (line.start.saturating_sub(1), line.end)
                    };
                    text.delete(from, to - from)?;
                    continue;
                }
                text.splice(line.start, line.end - line.start, &joined)?;
                let mut at = line.start;
                for (t, id) in &new_lines {
                    let len = t.chars().count();
                    text.mark(at..at + len, LINK, id.as_str())?;
                    seen.insert(id, true)?;
                    at += len + 1;
                }
            }
            meta.insert(UPDATED_AT, now_millis())?;
            Ok(())
        })?;
        Ok(self.note_text(note).map(|t| t.to_string()).unwrap_or_default())
    }

    /// The rows a task block of an earlier version showed: a whole list (open, then completed in
    /// the last day), or picked tasks with their subtrees; depths count from the block.
    fn block_rows(&self, source: &TaskSource) -> Result<Vec<OutlineRow>> {
        match source {
            TaskSource::List(id) => {
                let exists = id == INBOX || self.lists()?.iter().any(|l| l.id == *id);
                if !exists {
                    return Ok(Vec::new());
                }
                let outline = self.outline(id)?;
                Ok([outline.open, outline.completed].concat())
            }
            TaskSource::Tasks(ids) => {
                let mut lists: Vec<String> = Vec::new();
                for task in ids.iter().filter_map(|id| self.task_by_id(id).ok()) {
                    if !lists.contains(&task.list_id) {
                        lists.push(task.list_id);
                    }
                }
                let mut rows = Vec::new();
                for list in lists {
                    let outline = self.outline(&list)?;
                    for section in [outline.open, outline.completed] {
                        let mut taking: Option<u32> = None;
                        for row in section {
                            if let Some(depth) = taking {
                                if row.depth > depth {
                                    rows.push(OutlineRow {
                                        depth: row.depth - depth,
                                        ..row
                                    });
                                    continue;
                                }
                                taking = None;
                            }
                            if ids.contains(&row.task.id) {
                                taking = Some(row.depth);
                                rows.push(OutlineRow { depth: 0, ..row });
                            }
                        }
                    }
                }
                Ok(rows)
            }
        }
    }

    /// Saves `body` for `note`, pushing edits of linked lines to their tasks. Inside a transaction.
    pub(crate) fn write_note_body(&self, note: TreeID, body: &str, today: Date) -> Result<bool> {
        let meta = self.notes.get_meta(note)?;
        let text = meta.ensure_mergeable_text(BODY)?;
        if text.to_string() == body {
            return Ok(false);
        }
        let before = self.link_states(&text);
        let nesting = self.link_nesting(&text);
        crate::note_links::apply_body(&text, body)?;
        self.relink_moved_lines(&before, &text)?;
        self.push_link_edits(&before, &text, today)?;
        self.push_nesting_edits(&nesting, &text)?;
        self.link_new_items(note, &text)?;
        meta.insert(UPDATED_AT, now_millis())?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, done: bool, depth: u32) -> ChecklistItem {
        ChecklistItem {
            title: title.into(),
            done,
            depth,
        }
    }

    #[test]
    fn takes_the_touched_items() {
        let body = "Week\n## Groceries\n- [ ] milk\n  - [x] oat\n- [ ] eggs\nnot an item\n- [ ] later";
        // From inside "milk" to inside "not an item".
        let start = body.find("ilk").unwrap() as u32;
        let end = body.find("an item").unwrap() as u32;
        let c = checklist(body, start, end).unwrap();
        assert_eq!(
            c.items,
            vec![
                item("milk", false, 0),
                item("oat", true, 1),
                item("eggs", false, 0)
            ]
        );
        assert_eq!(c.suggested_title, "Groceries");
        assert_eq!(c.lines, vec![2, 3, 4]);
        assert_eq!(c.under, vec![Under::Top, Under::Item(0), Under::Top]);
    }

    #[test]
    fn a_caret_takes_its_line_and_titles_fall_back_to_the_note() {
        let body = "# Trip\nbring:\n* [ ] passport";
        let c = checklist(body, body.len() as u32, body.len() as u32).unwrap();
        assert_eq!(c.items, vec![item("passport", false, 0)]);
        assert_eq!(c.suggested_title, "Trip");
        assert!(checklist(body, 0, 3).is_none(), "the title line is no item");
        assert!(
            checklist("- [x]done", 0, 1).is_none(),
            "a box needs a space after it"
        );
        assert!(checklist("- [ ] ", 0, 6).is_none(), "an empty item makes no task");
    }

    #[test]
    fn numbered_and_starred_items_count() {
        let body = "1. [ ] one\n2) [X] two\n+ [ ] three";
        let c = checklist(body, 0, body.len() as u32).unwrap();
        assert_eq!(
            c.items,
            vec![
                item("one", false, 0),
                item("two", true, 0),
                item("three", false, 0)
            ]
        );
    }

    #[test]
    fn linked_lines_are_left_out_and_parent_their_nested_items() {
        let body = "- [ ] milk\n  - [ ] oat\n  - [ ] soy\n- [ ] eggs";
        let c = checklist_skipping(body, 0, body.len() as u32, &[0]).unwrap();
        assert_eq!(
            c.items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            ["oat", "soy", "eggs"]
        );
        assert_eq!(c.under, vec![Under::Linked(0), Under::Linked(0), Under::Top]);
        assert!(
            checklist_skipping(body, 0, 3, &[0]).is_none(),
            "only a linked line touched"
        );
    }

    #[test]
    fn a_parent_outside_the_selection_still_counts_when_linked() {
        let body = "- [ ] milk\n  - [ ] oat";
        let start = body.find("oat").unwrap() as u32;
        let c = checklist_skipping(body, start, start, &[0]).unwrap();
        assert_eq!(c.under, vec![Under::Linked(0)]);
        // Unlinked and not selected: the item goes to the top level.
        let c = checklist(body, start, start).unwrap();
        assert_eq!(c.under, vec![Under::Top]);
    }

    #[test]
    fn text_between_items_ends_the_nesting() {
        let body = "- [ ] milk\nsome words\n  - [ ] oat";
        let c = checklist_skipping(body, 0, body.len() as u32, &[0]).unwrap();
        assert_eq!(c.under, vec![Under::Top]);
    }

    #[test]
    fn legacy_links_are_split() {
        assert_eq!(
            split_link("- [ ] tea <!--task:5@9-->"),
            ("- [ ] tea".into(), Some("5@9"))
        );
        assert_eq!(
            split_link("- [ ] tea <!--task:5@9--> hot"),
            ("- [ ] tea hot".into(), Some("5@9"))
        );
        assert_eq!(
            split_link("- [ ] <!-- note -->"),
            ("- [ ] <!-- note -->".into(), None)
        );
        assert_eq!(
            strip_legacy_links("a\n- [x] b <!--task:1@2-->\nc"),
            "a\n- [x] b\nc"
        );
    }

    #[test]
    fn offsets_are_utf16() {
        let body = "😀😀\n- [ ] tea";
        let start = "😀😀\n".encode_utf16().count() as u32;
        let c = checklist(body, start, start).unwrap();
        assert_eq!(c.items[0].title, "tea");
    }
}
