# 0013 — Checklists stay in notes; "Make tasks" copies

Status: accepted (2026-11-12); supersedes 0012

## Context
Three ways of keeping note checklists and tasks in step (hidden markers, a live block, linked
lines via text marks) each brought edge cases (keyboards cutting markup, block layout, diffs
moving links between lines) for a feature the user decided they don't need.

## Decision
- Note checkboxes belong to the note only: ticking, editing or nesting them never touches tasks,
  and task changes never touch notes.
- "Make tasks" stays as a one-off copy: the selected items become tasks (ticked ones completed,
  indented ones subtasks) in a picked list, the Inbox or a new list; the lines stay unchanged.
  Making them again makes another copy.
- Ticking a box leaves the item where it is (Android no longer sinks ticked items to the end).
- Notes from earlier versions open as plain checklists (`open_note`): task block lines become the
  lines of the tasks they showed, per-line markers go, in a commit undo skips. `task` text marks
  left by 0012 are ignored; the style stays registered so Loro handles text typed next to them.

## Consequences
- No link means no drift handling, no line-aware diffing, no per-note bookkeeping.
- A note and the tasks made from it can disagree; that's expected.
