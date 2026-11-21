# 0017 — Empty lists show what comes later

Status: accepted (2026-11-21)

## Context
Tag pages and smart lists leave out tasks that start on a later day (ADR 0014): they show what
can be done now. A list whose tasks are all planned for later looked empty, as if they were gone,
and its empty state said "it gets what this list shows", which explains nothing.

## Decision
- With nothing open, a list shows a **Later** section: the tasks it leaves out only because
  their start day hasn't come (not Someday), soonest first, a subtask with its parent. The core
  answers it (`Store::view_later`); views that already show dated tasks (Today, Upcoming,
  Someday, date queries) have none.
- The empty state says what a task added there gets, in quick-add words, from the list's filter
  (`Store::view_defaults`): "Tap + to add one. It gets #work !1, so it shows here."
- On Android, task lists end with a strip of empty space, so there's always somewhere that isn't
  a row to swipe to the next list.

## Consequences
- Nothing about which tasks a list shows changes; the section is only there while it's empty.
- The Mac can use the same two core calls for its empty states.
