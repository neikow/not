package dev.lysen.not

import dev.lysen.not.core.Habit
import dev.lysen.not.core.HabitPeriod
import dev.lysen.not.core.HabitSummary

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
val HabitSummary.doneToday: Boolean get() = if (habit.isDaily) count >= habit.goal else count > 0u || done

/** On the day's to-do and not done yet: what badges count. */
val HabitSummary.pending: Boolean get() = onToday && !doneToday

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
