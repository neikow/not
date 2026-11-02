package dev.lysen.not

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.TaskItem
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.Menu
import dev.lysen.not.ui.MenuItem
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.RoundCheck
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.delay

/** Everything about one task: title, notes, subtasks, list, delete. Edits save as you go. */
@Composable
fun TaskDetailSheet(task: TaskItem, state: UiState, viewModel: TasksViewModel, onDismiss: () -> Unit) {
    var title by remember(task.id) { mutableStateOf(task.title) }
    var notes by remember(task.id) { mutableStateOf(task.notes) }
    var subtaskDraft by remember(task.id) { mutableStateOf("") }
    val colors = Not.colors

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

    Sheet(onDismiss = {
        saveTitle()
        if (notes != task.notes) viewModel.setNotes(task.id, notes)
        onDismiss()
    }) { close ->
        Column(
            Modifier
                .verticalScroll(rememberScrollState())
                .padding(start = 12.dp, end = 24.dp, bottom = 20.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                RoundCheck(task.completedAt != null, { viewModel.toggle(task) })
                Field(
                    title,
                    { title = it },
                    "Title",
                    style = Not.type.title,
                    onDone = ::saveTitle,
                    modifier = Modifier.weight(1f).padding(start = 4.dp),
                )
            }
            Field(
                notes,
                { notes = it },
                "Notes",
                style = Not.type.body.merge(color = colors.inkMuted),
                singleLine = false,
                minLines = 2,
                modifier = Modifier.fillMaxWidth().padding(start = 48.dp, top = 4.dp, bottom = 12.dp),
            )

            // Subtasks sit in a soft card, like a checklist inside the task.
            Column(
                Modifier
                    .padding(start = 12.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(20.dp))
                    .background(colors.background)
                    .padding(vertical = 6.dp),
            ) {
                Text("Subtasks", style = Not.type.label, color = colors.inkMuted, modifier = Modifier.padding(start = 16.dp, top = 8.dp, bottom = 2.dp))
                state.subtasks(of = task.id).forEach { subtask ->
                    val done = subtask.completedAt != null
                    Row(Modifier.padding(start = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        RoundCheck(done, { viewModel.toggle(subtask) }, size = 20.dp)
                        Text(
                            subtask.title,
                            style = Not.type.body,
                            color = if (done) colors.inkFaint else colors.ink,
                            textDecoration = if (done) TextDecoration.LineThrough else null,
                            modifier = Modifier.weight(1f).padding(end = 12.dp),
                        )
                    }
                }
                Row(Modifier.padding(start = 16.dp, end = 12.dp).heightIn(min = 44.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(R.drawable.ic_add, null, tint = colors.accentInk, size = 20.dp)
                    Spacer(Modifier.size(14.dp))
                    Field(
                        subtaskDraft,
                        { subtaskDraft = it },
                        "Add subtask",
                        onDone = {
                            if (subtaskDraft.isNotBlank()) viewModel.addTask(subtaskDraft, parentId = task.id)
                            subtaskDraft = ""
                        },
                        modifier = Modifier.weight(1f),
                    )
                }
            }

            Row(
                Modifier.fillMaxWidth().padding(start = 12.dp, top = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (task.parentId == null) MoveToList(task, state, viewModel)
                Spacer(Modifier.weight(1f))
                Button("Delete", {
                    close()
                    viewModel.delete(task)
                }, kind = ButtonKind.Danger, icon = R.drawable.ic_delete)
            }
        }
    }
}

@Composable
private fun MoveToList(task: TaskItem, state: UiState, viewModel: TasksViewModel) {
    var open by remember { mutableStateOf(false) }
    val current = state.lists.firstOrNull { it.id == task.listId }?.title ?: "Inbox"
    Box {
        Row(
            Modifier
                .clip(CircleShape)
                .background(Not.colors.sunken)
                .pressable({ open = true })
                .heightIn(min = 44.dp)
                .padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(if (task.listId == INBOX_ID) R.drawable.ic_inbox else R.drawable.ic_list, null, tint = Not.colors.inkMuted, size = 18.dp)
            Text(current, style = Not.type.label, maxLines = 1)
            Icon(R.drawable.ic_expand_more, "Move to list", tint = Not.colors.inkFaint, size = 18.dp)
        }
        Menu(open, onDismiss = { open = false }) {
            state.lists.forEach { list ->
                MenuItem(
                    list.title,
                    icon = if (list.id == INBOX_ID) R.drawable.ic_inbox else R.drawable.ic_list,
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
