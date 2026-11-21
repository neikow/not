package dev.lysen.not

import dev.lysen.not.core.Habit
import dev.lysen.not.core.HabitPeriod
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.TimedRule

/*
 * What a habit's goal period means for the day shown, shared by the screen, its badge and the
 * widget. Daily habits count per scheduled day; weekly and monthly ones on any day, toward a total.
 */

val Habit.isDaily: Boolean get() = period == HabitPeriod.DAY

/**
 * On the day's to-do: a daily habit on a scheduled day, or a weekly or monthly one that needs that
 * day to still make its goal, or got a check-in that day.
 */
val HabitSummary.onToday: Boolean get() = if (habit.isDaily) due else due || count > 0u

/** Done as far as the day goes: the day's goal met, or for weekly and monthly ones, a check-in. */
val HabitSummary.doneToday: Boolean get() = when {
    // Timed: the core judges the day (enough on, or still within the allowance off).
    habit.timed != null -> done
    habit.isDaily -> count >= habit.goal
    else -> count > 0u || done
}

/** On the day's to-do and not done yet: what badges count. */
val HabitSummary.pending: Boolean get() = onToday && !doneToday

/**
 * [doneToday] as of [now], for a summary loaded at [loadedAt]: a timed habit's running stretch
 * keeps counting between reloads, so it can meet its goal (or use up its allowance) meanwhile.
 */
fun HabitSummary.doneAt(now: Long, loadedAt: Long, isToday: Boolean = true): Boolean =
    timedLive(now, loadedAt, isToday)?.met ?: doneToday

/** [pending] as of [now]; see [doneAt]. */
fun HabitSummary.pendingAt(now: Long, loadedAt: Long): Boolean = onToday && !doneAt(now, loadedAt)

/** Check-ins that fill one day's dot: the goal for daily habits, one for the others. */
val Habit.dayGoal: UInt get() = if (isDaily) goal else 1u

/** "Once a day", "3 times a week", "Twice a month". */
fun goalText(goal: UInt, period: HabitPeriod): String {
    val times = when (goal) {
        1u -> "Once"
        2u -> "Twice"
        else -> "$goal times"
    }
    return "$times a ${periodName(period)}"
}

fun periodName(period: HabitPeriod): String = when (period) {
    HabitPeriod.DAY -> "day"
    HabitPeriod.WEEK -> "week"
    HabitPeriod.MONTH -> "month"
}

/** "4 days", "1 week", "2 months": a streak in the habit's periods. */
fun streakText(n: UInt, period: HabitPeriod): String = "$n ${periodName(period)}${if (n == 1u) "" else "s"}"

// ---- timed habits ----

fun nowMs(): Long = System.currentTimeMillis()

/** The device's offset from UTC at [at], in minutes (UTC+1 = 60), as the core places local days. */
fun utcOffsetMinutes(at: Long = nowMs()): Int =
    java.time.ZoneId.systemDefault().rules.getOffset(java.time.Instant.ofEpochMilli(at)).totalSeconds / 60

/** Habits as of [date], timed ones up to now in the device's time zone. */
fun dev.lysen.not.core.NotStore.habitsNow(date: String): List<HabitSummary> = habits(date, nowMs(), utcOffsetMinutes())

val Habit.isTimed: Boolean get() = timed != null

/** "45m", "1h 05m", "22h": whole minutes, rounded down. */
fun durationText(ms: Long): String {
    val minutes = (ms.coerceAtLeast(0) / 60_000).toInt()
    val h = minutes / 60
    val m = minutes % 60
    return when {
        h == 0 -> "${m}m"
        m == 0 -> "${h}h"
        else -> "${h}h %02dm".format(m)
    }
}

/** "14:20" in the device's time zone and clock format. */
fun clockText(ms: Long): String =
    java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(ms))

/** The button that changes a timed habit's state: Pause/Resume when it's normally on, else Start/Stop. */
fun switchLabel(rule: TimedRule, on: Boolean): String = when {
    rule.usualOn && on -> "Pause"
    rule.usualOn -> "Resume"
    on -> "Stop"
    else -> "Start"
}

/** What the state is called: On/Paused when it's normally on, Running/Stopped otherwise. */
fun stateName(rule: TimedRule, on: Boolean): String = when {
    rule.usualOn && on -> "On"
    rule.usualOn -> "Paused"
    on -> "Running"
    else -> "Stopped"
}

/** "At least 22h on" / "At most 2h off". */
fun timedGoalText(rule: TimedRule, minutes: UInt): String =
    if (rule.atMostOff) "At most ${durationText(minutes.toLong() * 60_000)} off" else "${durationText(minutes.toLong() * 60_000)} on"

/**
 * A timed habit as of [now], from a summary loaded at [loadedAt]: the running stretch keeps
 * counting between reloads (only for today).
 */
data class TimedLive(
    val tracked: Boolean,
    val on: Boolean,
    val since: Long?,
    val onMs: Long,
    val offMs: Long,
    /** Minutes counted toward the goal, in millis: on, or off for "at most" habits. */
    val countedMs: Long,
    val goalMs: Long,
    val atMostOff: Boolean,
) {
    /** At least: the goal is reached. At most: still within the allowance. */
    val met: Boolean get() = if (atMostOff) countedMs <= goalMs else countedMs >= goalMs

    /** At least: time still to go. At most: allowance left (negative when over). */
    val leftMs: Long get() = goalMs - countedMs

    /** 0..1 for the ring: done so far (at least), or allowance left (at most). */
    val ringFraction: Float
        get() = if (goalMs <= 0) 0f else if (atMostOff) (leftMs.toFloat() / goalMs).coerceIn(0f, 1f) else (countedMs.toFloat() / goalMs).coerceIn(0f, 1f)
}

fun HabitSummary.timedLive(now: Long, loadedAt: Long, isToday: Boolean = true): TimedLive? {
    val rule = habit.timed ?: return null
    val state = timed ?: return null
    val tracked = state.since != null
    // The day ends at midnight; a stale summary from yesterday doesn't keep counting.
    val extra = if (isToday && tracked) (now - loadedAt).coerceAtLeast(0) else 0L
    val onMs = state.onMs + if (state.on) extra else 0
    val offMs = state.offMs + if (!state.on) extra else 0
    return TimedLive(
        tracked = tracked,
        on = state.on,
        since = state.since,
        onMs = onMs,
        offMs = offMs,
        countedMs = if (rule.atMostOff) offMs else onMs,
        goalMs = habit.goal.toLong() * 60_000,
        atMostOff = rule.atMostOff,
    )
}

/**
 * One line about where a timed habit stands: "Paused 25m · 1h 35m left", "On · 18h 40m today",
 * "Running 12m · 18m to go", "Not started".
 */
fun timedStatus(rule: TimedRule, live: TimedLive, now: Long): String {
    if (!live.tracked) return "Not started"
    val elapsed = now - (live.since ?: now)
    val stretch = if (elapsed < 60_000) "just now" else durationText(elapsed)
    val unusual = live.on != rule.usualOn
    val head = if (unusual) "${stateName(rule, live.on)} $stretch" else stateName(rule, live.on)
    val tail = when {
        live.atMostOff && live.leftMs >= 0 -> "${durationText(live.leftMs)} left"
        live.atMostOff -> "${durationText(-live.leftMs)} over"
        live.met -> "${durationText(live.countedMs)} today ✓"
        else -> "${durationText(live.leftMs)} to go"
    }
    return "$head · $tail"
}

/** What a timed habit's live notification counts, chosen per habit on this phone. */
enum class LiveCounter(val label: String) {
    /** Up, from when the habit left its usual state. */
    Stretch("This stretch"),

    /** Up, the time counted toward the goal today. */
    Today("Today"),

    /** Down, to the goal or to the end of the allowance; then up, the time over. */
    Left("Time left"),
}

/** Out of its usual state, the habit counts toward the goal: [LiveCounter.Today] and [LiveCounter.Left] move then. */
val TimedRule.unusualCounts: Boolean get() = !usualOn != atMostOff

/** A notification chronometer: [base] for `setWhen`, counting [down] or up. */
data class Chrono(val base: Long, val down: Boolean)

/**
 * The chronometer showing [counter] as of [now]. When the running stretch doesn't count toward the
 * goal, only the stretch moves, so that's what it shows.
 */
fun TimedLive.chrono(counter: LiveCounter, now: Long): Chrono {
    val counts = tracked && on != atMostOff
    return when {
        !counts || counter == LiveCounter.Stretch -> Chrono(since ?: now, down = false)
        counter == LiveCounter.Today -> Chrono(now - countedMs, down = false)
        leftMs > 0 -> Chrono(now + leftMs, down = true)
        // Over the allowance: counts the time over, up.
        else -> Chrono(now + leftMs, down = false)
    }
}

/** Local midnight ending the day that contains [at]. */
fun endOfDay(at: Long): Long {
    val zone = java.time.ZoneId.systemDefault()
    return java.time.Instant.ofEpochMilli(at).atZone(zone).toLocalDate().plusDays(1).atStartOfDay(zone).toInstant().toEpochMilli()
}

/**
 * Today's goal can't be missed any more: time on reached for "at least" habits; for "at most",
 * even staying off until midnight ([dayEnd]) would keep within the allowance.
 */
fun TimedLive.secured(now: Long, dayEnd: Long = endOfDay(now)): Boolean =
    tracked && if (atMostOff) countedMs + (dayEnd - now).coerceAtLeast(0) <= goalMs else countedMs >= goalMs

/**
 * When today's goal becomes [secured] if the habit stays in its current state ([on]): the moment
 * time on reaches the goal, or for "at most" the moment what's left of the day fits in the
 * allowance. Null when it already is, or can't become safe that way.
 */
fun TimedLive.securedAt(now: Long, dayEnd: Long = endOfDay(now)): Long? {
    if (!tracked || secured(now, dayEnd)) return null
    val counting = on != atMostOff
    val at = when {
        !atMostOff && counting -> now + leftMs
        atMostOff && !counting -> dayEnd - leftMs
        else -> return null
    }
    return at.takeIf { it in (now + 1)..dayEnd }
}
