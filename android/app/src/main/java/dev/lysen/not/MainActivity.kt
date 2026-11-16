package dev.lysen.not

import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import dev.lysen.not.widget.HabitsWidgetReceiver
import dev.lysen.not.widget.NoteWidgetReceiver
import dev.lysen.not.widget.NotesWidgetReceiver
import dev.lysen.not.widget.TasksWidgetReceiver
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val viewModel: TasksViewModel by viewModels()
    private val habits: HabitsViewModel by viewModels()
    private val notes: NotesViewModel by viewModels()
    private val workouts: WorkoutsViewModel by viewModels()
    private val requestedTab = MutableStateFlow<Tab?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) handleIntent(intent)
        setContent {
            NotTheme { NotAppUi(viewModel, habits, notes, workouts, requestedTab, onTabShown = { requestedTab.value = null }) }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent) {
        intent.getStringExtra(EXTRA_LIST)?.let {
            viewModel.select(it)
            requestedTab.value = Tab.Tasks
        }
        intent.getStringExtra(EXTRA_TAB)?.let { name ->
            requestedTab.value = Tab.entries.firstOrNull { it.name == name }
        }
        if (intent.getBooleanExtra(EXTRA_REVIEW, false)) {
            viewModel.requestReview()
            requestedTab.value = Tab.Tasks
        }
        intent.getStringExtra(EXTRA_NOTE)?.let {
            notes.open(it)
            requestedTab.value = Tab.Notes
        }
        if (intent.hasExtra(EXTRA_NEW_NOTE)) {
            notes.selectFolder(intent.getStringExtra(EXTRA_NEW_NOTE)?.takeIf { it.isNotEmpty() })
            notes.newNote()
            requestedTab.value = Tab.Notes
        }
        handleDevIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        habits.refreshToday()
        workouts.refreshToday()
        viewModel.syncNow()
    }

    /**
     * Debug builds only, for scripted testing:
     * `adb shell am start -n dev.lysen.not/.MainActivity --es server URL --es code CODE --es add TITLE` (parsed like quick add),
     * `--ez pinWidget true` asks the launcher to place the tasks widget, `--ez addTile true` offers the
     * Quick Settings tile (Android 13+).
     * Never honoured in release builds: any app could otherwise pair this device with its own server.
     */
    private fun handleDevIntent(intent: Intent) {
        if (!BuildConfig.DEBUG) return
        val server = intent.getStringExtra("server")
        val code = intent.getStringExtra("code")
        if (server != null && code != null) viewModel.pair(server, code, viewModel.defaultDeviceName)
        intent.getStringExtra("add")?.let { viewModel.quickAdd(it) }
        // "tasks", "habits", "notes" or "note"
        intent.getStringExtra("pinWidget")?.let { name ->
            val receiver = when (name) {
                "habits" -> HabitsWidgetReceiver::class.java
                "notes" -> NotesWidgetReceiver::class.java
                "note" -> NoteWidgetReceiver::class.java
                "next" -> dev.lysen.not.widget.NextWidgetReceiver::class.java
                else -> TasksWidgetReceiver::class.java
            }
            lifecycleScope.launch { Shortcuts.requestWidget(this@MainActivity, receiver) }
        }
        if (intent.getBooleanExtra("addTile", false)) Shortcuts.requestTile(this)
        // `--ez seedWorkouts true`: sample sessions for screenshots, on an empty device only.
        if (intent.getBooleanExtra("seedWorkouts", false)) {
            workouts.seedDemo()
            requestedTab.value = Tab.Workouts
        }
        // `--ez seedNotes true`: a checklist made into tasks, and a note in the earlier task-block
        // format; on a device without notes only.
        if (intent.getBooleanExtra("seedNotes", false)) {
            notes.seedDemo()
            requestedTab.value = Tab.Notes
        }
        // `--ez seedTimed true`: a worn habit with breaks today and a reading timer; once only.
        if (intent.getBooleanExtra("seedTimed", false)) {
            seedTimed()
            requestedTab.value = Tab.Habits
        }
        if (intent.getStringExtra("pinWidget") == "timed") {
            lifecycleScope.launch { Shortcuts.requestWidget(this@MainActivity, dev.lysen.not.widget.TimedWidgetReceiver::class.java) }
        }
    }

    private fun seedTimed() {
        val store = (application as NotApp).store
        if (store.habitsNow(today()).any { it.habit.timed != null }) return
        val now = nowMs()
        val midnight = java.time.LocalDate.now().atStartOfDay(java.time.ZoneId.systemDefault()).toInstant().toEpochMilli()
        val h = 3_600_000L
        val wear = store.createTimedHabit("Wear retainer", 120u, dev.lysen.not.core.everyDay(), dev.lysen.not.core.TimedRule(atMostOff = true, usualOn = true, nudgeMinutes = 30u))
        // On since yesterday evening, a lunch break, and off for the last 25 minutes.
        store.setHabitState(wear, true, midnight - 3 * h, null, now)
        val lunch = (midnight + 12 * h).coerceAtMost(now - 2 * h)
        store.setHabitState(wear, false, lunch, lunch + 40 * 60_000, now)
        store.setHabitState(wear, false, now - 25 * 60_000, null, now)
        val read = store.createTimedHabit("Read", 30u, dev.lysen.not.core.everyDay(), dev.lysen.not.core.TimedRule(atMostOff = false, usualOn = false, nudgeMinutes = 0u))
        store.setHabitState(read, true, midnight - 3 * h, midnight - 3 * h + 35 * 60_000, now)
        (application as NotApp).timedAlerts.setLive(wear, true)
    }

    companion object {
        /** List id to show (or, for quick add, to add to). */
        const val EXTRA_LIST = "dev.lysen.not.LIST"
        const val EXTRA_TAB = "dev.lysen.not.TAB"
        /** Opens the weekly review (from its Sunday notification). */
        const val EXTRA_REVIEW = "dev.lysen.not.REVIEW"
        private const val EXTRA_NOTE = "dev.lysen.not.NOTE"
        /** Folder id for the new note, "" for none. */
        private const val EXTRA_NEW_NOTE = "dev.lysen.not.NEW_NOTE"

        fun openList(context: Context, listId: String): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_LIST, listId)

        fun openTab(context: Context, tab: Tab): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_TAB, tab.name)

        fun openNote(context: Context, noteId: String): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_NOTE, noteId)

        fun newNote(context: Context, folderId: String?): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_NEW_NOTE, folderId.orEmpty())
    }
}
