package dev.lysen.not.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.currentState
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
    override val sizeMode = androidx.glance.appwidget.SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent { WidgetLookProvider {
            val version by app.changes.collectAsState()
            val days = NextOptions.days(currentState<androidx.datastore.preferences.core.Preferences>())
            val next = remember(version, days) { load(app, days) }
            NextContent(next)
        } }
    }

    /** [tasks]: what's next, today's first; the first [dueToday] of them are due today. */
    private class Next(val tasks: List<TaskItem>, val dueToday: Int) {
        val task get() = tasks.first()
        val today get() = dueToday > 0
        val more get() = (dueToday - 1).coerceAtLeast(0)
    }

    /** [days]: how many days past today an upcoming task may start; null for any. */
    private fun load(app: NotApp, days: Int?): Next? = try {
        fun top(view: String) = app.store.viewOutline(view, today()).open.filter { it.depth == 0u }.map { it.task }
        val due = top(TODAY_ID)
        val horizon = days?.let { java.time.LocalDate.now().plusDays(it.toLong()).toString() }
        val later = top(UPCOMING_ID).filter { t ->
            val at = t.start ?: t.deadline
            horizon == null || (at != null && at <= horizon)
        }
        val tasks = (due + later).distinctBy { it.id }.take(8)
        tasks.takeIf { it.isNotEmpty() }?.let { Next(it, dueToday = due.size) }
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
                WidgetEmpty(R.drawable.ic_check, "All clear", "Nothing coming up.", open)
                return@WidgetSurface
            }
            // Taller than a cell: the next few, today's first.
            val size = androidx.glance.LocalSize.current
            if (size.height >= 120.dp && next.tasks.size > 1) {
                NextList(next, open)
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
                        style = TextStyle(color = WidgetColors.accentInk, fontSize = 11.wsp, fontWeight = FontWeight.Medium),
                    )
                    Text(
                        next.task.title,
                        maxLines = 1,
                        style = TextStyle(color = WidgetColors.ink, fontSize = 15.wsp, fontWeight = FontWeight.Medium),
                    )
                    val meta = listOfNotNull(
                        scheduleLabel(next.task.start ?: next.task.deadline, next.task.time, null),
                        next.task.tags.firstOrNull()?.let { "#$it" },
                        next.more.takeIf { it > 0 }?.let { "+$it today" },
                    ).joinToString(" · ")
                    Text(meta, maxLines = 1, style = TextStyle(color = WidgetColors.inkMuted, fontSize = 12.wsp))
                }
            }
        }
    }

    @Composable
    private fun NextList(next: Next, open: androidx.glance.action.Action) {
        val size = androidx.glance.LocalSize.current
        val fit = ((size.height - 36.dp) / 46.dp).toInt().coerceIn(2, 8)
        Column(GlanceModifier.fillMaxSize()) {
            Text(
                if (next.today) "Up next" else "Coming up",
                maxLines = 1,
                style = TextStyle(color = WidgetColors.accentInk, fontSize = 12.wsp, fontWeight = FontWeight.Medium),
                modifier = GlanceModifier.padding(start = 8.dp, top = 6.dp, bottom = 2.dp).clickable(open),
            )
            next.tasks.take(fit).forEachIndexed { i, task ->
                Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    WidgetCheck(
                        checked = false,
                        onClick = actionRunCallback<CompleteTask>(actionParametersOf(TASK_ID to task.id)),
                        size = 20.dp,
                        label = "Complete ${task.title}",
                    )
                    Spacer(GlanceModifier.width(4.dp))
                    Column(GlanceModifier.defaultWeight().padding(vertical = 4.wdp).clickable(open)) {
                        Text(task.title, maxLines = 1, style = TextStyle(color = WidgetColors.ink, fontSize = 14.wsp, fontWeight = FontWeight.Medium))
                        val meta = listOfNotNull(
                            if (i >= next.dueToday) scheduleLabel(task.start ?: task.deadline, task.time, null) else task.time?.let { scheduleLabel(task.start, it, null) },
                            task.tags.firstOrNull()?.let { "#$it" },
                        ).joinToString(" · ")
                        if (meta.isNotEmpty()) Text(meta, maxLines = 1, style = TextStyle(color = WidgetColors.inkMuted, fontSize = 12.wsp))
                    }
                }
            }
        }
    }
}

class NextWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = NextWidget()
}
