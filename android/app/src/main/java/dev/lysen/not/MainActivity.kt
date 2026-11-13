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
    private val budget: BudgetViewModel by viewModels()
    private val requestedTab = MutableStateFlow<Tab?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) handleIntent(intent)
        setContent {
            NotTheme { NotAppUi(viewModel, habits, notes, workouts, budget, requestedTab, onTabShown = { requestedTab.value = null }) }
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
        intent.getStringExtra(EXTRA_MORE_PAGE)?.let { name ->
            (application as NotApp).screenMemory.morePage = MorePage.entries.firstOrNull { it.name == name }
            requestedTab.value = Tab.More
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
        budget.refreshToday()
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
            (application as NotApp).screenMemory.morePage = MorePage.Workouts
            requestedTab.value = Tab.More
        }
        // `--ez seedBudget true`: accounts, limits and a period of spending, on an empty budget only.
        if (intent.getBooleanExtra("seedBudget", false)) {
            budget.seedDemo()
            (application as NotApp).screenMemory.morePage = MorePage.Budget
            requestedTab.value = Tab.More
        }
        // `--ez seedNotes true`: a checklist made into tasks, and a note in the earlier task-block
        // format; on a device without notes only.
        if (intent.getBooleanExtra("seedNotes", false)) {
            notes.seedDemo()
            requestedTab.value = Tab.Notes
        }
    }

    companion object {
        /** List id to show (or, for quick add, to add to). */
        const val EXTRA_LIST = "dev.lysen.not.LIST"
        const val EXTRA_TAB = "dev.lysen.not.TAB"
        /** A [MorePage] name: opens More on that page. */
        const val EXTRA_MORE_PAGE = "dev.lysen.not.MORE_PAGE"
        private const val EXTRA_NOTE = "dev.lysen.not.NOTE"
        /** Folder id for the new note, "" for none. */
        private const val EXTRA_NEW_NOTE = "dev.lysen.not.NEW_NOTE"

        fun openList(context: Context, listId: String): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_LIST, listId)

        fun openTab(context: Context, tab: Tab): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_TAB, tab.name)

        fun openWorkouts(context: Context): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_MORE_PAGE, MorePage.Workouts.name)

        fun openBudget(context: Context): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_MORE_PAGE, MorePage.Budget.name)

        fun openNote(context: Context, noteId: String): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_NOTE, noteId)

        fun newNote(context: Context, folderId: String?): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_NEW_NOTE, folderId.orEmpty())
    }
}
