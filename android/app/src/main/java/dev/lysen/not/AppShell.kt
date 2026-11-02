package dev.lysen.not

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle

enum class Tab(val label: String, val icon: Int) {
    Tasks("Tasks", R.drawable.ic_task_alt),
    Habits("Habits", R.drawable.ic_repeat),
    Notes("Notes", R.drawable.ic_note),
}

/** Tasks, Habits and Notes as bottom tabs; Settings opens over them from the Tasks drawer. */
@Composable
fun NotAppUi(tasks: TasksViewModel, habits: HabitsViewModel, notes: NotesViewModel) {
    var tab by rememberSaveable { mutableStateOf(Tab.Tasks) }
    var showsSettings by rememberSaveable { mutableStateOf(false) }
    if (showsSettings) {
        BackHandler { showsSettings = false }
        SettingsScreen(tasks, onBack = { showsSettings = false })
        return
    }
    val pendingHabits = habits.state.collectAsStateWithLifecycle().value.pending
    val editingNote = notes.state.collectAsStateWithLifecycle().value.openNote != null
    // Back from another tab goes to Tasks first, then leaves the app.
    if (tab != Tab.Tasks) BackHandler { tab = Tab.Tasks }

    Scaffold(
        // Only the bar's own height: each tab handles the status bar and keyboard itself.
        contentWindowInsets = WindowInsets(0),
        bottomBar = {
            // The note editor gets the whole screen.
            if (!(tab == Tab.Notes && editingNote)) {
                NavigationBar {
                    Tab.entries.forEach { item ->
                        NavigationBarItem(
                            selected = item == tab,
                            onClick = { tab = item },
                            icon = {
                                BadgedBox(badge = {
                                    if (item == Tab.Habits && pendingHabits > 0) Badge { Text(pendingHabits.toString()) }
                                }) { Icon(painterResource(item.icon), null) }
                            },
                            label = { Text(item.label) },
                        )
                    }
                }
            }
        },
    ) { padding ->
        Box(Modifier.padding(padding).consumeWindowInsets(padding)) {
            when (tab) {
                Tab.Tasks -> TasksScreen(tasks, onOpenSettings = { showsSettings = true })
                Tab.Habits -> HabitsScreen(habits)
                Tab.Notes -> NotesScreen(notes)
            }
        }
    }
}
