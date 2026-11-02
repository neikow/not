package dev.lysen.not

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Folder
import dev.lysen.not.core.NoteSummary
import dev.lysen.not.ui.AddAction
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
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.ToastHost
import dev.lysen.not.ui.ToastState
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.delay

/** Notes of a folder (or all), with search; opening a note shows the editor. */
@Composable
fun NotesScreen(viewModel: NotesViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toasts = remember { ToastState() }

    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            if (toasts.show(undoable.message, action = "Undo")) viewModel.undo()
        }
    }

    val open = state.openNote
    Box(Modifier.fillMaxSize()) {
        if (open != null) {
            BackHandler { viewModel.open(null) }
            NoteEditor(open, state, viewModel)
        } else {
            AddAction("New note", onClick = viewModel::newNote)
            NoteList(state, viewModel)
        }
        ToastHost(
            toasts,
            Modifier.align(Alignment.BottomCenter).imePadding().padding(bottom = LocalBottomBarHeight.current + 8.dp + if (open != null) 64.dp else 0.dp),
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
private fun NoteList(state: NotesState, viewModel: NotesViewModel) {
    var newFolder by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<Folder?>(null) }
    var moving by remember { mutableStateOf<NoteSummary?>(null) }
    val searching = state.query != null
    if (searching) BackHandler { viewModel.search(null) }

    LargeTitleScreen(
        title = state.folder?.title ?: "Notes",
        actions = {
            if (!searching) IconButton(R.drawable.ic_search, "Search", { viewModel.search("") })
            state.folder?.let { folder ->
                FolderMenu(onRename = { renaming = folder }, onDelete = { viewModel.deleteFolder(folder) })
            }
        },
        header = {
            if (searching) {
                SearchField(state.query.orEmpty(), viewModel::search, onClose = { viewModel.search(null) })
            } else {
                FolderChips(state, viewModel, onNewFolder = { newFolder = true })
            }
        },
    ) {
        if (state.notes.isEmpty()) {
            item(key = "empty") {
                if (searching) {
                    EmptyState(R.drawable.ic_search, "No results", "Try other words.")
                } else {
                    EmptyState(R.drawable.ic_note, "No notes yet", "Tap + to write one. The first line becomes its title.")
                }
            }
        }
        items(state.notes, key = { it.id }) { note ->
            NoteCard(
                note,
                folder = if (state.folderId == null && note.folderId != null) state.folderTitle(note.folderId) else null,
                viewModel = viewModel,
                onMove = { moving = note },
                modifier = Modifier.animateItem().padding(horizontal = 16.dp, vertical = 5.dp),
            )
        }
    }

    if (newFolder) {
        PromptDialog(
            title = state.folder?.let { "New folder in ${it.title}" } ?: "New folder",
            confirm = "Create",
            placeholder = "Recipes, Work, Trips…",
            onConfirm = {
                viewModel.addFolder(it)
                newFolder = false
            },
            onDismiss = { newFolder = false },
        )
    }
    renaming?.let { folder ->
        PromptDialog(
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
    moving?.let { note -> MoveSheet(note, state.folders, viewModel, onDismiss = { moving = null }) }
}

@Composable
private fun SearchField(query: String, onQuery: (String) -> Unit, onClose: () -> Unit) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Field(
        query,
        onQuery,
        "Search notes",
        filled = true,
        shape = CircleShape,
        leading = { Icon(R.drawable.ic_search, null, tint = Not.colors.inkMuted, size = 20.dp) },
        trailing = {
            Box(Modifier.clip(CircleShape).pressable(onClose, onClickLabel = "Close search").padding(2.dp)) {
                Icon(R.drawable.ic_close, "Close search", tint = Not.colors.inkMuted, size = 20.dp)
            }
        },
        modifier = Modifier.fillMaxWidth().padding(top = 12.dp).focusRequester(focus),
    )
}

/** All, each folder, and + Folder, as a scrolling row of pills. */
@Composable
private fun FolderChips(state: NotesState, viewModel: NotesViewModel, onNewFolder: () -> Unit) {
    LazyRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.padding(top = 12.dp),
    ) {
        item { Chip("All", selected = state.folderId == null, onClick = { viewModel.selectFolder(null) }) }
        items(state.folders, key = { it.id }) { folder ->
            Chip(folderPath(folder, state.folders), selected = folder.id == state.folderId, icon = R.drawable.ic_folder, onClick = { viewModel.selectFolder(folder.id) })
        }
        item { Chip("Folder", selected = false, icon = R.drawable.ic_new_folder, quiet = true, onClick = onNewFolder) }
    }
}

@Composable
private fun Chip(text: String, selected: Boolean, onClick: () -> Unit, icon: Int? = null, quiet: Boolean = false) {
    val colors = Not.colors
    val bg by animateColorAsState(
        when {
            selected -> colors.accent
            quiet -> Color.Transparent
            else -> colors.surface
        },
        label = "chip",
    )
    val fg = when {
        selected -> colors.onAccent
        quiet -> colors.accentInk
        else -> colors.ink
    }
    Row(
        Modifier
            .clip(CircleShape)
            .background(bg)
            .pressable(onClick, pressedScale = 0.94f)
            .heightIn(min = 38.dp)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (icon != null) Icon(icon, null, tint = if (selected) colors.onAccent else if (quiet) colors.accentInk else colors.inkMuted, size = 16.dp)
        Text(text, style = Not.type.label, color = fg, maxLines = 1)
    }
}

/** "Home › Recipes" for nested folders. */
private fun folderPath(folder: Folder, all: List<Folder>): String {
    val parent = all.firstOrNull { it.id == folder.parentId } ?: return folder.title
    return "${parent.title} › ${folder.title}"
}

/** A note as a soft card; long-press for pin, move and delete. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun NoteCard(
    note: NoteSummary,
    folder: String?,
    viewModel: NotesViewModel,
    onMove: () -> Unit,
    modifier: Modifier,
) {
    val colors = Not.colors
    val haptics = LocalHapticFeedback.current
    var menu by remember { mutableStateOf(false) }
    Box(modifier) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(22.dp))
                .background(colors.surface)
                .combinedClickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onLongClick = {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        menu = true
                    },
                    onClick = { viewModel.open(note.id) },
                )
                .padding(horizontal = 18.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(note.title.ifEmpty { "New note" }, style = Not.type.heading, maxLines = 1, modifier = Modifier.weight(1f))
                if (note.pinned) Icon(R.drawable.ic_pin, "Pinned", tint = colors.accentInk, size = 16.dp)
            }
            if (note.preview.isNotEmpty()) {
                Text(note.preview, style = Not.type.body.merge(fontSize = Not.type.caption.fontSize.times(1.1f), lineHeight = Not.type.caption.lineHeight.times(1.1f)), color = colors.inkMuted, maxLines = 2)
            }
            Text(
                listOfNotNull(edited(note.updatedAt), folder).joinToString(" · "),
                style = Not.type.caption,
                color = colors.inkFaint,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
        Menu(menu, onDismiss = { menu = false }) {
            MenuItem(if (note.pinned) "Unpin" else "Pin to top", icon = R.drawable.ic_pin, onClick = {
                menu = false
                viewModel.togglePin(note)
            })
            MenuItem("Move to…", icon = R.drawable.ic_folder, onClick = {
                menu = false
                onMove()
            })
            MenuItem("Delete", icon = R.drawable.ic_delete, danger = true, onClick = {
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
        IconButton(R.drawable.ic_more_vert, "Folder options", { open = true })
        Menu(open, onDismiss = { open = false }) {
            MenuItem("Rename folder", icon = R.drawable.ic_edit, onClick = {
                open = false
                onRename()
            })
            MenuItem("Delete folder", icon = R.drawable.ic_delete, danger = true, onClick = {
                open = false
                onDelete()
            })
        }
    }
}

@Composable
private fun MoveSheet(note: NoteSummary, folders: List<Folder>, viewModel: NotesViewModel, onDismiss: () -> Unit) {
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text("Move to", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 12.dp))
            (listOf<Folder?>(null) + folders).forEach { folder ->
                val current = folder?.id == note.folderId
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clip(CircleShape)
                        .background(if (current) Not.colors.accentSoft else Color.Transparent)
                        .pressable({
                            viewModel.move(note, folder?.id)
                            close()
                        }, pressedScale = 0.98f)
                        .heightIn(min = 52.dp)
                        .padding(start = (14 + 20 * (folder?.depth?.toInt() ?: 0)).dp, end = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(if (folder == null) R.drawable.ic_note else R.drawable.ic_folder, null, tint = if (current) Not.colors.accentInk else Not.colors.inkMuted)
                    Spacer(Modifier.width(14.dp))
                    Text(folder?.title ?: "No folder", style = if (current) Not.type.bodyStrong else Not.type.body)
                }
            }
        }
    }
}

// ---- editor ----

@Composable
private fun NoteEditor(note: NoteSummary, state: NotesState, viewModel: NotesViewModel) {
    var value by remember(note.id) { mutableStateOf(TextFieldValue(viewModel.body(note.id))) }
    // What the store had when last loaded or saved, to tell local edits from synced ones.
    var saved by remember(note.id) { mutableStateOf(value.text) }
    var menu by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val colors = Not.colors

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

    Column(Modifier.fillMaxSize().background(colors.background)) {
        Row(
            Modifier.fillMaxWidth().statusBarsPadding().height(56.dp).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(R.drawable.ic_arrow_back, "Back", { viewModel.open(null) })
            Text(
                listOfNotNull(state.folderTitle(note.folderId), "Edited ${edited(note.updatedAt)}").joinToString(" · "),
                style = Not.type.caption,
                color = colors.inkFaint,
                maxLines = 1,
                modifier = Modifier.weight(1f).padding(horizontal = 4.dp),
            )
            IconButton(
                R.drawable.ic_pin,
                if (note.pinned) "Unpin" else "Pin",
                { viewModel.togglePin(note) },
                tint = if (note.pinned) colors.accentInk else colors.inkMuted,
                background = if (note.pinned) colors.accentSoft else Color.Transparent,
            )
            Box {
                IconButton(R.drawable.ic_more_vert, "More", { menu = true })
                Menu(menu, onDismiss = { menu = false }) {
                    MenuItem("Move to…", icon = R.drawable.ic_folder, onClick = {
                        menu = false
                        moving = true
                    })
                    MenuItem("Delete", icon = R.drawable.ic_delete, danger = true, onClick = {
                        menu = false
                        viewModel.delete(note)
                    })
                }
            }
        }
        MarkdownField(
            value,
            { value = it },
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 8.dp)
                .focusRequester(focus),
        )
        FormatBar(value, onChange = { value = it })
    }
    if (moving) MoveSheet(note, state.folders, viewModel, onDismiss = { moving = false })
}

/** Formatting for the caret's line or the selection, riding above the keyboard. */
@Composable
private fun FormatBar(value: TextFieldValue, onChange: (TextFieldValue) -> Unit) {
    Column(Modifier.navigationBarsPadding().imePadding()) {
        Divider()
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.SpaceEvenly,
        ) {
            IconButton(R.drawable.ic_title, "Heading", { onChange(value.cycleHeading()) }, tint = Not.colors.inkMuted)
            IconButton(R.drawable.ic_checklist, "Checklist", { onChange(value.cycleChecklist()) }, tint = Not.colors.inkMuted)
            IconButton(R.drawable.ic_bullets, "Bulleted list", { onChange(value.toggleBullet()) }, tint = Not.colors.inkMuted)
            IconButton(R.drawable.ic_format_bold, "Bold", { onChange(value.wrap("**")) }, tint = Not.colors.inkMuted)
            IconButton(R.drawable.ic_format_italic, "Italic", { onChange(value.wrap("*")) }, tint = Not.colors.inkMuted)
            IconButton(R.drawable.ic_code, "Code", { onChange(value.wrap("`")) }, tint = Not.colors.inkMuted)
        }
    }
}

private fun edited(millis: Long): String =
    if (System.currentTimeMillis() - millis < DateUtils.MINUTE_IN_MILLIS) "just now"
    else DateUtils.getRelativeTimeSpanString(millis).toString()
