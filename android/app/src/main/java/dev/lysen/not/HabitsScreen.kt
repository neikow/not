package dev.lysen.not

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
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
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Habit
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.everyDay
import dev.lysen.not.ui.AddAction
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import androidx.compose.foundation.lazy.rememberLazyListState
import dev.lysen.not.ui.rememberReorderState
import dev.lysen.not.ui.reorderable
import dev.lysen.not.ui.Composer
import dev.lysen.not.ui.EmptyState
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.IconButton
import dev.lysen.not.ui.LargeTitleScreen
import dev.lysen.not.ui.LocalBottomBarHeight
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.ToastHost
import dev.lysen.not.ui.ToastState
import dev.lysen.not.ui.pressable
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun HabitsScreen(viewModel: HabitsViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toasts = remember { ToastState() }
    var composing by rememberSaveable { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            if (toasts.show(undoable.message, action = "Undo")) viewModel.undo()
        }
    }
    AddAction("New habit", enabled = !composing) { composing = true }

    val listState = rememberLazyListState()
    val reorder = rememberReorderState(listState, state.habits) { it.habit.id }

    Box(Modifier.fillMaxSize()) {
        LargeTitleScreen(title = "Habits", state = listState, header = { TodayHeader(state) }) {
            if (state.habits.isEmpty()) {
                item(key = "empty") {
                    EmptyState(R.drawable.ic_repeat, "No habits yet", "Tap + to add one, then check it off each day to build a streak.")
                }
            }
            items(reorder.items, key = { it.habit.id }) { summary ->
                HabitRow(
                    summary,
                    state.today,
                    viewModel,
                    (if (reorder.isDragging(summary)) Modifier else Modifier.animateItem())
                        .reorderable(reorder, summary, summary.habit.id) {
                            reorder.start(summary, canSwap = { true }, onDrop = { order ->
                                viewModel.move(summary.habit.id, order.indexOfFirst { it.habit.id == summary.habit.id })
                            })
                        },
                )
            }
        }
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .imePadding()
                .padding(bottom = if (WindowInsets.isImeVisible) 8.dp else LocalBottomBarHeight.current + 8.dp),
        ) {
            ToastHost(toasts, Modifier.align(Alignment.CenterHorizontally).padding(bottom = 8.dp))
            if (composing) Composer("New habit", onAdd = viewModel::add, onClose = { composing = false })
        }
    }

    state.openHabit?.let { HabitSheet(it, state.today, viewModel, onDismiss = { viewModel.open(null) }) }
    state.error?.let {
        CardDialog(
            title = "Something went wrong",
            onDismiss = viewModel::dismissError,
            buttons = { Button("OK", viewModel::dismissError) },
        ) { Text(it, color = Not.colors.inkMuted) }
    }
}

/** "Monday, November 2" and "1 of 3 done"; keeps its height when empty so nothing jumps. */
@Composable
private fun TodayHeader(state: HabitsState) {
    val due = state.habits.filter { it.due }
    val done = due.count { it.count >= it.habit.goal }
    val progress = when {
        due.isEmpty() -> "Nothing due today"
        done == due.size -> "All done for today"
        else -> "$done of ${due.size} done"
    }
    Row(Modifier.padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(state.today.format(DateTimeFormatter.ofPattern("EEEE, MMMM d")), style = Not.type.body, color = Not.colors.inkMuted)
        Box(Modifier.size(3.dp).clip(CircleShape).background(Not.colors.inkFaint))
        Text(progress, style = Not.type.body, color = if (due.isNotEmpty() && done == due.size) Not.colors.accentInk else Not.colors.inkMuted)
    }
}

@Composable
private fun HabitRow(summary: HabitSummary, today: LocalDate, viewModel: HabitsViewModel, modifier: Modifier) {
    val haptics = LocalHapticFeedback.current
    val habit = summary.habit
    val colors = Not.colors
    Row(
        modifier
            .fillMaxWidth()
            .background(colors.background)
            .pressable({ viewModel.open(habit.id) }, pressedScale = 0.985f)
            .padding(start = 16.dp, end = 24.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CheckRing(
            summary.count,
            habit.goal,
            summary.due,
            onClick = {
                val completes = summary.count + 1u >= habit.goal && summary.count < habit.goal
                haptics.performHapticFeedback(if (completes) HapticFeedbackType.Confirm else HapticFeedbackType.ToggleOn)
                viewModel.toggle(habit.id)
            },
        )
        Column(Modifier.weight(1f)) {
            Text(habit.title, style = Not.type.body, color = if (summary.due) colors.ink else colors.inkMuted, maxLines = 1)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(scheduleText(habit), style = Not.type.caption, color = colors.inkMuted, maxLines = 1)
                if (summary.streak > 0u) Streak(summary.streak)
            }
        }
        WeekStrip(summary, today)
    }
}

@Composable
private fun Streak(days: UInt) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        Icon(R.drawable.ic_flame, null, tint = StreakColor, size = 13.dp)
        Text(days.toString(), style = Not.type.caption, color = StreakColor)
    }
}

private val StreakColor = Color(0xFFF08A24)

/**
 * Today's progress: a ring that fills with each check-in and becomes a mint disc with a check once
 * the goal is met. 44dp touch target around [size].
 */
@Composable
fun CheckRing(count: UInt, goal: UInt, due: Boolean, onClick: () -> Unit, size: Dp = 32.dp) {
    val colors = Not.colors
    val progress by animateFloatAsState((count.toFloat() / goal.coerceAtLeast(1u).toFloat()).coerceAtMost(1f), label = "ring")
    val done = count >= goal
    val fill by animateColorAsState(if (done) colors.accent else Color.Transparent, label = "fill")
    val tick by animateFloatAsState(if (done) 1f else 0f, spring(Spring.DampingRatioMediumBouncy, Spring.StiffnessMedium), label = "tick")
    Box(
        Modifier
            .size(maxOf(size + 12.dp, 44.dp))
            .clip(CircleShape)
            .toggleable(done, remember { MutableInteractionSource() }, indication = null, role = Role.Checkbox) { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(size).background(fill, CircleShape), contentAlignment = Alignment.Center) {
            if (!done) {
                Canvas(Modifier.fillMaxSize()) {
                    val stroke = 3.dp.toPx()
                    drawCircle(colors.inkFaint.copy(alpha = if (due) 0.7f else 0.35f), radius = (this.size.minDimension - stroke) / 2, style = Stroke(1.75.dp.toPx()))
                    drawArc(
                        colors.accent,
                        startAngle = -90f,
                        sweepAngle = 360f * progress,
                        useCenter = false,
                        topLeft = androidx.compose.ui.geometry.Offset(stroke / 2, stroke / 2),
                        size = androidx.compose.ui.geometry.Size(this.size.width - stroke, this.size.height - stroke),
                        style = Stroke(stroke, cap = StrokeCap.Round),
                    )
                }
                if (count > 0u) Text(count.toString(), style = Not.type.label.merge(fontSize = (size.value * 0.4f).sp), color = colors.accentInk)
            }
            if (tick > 0.01f) Icon(R.drawable.ic_check, if (done) "Done" else null, tint = colors.onAccent, size = size * 0.6f, modifier = Modifier.scale(tick))
        }
    }
}

/** The last 7 days, oldest first; today is ringed. */
@Composable
private fun WeekStrip(summary: HabitSummary, today: LocalDate) {
    Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        summary.week.forEachIndexed { index, count ->
            val day = today.minusDays((6 - index).toLong())
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(3.dp)) {
                DayDot(count, summary.habit.goal, scheduled = isScheduled(summary.habit.days, day.dayOfWeek), size = 10.dp, ringed = index == 6)
                Text(
                    day.dayOfWeek.getDisplayName(TextStyle.NARROW, Locale.getDefault()),
                    style = Not.type.tiny.merge(fontSize = 9.sp, lineHeight = 10.sp),
                    color = if (index == 6) Not.colors.inkMuted else Not.colors.inkFaint,
                )
            }
        }
    }
}

@Composable
private fun DayDot(count: UInt, goal: UInt, scheduled: Boolean, size: Dp, ringed: Boolean = false) {
    val colors = Not.colors
    val fill = when {
        count >= goal -> colors.accent
        count > 0u -> colors.accent.copy(alpha = 0.4f)
        scheduled -> colors.sunken
        else -> colors.sunken.copy(alpha = 0.4f)
    }
    Box(
        Modifier
            .size(size)
            .then(if (ringed) Modifier.border(1.25.dp, colors.inkMuted, CircleShape) else Modifier)
            .background(fill, CircleShape),
    )
}

// ---- detail ----

/** Everything about one habit: today's count, schedule, streaks, history, delete. Saves as you go. */
@Composable
private fun HabitSheet(summary: HabitSummary, today: LocalDate, viewModel: HabitsViewModel, onDismiss: () -> Unit) {
    val habit = summary.habit
    val colors = Not.colors
    var title by remember(habit.id) { mutableStateOf(habit.title) }
    fun save(goal: UInt = habit.goal, days: UByte = habit.days) =
        viewModel.update(Habit(habit.id, title.ifBlank { habit.title }, goal, days))

    Sheet(onDismiss = {
        if (title.isNotBlank() && title != habit.title) save()
        onDismiss()
    }) { close ->
        Column(
            Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 20.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Field(title, { title = it }, "Habit", style = Not.type.title, onDone = { save() }, modifier = Modifier.fillMaxWidth())

            // Today, in a soft card: the ring and a stepper for extra or mistaken check-ins.
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(colors.background).padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CheckRing(summary.count, habit.goal, summary.due, onClick = { viewModel.toggle(habit.id) }, size = 48.dp)
                Column(Modifier.weight(1f)) {
                    Text("${summary.count} of ${habit.goal} today", style = Not.type.bodyStrong)
                    Text(if (summary.due) "Due today" else "Not scheduled today", style = Not.type.caption, color = colors.inkMuted)
                }
                Stepper(
                    onMinus = { viewModel.log(habit.id, -1) },
                    onPlus = { viewModel.log(habit.id, 1) },
                    minusEnabled = summary.count > 0u,
                    minusLabel = "Remove a check-in",
                    plusLabel = "Add a check-in",
                )
            }

            Section("Goal") {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(if (habit.goal == 1u) "Once a day" else "${habit.goal} times a day", style = Not.type.body, modifier = Modifier.weight(1f))
                    Stepper(
                        onMinus = { save(goal = habit.goal - 1u) },
                        onPlus = { save(goal = habit.goal + 1u) },
                        minusEnabled = habit.goal > 1u,
                        plusEnabled = habit.goal < 99u,
                        minusLabel = "Fewer",
                        plusLabel = "More",
                    )
                }
            }

            Section("Repeat") { WeekdayPicker(habit.days, onChange = { save(days = it) }) }

            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Stat("Current streak", days(summary.streak), Modifier.weight(1f))
                Stat("Best", days(summary.bestStreak), Modifier.weight(1f))
            }

            Section("Last 15 weeks") { HistoryGrid(summary, today, viewModel, weeks = 15) }

            Button("Delete habit", {
                close()
                viewModel.delete(habit)
            }, kind = ButtonKind.Danger, icon = R.drawable.ic_delete)
        }
    }
}

@Composable
private fun Section(label: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(label, style = Not.type.label, color = Not.colors.inkMuted)
        content()
    }
}

@Composable
private fun Stepper(
    onMinus: () -> Unit,
    onPlus: () -> Unit,
    minusLabel: String,
    plusLabel: String,
    minusEnabled: Boolean = true,
    plusEnabled: Boolean = true,
) {
    val colors = Not.colors
    Row(Modifier.clip(CircleShape).background(colors.sunken), verticalAlignment = Alignment.CenterVertically) {
        IconButton(R.drawable.ic_remove, minusLabel, { if (minusEnabled) onMinus() }, tint = if (minusEnabled) colors.ink else colors.inkFaint)
        Box(Modifier.size(width = 1.dp, height = 20.dp).background(colors.line))
        IconButton(R.drawable.ic_add, plusLabel, { if (plusEnabled) onPlus() }, tint = if (plusEnabled) colors.ink else colors.inkFaint)
    }
}

@Composable
private fun Stat(label: String, value: String, modifier: Modifier = Modifier) {
    Column(modifier.clip(RoundedCornerShape(20.dp)).background(Not.colors.background).padding(16.dp)) {
        Text(value, style = Not.type.title)
        Text(label, style = Not.type.caption, color = Not.colors.inkMuted)
    }
}

private fun days(n: UInt) = if (n == 1u) "1 day" else "$n days"

/** Seven round toggles, Monday first. At least one day stays selected. */
@Composable
private fun WeekdayPicker(days: UByte, onChange: (UByte) -> Unit) {
    val colors = Not.colors
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        DayOfWeek.entries.forEach { day ->
            val bit = 1 shl day.ordinal
            val on = isScheduled(days, day)
            val bg by animateColorAsState(if (on) colors.accent else colors.sunken, label = "day")
            Box(
                Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .background(bg)
                    .pressable({
                        val next = (days.toInt() xor bit).toUByte()
                        if (next.toInt() != 0) onChange(next)
                    }, role = Role.Checkbox, pressedScale = 0.88f, onClickLabel = day.getDisplayName(TextStyle.FULL, Locale.getDefault())),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    day.getDisplayName(TextStyle.NARROW, Locale.getDefault()),
                    style = Not.type.label,
                    color = if (on) colors.onAccent else colors.inkMuted,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/** Weeks as columns (oldest left), Monday on top. */
@Composable
private fun HistoryGrid(summary: HabitSummary, today: LocalDate, viewModel: HabitsViewModel, weeks: Int) {
    val start = today.minusDays((today.dayOfWeek.ordinal + 7L * (weeks - 1)))
    // Keyed on today's count: past days only change through sync, which changes the summary too.
    val history = remember(summary, today) { viewModel.history(summary.habit.id, start) }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        repeat(weeks) { week ->
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                DayOfWeek.entries.forEach { weekday ->
                    val date = start.plusDays(week * 7L + weekday.ordinal)
                    if (date.isAfter(today)) {
                        Spacer(Modifier.size(14.dp))
                    } else {
                        DayDot(history[date] ?: 0u, summary.habit.goal, scheduled = isScheduled(summary.habit.days, weekday), size = 14.dp, ringed = date == today)
                    }
                }
            }
        }
    }
}

private fun isScheduled(days: UByte, day: DayOfWeek) = days.toInt() and (1 shl day.ordinal) != 0

private fun scheduleText(habit: Habit): String {
    val days = when (habit.days.toInt()) {
        everyDay().toInt() -> "Every day"
        0b001_1111 -> "Weekdays"
        0b110_0000 -> "Weekends"
        else -> DayOfWeek.entries.filter { isScheduled(habit.days, it) }
            .joinToString { it.getDisplayName(TextStyle.SHORT, Locale.getDefault()) }
    }
    return if (habit.goal > 1u) "$days · ${habit.goal}×" else days
}
