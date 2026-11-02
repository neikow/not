package dev.lysen.not

import dev.lysen.not.ui.AddAction
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.border
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.OutlineRow
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.TaskList
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Divider
import dev.lysen.not.ui.EmptyState
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.IconButton
import dev.lysen.not.ui.LargeTitleScreen
import dev.lysen.not.ui.LocalBottomBarHeight
import dev.lysen.not.ui.Menu
import dev.lysen.not.ui.MenuItem
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.PromptDialog
import dev.lysen.not.ui.RoundCheck
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.SwipeAction
import dev.lysen.not.ui.SwipeRow
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.ToastHost
import dev.lysen.not.ui.ToastState
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.delay

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TasksScreen(viewModel: TasksViewModel, onOpenSettings: () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toasts = remember { ToastState() }
    var showsLists by remember { mutableStateOf(false) }
    var composing by rememberSaveable { mutableStateOf(false) }
    var showsCompleted by rememberSaveable { mutableStateOf(false) }
    var newList by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<TaskList?>(null) }
    val title = state.selectedList?.title ?: "Inbox"

    // Destructive actions confirm after the fact, with Undo, instead of asking first.
    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            if (toasts.show(undoable.message, action = "Undo")) viewModel.undo()
        }
    }

    Box(Modifier.fillMaxSize()) {
        LargeTitleScreen(
            title = title,
            onTitleClick = { showsLists = true },
            titleAccessory = { Icon(R.drawable.ic_expand_more, "Switch list", tint = Not.colors.inkFaint, size = 28.dp) },
            navigation = { IconButton(R.drawable.ic_menu, "Lists", { showsLists = true }) },
            actions = {
                SyncStatus(state.sync, onClick = onOpenSettings)
                val list = state.selectedList
                if (list != null && list.id != INBOX_ID) {
                    ListMenu(onRename = { renaming = list }, onDelete = { viewModel.deleteList(list) })
                }
            },
            header = { Subtitle(state) },
        ) {
            val outline = state.outline
            if (outline.open.isEmpty()) {
                item(key = "empty") {
                    when {
                        outline.completed.isNotEmpty() ->
                            EmptyState(R.drawable.ic_check, "All done", "Everything in $title is checked off.", Modifier.padding(bottom = 24.dp))
                        state.selectedListId == INBOX_ID ->
                            EmptyState(R.drawable.ic_inbox, "Inbox zero", "Anything you jot down lands here first. Sort it into lists when you have a moment.")
                        else -> EmptyState(R.drawable.ic_list, "Nothing in $title yet", "Tap + to add the first one.")
                    }
                }
            }
            items(outline.open, key = { it.task.id }) { row ->
                TaskRow(row, viewModel, Modifier.animateItem())
            }
            if (outline.completed.isNotEmpty()) {
                item(key = "completed") {
                    CompletedHeader(
                        count = outline.completed.count { it.depth == 0u },
                        expanded = showsCompleted,
                        onToggle = { showsCompleted = !showsCompleted },
                        modifier = Modifier.animateItem(),
                    )
                }
                if (showsCompleted) {
                    items(outline.completed, key = { it.task.id }) { row ->
                        TaskRow(row, viewModel, Modifier.animateItem())
                    }
                }
            }
        }

        // The + beside the tab bar opens the composer above the keyboard.
        AddAction("New task", enabled = !composing) { composing = true }
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .imePadding()
                .padding(bottom = if (WindowInsets.isImeVisible) 8.dp else LocalBottomBarHeight.current + 8.dp),
        ) {
            ToastHost(toasts, Modifier.align(Alignment.CenterHorizontally).padding(bottom = 8.dp))
            if (composing) Composer("New task in $title", onAdd = { viewModel.addTask(it) }, onClose = { composing = false })
        }
    }

    state.openTask?.let { task ->
        TaskDetailSheet(task, state, viewModel, onDismiss = { viewModel.openTask(null) })
    }
    if (showsLists) {
        ListsSheet(
            state = state,
            onSelect = viewModel::select,
            onNewList = { newList = true },
            onSettings = onOpenSettings,
            onDismiss = { showsLists = false },
        )
    }
    if (newList) {
        PromptDialog(
            title = "New list",
            confirm = "Create",
            placeholder = "Groceries, Work, Trip…",
            onConfirm = {
                viewModel.addList(it)
                newList = false
                showsLists = false
            },
            onDismiss = { newList = false },
        )
    }
    renaming?.let { list ->
        PromptDialog(
            title = "Rename list",
            confirm = "Rename",
            initial = list.title,
            onConfirm = {
                viewModel.rename(list.id, it)
                renaming = null
            },
            onDismiss = { renaming = null },
        )
    }
    state.error?.let {
        CardDialog(
            title = "Something went wrong",
            onDismiss = viewModel::dismissError,
            buttons = { Button("OK", viewModel::dismissError) },
        ) { Text(it, color = Not.colors.inkMuted) }
    }
}

@Composable
private fun Subtitle(state: UiState) {
    val open = state.outline.open.count { it.depth == 0u }
    if (open == 0) return
    Text(
        if (open == 1) "1 task" else "$open tasks",
        style = Not.type.body,
        color = Not.colors.inkMuted,
        modifier = Modifier.padding(top = 2.dp),
    )
}

/**
 * The add field, floating above the keyboard. It keeps focus after adding, for the next one, and
 * closes when the keyboard goes away with nothing typed, or on Back.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Composer(placeholder: String, onAdd: (String) -> Unit, onClose: () -> Unit) {
    var draft by rememberSaveable { mutableStateOf("") }
    val colors = Not.colors
    val focus = remember { FocusRequester() }
    val imeVisible = WindowInsets.isImeVisible
    var hadKeyboard by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { focus.requestFocus() }
    LaunchedEffect(imeVisible) {
        if (imeVisible) hadKeyboard = true else if (hadKeyboard && draft.isBlank()) onClose()
    }
    BackHandler(onBack = onClose)
    fun submit() {
        if (draft.isBlank()) return
        onAdd(draft)
        draft = ""
    }
    Field(
        draft,
        { draft = it },
        placeholder,
        modifier = Modifier
            .padding(horizontal = 12.dp)
            .fillMaxWidth()
            .shadow(14.dp, RoundedCornerShape(24.dp), ambientColor = Color.Black.copy(alpha = 0.14f), spotColor = Color.Black.copy(alpha = 0.14f))
            .clip(RoundedCornerShape(24.dp))
            .background(colors.surface)
            .padding(horizontal = 8.dp, vertical = 8.dp)
            .focusRequester(focus),
        leading = { Spacer(Modifier.width(4.dp)); RoundCheckPlaceholder() },
        trailing = {
            Box(
                Modifier
                    .size(36.dp)
                    .clip(CircleShape)
                    .background(if (draft.isNotBlank()) colors.accent else colors.sunken)
                    .pressable(::submit, enabled = draft.isNotBlank()),
                contentAlignment = Alignment.Center,
            ) { Icon(R.drawable.ic_arrow_up, "Add", tint = if (draft.isNotBlank()) colors.onAccent else colors.inkFaint, size = 20.dp) }
        },
        onDone = ::submit,
    )
}

/** The empty ring a new task will have, as the composer's leading mark. */
@Composable
private fun RoundCheckPlaceholder() {
    Box(Modifier.size(22.dp).border(1.75.dp, Not.colors.inkFaint, CircleShape))
}

/** Swipe right to complete (or reopen), left to delete; tap for details. */
@Composable
private fun TaskRow(row: OutlineRow, viewModel: TasksViewModel, modifier: Modifier) {
    val haptics = LocalHapticFeedback.current
    val colors = Not.colors
    val done = row.task.completedAt != null
    SwipeRow(
        start = SwipeAction(if (done) R.drawable.ic_add_task else R.drawable.ic_check, colors.accent, colors.onAccent) { viewModel.toggle(row.task) },
        end = SwipeAction(R.drawable.ic_delete, colors.danger, Color.White, dismisses = true) {
            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
            viewModel.delete(row.task)
        },
        modifier = modifier,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(colors.background)
                .pressable({ viewModel.openTask(row.task.id) }, pressedScale = 0.985f)
                .heightIn(min = 56.dp)
                .padding(start = (12 + row.depth.toInt() * 28).dp, end = 24.dp, top = 2.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            RoundCheck(done, { viewModel.toggle(row.task) }, size = if (row.depth > 0u) 20.dp else 24.dp)
            Column(Modifier.weight(1f).padding(start = 4.dp)) {
                Text(
                    row.task.title,
                    style = Not.type.body,
                    color = if (row.done) colors.inkFaint else colors.ink,
                    textDecoration = if (row.done) TextDecoration.LineThrough else null,
                )
                val preview = row.task.notes.lineSequence().firstOrNull { it.isNotBlank() }
                if (preview != null) Text(preview, style = Not.type.caption, color = colors.inkMuted, maxLines = 1)
            }
        }
    }
}

@Composable
private fun CompletedHeader(count: Int, expanded: Boolean, onToggle: () -> Unit, modifier: Modifier) {
    val rotation by animateFloatAsState(if (expanded) 180f else 0f, label = "chevron")
    Row(
        modifier
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .clip(CircleShape)
            .pressable(onToggle, pressedScale = 0.97f)
            .padding(horizontal = 8.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text("Completed", style = Not.type.label, color = Not.colors.inkMuted)
        Text(count.toString(), style = Not.type.label, color = Not.colors.inkFaint)
        Icon(R.drawable.ic_expand_more, if (expanded) "Hide completed" else "Show completed", tint = Not.colors.inkFaint, size = 20.dp, modifier = Modifier.rotate(rotation))
    }
}

/** Every list, new list and settings: what the drawer used to hold. */
@Composable
private fun ListsSheet(
    state: UiState,
    onSelect: (String) -> Unit,
    onNewList: () -> Unit,
    onSettings: () -> Unit,
    onDismiss: () -> Unit,
) {
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text("Lists", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 12.dp))
            state.lists.forEach { list ->
                ListRow(
                    icon = if (list.id == INBOX_ID) R.drawable.ic_inbox else R.drawable.ic_list,
                    title = list.title,
                    count = list.openCount.toInt(),
                    selected = list.id == state.selectedListId,
                    onClick = {
                        onSelect(list.id)
                        close()
                    },
                )
            }
            ListRow(R.drawable.ic_add, "New list", accent = true, onClick = onNewList)
            Divider(Modifier.padding(horizontal = 14.dp, vertical = 8.dp))
            ListRow(R.drawable.ic_settings, "Settings", onClick = {
                close()
                onSettings()
            })
        }
    }
}

@Composable
private fun ListRow(icon: Int, title: String, onClick: () -> Unit, count: Int = 0, selected: Boolean = false, accent: Boolean = false) {
    val colors = Not.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(CircleShape)
            .background(if (selected) colors.accentSoft else Color.Transparent)
            .pressable(onClick, pressedScale = 0.98f)
            .heightIn(min = 52.dp)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = if (accent || selected) colors.accentInk else colors.inkMuted)
        Spacer(Modifier.width(14.dp))
        Text(
            title,
            style = if (selected) Not.type.bodyStrong else Not.type.body,
            color = if (accent) colors.accentInk else colors.ink,
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        if (count > 0) Text(count.toString(), style = Not.type.label, color = colors.inkFaint)
    }
}

@Composable
private fun ListMenu(onRename: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(R.drawable.ic_more_vert, "List options", { open = true })
        Menu(open, onDismiss = { open = false }) {
            MenuItem("Rename list", icon = R.drawable.ic_edit, onClick = {
                open = false
                onRename()
            })
            MenuItem("Delete list", icon = R.drawable.ic_delete, danger = true, onClick = {
                open = false
                onDelete()
            })
        }
    }
}

/**
 * Quiet unless something needs attention: nothing while synced or unpaired, an icon while offline
 * or signed out, and one for syncs that take a while. Tapping opens Settings.
 */
@Composable
private fun SyncStatus(state: SyncState?, onClick: () -> Unit) {
    var slow by remember { mutableStateOf(false) }
    LaunchedEffect(state) {
        slow = false
        if (state == SyncState.Syncing) {
            delay(1500)
            slow = true
        }
    }
    val (icon, label, tint) = when (state) {
        is SyncState.Offline -> Triple(R.drawable.ic_cloud_off, "Offline", Not.colors.inkMuted)
        SyncState.Unauthorized -> Triple(R.drawable.ic_error, "Signed out of sync", Not.colors.danger)
        SyncState.Syncing -> if (slow) Triple(R.drawable.ic_sync, "Syncing", Not.colors.inkMuted) else return
        null, SyncState.Synced -> return
    }
    IconButton(icon, label, onClick, tint = tint)
}

/** One line of text in a dialog; for screens still on the old design. */
@Composable
fun TextPromptDialog(title: String, confirm: String, onConfirm: (String) -> Unit, onDismiss: () -> Unit, initial: String = "") =
    PromptDialog(title, confirm, onConfirm, onDismiss, initial)
