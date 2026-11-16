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
import androidx.glance.appwidget.AndroidRemoteViews
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
import dev.lysen.not.secured
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
        provideContent { WidgetLookProvider {
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
        } }
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
        val secured = live.secured(now)
        val prefs = currentState<androidx.datastore.preferences.core.Preferences>()
        val showsLeft = TimedOptions.showsLeft(prefs, live.atMostOff)
        val showDetail = TimedOptions.detail.isOn(prefs)
        val size = LocalSize.current
        val compact = size.height < 150.dp
        // The ring fills the widget's height (less the card's padding), up to 40% of its width.
        val ring = minOf(size.height - 28.dp, size.width * 0.4f, 120.dp).coerceAtLeast(52.dp)

        // Ring on the left, everything else stacked beside it, centered: no dead space between
        // the text and the button, whatever the widget's size.
        Row(
            GlanceModifier.fillMaxSize().padding(start = 4.dp, end = 4.dp, top = 4.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(GlanceModifier.size(ring).clickable(open), contentAlignment = Alignment.Center) {
                if (secured) {
                    // Done for the day: a solid mint disc with a check, like a ticked habit.
                    Box(GlanceModifier.fillMaxSize().cornerRadius(ring / 2).background(WidgetColors.accent), contentAlignment = Alignment.Center) {
                        Image(
                            ImageProvider(R.drawable.ic_check),
                            contentDescription = "Goal met",
                            colorFilter = ColorFilter.tint(WidgetColors.onAccent),
                            modifier = GlanceModifier.size(ring * 0.5f),
                        )
                    }
                } else {
                    Image(
                        ImageProvider(ringBitmap(LocalWidgetLook.current.palette(context), live, (ring.value * 3).toInt(), showsLeft)),
                        contentDescription = null,
                        modifier = GlanceModifier.fillMaxSize(),
                    )
                    RingCenter(rule, live, compact = ring < 80.dp, showsLeft = showsLeft)
                }
            }
            Spacer(GlanceModifier.width(14.dp))
            Column(GlanceModifier.defaultWeight(), verticalAlignment = Alignment.CenterVertically) {
                Column(GlanceModifier.fillMaxWidth().clickable(open)) {
                    Text(
                        summary.habit.title,
                        maxLines = 1,
                        style = TextStyle(color = WidgetColors.ink, fontSize = 16.wsp, fontWeight = FontWeight.Bold),
                    )
                    if (secured) {
                        Text(
                            if (live.atMostOff) "Goal safe for today" else "Goal met today",
                            maxLines = 1,
                            style = TextStyle(color = WidgetColors.accentInk, fontSize = 14.wsp, fontWeight = FontWeight.Bold),
                        )
                        Text(
                            if (live.atMostOff) "Only ${durationText(live.countedMs)} ${stateName(rule, false).lowercase()}" else "${durationText(live.countedMs)} today",
                            maxLines = 1,
                            style = TextStyle(color = WidgetColors.inkMuted, fontSize = 13.wsp),
                        )
                        if (!compact && showDetail) {
                            Text(
                                if (summary.streak > 1u) "${summary.streak}-day streak" else "Well done",
                                maxLines = 1,
                                style = TextStyle(color = WidgetColors.inkFaint, fontSize = 13.wsp),
                            )
                        }
                    } else {
                        Text(
                            headline(rule, live),
                            maxLines = 1,
                            style = TextStyle(color = if (live.on == rule.usualOn) WidgetColors.inkMuted else WidgetColors.accentInk, fontSize = 14.wsp, fontWeight = FontWeight.Medium),
                        )
                        detail(rule, live, now)?.takeIf { showDetail }?.let {
                            Text(it, maxLines = if (compact) 1 else 2, style = TextStyle(color = WidgetColors.inkFaint, fontSize = 13.wsp))
                        }
                    }
                }
                Spacer(GlanceModifier.height(if (compact) 6.dp else 10.dp))
                SwitchButton(summary, rule, live, secured, compact)
            }
        }
    }
}

/** The one thing to do, under the text: the usual state's action in mint; quiet once the day is won. */
@Composable
private fun SwitchButton(summary: HabitSummary, rule: TimedRule, live: TimedLive, secured: Boolean, compact: Boolean) {
    val goingUsual = live.tracked && live.on != rule.usualOn
    val loud = !secured && (goingUsual || !live.tracked)
    val fg = if (loud) WidgetColors.onAccent else WidgetColors.ink
    Row(
        GlanceModifier
            .fillMaxWidth()
            .height(if (compact) 36.dp else 42.dp)
            .cornerRadius(21.dp)
            .background(if (loud) WidgetColors.accent else WidgetColors.sunken)
            .clickable(actionRunCallback<SwitchTimedHabit>(actionParametersOf(SWITCH_ID to summary.habit.id, SWITCH_ON to !live.on))),
        verticalAlignment = Alignment.CenterVertically,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Image(
            ImageProvider(if (live.on) R.drawable.ic_pause else R.drawable.ic_play),
            contentDescription = null,
            colorFilter = ColorFilter.tint(fg),
            modifier = GlanceModifier.size(18.dp),
        )
        Spacer(GlanceModifier.width(6.dp))
        Text(switchLabel(rule, live.on), style = TextStyle(color = fg, fontSize = 15.wsp, fontWeight = FontWeight.Bold))
    }
}

/**
 * The number inside the ring. While the running stretch moves it (an allowance being used, time
 * on adding up) it's a system chronometer that ticks by itself; otherwise plain text.
 */
@Composable
private fun RingCenter(rule: TimedRule, live: TimedLive, compact: Boolean, showsLeft: Boolean) {
    val counts = live.tracked && live.on != rule.atMostOff
    val size = if (compact) 12f else 15f
    if (!counts) {
        Text(
            centerText(live, showsLeft),
            maxLines = 1,
            style = TextStyle(color = WidgetColors.ink, fontSize = size.wsp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center),
        )
        return
    }
    val context = LocalContext.current
    val look = LocalWidgetLook.current
    val scale = look.textSize.scale
    val elapsed = android.os.SystemClock.elapsedRealtime()
    val views = android.widget.RemoteViews(context.packageName, R.layout.widget_chronometer).apply {
        if (showsLeft) {
            // Counts what's left down (past zero, how far over).
            setChronometer(R.id.widget_chronometer, elapsed + live.leftMs, null, true)
            setChronometerCountDown(R.id.widget_chronometer, true)
        } else {
            setChronometer(R.id.widget_chronometer, elapsed - live.countedMs, null, true)
        }
        setTextViewTextSize(R.id.widget_chronometer, android.util.TypedValue.COMPLEX_UNIT_SP, (size - 1f) * scale)
        // RemoteViews can't follow the look's theme on its own: color it like the rest.
        setTextColor(R.id.widget_chronometer, look.palette(context).ink.toArgb())
    }
    AndroidRemoteViews(views)
}

/** "Paused at 14:20", "On at 07:00", "Not started": short enough for a 3-wide widget. */
private fun headline(rule: TimedRule, live: TimedLive): String =
    if (!live.tracked) "Not started" else "${stateName(rule, live.on)} at ${clockText(live.since!!)}"

/** What the running stretch leads to, as a clock time so it stays true: "Allowance runs out at 16:05". */
private fun detail(rule: TimedRule, live: TimedLive, now: Long): String? {
    if (!live.tracked) return if (rule.atMostOff) "At most ${durationText(live.goalMs)} off a day" else "${durationText(live.goalMs)} a day"
    val counts = live.on != rule.atMostOff
    return when {
        live.atMostOff && live.leftMs <= 0 -> "Over the ${durationText(live.goalMs)} allowance today"
        live.atMostOff && counts -> "Runs out at ${clockText(now + live.leftMs)}"
        live.atMostOff -> "Allowance left today"
        counts -> "Goal at ${clockText(now + live.leftMs)}"
        else -> "of ${durationText(live.goalMs)} today"
    }
}

/** Time left (allowance or to go), or time so far. */
private fun centerText(live: TimedLive, showsLeft: Boolean): String = when {
    !live.tracked -> "–"
    !showsLeft -> durationText(live.countedMs)
    live.leftMs < 0 -> "-${durationText(-live.leftMs)}"
    else -> durationText(live.leftMs)
}

/** The app's ring, drawn: a faint track and a mint arc (amber when an allowance runs low, red when over). */
/** [showsLeft]: the arc is what's left (of the allowance, or to go) rather than what's done. */
private fun ringBitmap(colors: NotColors, live: TimedLive, px: Int, showsLeft: Boolean): Bitmap {
    val bitmap = Bitmap.createBitmap(px, px, Bitmap.Config.ARGB_8888)
    val canvas = android.graphics.Canvas(bitmap)
    val stroke = px * 0.09f
    val rect = RectF(stroke / 2, stroke / 2, px - stroke / 2, px - stroke / 2)
    val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = stroke
        strokeCap = Paint.Cap.ROUND
    }
    val left = if (live.goalMs > 0) (live.leftMs.toFloat() / live.goalMs).coerceIn(0f, 1f) else 0f
    val fraction = if (showsLeft) left else 1f - left
    paint.color = colors.sunken.toArgb()
    canvas.drawArc(rect, 0f, 360f, false, paint)
    paint.color = when {
        live.atMostOff && live.leftMs < 0 -> colors.danger.toArgb()
        live.atMostOff && left < 0.25f -> 0xFFE29A2D.toInt()
        else -> colors.accent.toArgb()
    }
    val sweep = if (live.atMostOff && live.leftMs < 0) 360f else 360f * fraction
    if (sweep > 0f) canvas.drawArc(rect, -90f, sweep, false, paint)
    return bitmap
}
