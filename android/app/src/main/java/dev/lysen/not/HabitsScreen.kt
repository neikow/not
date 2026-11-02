package dev.lysen.not

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Habit
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.everyDay
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.time.format.TextStyle
import java.util.Locale

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HabitsScreen(viewModel: HabitsViewModel, navigationIcon: @Composable () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }

    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            snackbar.currentSnackbarData?.dismiss()
            val result = snackbar.showSnackbar(undoable.message, actionLabel = "Undo", duration = SnackbarDuration.Short)
            if (result == SnackbarResult.ActionPerformed) viewModel.undo()
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = navigationIcon,
                title = {
                    Column {
                        Text("Habits")
                        Text(
                            state.today.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.FULL)),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = { AddBar("New habit", onAdd = viewModel::add) },
    ) { padding ->
        if (state.habits.isEmpty()) {
            EmptyState(
                R.drawable.ic_repeat,
                "No habits yet",
                "Add one below, then tap its circle each day to build a streak",
                Modifier.padding(padding),
            )
        } else {
            LazyColumn(
                Modifier.fillMaxSize().padding(padding),
                contentPadding = PaddingValues(vertical = 8.dp),
            ) {
                items(state.habits, key = { it.habit.id }) { summary ->
                    HabitRow(summary, state.today, viewModel, Modifier.animateItem())
                }
            }
        }
    }

    state.openHabit?.let { HabitSheet(it, state.today, viewModel, onDismiss = { viewModel.open(null) }) }
    state.error?.let {
        AlertDialog(
            onDismissRequest = viewModel::dismissError,
            confirmButton = { TextButton(onClick = viewModel::dismissError) { Text("OK") } },
            title = { Text("Something went wrong") },
            text = { Text(it) },
        )
    }
}

@Composable
private fun HabitRow(summary: HabitSummary, today: LocalDate, viewModel: HabitsViewModel, modifier: Modifier) {
    val haptics = LocalHapticFeedback.current
    val habit = summary.habit
    Row(
        modifier
            .fillMaxWidth()
            .clickable { viewModel.open(habit.id) }
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
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
            Text(
                habit.title,
                style = MaterialTheme.typography.bodyLarge,
                color = if (summary.due) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    scheduleText(habit),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (summary.streak > 0u) Streak(summary.streak)
            }
        }
        WeekStrip(summary, today)
    }
}

@Composable
private fun Streak(days: UInt) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(painterResource(R.drawable.ic_flame), null, tint = StreakColor, modifier = Modifier.size(14.dp))
        Text(days.toString(), style = MaterialTheme.typography.labelMedium, color = StreakColor)
    }
}

private val StreakColor = Color(0xFFF57C00)

/** Today's progress: a ring that fills with each check-in and turns solid when the goal is met. */
@Composable
fun CheckRing(count: UInt, goal: UInt, due: Boolean, onClick: () -> Unit, size: Dp = 40.dp) {
    val colors = MaterialTheme.colorScheme
    val progress by animateFloatAsState((count.toFloat() / goal.coerceAtLeast(1u).toFloat()).coerceAtMost(1f), label = "ring")
    val done = count >= goal
    Box(
        Modifier
            .size(size)
            .background(if (done) colors.primary else Color.Transparent, CircleShape)
            .clickable(onClick = onClick, onClickLabel = if (done) "Reset" else "Check in"),
        contentAlignment = Alignment.Center,
    ) {
        if (done) {
            Icon(painterResource(R.drawable.ic_check), null, tint = colors.onPrimary)
        } else {
            Canvas(Modifier.fillMaxSize().padding(2.dp)) {
                val stroke = 3.dp.toPx()
                drawCircle(colors.outlineVariant.copy(alpha = if (due) 1f else 0.5f), style = Stroke(1.5.dp.toPx()))
                drawArc(
                    colors.primary,
                    startAngle = -90f,
                    sweepAngle = 360f * progress,
                    useCenter = false,
                    style = Stroke(stroke, cap = StrokeCap.Round),
                )
            }
            if (count > 0u) {
                Text(
                    count.toString(),
                    color = colors.primary,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = (size.value * 0.36f).sp,
                )
            }
        }
    }
}

/** The last 7 days, oldest first; today is outlined. */
@Composable
private fun WeekStrip(summary: HabitSummary, today: LocalDate) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        summary.week.forEachIndexed { index, count ->
            val day = today.minusDays((6 - index).toLong())
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                DayDot(
                    count,
                    summary.habit.goal,
                    scheduled = isScheduled(summary.habit.days, day.dayOfWeek),
                    size = 10.dp,
                    outlined = index == 6,
                )
                Text(
                    day.dayOfWeek.getDisplayName(TextStyle.NARROW, Locale.getDefault()),
                    fontSize = 9.sp,
                    color = MaterialTheme.colorScheme.outline,
                )
            }
        }
    }
}

@Composable
private fun DayDot(count: UInt, goal: UInt, scheduled: Boolean, size: Dp, outlined: Boolean = false) {
    val colors = MaterialTheme.colorScheme
    val fill = when {
        count >= goal -> colors.primary
        count > 0u -> colors.primary.copy(alpha = 0.4f)
        scheduled -> colors.surfaceContainerHighest
        else -> colors.surfaceContainer
    }
    Box(
        Modifier
            .size(size)
            .then(if (outlined) Modifier.border(1.dp, colors.outline, CircleShape) else Modifier)
            .background(fill, CircleShape),
    )
}

// ---- detail ----

/** Everything about one habit: today's count, schedule, streaks, history, delete. Saves as you go. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HabitSheet(summary: HabitSummary, today: LocalDate, viewModel: HabitsViewModel, onDismiss: () -> Unit) {
    val habit = summary.habit
    var title by remember(habit.id) { mutableStateOf(habit.title) }
    fun save(goal: UInt = habit.goal, days: UByte = habit.days) =
        viewModel.update(Habit(habit.id, title.ifBlank { habit.title }, goal, days))

    ModalBottomSheet(
        onDismissRequest = {
            if (title.isNotBlank() && title != habit.title) save()
            onDismiss()
        },
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(
            Modifier
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                .padding(bottom = 24.dp)
                .imePadding(),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            TextField(
                title,
                { title = it },
                textStyle = MaterialTheme.typography.titleLarge,
                singleLine = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { save() }),
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = Color.Transparent,
                    unfocusedContainerColor = Color.Transparent,
                ),
                modifier = Modifier.fillMaxWidth(),
            )

            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                CheckRing(summary.count, habit.goal, summary.due, onClick = { viewModel.toggle(habit.id) }, size = 56.dp)
                Column(Modifier.weight(1f)) {
                    Text("${summary.count} of ${habit.goal}", style = MaterialTheme.typography.titleMedium)
                    Text(
                        if (summary.due) "Due today" else "Not scheduled today",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                FilledTonalIconButton(onClick = { viewModel.log(habit.id, -1) }, enabled = summary.count > 0u) {
                    Icon(painterResource(R.drawable.ic_remove), "Remove a check-in")
                }
                FilledTonalIconButton(onClick = { viewModel.log(habit.id, 1) }) {
                    Icon(painterResource(R.drawable.ic_add), "Add a check-in")
                }
            }

            HorizontalDivider()
            SheetLabel("Goal")
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                FilledTonalIconButton(onClick = { save(goal = habit.goal - 1u) }, enabled = habit.goal > 1u) {
                    Icon(painterResource(R.drawable.ic_remove), "Fewer")
                }
                Text(
                    if (habit.goal == 1u) "Once a day" else "${habit.goal} times a day",
                    style = MaterialTheme.typography.bodyLarge,
                    modifier = Modifier.weight(1f),
                )
                FilledTonalIconButton(onClick = { save(goal = habit.goal + 1u) }, enabled = habit.goal < 99u) {
                    Icon(painterResource(R.drawable.ic_add), "More")
                }
            }

            SheetLabel("Repeat")
            WeekdayChips(habit.days, onChange = { save(days = it) })

            HorizontalDivider()
            Row(horizontalArrangement = Arrangement.spacedBy(32.dp)) {
                Stat("Current streak", days(summary.streak))
                Stat("Best", days(summary.bestStreak))
            }

            SheetLabel("Last 15 weeks")
            HistoryGrid(summary, today, viewModel, weeks = 15)

            TextButton(onClick = { viewModel.delete(habit) }) {
                Icon(painterResource(R.drawable.ic_delete), null, tint = MaterialTheme.colorScheme.error)
                Spacer(Modifier.width(8.dp))
                Text("Delete habit", color = MaterialTheme.colorScheme.error)
            }
        }
    }
}

@Composable
private fun SheetLabel(text: String) {
    Text(text, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
}

@Composable
private fun Stat(label: String, value: String) {
    Column {
        Text(value, style = MaterialTheme.typography.titleLarge)
        Text(label, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

private fun days(n: UInt) = if (n == 1u) "1 day" else "$n days"

/** Seven chips, Monday first. At least one day stays selected. */
@Composable
private fun WeekdayChips(days: UByte, onChange: (UByte) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        DayOfWeek.entries.forEach { day ->
            val bit = (1 shl day.ordinal)
            val on = isScheduled(days, day)
            FilterChip(
                selected = on,
                onClick = {
                    val next = (days.toInt() xor bit).toUByte()
                    if (next.toInt() != 0) onChange(next)
                },
                label = {
                    Text(
                        day.getDisplayName(TextStyle.NARROW, Locale.getDefault()),
                        modifier = Modifier.fillMaxWidth(),
                        textAlign = TextAlign.Center,
                    )
                },
                modifier = Modifier.weight(1f),
            )
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
                        DayDot(
                            history[date] ?: 0u,
                            summary.habit.goal,
                            scheduled = isScheduled(summary.habit.days, weekday),
                            size = 14.dp,
                        )
                    }
                }
            }
        }
    }
}

// ---- shared ----

@Composable
fun AddBar(placeholder: String, onAdd: (String) -> Unit) {
    var draft by remember { mutableStateOf("") }
    Surface(tonalElevation = 3.dp) {
        OutlinedTextField(
            value = draft,
            onValueChange = { draft = it },
            placeholder = { Text(placeholder) },
            singleLine = true,
            shape = MaterialTheme.shapes.extraLarge,
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.Sentences,
                imeAction = ImeAction.Done,
            ),
            // Keeps the keyboard up for the next one.
            keyboardActions = KeyboardActions(onDone = {
                onAdd(draft)
                draft = ""
            }),
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .imePadding()
                .padding(12.dp),
        )
    }
}

@Composable
fun EmptyState(icon: Int, title: String, hint: String, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(painterResource(icon), null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(40.dp))
            Spacer(Modifier.height(4.dp))
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(
                hint,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
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
