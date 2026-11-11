package dev.lysen.not

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.ui.AddActionHolder
import dev.lysen.not.ui.BottomBar
import dev.lysen.not.ui.LocalAddAction
import dev.lysen.not.ui.LocalBottomBarHeight
import dev.lysen.not.ui.LocalOpenSettings
import dev.lysen.not.ui.LocalTabGestures
import dev.lysen.not.ui.TabGesture
import dev.lysen.not.ui.TabGestures
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.TabItem
import dev.lysen.not.ui.floatingTabBarHeight
import kotlinx.coroutines.flow.StateFlow

/** [holdLabel] says what holding the tab does, for screen readers. */
enum class Tab(val label: String, val icon: Int, val holdLabel: String? = null) {
    Tasks("Tasks", R.drawable.ic_task_alt, "Show lists"),
    Habits("Habits", R.drawable.ic_repeat),
    Notes("Notes", R.drawable.ic_note, "Show folders"),
    Workouts("Workouts", R.drawable.ic_fitness, "Start a workout"),
}

/** Tasks, Habits, Notes and Workouts under a floating tab bar; Settings opens over them from each one's top-right profile circle. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NotAppUi(
    tasks: TasksViewModel,
    habits: HabitsViewModel,
    notes: NotesViewModel,
    workouts: WorkoutsViewModel,
    requestedTab: StateFlow<Tab?>,
    onTabShown: () -> Unit,
) {
    var tab by rememberSaveable { mutableStateOf(Tab.Tasks) }
    var showsSettings by rememberSaveable { mutableStateOf(false) }
    // Links from widgets and shortcuts switch tabs.
    val requested by requestedTab.collectAsStateWithLifecycle()
    LaunchedEffect(requested) {
        requested?.let {
            tab = it
            showsSettings = false
            onTabShown()
        }
    }
    if (showsSettings) {
        BackHandler { showsSettings = false }
        SettingsScreen(tasks, workouts, onBack = { showsSettings = false })
        return
    }
    val pendingHabits = habits.state.collectAsStateWithLifecycle().value.pending
    val editingNote = notes.state.collectAsStateWithLifecycle().value.openNote != null
    val inWorkout = workouts.state.collectAsStateWithLifecycle().value.openId != null
    // Back from another tab goes to Tasks first, then leaves the app.
    if (tab != Tab.Tasks) BackHandler { tab = Tab.Tasks }

    // The note editor gets the whole screen, and typing gets the room the bar would take.
    val showsBar = !(tab == Tab.Notes && editingNote) && !(tab == Tab.Workouts && inWorkout) && !WindowInsets.isImeVisible
    val barHeight = if (showsBar) floatingTabBarHeight() else 0.dp

    val addAction = remember { AddActionHolder() }
    val gestures = remember { TabGestures() }
    val openSettings = remember { { showsSettings = true } }
    CompositionLocalProvider(LocalAddAction provides addAction, LocalTabGestures provides gestures, LocalOpenSettings provides openSettings) {
    Box(Modifier.fillMaxSize().background(Not.colors.background)) {
        CompositionLocalProvider(LocalBottomBarHeight provides barHeight) {
            when (tab) {
                Tab.Tasks -> TasksScreen(tasks, onOpenSettings = { showsSettings = true })
                Tab.Habits -> HabitsScreen(habits)
                Tab.Notes -> NotesScreen(notes)
                Tab.Workouts -> WorkoutsScreen(workouts)
            }
        }
        AnimatedVisibility(
            showsBar,
            modifier = Modifier.align(Alignment.BottomCenter),
            enter = slideInVertically(spring(0.8f, Spring.StiffnessMediumLow)) { it } + fadeIn(),
            exit = slideOutVertically { it } + fadeOut(),
        ) {
            BottomBar(
                tabs = Tab.entries.map { TabItem(it.label, it.icon, badge = if (it == Tab.Habits) pendingHabits else 0, holdLabel = it.holdLabel) },
                selected = tab.ordinal,
                // Tapping the current tab again, or holding any tab, is the screen's to handle
                // (Tasks opens its lists); holding another tab switches to it first.
                onSelect = { if (it == tab.ordinal) gestures.send(TabGesture.Reselect) else tab = Tab.entries[it] },
                onHold = {
                    if (it == tab.ordinal) {
                        gestures.send(TabGesture.Hold)
                    } else {
                        gestures.sendToNext(TabGesture.Hold)
                        tab = Tab.entries[it]
                    }
                },
            )
        }
    }
    }
}
