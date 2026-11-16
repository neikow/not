package dev.lysen.not

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.Snooze
import dev.lysen.not.core.TaskItem
import dev.lysen.not.core.normalizeTag
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text

/** A smart list being made (no [id]) or edited. */
data class ViewDraft(val id: String? = null, val title: String = "", val query: String = "")

/**
 * Makes or edits a smart list: a name and a filter. The filter is checked as it's typed and shows how
 * many tasks it finds; chips put in the usual words so nobody has to learn the syntax first.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ViewEditor(draft: ViewDraft, state: UiState, viewModel: TasksViewModel, onDone: () -> Unit, onDismiss: () -> Unit) {
    var title by remember { mutableStateOf(draft.title) }
    var query by remember { mutableStateOf(draft.query) }
    val problem = remember(query) { if (query.isBlank()) null else viewModel.checkQuery(query) }
    val count = remember(query, problem) { if (query.isBlank() || problem != null) null else viewModel.countQuery(query) }
    val ready = title.isNotBlank() && query.isNotBlank() && problem == null
    fun save() {
        if (!ready) return
        if (draft.id == null) viewModel.createView(title, query) else viewModel.updateView(draft.id, title, query)
        onDone()
    }
    fun add(term: String) {
        query = (query.trimEnd() + " " + term).trim()
    }
    CardDialog(
        title = if (draft.id == null) "New smart list" else "Edit smart list",
        onDismiss = onDismiss,
        buttons = {
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button(if (draft.id == null) "Create" else "Save", ::save, enabled = ready)
        },
    ) {
        Field(title, { title = it }, "Name: Work, Errands, This week…", filled = true, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(10.dp))
        Field(query, { query = it }, "Filter: #work !1, #home or #errands, week…", filled = true, onDone = ::save, modifier = Modifier.fillMaxWidth())
        Text(
            when {
                query.isBlank() -> "Tasks matching every word show; “or” joins alternatives, “-” leaves out."
                problem != null -> problem
                count == 1 -> "1 task now"
                else -> "$count tasks now"
            },
            style = Not.type.caption,
            color = if (problem != null) Not.colors.danger else Not.colors.inkMuted,
            modifier = Modifier.padding(start = 4.dp, top = 6.dp, bottom = 8.dp),
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            state.tags.take(12).forEach { tag -> Chip(R.drawable.ic_tag, tag.name, onClick = { add("#${tag.name}") }) }
            for ((label, term) in listOf(
                "Today" to "today", "This week" to "week", "Overdue" to "overdue", "Has deadline" to "deadline",
                "High priority" to "!1", "No date" to "nodate", "Someday" to "someday", "Untagged" to "untagged", "or" to "or",
            )) {
                Chip(R.drawable.ic_add, label, onClick = { add(term) })
            }
        }
    }
}

/**
 * Holding a task: put it off (snooze, Someday), bring it to today, change its tags, or delete it.
 * Everything confirms with Undo afterwards.
 */
@Composable
fun TaskActionsSheet(task: TaskItem, state: UiState, viewModel: TasksViewModel, onDismiss: () -> Unit) {
    var tagging by remember { mutableStateOf(false) }
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text(task.title, style = Not.type.title, maxLines = 2, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 4.dp))
            TaskMeta(task, done = task.completedAt != null)
            Text("Snooze", style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(start = 14.dp, top = 12.dp, bottom = 4.dp))
            for ((label, to) in SNOOZES) {
                ListRow(R.drawable.ic_snooze, label, onClick = {
                    close()
                    viewModel.snooze(listOf(task), to)
                })
            }
            if (task.start == null || task.start!! < today()) {
                ListRow(R.drawable.ic_today, "Today", onClick = {
                    close()
                    viewModel.moveToToday(listOf(task))
                })
            }
            if (!task.someday) {
                ListRow(R.drawable.ic_someday, "Someday", onClick = {
                    close()
                    viewModel.setSomeday(task)
                })
            }
            Spacer(Modifier.height(8.dp))
            ListRow(R.drawable.ic_tag, if (task.tags.isEmpty()) "Add tags" else task.tags.joinToString(" ") { "#$it" }, onClick = { tagging = true })
            ListRow(R.drawable.ic_delete, "Delete", onClick = {
                close()
                viewModel.delete(task)
            })
        }
    }
    if (tagging) TagsDialog(task.tags, state, onSave = { viewModel.setTags(task.id, it) }, onDismiss = { tagging = false })
}

val SNOOZES = listOf(
    "Later today" to Snooze.LATER_TODAY,
    "This evening" to Snooze.THIS_EVENING,
    "Tomorrow" to Snooze.TOMORROW,
    "This weekend" to Snooze.WEEKEND,
    "Next week" to Snooze.NEXT_WEEK,
)

/** Tags as chips to switch on and off, and a field for a new one. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TagsDialog(current: List<String>, state: UiState, onSave: (List<String>) -> Unit, onDismiss: () -> Unit) {
    var chosen by remember { mutableStateOf(current) }
    var draft by remember { mutableStateOf("") }
    val all = (state.tags.map { it.name } + current).distinct().sorted()
    fun addDraft() {
        val tag = normalizeTag(draft)
        if (tag.isNotEmpty() && tag !in chosen) chosen = chosen + tag
        draft = ""
    }
    CardDialog(
        title = "Tags",
        onDismiss = onDismiss,
        buttons = {
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button("Save", {
                addDraft()
                onSave(chosen)
                onDismiss()
            })
        },
    ) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            (all + chosen.filter { it !in all }).forEach { tag ->
                val on = tag in chosen
                Chip(
                    R.drawable.ic_tag,
                    tag,
                    tint = if (on) null else Not.colors.inkMuted,
                    background = if (on) null else Not.colors.sunken,
                    onClick = { chosen = if (on) chosen - tag else chosen + tag },
                )
            }
        }
        Field(draft, { draft = it }, "New tag", filled = true, onDone = ::addDraft, modifier = Modifier.fillMaxWidth().padding(top = 10.dp))
    }
}
