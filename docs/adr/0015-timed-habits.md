# 0015 — Timed habits

Status: accepted (2026-11-16)

## Context
Some habits are about time, not check-ins: wearing something 22 hours a day, reading 30 minutes,
staying under 2 hours of breaks. The user wants to say when it came off or went back on (often
after the fact), see the day's total at a glance on the home and lock screens, and be reminded
when it has been off too long.

## Decision
- A habit kind, chosen at creation: **timed**. `goal` is minutes per scheduled day; timed habits
  are always daily and days split at local midnight.
- Per habit (user's picks): the goal is **at least N on** or **at most N off**; the **usual
  state** is on (worded Pause/Resume, the time off is what's watched) or off (Start/Stop, the
  time on); a **nudge** after N minutes out of the usual state.
- Switches live in the habit's meta as a `switches` map, created with the habit: switch id →
  Unix millis, negated for "off". One key per switch, so offline switches from several devices
  all survive and are read in time order (ties by key). Time before the first switch is
  untracked: neither on nor off.
- One command, `set_habit_state(on, from, to?)`: "it was on/off from A until B, or from A on".
  A tap, backdating ("off since 15 min ago") and timeline fixes are all this. Switches inside
  the range are replaced, the state after B carries on, redundant switches at the edges aren't
  written. One undo step.
- Queries take a clock (now + UTC offset) from the app; the core has no time zone. A day is
  24 hours at that offset, so on daylight-saving days an hour lands on the neighbouring day.
- Live notification (lock screen / Now Bar) is opt-in per habit and per device, shown while out
  of the usual state. The nudge setting syncs; reminders fire only on Android for now.
- Mac: a row with the day's total and a toggle; no timeline editing there yet.

## Consequences
- Counts, streaks, week dots and the history grid reuse the habit summary: for timed habits
  `count`/`progress`/`week` are the minutes that count (on, or off for "at most").
- Two devices editing the same stretch offline merge to the union of their switches in time
  order: deterministic, though not always what either meant; the timeline makes it easy to fix.
- A timed habit can't become a count habit or the reverse; delete and recreate instead.
- Switches grow by a few per day and are re-projected with the habits; fine for years of use,
  revisit with compaction if it ever shows.
