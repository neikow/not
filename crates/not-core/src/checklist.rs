//! Turning checklist lines of a note into tasks: the editor sends the body it shows and the
//! selection, the lines it touches that are checklist items (`- [ ] milk`, `  - [x] eggs`) become
//! tasks (ticked ones completed, indented ones subtasks) and leave the note, in one undo step.
//!
//! Offsets are UTF-16 code units, as in [`crate::markdown`].

use loro::TreeParentId;

use crate::notes::{BODY, KIND_NOTE, UPDATED_AT};
use crate::projection::{COMPLETED_AT, KIND, KIND_LIST, KIND_TASK, TITLE};
use crate::store::{Store, create_task_node, now_millis};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub title: String,
    pub done: bool,
    /// 0 = top level; each level is under the item before it with a smaller depth.
    pub depth: u32,
}

/// The checklist items a selection touches, and what the note looks like without them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checklist {
    pub items: Vec<ChecklistItem>,
    /// For a new list: the nearest heading above the items, else the note's title.
    pub suggested_title: String,
    /// The body with the items' lines taken out (other lines in the selection stay).
    pub remaining: String,
    /// Where the first item's line was, in `remaining`: a place for the caret.
    pub caret: u32,
}

/// What [`Store::checklist_to_tasks`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistMoved {
    pub list_id: String,
    /// The note's new body (also saved), for the editor to show.
    pub body: String,
    pub caret: u32,
    pub count: u32,
}

/// The checklist items on the lines `start..end` touches (a caret touches its own line). `None`
/// when there are none.
pub fn checklist(body: &str, start: u32, end: u32) -> Option<Checklist> {
    let (start, end) = (start.min(end), start.max(end));
    let mut items = Vec::new();
    let mut remaining = String::with_capacity(body.len());
    let mut caret = None;
    let mut first_item_line = None;
    // Indents of the open levels, to turn indentation into depth.
    let mut indents: Vec<usize> = Vec::new();
    let mut offset = 0u32; // UTF-16 offset of the line's start in `body`
    let mut remaining_utf16 = 0u32;
    let lines: Vec<&str> = body.split('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        let len = line.encode_utf16().count() as u32;
        let last = index + 1 == lines.len();
        let touched = offset <= end && start <= offset + len;
        let item = touched.then(|| parse_item(line)).flatten();
        if let Some((indent, done, title)) = item {
            while indents.last().is_some_and(|&i| i >= indent) {
                indents.pop();
            }
            let depth = indents.len() as u32;
            indents.push(indent);
            first_item_line.get_or_insert(index);
            caret.get_or_insert(remaining_utf16);
            if !title.is_empty() {
                items.push(ChecklistItem { title, done, depth });
            }
        } else {
            remaining.push_str(line);
            remaining_utf16 += len;
            if !last {
                remaining.push('\n');
                remaining_utf16 += 1;
            }
        }
        offset += len + 1;
    }
    if items.is_empty() {
        return None;
    }
    // Taking out the last line leaves the newline before it dangling.
    if remaining.ends_with('\n') && !body.ends_with('\n') {
        remaining.pop();
    }
    let caret = caret.unwrap_or(0).min(remaining.encode_utf16().count() as u32);
    Some(Checklist {
        suggested_title: suggested_title(&lines, first_item_line.unwrap_or(0)),
        items,
        remaining,
        caret,
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
    /// Moves the checklist items `start..end` touches in `body` (what the editor shows for note
    /// `note_id`) into list `list_id` ([`crate::INBOX`] works too), or into a new list titled
    /// `new_list` when `list_id` is `None`. They keep their order, go first in the list, and the
    /// note is saved without them, all in one undo step.
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
            let meta = self.notes.get_meta(note)?;
            meta.ensure_mergeable_text(BODY)?
                .update(&checklist.remaining, Default::default())
                .map_err(|e| Error::Invalid(format!("note body: {e}")))?;
            meta.insert(UPDATED_AT, now)?;
            Ok(ChecklistMoved {
                list_id,
                body: checklist.remaining.clone(),
                caret: checklist.caret,
                count: checklist.items.len() as u32,
            })
        })
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
    fn takes_the_touched_items_and_keeps_the_rest() {
        let body = "Week\n## Groceries\n- [ ] milk\n  - [x] oat\n- [ ] eggs\nnot an item\n- [ ] later";
        // From inside "milk" to inside "not an item": three items, the plain line stays.
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
        assert_eq!(c.remaining, "Week\n## Groceries\nnot an item\n- [ ] later");
        assert_eq!(c.caret, "Week\n## Groceries\n".len() as u32);
    }

    #[test]
    fn a_caret_takes_its_line_and_titles_fall_back_to_the_note() {
        let body = "# Trip\nbring:\n* [ ] passport";
        let c = checklist(body, body.len() as u32, body.len() as u32).unwrap();
        assert_eq!(c.items, vec![item("passport", false, 0)]);
        assert_eq!(c.suggested_title, "Trip");
        assert_eq!(c.remaining, "# Trip\nbring:");
        assert!(checklist(body, 0, 3).is_none(), "the title line is no item");
        assert!(
            checklist("- [x]done", 0, 1).is_none(),
            "a box needs a space after it"
        );
    }

    #[test]
    fn offsets_are_utf16() {
        let body = "😀😀\n- [ ] tea";
        let start = "😀😀\n".encode_utf16().count() as u32;
        let c = checklist(body, start, start).unwrap();
        assert_eq!(c.items[0].title, "tea");
        assert_eq!(c.remaining, "😀😀");
    }
}
