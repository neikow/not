package dev.lysen.not

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.NavigationDrawerItemDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberDrawerState
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.semantics.Role
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.OutlineRow
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.TaskList
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TasksScreen(viewModel: TasksViewModel, onOpenSettings: () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    var newList by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<TaskList?>(null) }
    val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()

    // Destructive actions confirm after the fact, with Undo, instead of asking first.
    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            snackbar.currentSnackbarData?.dismiss()
            val result = snackbar.showSnackbar(undoable.message, actionLabel = "Undo", duration = SnackbarDuration.Short)
            if (result == SnackbarResult.ActionPerformed) viewModel.undo()
        }
    }

    ModalNavigationDrawer(
        drawerState = drawer,
        drawerContent = {
            ListsDrawer(
                state = state,
                onSelect = {
                    viewModel.select(it)
                    scope.launch { drawer.close() }
                },
                onNewList = { newList = true },
                onSettings = {
                    scope.launch { drawer.close() }
                    onOpenSettings()
                },
            )
        },
    ) {
        Scaffold(
            modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
            topBar = {
                LargeTopAppBar(
                    scrollBehavior = scroll,
                    navigationIcon = {
                        IconButton(onClick = { scope.launch { drawer.open() } }) {
                            Icon(painterResource(R.drawable.ic_menu), "Lists")
                        }
                    },
                    title = { Text(state.selectedList?.title ?: "Inbox") },
                    actions = {
                        SyncDot(state.sync)
                        val list = state.selectedList
                        if (list != null && list.id != INBOX_ID) {
                            ListMenu(onRename = { renaming = list }, onDelete = { viewModel.deleteList(list) })
                        } else {
                            Spacer(Modifier.width(16.dp))
                        }
                    },
                )
            },
            snackbarHost = { SnackbarHost(snackbar) },
            bottomBar = { AddBar("Add to ${state.selectedList?.title ?: "Inbox"}", onAdd = { viewModel.addTask(it) }) },
        ) { padding ->
            TaskList(state, viewModel, Modifier.padding(padding))
        }
    }

    state.openTask?.let { task ->
        TaskDetailSheet(task, state, viewModel, onDismiss = { viewModel.openTask(null) })
    }
    if (newList) {
        TextPromptDialog(
            title = "New list",
            confirm = "Create",
            onConfirm = {
                viewModel.addList(it)
                newList = false
                scope.launch { drawer.close() }
            },
            onDismiss = { newList = false },
        )
    }
    renaming?.let { list ->
        TextPromptDialog(
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
        AlertDialog(
            onDismissRequest = viewModel::dismissError,
            confirmButton = { TextButton(onClick = viewModel::dismissError) { Text("OK") } },
            title = { Text("Something went wrong") },
            text = { Text(it) },
        )
    }
}

@Composable
private fun ListsDrawer(
    state: UiState,
    onSelect: (String) -> Unit,
    onNewList: () -> Unit,
    onSettings: () -> Unit,
) {
    ModalDrawerSheet {
        Text(
            "not",
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.padding(horizontal = 28.dp, vertical = 20.dp),
        )
        state.lists.forEachIndexed { index, list ->
            if (index == 1) {
                Text(
                    "Lists",
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 28.dp, top = 16.dp, bottom = 8.dp),
                )
            }
            NavigationDrawerItem(
                label = { Text(list.title) },
                icon = {
                    Icon(painterResource(if (list.id == INBOX_ID) R.drawable.ic_inbox else R.drawable.ic_list), null)
                },
                badge = { if (list.openCount > 0u) Text(list.openCount.toString()) },
                selected = list.id == state.selectedListId,
                onClick = { onSelect(list.id) },
                modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding),
            )
        }
        NavigationDrawerItem(
            label = { Text("New list") },
            icon = { Icon(painterResource(R.drawable.ic_add), null) },
            selected = false,
            onClick = onNewList,
            modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding),
        )
        HorizontalDivider(Modifier.padding(horizontal = 28.dp, vertical = 8.dp))
        NavigationDrawerItem(
            label = { Text("Settings") },
            icon = { Icon(painterResource(R.drawable.ic_settings), null) },
            selected = false,
            onClick = onSettings,
            modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding),
        )
    }
}

@Composable
private fun ListMenu(onRename: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) { Icon(painterResource(R.drawable.ic_more_vert), "List options") }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(text = { Text("Rename list") }, onClick = {
                open = false
                onRename()
            })
            DropdownMenuItem(text = { Text("Delete list") }, onClick = {
                open = false
                onDelete()
            })
        }
    }
}

@Composable
private fun TaskList(state: UiState, viewModel: TasksViewModel, modifier: Modifier) {
    var showsCompleted by rememberSaveable { mutableStateOf(false) }
    val outline = state.outline
    if (outline.open.isEmpty() && outline.completed.isEmpty()) {
        EmptyState(R.drawable.ic_task_alt, "Nothing here yet", "Add a task below", modifier)
        return
    }
    LazyColumn(modifier.fillMaxSize(), contentPadding = PaddingValues(vertical = 4.dp)) {
        items(outline.open, key = { it.task.id }) { row ->
            SwipeableTaskRow(row, viewModel, Modifier.animateItem())
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
                    SwipeableTaskRow(row, viewModel, Modifier.animateItem())
                }
            }
        }
    }
}

/** Swipe right to complete (or reopen), left to delete; tap for details. */
@Composable
private fun SwipeableTaskRow(row: OutlineRow, viewModel: TasksViewModel, modifier: Modifier) {
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val swipe = rememberSwipeToDismissBoxState()
    val done = row.task.completedAt != null

    fun toggle() {
        haptics.performHapticFeedback(if (done) HapticFeedbackType.ToggleOff else HapticFeedbackType.ToggleOn)
        viewModel.toggle(row.task)
    }

    SwipeToDismissBox(
        state = swipe,
        modifier = modifier,
        backgroundContent = { SwipeBackground(swipe.dismissDirection, done) },
        onDismiss = { direction ->
            scope.launch {
                // Reset before acting: the swipe state is saved per row, and a row that comes back
                // (Undo, or moving to Completed) would otherwise be dismissed again.
                swipe.reset()
                when (direction) {
                    SwipeToDismissBoxValue.StartToEnd -> toggle()
                    SwipeToDismissBoxValue.EndToStart -> {
                        haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                        viewModel.delete(row.task)
                    }
                    SwipeToDismissBoxValue.Settled -> {}
                }
            }
        },
    ) {
        Surface(color = MaterialTheme.colorScheme.surface) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .clickable { viewModel.openTask(row.task.id) }
                    .heightIn(min = 56.dp)
                    .padding(start = (8 + row.depth.toInt() * 32).dp, end = 16.dp, top = 6.dp, bottom = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RoundCheck(checked = done, onClick = ::toggle, small = row.depth > 0u)
                Column(Modifier.weight(1f).padding(start = 8.dp)) {
                    Text(
                        row.task.title,
                        style = MaterialTheme.typography.bodyLarge,
                        textDecoration = if (row.done) TextDecoration.LineThrough else null,
                        color = if (row.done) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
                    )
                    val preview = row.task.notes.lineSequence().firstOrNull { it.isNotBlank() }
                    if (preview != null) {
                        Text(
                            preview,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

/** A round checkbox, filled when checked. 48dp touch target. */
@Composable
private fun RoundCheck(checked: Boolean, onClick: () -> Unit, small: Boolean = false) {
    val colors = MaterialTheme.colorScheme
    val size = if (small) 20.dp else 24.dp
    Box(
        Modifier
            .size(48.dp)
            .clip(CircleShape)
            .toggleable(value = checked, role = Role.Checkbox, onValueChange = { onClick() }),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(size)
                .border(2.dp, if (checked) colors.primary else colors.outline, CircleShape)
                .background(if (checked) colors.primary else Color.Transparent, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (checked) Icon(painterResource(R.drawable.ic_check), null, tint = colors.onPrimary, modifier = Modifier.size(size - 6.dp))
        }
    }
}

@Composable
private fun SwipeBackground(direction: SwipeToDismissBoxValue, done: Boolean) {
    val (color, icon, alignment) = when (direction) {
        SwipeToDismissBoxValue.StartToEnd -> Triple(
            MaterialTheme.colorScheme.primaryContainer,
            if (done) R.drawable.ic_add_task else R.drawable.ic_check,
            Alignment.CenterStart,
        )
        SwipeToDismissBoxValue.EndToStart ->
            Triple(MaterialTheme.colorScheme.errorContainer, R.drawable.ic_delete, Alignment.CenterEnd)
        SwipeToDismissBoxValue.Settled -> Triple(Color.Transparent, null, Alignment.Center)
    }
    Box(
        Modifier
            .fillMaxSize()
            .background(color)
            .padding(horizontal = 24.dp),
        contentAlignment = alignment,
    ) {
        if (icon != null) Icon(painterResource(icon), null)
    }
}

@Composable
private fun CompletedHeader(count: Int, expanded: Boolean, onToggle: () -> Unit, modifier: Modifier) {
    val rotation by animateFloatAsState(if (expanded) 180f else 0f, label = "chevron")
    Row(
        modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle)
            .padding(horizontal = 20.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "Completed · $count",
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.weight(1f),
        )
        Icon(
            painterResource(R.drawable.ic_expand_more),
            if (expanded) "Hide completed" else "Show completed",
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.rotate(rotation),
        )
    }
}

@Composable
private fun SyncDot(state: SyncState?) {
    val color = when (state) {
        null -> Color.Gray
        SyncState.Syncing -> Color(0xFF2196F3)
        SyncState.Synced -> Color(0xFF4CAF50)
        is SyncState.Offline -> Color(0xFFFF9800)
        SyncState.Unauthorized -> Color(0xFFF44336)
    }
    Box(Modifier.padding(horizontal = 8.dp).size(8.dp).background(color, CircleShape))
}

@Composable
fun TextPromptDialog(
    title: String,
    confirm: String,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
    initial: String = "",
) {
    var text by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(
                text,
                { text = it },
                singleLine = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            )
        },
        confirmButton = { TextButton(onClick = { if (text.isNotBlank()) onConfirm(text) }) { Text(confirm) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
