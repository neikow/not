//! Checklists in notes made into tasks. "Make tasks" turns the checklist items a selection
//! touches (`- [ ] milk`, `  - [x] eggs`) into tasks (ticked ones completed, indented ones
//! subtasks) and puts one task block line where they were: `![](not-tasks:list:<id>)` for a new
//! list, `![](not-tasks:<id>,<id>)` for tasks added to an existing one. The editors draw the block
//! from [`Store::task_block`]: the tasks as they are now, with their subtasks, so ticks, renames
//! and subtasks made anywhere show in the note. The line is atomic, like an image line, so typing
//! can't break it. See `docs/adr/0011-task-blocks-in-notes.md`.
//!
//! Offsets are UTF-16 code units, as in [`crate::markdown`].

use loro::TreeParentId;

use crate::markdown::{TaskSource, task_block_markup};
use crate::notes::{BODY, KIND_NOTE, UPDATED_AT};
use crate::projection::{COMPLETED_AT, KIND, KIND_LIST, KIND_TASK, TITLE};
use crate::store::{OutlineRow, Store, create_task_node, now_millis};
use crate::{Error, INBOX, Result};

const LINK_OPEN: &str = "<!--task:";
const LINK_CLOSE: &str = "-->";

/// Per-line task links of an earlier version (` <!--task:ID-->` at a line's end): the line
/// without one (and the space before it), and the id. Notes still holding them read as plain
/// checklists (see [`strip_legacy_links`]).
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

/// The checklist items a selection touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checklist {
    pub items: Vec<ChecklistItem>,
    /// For a new list: the nearest heading above the items, else the note's title.
    pub suggested_title: String,
    /// Index of each item's line in the body (split on `\n`).
    lines: Vec<usize>,
}

/// What [`Store::checklist_to_tasks`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistMoved {
    pub list_id: String,
    /// The note's new body (also saved), for the editor to show: the block line in place of the
    /// items.
    pub body: String,
    /// At the start of the line after the block: a place for the caret.
    pub caret: u32,
    pub count: u32,
}

/// A task block as the editors draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBlock {
    /// The list's title for a whole-list block; empty for picked tasks.
    pub title: String,
    /// Where tasks added from the block go: the list, or the picked tasks' list ([`INBOX`] for
    /// Inbox tasks).
    pub list_id: String,
    /// Open tasks with their subtasks in list order, then completed ones (see
    /// [`Store::outline`]); depths count from the block's top level.
    pub rows: Vec<OutlineRow>,
    /// The list (or every picked task) was deleted: the block says so instead of showing rows.
    pub gone: bool,
}

/// The checklist items on the lines `start..end` touches (a caret touches its own line). `None`
/// when there are none.
pub fn checklist(body: &str, start: u32, end: u32) -> Option<Checklist> {
    let (start, end) = (start.min(end), start.max(end));
    let mut items = Vec::new();
    let mut item_lines = Vec::new();
    // Indents of the open levels, to turn indentation into depth.
    let mut indents: Vec<usize> = Vec::new();
    let mut offset = 0u32; // UTF-16 offset of the line's start in `body`
    let lines: Vec<&str> = body.split('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        let len = line.encode_utf16().count() as u32;
        let touched = offset <= end && start <= offset + len;
        offset += len + 1;
        if !touched {
            continue;
        }
        let Some((indent, done, title)) = parse_item(line) else {
            continue;
        };
        while indents.last().is_some_and(|&i| i >= indent) {
            indents.pop();
        }
        let depth = indents.len() as u32;
        indents.push(indent);
        if !title.is_empty() {
            items.push(ChecklistItem { title, done, depth });
            item_lines.push(index);
        }
    }
    let first = *item_lines.first()?;
    Some(Checklist {
        suggested_title: suggested_title(&lines, first),
        items,
        lines: item_lines,
    })
}

/// `(indent width, ticked, title)` of a checklist line: a `-`, `*`, `+` or `1.` marker, then `[ ]`
/// or `[x]`.
fn parse_item(line: &str) -> Option<(usize, bool, String)> {
    let rest = line.trim_start_matches([' ', '\t']);
    let indent: usize = line[..line.len() - rest.len()]
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    let rest = if let Some(r) = rest.strip_prefix(['-', '*', '+']) {
        r
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        rest[digits..].strip_prefix(['.', ')']).filter(|_| digits > 0)?
    };
    let rest = rest.strip_prefix(' ')?;
    let (done, title) = match rest.strip_prefix("[ ]") {
        Some(t) => (false, t),
        None => (
            true,
            rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]"))?,
        ),
    };
    if !(title.is_empty() || title.starts_with(' ')) {
        return None;
    }
    Some((indent, done, title.trim().to_owned()))
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

impl Store {
    /// Makes tasks of the checklist items `start..end` touches in `body` (what the editor shows
    /// for note `note_id`), in list `list_id` ([`crate::INBOX`] works too) or a new list titled
    /// `new_list` when `list_id` is `None`. They keep their order and go first in the list; a task
    /// block line takes the place of the first item, the other items' lines go. One undo step.
    pub fn checklist_to_tasks(
        &self,
        note_id: &str,
        body: &str,
        start: u32,
        end: u32,
        list_id: Option<&str>,
        new_list: &str,
    ) -> Result<ChecklistMoved> {
        let checklist = checklist(body, start, end)
            .ok_or_else(|| Error::Invalid("no checklist items selected".into()))?;
        let note = self.notes_node(note_id, KIND_NOTE)?;
        let target = match list_id {
            Some(id) => Some(self.container(id)?),
            None => None,
        };
        let new_title = match new_list.trim() {
            "" => checklist.suggested_title.clone(),
            t => t.to_owned(),
        };
        self.transact(|| {
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
            // The task made for each open level, and the top-level one before.
            let mut levels = Vec::new();
            let mut last_top = None;
            let mut roots = Vec::new();
            for item in &checklist.items {
                levels.truncate(item.depth as usize);
                let id = match levels.last() {
                    Some(&above) => create_task_node(tree, TreeParentId::Node(above))?,
                    None => {
                        let id = create_task_node(tree, parent)?;
                        if let Some(prev) = last_top {
                            tree.mov_after(id, prev)?;
                        }
                        last_top = Some(id);
                        roots.push(id.to_string());
                        id
                    }
                };
                let meta = tree.get_meta(id)?;
                meta.insert(KIND, KIND_TASK)?;
                meta.insert(TITLE, item.title.as_str())?;
                if item.done {
                    meta.insert(COMPLETED_AT, now)?;
                }
                levels.push(id);
            }
            // A new list shows whole (tasks added to it later too); in an existing list, just
            // these tasks.
            let source = if target.is_none() {
                TaskSource::List(list_id.clone())
            } else {
                TaskSource::Tasks(roots)
            };
            let mut lines: Vec<String> = Vec::new();
            let mut caret = 0;
            for (index, line) in body.split('\n').enumerate() {
                if index == checklist.lines[0] {
                    lines.push(task_block_markup(&source));
                    caret = lines.iter().map(|l| l.encode_utf16().count() as u32 + 1).sum();
                } else if !checklist.lines.contains(&index) {
                    lines.push(line.to_owned());
                }
            }
            // Always a line after the block, for the caret and for typing on.
            if lines.len() == checklist.lines[0] + 1 {
                lines.push(String::new());
            }
            let new_body = lines.join("\n");
            let meta = self.notes.get_meta(note)?;
            meta.ensure_mergeable_text(BODY)?
                .update(&new_body, Default::default())
                .map_err(|e| Error::Invalid(format!("note body: {e}")))?;
            meta.insert(UPDATED_AT, now)?;
            Ok(ChecklistMoved {
                list_id,
                body: new_body,
                caret,
                count: checklist.items.len() as u32,
            })
        })
    }

    /// Adds a task from a block (first in the list, like any new task). A block of picked tasks
    /// takes the new one in: the returned source has its id, and the caller puts
    /// [`task_block_markup`] of it in place of the block's line. Returns `(task id, source)`.
    pub fn add_task_from_block(&self, source: &TaskSource, title: &str) -> Result<(String, TaskSource)> {
        let block = self.task_block(source)?;
        if block.gone {
            return Err(Error::NotFound("the block's list or tasks".into()));
        }
        let id = self.add_task(&block.list_id, title)?;
        let source = match source {
            TaskSource::List(_) => source.clone(),
            TaskSource::Tasks(ids) => TaskSource::Tasks([vec![id.clone()], ids.clone()].concat()),
        };
        Ok((id, source))
    }

    /// What a task block shows now.
    pub fn task_block(&self, source: &TaskSource) -> Result<TaskBlock> {
        match source {
            TaskSource::List(id) => {
                let title = if id == INBOX {
                    Some("Inbox".to_owned())
                } else {
                    self.lists()?.into_iter().find(|l| l.id == *id).map(|l| l.title)
                };
                let Some(title) = title else {
                    return Ok(TaskBlock {
                        title: String::new(),
                        list_id: id.clone(),
                        rows: Vec::new(),
                        gone: true,
                    });
                };
                let outline = self.outline(id)?;
                Ok(TaskBlock {
                    title,
                    list_id: id.clone(),
                    rows: [outline.open, outline.completed].concat(),
                    gone: false,
                })
            }
            TaskSource::Tasks(ids) => {
                let alive: Vec<_> = ids.iter().filter_map(|id| self.task_by_id(id).ok()).collect();
                let Some(first) = alive.first() else {
                    return Ok(TaskBlock {
                        title: String::new(),
                        list_id: INBOX.to_owned(),
                        rows: Vec::new(),
                        gone: true,
                    });
                };
                let list_id = first.list_id.clone();
                let mut lists: Vec<String> = Vec::new();
                for task in &alive {
                    if !lists.contains(&task.list_id) {
                        lists.push(task.list_id.clone());
                    }
                }
                // Each picked task with its subtree, wherever it is now, in list order. A picked
                // task that became another's subtask shows under its own level.
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
                Ok(TaskBlock {
                    title: String::new(),
                    list_id,
                    rows,
                    gone: false,
                })
            }
        }
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
    fn legacy_links_are_stripped() {
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
