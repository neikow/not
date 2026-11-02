package dev.lysen.not.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.components.Scaffold
import androidx.glance.appwidget.components.TitleBar
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.provideContent
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import dev.lysen.not.MainActivity
import dev.lysen.not.NotApp
import dev.lysen.not.R
import dev.lysen.not.SyncWorker
import dev.lysen.not.Tab
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.NotException
import java.time.LocalDate

private val HABIT_ID = ActionParameters.Key<String>("habit_id")

/**
 * Today's habits: tap the ring to check in (it counts up to the goal, then resets, like in the app).
 * The day is read at every render; the widget re-renders at least every 30 minutes, so it rolls over
 * shortly after midnight.
 */
class HabitsWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent {
            val version by app.changes.collectAsState()
            val today = LocalDate.now()
            val habits = remember(version, today) {
                try {
                    app.store.habits(today.toString()).filter { it.due }
                } catch (_: NotException) {
                    emptyList()
                }
            }
            GlanceTheme { HabitsWidgetContent(habits) }
        }
    }
}

class HabitsWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = HabitsWidget()
}

class ToggleHabit : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val id = parameters[HABIT_ID] ?: return
        try {
            (context.applicationContext as NotApp).store.toggleHabit(id, LocalDate.now().toString())
        } catch (_: NotException) {
            return // deleted on another device in the meantime
        }
        SyncWorker.pushSoon(context)
    }
}

@Composable
private fun HabitsWidgetContent(habits: List<HabitSummary>) {
    val context = LocalContext.current
    val left = habits.count { it.count < it.habit.goal }
    Scaffold(
        titleBar = {
            TitleBar(
                startIcon = ImageProvider(R.drawable.ic_repeat),
                title = when {
                    habits.isEmpty() -> "Habits"
                    left == 0 -> "Habits · all done"
                    else -> "Habits · $left left"
                },
                modifier = GlanceModifier.clickable(actionStartActivity(MainActivity.openTab(context, Tab.Habits))),
            )
        },
    ) {
        if (habits.isEmpty()) {
            Box(GlanceModifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("Nothing due today", style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant))
            }
        } else {
            LazyColumn(GlanceModifier.fillMaxSize()) {
                items(habits, itemId = { it.habit.id.hashCode().toLong() }) { HabitRow(it) }
            }
        }
    }
}

@Composable
private fun HabitRow(summary: HabitSummary) {
    val habit = summary.habit
    val done = summary.count >= habit.goal
    Row(
        GlanceModifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .clickable(actionRunCallback<ToggleHabit>(actionParametersOf(HABIT_ID to habit.id))),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(GlanceModifier.size(32.dp), contentAlignment = Alignment.Center) {
            Image(
                ImageProvider(if (done) R.drawable.widget_ring_done else R.drawable.widget_ring),
                contentDescription = if (done) "Done" else "Check in",
                colorFilter = ColorFilter.tint(if (done) GlanceTheme.colors.primary else GlanceTheme.colors.outline),
                modifier = GlanceModifier.size(28.dp),
            )
            if (done) {
                Image(
                    ImageProvider(R.drawable.ic_check),
                    contentDescription = null,
                    colorFilter = ColorFilter.tint(GlanceTheme.colors.onPrimary),
                    modifier = GlanceModifier.size(18.dp),
                )
            } else if (summary.count > 0u) {
                Text(
                    summary.count.toString(),
                    style = TextStyle(color = GlanceTheme.colors.primary, fontSize = 12.sp, fontWeight = FontWeight.Bold),
                )
            }
        }
        Spacer(GlanceModifier.width(12.dp))
        Column(GlanceModifier.defaultWeight()) {
            Text(
                habit.title,
                maxLines = 1,
                style = TextStyle(color = GlanceTheme.colors.onSurface, fontSize = 15.sp),
            )
            val details = listOfNotNull(
                if (habit.goal > 1u) "${summary.count} of ${habit.goal}" else null,
                if (summary.streak > 0u) "${summary.streak}-day streak" else null,
            )
            if (details.isNotEmpty()) {
                Text(
                    details.joinToString(" · "),
                    maxLines = 1,
                    style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant, fontSize = 12.sp),
                )
            }
        }
    }
}
