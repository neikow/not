package dev.lysen.not

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.TaskItem
import kotlinx.coroutines.delay

/** Everything about one task: title, notes, subtasks, list, delete. Edits save as you go. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TaskDetailSheet(task: TaskItem, state: UiState, viewModel: TasksViewModel, onDismiss: () -> Unit) {
    val haptics = LocalHapticFeedback.current
    var title by remember(task.id) { mutableStateOf(task.title) }
    var notes by remember(task.id) { mutableStateOf(task.notes) }
    var subtaskDraft by remember(task.id) { mutableStateOf("") }

    // Notes save once typing pauses; each save is one undo step.
    LaunchedEffect(task.id, notes) {
        if (notes != task.notes) {
            delay(500)
            viewModel.setNotes(task.id, notes)
        }
    }

    fun saveTitle() {
        if (title.isNotBlank() && title != task.title) viewModel.rename(task.id, title)
    }

    ModalBottomSheet(
        onDismissRequest = {
            saveTitle()
            if (notes != task.notes) viewModel.setNotes(task.id, notes)
            onDismiss()
        },
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(
            Modifier
                .verticalScroll(rememberScrollState())
                .imePadding()
                .padding(horizontal = 16.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(
                    checked = task.completedAt != null,
                    onCheckedChange = {
                        haptics.performHapticFeedback(if (it) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
                        viewModel.toggle(task)
                    },
                )
                PlainField(
                    value = title,
                    onValueChange = { title = it },
                    placeholder = "Title",
                    textStyle = MaterialTheme.typography.titleLarge,
                    singleLine = true,
                    onDone = ::saveTitle,
                    modifier = Modifier.weight(1f),
                )
            }
            PlainField(
                value = notes,
                onValueChange = { notes = it },
                placeholder = "Notes",
                textStyle = MaterialTheme.typography.bodyLarge,
                singleLine = false,
                modifier = Modifier.fillMaxWidth().padding(start = 40.dp),
            )

            HorizontalDivider(Modifier.padding(vertical = 8.dp))
            Text(
                "Subtasks",
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(start = 12.dp),
            )
            state.subtasks(of = task.id).forEach { subtask ->
                val done = subtask.completedAt != null
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = done, onCheckedChange = { viewModel.toggle(subtask) })
                    Text(
                        subtask.title,
                        style = MaterialTheme.typography.bodyLarge,
                        textDecoration = if (done) TextDecoration.LineThrough else null,
                        color = if (done) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
                    )
                }
            }
            PlainField(
                value = subtaskDraft,
                onValueChange = { subtaskDraft = it },
                placeholder = "Add subtask",
                textStyle = MaterialTheme.typography.bodyLarge,
                singleLine = true,
                onDone = {
                    viewModel.addTask(subtaskDraft, parentId = task.id)
                    subtaskDraft = ""
                },
                modifier = Modifier.fillMaxWidth().padding(start = 40.dp),
            )

            HorizontalDivider(Modifier.padding(vertical = 8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (task.parentId == null) MoveToList(task, state, viewModel)
                Spacer(Modifier.weight(1f))
                TextButton(onClick = { viewModel.delete(task) }) {
                    Icon(painterResource(R.drawable.ic_delete), null, tint = MaterialTheme.colorScheme.error)
                    Text("  Delete", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}

@Composable
private fun MoveToList(task: TaskItem, state: UiState, viewModel: TasksViewModel) {
    var open by remember { mutableStateOf(false) }
    val current = state.lists.firstOrNull { it.id == task.listId }?.title ?: "Inbox"
    Box {
        TextButton(onClick = { open = true }) {
            Icon(painterResource(R.drawable.ic_list), null)
            Text("  $current")
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            state.lists.forEach { list ->
                DropdownMenuItem(
                    text = { Text(list.title) },
                    enabled = list.id != task.listId,
                    onClick = {
                        viewModel.move(task, list.id)
                        open = false
                    },
                )
            }
        }
    }
}

/** A text field without a box or underline, for a calm, document-like sheet. */
@Composable
private fun PlainField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    textStyle: androidx.compose.ui.text.TextStyle,
    singleLine: Boolean,
    modifier: Modifier = Modifier,
    onDone: () -> Unit = {},
) {
    TextField(
        value = value,
        onValueChange = onValueChange,
        placeholder = { Text(placeholder, style = textStyle) },
        textStyle = textStyle,
        singleLine = singleLine,
        minLines = if (singleLine) 1 else 3,
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.Sentences,
            imeAction = if (singleLine) ImeAction.Done else ImeAction.Default,
        ),
        keyboardActions = KeyboardActions(onDone = { onDone() }),
        colors = TextFieldDefaults.colors(
            focusedContainerColor = Color.Transparent,
            unfocusedContainerColor = Color.Transparent,
            focusedIndicatorColor = Color.Transparent,
            unfocusedIndicatorColor = Color.Transparent,
        ),
        modifier = modifier,
    )
}
