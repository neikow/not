package dev.lysen.not.widget

import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey

/*
 * What each widget shows, beyond which list, folder, note or habit: set in its setup screen and
 * kept in its Glance state. Everything has a default, so a widget placed without setup works.
 */

/** A choice between a few values, stored by key; the first option is the default. */
class WidgetChoice(val key: Preferences.Key<String>, val label: String, val options: List<Pair<String, String>>) {
    val default: String get() = options.first().first

    fun of(prefs: Preferences): String = prefs[key]?.takeIf { v -> options.any { it.first == v } } ?: default
}

private fun onOff(key: String, label: String, defaultOn: Boolean) =
    WidgetChoice(stringPreferencesKey(key), label, if (defaultOn) listOf("on" to "Show", "off" to "Hide") else listOf("off" to "Hide", "on" to "Show"))

fun WidgetChoice.isOn(prefs: Preferences) = of(prefs) == "on"

object TasksOptions {
    val done = onOff("tasks_done", "Done tasks", defaultOn = false)
    val meta = onOff("tasks_meta", "Dates and tags", defaultOn = true)
    val add = onOff("tasks_add", "+ button", defaultOn = true)
    val all = listOf(done, meta, add)
}

object HabitsOptions {
    /** Ids of the habits to show; absent or empty means all. */
    val ids = stringSetPreferencesKey("habits_ids")
    val hideDone = WidgetChoice(stringPreferencesKey("habits_done"), "Done habits", listOf("show" to "Show", "hide" to "Hide"))
    val streaks = onOff("habits_streaks", "Streaks", defaultOn = true)
    val all = listOf(hideDone, streaks)
}

object NotesOptions {
    val count = WidgetChoice(stringPreferencesKey("notes_count"), "How many", listOf("all" to "All", "3" to "3", "5" to "5", "10" to "10"))
    val previews = onOff("notes_previews", "Previews", defaultOn = true)
    val all = listOf(count, previews)

    fun limit(prefs: Preferences): Int = count.of(prefs).toIntOrNull() ?: 30
}

object TimedOptions {
    /** The number and ring: time left, or time so far. Default: left for allowances, so far for goals. */
    val ring = WidgetChoice(stringPreferencesKey("timed_ring"), "Ring shows", listOf("auto" to "Default", "left" to "Time left", "used" to "Time so far"))
    val detail = onOff("timed_detail", "Detail line", defaultOn = true)
    val all = listOf(ring, detail)

    fun showsLeft(prefs: Preferences, atMostOff: Boolean): Boolean = when (ring.of(prefs)) {
        "left" -> true
        "used" -> false
        else -> atMostOff
    }
}

object NextOptions {
    /** How far ahead to look once today is clear: days, or "any". */
    val ahead = WidgetChoice(stringPreferencesKey("next_ahead"), "Look ahead", listOf("any" to "Any time", "0" to "Today", "3" to "3 days", "7" to "A week"))
    val all = listOf(ahead)

    /** Days after today an upcoming task may start to show; null for no limit. */
    fun days(prefs: Preferences): Int? = ahead.of(prefs).toIntOrNull()
}
