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
import dev.lysen.not.core.TimeSegment
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
            val segments = remember(version, summary?.habit?.id, today) {
                summary?.let { runCatching { app.store.habitTimeline(it.habit.id, today.toString(), nowMs(), dev.lysen.not.utcOffsetMinutes()) }.getOrNull() }.orEmpty()
            }
            TimedContent(summary, segments)
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
        WidgetKind.updateAll(context)
    }
}

@Composable
private fun TimedContent(summary: HabitSummary?, segments: List<TimeSegment>) {
    val context = LocalContext.current
    val open = actionStartActivity(MainActivity.openTab(context, Tab.Habits))
    val size = LocalSize.current
    WidgetSurface {
        if (summary == null) {
            WidgetEmpty(R.drawable.ic_timer, "No timed habit", if (size.height < 100.dp) null else "Long-press to pick one, or add one in Habits.", open)
            return@WidgetSurface
        }
        val now = nowMs()
        val prefs = currentState<androidx.datastore.preferences.core.Preferences>()
        val live = summary.timedLive(now, now)!!
        val t = Timed(
            summary = summary,
            rule = summary.habit.timed!!,
            live = live,
            now = now,
            secured = live.secured(now),
            showsLeft = TimedOptions.showsLeft(prefs, live.atMostOff),
            showDetail = TimedOptions.detail.isOn(prefs),
        )
        // One layout per shape: a lone ring (1×1), a strip (one row tall), a stack (narrow),
        // the ring beside text and button (3×2), and with room to spare, the day's timeline.
        when {
            size.width < 130.dp && size.height < 130.dp -> TimedTiny(t)
            size.height < 100.dp -> TimedStrip(t, open)
            size.width < 200.dp -> TimedStack(t, open)
            else -> TimedBeside(t, open, timeline = if (size.width >= 290.dp && size.height >= 170.dp) segments else null)
        }
    }
}

/** What every layout needs about the habit right now. */
private class Timed(
    val summary: HabitSummary,
    val rule: TimedRule,
    val live: TimedLive,
    val now: Long,
    val secured: Boolean,
    val showsLeft: Boolean,
    val showDetail: Boolean,
) {
    val switch get() = actionRunCallback<SwitchTimedHabit>(actionParametersOf(SWITCH_ID to summary.habit.id, SWITCH_ON to !live.on))
}

/** The ring, or once the day is won a mint disc with a check, with its number inside. */
@Composable
private fun TimedRingBox(t: Timed, ring: androidx.compose.ui.unit.Dp, modifier: GlanceModifier = GlanceModifier) {
    val context = LocalContext.current
    Box(modifier.size(ring), contentAlignment = Alignment.Center) {
        if (t.secured) {
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
                ImageProvider(ringBitmap(LocalWidgetLook.current.palette(context), t.live, (ring.value * 3).toInt(), t.showsLeft)),
                contentDescription = null,
                modifier = GlanceModifier.fillMaxSize(),
            )
            RingCenter(t.rule, t.live, compact = ring < 80.dp, showsLeft = t.showsLeft)
        }
    }
}

/** Title, where it stands, and (room permitting) the detail and streak lines. */
@Composable
private fun TimedLines(t: Timed, lines: Int, center: Boolean = false) {
    val align = if (center) TextAlign.Center else TextAlign.Start
    Text(
        t.summary.habit.title,
        maxLines = 1,
        style = TextStyle(color = WidgetColors.ink, fontSize = 16.wsp, fontWeight = FontWeight.Bold, textAlign = align),
    )
    if (lines < 2) return
    if (t.secured) {
        Text(
            if (t.live.atMostOff) "Goal safe for today" else "Goal met today",
            maxLines = 1,
            style = TextStyle(color = WidgetColors.accentInk, fontSize = 14.wsp, fontWeight = FontWeight.Bold, textAlign = align),
        )
    } else {
        Text(
            headline(t.rule, t.live),
            maxLines = 1,
            style = TextStyle(
                color = if (t.live.on == t.rule.usualOn) WidgetColors.inkMuted else WidgetColors.accentInk,
                fontSize = 14.wsp,
                fontWeight = FontWeight.Medium,
                textAlign = align,
            ),
        )
    }
    if (lines < 3 || !t.showDetail) return
    val third = if (t.secured) {
        if (t.live.atMostOff) "Only ${durationText(t.live.countedMs)} ${stateName(t.rule, false).lowercase()}" else "${durationText(t.live.countedMs)} today"
    } else {
        detail(t.rule, t.live, t.now)
    }
    third?.let { Text(it, maxLines = 1, style = TextStyle(color = WidgetColors.inkFaint, fontSize = 13.wsp, textAlign = align)) }
    if (lines >= 4 && t.secured && t.summary.streak > 1u) {
        Text("${t.summary.streak}-day streak", maxLines = 1, style = TextStyle(color = WidgetColors.inkFaint, fontSize = 13.wsp, textAlign = align))
    }
}

/** 1×1: just the ring, and the whole widget is the switch. */
@Composable
private fun TimedTiny(t: Timed) {
    val size = LocalSize.current
    val ring = (minOf(size.width, size.height) - 16.dp).coerceAtLeast(36.dp)
    Box(GlanceModifier.fillMaxSize().clickable(t.switch), contentAlignment = Alignment.Center) {
        TimedRingBox(t, ring)
    }
}

/** One row tall: small ring, title and state, a compact switch on the end. */
@Composable
private fun TimedStrip(t: Timed, open: androidx.glance.action.Action) {
    val size = LocalSize.current
    Row(GlanceModifier.fillMaxSize().padding(horizontal = 2.dp), verticalAlignment = Alignment.CenterVertically) {
        TimedRingBox(t, (size.height - 16.dp).coerceIn(32.dp, 56.dp), GlanceModifier.clickable(open))
        Spacer(GlanceModifier.width(10.dp))
        if (size.width < 190.dp) {
            // 2×1: no room for words; the ring and the switch.
            Spacer(GlanceModifier.defaultWeight())
        } else {
            Column(GlanceModifier.defaultWeight().clickable(open)) { TimedLines(t, lines = if (size.height < 70.dp) 1 else 2) }
        }
        if (size.width >= 200.dp) {
            Spacer(GlanceModifier.width(8.dp))
            Box(GlanceModifier.width(if (size.width >= 260.dp) 112.dp else 52.dp)) { SwitchButton(t, compact = true, iconOnly = size.width < 260.dp) }
        } else {
            // Too narrow for a button: tapping the ring area opens, the rest switches.
            Spacer(GlanceModifier.width(4.dp))
            val loud = !t.secured && (!t.live.tracked || t.live.on != t.rule.usualOn)
            Box(GlanceModifier.size(40.dp).cornerRadius(20.dp).background(if (loud) WidgetColors.accent else WidgetColors.sunken).clickable(t.switch), contentAlignment = Alignment.Center) {
                Image(
                    ImageProvider(if (t.live.on) R.drawable.ic_pause else R.drawable.ic_play),
                    contentDescription = switchLabel(t.rule, t.live.on),
                    colorFilter = ColorFilter.tint(if (loud) WidgetColors.onAccent else WidgetColors.ink),
                    modifier = GlanceModifier.size(18.dp),
                )
            }
        }
    }
}

/** Narrow and tall (2×2): the ring on top, a line or two, the switch at the bottom. */
@Composable
private fun TimedStack(t: Timed, open: androidx.glance.action.Action) {
    val size = LocalSize.current
    // Room left for two lines and the button decides the ring.
    val ring = minOf(size.height - 122.dp, size.width - 24.dp, 110.dp).coerceAtLeast(40.dp)
    Column(
        GlanceModifier.fillMaxSize().padding(bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TimedRingBox(t, ring, GlanceModifier.clickable(open))
        Spacer(GlanceModifier.height(6.dp))
        Column(GlanceModifier.fillMaxWidth().clickable(open), horizontalAlignment = Alignment.CenterHorizontally) {
            TimedLines(t, lines = if (size.height >= 200.dp) 3 else 2, center = true)
        }
        Spacer(GlanceModifier.height(8.dp))
        SwitchButton(t, compact = true)
    }
}

/**
 * 3×2 and up: the ring fills the height on the left, text and the switch stacked beside it; with
 * room to spare, the day's [timeline] runs underneath.
 */
@Composable
private fun TimedBeside(t: Timed, open: androidx.glance.action.Action, timeline: List<TimeSegment>?) {
    val size = LocalSize.current
    val compact = size.height < 150.dp
    val tall = size.height >= 240.dp
    val timelineHeight = if (timeline != null) (if (tall) 46.dp else 30.dp) else 0.dp
    val ring = minOf(size.height - 28.dp - timelineHeight, size.width * 0.36f, if (tall) 150.dp else 120.dp).coerceAtLeast(52.dp)
    Column(GlanceModifier.fillMaxSize().padding(start = 4.dp, end = 4.dp, top = 4.dp, bottom = 8.dp)) {
        Row(GlanceModifier.fillMaxWidth().defaultWeight(), verticalAlignment = Alignment.CenterVertically) {
            TimedRingBox(t, ring, GlanceModifier.clickable(open))
            Spacer(GlanceModifier.width(14.dp))
            Column(GlanceModifier.defaultWeight(), verticalAlignment = Alignment.CenterVertically) {
                Column(GlanceModifier.fillMaxWidth().clickable(open)) {
                    TimedLines(t, lines = if (compact) 3 else 4)
                }
                Spacer(GlanceModifier.height(if (compact) 6.dp else 10.dp))
                SwitchButton(t, compact)
            }
        }
        if (timeline != null) {
            Spacer(GlanceModifier.height(10.dp))
            DayBar(t, timeline, size.width - 28.dp, hours = tall)
        }
    }
}

/** The day from midnight to midnight: on in mint, the unusual state amber, a tick at now. */
@Composable
private fun DayBar(t: Timed, segments: List<TimeSegment>, width: androidx.compose.ui.unit.Dp, hours: Boolean) {
    val context = LocalContext.current
    val colors = LocalWidgetLook.current.palette(context)
    val pxW = (width.value * 3).toInt().coerceAtLeast(30)
    val pxH = 36
    val bitmap = Bitmap.createBitmap(pxW, pxH, Bitmap.Config.ARGB_8888)
    val canvas = android.graphics.Canvas(bitmap)
    val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    val start = java.time.LocalDate.now().atStartOfDay(java.time.ZoneId.systemDefault()).toInstant().toEpochMilli()
    val day = 24 * 3_600_000f
    paint.color = colors.sunken.toArgb()
    canvas.drawRoundRect(RectF(0f, 0f, pxW.toFloat(), pxH.toFloat()), 12f, 12f, paint)
    for (s in segments) {
        val x0 = pxW * ((s.start - start) / day)
        val x1 = pxW * ((s.end - start) / day)
        paint.color = when {
            s.on -> colors.accent.toArgb()
            s.on == t.rule.usualOn -> colors.inkFaint.copy(alpha = 0.45f).toArgb()
            else -> 0xFFE29A2D.toInt()
        }
        canvas.drawRect(x0, 0f, maxOf(x1, x0 + 2f), pxH.toFloat(), paint)
    }
    paint.color = colors.ink.toArgb()
    val x = pxW * ((t.now - start) / day)
    canvas.drawRect(x - 2f, 0f, x + 2f, pxH.toFloat(), paint)
    Column(GlanceModifier.fillMaxWidth()) {
        Image(ImageProvider(bitmap), contentDescription = "Today's timeline", modifier = GlanceModifier.width(width).height(12.dp).cornerRadius(6.dp))
        if (hours) {
            Row(GlanceModifier.width(width).padding(top = 3.dp)) {
                listOf("0", "6", "12", "18", "24").forEachIndexed { i, h ->
                    Text(
                        h,
                        style = TextStyle(
                            color = WidgetColors.inkFaint,
                            fontSize = 10.wsp,
                            textAlign = when (i) {
                                0 -> TextAlign.Start
                                4 -> TextAlign.End
                                else -> TextAlign.Center
                            },
                        ),
                        modifier = GlanceModifier.defaultWeight(),
                    )
                }
            }
        }
    }
}

/** The one thing to do, under the text: the usual state's action in mint; quiet once the day is won. */
@Composable
private fun SwitchButton(t: Timed, compact: Boolean, iconOnly: Boolean = false) {
    val rule = t.rule
    val live = t.live
    val goingUsual = live.tracked && live.on != rule.usualOn
    val loud = !t.secured && (goingUsual || !live.tracked)
    val fg = if (loud) WidgetColors.onAccent else WidgetColors.ink
    Row(
        GlanceModifier
            .fillMaxWidth()
            .height(if (compact) 36.dp else 42.dp)
            .cornerRadius(21.dp)
            .background(if (loud) WidgetColors.accent else WidgetColors.sunken)
            .clickable(t.switch),
        verticalAlignment = Alignment.CenterVertically,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Image(
            ImageProvider(if (live.on) R.drawable.ic_pause else R.drawable.ic_play),
            contentDescription = if (iconOnly) switchLabel(rule, live.on) else null,
            colorFilter = ColorFilter.tint(fg),
            modifier = GlanceModifier.size(18.dp),
        )
        if (!iconOnly) {
            Spacer(GlanceModifier.width(6.dp))
            Text(switchLabel(rule, live.on), maxLines = 1, style = TextStyle(color = fg, fontSize = 15.wsp, fontWeight = FontWeight.Bold))
        }
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
