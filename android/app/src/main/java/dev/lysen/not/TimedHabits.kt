package dev.lysen.not

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.TimeSegment
import dev.lysen.not.core.TimedRule
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.RoundCheck
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.delay
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter

private const val MINUTE = 60_000L
private const val HOUR = 60 * MINUTE

/** Amber for an allowance running low; the app's other warm tone. */
private val Amber = Color(0xFFE29A2D)

/** The clock, stepping on whole seconds, so timed habits count up on screen. */
@Composable
fun rememberNow(stepMs: Long = 1_000L): Long {
    val now by produceState(nowMs()) {
        while (true) {
            delay(stepMs - nowMs() % stepMs)
            value = nowMs()
        }
    }
    return now
}

private fun dayStart(date: LocalDate): Long = date.atStartOfDay(ZoneId.systemDefault()).toInstant().toEpochMilli()

/**
 * Progress for a timed habit: time toward the goal fills the ring; for an allowance ("at most"),
 * what's left of it, turning amber under a quarter and red once over. A mint disc once a goal
 * of time on is met.
 */
@Composable
fun TimedRing(live: TimedLive?, size: Dp, center: String? = null) {
    val colors = Not.colors
    val fraction by animateFloatAsState(live?.ringFraction ?: 0f, label = "timed")
    val over = live != null && live.atMostOff && live.leftMs < 0
    val met = live != null && !live.atMostOff && live.met
    val arc = when {
        over -> colors.danger
        live != null && live.atMostOff && fraction < 0.25f -> Amber
        else -> colors.accent
    }
    val fill by animateColorAsState(if (met) colors.accent else Color.Transparent, label = "met")
    Box(Modifier.size(size).background(fill, CircleShape), contentAlignment = Alignment.Center) {
        if (!met) {
            Canvas(Modifier.fillMaxSize()) {
                val stroke = (size.value * 0.09f).coerceAtLeast(3f).dp.toPx()
                drawCircle(colors.sunken, radius = (this.size.minDimension - stroke) / 2, style = Stroke(stroke))
                val sweep = if (over) 360f else 360f * fraction
                if (sweep > 0f) {
                    drawArc(
                        arc, -90f, sweep, useCenter = false,
                        topLeft = Offset(stroke / 2, stroke / 2),
                        size = Size(this.size.width - stroke, this.size.height - stroke),
                        style = Stroke(stroke, cap = StrokeCap.Round),
                    )
                }
            }
        }
        if (center != null) {
            Text(center, style = Not.type.label.merge(fontSize = (size.value * 0.19f).sp), color = if (met) colors.onAccent else colors.ink, maxLines = 1)
        } else if (met) {
            Icon(R.drawable.ic_check, "Goal met", tint = colors.onAccent, size = size * 0.55f)
        }
    }
}

/**
 * The switch: going back to the usual state (Resume) or starting is mint, leaving it (Pause,
 * Stop) is quiet. Long-press to say it happened earlier.
 */
@Composable
fun SwitchPill(rule: TimedRule, on: Boolean, tracked: Boolean, onSwitch: () -> Unit, onLongPress: () -> Unit, modifier: Modifier = Modifier, large: Boolean = false) {
    val colors = Not.colors
    val primary = !tracked || on != rule.usualOn || (!rule.usualOn && !on)
    val leavingUsual = tracked && on == rule.usualOn
    val bg by animateColorAsState(if (primary && !leavingUsual) colors.accent else colors.sunken, label = "pill")
    val fg = if (primary && !leavingUsual) colors.onAccent else colors.ink
    Row(
        modifier
            .height(if (large) 52.dp else 40.dp)
            .clip(CircleShape)
            .background(bg)
            .pressable(onSwitch, pressedScale = 0.94f, onLongClick = onLongPress, onLongClickLabel = "It happened earlier")
            .padding(horizontal = if (large) 22.dp else 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
    ) {
        Icon(if (on) R.drawable.ic_pause else R.drawable.ic_play, null, tint = fg, size = if (large) 20.dp else 16.dp)
        Text(switchLabel(rule, on), style = if (large) Not.type.bodyStrong else Not.type.label, color = fg, maxLines = 1)
    }
}

/** A timed habit in the list: ring, title, where it stands (counting live), and the switch. */
@Composable
fun TimedRow(summary: HabitSummary, state: HabitsState, viewModel: HabitsViewModel, onEarlier: (Boolean) -> Unit, modifier: Modifier) {
    val haptics = LocalHapticFeedback.current
    val habit = summary.habit
    val rule = habit.timed ?: return
    val colors = Not.colors
    val now = rememberNow()
    val live = summary.timedLive(now, state.loadedAt, state.isToday) ?: return
    Row(
        modifier
            .fillMaxWidth()
            .background(colors.background)
            .pressable({ viewModel.open(habit.id) }, pressedScale = 0.985f)
            .padding(start = 22.dp, end = 20.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        TimedRing(live, 32.dp)
        Column(Modifier.weight(1f)) {
            Text(habit.title, style = Not.type.body, color = if (summary.due) colors.ink else colors.inkMuted, maxLines = 1)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val unusual = live.tracked && live.on != rule.usualOn
                Text(
                    if (state.isToday) timedStatus(rule, live, now) else pastDayText(live),
                    // The streak keeps its place; the status gives way.
                    modifier = Modifier.weight(1f, fill = false),
                    style = Not.type.caption,
                    color = when {
                        live.atMostOff && live.leftMs < 0 -> colors.danger
                        unusual && state.isToday -> colors.accentInk
                        else -> colors.inkMuted
                    },
                    maxLines = 1,
                )
                if (summary.streak > 0u) Streak(summary.streak, habit.period)
            }
        }
        // Switching is about now: only on today.
        if (state.isToday) {
            SwitchPill(rule, live.on, live.tracked, onSwitch = {
                haptics.performHapticFeedback(if (live.on) HapticFeedbackType.ToggleOff else HapticFeedbackType.ToggleOn)
                viewModel.switchNow(habit.id, !live.on)
            }, onLongPress = { onEarlier(!live.on) })
        }
    }
}

/** A past day: "1h 20m off of 2h" / "35m of 30m ✓" / "Not tracked". */
private fun pastDayText(live: TimedLive): String = when {
    live.onMs + live.offMs == 0L -> "Not tracked"
    live.atMostOff -> "${durationText(live.countedMs)} off of ${durationText(live.goalMs)}${if (live.met) " ✓" else ""}"
    else -> "${durationText(live.countedMs)} of ${durationText(live.goalMs)}${if (live.met) " ✓" else ""}"
}

// ---- detail ----

/** Everything about a timed habit: today's ring and switch, the day's timeline, its rule, history. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TimedSheet(summary: HabitSummary, state: HabitsState, viewModel: HabitsViewModel, onDismiss: () -> Unit, onEarlier: (Boolean) -> Unit) {
    val habit = summary.habit
    val rule = habit.timed ?: return
    val colors = Not.colors
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as NotApp
    val alerts = app.timedAlerts
    // Live notifications and reminders need the permission; ask the moment one is turned on.
    val askNotify = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.RequestPermission()) { alerts.refresh() }
    fun ensureNotify() {
        if (!app.reminders.canNotify && android.os.Build.VERSION.SDK_INT >= 33) askNotify.launch(android.Manifest.permission.POST_NOTIFICATIONS)
    }
    var title by remember(habit.id) { mutableStateOf(habit.title) }
    var editing by remember { mutableStateOf<StretchEdit?>(null) }
    val now = rememberNow()
    val live = summary.timedLive(now, state.loadedAt, state.isToday) ?: return
    fun save(minutes: UInt = habit.goal, days: UByte = habit.days, newRule: TimedRule = rule) =
        viewModel.updateTimed(habit, title.ifBlank { habit.title }, minutes, days, newRule)

    Sheet(onDismiss = {
        if (title.isNotBlank() && title != habit.title) save()
        onDismiss()
    }) { close ->
        Column(
            Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 20.dp),
            verticalArrangement = Arrangement.spacedBy(22.dp),
        ) {
            Field(title, { title = it }, "Habit", style = Not.type.title, onDone = { save() }, modifier = Modifier.fillMaxWidth())

            // Today, big: the ring with the number that matters, where it stands, the switch.
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(28.dp)).background(colors.background).padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(18.dp)) {
                    TimedRing(live, 92.dp, center = ringCenter(live))
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(
                            when {
                                !state.isToday -> state.day.format(DateTimeFormatter.ofPattern("EEEE d"))
                                !live.tracked -> "Not started"
                                else -> "${stateName(rule, live.on)} since ${clockText(live.since!!)}"
                            },
                            style = Not.type.bodyStrong,
                        )
                        Text(ringCaption(live), style = Not.type.caption, color = if (live.atMostOff && live.leftMs < 0) colors.danger else colors.inkMuted)
                        if (state.isToday && live.tracked) {
                            Text(
                                "${durationText(live.onMs)} ${stateName(rule, true).lowercase()} · ${durationText(live.offMs)} ${stateName(rule, false).lowercase()} today",
                                style = Not.type.caption,
                                color = colors.inkFaint,
                            )
                        }
                    }
                }
                if (state.isToday) {
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                        SwitchPill(rule, live.on, live.tracked, onSwitch = { viewModel.switchNow(habit.id, !live.on) }, onLongPress = { onEarlier(!live.on) }, modifier = Modifier.weight(1f), large = true)
                        Button("Earlier…", { onEarlier(!live.on) }, kind = ButtonKind.Secondary)
                    }
                }
            }

            val segments = remember(summary, state.day, state.loadedAt) { viewModel.timeline(habit.id, state.day) }
            HabitSection(if (state.isToday) "Today" else state.day.format(DateTimeFormatter.ofPattern("EEEE d MMMM"))) {
                Timeline(segments, dayStart(state.day), if (state.isToday) now else null, rule)
                if (segments.isEmpty()) {
                    Text("Nothing tracked this day.", style = Not.type.caption, color = colors.inkMuted)
                }
                segments.forEachIndexed { i, seg ->
                    val open = state.isToday && i == segments.lastIndex
                    StretchRow(seg, rule, open, now) {
                        editing = StretchEdit(seg, segments.getOrNull(i - 1), segments.getOrNull(i + 1), open)
                    }
                }
                Button(
                    if (rule.usualOn) "Add a break" else "Add a session",
                    { editing = StretchEdit(null, null, null, false) },
                    kind = ButtonKind.Secondary,
                    icon = R.drawable.ic_add,
                )
            }

            HabitSection("Normally") {
                ChoicePills(listOf("On", "Off"), if (rule.usualOn) 0 else 1) { save(newRule = rule.copy(usualOn = it == 0)) }
                Text(
                    if (rule.usualOn) "Something worn or kept up: the button says Pause and Resume, and breaks are what's watched."
                    else "Something you do for a while: the button says Start and Stop, and sessions are what's watched.",
                    style = Not.type.caption,
                    color = colors.inkMuted,
                )
            }

            HabitSection("Goal") {
                ChoicePills(
                    listOf("At least ${stateName(rule, true).lowercase()}", "At most ${stateName(rule, false).lowercase()}"),
                    if (rule.atMostOff) 1 else 0,
                ) { save(newRule = rule.copy(atMostOff = it == 1)) }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (rule.atMostOff) "At most ${durationText(habit.goal.toLong() * MINUTE)} a day" else "At least ${durationText(habit.goal.toLong() * MINUTE)} a day",
                        style = Not.type.body,
                        modifier = Modifier.weight(1f),
                    )
                    Stepper(
                        onMinus = { save(minutes = stepGoal(habit.goal, -1)) },
                        onPlus = { save(minutes = stepGoal(habit.goal, +1)) },
                        minusEnabled = habit.goal > 5u,
                        plusEnabled = habit.goal < 24u * 60u,
                        minusLabel = "Less",
                        plusLabel = "More",
                    )
                }
            }

            HabitSection("Reminder") {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (rule.nudgeMinutes == 0u) "None"
                        else "After ${durationText(rule.nudgeMinutes.toLong() * MINUTE)} ${stateName(rule, !rule.usualOn).lowercase()}",
                        style = Not.type.body,
                        modifier = Modifier.weight(1f),
                    )
                    val i = NUDGES.indexOfFirst { it >= rule.nudgeMinutes }.coerceAtLeast(0)
                    Stepper(
                        onMinus = { save(newRule = rule.copy(nudgeMinutes = NUDGES[(i - 1).coerceAtLeast(0)])) },
                        onPlus = {
                            ensureNotify()
                            save(newRule = rule.copy(nudgeMinutes = NUDGES[(i + 1).coerceAtMost(NUDGES.lastIndex)]))
                        },
                        minusEnabled = i > 0,
                        plusEnabled = i < NUDGES.lastIndex,
                        minusLabel = "Sooner",
                        plusLabel = "Later",
                    )
                }
                var liveOn by remember(habit.id) { mutableStateOf(alerts.liveEnabled(habit.id)) }
                Row(
                    Modifier.fillMaxWidth().pressable({
                        liveOn = !liveOn
                        if (liveOn) ensureNotify()
                        alerts.setLive(habit.id, liveOn)
                    }, role = Role.Switch, pressedScale = 0.98f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("Show on the lock screen", style = Not.type.body)
                        Text(
                            "A live notification while ${stateName(rule, !rule.usualOn).lowercase()}, with a button to ${switchLabel(rule, !rule.usualOn).lowercase()}. This phone only.",
                            style = Not.type.caption,
                            color = colors.inkMuted,
                        )
                    }
                    RoundCheck(liveOn, {
                        liveOn = !liveOn
                        if (liveOn) ensureNotify()
                        alerts.setLive(habit.id, liveOn)
                    })
                }
                // Only worth a choice when the stretch counts toward the goal; otherwise just it moves.
                if (liveOn && rule.unusualCounts) {
                    var counter by remember(habit.id) { mutableStateOf(alerts.liveCounter(habit.id)) }
                    ChoicePills(LiveCounter.entries.map { it.label }, counter.ordinal) {
                        counter = LiveCounter.entries[it]
                        alerts.setLiveCounter(habit.id, counter)
                    }
                    val stretch = stateName(rule, !rule.usualOn).lowercase()
                    Text(
                        when (counter) {
                            LiveCounter.Stretch -> "Counts up from the moment it's $stretch."
                            LiveCounter.Today -> "Counts up all the time ${stateName(rule, !rule.atMostOff).lowercase()} today."
                            LiveCounter.Left -> if (rule.atMostOff) "Counts down the allowance left today, then the time over." else "Counts down to today's goal."
                        },
                        style = Not.type.caption,
                        color = colors.inkMuted,
                    )
                }
            }

            HabitSection("Repeat") { WeekdayPicker(habit.days, onChange = { save(days = it) }) }

            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Stat("Current streak", streakText(summary.streak, habit.period), Modifier.weight(1f))
                Stat("Best", streakText(summary.bestStreak, habit.period), Modifier.weight(1f))
            }

            HabitSection("Last 15 weeks") { TimedHistory(summary, state.today, viewModel, weeks = 15) }

            Button("Delete habit", {
                close()
                viewModel.delete(habit)
            }, kind = ButtonKind.Danger, icon = R.drawable.ic_delete)
        }
    }

    editing?.let { edit ->
        StretchSheet(edit, rule, state.day, state.isToday, onDismiss = { editing = null }) { on, start, end ->
            viewModel.editStretch(habit.id, edit.segment, edit.prev, edit.next, on, start, end)
            editing = null
        }
    }
}

/** Reminder delays to step through, in minutes; 0 is none. */
private val NUDGES = listOf(0u, 10u, 15u, 20u, 30u, 45u, 60u, 90u, 120u, 180u)

/** 5-minute steps under an hour, 15 above. */
private fun stepGoal(minutes: UInt, dir: Int): UInt {
    val step = if (minutes < 60u || (minutes == 60u && dir < 0)) 5 else 15
    val next = (minutes.toInt() + dir * step).coerceIn(5, 24 * 60)
    return (next - next % step).coerceAtLeast(5).toUInt()
}

private fun ringCenter(live: TimedLive): String = when {
    !live.tracked -> "–"
    live.atMostOff && live.leftMs < 0 -> "-${durationText(-live.leftMs)}"
    live.atMostOff -> durationText(live.leftMs)
    else -> durationText(live.countedMs)
}

private fun ringCaption(live: TimedLive): String = when {
    live.atMostOff && live.leftMs < 0 -> "${durationText(-live.leftMs)} over the ${durationText(live.goalMs)} allowance"
    live.atMostOff -> "left of ${durationText(live.goalMs)} off"
    live.met -> "Goal of ${durationText(live.goalMs)} met"
    else -> "of ${durationText(live.goalMs)} · ${durationText(live.leftMs)} to go"
}

/** The day as a bar from midnight to midnight: on in mint, off soft, untracked empty; a tick at now. */
@Composable
private fun Timeline(segments: List<TimeSegment>, dayStart: Long, now: Long?, rule: TimedRule) {
    val colors = Not.colors
    val day = 24 * HOUR
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Canvas(Modifier.fillMaxWidth().height(28.dp).clip(RoundedCornerShape(8.dp)).background(colors.sunken.copy(alpha = 0.5f))) {
            val w = size.width
            for (s in segments) {
                val x0 = w * ((s.start - dayStart).toFloat() / day)
                val x1 = w * ((s.end - dayStart).toFloat() / day)
                val usual = s.on == rule.usualOn
                drawRect(
                    if (s.on) colors.accent else if (usual) colors.inkFaint.copy(alpha = 0.35f) else Amber.copy(alpha = 0.75f),
                    topLeft = Offset(x0, 0f),
                    size = Size((x1 - x0).coerceAtLeast(1.5f), size.height),
                )
            }
            if (now != null) {
                val x = w * ((now - dayStart).toFloat() / day)
                drawRect(colors.ink, topLeft = Offset(x - 1f, 0f), size = Size(2f, size.height))
            }
        }
        Row(Modifier.fillMaxWidth()) {
            listOf("0", "6", "12", "18", "24").forEachIndexed { i, label ->
                Text(
                    label,
                    style = Not.type.tiny,
                    color = colors.inkFaint,
                    textAlign = when (i) {
                        0 -> TextAlign.Start
                        4 -> TextAlign.End
                        else -> TextAlign.Center
                    },
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

/** "07:00 – 12:00   On   5h"; the running one ends "now". Tap to fix it. */
@Composable
private fun StretchRow(seg: TimeSegment, rule: TimedRule, open: Boolean, now: Long, onClick: () -> Unit) {
    val colors = Not.colors
    val end = if (open) now else seg.end
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).pressable(onClick, pressedScale = 0.98f, onClickLabel = "Edit").padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.size(10.dp).clip(CircleShape).background(if (seg.on) colors.accent else if (seg.on == rule.usualOn) colors.inkFaint else Amber))
        Text("${clockText(seg.start)} – ${if (open) "now" else clockText(seg.end)}", style = Not.type.body, modifier = Modifier.weight(1f))
        Text(stateName(rule, seg.on), style = Not.type.caption, color = colors.inkMuted)
        Text(durationText(end - seg.start), style = Not.type.label, color = colors.ink, modifier = Modifier.width(64.dp), textAlign = TextAlign.End)
    }
}

/** Two or three pills, the chosen one filled in. */
@Composable
private fun ChoicePills(options: List<String>, selected: Int, onSelect: (Int) -> Unit) {
    val colors = Not.colors
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEachIndexed { i, label ->
            val on = i == selected
            val bg by animateColorAsState(if (on) colors.accent else colors.sunken, label = "pill")
            Box(
                Modifier.weight(1f).height(40.dp).clip(CircleShape).background(bg)
                    .pressable({ if (!on) onSelect(i) }, role = Role.RadioButton, pressedScale = 0.94f),
                contentAlignment = Alignment.Center,
            ) {
                Text(label, style = Not.type.label, color = if (on) colors.onAccent else colors.inkMuted, maxLines = 1)
            }
        }
    }
}

/** Weeks as columns, Monday on top: full when the day's goal was met, half when partly. */
@Composable
private fun TimedHistory(summary: HabitSummary, today: LocalDate, viewModel: HabitsViewModel, weeks: Int) {
    val habit = summary.habit
    val rule = habit.timed ?: return
    val start = today.minusDays((today.dayOfWeek.ordinal + 7L * (weeks - 1)))
    val history = remember(summary, today) { viewModel.history(habit.id, start) }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        repeat(weeks) { week ->
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                DayOfWeek.entries.forEach { weekday ->
                    val date = start.plusDays(week * 7L + weekday.ordinal)
                    if (date.isAfter(today)) {
                        Spacer(Modifier.size(14.dp))
                    } else {
                        val minutes = history[date]
                        // Tracked days only: untracked ones stay empty.
                        val (count, goal) = when {
                            minutes == null -> 0u to 1u
                            rule.atMostOff -> (if (minutes <= habit.goal) 2u else 1u) to 2u
                            else -> (if (minutes >= habit.goal) 2u else if (minutes > 0u) 1u else 0u) to 2u
                        }
                        DayDot(count, goal, scheduled = isScheduled(habit.days, weekday), size = 14.dp, ringed = date == today)
                    }
                }
            }
        }
    }
}

// ---- backdating and fixing stretches ----

/**
 * "Paused since…": now or a few minutes back, or a time of your choosing. Whatever was recorded
 * after that moment gives way.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SinceSheet(rule: TimedRule, on: Boolean, onPick: (Long) -> Unit, onDismiss: () -> Unit) {
    val now = remember { nowMs() }
    var custom by remember { mutableStateOf(now - 20 * MINUTE - (now % (5 * MINUTE))) }
    Sheet(onDismiss = onDismiss) { close ->
        Column(Modifier.padding(horizontal = 24.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            Text("${stateName(rule, on)} since…", style = Not.type.title)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("Just now" to 0L, "5 min ago" to 5L, "15 min ago" to 15L, "30 min ago" to 30L, "1 hour ago" to 60L).forEach { (label, back) ->
                    Button(label, { onPick(now - back * MINUTE); close() }, kind = ButtonKind.Secondary)
                }
            }
            Text("Or at", style = Not.type.label, color = Not.colors.inkMuted)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                TimeStepper(custom, { custom = it }, min = now - 24 * HOUR, max = now, modifier = Modifier.weight(1f))
                Button("Set", { onPick(custom); close() })
            }
        }
    }
}

/** A stretch being edited (or a new one when [segment] is null), with its neighbours. */
data class StretchEdit(val segment: TimeSegment?, val prev: TimeSegment?, val next: TimeSegment?, val open: Boolean)

/**
 * Fix one stretch of the day: on or off, when it started and ended. Ends move only as far as
 * the neighbours; the time they free goes to the neighbour on that side.
 */
@Composable
private fun StretchSheet(edit: StretchEdit, rule: TimedRule, day: LocalDate, isToday: Boolean, onDismiss: () -> Unit, onSave: (Boolean, Long, Long?) -> Unit) {
    val now = remember { nowMs() }
    val dayStart = dayStart(day)
    val dayEnd = if (isToday) now else dayStart(day.plusDays(1))
    val seg = edit.segment
    var on by remember { mutableStateOf(seg?.on ?: !rule.usualOn) }
    var start by remember { mutableStateOf(seg?.start ?: (dayEnd - 30 * MINUTE).coerceAtLeast(dayStart)) }
    var end by remember { mutableStateOf(if (edit.open) now else seg?.end ?: dayEnd) }
    var running by remember { mutableStateOf(edit.open) }
    // Without a neighbour there's nothing to give freed time to, so that end only grows outward.
    val startMin = edit.prev?.let { it.start + MINUTE } ?: dayStart
    val startMax = if (seg != null && edit.prev == null) seg.start else end - MINUTE
    val endMin = if (seg != null && edit.next == null && !edit.open) seg.end else start + MINUTE
    val endMax = edit.next?.let { it.end - MINUTE } ?: dayEnd

    Sheet(onDismiss = onDismiss) { close ->
        Column(Modifier.padding(horizontal = 24.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            Text(if (seg == null) (if (rule.usualOn) "Add a break" else "Add a session") else "Edit stretch", style = Not.type.title)
            ChoicePills(listOf(stateName(rule, true), stateName(rule, false)), if (on) 0 else 1) { on = it == 0 }
            HabitSection("From") { TimeStepper(start, { start = it }, min = startMin, max = startMax.coerceAtLeast(startMin)) }
            HabitSection("Until") {
                if (running) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Now — still going", style = Not.type.body, modifier = Modifier.weight(1f))
                        Button("Set an end", { running = false; end = now }, kind = ButtonKind.Quiet)
                    }
                } else {
                    TimeStepper(end, { end = it }, min = endMin.coerceAtMost(endMax), max = endMax)
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Button("Cancel", close, kind = ButtonKind.Secondary, modifier = Modifier.weight(1f))
                Button("Save", {
                    // Ending now (or later) leaves it running.
                    val until = if (running || (isToday && end >= now - MINUTE)) null else end
                    onSave(on, start, until)
                    close()
                }, modifier = Modifier.weight(1f))
            }
        }
    }
}

/** A clock time to nudge: −1h, −5, the time, +5, +1h; snaps to 5 minutes, kept within [min]..[max]. */
@Composable
fun TimeStepper(value: Long, onChange: (Long) -> Unit, min: Long, max: Long, modifier: Modifier = Modifier) {
    val colors = Not.colors
    fun step(delta: Long) {
        val snapped = if (delta % (5 * MINUTE) == 0L) {
            val base = value - Math.floorMod(value + utcOffsetMinutes(value) * MINUTE, 5 * MINUTE)
            if (delta > 0 || base == value) base + delta else base
        } else {
            value + delta
        }
        onChange(snapped.coerceIn(min, max))
    }
    Row(
        modifier.clip(CircleShape).background(colors.sunken).padding(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StepButton("−1h") { step(-HOUR) }
        StepButton("−5") { step(-5 * MINUTE) }
        Text(clockText(value), style = Not.type.heading, textAlign = TextAlign.Center, modifier = Modifier.weight(1f))
        StepButton("+5") { step(5 * MINUTE) }
        StepButton("+1h") { step(HOUR) }
    }
}

@Composable
private fun StepButton(label: String, onClick: () -> Unit) {
    Box(
        Modifier.size(44.dp).clip(CircleShape).pressable(onClick, pressedScale = 0.88f, onClickLabel = label),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, style = Not.type.label, color = Not.colors.ink)
    }
}
