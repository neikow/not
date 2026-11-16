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
import dev.lysen.not.core.TaskView
import dev.lysen.not.core.TagInfo
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
    // A smart list being made (an empty one) or edited; its query may start from a tag.
    var editingView by remember { mutableStateOf<ViewDraft?>(null) }
    var renamingTag by remember { mutableStateOf<String?>(null) }
    var sharing by remember { mutableStateOf<TaskView?>(null) }
    var processing by rememberSaveable { mutableStateOf(false) }
    var reviewing by rememberSaveable { mutableStateOf(false) }
    var showsLogbook by rememberSaveable { mutableStateOf(false) }
    // Held without dragging: quick actions for that task.
    var held by remember { mutableStateOf<TaskItem?>(null) }
    val calendar = rememberCalendarEvents(state.selectedViewId)
    val title = state.title
    val haptics = LocalHapticFeedback.current
    // The title is a long reach: tapping or holding Tasks in the tab bar opens the lists too.
    OnTabGesture { showsLists = true }

    LaunchedEffect(Unit) { viewModel.reviewRequests.collect { reviewing = true } }
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
                // Upcoming is always by date.
                if (!state.isUpcoming) SortMenu(state.sort) { viewModel.setSort(state.selectedViewId, it) }
            },
            header = { Subtitle(state, onProcess = { processing = true }) },
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
            if (state.earlier.isNotEmpty()) {
                item(key = "earlier") {
                    EarlierBanner(state.earlier.size, Modifier.animateItem()) { viewModel.moveToToday(state.earlier) }
                }
            }
            // The day's calendar, read only, above what there is to do.
            if (state.isToday) {
                items(calendar.today(), key = { "event-${it.key}" }) { event -> EventRow(event, Modifier.animateItem()) }
            }
            // Today's habits lead, as rows like the tasks under them.
            if (state.isToday) {
                items(state.todayHabits, key = { "habit-${it.habit.id}" }) { summary ->
                    TodayHabitRow(summary, viewModel, Modifier.animateItem(), onOpen = { onOpenHabit(summary.habit.id) })
                }
            }
            if (outline.open.isEmpty() && !(state.isToday && (state.todayHabits.isNotEmpty() || calendar.today().isNotEmpty()))) {
                // Fades in once the last row has left, instead of popping in under it.
                item(key = "empty") {
                    Box(Modifier.animateItem(fadeInSpec = tween(260, delayMillis = 180), fadeOutSpec = tween(120))) {
                    when {
                        state.isUpcoming ->
                            EmptyState(R.drawable.ic_calendar, "Nothing coming up", "Tasks starting or due after today show here, by day.")
                        state.isToday ->
                            EmptyState(R.drawable.ic_today, "Nothing for today", "Add “today”, “fri 9am”, “by mon” or “every mon” to a task to see it here on the day.")
                        state.isSomeday ->
                            EmptyState(R.drawable.ic_someday, "Nothing for someday", "Park ideas and maybes here with “someday”, out of the way until you want them.")
                        outline.completed.isNotEmpty() ->
                            EmptyState(R.drawable.ic_check, "All done", "Everything in $title is checked off.", Modifier.padding(bottom = 24.dp))
                        state.isInbox ->
                            EmptyState(R.drawable.ic_inbox, "Inbox zero", "Anything you jot down without a #tag lands here first.")
                        else -> EmptyState(R.drawable.ic_list, "Nothing in $title yet", "Tap + to add the first one: it gets what this list shows.")
                    }
                    }
                }
            }
            if (state.isUpcoming) {
                byDay(outline.open, calendar).forEach { (label, rows, events) ->
                    item(key = "day-$label") { DayHeader(label, Modifier.animateItem()) }
                    items(events, key = { "event-${it.key}" }) { event -> EventRow(event, Modifier.animateItem()) }
                    items(rows, key = { it.task.id }) { row ->
                        TaskRow(row, viewModel, Modifier.animateItem(), hideTag = null, onHold = { held = row.task })
                    }
                }
            } else items(reorder.items, key = { it.task.id }) { row ->
                TaskRow(
                    row,
                    viewModel,
                    (if (reorder.isDragging(row)) Modifier else Modifier.animateItem())
                        .reorderable(reorder, row, row.task.id, enabled = !row.done) {
                            startDrag(reorder, outline.open, row, viewModel) { held = row.task }
                        },
                    // The tag a view is about goes without saying on its rows.
                    hideTag = state.selectedViewId.removePrefix("#").takeIf { state.isTag } ?: soleTag(state.selectedView?.query),
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
                        TaskRow(row, viewModel, Modifier.animateItem(), hideTag = null)
                    }
                }
            }
        }

        // The + beside the tab bar opens the composer above the keyboard.
        AddAction("New task", enabled = !composing, holdLabel = "New list", onHold = { editingView = ViewDraft() }) { composing = true }
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
                        state.isUpcoming -> "New task — say when, like “fri”"
                        state.isSomeday -> "Someday…"
                        state.isInbox -> "New task — try “call mum fri 6pm #home”"
                        else -> "New task in $title"
                    },
                    onAdd = { viewModel.quickAdd(it.text, it.dismissed) },
                    onPaste = { viewModel.quickAdd(it) },
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
            onReveal = { viewModel.openTask(it.id) },
            onNewList = { editingView = ViewDraft() },
            onEdit = { editingView = ViewDraft(it.id, it.title, it.query) },
            onShare = { sharing = it },
            onDelete = viewModel::deleteView,
            onSaveTag = { editingView = ViewDraft(title = it.replaceFirstChar(Char::uppercase), query = "#$it") },
            onRenameTag = { renamingTag = it },
            onDeleteTag = viewModel::deleteTag,
            onReview = { reviewing = true },
            onLogbook = { showsLogbook = true },
            onDismiss = { showsLists = false },
        )
    }
    editingView?.let { draft ->
        ViewEditor(draft, state, viewModel, onDone = {
            editingView = null
            showsLists = false
        }, onDismiss = { editingView = null })
    }
    renamingTag?.let { tag ->
        PromptDialog(
            title = "Rename #$tag",
            confirm = "Rename",
            initial = tag,
            onConfirm = {
                viewModel.renameTag(tag, it)
                renamingTag = null
            },
            onDismiss = { renamingTag = null },
        )
    }
    sharing?.let { view -> ShareDialog(view, state, viewModel, onDismiss = { sharing = null }) }
    if (processing) InboxFlow(viewModel, onDismiss = { processing = false })
    if (reviewing) ReviewFlow(viewModel, onProcessInbox = { processing = true }, onDismiss = { reviewing = false })
    if (showsLogbook) LogbookSheet(viewModel, onDismiss = { showsLogbook = false })
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
        // Top-level rows reorder within the view; subtasks among their siblings.
        canSwap = { it.depth == row.depth && (row.depth == 0u || it.task.parentId == row.task.parentId) },
        depth = { it.depth.toInt() },
        onDrop = { order ->
            val i = order.indexOfFirst { it.task.id == row.task.id }
            val next = order.drop(i + 1).firstOrNull { it.depth == row.depth && (row.depth == 0u || it.task.parentId == row.task.parentId) }
            viewModel.moveBefore(row.task, next?.task?.id)
        },
        onHold = onHold,
    )
}

/** The id and title of the view before or after the shown one, in the lists sheet's order. */
private fun neighbour(state: UiState, forward: Boolean): Pair<String, String>? {
    val views = state.views.map { it.id to it.title }
    val at = views.indexOfFirst { it.first == state.selectedViewId }
    if (at < 0) return null
    return views.getOrNull(if (forward) at + 1 else at - 1)
}

/** The tag a smart list is about when it names just one (`#work`, `#work !1`). */
private fun soleTag(query: String?): String? {
    val tags = query.orEmpty().split(' ').filter { it.startsWith("#") }
    return tags.singleOrNull()?.removePrefix("#")?.lowercase()
}

/** The date a task shows under in Upcoming: its start once it's ahead, else its deadline. */
private fun upcomingDate(task: TaskItem): java.time.LocalDate? {
    val today = today()
    val date = task.start?.takeIf { it > today } ?: task.deadline
    return date?.let { runCatching { java.time.LocalDate.parse(it) }.getOrNull() }
}

/** Upcoming's groups: Tomorrow, then each day of the coming week by name, then Later; with each day's events. */
private fun byDay(rows: List<OutlineRow>, calendar: CalendarEvents): List<Triple<String, List<OutlineRow>, List<CalendarEvent>>> {
    val today = java.time.LocalDate.now()
    fun label(date: java.time.LocalDate?): String {
        val days = date?.let { java.time.temporal.ChronoUnit.DAYS.between(today, it) } ?: Long.MAX_VALUE
        return when {
            days == 1L -> "Tomorrow"
            days < 7 -> date!!.dayOfWeek.getDisplayName(java.time.format.TextStyle.FULL, java.util.Locale.getDefault())
            else -> "Later"
        }
    }
    val tasks = rows.groupBy { row -> label(upcomingDate(row.task)) }
    val events = calendar.week().groupBy { label(it.day) }
    val order = (1..6).map { label(today.plusDays(it.toLong())) } + "Later"
    return order.mapNotNull { day ->
        val t = tasks[day].orEmpty()
        val e = events[day].orEmpty()
        if (t.isEmpty() && e.isEmpty()) null else Triple(day, t, e)
    }
}

/** "3 from earlier days · Move to today": what slipped, dealt with in one tap. */
@Composable
private fun EarlierBanner(count: Int, modifier: Modifier, onMove: () -> Unit) {
    val colors = Not.colors
    Row(
        modifier
            .padding(horizontal = 16.dp, vertical = 6.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(colors.sunken)
            .padding(start = 16.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(R.drawable.ic_history, null, tint = colors.inkMuted, size = 20.dp)
        Text(
            if (count == 1) "1 from an earlier day" else "$count from earlier days",
            style = Not.type.body,
            color = colors.inkMuted,
            modifier = Modifier.weight(1f).padding(start = 10.dp),
        )
        Button("Move to today", onMove, kind = ButtonKind.Quiet)
    }
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
private fun Subtitle(state: UiState, onProcess: () -> Unit) {
    val open = state.outline.open.count { it.depth == 0u }
    val sort = state.sort.takeUnless { state.isUpcoming || it == TaskSort.MANUAL || (state.isToday && it == TaskSort.DUE) }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 2.dp).alpha(if (open == 0) 0f else 1f)) {
        Text(
            (if (open == 1) "1 task" else "$open tasks") + (sort?.let { " · by ${it.label.lowercase()}" } ?: ""),
            style = Not.type.body,
            color = Not.colors.inkMuted,
        )
        // One at a time through the Inbox: give each a day, a tag, or let it go.
        if (state.isInbox && open > 0) {
            Spacer(Modifier.width(10.dp))
            Chip(R.drawable.ic_play, "Process", onClick = onProcess)
        }
    }
}

/** Swipe right to complete (or reopen), left to delete; tap for details. [hideTag] goes unsaid. */
@Composable
private fun TaskRow(
    row: OutlineRow,
    viewModel: TasksViewModel,
    modifier: Modifier,
    hideTag: String?,
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
                TaskMeta(row.task, done = row.done, hideTag = hideTag, subtasks = row.subtasks to row.subtasksDone)
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

/**
 * "Tomorrow 9:00 ⟳ · Due Fri · 2/5 · #home" under a task's title. A deadline turns accent on the day and
 * red once passed; a start date in the past shows faint (it's in Today anyway).
 */
@Composable
fun TaskMeta(task: TaskItem, done: Boolean, hideTag: String? = null, subtasks: Pair<UInt, UInt> = 0u to 0u) {
    val colors = Not.colors
    val today = today()
    val tags = task.tags.filter { it != hideTag }
    val (total, finished) = subtasks
    if (task.start == null && task.deadline == null && !task.someday && tags.isEmpty() && total == 0u) return
    val faint = colors.inkFaint
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        val parts = mutableListOf<@Composable () -> Unit>()
        task.start?.let { start ->
            val tint = when {
                done -> faint
                start == today -> colors.accentInk
                else -> colors.inkMuted
            }
            parts += {
                Text(scheduleLabel(start, task.time, null) ?: start, style = Not.type.caption, color = tint, maxLines = 1)
                if (task.repeat != null) Icon(R.drawable.ic_repeat, "Repeats", tint = tint, size = 13.dp)
            }
        }
        if (task.someday) parts += { Text("Someday", style = Not.type.caption, color = faint, maxLines = 1) }
        task.deadline?.let { deadline ->
            val tint = when {
                done -> faint
                deadline < today -> colors.danger
                deadline == today -> colors.accentInk
                else -> colors.inkMuted
            }
            parts += {
                Icon(R.drawable.ic_deadline, "Deadline", tint = tint, size = 13.dp)
                Text(scheduleLabel(deadline, null, null) ?: deadline, style = Not.type.caption, color = tint, maxLines = 1)
                if (task.repeat != null && task.start == null) Icon(R.drawable.ic_repeat, "Repeats", tint = tint, size = 13.dp)
            }
        }
        if (total > 0u) parts += { Text("$finished/$total", style = Not.type.caption, color = faint, maxLines = 1) }
        if (tags.isNotEmpty()) parts += { Text(tags.joinToString(" ") { "#$it" }, style = Not.type.caption, color = faint, maxLines = 1) }
        parts.forEachIndexed { i, part ->
            if (i > 0) Text("·", style = Not.type.caption, color = faint)
            part()
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

/** Every view, every tag, and the ways to make more: what the drawer used to hold. */
@Composable
private fun ListsSheet(
    state: UiState,
    onSelect: (String) -> Unit,
    onSearch: (String) -> List<TaskItem>,
    onReveal: (TaskItem) -> Unit,
    onNewList: () -> Unit,
    onEdit: (TaskView) -> Unit,
    onShare: (TaskView) -> Unit,
    onDelete: (TaskView) -> Unit,
    onSaveTag: (String) -> Unit,
    onRenameTag: (String) -> Unit,
    onDeleteTag: (String) -> Unit,
    onReview: () -> Unit,
    onLogbook: () -> Unit,
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
                val found = remember(query, state.views, state.outline) { onSearch(query) }
                if (found.isEmpty()) {
                    Text("No tasks match “${query.trim()}”", style = Not.type.body, color = Not.colors.inkMuted, modifier = Modifier.padding(14.dp))
                }
                found.forEach { task ->
                    SearchResult(task) {
                        onReveal(task)
                        close()
                    }
                }
                return@Column
            }
            fun pick(id: String) {
                onSelect(id)
                close()
            }
            state.views.filter { it.builtIn }.forEach { view ->
                ListRow(
                    icon = builtInIcon(view.id),
                    title = view.title,
                    count = view.openCount.toInt(),
                    selected = view.id == state.selectedViewId,
                    onClick = { pick(view.id) },
                    menu = NoMenu,
                )
            }
            val saved = state.views.filter { !it.builtIn }
            SectionLabel("Smart lists")
            saved.forEach { view ->
                ListRow(
                    icon = R.drawable.ic_list,
                    title = view.title,
                    count = view.openCount.toInt(),
                    selected = view.id == state.selectedViewId,
                    onClick = { pick(view.id) },
                    menu = {
                        ListMenu(
                            onEdit = { onEdit(view) },
                            onShare = { onShare(view) },
                            onDelete = {
                                // Close so the Undo toast underneath shows.
                                close()
                                onDelete(view)
                            },
                        )
                    },
                )
            }
            ListRow(R.drawable.ic_add, "New smart list", accent = true, onClick = onNewList)
            if (state.tags.isNotEmpty()) {
                SectionLabel("Tags")
                state.tags.forEach { tag ->
                    ListRow(
                        icon = R.drawable.ic_tag,
                        title = tag.name,
                        count = tag.openCount.toInt(),
                        selected = state.selectedViewId == "#${tag.name}",
                        onClick = { pick("#${tag.name}") },
                        menu = {
                            TagMenu(
                                saved = saved.any { soleTag(it.query) == tag.name },
                                onSave = { onSaveTag(tag.name) },
                                onRename = { onRenameTag(tag.name) },
                                onDelete = {
                                    close()
                                    onDeleteTag(tag.name)
                                },
                            )
                        },
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
            Divider(Modifier.padding(horizontal = 14.dp, vertical = 4.dp))
            ListRow(R.drawable.ic_review, "Weekly review", onClick = {
                close()
                onReview()
            })
            ListRow(R.drawable.ic_history, "Logbook", onClick = {
                close()
                onLogbook()
            })
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(text, style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(start = 14.dp, top = 16.dp, bottom = 4.dp))
}

fun builtInIcon(id: String): Int = when (id) {
    TODAY_ID -> R.drawable.ic_today
    UPCOMING_ID -> R.drawable.ic_calendar
    SOMEDAY_ID -> R.drawable.ic_someday
    else -> R.drawable.ic_inbox
}

@Composable
private fun SearchResult(task: TaskItem, onClick: () -> Unit) {
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
            TaskMeta(task, done)
        }
    }
}

@Composable
internal fun ListRow(
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
private fun ListMenu(onEdit: () -> Unit, onShare: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(R.drawable.ic_more_vert, "List options", { open = true }, tint = Not.colors.inkFaint)
        Menu(open, onDismiss = { open = false }) {
            MenuItem("Edit…", icon = R.drawable.ic_edit, onClick = {
                open = false
                onEdit()
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

@Composable
private fun TagMenu(saved: Boolean, onSave: () -> Unit, onRename: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(R.drawable.ic_more_vert, "Tag options", { open = true }, tint = Not.colors.inkFaint)
        Menu(open, onDismiss = { open = false }) {
            if (!saved) {
                MenuItem("Make a smart list", icon = R.drawable.ic_list, onClick = {
                    open = false
                    onSave()
                })
            }
            MenuItem("Rename tag", icon = R.drawable.ic_edit, onClick = {
                open = false
                onRename()
            })
            MenuItem("Remove from all tasks", icon = R.drawable.ic_delete, danger = true, onClick = {
                open = false
                onDelete()
            })
        }
    }
}

private val TaskSort.label: String
    get() = when (this) {
        TaskSort.MANUAL -> "Manual"
        TaskSort.DUE -> "Date"
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
private fun ShareDialog(list: TaskView, state: UiState, viewModel: TasksViewModel, onDismiss: () -> Unit) {
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
