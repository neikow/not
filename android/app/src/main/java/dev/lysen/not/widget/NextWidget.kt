package dev.lysen.not.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.LocalContext
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.provideContent
import androidx.glance.layout.Alignment
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import dev.lysen.not.MainActivity
import dev.lysen.not.NotApp
import dev.lysen.not.R
import dev.lysen.not.TODAY_ID
import dev.lysen.not.UPCOMING_ID
import dev.lysen.not.core.NotException
import dev.lysen.not.core.TaskItem
import dev.lysen.not.scheduleLabel
import dev.lysen.not.today

/**
 * The one thing up next: the first task due today (overdue first, then by time), else the soonest
 * upcoming one. Small enough for a corner of the home screen; tick it and the next one shows.
 */
class NextWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent {
            val version by app.changes.collectAsState()
            val next = remember(version) { load(app) }
            NextContent(next)
        }
    }

    private class Next(val task: TaskItem, val more: Int, val today: Boolean)

    private fun load(app: NotApp): Next? = try {
        fun top(view: String) = app.store.viewOutline(view, today()).open.filter { it.depth == 0u }.map { it.task }
        val due = top(TODAY_ID)
        val pick = due.firstOrNull() ?: top(UPCOMING_ID).firstOrNull()
        pick?.let { task -> Next(task, more = (due.size - 1).coerceAtLeast(0), today = due.isNotEmpty()) }
    } catch (e: NotException) {
        logLoadFailure("next", e)
        null
    }

    @Composable
    private fun NextContent(next: Next?) {
        val context = LocalContext.current
        val open = actionStartActivity(MainActivity.openList(context, if (next?.today == false) UPCOMING_ID else TODAY_ID))
        WidgetSurface {
            if (next == null) {
                WidgetEmpty(R.drawable.ic_check, "All clear", "Nothing dated is left.", open)
                return@WidgetSurface
            }
            // Check on the left, three short lines beside it: fits a 2×1 cell.
            Row(
                GlanceModifier.fillMaxSize().padding(start = 0.dp, end = 4.dp).clickable(open),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                WidgetCheck(
                    checked = false,
                    onClick = actionRunCallback<CompleteTask>(actionParametersOf(TASK_ID to next.task.id)),
                    label = "Complete ${next.task.title}",
                )
                Spacer(GlanceModifier.width(4.dp))
                Column(GlanceModifier.defaultWeight()) {
                    Text(
                        if (next.today) "Up next" else "Coming up",
                        maxLines = 1,
                        style = TextStyle(color = WidgetColors.accentInk, fontSize = 11.sp, fontWeight = FontWeight.Medium),
                    )
                    Text(
                        next.task.title,
                        maxLines = 1,
                        style = TextStyle(color = WidgetColors.ink, fontSize = 15.sp, fontWeight = FontWeight.Medium),
                    )
                    val meta = listOfNotNull(
                        scheduleLabel(next.task.start ?: next.task.deadline, next.task.time, null),
                        next.task.tags.firstOrNull()?.let { "#$it" },
                        next.more.takeIf { it > 0 }?.let { "+$it today" },
                    ).joinToString(" · ")
                    Text(meta, maxLines = 1, style = TextStyle(color = WidgetColors.inkMuted, fontSize = 12.sp))
                }
            }
        }
    }
}

class NextWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = NextWidget()
}
