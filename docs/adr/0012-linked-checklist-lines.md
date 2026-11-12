# 0012 — Checklist lines linked to tasks with text marks

Status: accepted (2026-11-12); supersedes 0011

## Context
A checklist made into tasks should stay ordinary checklist lines in the note (tick, type, indent
like any markdown checkbox) while staying in step with the tasks. Two earlier tries fell short:
hidden ` <!--task:ID-->` markup got cut by keyboards and autocorrect, and the atomic task block
(0011) didn't edit like text and drew badly (caret through the block, heights out of step).

## Decision
- **The link is a Loro rich-text mark** `task` = task id on the line's characters
  (`ExpandType::None`), not text. Editors see plain markdown; the mark rides on the characters
  that remain through edits, merges and undo.
- **A line is linked** when it is a checklist item carrying the mark of a live task; when two lines
  claim one task (a split line), the first wins. Marks are never removed: a deleted task's line
  reads as plain, and undoing the delete links it again.
- **Note → tasks:** `set_note_body` compares linked lines before and after and ticks, reopens or
  renames their tasks in the same commit. An emptied title doesn't rename.
- **Tasks → note:** every local transaction ends by catching linked notes up — ticks, titles, and
  new open subtasks of linked tasks (plus new tasks of a list made from the note) come in as linked
  lines where they belong. Each note keeps `task_seen` (tasks it has shown), so a line the user
  deleted doesn't come back, and `task_lists` (lists made from it).
- **Not on import:** the device that made a change caught the note up in the same commit; doing it
  again on import could put a line in twice.
- **Linking:** "Make tasks" on checklist items (already linked lines aren't offered); an item
  nested under a linked line becomes a subtask of that line's task. Items typed into a linked
  checklist (Return at a linked line, a line put in between, a line split in two) become tasks on
  save: after their nearest linked sibling above, else before the one below, else as a subtask of
  their linked parent. Empty items wait for a title; lines still marked for a deleted task stay
  plain; blank and indented lines don't end a checklist, other text does.
- **Two marks on one line** (edits that join or split lines): the task marking most of the title
  owns the line; making a new task of a line replaces its old marks.
- **Nesting:** indenting a linked line under another linked line makes its task a subtask there
  (after its nearest linked sibling above); outdenting takes it out (after its sibling above, else
  after the task it was under). Only lines whose nesting changed in that save move; nesting under
  a plain item moves nothing.
- **Saving is line-aware:** a plain character diff can line up a new line with the inside of a
  linked one, and text inserted inside a mark takes it (a line typed above a task would take that
  task over). Bodies are saved line by line instead: equal lines stay, changed lines are paired by
  likeness (same item title, else shared start and end) and edited in place, extra lines go in
  whole at line boundaries, unmarked. A line moved within the note (reordered, cut and pasted)
  takes back the link of the task with its title whose line went in the same save.
- Notes from 0011 (block lines) and the marker version are converted on `open_note`, in a commit
  undo skips. Editors draw linked boxes in the accent.

## Consequences
- Redo after undoing "Make tasks" recreates the tasks as new nodes (Loro gives undone tree nodes
  new ids), so the lines come back unlinked and can be made into tasks again.
- An editor saving stale text (inside its ~400 ms save delay) can push an old tick or title over a
  change just made elsewhere.
- Completed tasks added later, and tasks moved around in the list, don't rearrange the note; nor
  does reordering lines in the note reorder the list (nesting changes do move tasks).
