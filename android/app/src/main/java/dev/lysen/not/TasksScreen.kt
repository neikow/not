package dev.lysen.not

import dev.lysen.not.ui.AddAction
import androidx.compose.foundation.lazy.rememberLazyListState
import dev.lysen.not.ui.ReorderState
import dev.lysen.not.ui.rememberReorderState
import dev.lysen.not.ui.reorderable
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.border
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
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
import androidx.compose.ui.draw.alpha
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
import dev.lysen.not.core.TaskItem
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
import dev.lysen.not.ui.OnTabGesture
import dev.lysen.not.ui.PullAction
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.SwipeAction
import dev.lysen.not.ui.SwipeRow
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.ToastHost
import dev.lysen.not.core.TaskSort
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
    val title = state.title
    val haptics = LocalHapticFeedback.current
    // The title is a long reach: tapping or holding Tasks in the tab bar opens the lists too.
    OnTabGesture { showsLists = true }

    // Destructive actions confirm after the fact, with Undo, instead of asking first.
    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            if (toasts.show(undoable.message, action = "Undo")) viewModel.undo()
        }
    }

    val listState = rememberLazyListState()
    val reorder = rememberReorderState(listState, state.outline.open) { it.task.id }

    Box(Modifier.fillMaxSize()) {
        LargeTitleScreen(
            title = title,
            state = listState,
            onTitleClick = { showsLists = true },
            titleAccessory = { Icon(R.drawable.ic_expand_more, "Switch list", tint = Not.colors.inkFaint, size = 28.dp) },
            actions = {
                SyncStatus(state.sync, onClick = onOpenSettings)
                // Today has its own order: overdue first, then by time.
                state.selectedList?.takeUnless { state.isToday }?.let { list ->
                    SortMenu(list.sort) { viewModel.setSort(list.id, it) }
                }
            },
            header = { Subtitle(state) },
            pull = PullAction("Lists", R.drawable.ic_list) { showsLists = true },
            // Swiping the title bar steps through Today and the lists, in the order the sheet shows them.
            onSwipe = { forward ->
                neighbour(state, forward)?.let { (id, _) ->
                    haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                    viewModel.select(id)
                }
            },
            next = neighbour(state, forward = true)?.second,
        ) {
            val outline = state.outline
            if (outline.open.isEmpty()) {
                // Fades in once the last row has left, instead of popping in under it.
                item(key = "empty") {
                    Box(Modifier.animateItem(fadeInSpec = tween(260, delayMillis = 180), fadeOutSpec = tween(120))) {
                    when {
                        state.isToday ->
                            EmptyState(R.drawable.ic_today, "Nothing due today", "Add “tomorrow”, “fri 9am” or “every mon” to a task to see it here when it's due.")
                        outline.completed.isNotEmpty() ->
                            EmptyState(R.drawable.ic_check, "All done", "Everything in $title is checked off.", Modifier.padding(bottom = 24.dp))
                        state.selectedListId == INBOX_ID ->
                            EmptyState(R.drawable.ic_inbox, "Inbox zero", "Anything you jot down lands here first. Sort it into lists when you have a moment.")
                        else -> EmptyState(R.drawable.ic_list, "Nothing in $title yet", "Tap + to add the first one.")
                    }
                    }
                }
            }
            items(reorder.items, key = { it.task.id }) { row ->
                TaskRow(
                    row,
                    viewModel,
                    (if (reorder.isDragging(row)) Modifier else Modifier.animateItem())
                        // Today gathers tasks from every list, so it has no order of its own.
                        .reorderable(reorder, row, row.task.id, enabled = !state.isToday && !row.done) {
                            startDrag(reorder, outline.open, row, viewModel)
                        },
                    listTitle = if (state.isToday) state.listTitle(row.task.listId) else null,
                )
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
        AddAction("New task", enabled = !composing, holdLabel = "New list", onHold = { newList = true }) { composing = true }
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .imePadding()
                .padding(bottom = if (WindowInsets.isImeVisible) 8.dp else LocalBottomBarHeight.current + 8.dp),
        ) {
            ToastHost(toasts, Modifier.align(Alignment.CenterHorizontally).padding(bottom = 8.dp))
            if (composing) {
                TaskComposer(
                    if (state.isToday) "New task for today" else "New task in $title",
                    state.lists,
                    onAdd = { viewModel.quickAdd(it.text, it.dismissed) },
                    onClose = { composing = false },
                )
            }
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
            onRename = { renaming = it },
            onDelete = viewModel::deleteList,
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

/**
 * Drags [row] among its siblings, with its subtasks folded away until it's dropped. Dropping places
 * it before the next sibling, so completed siblings (shown elsewhere) don't throw it off.
 */
private fun startDrag(reorder: ReorderState<OutlineRow>, open: List<OutlineRow>, row: OutlineRow, viewModel: TasksViewModel) {
    val at = open.indexOfFirst { it.task.id == row.task.id }
    if (at < 0) return
    var end = at + 1
    while (end < open.size && open[end].depth > row.depth) end++
    reorder.start(
        row,
        shown = open.take(at + 1) + open.drop(end),
        canSwap = { it.task.parentId == row.task.parentId && it.depth == row.depth },
        depth = { it.depth.toInt() },
        onDrop = { order ->
            val i = order.indexOfFirst { it.task.id == row.task.id }
            val next = order.drop(i + 1).firstOrNull { it.task.parentId == row.task.parentId && it.depth == row.depth }
            viewModel.moveBefore(row.task, next?.task?.id)
        },
    )
}

/** The id and title of the list before or after the shown one, in the lists sheet's order. */
private fun neighbour(state: UiState, forward: Boolean): Pair<String, String>? {
    val lists = listOf(TODAY_ID to "Today") + state.lists.map { it.id to it.title }
    val at = lists.indexOfFirst { it.first == if (state.isToday) TODAY_ID else state.selectedListId }
    if (at < 0) return null
    return lists.getOrNull(if (forward) at + 1 else at - 1)
}

/** "3 tasks · by due date"; keeps its line when there are none, so the list below doesn't jump. */
@Composable
private fun Subtitle(state: UiState) {
    val open = state.outline.open.count { it.depth == 0u }
    val sort = state.selectedList?.sort?.takeUnless { state.isToday || it == TaskSort.MANUAL }
    Text(
        (if (open == 1) "1 task" else "$open tasks") + (sort?.let { " · by ${it.label.lowercase()}" } ?: ""),
        style = Not.type.body,
        color = Not.colors.inkMuted,
        modifier = Modifier.padding(top = 2.dp).alpha(if (open == 0) 0f else 1f),
    )
}

/** Swipe right to complete (or reopen), left to delete; tap for details. [listTitle] shows in Today. */
@Composable
private fun TaskRow(row: OutlineRow, viewModel: TasksViewModel, modifier: Modifier, listTitle: String? = null) {
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
            RoundCheck(
                done,
                { viewModel.toggle(row.task) },
                size = if (row.depth > 0u) 20.dp else 24.dp,
                ring = colors.priority(row.task.priority).takeUnless { row.done },
            )
            Column(Modifier.weight(1f).padding(start = 4.dp)) {
                Text(
                    row.task.title,
                    style = Not.type.body,
                    color = if (row.done) colors.inkFaint else colors.ink,
                    textDecoration = if (row.done) TextDecoration.LineThrough else null,
                )
                TaskMeta(row.task, done = row.done, listTitle = listTitle)
                val preview = row.task.notes.lineSequence().firstOrNull { it.isNotBlank() }
                if (preview != null) Text(preview, style = Not.type.caption, color = colors.inkMuted, maxLines = 1)
            }
        }
    }
}

/** "Tomorrow 9:00 ⟳ · Groceries" under a task's title: red once overdue, accent when due today. */
@Composable
private fun TaskMeta(task: TaskItem, done: Boolean, listTitle: String?) {
    val colors = Not.colors
    val due = task.due
    if (due == null && listTitle == null) return
    val today = today()
    val tint = when {
        done || due == null -> colors.inkFaint
        due < today -> colors.danger
        due == today -> colors.accentInk
        else -> colors.inkMuted
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (due != null) {
            Text(scheduleLabel(due, task.dueTime, null) ?: due, style = Not.type.caption, color = tint, maxLines = 1)
            if (task.repeat != null) Icon(R.drawable.ic_repeat, "Repeats", tint = tint, size = 13.dp)
        }
        if (listTitle != null) {
            Text(if (due != null) "· $listTitle" else listTitle, style = Not.type.caption, color = colors.inkFaint, maxLines = 1)
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

/** Every list and a new one: what the drawer used to hold. */
@Composable
private fun ListsSheet(
    state: UiState,
    onSelect: (String) -> Unit,
    onNewList: () -> Unit,
    onRename: (TaskList) -> Unit,
    onDelete: (TaskList) -> Unit,
    onDismiss: () -> Unit,
) {
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text("Lists", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 12.dp))
            ListRow(
                icon = R.drawable.ic_today,
                title = "Today",
                count = state.dueCount,
                selected = state.isToday,
                onClick = {
                    onSelect(TODAY_ID)
                    close()
                },
                menu = NoMenu,
            )
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
                    // The inbox is where tasks land, so it can't be renamed or deleted.
                    menu = if (list.id == INBOX_ID) NoMenu else {
                        {
                            ListMenu(
                                onRename = { onRename(list) },
                                onDelete = {
                                    // Close so the Undo toast underneath shows.
                                    close()
                                    onDelete(list)
                                },
                            )
                        }
                    },
                )
            }
            ListRow(R.drawable.ic_add, "New list", accent = true, onClick = onNewList)
        }
    }
}

@Composable
private fun ListRow(
    icon: Int,
    title: String,
    onClick: () -> Unit,
    count: Int = 0,
    selected: Boolean = false,
    accent: Boolean = false,
    menu: (@Composable () -> Unit)? = null,
) {
    val colors = Not.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(CircleShape)
            .background(if (selected) colors.accentSoft else Color.Transparent)
            .pressable(onClick, pressedScale = 0.98f)
            .heightIn(min = 52.dp)
            .padding(start = 14.dp, end = if (menu != null) 2.dp else 14.dp),
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
        menu?.invoke()
    }
}

/** Keeps a row's count in line with the rows that have a menu. */
private val NoMenu: @Composable () -> Unit = { Spacer(Modifier.size(44.dp)) }

@Composable
private fun ListMenu(onRename: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(R.drawable.ic_more_vert, "List options", { open = true }, tint = Not.colors.inkFaint)
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

private val TaskSort.label: String
    get() = when (this) {
        TaskSort.MANUAL -> "Manual"
        TaskSort.DUE -> "Due date"
        TaskSort.PRIORITY -> "Priority"
        TaskSort.TITLE -> "Title"
    }

/** Faint until the list is sorted; dragging a task in a sorted list switches it back to Manual. */
@Composable
private fun SortMenu(sort: TaskSort, onSort: (TaskSort) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(
            R.drawable.ic_sort,
            "Sort: ${sort.label}",
            { open = true },
            tint = if (sort == TaskSort.MANUAL) Not.colors.inkFaint else Not.colors.accentInk,
        )
        Menu(open, onDismiss = { open = false }) {
            TaskSort.entries.forEach { option ->
                MenuItem(option.label, checked = option == sort, onClick = {
                    open = false
                    onSort(option)
                })
            }
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
