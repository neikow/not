# 0019 — Import from Todoist and TickTick

Status: accepted (2026-11-21)

## Context
People coming from the stores bring years of tasks from another app. Typing them again is the
biggest reason not to switch. Todoist exports a CSV per project; TickTick exports one backup CSV
of everything. Things 3 has no export file (only AppleScript on the Mac).

## Decision
- One core command, `Store::import_tasks(text, file_name, today, utc_offset_minutes)`, tells the
  two formats apart by their header row and writes everything as one undo step. A small CSV reader
  lives in the module (no new dependency).
- Mapping: a Todoist project (from the file name; Inbox stays untagged) or a TickTick list becomes
  a tag; labels and tags stay tags; priorities map to !1/!2/!3; descriptions, content and Todoist
  comments go to notes; Todoist INDENT and TickTick parentId nest subtasks, which drop the tags
  their parent already gives them.
- Dates: Todoist's are as typed ("every mon", "Nov 25 10:00") and go through the quick-add parser;
  one it can't read is kept in the notes rather than lost. TickTick's are UTC timestamps, placed at
  the device's offset, with RRULE repeats. Its start date (else due date) is our start date.
- Completed tasks are left out and counted: they're history, and the Logbook would fill with them.
- Apps: Settings → Import, export and backup (Android), Settings → General → Import (Mac), and a
  first-run choice.

## Consequences
- Things 3 isn't covered; a Mac-only AppleScript reader could come later.
- The device's current offset is used for every TickTick time, so a task across a daylight saving
  change can land an hour off. All-day tasks, the common case, are unaffected unless that hour
  crosses midnight.
