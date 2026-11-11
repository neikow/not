package dev.lysen.not

import android.content.Context
import androidx.core.content.edit

/**
 * Where the app was left: the tab, the task list, the notes folder and note, the workout. Kept on
 * this device so reopening the app (even after the system or a swipe closed it) lands back there.
 * Anything since deleted falls back where each screen already falls back to (the Inbox, all notes).
 */
class ScreenMemory(context: Context) {
    private val prefs = context.getSharedPreferences("screen", Context.MODE_PRIVATE)

    var tab: Tab
        get() = prefs.getString(TAB, null)?.let { name -> Tab.entries.firstOrNull { it.name == name } } ?: Tab.Tasks
        set(value) = prefs.edit { putString(TAB, value.name) }

    var list: String? by string(LIST)
    var folder: String? by string(FOLDER)
    var note: String? by string(NOTE)
    var workout: String? by string(WORKOUT)

    private fun string(key: String) = object : kotlin.properties.ReadWriteProperty<Any?, String?> {
        override fun getValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>) = prefs.getString(key, null)
        override fun setValue(thisRef: Any?, property: kotlin.reflect.KProperty<*>, value: String?) =
            prefs.edit { if (value == null) remove(key) else putString(key, value) }
    }

    private companion object {
        const val TAB = "tab"
        const val LIST = "list"
        const val FOLDER = "folder"
        const val NOTE = "note"
        const val WORKOUT = "workout"
    }
}
