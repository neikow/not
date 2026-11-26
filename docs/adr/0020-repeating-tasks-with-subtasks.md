# 0020. Repeating tasks with subtasks

## Context

Completing a repeating task logs the occurrence as a completed copy and moves the task on, with
its subtasks reopened. The copy had no subtasks, so the log lost what was done. Subtasks that
repeat themselves log their own copies under the parent; when the parent moved on, those copies
were reopened with the rest and piled up as open duplicates. Dated subtasks kept their old dates
and showed as overdue in the next round.

## Decision

When a repeating task moves on (`complete_in`):

- the logged copy gets a copy of every live subtask, at any depth, done or not, as it was;
- copies logged by repeating subtasks (marked with the meta `occurrence_of`, the id of the task
  they were done from) move under the parent's logged copy instead of being reopened;
- the subtasks reopen, and those with dates (not repeating ones) move by as many days as the
  parent did.

All of it is one transaction, so one undo step.

## Consequences

- The log shows each round with its checklist. Copies made before this change have no
  `occurrence_of`, so they still reopen once with their parent.
- Two devices completing the same repeating task concurrently still both move it on (an
  occurrence may be skipped); unchanged.
