//! Checklists in notes made into tasks. "Make tasks" copies the checklist items a selection touches
//! (`- [ ] milk`, `  - [x] eggs`) into tasks (ticked ones completed, indented ones subtasks); the
//! lines stay in the note as ordinary checkboxes, and the note and the tasks go their own ways
//! from then on. See `docs/adr/0013-checklists-stay-in-notes.md`.
//!
//! Offsets are UTF-16 code units, as in [`crate::markdown`].

use loro::{TreeID, TreeParentId};

use crate::markdown::{TaskSource, task_blocks};
use crate::notes::{BODY, KIND_NOTE, UPDATED_AT};
use crate::projection::{COMPLETED_AT, KIND, KIND_LIST, KIND_TASK, TITLE};
use crate::store::{OutlineRow, Store, create_task_node, now_millis};
use crate::{Error, INBOX, Result};

const LINK_OPEN: &str = "<!--task:";
const LINK_CLOSE: &str = "-->";

/// Commits that bring old notes up to date carry this origin, so undo skips them.
pub(crate) const MIGRATE_ORIGIN: &str = "not-migrate";

/// Per-line task links of an earlier version (` <!--task:ID-->` at a line's end): the line
/// without one (and the space before it), and the id.
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
    let (start, end) = (start.min(end), start.max(end));
    let mut items = Vec::new();
    let mut first = None;
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
            first.get_or_insert(index);
        }
    }
    Some(Checklist {
        suggested_title: suggested_title(&lines, first?),
        items,
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
    /// for note `note_id`, saved first), in list `list_id` ([`crate::INBOX`] works too) or a new
    /// list titled `new_list` when `list_id` is `None`. They keep their order and go first in the
    /// list. The note keeps its lines as they are. One undo step.
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
            self.write_note_body(note, body)?;
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
            let mut levels: Vec<TreeID> = Vec::new();
            let mut last_top = None;
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
            Ok(ChecklistMoved {
                list_id,
                count: checklist.items.len() as u32,
            })
        })
    }

    /// The body for the editor to open. Notes from earlier versions are brought up to date first:
    /// a task block line (`![](not-tasks:…)`) becomes plain checklist lines of the tasks it
    /// showed, and per-line ` <!--task:ID-->` markers go. That step isn't undoable.
    pub fn open_note(&self, note_id: &str) -> Result<String> {
        let note = self.notes_node(note_id, KIND_NOTE)?;
        let body = match self.notes.get_meta(note)?.get(BODY) {
            Some(loro::ValueOrContainer::Container(c)) => {
                c.into_text().map(|t| t.to_string()).unwrap_or_default()
            }
            _ => return Ok(String::new()),
        };
        let blocks = task_blocks(&body);
        if blocks.is_empty() && !body.contains(LINK_OPEN) {
            return Ok(body);
        }
        // What each block line shows, by line index (read before writing: the outline reads the
        // read model, which writing locks).
        let mut starts = std::collections::HashMap::new();
        let mut offset = 0u32;
        for (i, l) in body.split('\n').enumerate() {
            starts.insert(offset, i);
            offset += l.encode_utf16().count() as u32 + 1;
        }
        let mut replaced = std::collections::HashMap::new();
        for block in &blocks {
            let Some(&index) = starts.get(&block.start) else {
                continue;
            };
            let lines: Vec<String> = self
                .block_rows(&block.source)?
                .into_iter()
                .map(|r| {
                    let indent = "  ".repeat(r.depth as usize);
                    let tick = if r.task.completed_at.is_some() { 'x' } else { ' ' };
                    let title = r.task.title.split_whitespace().collect::<Vec<_>>().join(" ");
                    format!("{indent}- [{tick}] {title}")
                })
                .collect();
            replaced.insert(index, lines);
        }
        let mut out: Vec<String> = Vec::new();
        for (i, line) in body.split('\n').enumerate() {
            match replaced.remove(&i) {
                Some(lines) => out.extend(lines),
                None => out.push(split_link(line).0),
            }
        }
        let updated = out.join("\n");
        self.transact_with_origin(MIGRATE_ORIGIN, || {
            self.write_note_body(note, &updated).map(|_| ())
        })?;
        Ok(updated)
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

    /// Saves `body` for `note` if it changed. Inside a transaction.
    pub(crate) fn write_note_body(&self, note: TreeID, body: &str) -> Result<bool> {
        let meta = self.notes.get_meta(note)?;
        let text = meta.ensure_mergeable_text(BODY)?;
        if text.to_string() == body {
            return Ok(false);
        }
        text.update(body, Default::default())
            .map_err(|e| Error::Invalid(format!("note body: {e}")))?;
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
