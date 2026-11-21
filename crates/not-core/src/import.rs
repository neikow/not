//! Bringing tasks in from other apps' exports (ADR 0019): Todoist's project CSV and TickTick's
//! backup CSV, told apart by their header row.
//!
//! - A project or list becomes a tag (the Inbox stays untagged), labels and tags stay tags.
//! - Priorities map to !1/!2/!3, notes and descriptions to notes, nesting to subtasks.
//! - Dates: Todoist writes them the way they were typed ("every mon", "Nov 25 10:00"), so they go
//!   through the quick-add parser; one it can't read is kept in the notes. TickTick writes UTC
//!   timestamps (moved to the device's offset) and RRULE repeats.
//! - Completed tasks are left out (they're history), and counted.
//!
//! Everything goes in as one undo step.

use loro::{LoroMap, LoroTree, TreeID, TreeParentId};

use crate::date::Date;
use crate::parse::{normalize_tag, parse_task};
use crate::projection::NOTES;
use crate::schedule::{Repeat, Schedule};
use crate::store::{Store, create_task_node, fill_task};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSource {
    Todoist,
    TickTick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub source: ImportSource,
    /// Tasks added, subtasks included.
    pub added: u32,
    /// Completed tasks left out.
    pub skipped_done: u32,
}

/// One task read from a file, before it's written.
#[derive(Debug, Default)]
struct Incoming {
    title: String,
    notes: String,
    schedule: Schedule,
    priority: u8,
    tags: Vec<String>,
    /// Index of the parent in the same list, for subtasks.
    parent: Option<usize>,
}

impl Store {
    /// Imports a Todoist or TickTick CSV export. `file_name` names the Todoist project (its export
    /// has one file per project: "Groceries.csv" tags its tasks #groceries); `utc_offset_minutes`
    /// places TickTick's UTC times on the device's clock.
    pub fn import_tasks(
        &self,
        text: &str,
        file_name: &str,
        today: Date,
        utc_offset_minutes: i32,
    ) -> Result<ImportReport> {
        let rows = read_csv(text.trim_start_matches('\u{feff}'));
        let header_at = |first: &str| rows.iter().position(|r| r.first().is_some_and(|c| c == first));
        let (source, tasks, skipped_done) = if let Some(h) = header_at("TYPE") {
            let (tasks, skipped) = todoist(&rows[h..], file_name, today);
            (ImportSource::Todoist, tasks, skipped)
        } else if let Some(h) = header_at("Folder Name") {
            let (tasks, skipped) = ticktick(&rows[h..], utc_offset_minutes);
            (ImportSource::TickTick, tasks, skipped)
        } else {
            return Err(Error::Invalid(
                "that file isn't a Todoist or TickTick export (CSV)".into(),
            ));
        };
        let added = tasks.len() as u32;
        self.write(|tree| write_all(tree, &tasks))?;
        Ok(ImportReport {
            source,
            added,
            skipped_done,
        })
    }
}

fn write_all(tree: &LoroTree, tasks: &[Incoming]) -> Result<()> {
    let mut ids: Vec<Option<TreeID>> = vec![None; tasks.len()];
    // New top-level tasks go first, so write them bottom-up to keep the file's order; subtasks
    // go last under their parent, so those are written top-down afterwards.
    for i in (0..tasks.len()).rev().filter(|&i| tasks[i].parent.is_none()) {
        ids[i] = Some(write_one(tree, TreeParentId::Root, &tasks[i])?);
    }
    for i in (0..tasks.len()).filter(|&i| tasks[i].parent.is_some()) {
        let parent = tasks[i].parent.and_then(|p| ids[p]);
        let at = parent.map_or(TreeParentId::Root, TreeParentId::Node);
        ids[i] = Some(write_one(tree, at, &tasks[i])?);
    }
    Ok(())
}

fn write_one(tree: &LoroTree, parent: TreeParentId, task: &Incoming) -> Result<TreeID> {
    let id = create_task_node(tree, parent)?;
    let meta: LoroMap = tree.get_meta(id)?;
    let title = if task.title.trim().is_empty() {
        "Untitled"
    } else {
        task.title.trim()
    };
    fill_task(&meta, title, &task.schedule, task.priority, &task.tags)?;
    if !task.notes.trim().is_empty() {
        meta.ensure_mergeable_text(NOTES)?
            .update(task.notes.trim(), Default::default())
            .map_err(|e| Error::Invalid(format!("notes: {e}")))?;
    }
    Ok(id)
}

// ---- Todoist -------------------------------------------------------------------------------------

/// Rows from the header on: TYPE,CONTENT,DESCRIPTION,PRIORITY,INDENT,…,DATE,…,DEADLINE.
fn todoist(rows: &[Vec<String>], file_name: &str, today: Date) -> (Vec<Incoming>, u32) {
    let col = Columns::new(&rows[0]);
    let project = project_tag(file_name);
    let mut tasks: Vec<Incoming> = Vec::new();
    // The last task at each indent, for nesting (INDENT starts at 1).
    let mut open_at: Vec<usize> = Vec::new();
    for row in &rows[1..] {
        match col.get(row, "TYPE") {
            "task" => {}
            // A comment on the task above.
            "note" => {
                if let (Some(last), text) = (tasks.last_mut(), col.get(row, "CONTENT"))
                    && !text.is_empty()
                {
                    append(&mut last.notes, text);
                }
                continue;
            }
            _ => continue,
        }
        let indent = col.get(row, "INDENT").parse::<usize>().unwrap_or(1).max(1);
        let (title, labels) = todoist_labels(col.get(row, "CONTENT"));
        let mut task = Incoming {
            title,
            notes: col.get(row, "DESCRIPTION").to_owned(),
            // Todoist's 4 is its highest (p1).
            priority: match col.get(row, "PRIORITY") {
                "4" => 3,
                "3" => 2,
                "2" => 1,
                _ => 0,
            },
            ..Incoming::default()
        };
        task.tags.extend(project.clone());
        for label in labels {
            if !task.tags.contains(&label) {
                task.tags.push(label);
            }
        }
        let date = col.get(row, "DATE");
        if !date.is_empty() {
            match parse_date_words(date, today) {
                Some(schedule) => task.schedule = schedule,
                None => append(&mut task.notes, &format!("Todoist date: {date}")),
            }
        }
        let deadline = col.get(row, "DEADLINE");
        if !deadline.is_empty() {
            match parse_date_words(deadline, today).and_then(|s| s.start) {
                Some(day) => task.schedule.deadline = Some(day),
                None => append(&mut task.notes, &format!("Todoist deadline: {deadline}")),
            }
        }
        open_at.truncate(indent - 1);
        if indent > 1 {
            task.parent = open_at.last().copied();
            // A subtask carries its parent's tags already.
            task.tags.retain(|t| Some(t) != project.as_ref());
        }
        open_at.push(tasks.len());
        tasks.push(task);
    }
    // Todoist's project export has no completed tasks.
    (tasks, 0)
}

/// "Groceries.csv" → groceries; Todoist's Inbox stays untagged.
fn project_tag(file_name: &str) -> Option<String> {
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem.strip_suffix(".csv").unwrap_or(stem);
    let tag = normalize_tag(stem);
    (!tag.is_empty() && tag != "inbox").then_some(tag)
}

/// "Buy milk @errands @home" → ("Buy milk", [errands, home]).
fn todoist_labels(content: &str) -> (String, Vec<String>) {
    let mut labels = Vec::new();
    let words: Vec<&str> = content
        .split(' ')
        .filter(|w| match w.strip_prefix('@') {
            Some(label) if !label.is_empty() => {
                labels.push(normalize_tag(label));
                false
            }
            _ => true,
        })
        .collect();
    (words.join(" ").trim().to_owned(), labels)
}

/// A date as typed ("every mon", "Nov 25 10:00"), when the quick-add parser reads all of it.
fn parse_date_words(words: &str, today: Date) -> Option<Schedule> {
    let parsed = parse_task(&format!("x {words}"), today, &[]);
    (parsed.title == "x" && (parsed.schedule.start.is_some() || parsed.schedule.deadline.is_some()))
        .then_some(parsed.schedule)
}

// ---- TickTick ------------------------------------------------------------------------------------

/// Rows from the header on: "Folder Name","List Name","Title","Kind","Tags","Content",…
fn ticktick(rows: &[Vec<String>], utc_offset_minutes: i32) -> (Vec<Incoming>, u32) {
    let col = Columns::new(&rows[0]);
    let mut tasks: Vec<Incoming> = Vec::new();
    let mut skipped = 0;
    let mut by_id: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut parents: Vec<(usize, String)> = Vec::new();
    for row in &rows[1..] {
        if col.get(row, "Title").is_empty() && col.get(row, "Content").is_empty() {
            continue;
        }
        // 0 is open; 1 completed, 2 archived.
        if col.get(row, "Status") != "0" {
            skipped += 1;
            continue;
        }
        let mut task = Incoming {
            title: col.get(row, "Title").to_owned(),
            notes: col.get(row, "Content").to_owned(),
            priority: match col.get(row, "Priority") {
                "5" => 3,
                "3" => 2,
                "1" => 1,
                _ => 0,
            },
            ..Incoming::default()
        };
        let list = normalize_tag(col.get(row, "List Name"));
        if !list.is_empty() && list != "inbox" {
            task.tags.push(list);
        }
        for tag in col.get(row, "Tags").split(',').map(normalize_tag) {
            if !tag.is_empty() && !task.tags.contains(&tag) {
                task.tags.push(tag);
            }
        }
        let all_day = col.get(row, "Is All Day") == "true";
        let start = local(col.get(row, "Start Date"), utc_offset_minutes);
        let due = local(col.get(row, "Due Date"), utc_offset_minutes);
        if let Some((day, minute)) = start.or(due) {
            task.schedule.start = Some(day);
            task.schedule.time = (!all_day).then_some(minute);
        }
        if let Ok(repeat) = Repeat::parse_rrule(col.get(row, "Repeat"))
            && task.schedule.start.is_some()
        {
            task.schedule.repeat = Some(repeat);
        }
        let id = col.get(row, "taskId");
        if !id.is_empty() {
            by_id.insert(id.to_owned(), tasks.len());
        }
        let parent = col.get(row, "parentId");
        if !parent.is_empty() {
            parents.push((tasks.len(), parent.to_owned()));
        }
        tasks.push(task);
    }
    for (child, parent) in parents {
        // A subtask whose parent was completed (and left out) stays top-level.
        if let Some(&p) = by_id.get(&parent)
            && p != child
        {
            tasks[child].parent = Some(p);
            let parent_tags = tasks[p].tags.clone();
            tasks[child].tags.retain(|t| !parent_tags.contains(t));
        }
    }
    (tasks, skipped)
}

/// "2026-11-20T23:00:00+0000" → its day and minute after midnight at `offset` minutes from UTC.
fn local(stamp: &str, offset: i32) -> Option<(Date, u32)> {
    let (date, rest) = stamp.split_once('T')?;
    let date = Date::parse(date).ok()?;
    let hour: i64 = rest.get(0..2)?.parse().ok()?;
    let minute: i64 = rest.get(3..5)?.parse().ok()?;
    // The stamp's own offset, "+0000" or "+01:00".
    let zone = rest.get(8..).unwrap_or("").replace(':', "");
    let stamp_offset = match zone.as_bytes().first() {
        Some(b'+' | b'-') if zone.len() >= 5 => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            sign * (zone[1..3].parse::<i64>().ok()? * 60 + zone[3..5].parse::<i64>().ok()?)
        }
        _ => 0,
    };
    let total = hour * 60 + minute - stamp_offset + i64::from(offset);
    let day_shift = total.div_euclid(24 * 60);
    Some((date.add_days(day_shift), total.rem_euclid(24 * 60) as u32))
}

// ---- CSV -----------------------------------------------------------------------------------------

/// Columns by header name; a missing column reads as empty.
struct Columns(Vec<String>);

impl Columns {
    fn new(header: &[String]) -> Self {
        Self(header.iter().map(|h| h.trim().to_owned()).collect())
    }

    fn get<'a>(&self, row: &'a [String], name: &str) -> &'a str {
        self.0
            .iter()
            .position(|h| h == name)
            .and_then(|i| row.get(i))
            .map_or("", |s| s.as_str())
    }
}

fn append(notes: &mut String, text: &str) {
    if !notes.is_empty() {
        notes.push_str("\n\n");
    }
    notes.push_str(text);
}

/// RFC 4180: commas, quoted fields with "" for a quote and line breaks inside, CRLF or LF.
fn read_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.is_empty()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_commas_and_line_breaks() {
        let rows = read_csv("a,\"b, c\",\"say \"\"hi\"\"\"\r\n\"two\nlines\",x\n\n");
        assert_eq!(rows, [vec!["a", "b, c", "say \"hi\""], vec!["two\nlines", "x"]]);
    }

    #[test]
    fn utc_stamps_move_to_the_device_offset() {
        let d = |s| Date::parse(s).unwrap();
        // Paris (UTC+1) all-day task: TickTick writes the local midnight in UTC.
        assert_eq!(local("2026-11-20T23:00:00+0000", 60), Some((d("2026-11-21"), 0)));
        assert_eq!(
            local("2026-11-21T09:30:00+0000", -300),
            Some((d("2026-11-21"), 270))
        );
        assert_eq!(
            local("2026-11-21T01:00:00+0000", -120),
            Some((d("2026-11-20"), 23 * 60))
        );
        assert_eq!(local("", 0), None);
    }

    #[test]
    fn project_names_and_labels_become_tags() {
        assert_eq!(
            project_tag("Downloads/Side Project.csv").as_deref(),
            Some("side-project")
        );
        assert_eq!(project_tag("Inbox.csv"), None);
        assert_eq!(
            todoist_labels("Buy milk @errands @Home"),
            (
                "Buy milk".to_owned(),
                vec!["errands".to_owned(), "home".to_owned()]
            )
        );
    }
}
