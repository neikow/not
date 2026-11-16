package dev.lysen.not.widget

import dev.lysen.not.doneToday
import dev.lysen.not.isDaily
import dev.lysen.not.onToday
import dev.lysen.not.periodName
import dev.lysen.not.streakText
import dev.lysen.not.habitsNow

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.currentState
import androidx.glance.GlanceModifier
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
        provideContent { WidgetLookProvider {
            val version by app.changes.collectAsState()
            val today = LocalDate.now()
            // All habits; the widget lists the ones due today.
            val all = remember(version, today) {
                try {
                    app.store.habitsNow(today.toString())
                } catch (e: NotException) {
                    logLoadFailure("habits", e)
                    emptyList()
                }
            }
            val prefs = currentState<androidx.datastore.preferences.core.Preferences>()
            // The habits picked in setup (all when none), due today; done ones unless hidden.
            val picked = prefs[HabitsOptions.ids].orEmpty()
            val hideDone = HabitsOptions.hideDone.of(prefs) == "hide"
            val shown = all.filter { (picked.isEmpty() || it.habit.id in picked) && it.onToday && !(hideDone && it.doneToday) }
            val due = all.filter { (picked.isEmpty() || it.habit.id in picked) && it.onToday }
            HabitsWidgetContent(shown, left = due.count { !it.doneToday }, anyDue = due.isNotEmpty(), hasAny = all.isNotEmpty(), streaks = HabitsOptions.streaks.isOn(prefs))
        } }
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
private fun HabitsWidgetContent(habits: List<HabitSummary>, left: Int, anyDue: Boolean, hasAny: Boolean, streaks: Boolean) {
    val context = LocalContext.current
    val open = actionStartActivity(MainActivity.openTab(context, Tab.Habits))
    WidgetSurface {
        WidgetHeader(
            title = "Habits",
            open = open,
            detail = when {
                !anyDue -> null
                left == 0 -> "All done"
                else -> "$left left"
            },
            detailAccent = anyDue && left == 0,
        )
        if (habits.isEmpty()) {
            if (anyDue) {
                WidgetEmpty(R.drawable.ic_check, "All done for today", null, open)
            } else if (hasAny) {
                WidgetEmpty(R.drawable.ic_check, "Nothing due today", "Enjoy the day off.", open)
            } else {
                WidgetEmpty(R.drawable.ic_repeat, "No habits yet", "Tap to add one in the app.", open)
            }
        } else {
            WidgetItems(habits, { it.habit.id.hashCode().toLong() }) { HabitRow(it, streaks) }
        }
    }
}

/** The whole row checks in: habits are tapped many times a day, so the target is big. */
@Composable
private fun HabitRow(summary: HabitSummary, streaks: Boolean) {
    val habit = summary.habit
    val rule = habit.timed
    val done = summary.doneToday
    // Timed: "started" = out of the usual state right now (paused, or running).
    val started = if (rule != null) summary.timed?.let { it.since != null && it.on != rule.usualOn } == true else summary.count > 0u && !done
    Row(
        GlanceModifier
            .fillMaxWidth()
            .padding(vertical = 2.wdp)
            .clickable(actionRunCallback<ToggleHabit>(actionParametersOf(HABIT_ID to habit.id))),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Ring like the app's: faint when untouched, mint once started, a mint disc with a check when done.
        Box(GlanceModifier.size(40.wdp), contentAlignment = Alignment.Center) {
            Image(
                ImageProvider(if (done) R.drawable.widget_ring_done else R.drawable.widget_ring),
                contentDescription = if (done) "Done" else "Check in",
                colorFilter = ColorFilter.tint(if (done || started) WidgetColors.accent else WidgetColors.inkFaint),
                modifier = GlanceModifier.size(28.dp),
            )
            if (done) {
                Image(
                    ImageProvider(R.drawable.ic_check),
                    contentDescription = null,
                    colorFilter = ColorFilter.tint(WidgetColors.onAccent),
                    modifier = GlanceModifier.size(18.dp),
                )
            } else if (started && rule == null) {
                Text(
                    summary.count.toString(),
                    style = TextStyle(color = WidgetColors.accentInk, fontSize = 12.wsp, fontWeight = FontWeight.Bold),
                )
            }
        }
        Spacer(GlanceModifier.width(8.dp))
        Column(GlanceModifier.defaultWeight()) {
            Text(
                habit.title,
                maxLines = 1,
                style = TextStyle(color = if (done) WidgetColors.inkMuted else WidgetColors.ink, fontSize = 15.wsp),
            )
            val details = listOfNotNull(
                when {
                    rule != null -> summary.timed?.let { t ->
                        if (t.since == null) "Not started — tap to ${dev.lysen.not.switchLabel(rule, false).lowercase()}"
                        else "${dev.lysen.not.stateName(rule, t.on)} since ${dev.lysen.not.clockText(t.since!!)}"
                    }
                    !habit.isDaily -> "${summary.progress} of ${habit.goal} this ${periodName(habit.period)}"
                    habit.goal > 1u -> "${summary.count} of ${habit.goal}"
                    else -> null
                },
                if (streaks && summary.streak > 0u) "${streakText(summary.streak, habit.period)} streak" else null,
            )
            if (details.isNotEmpty()) {
                Text(
                    details.joinToString(" · "),
                    maxLines = 1,
                    style = TextStyle(color = WidgetColors.inkMuted, fontSize = 12.wsp),
                )
            }
        }
    }
}
