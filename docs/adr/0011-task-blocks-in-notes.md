# 0011 — Checklists made into tasks show as a live task block

Status: accepted (2026-11-12); replaces per-line task links (briefly shipped the same day)

## Context
"Make tasks" turns a note's checklist into tasks. The user wants them to stay in the note and keep
up with the tasks: ticks, renames, new subtasks, order. A first try kept each line in the text with
a hidden ` <!--task:ID-->` marker and synced edits both ways. It was unreliable: keyboards replace,
autocorrect and delete words in ways that cut into hidden markup, and a line only knows its own
task, so subtasks added in the list had nowhere to appear.

## Decision
- **One atomic block line replaces the items:** `![](not-tasks:list:<id>)` when they went into a new
  list (the block shows the whole list, tasks added later too), `![](not-tasks:<id>,<id>)` when they
  went into an existing list or the Inbox (just those tasks, each with its subtasks).
- **The block is drawn from the tasks**, never stored as text: `Store::task_block` returns rows as
  the list's outline has them (open first, then completed in the last day), depths counted from the
  block. A list or tasks deleted (or not synced yet) read as a "Tasks deleted" block.
- **Editors treat the line like an image line:** markup hidden, caret kept out, Backspace removes it
  whole, typing after it starts a new line. Taps: the box ticks the task, the row opens it (Android:
  the task sheet over the note; Mac: the task in its list), "Add task" adds one
  (`Store::add_task_from_block`; a picked-task block takes the new id into its line).
- Making the block is one undo step with the tasks; undo restores the checklist text.
- Notes holding per-line markers from the first try read as plain checklists (markers stripped).
- Other markdown readers see an image with no picture; the owner's web pages render the rows.

## Consequences
- Task titles aren't edited as note text; they're edited where tasks are edited.
- A block of picked tasks doesn't show other tasks later added to that list.
