package dev.lysen.not

import dev.lysen.not.core.Habit
import dev.lysen.not.core.HabitPeriod
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.TimedRule
import dev.lysen.not.core.TimedState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Wording and live counting of timed habits; nothing here calls the core. */
class TimedRulesTest {
    private val min = 60_000L
    private val wear = TimedRule(atMostOff = true, usualOn = true, nudgeMinutes = 30u)
    private val read = TimedRule(atMostOff = false, usualOn = false, nudgeMinutes = 0u)

    private fun summary(rule: TimedRule, goal: UInt, on: Boolean, since: Long?, onMs: Long, offMs: Long, done: Boolean = false) = HabitSummary(
        habit = Habit("1@1", "Retainer", goal, 0x7Fu, HabitPeriod.DAY, rule),
        count = 0u, progress = 0u, done = done, due = true, streak = 0u, bestStreak = 0u, week = emptyList(),
        timed = TimedState(on, since, onMs, offMs),
    )

    @Test
    fun durations() {
        assertEquals("0m", durationText(59_999))
        assertEquals("45m", durationText(45 * min))
        assertEquals("1h 05m", durationText(65 * min))
        assertEquals("22h", durationText(22 * 60 * min))
        assertEquals("0m", durationText(-5 * min))
    }

    @Test
    fun wordsFollowTheUsualState() {
        assertEquals("Pause", switchLabel(wear, on = true))
        assertEquals("Resume", switchLabel(wear, on = false))
        assertEquals("Start", switchLabel(read, on = false))
        assertEquals("Stop", switchLabel(read, on = true))
        assertEquals("Paused", stateName(wear, false))
        assertEquals("Running", stateName(read, true))
    }

    @Test
    fun theRunningStretchKeepsCountingBetweenReloads() {
        // Paused at 0 with 40 min off already today; loaded at 10 min, shown at 25 min.
        val s = summary(wear, 120u, on = false, since = 0, onMs = 300 * min, offMs = 50 * min)
        val live = s.timedLive(now = 25 * min, loadedAt = 10 * min)!!
        assertEquals(65 * min, live.offMs)
        assertEquals(300 * min, live.onMs)
        assertEquals(55 * min, live.leftMs)
        assertTrue(live.met)
        assertEquals(55f / 120f, live.ringFraction, 0.001f)
        assertEquals("Paused 25m · 55m left", timedStatus(wear, live, 25 * min))
        // A past day doesn't keep counting.
        assertEquals(50 * min, s.timedLive(25 * min, 10 * min, isToday = false)!!.offMs)
    }

    @Test
    fun allowancesGoOverAndGoalsGetMet() {
        val over = summary(wear, 60u, on = false, since = 0, onMs = 0, offMs = 70 * min).timedLive(0, 0)!!
        assertFalse(over.met)
        assertEquals(0f, over.ringFraction)
        assertEquals("Paused just now · 10m over", timedStatus(wear, over, 0))

        val reading = summary(read, 30u, on = true, since = 0, onMs = 12 * min, offMs = 0).timedLive(12 * min, 12 * min)!!
        assertEquals("Running 12m · 18m to go", timedStatus(read, reading, 12 * min))
        val done = summary(read, 30u, on = false, since = 0, onMs = 31 * min, offMs = 0).timedLive(40 * min, 40 * min)!!
        assertEquals("Stopped · 31m today ✓", timedStatus(read, done, 40 * min))
        assertEquals(1f, done.ringFraction)
    }

    @Test
    fun beforeTheFirstSwitchNothingIsTracked() {
        val fresh = summary(wear, 120u, on = false, since = null, onMs = 0, offMs = 0, done = true)
        val live = fresh.timedLive(5 * min, 0)!!
        assertFalse(live.tracked)
        assertEquals(0L, live.offMs)
        assertEquals("Not started", timedStatus(wear, live, 5 * min))
        assertTrue("the core judges the day", fresh.doneToday)
        assertNull(summary(wear, 1u, false, null, 0, 0).copy(timed = null).timedLive(0, 0))
    }

    @Test
    fun goalTexts() {
        assertEquals("At most 2h off", timedGoalText(wear, 120u))
        assertEquals("30m on", timedGoalText(read, 30u))
    }

    @Test
    fun aGoalIsSecuredOnceItCantBeMissed() {
        val h = 60 * min
        // At least 30 min: secured the moment it's met.
        val reading = summary(read, 30u, on = true, since = 0, onMs = 30 * min, offMs = 0).timedLive(0, 0)!!
        assertTrue(reading.secured(now = 0, dayEnd = 10 * h))
        assertFalse(summary(read, 30u, true, 0, 29 * min, 0).timedLive(0, 0)!!.secured(0, 10 * h))
        // At most 2 h off with 1 h used: safe only once less than an hour of the day is left.
        val wearing = summary(wear, 120u, on = true, since = 0, onMs = 20 * h, offMs = h).timedLive(0, 0)!!
        assertFalse(wearing.secured(now = 0, dayEnd = 2 * h))
        assertTrue(wearing.secured(now = 0, dayEnd = h))
        assertFalse("over is never safe", summary(wear, 60u, false, 0, 0, 2 * h).timedLive(0, 0)!!.secured(0, 0))
        assertFalse("untracked isn't won", summary(wear, 60u, false, null, 0, 0).timedLive(0, 0)!!.secured(0, 0))
    }
}
