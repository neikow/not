package dev.lysen.not

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.TaskItem
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
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

/** Everything about one task: title, notes, dates, tags, subtasks, delete. Edits save as you go. */
@OptIn(ExperimentalLayoutApi::class)
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
                RoundCheck(task.completedAt != null, { viewModel.toggle(task) }, ring = colors.priority(task.priority))
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
            FlowRow(
                Modifier.padding(start = 48.dp, bottom = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                SchedulePill(task, viewModel)
                DeadlinePill(task, viewModel)
                PriorityPill(task, viewModel)
                TagsPill(task, state, viewModel)
                ParentPill(task, state, viewModel)
                PlacePill(task)
            }

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
                state.openSubtasks.forEach { subtask ->
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
                            if (subtaskDraft.isNotBlank()) viewModel.addSubtask(subtaskDraft, parentId = task.id)
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
                Spacer(Modifier.weight(1f))
                Button("Delete", {
                    close()
                    viewModel.delete(task)
                }, kind = ButtonKind.Danger, icon = R.drawable.ic_delete)
            }
        }
    }
}

/** When the task is planned for: tap to change it in words ("fri 9am", "every mon", "someday"). */
@Composable
private fun SchedulePill(task: TaskItem, viewModel: TasksViewModel) {
    var editing by remember { mutableStateOf(false) }
    val label = when {
        task.someday -> "Someday"
        else -> scheduleLabel(task.start, task.time, task.repeat.takeIf { task.start != null })
    }
    val colors = Not.colors
    Chip(
        when {
            task.someday -> R.drawable.ic_someday
            task.repeat != null && task.start != null -> R.drawable.ic_repeat
            else -> R.drawable.ic_calendar
        },
        label ?: "When",
        tint = if (task.start == null && !task.someday) colors.inkMuted else colors.accentInk,
        onClick = { editing = true },
    )
    if (editing) ScheduleDialog(task, viewModel, onDismiss = { editing = false })
}

/** The day it must be done by: accent on the day, red after. */
@Composable
private fun DeadlinePill(task: TaskItem, viewModel: TasksViewModel) {
    var editing by remember { mutableStateOf(false) }
    val colors = Not.colors
    val deadline = task.deadline
    val today = today()
    Chip(
        R.drawable.ic_deadline,
        deadline?.let { "Due " + (scheduleLabel(it, null, task.repeat.takeIf { task.start == null }) ?: it) } ?: "Deadline",
        tint = when {
            deadline == null -> colors.inkMuted
            deadline < today -> colors.danger
            else -> colors.accentInk
        },
        onClick = { editing = true },
    )
    if (editing) {
        val chips = listOf("Today" to "today", "Tomorrow" to "tomorrow", "Friday" to "fri", "End of month" to "end of month")
        WordsDialog("Deadline", "fri, 12 dec, end of month…", chips, canClear = deadline != null, onPick = { viewModel.setDeadline(task, it) }, onDismiss = { editing = false })
    }
}

@Composable
private fun TagsPill(task: TaskItem, state: UiState, viewModel: TasksViewModel) {
    var editing by remember { mutableStateOf(false) }
    Chip(
        R.drawable.ic_tag,
        if (task.tags.isEmpty()) "Tags" else task.tags.joinToString(" ") { "#$it" },
        tint = if (task.tags.isEmpty()) Not.colors.inkMuted else null,
        onClick = { editing = true },
    )
    if (editing) TagsDialog(task.tags, state, onSave = { viewModel.setTags(task.id, it) }, onDismiss = { editing = false })
}

/** "Under Trip" (tap to change, × to move it out), or "Make subtask" to pick a parent. */
@Composable
private fun ParentPill(task: TaskItem, state: UiState, viewModel: TasksViewModel) {
    var picking by remember { mutableStateOf(false) }
    val parent = remember(task.parentId) { task.parentId?.let(viewModel::taskById) }
    if (parent == null) {
        Chip(R.drawable.ic_indent, "Make subtask", tint = Not.colors.inkMuted, onClick = { picking = true })
    } else {
        Chip(
            R.drawable.ic_indent,
            "Under “${parent.title}”",
            onClick = { picking = true },
            onRemove = { viewModel.moveOut(task) },
        )
    }
    if (picking) {
        ParentPicker(task, state, viewModel, onPick = {
            picking = false
            viewModel.nest(task, it.id, null, say = true)
        }, onDismiss = { picking = false })
    }
}

/** A plain field for a date in words, with a few quick picks. */
@Composable
private fun WordsDialog(title: String, placeholder: String, chips: List<Pair<String, String>>, canClear: Boolean, onPick: (String) -> Unit, onDismiss: () -> Unit) {
    var text by remember { mutableStateOf("") }
    fun pick(words: String) {
        onPick(words)
        onDismiss()
    }
    CardDialog(
        title = title,
        onDismiss = onDismiss,
        buttons = {
            if (canClear) Button("Clear", { pick("") }, kind = ButtonKind.Quiet)
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button("Save", { pick(text) }, enabled = text.isNotBlank())
        },
    ) {
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for ((label, words) in chips) Chip(R.drawable.ic_calendar, label, onClick = { pick(words) })
        }
        Field(text, { text = it }, placeholder, filled = true, onDone = { if (text.isNotBlank()) pick(text) }, modifier = Modifier.fillMaxWidth().padding(top = 12.dp))
    }
}

@Composable
private fun ScheduleDialog(task: TaskItem, viewModel: TasksViewModel, onDismiss: () -> Unit) {
    val draft = rememberTaskDraft()
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    fun save() {
        if (!draft.isBlank) viewModel.setSchedule(task.id, draft.text, draft.dismissed)
        onDismiss()
    }
    fun pick(words: String) {
        viewModel.setSchedule(task.id, words)
        onDismiss()
    }
    CardDialog(
        title = "When",
        onDismiss = onDismiss,
        buttons = {
            if (task.start != null || task.someday || task.deadline != null) Button("Clear", { pick("") }, kind = ButtonKind.Quiet)
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button("Save", ::save, enabled = !draft.isBlank)
        },
    ) {
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for ((label, words) in listOf("Today" to "today", "This evening" to "tonight", "Tomorrow" to "tomorrow", "This weekend" to "this weekend", "Next week" to "next week", "Someday" to "someday")) {
                Chip(R.drawable.ic_calendar, label, onClick = { pick(words) })
            }
        }
        TaskDraftField(
            draft,
            "fri 9am, every last fri, in 2 hours…",
            onSubmit = ::save,
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 12.dp)
                .clip(RoundedCornerShape(16.dp))
                .background(Not.colors.sunken)
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .focusRequester(focus),
        )
        TokenChips(draft, Modifier.padding(top = 8.dp))
    }
}

@Composable
private fun PriorityPill(task: TaskItem, viewModel: TasksViewModel) {
    var open by remember { mutableStateOf(false) }
    val colors = Not.colors
    Box {
        Chip(
            R.drawable.ic_flag,
            if (task.priority > 0u) priorityName(task.priority) else "Priority",
            tint = colors.priority(task.priority) ?: colors.inkMuted,
            onClick = { open = true },
        )
        Menu(open, onDismiss = { open = false }) {
            for (p in listOf<UByte>(3u, 2u, 1u, 0u)) {
                MenuItem(priorityName(p), icon = R.drawable.ic_flag, enabled = p != task.priority, onClick = {
                    viewModel.setPriority(task.id, p)
                    open = false
                })
            }
        }
    }
}
