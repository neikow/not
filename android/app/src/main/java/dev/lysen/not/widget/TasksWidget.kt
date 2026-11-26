package dev.lysen.not.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.action.Action
import androidx.glance.LocalContext
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.provideContent
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.width
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import dev.lysen.not.INBOX_ID
import dev.lysen.not.TODAY_ID
import dev.lysen.not.UPCOMING_ID
import dev.lysen.not.scheduleLabel
import dev.lysen.not.today
import androidx.glance.layout.Column
import dev.lysen.not.MainActivity
import dev.lysen.not.NotApp
import dev.lysen.not.QuickAddActivity
import dev.lysen.not.R
import dev.lysen.not.SyncWorker
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore
import dev.lysen.not.core.OutlineRow

/** Per-widget setting: which view it shows (built in or a smart list). Missing means the Inbox. */
val LIST_KEY = stringPreferencesKey("list_id")
internal val TASK_ID = ActionParameters.Key<String>("task_id")

/** Open tasks of one list with checkboxes; the title opens the list in the app, + adds a task. */
class TasksWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent { WidgetLookProvider {
            val listId = currentState(LIST_KEY) ?: INBOX_ID
            // Re-read the store whenever it changes while this widget's session is alive.
            val version by app.changes.collectAsState()
            val prefs = currentState<androidx.datastore.preferences.core.Preferences>()
            val showDone = TasksOptions.done.isOn(prefs)
            val content = remember(listId, version, showDone) { WidgetContent.load(app.store, listId, showDone) }
            TasksWidgetContent(content, showMeta = TasksOptions.meta.isOn(prefs), showAdd = TasksOptions.add.isOn(prefs))
        } }
    }
}

class TasksWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = TasksWidget()
}

internal val TASK_DONE = ActionParameters.Key<Boolean>("task_done")

/**
 * Ticking a checkbox completes the task; it drops out of the widget on the next render (or moves
 * to the done ones, when those show). Ticking a done one reopens it.
 */
class CompleteTask : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val id = parameters[TASK_ID] ?: return
        val reopen = parameters[TASK_DONE] ?: false
        try {
            (context.applicationContext as NotApp).store.setCompleted(id, !reopen, dev.lysen.not.today())
        } catch (_: NotException) {
            return // deleted on another device in the meantime
        }
        SyncWorker.pushSoon(context)
        WidgetKind.updateAll(context)
    }
}

private data class WidgetContent(
    val listId: String,
    val title: String,
    val rows: List<OutlineRow>,
) {
    companion object {
        fun load(store: NotStore, listId: String, showDone: Boolean): WidgetContent =
            try {
                val views = store.views(today())
                // The view may have been deleted on another device: fall back to the Inbox.
                val view = views.firstOrNull { it.id == listId } ?: views.first()
                // Open tasks only: completed subtasks of open tasks are left out too.
                val outline = store.viewOutline(view.id, today())
                val done = if (showDone) outline.completed.filter { it.depth == 0u } else emptyList()
                WidgetContent(view.id, view.title, outline.open.filter { !it.done } + done)
            } catch (e: NotException) {
                logLoadFailure("tasks", e)
                WidgetContent(INBOX_ID, "Inbox", emptyList())
            }
    }
}

@Composable
private fun TasksWidgetContent(content: WidgetContent, showMeta: Boolean, showAdd: Boolean) {
    val context = LocalContext.current
    val open = actionStartActivity(MainActivity.openList(context, content.listId))
    WidgetSurface {
        WidgetHeader(
            title = content.title,
            open = open,
            detail = content.rows.count { it.task.completedAt == null }.takeIf { it > 0 }?.toString(),
            add = if (showAdd) actionStartActivity(QuickAddActivity.intent(context, content.listId)) else null,
            addLabel = context.getString(R.string.add_task),
        )
        when {
            content.rows.isNotEmpty() -> WidgetItems(content.rows, { it.task.id.hashCode().toLong() }) { row ->
                    val meta = if (!showMeta) null else listOfNotNull(
                        row.task.start?.let { scheduleLabel(it, row.task.time, null) },
                        row.task.deadline?.let { "Due " + scheduleLabel(it, null, null) },
                        row.task.tags.takeIf { it.isNotEmpty() }?.joinToString(" ") { "#$it" },
                    ).joinToString(" · ").ifEmpty { null }
                    TaskRow(row, open, meta)
            }
            content.listId == TODAY_ID -> WidgetEmpty(R.drawable.ic_today, "Nothing for today", null, open)
            content.listId == UPCOMING_ID -> WidgetEmpty(R.drawable.ic_calendar, "Nothing coming up", null, open)
            content.listId == INBOX_ID -> WidgetEmpty(R.drawable.ic_inbox, "Inbox zero", "Tap + to jot something down.", open)
            else -> WidgetEmpty(R.drawable.ic_check, "All done", "Nothing left in ${content.title}.", open)
        }
    }
}

@Composable
private fun TaskRow(row: OutlineRow, open: Action, meta: String? = null) {
    val sub = row.depth > 0u
    val done = row.task.completedAt != null
    Row(
        GlanceModifier.fillMaxWidth().padding(start = (row.depth.toInt() * 22).dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        WidgetCheck(
            checked = done,
            onClick = actionRunCallback<CompleteTask>(actionParametersOf(TASK_ID to row.task.id, TASK_DONE to done)),
            size = if (sub) 18.dp else 22.dp,
            label = if (done) "Reopen ${row.task.title}" else "Complete ${row.task.title}",
        )
        Spacer(GlanceModifier.width(6.dp))
        Column(GlanceModifier.defaultWeight().padding(vertical = 8.wdp).clickable(open)) {
            Text(
                row.task.title,
                maxLines = 2,
                style = TextStyle(color = if (done) WidgetColors.inkFaint else WidgetColors.ink, fontSize = if (sub) 14.wsp else 15.wsp),
            )
            if (meta != null) Text(meta, maxLines = 1, style = TextStyle(color = WidgetColors.inkMuted, fontSize = 12.wsp))
        }
    }
}
