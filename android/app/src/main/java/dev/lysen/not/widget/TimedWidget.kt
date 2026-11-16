package dev.lysen.not.widget

import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Paint
import android.graphics.RectF
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextAlign
import androidx.glance.text.TextStyle
import dev.lysen.not.MainActivity
import dev.lysen.not.NotApp
import dev.lysen.not.R
import dev.lysen.not.SyncWorker
import dev.lysen.not.Tab
import dev.lysen.not.TimedLive
import dev.lysen.not.clockText
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore
import dev.lysen.not.core.TimedRule
import dev.lysen.not.durationText
import dev.lysen.not.habitsNow
import dev.lysen.not.nowMs
import dev.lysen.not.stateName
import dev.lysen.not.switchLabel
import dev.lysen.not.timedLive
import dev.lysen.not.ui.DarkColors
import dev.lysen.not.ui.LightColors
import dev.lysen.not.ui.NotColors
import java.time.LocalDate

val TIMED_KEY = stringPreferencesKey("timed_habit_id")
private val SWITCH_ID = ActionParameters.Key<String>("habit_id")
private val SWITCH_ON = ActionParameters.Key<Boolean>("on")

/**
 * One timed habit, big: today's ring, where it stands, and one large button to switch it. Every
 * line states facts that stay true while it sits there ("Paused since 14:20", "until 16:05"), so
 * nothing needs to tick; it re-renders on every change and every 30 minutes.
 */
class TimedWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent {
            val version by app.changes.collectAsState()
            val habitId = currentState(TIMED_KEY)
            val today = LocalDate.now()
            val summary = remember(version, habitId, today) {
                try {
                    val timed = app.store.habitsNow(today.toString()).filter { it.habit.timed != null }
                    // Not picked yet (some launchers skip setup): the first timed habit.
                    timed.firstOrNull { it.habit.id == habitId } ?: timed.firstOrNull().takeIf { habitId == null }
                } catch (e: NotException) {
                    logLoadFailure("timed", e)
                    null
                }
            }
            TimedContent(summary)
        }
    }
}

class TimedWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = TimedWidget()
}

class SwitchTimedHabit : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val id = parameters[SWITCH_ID] ?: return
        val on = parameters[SWITCH_ON] ?: return
        val app = context.applicationContext as NotApp
        val now = nowMs()
        try {
            app.store.setHabitState(id, on, now, null, now)
        } catch (_: NotException) {
            return // deleted on another device in the meantime
        }
        app.timedAlerts.refresh()
        SyncWorker.pushSoon(context)
    }
}

/** Picks the timed habit a widget shows. */
class TimedWidgetConfigActivity : PickerConfigActivity() {
    override val dialogTitle = "Show in widget"
    override val widget = TimedWidget()

    override fun options(store: NotStore) =
        store.habitsNow(dev.lysen.not.today()).filter { it.habit.timed != null }.map { PickerOption(it.habit.id, it.habit.title) }

    override fun save(prefs: MutablePreferences, choice: String) {
        prefs[TIMED_KEY] = choice
    }
}

@Composable
private fun TimedContent(summary: HabitSummary?) {
    val context = LocalContext.current
    val open = actionStartActivity(MainActivity.openTab(context, Tab.Habits))
    WidgetSurface {
        if (summary == null) {
            WidgetEmpty(R.drawable.ic_timer, "No timed habit", "Long-press to pick one, or add one in Habits.", open)
            return@WidgetSurface
        }
        val rule = summary.habit.timed!!
        val now = nowMs()
        val live = summary.timedLive(now, now)!!
        val size = LocalSize.current
        val compact = size.height < 150.dp
        val ring = if (compact) 56.dp else 84.dp

        Row(
            GlanceModifier.fillMaxWidth().padding(start = 6.dp, end = 4.dp, top = 6.dp).clickable(open),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(GlanceModifier.size(ring), contentAlignment = Alignment.Center) {
                Image(
                    ImageProvider(ringBitmap(context, live, (ring.value * 3).toInt())),
                    contentDescription = null,
                    modifier = GlanceModifier.fillMaxSize(),
                )
                Text(
                    centerText(live),
                    maxLines = 1,
                    style = TextStyle(
                        color = WidgetColors.ink,
                        fontSize = if (compact) 12.sp else 15.sp,
                        fontWeight = FontWeight.Bold,
                        textAlign = TextAlign.Center,
                    ),
                )
            }
            Spacer(GlanceModifier.width(12.dp))
            Column(GlanceModifier.defaultWeight()) {
                Text(
                    summary.habit.title,
                    maxLines = 1,
                    style = TextStyle(color = WidgetColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold),
                )
                Text(
                    headline(rule, live),
                    maxLines = 1,
                    style = TextStyle(color = if (live.on == rule.usualOn) WidgetColors.inkMuted else WidgetColors.accentInk, fontSize = 14.sp, fontWeight = FontWeight.Medium),
                )
                detail(rule, live, now)?.let {
                    Text(it, maxLines = 2, style = TextStyle(color = WidgetColors.inkFaint, fontSize = 13.sp))
                }
            }
        }
        Spacer(GlanceModifier.defaultWeight())
        // The one thing to do: a full-width button, the usual state's action in mint.
        val goingUsual = live.tracked && live.on != rule.usualOn
        Row(
            GlanceModifier
                .fillMaxWidth()
                .height(if (compact) 40.dp else 48.dp)
                .cornerRadius(24.dp)
                .background(if (goingUsual || !live.tracked) WidgetColors.accent else WidgetColors.sunken)
                .clickable(actionRunCallback<SwitchTimedHabit>(actionParametersOf(SWITCH_ID to summary.habit.id, SWITCH_ON to !live.on))),
            verticalAlignment = Alignment.CenterVertically,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            val fg = if (goingUsual || !live.tracked) WidgetColors.onAccent else WidgetColors.ink
            Image(
                ImageProvider(if (live.on) R.drawable.ic_pause else R.drawable.ic_play),
                contentDescription = null,
                colorFilter = ColorFilter.tint(fg),
                modifier = GlanceModifier.size(20.dp),
            )
            Spacer(GlanceModifier.width(8.dp))
            Text(switchLabel(rule, live.on), style = TextStyle(color = fg, fontSize = 15.sp, fontWeight = FontWeight.Bold))
        }
        Spacer(GlanceModifier.height(6.dp))
    }
}

/** "Paused since 14:20", "On since 07:00", "Not started". */
private fun headline(rule: TimedRule, live: TimedLive): String =
    if (!live.tracked) "Not started" else "${stateName(rule, live.on)} since ${clockText(live.since!!)}"

/** What the running stretch leads to, as a clock time so it stays true: "1h 35m left · until 16:05". */
private fun detail(rule: TimedRule, live: TimedLive, now: Long): String? {
    if (!live.tracked) return if (rule.atMostOff) "At most ${durationText(live.goalMs)} off a day" else "${durationText(live.goalMs)} a day"
    val counts = live.on != rule.atMostOff
    return when {
        live.atMostOff && live.leftMs <= 0 -> "${durationText(-live.leftMs)} over today"
        live.atMostOff && counts -> "Allowance runs out at ${clockText(now + live.leftMs)}"
        live.atMostOff -> "${durationText(live.leftMs)} off left today"
        live.met -> "Goal met today"
        counts -> "Goal reached at ${clockText(now + live.leftMs)}"
        else -> "${durationText(live.leftMs)} to go today"
    }
}

/** At most: allowance left. At least: time so far. */
private fun centerText(live: TimedLive): String = when {
    !live.tracked -> "–"
    live.atMostOff && live.leftMs < 0 -> "-${durationText(-live.leftMs)}"
    live.atMostOff -> durationText(live.leftMs)
    else -> durationText(live.countedMs)
}

/** The app's ring, drawn: a faint track and a mint arc (amber when an allowance runs low, red when over). */
private fun ringBitmap(context: Context, live: TimedLive, px: Int): Bitmap {
    val night = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
    val colors: NotColors = if (night) DarkColors else LightColors
    val bitmap = Bitmap.createBitmap(px, px, Bitmap.Config.ARGB_8888)
    val canvas = android.graphics.Canvas(bitmap)
    val stroke = px * 0.09f
    val rect = RectF(stroke / 2, stroke / 2, px - stroke / 2, px - stroke / 2)
    val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = stroke
        strokeCap = Paint.Cap.ROUND
    }
    paint.color = colors.sunken.toArgb()
    canvas.drawArc(rect, 0f, 360f, false, paint)
    paint.color = when {
        live.atMostOff && live.leftMs < 0 -> colors.danger.toArgb()
        live.atMostOff && live.ringFraction < 0.25f -> 0xFFE29A2D.toInt()
        else -> colors.accent.toArgb()
    }
    val sweep = if (live.atMostOff && live.leftMs < 0) 360f else 360f * live.ringFraction
    if (sweep > 0f) canvas.drawArc(rect, -90f, sweep, false, paint)
    return bitmap
}
