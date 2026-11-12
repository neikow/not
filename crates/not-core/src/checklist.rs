//! Checklist lines of a note linked to tasks. "Make tasks" turns the checklist items a selection
//! touches (`- [ ] milk`, `  - [x] eggs`) into tasks (ticked ones completed, indented ones
//! subtasks) and keeps each line in the note with a hidden link to its task: ` <!--task:ID-->` at
//! the end of the line. From then on the line shows the task (its tick and title, see
//! [`Store::render_task_links`]) and editing the line in the note ticks or renames the task
//! ([`Store::apply_task_link_edits`]). Other markdown readers see a plain checklist.
//!
//! Offsets are UTF-16 code units, as in [`crate::markdown`].

use std::collections::HashMap;

use loro::TreeParentId;

use crate::date::Date;
use crate::notes::{BODY, KIND_NOTE, UPDATED_AT};
use crate::projection::{COMPLETED_AT, KIND, KIND_LIST, KIND_TASK, TITLE};
use crate::store::{Store, clean_title, complete_in, create_task_node, now_millis};
use crate::{Error, Result};

const LINK_OPEN: &str = "<!--task:";
const LINK_CLOSE: &str = "-->";

/// The hidden link put at the end of a line made into a task.
pub fn task_link(id: &str) -> String {
    format!(" {LINK_OPEN}{id}{LINK_CLOSE}")
}

/// A line without its task link (with the space before it), and the linked task's id.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub title: String,
    pub done: bool,
    /// 0 = top level; each level is under the item before it with a smaller depth.
    pub depth: u32,
}

/// The checklist items a selection touches that aren't linked to tasks yet.
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
    /// The note's new body (also saved), for the editor to show: the lines, now linked.
    pub body: String,
    /// The end of the last linked line: a place for the caret.
    pub caret: u32,
    pub count: u32,
}

/// The checklist items not yet linked to tasks on the lines `start..end` touches (a caret
/// touches its own line). `None` when there are none.
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
        if !touched || split_link(line).1.is_some() {
            continue;
        }
        let Some((indent, done, title)) = parse_item(line) else { continue };
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
    let (line, _) = split_link(line);
    let line = line.as_str();
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
        None => (true, rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]"))?),
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
    /// `new_list` when `list_id` is `None`. They keep their order and go first in the list; their
    /// lines stay in the note, linked to the tasks. One undo step.
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
            let mut lines: Vec<String> = body.split('\n').map(str::to_owned).collect();
            for (item, &line) in checklist.items.iter().zip(&checklist.lines) {
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
                lines[line].push_str(&task_link(&id.to_string()));
            }
            let last = *checklist.lines.last().unwrap_or(&0);
            let caret = lines[..=last].iter().map(|l| l.encode_utf16().count() as u32 + 1).sum::<u32>() - 1;
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
}

impl Store {
    /// `body` with each linked line showing its task: the tick and title as the task has them
    /// now (it may have been ticked or renamed elsewhere). Lines of deleted tasks stay as written.
    pub(crate) fn render_task_links(&self, body: &str) -> String {
        if !body.contains(LINK_OPEN) {
            return body.to_owned();
        }
        body.split('\n')
            .map(|line| {
                let (plain, Some(id)) = split_link(line) else {
                    return line.to_owned();
                };
                match (self.task_by_id(id), item_prefix(&plain)) {
                    (Ok(task), Some(prefix)) => format!(
                        "{prefix}[{}] {}{}",
                        if task.completed_at.is_some() { 'x' } else { ' ' },
                        task.title,
                        task_link(id)
                    ),
                    _ => line.to_owned(),
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// What saving `body` over `shown` (the rendered body the editor had) does to linked tasks:
    /// lines whose tick or title changed tick, reopen or rename their task. Call inside the
    /// note's transaction so it's one undo step.
    pub(crate) fn apply_task_link_edits(&self, shown: &str, body: &str) -> Result<()> {
        if !body.contains(LINK_OPEN) {
            return Ok(());
        }
        let before: HashMap<String, (bool, String)> = shown
            .split('\n')
            .filter_map(|l| linked_item(l).map(|(id, done, title)| (id, (done, title))))
            .collect();
        let today = Date::from_ymd(1970, 1, 1)
            .expect("epoch")
            .add_days(now_millis().div_euclid(86_400_000));
        for line in body.split('\n') {
            let Some((id, done, title)) = linked_item(line) else { continue };
            let Some((was_done, was_title)) = before.get(&id) else { continue };
            // A task deleted since: the line is just text now.
            let Ok(node) = self.node(&id, &[KIND_TASK]) else { continue };
            if done != *was_done {
                complete_in(&self.tasks, node, done, today)?;
            }
            if title != *was_title && let Ok(title) = clean_title(&title) {
                self.tasks.get_meta(node)?.insert(TITLE, title)?;
            }
        }
        Ok(())
    }
}

/// `(task id, ticked, title)` of a linked checklist line.
fn linked_item(line: &str) -> Option<(String, bool, String)> {
    let (_, id) = split_link(line);
    let id = id?.to_owned();
    let (_, done, title) = parse_item(line)?;
    Some((id, done, title))
}

/// What comes before the box of a checklist line: indent and marker (`"  - "`).
fn item_prefix(line: &str) -> Option<&str> {
    parse_item(line)?;
    let at = line.find('[')?;
    Some(&line[..at])
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
    fn takes_the_touched_items_that_arent_linked() {
        let body = "Week\n## Groceries\n- [ ] milk\n  - [x] oat\n- [ ] eggs <!--task:1@2-->\nnot an item\n- [ ] later";
        // From inside "milk" to inside "not an item": the linked "eggs" is left out.
        let start = body.find("ilk").unwrap() as u32;
        let end = body.find("an item").unwrap() as u32;
        let c = checklist(body, start, end).unwrap();
        assert_eq!(c.items, vec![item("milk", false, 0), item("oat", true, 1)]);
        assert_eq!(c.suggested_title, "Groceries");
        assert_eq!(c.lines, vec![2, 3]);
    }

    #[test]
    fn a_caret_takes_its_line_and_titles_fall_back_to_the_note() {
        let body = "# Trip\nbring:\n* [ ] passport";
        let c = checklist(body, body.len() as u32, body.len() as u32).unwrap();
        assert_eq!(c.items, vec![item("passport", false, 0)]);
        assert_eq!(c.suggested_title, "Trip");
        assert!(checklist(body, 0, 3).is_none(), "the title line is no item");
        assert!(checklist("- [x]done", 0, 1).is_none(), "a box needs a space after it");
    }

    #[test]
    fn links_split_off_lines() {
        assert_eq!(split_link("- [ ] tea <!--task:5@9-->"), ("- [ ] tea".into(), Some("5@9")));
        assert_eq!(split_link("- [ ] tea <!--task:5@9--> hot"), ("- [ ] tea hot".into(), Some("5@9")));
        assert_eq!(split_link("- [ ] <!-- note -->"), ("- [ ] <!-- note -->".into(), None));
        assert_eq!(task_link("5@9"), " <!--task:5@9-->");
    }

    #[test]
    fn offsets_are_utf16() {
        let body = "😀😀\n- [ ] tea";
        let start = "😀😀\n".encode_utf16().count() as u32;
        let c = checklist(body, start, start).unwrap();
        assert_eq!(c.items[0].title, "tea");
    }
}
