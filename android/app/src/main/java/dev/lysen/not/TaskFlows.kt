package dev.lysen.not

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Snooze
import dev.lysen.not.core.TaskItem
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.EmptyState
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.RoundCheck
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/**
 * The Inbox one task at a time: give it a day, a tag, park it, finish it or let it go. Each choice
 * moves on to the next; Undo (in the toast behind) takes a choice back.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun InboxFlow(viewModel: TasksViewModel, onDismiss: () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    // Taken once, so tasks sorted away don't shift what's next.
    val queue = remember { viewModel.viewTasks(INBOX_ID) }
    var at by remember { mutableIntStateOf(0) }
    var tagging by remember { mutableStateOf(false) }
    Sheet(onDismiss) { close ->
        Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 20.dp)) {
            Text("Process Inbox", style = Not.type.title, modifier = Modifier.padding(top = 4.dp))
            Text(
                if (at < queue.size) "${at + 1} of ${queue.size}" else "Done",
                style = Not.type.caption,
                color = Not.colors.inkMuted,
                modifier = Modifier.padding(bottom = 16.dp),
            )
            AnimatedContent(at, label = "inbox task") { index ->
                val task = queue.getOrNull(index)
                if (task == null) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                        EmptyState(R.drawable.ic_inbox, "All sorted", "Everything in the Inbox has a place now.")
                        Button("Close", close)
                    }
                    return@AnimatedContent
                }
                Column {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(20.dp))
                            .background(Not.colors.sunken)
                            .heightIn(min = 96.dp)
                            .padding(18.dp),
                    ) {
                        Text(task.title, style = Not.type.title)
                        TaskMeta(task, done = false)
                        task.notes.lineSequence().firstOrNull { it.isNotBlank() }?.let {
                            Text(it, style = Not.type.body, color = Not.colors.inkMuted, maxLines = 3, modifier = Modifier.padding(top = 6.dp))
                        }
                    }
                    Spacer(Modifier.height(16.dp))
                    fun next() {
                        at = index + 1
                    }
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button("Today", { viewModel.moveToToday(listOf(task)); next() }, kind = ButtonKind.Secondary, icon = R.drawable.ic_today)
                        Button("Tomorrow", { viewModel.snooze(listOf(task), Snooze.TOMORROW); next() }, kind = ButtonKind.Secondary, icon = R.drawable.ic_calendar)
                        Button("Weekend", { viewModel.snooze(listOf(task), Snooze.WEEKEND); next() }, kind = ButtonKind.Secondary, icon = R.drawable.ic_calendar)
                        Button("Someday", { viewModel.setSomeday(task); next() }, kind = ButtonKind.Secondary, icon = R.drawable.ic_someday)
                        Button("Tag…", { tagging = true }, kind = ButtonKind.Secondary, icon = R.drawable.ic_tag)
                        Button("Done", { viewModel.toggle(task); next() }, kind = ButtonKind.Secondary, icon = R.drawable.ic_check)
                        Button("Delete", { viewModel.delete(task); next() }, kind = ButtonKind.Danger, icon = R.drawable.ic_delete)
                        Button("Skip", { next() }, kind = ButtonKind.Quiet)
                    }
                    if (tagging) {
                        TagsDialog(task.tags, state, onSave = {
                            viewModel.setTags(task.id, it)
                            next()
                        }, onDismiss = { tagging = false })
                    }
                }
            }
        }
    }
}

/**
 * A weekly look over everything: empty the Inbox, deal with what slipped, go through Someday, and ask
 * about tasks nobody touched for a month. Each step lists its tasks with a few actions each.
 */
@Composable
fun ReviewFlow(viewModel: TasksViewModel, onProcessInbox: () -> Unit, onDismiss: () -> Unit) {
    var step by remember { mutableIntStateOf(0) }
    // Tasks dealt with in this review, hidden from its lists.
    var handled by remember { mutableStateOf(setOf<String>()) }
    fun done(task: TaskItem) {
        handled = handled + task.id
    }
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 20.dp)) {
            Text("Weekly review", style = Not.type.title, modifier = Modifier.padding(top = 4.dp))
            Text(if (step < 4) "Step ${step + 1} of 4" else "Done", style = Not.type.caption, color = Not.colors.inkMuted, modifier = Modifier.padding(bottom = 12.dp))
            when (step) {
                0 -> {
                    val inbox = remember { viewModel.viewTasks(INBOX_ID) }
                    ReviewHeading("Inbox", if (inbox.isEmpty()) "Empty: nothing to sort." else "${inbox.size} waiting for a day, a tag or Someday.")
                    if (inbox.isNotEmpty()) {
                        Button("Process Inbox", {
                            close()
                            onProcessInbox()
                        }, icon = R.drawable.ic_play)
                    }
                }
                1 -> {
                    val earlier = remember { viewModel.viewTasks(TODAY_ID).filter { it.start?.let { d -> d < today() } == true } }
                    ReviewHeading("Slipped", if (earlier.isEmpty()) "Nothing left over from earlier days." else "Planned for an earlier day and not done yet.")
                    if (earlier.any { it.id !in handled }) {
                        Button("Move all to today", {
                            viewModel.moveToToday(earlier.filter { it.id !in handled })
                            handled = handled + earlier.map { it.id }
                        }, kind = ButtonKind.Secondary, icon = R.drawable.ic_today)
                    }
                    earlier.filter { it.id !in handled }.forEach { task ->
                        ReviewRow(task) {
                            ReviewAction("Today") { viewModel.moveToToday(listOf(task)); done(task) }
                            ReviewAction("Next week") { viewModel.snooze(listOf(task), Snooze.NEXT_WEEK); done(task) }
                            ReviewAction("Someday") { viewModel.setSomeday(task); done(task) }
                            ReviewAction("Done") { viewModel.toggle(task); done(task) }
                        }
                    }
                }
                2 -> {
                    val someday = remember { viewModel.viewTasks(SOMEDAY_ID) }
                    ReviewHeading("Someday", if (someday.isEmpty()) "Nothing parked." else "Still maybe? Bring one back or let it go.")
                    someday.filter { it.id !in handled }.forEach { task ->
                        ReviewRow(task) {
                            ReviewAction("Today") { viewModel.moveToToday(listOf(task)); done(task) }
                            ReviewAction("Keep") { viewModel.keep(task); done(task) }
                            ReviewAction("Delete") { viewModel.delete(task); done(task) }
                        }
                    }
                }
                3 -> {
                    val stale = remember { viewModel.staleTasks() }
                    ReviewHeading("Untouched for a month", if (stale.isEmpty()) "Every undated task has been looked at lately." else "No date and no change in 30 days: still relevant?")
                    stale.filter { it.id !in handled }.forEach { task ->
                        ReviewRow(task) {
                            ReviewAction("Keep") { viewModel.keep(task); done(task) }
                            ReviewAction("Today") { viewModel.moveToToday(listOf(task)); done(task) }
                            ReviewAction("Someday") { viewModel.setSomeday(task); done(task) }
                            ReviewAction("Done") { viewModel.toggle(task); done(task) }
                            ReviewAction("Delete") { viewModel.delete(task); done(task) }
                        }
                    }
                }
                else -> EmptyState(R.drawable.ic_review, "Reviewed", "Everything has a place. See you next week.")
            }
            Row(Modifier.fillMaxWidth().padding(top = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                if (step in 1..3) Button("Back", { step-- }, kind = ButtonKind.Quiet)
                if (step < 4) Button("Next", { step++ }) else Button("Close", close)
            }
        }
    }
}

@Composable
private fun ReviewHeading(title: String, hint: String) {
    Text(title, style = Not.type.bodyStrong)
    Text(hint, style = Not.type.body, color = Not.colors.inkMuted, modifier = Modifier.padding(bottom = 12.dp))
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ReviewRow(task: TaskItem, actions: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Text(task.title, style = Not.type.body)
        TaskMeta(task, done = false)
        FlowRow(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) { actions() }
    }
}

@Composable
private fun ReviewAction(label: String, onClick: () -> Unit) {
    Chip(R.drawable.ic_check, label, tint = Not.colors.ink, background = Not.colors.sunken, onClick = onClick)
}

/** Everything completed, by day, latest first; tapping the check reopens one. */
@Composable
fun LogbookSheet(viewModel: TasksViewModel, onDismiss: () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val done = remember(state.outline, state.archive) { viewModel.logbook() }
    val zone = ZoneId.systemDefault()
    val today = LocalDate.now()
    val days = done.groupBy { Instant.ofEpochMilli(it.completedAt ?: 0).atZone(zone).toLocalDate() }
    Sheet(onDismiss) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 20.dp)) {
            Text("Logbook", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 8.dp))
            if (done.isEmpty()) EmptyState(R.drawable.ic_history, "Nothing yet", "Tasks you complete are kept here, by day.")
            days.forEach { (day, tasks) ->
                val label = when (day) {
                    today -> "Today"
                    today.minusDays(1) -> "Yesterday"
                    else -> day.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
                }
                Row(Modifier.padding(start = 14.dp, top = 14.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(label, style = Not.type.label, color = Not.colors.inkMuted)
                    Text("  ${tasks.size}", style = Not.type.label, color = Not.colors.inkFaint)
                }
                tasks.forEach { task ->
                    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp), verticalAlignment = Alignment.CenterVertically) {
                        RoundCheck(true, { viewModel.restore(task) }, size = 20.dp)
                        Column(Modifier.weight(1f).padding(start = 4.dp)) {
                            Text(task.title, style = Not.type.body, color = Not.colors.inkMuted, textDecoration = TextDecoration.LineThrough, maxLines = 1)
                            if (task.tags.isNotEmpty()) Text(task.tags.joinToString(" ") { "#$it" }, style = Not.type.caption, color = Not.colors.inkFaint)
                        }
                    }
                }
            }
        }
    }
}
