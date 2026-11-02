# 0007 — Soft delete, and undo through Loro's UndoManager

Status: accepted (2026-11-02)

## Context
The apps should forgive mistakes instead of asking "are you sure?" first (swipe or ⌘⌫ to delete, then
Undo). Undo has to work across every surface on a device (window, menu bar panel, widget, quick add)
and must never revert another device's changes.

## Decision
- **Undo lives in the core**: one `loro::UndoManager` per store, created after loading, so it holds
  this session's local commits only. Each command is one step. Remote changes are never undone; Loro
  rebases the stack over them.
- **Delete is a soft delete**: `delete` sets `deleted_at` in the node's meta instead of removing the
  tree node. Undoing a real tree delete would bring the subtree back under new node ids, breaking
  references held elsewhere (other devices' concurrent edits, open screens, widgets). Undoing a field
  restores the same nodes.
- A node is visible if neither it nor an ancestor has `deleted_at`; setting or clearing it re-projects
  the subtree.
- Task notes are a mergeable `LoroText` (`ensure_mergeable_text`), so two devices writing a task's
  first notes concurrently end up sharing one text.

## Consequences
- Deleted nodes stay in the doc. A trash view (and a purge after N days) is cheap to add later.
- On macOS ⌘Z goes to the text field being edited first, then to the store. On Android, Undo is
  offered in a snackbar after deletes.
- Undo history is per app session; it doesn't survive a restart.
