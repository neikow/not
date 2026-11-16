# 0014 — Tags and smart lists replace lists; start dates, deadlines, Someday

Status: accepted (2026-11-16)

## Context
Tasks lived in lists (tree nodes), with one date ("due"). The user wants lists gone: tasks carry
`#tags`, and lists become smart lists — saved filters the user writes from tags, dates and
priority. They also want Things-style planning: a start date ("when I'll do it", hiding the task
until then) separate from a deadline, a Someday bucket, repeats counted from completion, "last
Friday of the month" repeats, and a quick way to deal with tasks that slipped past their day.

## Decision

### Tasks
- All top-level tasks sit at the root of the `tasks` tree; subtasks stay children of tasks. No
  new list nodes are created.
- Meta keys: `due` now means the **start date** (existing due dates become start dates, as the
  user chose); `due_time` is its time; new `deadline` (`YYYY-MM-DD`), `someday` (`true`, cleared
  by setting a start date and vice versa), `tags` (mergeable map, tag → `true`).
- Tags are lowercase, without `#`, spaces turned into `-`. A subtask's effective tags include its
  ancestors' (it shows under its parent in a tag's view).
- A repeat is anchored on the start date, else the deadline. Completing moves both by the same
  number of days. `X-FROM=COMPLETION` in the rule counts from the completion day (E). Monthly
  rules take an ordinal weekday: `FREQ=MONTHLY;BYDAY=-1FR` (F).

### Views (smart lists)
- Built in: **Inbox** (untagged), **Today** (start or deadline on/before today), **Upcoming**
  (start or deadline after today), **Someday**. Ids `inbox`, `today`, `upcoming`, `someday`.
- Saved views live in a root map `views`: id → mergeable map `{title, query, position,
  deleted_at?}`. Sort per view stays in `task_sort` (view id → key).
- Query language (whitespace = AND, `or` between groups, `-` negates a term): `#tag`, `!1`/`!2`/
  `!3` (high/medium/low, as in quick add), `!` (any priority), `today`, `tomorrow`, `week`,
  `overdue`, `upcoming`, `someday`, `nodate`, `deadline`, `repeating`, `tagged`, `untagged`,
  `all`, and any other word or `"quoted phrase"` matched in title and notes.
- Unless a query names `upcoming`, `tomorrow`, `week`, `someday` or `all`, it only shows tasks
  that are available: no future start date and not Someday (A: hidden until the start date).
- Adding a task in a view gives it what makes it match: the first group's `#tags`, priority,
  `today`/`tomorrow` as start date, `someday`.
- **Order per view** (user's pick): root map `view_order`: view id → mergeable map task id →
  number. Unordered tasks come first (newest first), then ordered ones. Dragging writes the
  shown order and, in a sorted view, switches it to Manual in the same undo step. Subtasks keep
  one order under their parent (tree order).

### Migration
On open, each live list node becomes: tag `slug(title)` on its tasks, its top-level tasks moved
to the root, a saved view with the **list's id** (so share links and widgets keep working),
title and query `#slug`, its sort, and its order in `view_order`; the list node is soft-deleted.
It commits with origin `not-migrate` (not undoable). Every id and key is derived from the list,
so devices and the server replica migrating concurrently merge into the same result.

### Overdue triage (H)
Today flags tasks whose start date is before today; `Store::reschedule(ids, date)` moves any set
of tasks in one undo step ("Move to today").

## Consequences
- Lists, `create_list`, `move_list`, the `list` table and `list_id` are gone; share links point
  at a view id (`list_id` stays as the wire name).
- Views are computed in Rust over the task table; fine for a single user's few thousand tasks.
- Old app versions see migrated tasks as Inbox tasks until updated.
