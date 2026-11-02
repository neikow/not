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
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.TabItem
import dev.lysen.not.ui.floatingTabBarHeight
import kotlinx.coroutines.flow.StateFlow

enum class Tab(val label: String, val icon: Int) {
    Tasks("Tasks", R.drawable.ic_task_alt),
    Habits("Habits", R.drawable.ic_repeat),
    Notes("Notes", R.drawable.ic_note),
}

/** Tasks, Habits and Notes under a floating tab bar; Settings opens over them from the lists sheet. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NotAppUi(
    tasks: TasksViewModel,
    habits: HabitsViewModel,
    notes: NotesViewModel,
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
        SettingsScreen(tasks, onBack = { showsSettings = false })
        return
    }
    val pendingHabits = habits.state.collectAsStateWithLifecycle().value.pending
    val editingNote = notes.state.collectAsStateWithLifecycle().value.openNote != null
    // Back from another tab goes to Tasks first, then leaves the app.
    if (tab != Tab.Tasks) BackHandler { tab = Tab.Tasks }

    // The note editor gets the whole screen, and typing gets the room the bar would take.
    val showsBar = !(tab == Tab.Notes && editingNote) && !WindowInsets.isImeVisible
    val barHeight = if (showsBar) floatingTabBarHeight() else 0.dp

    val addAction = remember { AddActionHolder() }
    CompositionLocalProvider(LocalAddAction provides addAction) {
    Box(Modifier.fillMaxSize().background(Not.colors.background)) {
        CompositionLocalProvider(LocalBottomBarHeight provides barHeight) {
            when (tab) {
                Tab.Tasks -> TasksScreen(tasks, onOpenSettings = { showsSettings = true })
                // Not yet on the new design: they keep their own bottom bars above ours.
                Tab.Habits -> Box(Modifier.padding(bottom = barHeight)) { HabitsScreen(habits) }
                Tab.Notes -> Box(Modifier.padding(bottom = barHeight)) { NotesScreen(notes) }
            }
        }
        AnimatedVisibility(
            showsBar,
            modifier = Modifier.align(Alignment.BottomCenter),
            enter = slideInVertically(spring(0.8f, Spring.StiffnessMediumLow)) { it } + fadeIn(),
            exit = slideOutVertically { it } + fadeOut(),
        ) {
            BottomBar(
                tabs = Tab.entries.map { TabItem(it.label, it.icon, badge = if (it == Tab.Habits) pendingHabits else 0) },
                selected = tab.ordinal,
                onSelect = { tab = Tab.entries[it] },
            )
        }
    }
    }
}
