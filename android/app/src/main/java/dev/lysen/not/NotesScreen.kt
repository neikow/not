package dev.lysen.not

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Folder
import dev.lysen.not.core.NoteSummary
import kotlinx.coroutines.delay

/** Notes of a folder (or all), with search; opening a note shows the editor. */
@Composable
fun NotesScreen(viewModel: NotesViewModel, navigationIcon: @Composable () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }

    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            snackbar.currentSnackbarData?.dismiss()
            val result = snackbar.showSnackbar(undoable.message, actionLabel = "Undo", duration = SnackbarDuration.Short)
            if (result == SnackbarResult.ActionPerformed) viewModel.undo()
        }
    }

    val open = state.openNote
    if (open != null) {
        BackHandler { viewModel.open(null) }
        NoteEditor(open, state, viewModel, snackbar)
    } else {
        NoteList(state, viewModel, snackbar, navigationIcon)
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NoteList(
    state: NotesState,
    viewModel: NotesViewModel,
    snackbar: SnackbarHostState,
    navigationIcon: @Composable () -> Unit,
) {
    var newFolder by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<Folder?>(null) }
    var moving by remember { mutableStateOf<NoteSummary?>(null) }
    val searching = state.query != null
    if (searching) BackHandler { viewModel.search(null) }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = navigationIcon,
                title = {
                    if (searching) {
                        val focus = remember { FocusRequester() }
                        TextField(
                            state.query.orEmpty(),
                            viewModel::search,
                            placeholder = { Text("Search notes") },
                            singleLine = true,
                            colors = TextFieldDefaults.colors(
                                focusedContainerColor = Color.Transparent,
                                unfocusedContainerColor = Color.Transparent,
                                focusedIndicatorColor = Color.Transparent,
                                unfocusedIndicatorColor = Color.Transparent,
                            ),
                            modifier = Modifier.fillMaxWidth().focusRequester(focus),
                        )
                        LaunchedEffect(Unit) { focus.requestFocus() }
                    } else {
                        Text(state.folder?.title ?: "Notes")
                    }
                },
                actions = {
                    if (searching) {
                        IconButton(onClick = { viewModel.search(null) }) { Icon(painterResource(R.drawable.ic_close), "Close search") }
                    } else {
                        IconButton(onClick = { viewModel.search("") }) { Icon(painterResource(R.drawable.ic_search), "Search") }
                        state.folder?.let { folder ->
                            FolderMenu(onRename = { renaming = folder }, onDelete = { viewModel.deleteFolder(folder) })
                        }
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = viewModel::newNote,
                icon = { Icon(painterResource(R.drawable.ic_edit), null) },
                text = { Text("New note") },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            if (!searching) {
                FolderChips(state, viewModel, onNewFolder = { newFolder = true })
            }
            if (state.notes.isEmpty()) {
                EmptyState(
                    if (searching) R.drawable.ic_search else R.drawable.ic_note,
                    if (searching) "No results" else "No notes yet",
                    if (searching) "Try other words" else "Tap New note to write one",
                )
            } else {
                LazyColumn(
                    contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 4.dp, bottom = 96.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(state.notes, key = { it.id }) { note ->
                        NoteCard(
                            note,
                            folder = if (state.folderId == null && note.folderId != null) state.folderTitle(note.folderId) else null,
                            viewModel = viewModel,
                            onMove = { moving = note },
                            modifier = Modifier.animateItem(),
                        )
                    }
                }
            }
        }
    }

    if (newFolder) {
        TextPromptDialog(
            title = state.folder?.let { "New folder in ${it.title}" } ?: "New folder",
            confirm = "Create",
            onConfirm = {
                viewModel.addFolder(it)
                newFolder = false
            },
            onDismiss = { newFolder = false },
        )
    }
    renaming?.let { folder ->
        TextPromptDialog(
            title = "Rename folder",
            confirm = "Rename",
            initial = folder.title,
            onConfirm = {
                viewModel.renameFolder(folder.id, it)
                renaming = null
            },
            onDismiss = { renaming = null },
        )
    }
    moving?.let { note -> MoveDialog(note, state.folders, viewModel, onDismiss = { moving = null }) }
}

@Composable
private fun FolderChips(state: NotesState, viewModel: NotesViewModel, onNewFolder: () -> Unit) {
    LazyRow(
        contentPadding = PaddingValues(horizontal = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.padding(bottom = 4.dp),
    ) {
        item {
            FilterChip(
                selected = state.folderId == null,
                onClick = { viewModel.selectFolder(null) },
                label = { Text("All") },
            )
        }
        items(state.folders, key = { it.id }) { folder ->
            FilterChip(
                selected = folder.id == state.folderId,
                onClick = { viewModel.selectFolder(folder.id) },
                label = { Text(folderPath(folder, state.folders)) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_folder), null, Modifier.size(18.dp)) },
            )
        }
        item {
            AssistChip(
                onClick = onNewFolder,
                label = { Text("Folder") },
                leadingIcon = { Icon(painterResource(R.drawable.ic_new_folder), null, Modifier.size(18.dp)) },
            )
        }
    }
}

/** "Home › Recipes" for nested folders. */
private fun folderPath(folder: Folder, all: List<Folder>): String {
    val parent = all.firstOrNull { it.id == folder.parentId } ?: return folder.title
    return "${parent.title} › ${folder.title}"
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun NoteCard(
    note: NoteSummary,
    folder: String?,
    viewModel: NotesViewModel,
    onMove: () -> Unit,
    modifier: Modifier,
) {
    var menu by remember { mutableStateOf(false) }
    Box(modifier) {
        Surface(
            shape = MaterialTheme.shapes.large,
            color = MaterialTheme.colorScheme.surfaceContainerLow,
            modifier = Modifier
                .fillMaxWidth()
                .combinedClickable(onLongClick = { menu = true }, onClick = { viewModel.open(note.id) }),
        ) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        note.title.ifEmpty { "New note" },
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    if (note.pinned) {
                        Icon(painterResource(R.drawable.ic_pin), "Pinned", tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp))
                    }
                }
                if (note.preview.isNotEmpty()) {
                    Text(
                        note.preview,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Text(
                    listOfNotNull(edited(note.updatedAt), folder).joinToString(" · "),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.outline,
                )
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(if (note.pinned) "Unpin" else "Pin") }, onClick = {
                menu = false
                viewModel.togglePin(note)
            })
            DropdownMenuItem(text = { Text("Move to…") }, onClick = {
                menu = false
                onMove()
            })
            DropdownMenuItem(text = { Text("Delete") }, onClick = {
                menu = false
                viewModel.delete(note)
            })
        }
    }
}

@Composable
private fun FolderMenu(onRename: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) { Icon(painterResource(R.drawable.ic_more_vert), "Folder options") }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(text = { Text("Rename folder") }, onClick = {
                open = false
                onRename()
            })
            DropdownMenuItem(text = { Text("Delete folder") }, onClick = {
                open = false
                onDelete()
            })
        }
    }
}

@Composable
private fun MoveDialog(note: NoteSummary, folders: List<Folder>, viewModel: NotesViewModel, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Move to") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                val choices = listOf<Folder?>(null) + folders
                choices.forEach { folder ->
                    Text(
                        folder?.title ?: "No folder",
                        style = MaterialTheme.typography.bodyLarge,
                        color = if (folder?.id == note.folderId) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .fillMaxWidth()
                            .combinedClickable(onClick = {
                                viewModel.move(note, folder?.id)
                                onDismiss()
                            })
                            .padding(start = (16 * (folder?.depth?.toInt() ?: 0)).dp, top = 12.dp, bottom = 12.dp),
                    )
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

// ---- editor ----

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NoteEditor(note: NoteSummary, state: NotesState, viewModel: NotesViewModel, snackbar: SnackbarHostState) {
    var value by remember(note.id) { mutableStateOf(TextFieldValue(viewModel.body(note.id))) }
    // What the store had when last loaded or saved, to tell local edits from synced ones.
    var saved by remember(note.id) { mutableStateOf(value.text) }
    var menu by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }

    LaunchedEffect(note.id, value.text) {
        if (value.text != saved) {
            delay(400)
            saved = value.text
            viewModel.setBody(note.id, value.text)
        }
    }
    // Edited on another device: take it unless there are unsaved local changes.
    LaunchedEffect(note.updatedAt) {
        val body = viewModel.body(note.id)
        if (value.text == saved && body != value.text) {
            value = TextFieldValue(body, TextRange(value.selection.start.coerceAtMost(body.length)))
            saved = body
        }
    }
    val latest by rememberUpdatedState(value.text to saved)
    DisposableEffect(note.id) {
        onDispose {
            val (text, lastSaved) = latest
            viewModel.close(note.id, text.takeIf { it != lastSaved })
        }
    }
    LaunchedEffect(note.id) { if (value.text.isEmpty()) focus.requestFocus() }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = {
                    IconButton(onClick = { viewModel.open(null) }) { Icon(painterResource(R.drawable.ic_arrow_back), "Back") }
                },
                title = {
                    Text(
                        listOfNotNull(state.folderTitle(note.folderId), "Edited ${edited(note.updatedAt)}").joinToString(" · "),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                },
                actions = {
                    IconButton(onClick = { viewModel.togglePin(note) }) {
                        Icon(
                            painterResource(R.drawable.ic_pin),
                            if (note.pinned) "Unpin" else "Pin",
                            tint = if (note.pinned) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Box {
                        IconButton(onClick = { menu = true }) { Icon(painterResource(R.drawable.ic_more_vert), "More") }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            DropdownMenuItem(text = { Text("Move to…") }, onClick = {
                                menu = false
                                moving = true
                            })
                            DropdownMenuItem(text = { Text("Delete") }, onClick = {
                                menu = false
                                viewModel.delete(note)
                            })
                        }
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = { FormatBar(value, onChange = { value = it }) },
    ) { padding ->
        MarkdownField(
            value,
            { value = it },
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 8.dp)
                .focusRequester(focus),
        )
    }
    if (moving) MoveDialog(note, state.folders, viewModel, onDismiss = { moving = false })
}

/** Formatting for the caret's line or the selection, above the keyboard. */
@Composable
private fun FormatBar(value: TextFieldValue, onChange: (TextFieldValue) -> Unit) {
    Surface(tonalElevation = 3.dp) {
        Column(Modifier.navigationBarsPadding().imePadding()) {
            HorizontalDivider()
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                horizontalArrangement = Arrangement.SpaceEvenly,
            ) {
                FormatButton(R.drawable.ic_title, "Heading") { onChange(value.cycleHeading()) }
                FormatButton(R.drawable.ic_checklist, "Checklist") { onChange(value.cycleChecklist()) }
                FormatButton(R.drawable.ic_bullets, "Bulleted list") { onChange(value.toggleBullet()) }
                FormatButton(R.drawable.ic_format_bold, "Bold") { onChange(value.wrap("**")) }
                FormatButton(R.drawable.ic_format_italic, "Italic") { onChange(value.wrap("*")) }
                FormatButton(R.drawable.ic_code, "Code") { onChange(value.wrap("`")) }
            }
        }
    }
}

@Composable
private fun FormatButton(icon: Int, label: String, onClick: () -> Unit) {
    IconButton(onClick = onClick) { Icon(painterResource(icon), label) }
}

private fun edited(millis: Long): String =
    if (System.currentTimeMillis() - millis < DateUtils.MINUTE_IN_MILLIS) "just now"
    else DateUtils.getRelativeTimeSpanString(millis).toString()
