package dev.lysen.not

import dev.lysen.not.core.Habit
import dev.lysen.not.core.HabitPeriod
import dev.lysen.not.core.HabitSummary
import java.time.LocalDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Plain rules the screens, badges, widgets and alarms share; nothing here calls the core. */
class RulesTest {
    private fun summary(period: HabitPeriod, goal: UInt, count: UInt, due: Boolean, done: Boolean = false) =
        HabitSummary(
            habit = Habit("1@1", "Run", goal, 0x7Fu, period, null),
            count = count,
            progress = count,
            done = done,
            due = due,
            streak = 0u,
            bestStreak = 0u,
            week = emptyList(),
            timed = null,
        )

    @Test
    fun dailyHabitsCountTowardTheDaysGoal() {
        val twice = summary(HabitPeriod.DAY, goal = 2u, count = 1u, due = true)
        assertTrue(twice.onToday)
        assertFalse(twice.doneToday)
        assertTrue(twice.pending)
        assertTrue(summary(HabitPeriod.DAY, goal = 2u, count = 2u, due = true).doneToday)
        // Not scheduled today: off the to-do, so no badge, even with no check-in.
        val off = summary(HabitPeriod.DAY, goal = 1u, count = 0u, due = false)
        assertFalse(off.onToday)
        assertFalse(off.pending)
        assertEquals(2u, twice.habit.dayGoal)
    }

    @Test
    fun weeklyHabitsAreDoneForTheDayWithOneCheckIn() {
        val week = summary(HabitPeriod.WEEK, goal = 3u, count = 1u, due = false)
        assertTrue("a check-in today keeps it on today", week.onToday)
        assertTrue(week.doneToday)
        assertFalse(week.pending)
        val needed = summary(HabitPeriod.WEEK, goal = 3u, count = 0u, due = true)
        assertTrue(needed.pending)
        assertTrue(
            "the period's goal is met",
            summary(HabitPeriod.MONTH, goal = 3u, count = 0u, due = false, done = true).doneToday,
        )
        assertEquals(1u, week.habit.dayGoal)
    }

    @Test
    fun habitTexts() {
        assertEquals("Once a day", goalText(1u, HabitPeriod.DAY))
        assertEquals("Twice a week", goalText(2u, HabitPeriod.WEEK))
        assertEquals("5 times a month", goalText(5u, HabitPeriod.MONTH))
        assertEquals("1 week", streakText(1u, HabitPeriod.WEEK))
        assertEquals("4 days", streakText(4u, HabitPeriod.DAY))
        assertEquals("0 months", streakText(0u, HabitPeriod.MONTH))
    }

    @Test
    fun dailyRemindersFireAtTheNextOccurrence() {
        val morning = LocalDateTime.of(2026, 11, 16, 8, 59)
        assertEquals(LocalDateTime.of(2026, 11, 16, 9, 0), Reminders.next(morning, 9 * 60))
        // At or after the time: tomorrow, so it never fires twice.
        assertEquals(LocalDateTime.of(2026, 11, 17, 9, 0), Reminders.next(morning.plusMinutes(1), 9 * 60))
        assertEquals(LocalDateTime.of(2026, 12, 1, 0, 30), Reminders.next(LocalDateTime.of(2026, 11, 30, 23, 0), 30))
    }

    @Test
    fun weeklyReviewIsSundayEvening() {
        // Monday 16 November 2026.
        assertEquals(LocalDateTime.of(2026, 11, 22, 18, 0), Reminders.nextSunday(LocalDateTime.of(2026, 11, 16, 10, 0)))
        // Sunday before 18:00: today; after: next week.
        assertEquals(LocalDateTime.of(2026, 11, 22, 18, 0), Reminders.nextSunday(LocalDateTime.of(2026, 11, 22, 17, 59)))
        assertEquals(LocalDateTime.of(2026, 11, 29, 18, 0), Reminders.nextSunday(LocalDateTime.of(2026, 11, 22, 18, 0)))
    }

    @Test
    fun taskLabels() {
        assertEquals("9:00", clockLabel(540u))
        assertEquals("18:05", clockLabel(1085u))
        assertEquals("0:00", clockLabel(0u))
        assertEquals("High", priorityName(3u))
        assertEquals("Low", priorityName(1u))
        assertEquals("None", priorityName(0u))
        assertEquals(null, scheduleLabel(null, 540u, "FREQ=DAILY"))
    }

    @Test
    fun theDaysCountHasEveryHabitButRestDays() {
        // Weekly habits count every day, needed that day or not.
        assertTrue(summary(HabitPeriod.WEEK, 2u, count = 0u, due = false).inDayCount)
        // A daily habit off its schedule counts only once checked in.
        assertFalse(summary(HabitPeriod.DAY, 1u, count = 0u, due = false).inDayCount)
        assertTrue(summary(HabitPeriod.DAY, 1u, count = 1u, due = false).inDayCount)
        assertTrue(summary(HabitPeriod.DAY, 1u, count = 0u, due = true).inDayCount)
    }
}
