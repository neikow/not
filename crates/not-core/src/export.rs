//! Getting everything out: a readable JSON export (for keeping or moving elsewhere) and a backup
//! file (a Loro snapshot) that can be merged back into any replica.

use loro::ExportMode;
use serde_json::{Value, json};

use crate::date::Date;
use crate::store::{INBOX, Store};
use crate::{Error, Period, Result};

/// Marks a backup file, so a random file isn't fed to Loro.
const BACKUP_MAGIC: &[u8] = b"not-backup-1\n";

impl Store {
    /// Everything as indented JSON: lists with their tasks (completed and archived ones too),
    /// habits with every day's check-ins up to `today`, folders and notes with their markdown, and
    /// workouts with their sets as typed.
    pub fn export_json(&self, today: Date) -> Result<String> {
        let lists = self
            .lists()?
            .into_iter()
            .map(|list| {
                let tasks: Vec<Value> = self
                    .tasks(&list.id)?
                    .into_iter()
                    .map(|t| {
                        json!({
                            "id": t.id,
                            "parent_id": t.parent_id,
                            "title": t.title,
                            "notes": t.notes,
                            "due": t.due,
                            "due_time": t.due_time.map(|m| format!("{:02}:{:02}", m / 60, m % 60)),
                            "repeat": t.repeat,
                            "priority": t.priority,
                            "completed_at": t.completed_at,
                        })
                    })
                    .collect();
                Ok(json!({ "id": list.id, "title": list.title, "inbox": list.id == INBOX, "tasks": tasks }))
            })
            .collect::<Result<Vec<_>>>()?;

        let habits = self
            .habits(&today.to_string())?
            .into_iter()
            .map(|h| {
                let days: Vec<Value> = self
                    .habit_days(&h.habit.id, "2000-01-01", &today.to_string())?
                    .into_iter()
                    .filter(|d| d.count > 0)
                    .map(|d| json!({ "date": d.date, "count": d.count }))
                    .collect();
                let period = match h.habit.period {
                    Period::Day => "day",
                    Period::Week => "week",
                    Period::Month => "month",
                };
                Ok(json!({
                    "id": h.habit.id,
                    "title": h.habit.title,
                    "goal": h.habit.goal,
                    "period": period,
                    "weekdays": h.habit.days,
                    "check_ins": days,
                }))
            })
            .collect::<Result<Vec<_>>>()?;

        let folders: Vec<Value> = self
            .folders()?
            .into_iter()
            .map(|f| json!({ "id": f.id, "parent_id": f.parent_id, "title": f.title }))
            .collect();
        let notes = self
            .notes(None)?
            .into_iter()
            .map(|n| {
                Ok(json!({
                    "id": n.id,
                    "folder_id": n.folder_id,
                    "title": n.title,
                    "pinned": n.pinned,
                    "updated_at": n.updated_at,
                    "markdown": self.note_body(&n.id)?,
                }))
            })
            .collect::<Result<Vec<_>>>()?;

        let workouts = self
            .workouts()?
            .into_iter()
            .map(|w| {
                let w = self.workout(&w.id)?;
                let exercises: Vec<Value> = w
                    .exercises
                    .into_iter()
                    .map(|e| json!({ "name": e.name, "sets": e.sets }))
                    .collect();
                Ok(json!({ "id": w.id, "date": w.date, "title": w.title, "exercises": exercises }))
            })
            .collect::<Result<Vec<_>>>()?;

        let out = json!({
            "format": "not-export",
            "version": 1,
            "exported_on": today.to_string(),
            "lists": lists,
            "habits": habits,
            "folders": folders,
            "notes": notes,
            "workouts": workouts,
        });
        serde_json::to_string_pretty(&out).map_err(|e| Error::Invalid(format!("export: {e}")))
    }

    /// The whole workspace, history included, as one file. [`Store::restore_backup`] merges it into
    /// any replica, e.g. a fresh device after the server was lost.
    pub fn backup(&self) -> Result<Vec<u8>> {
        let _inner = self.lock();
        let mut out = BACKUP_MAGIC.to_vec();
        out.extend(self.doc.export(ExportMode::Snapshot)?);
        Ok(out)
    }

    /// Merges a [`Store::backup`] file in. Nothing is lost on either side: it's the same merge as a
    /// sync, so items deleted since the backup stay deleted. The merged history syncs on as usual.
    pub fn restore_backup(&self, bytes: &[u8]) -> Result<()> {
        let snapshot = bytes
            .strip_prefix(BACKUP_MAGIC)
            .ok_or_else(|| Error::Invalid("not a backup file from this app".into()))?;
        self.import_local(snapshot)
    }
}
