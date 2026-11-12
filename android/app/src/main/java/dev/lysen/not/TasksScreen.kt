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
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.Spinner
import androidx.compose.ui.platform.LocalContext
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.ListShare
import androidx.compose.foundation.layout.height
import androidx.compose.ui.text.input.ImeAction
import dev.lysen.not.core.TaskSort
import dev.lysen.not.ui.ToastState
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.delay

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TasksScreen(viewModel: TasksViewModel, onOpenSettings: () -> Unit, onOpenHabit: (String) -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toasts = remember { ToastState() }
    var showsLists by remember { mutableStateOf(false) }
    var composing by rememberSaveable { mutableStateOf(false) }
    var showsCompleted by rememberSaveable { mutableStateOf(false) }
    var newList by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<TaskList?>(null) }
    var sharing by remember { mutableStateOf<TaskList?>(null) }
    // Held without dragging: quick actions for that task.
    var held by remember { mutableStateOf<TaskItem?>(null) }
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
                // Today and Upcoming have their own order: by date, then time.
                state.selectedList?.takeUnless { state.isDateView }?.let { list ->
                    SortMenu(list.sort) { viewModel.setSort(list.id, it) }
                }
            },
            header = { Subtitle(state) },
            pull = PullAction("Lists", R.drawable.ic_list) { showsLists = true },
            // Swiping the title bar steps through Today, Upcoming and the lists, in the order the sheet shows them.
            onSwipe = { forward ->
                neighbour(state, forward)?.let { (id, _) ->
                    haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                    viewModel.select(id)
                }
            },
            next = neighbour(state, forward = true)?.second,
        ) {
            val outline = state.outline
            // Today's habits lead, as rows like the tasks under them.
            if (state.isToday) {
                items(state.todayHabits, key = { "habit-${it.habit.id}" }) { summary ->
                    TodayHabitRow(summary, viewModel, Modifier.animateItem(), onOpen = { onOpenHabit(summary.habit.id) })
                }
            }
            if (outline.open.isEmpty() && !(state.isToday && state.todayHabits.isNotEmpty())) {
                // Fades in once the last row has left, instead of popping in under it.
                item(key = "empty") {
                    Box(Modifier.animateItem(fadeInSpec = tween(260, delayMillis = 180), fadeOutSpec = tween(120))) {
                    when {
                        state.isUpcoming ->
                            EmptyState(R.drawable.ic_calendar, "Nothing coming up", "Tasks with a date after today show here, by day.")
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
            if (state.isUpcoming) {
                byDay(outline.open).forEach { (label, rows) ->
                    item(key = "day-$label") { DayHeader(label, Modifier.animateItem()) }
                    items(rows, key = { it.task.id }) { row ->
                        TaskRow(row, viewModel, Modifier.animateItem(), listTitle = state.listTitle(row.task.listId), onHold = { held = row.task })
                    }
                }
            } else items(reorder.items, key = { it.task.id }) { row ->
                TaskRow(
                    row,
                    viewModel,
                    (if (reorder.isDragging(row)) Modifier else Modifier.animateItem())
                        // Today gathers tasks from every list, so it has no order of its own.
                        .reorderable(reorder, row, row.task.id, enabled = !state.isDateView && !row.done) {
                            startDrag(reorder, outline.open, row, viewModel) { held = row.task }
                        },
                    listTitle = if (state.isToday) state.listTitle(row.task.listId) else null,
                    // Today's rows don't drag, so holding them goes straight to the actions.
                    onHold = if (state.isToday) ({ held = row.task }) else null,
                    reorder = reorder,
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
                    when {
                        state.isToday -> "New task for today"
                        state.isUpcoming -> "New task for tomorrow"
                        else -> "New task in $title"
                    },
                    state.lists,
                    onAdd = { viewModel.quickAdd(it.text, it.dismissed) },
                    onClose = { composing = false },
                )
            }
        }
    }

    held?.let { task ->
        TaskActionsSheet(task, state, viewModel, onDismiss = { held = null })
    }
    state.openTask?.let { task ->
        TaskDetailSheet(task, state, viewModel, onDismiss = { viewModel.openTask(null) })
    }
    if (showsLists) {
        ListsSheet(
            state = state,
            onSelect = viewModel::select,
            onSearch = viewModel::search,
            onReveal = viewModel::reveal,
            onNewList = { newList = true },
            onRename = { renaming = it },
            onShare = { sharing = it },
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
    sharing?.let { list -> ShareDialog(list, state, viewModel, onDismiss = { sharing = null }) }
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
private fun startDrag(reorder: ReorderState<OutlineRow>, open: List<OutlineRow>, row: OutlineRow, viewModel: TasksViewModel, onHold: () -> Unit) {
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
        onHold = onHold,
    )
}

/** The id and title of the list before or after the shown one, in the lists sheet's order. */
private fun neighbour(state: UiState, forward: Boolean): Pair<String, String>? {
    val lists = listOf(TODAY_ID to "Today", UPCOMING_ID to "Upcoming") + state.lists.map { it.id to it.title }
    val at = lists.indexOfFirst { it.first == state.selectedListId }
    if (at < 0) return null
    return lists.getOrNull(if (forward) at + 1 else at - 1)
}

/** "3 tasks · by due date"; keeps its line when there are none, so the list below doesn't jump. */
/** Upcoming's groups: Tomorrow, then each day of the coming week by name, then Later. */
private fun byDay(rows: List<OutlineRow>): List<Pair<String, List<OutlineRow>>> {
    val today = java.time.LocalDate.now()
    return rows.groupBy { row ->
        val due = runCatching { java.time.LocalDate.parse(row.task.due) }.getOrNull()
        val days = due?.let { java.time.temporal.ChronoUnit.DAYS.between(today, it) } ?: Long.MAX_VALUE
        when {
            days == 1L -> "Tomorrow"
            days < 7 -> due!!.dayOfWeek.getDisplayName(java.time.format.TextStyle.FULL, java.util.Locale.getDefault())
            else -> "Later"
        }
    }.toList()
}

@Composable
private fun DayHeader(label: String, modifier: Modifier) {
    Text(
        label,
        style = Not.type.label,
        color = Not.colors.inkMuted,
        modifier = modifier.padding(start = 24.dp, top = 16.dp, bottom = 4.dp),
    )
}

@Composable
private fun Subtitle(state: UiState) {
    val open = state.outline.open.count { it.depth == 0u }
    val sort = state.selectedList?.sort?.takeUnless { state.isDateView || it == TaskSort.MANUAL }
    Text(
        (if (open == 1) "1 task" else "$open tasks") + (sort?.let { " · by ${it.label.lowercase()}" } ?: ""),
        style = Not.type.body,
        color = Not.colors.inkMuted,
        modifier = Modifier.padding(top = 2.dp).alpha(if (open == 0) 0f else 1f),
    )
}

/** Swipe right to complete (or reopen), left to delete; tap for details. [listTitle] shows in Today. */
@Composable
private fun TaskRow(
    row: OutlineRow,
    viewModel: TasksViewModel,
    modifier: Modifier,
    listTitle: String? = null,
    onHold: (() -> Unit)? = null,
    reorder: ReorderState<OutlineRow>? = null,
) {
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
                .pressable(
                    { if (reorder?.consumeHold(row.task.id) != true) viewModel.openTask(row.task.id) },
                    pressedScale = 0.985f,
                    // Rows that drag handle the hold themselves (see [reorder]).
                    onLongClick = onHold,
                    onLongClickLabel = onHold?.let { "Actions" },
                )
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

/**
 * A habit on Today, shaped like a task row: its ring checks in (the goal's progress shows in it),
 * swiping right checks in and left takes one back, tapping opens it on the Habits tab. Done ones
 * stay, struck through, like ticked subtasks.
 */
@Composable
private fun TodayHabitRow(summary: HabitSummary, viewModel: TasksViewModel, modifier: Modifier, onOpen: () -> Unit) {
    val haptics = LocalHapticFeedback.current
    val colors = Not.colors
    val habit = summary.habit
    val done = summary.doneToday
    SwipeRow(
        start = SwipeAction(R.drawable.ic_check, colors.accent, colors.onAccent) {
            if (done) return@SwipeAction
            haptics.performHapticFeedback(HapticFeedbackType.ToggleOn)
            viewModel.logHabit(habit.id, 1)
        },
        end = SwipeAction(R.drawable.ic_remove, colors.sunken, colors.ink) {
            if (summary.count == 0u) return@SwipeAction
            haptics.performHapticFeedback(HapticFeedbackType.ToggleOff)
            viewModel.logHabit(habit.id, -1)
        },
        modifier = modifier,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(colors.background)
                .pressable(onOpen, pressedScale = 0.985f, onClickLabel = "Open habit")
                .heightIn(min = 56.dp)
                .padding(start = 12.dp, end = 24.dp, top = 2.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            HabitRing(summary, onClick = {
                haptics.performHapticFeedback(if (done) HapticFeedbackType.ToggleOff else HapticFeedbackType.ToggleOn)
                viewModel.toggleHabit(habit.id)
            }, size = 24.dp)
            Column(Modifier.weight(1f).padding(start = 4.dp)) {
                Text(
                    habit.title,
                    style = Not.type.body,
                    color = if (done) colors.inkFaint else colors.ink,
                    textDecoration = if (done) TextDecoration.LineThrough else null,
                )
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Icon(R.drawable.ic_repeat, null, tint = colors.inkFaint, size = 13.dp)
                    Text(scheduleText(habit, summary), style = Not.type.caption, color = colors.inkFaint, maxLines = 1)
                    if (summary.streak > 0u && !done) Streak(summary.streak, habit.period)
                }
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
    onSearch: (String) -> List<TaskItem>,
    onReveal: (TaskItem) -> Unit,
    onNewList: () -> Unit,
    onRename: (TaskList) -> Unit,
    onShare: (TaskList) -> Unit,
    onDelete: (TaskList) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by rememberSaveable { mutableStateOf("") }
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text("Lists", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 12.dp))
            Field(
                query,
                { query = it },
                "Search tasks",
                filled = true,
                shape = CircleShape,
                leading = { Icon(R.drawable.ic_search, null, tint = Not.colors.inkFaint, size = 20.dp) },
                trailing = if (query.isEmpty()) null else ({ IconButton(R.drawable.ic_close, "Clear", { query = "" }, tint = Not.colors.inkFaint) }),
                imeAction = ImeAction.Search,
                modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
            )
            if (query.isNotBlank()) {
                val found = remember(query, state.lists, state.outline) { onSearch(query) }
                if (found.isEmpty()) {
                    Text("No tasks match “${query.trim()}”", style = Not.type.body, color = Not.colors.inkMuted, modifier = Modifier.padding(14.dp))
                }
                found.forEach { task ->
                    SearchResult(task, state.listTitle(task.listId)) {
                        onReveal(task)
                        close()
                    }
                }
                return@Column
            }
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
            ListRow(
                icon = R.drawable.ic_calendar,
                title = "Upcoming",
                count = state.upcomingCount,
                selected = state.isUpcoming,
                onClick = {
                    onSelect(UPCOMING_ID)
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
                                onShare = { onShare(list) },
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
private fun SearchResult(task: TaskItem, listTitle: String, onClick: () -> Unit) {
    val colors = Not.colors
    val done = task.completedAt != null
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .pressable(onClick, pressedScale = 0.98f)
            .heightIn(min = 52.dp)
            .padding(horizontal = 14.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(if (done) R.drawable.ic_check else R.drawable.ic_task_alt, null, tint = if (done) colors.inkFaint else colors.inkMuted, size = 20.dp)
        Column(Modifier.weight(1f).padding(start = 14.dp)) {
            Text(
                task.title,
                style = Not.type.body,
                color = if (done) colors.inkFaint else colors.ink,
                textDecoration = if (done) TextDecoration.LineThrough else null,
                maxLines = 1,
            )
            val due = task.due?.let { scheduleLabel(it, task.dueTime, null) }
            Text(listOfNotNull(listTitle, due).joinToString(" · "), style = Not.type.caption, color = colors.inkFaint, maxLines = 1)
        }
    }
}

/** What holding a task offers: move it to another list, or delete it. */
@Composable
private fun TaskActionsSheet(task: TaskItem, state: UiState, viewModel: TasksViewModel, onDismiss: () -> Unit) {
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text(task.title, style = Not.type.title, maxLines = 2, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 4.dp))
            Text("Move to", style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(start = 14.dp, top = 8.dp, bottom = 4.dp))
            // Subtasks move with their parent; on their own they only change place within it.
            state.lists.forEach { list ->
                ListRow(
                    icon = if (list.id == INBOX_ID) R.drawable.ic_inbox else R.drawable.ic_list,
                    title = list.title,
                    selected = list.id == task.listId,
                    onClick = {
                        if (task.parentId == null) viewModel.move(task, list.id)
                        close()
                    },
                )
            }
            Spacer(Modifier.height(8.dp))
            ListRow(R.drawable.ic_delete, "Delete", onClick = {
                close()
                viewModel.delete(task)
            })
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
private fun ListMenu(onRename: () -> Unit, onShare: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(R.drawable.ic_more_vert, "List options", { open = true }, tint = Not.colors.inkFaint)
        Menu(open, onDismiss = { open = false }) {
            MenuItem("Rename list", icon = R.drawable.ic_edit, onClick = {
                open = false
                onRename()
            })
            MenuItem("Share…", icon = R.drawable.ic_copy, onClick = {
                open = false
                onShare()
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
 * Links that show [list] in a browser, for people without the app (a partner with the groceries).
 * Each link is read only or lets visitors tick off and add tasks; the sharer picks per link.
 */
@Composable
private fun ShareDialog(list: TaskList, state: UiState, viewModel: TasksViewModel, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val shares by viewModel.shares.collectAsStateWithLifecycle()
    LaunchedEffect(list.id) { if (state.syncInfo != null) viewModel.loadShares(list.id) }
    fun send(share: ListShare) {
        val intent = android.content.Intent(android.content.Intent.ACTION_SEND)
            .setType("text/plain")
            .putExtra(android.content.Intent.EXTRA_TEXT, share.url)
        context.startActivity(android.content.Intent.createChooser(intent, "Share “${list.title}”"))
    }
    CardDialog(
        title = "Share “${list.title}”",
        onDismiss = onDismiss,
        buttons = { Button("Done", onDismiss, kind = ButtonKind.Quiet) },
    ) {
        if (state.syncInfo == null) {
            Text("Links are served by your sync server: pair this phone first (Settings → Sync).", color = Not.colors.inkMuted)
            return@CardDialog
        }
        Text("Anyone with a link sees this list in a browser, no app needed.", style = Not.type.caption, color = Not.colors.inkMuted)
        when (val links = shares) {
            null -> Spinner(Modifier.padding(vertical = 12.dp))
            else -> links.forEach { share ->
                Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(if (share.canEdit) "Can tick off and add" else "View only", style = Not.type.body)
                        Text(share.url.substringAfter("://"), style = Not.type.caption, color = Not.colors.inkFaint, maxLines = 1)
                    }
                    IconButton(R.drawable.ic_copy, "Send link", { send(share) }, tint = Not.colors.inkMuted)
                    IconButton(R.drawable.ic_delete, "Stop sharing", { viewModel.revokeShare(share) }, tint = Not.colors.danger)
                }
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
            Button("Can edit", { viewModel.shareList(list.id, canEdit = true, ::send) }, kind = ButtonKind.Secondary, icon = R.drawable.ic_add, enabled = !state.busy)
            Button("View only", { viewModel.shareList(list.id, canEdit = false, ::send) }, kind = ButtonKind.Secondary, icon = R.drawable.ic_add, enabled = !state.busy)
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
