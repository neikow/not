# 0008 — Notes live in the workspace doc, edited as styled markdown

Status: accepted (2026-11-02)

## Context
The architecture planned one Loro doc per note (`note:<id>`), loaded lazily, so the workspace stays
small. Notes also need rich-text editing on both apps without a second document model.

## Decision
- **Bodies in the workspace doc for now.** The `notes` tree holds folders and notes; a note's body is a
  mergeable `LoroText` in its meta map. One doc keeps sync (one version vector), undo and persistence
  unchanged. A single user's notes are small; if the doc grows too big, bodies can move to per-note
  docs behind the same `Store` API.
- **Markdown is the format; the editors style it in place.** Text stays plain markdown (portable,
  merges as text); the core's `markdown::spans` returns UTF-16 ranges to style (headings, emphasis,
  code, checklists, dimmed markers), so the Mac `NSTextView` and the Android `BasicTextField` render
  the same way without a rich-text model to convert to and from.
- **Title and preview are derived** from the first lines of the body (no separate title field), and
  the read model keeps them with the body so lists and search never touch Loro.
- Folders nest (tree moves rule out cycles); notes sort pinned first, then by last edit.
- Search is `LIKE` over the read model for now; FTS5 later if it gets slow.

## Consequences
- Every note body syncs to every device eagerly, and opening the app loads all of them.
- Editors save after a short pause; a synced edit to the open note is taken only when there are no
  unsaved local changes, otherwise the next save wins for that window (rare for a single user).
