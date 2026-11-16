package dev.lysen.not

import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.mutablePreferencesOf
import dev.lysen.not.widget.HabitsOptions
import dev.lysen.not.widget.LOOK_BACKGROUND
import dev.lysen.not.widget.LOOK_HEADER
import dev.lysen.not.widget.LOOK_THEME
import dev.lysen.not.widget.NextOptions
import dev.lysen.not.widget.NotesOptions
import dev.lysen.not.widget.TasksOptions
import dev.lysen.not.widget.TimedOptions
import dev.lysen.not.widget.WidgetBackground
import dev.lysen.not.widget.WidgetDensity
import dev.lysen.not.widget.WidgetLook
import dev.lysen.not.widget.WidgetTheme
import dev.lysen.not.widget.asLook
import dev.lysen.not.widget.asPreferences
import dev.lysen.not.widget.isOn
import dev.lysen.not.widget.lookOver
import dev.lysen.not.widget.setOverride
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Widget looks (defaults + per-widget overrides) and content options, as stored. */
class WidgetSettingsTest {
    private val defaults = WidgetLook(background = WidgetBackground.Translucent, density = WidgetDensity.Compact, header = false)

    @Test
    fun aWidgetWithoutOverridesTakesTheDefaults() {
        assertEquals(defaults, emptyPreferences().lookOver(defaults))
    }

    @Test
    fun overridesWinOneOptionAtATime() {
        val prefs = mutablePreferencesOf(LOOK_THEME to "Dark", LOOK_HEADER to "on")
        val look = prefs.lookOver(defaults)
        assertEquals(WidgetTheme.Dark, look.theme)
        assertTrue(look.header)
        assertEquals("the rest still follows the defaults", WidgetBackground.Translucent, look.background)
        // Back to "Default" clears the override.
        prefs.setOverride(LOOK_THEME, null)
        assertEquals(defaults.theme, prefs.lookOver(defaults).theme)
        // A value from a newer version reads as the default.
        assertEquals(defaults.background, mutablePreferencesOf(LOOK_BACKGROUND to "Glass").lookOver(defaults).background)
    }

    @Test
    fun defaultsRoundTripThroughPreferences() {
        assertEquals(defaults, defaults.asPreferences().asLook())
    }

    @Test
    fun contentOptionsHaveDefaults() {
        val none = emptyPreferences()
        assertFalse(TasksOptions.done.isOn(none))
        assertTrue(TasksOptions.meta.isOn(none))
        assertTrue(TasksOptions.add.isOn(none))
        assertTrue(HabitsOptions.streaks.isOn(none))
        assertEquals("show", HabitsOptions.hideDone.of(none))
        assertNull("all habits", none[HabitsOptions.ids])
        assertEquals(30, NotesOptions.limit(none))
        assertEquals(5, NotesOptions.limit(mutablePreferencesOf(NotesOptions.count.key to "5")))
        assertNull("any time", NextOptions.days(none))
        assertEquals(3, NextOptions.days(mutablePreferencesOf(NextOptions.ahead.key to "3")))
        assertEquals(0, NextOptions.days(mutablePreferencesOf(NextOptions.ahead.key to "0")))
    }

    @Test
    fun theTimedRingShowsWhatMattersUnlessToldOtherwise() {
        val none = emptyPreferences()
        assertTrue("allowance: time left", TimedOptions.showsLeft(none, atMostOff = true))
        assertFalse("goal: time so far", TimedOptions.showsLeft(none, atMostOff = false))
        val left = mutablePreferencesOf(TimedOptions.ring.key to "left")
        assertTrue(TimedOptions.showsLeft(left, atMostOff = false))
        val used = mutablePreferencesOf(TimedOptions.ring.key to "used")
        assertFalse(TimedOptions.showsLeft(used, atMostOff = true))
        assertTrue(TimedOptions.detail.isOn(none))
    }
}
