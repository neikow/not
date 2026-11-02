package dev.lysen.not.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.CheckBox
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.components.CircleIconButton
import androidx.glance.appwidget.components.Scaffold
import androidx.glance.appwidget.components.TitleBar
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.provideContent
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import dev.lysen.not.INBOX_ID
import dev.lysen.not.MainActivity
import dev.lysen.not.NotApp
import dev.lysen.not.QuickAddActivity
import dev.lysen.not.R
import dev.lysen.not.SyncWorker
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore
import dev.lysen.not.core.OutlineRow

/** Per-widget setting: which list it shows. Missing means the Inbox. */
val LIST_KEY = stringPreferencesKey("list_id")
private val TASK_ID = ActionParameters.Key<String>("task_id")

/** Open tasks of one list with checkboxes; the title opens the list in the app, + adds a task. */
class TasksWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent {
            val listId = currentState(LIST_KEY) ?: INBOX_ID
            // Re-read the store whenever it changes while this widget's session is alive.
            val version by app.changes.collectAsState()
            val content = remember(listId, version) { WidgetContent.load(app.store, listId) }
            GlanceTheme { TasksWidgetContent(content) }
        }
    }
}

class TasksWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = TasksWidget()
}

/** Ticking a checkbox completes the task; it drops out of the widget on the next render. */
class CompleteTask : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val id = parameters[TASK_ID] ?: return
        try {
            (context.applicationContext as NotApp).store.setCompleted(id, true)
        } catch (_: NotException) {
            return // deleted on another device in the meantime
        }
        SyncWorker.pushSoon(context)
    }
}

private data class WidgetContent(val listId: String, val title: String, val rows: List<OutlineRow>) {
    companion object {
        fun load(store: NotStore, listId: String): WidgetContent =
            try {
                val lists = store.lists()
                // The list may have been deleted on another device: fall back to the Inbox.
                val list = lists.firstOrNull { it.id == listId } ?: lists.first()
                // Open tasks only: completed subtasks of open tasks are left out too.
                WidgetContent(list.id, list.title, store.outline(list.id).open.filter { !it.done })
            } catch (_: NotException) {
                WidgetContent(INBOX_ID, "Inbox", emptyList())
            }
    }
}

@Composable
private fun TasksWidgetContent(content: WidgetContent) {
    val context = LocalContext.current
    Scaffold(
        titleBar = {
            TitleBar(
                startIcon = ImageProvider(R.drawable.ic_task_alt),
                title = content.title,
                modifier = GlanceModifier.clickable(
                    actionStartActivity(MainActivity.openList(context, content.listId)),
                ),
                actions = {
                    CircleIconButton(
                        imageProvider = ImageProvider(R.drawable.ic_add),
                        contentDescription = context.getString(R.string.add_task),
                        onClick = actionStartActivity(QuickAddActivity.intent(context, content.listId)),
                        backgroundColor = null,
                    )
                },
            )
        },
    ) {
        if (content.rows.isEmpty()) {
            Box(GlanceModifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("Nothing to do", style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant))
            }
        } else {
            LazyColumn(GlanceModifier.fillMaxSize()) {
                items(content.rows, itemId = { it.task.id.hashCode().toLong() }) { row ->
                    CheckBox(
                        checked = false,
                        onCheckedChange = actionRunCallback<CompleteTask>(actionParametersOf(TASK_ID to row.task.id)),
                        text = row.task.title,
                        maxLines = 2,
                        modifier = GlanceModifier.fillMaxWidth().padding(start = (row.depth.toInt() * 16).dp),
                    )
                }
            }
        }
    }
}
