package dev.lysen.not

import dev.lysen.not.ui.OnTabGesture
import dev.lysen.not.ui.TabGesture
import android.Manifest
import android.app.DatePickerDialog
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.shrinkVertically
import androidx.compose.ui.draw.rotate
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.CatalogExercise
import dev.lysen.not.core.SetLine
import dev.lysen.not.core.Workout
import dev.lysen.not.core.WorkoutExercise
import dev.lysen.not.core.WorkoutSummary
import dev.lysen.not.core.parseSets
import dev.lysen.not.ui.AddAction
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Divider
import dev.lysen.not.ui.EmptyState
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.IconButton
import dev.lysen.not.ui.LargeTitleScreen
import dev.lysen.not.ui.LocalBottomBarHeight
import dev.lysen.not.ui.Menu
import dev.lysen.not.ui.MenuItem
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.RoundCheck
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.ToastHost
import dev.lysen.not.ui.ToastState
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.delay
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit

/** Sessions, latest first; opening one shows its exercises. */
@Composable
fun WorkoutsScreen(viewModel: WorkoutsViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toasts = remember { ToastState() }
    var starting by remember { mutableStateOf(false) }
    var importing by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        viewModel.undoables.collect { undoable ->
            if (toasts.show(undoable.message, action = "Undo")) viewModel.undo()
        }
    }

    val open = state.open
    // Tapping Workouts again goes back into today's session, if there is one; holding it offers
    // the ways to start one.
    OnTabGesture { gesture ->
        val todays = state.workouts.firstOrNull { it.date == state.today.toString() }
        when {
            gesture == TabGesture.Reselect && todays != null -> viewModel.open(todays.id)
            // With nothing to repeat, the sheet would only offer an empty workout; a stray
            // re-tap shouldn't make one, a deliberate hold can.
            state.workouts.none { it.exerciseNames.isNotEmpty() } -> if (gesture == TabGesture.Hold) viewModel.newWorkout()
            else -> starting = true
        }
    }
    Box(Modifier.fillMaxSize()) {
        if (open != null) {
            BackHandler { viewModel.open(null) }
            WorkoutEditor(open, state, viewModel)
        } else {
            val last = state.workouts.firstOrNull { it.exerciseNames.isNotEmpty() }
            AddAction(
                "New workout",
                holdLabel = if (last != null) "Repeat last workout" else null,
                onHold = last?.let { { viewModel.repeat(it.id) } },
            ) { if (last != null) starting = true else viewModel.newWorkout() }
            WorkoutList(state, viewModel, onImport = { importing = true })
        }
        ToastHost(
            toasts,
            Modifier.align(Alignment.BottomCenter).imePadding().padding(bottom = LocalBottomBarHeight.current + 8.dp + if (open != null) 72.dp else 0.dp),
        )
    }

    if (starting) StartSheet(state, viewModel, onDismiss = { starting = false })
    if (importing) ImportSheet(viewModel, onDismiss = { importing = false })
    state.error?.let {
        CardDialog(
            title = "Something went wrong",
            onDismiss = viewModel::dismissError,
            buttons = { Button("OK", viewModel::dismissError) },
        ) { Text(it, color = Not.colors.inkMuted) }
    }
}

// ---- list ----------------------------------------------------------------------------------------

@Composable
private fun WorkoutList(state: WorkoutsState, viewModel: WorkoutsViewModel, onImport: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    LargeTitleScreen(
        title = "Workouts",
        actions = {
            Box {
                IconButton(R.drawable.ic_more_vert, "More", { menu = true })
                Menu(menu, onDismiss = { menu = false }) {
                    MenuItem("Import from text…", icon = R.drawable.ic_note, onClick = {
                        menu = false
                        onImport()
                    })
                }
            }
        },
        header = { WeekHeader(state) },
    ) {
        if (state.workouts.isEmpty()) {
            item(key = "empty") {
                EmptyState(R.drawable.ic_fitness, "No workouts yet", "Tap + when you start. Logged them in another app? Import from the menu.")
            }
        }
        items(state.workouts, key = { it.id }) { w ->
            WorkoutCard(w, state.today, viewModel, Modifier.animateItem().padding(horizontal = 16.dp, vertical = 5.dp))
        }
    }
}

/** "2 this week · 9 this month"; keeps its line when empty so nothing jumps. */
@Composable
private fun WeekHeader(state: WorkoutsState) {
    val monday = state.today.minusDays(state.today.dayOfWeek.value - 1L)
    val dates = state.workouts.filter { it.exerciseNames.isNotEmpty() }.mapNotNull { runCatching { LocalDate.parse(it.date) }.getOrNull() }
    val week = dates.count { !it.isBefore(monday) }
    val month = dates.count { it.year == state.today.year && it.month == state.today.month }
    Column(Modifier.padding(top = 2.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(if (dates.isEmpty()) " " else "$week this week · $month this month", style = Not.type.body, color = Not.colors.inkMuted)
        if (state.weekMuscles.isNotEmpty()) {
            Text(
                "Sets: " + state.weekMuscles.take(6).joinToString(" · ") { "${it.muscle} ${it.sets}" },
                style = Not.type.caption,
                color = Not.colors.inkFaint,
            )
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun WorkoutCard(w: WorkoutSummary, today: LocalDate, viewModel: WorkoutsViewModel, modifier: Modifier) {
    val colors = Not.colors
    val haptics = LocalHapticFeedback.current
    var menu by remember { mutableStateOf(false) }
    Box(modifier) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(22.dp))
                .background(colors.surface)
                .combinedClickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onLongClick = {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        menu = true
                    },
                    onClick = { viewModel.open(w.id) },
                )
                .padding(horizontal = 18.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(w.title.ifEmpty { dayLabel(w.date, today) }, style = Not.type.heading, maxLines = 1, modifier = Modifier.weight(1f))
                if (w.setCount > 0u) Text("${w.setCount} sets", style = Not.type.caption, color = colors.inkFaint)
            }
            Text(
                w.exerciseNames.joinToString(" · ").ifEmpty { "No exercises yet" },
                style = Not.type.caption.merge(fontSize = Not.type.caption.fontSize.times(1.1f), lineHeight = Not.type.caption.lineHeight.times(1.1f)),
                color = colors.inkMuted,
                maxLines = 2,
            )
            if (w.title.isNotEmpty()) Text(dayLabel(w.date, today), style = Not.type.caption, color = colors.inkFaint, modifier = Modifier.padding(top = 2.dp))
        }
        Menu(menu, onDismiss = { menu = false }) {
            if (w.exerciseNames.isNotEmpty()) {
                MenuItem("Repeat today", icon = R.drawable.ic_repeat, onClick = {
                    menu = false
                    viewModel.repeat(w.id)
                })
            }
            MenuItem("Delete", icon = R.drawable.ic_delete, danger = true, onClick = {
                menu = false
                viewModel.delete(w)
            })
        }
    }
}

/**
 * What + opens: start empty, repeat a named workout (they're the routines, so they lead), or one of
 * the recent unnamed sessions, folded away until asked for.
 */
@Composable
private fun StartSheet(state: WorkoutsState, viewModel: WorkoutsViewModel, onDismiss: () -> Unit) {
    val past = state.workouts.filter { it.exerciseNames.isNotEmpty() }
    // The latest of each name, then of each unnamed set of exercises; both latest first.
    val named = past.filter { it.title.isNotBlank() }.distinctBy { it.title.trim().lowercase() }
    val recent = past.filter { it.title.isBlank() }.distinctBy { it.exerciseNames }.take(8)
    // With no routines yet, there's nothing to fold the recent ones under.
    var showsRecent by remember { mutableStateOf(named.isEmpty()) }
    val chevron by animateFloatAsState(if (showsRecent) 180f else 0f, label = "chevron")
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
            Text("Start a workout", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 12.dp))
            SheetRow(R.drawable.ic_add, "Empty workout", accent = true) {
                viewModel.newWorkout()
                close()
            }
            named.forEach { w ->
                SheetRow(R.drawable.ic_repeat, w.title.trim(), detail = dayLabel(w.date, state.today)) {
                    viewModel.repeat(w.id)
                    close()
                }
            }
            if (recent.isNotEmpty()) {
                if (named.isNotEmpty()) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clip(CircleShape)
                            .pressable({ showsRecent = !showsRecent }, pressedScale = 0.98f)
                            .heightIn(min = 44.dp)
                            .padding(horizontal = 14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text("Recent", style = Not.type.label, color = Not.colors.inkMuted)
                        Text(recent.size.toString(), style = Not.type.label, color = Not.colors.inkFaint)
                        Icon(
                            R.drawable.ic_expand_more,
                            if (showsRecent) "Hide recent" else "Show recent",
                            tint = Not.colors.inkFaint,
                            size = 20.dp,
                            modifier = Modifier.rotate(chevron),
                        )
                    }
                }
                AnimatedVisibility(showsRecent, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
                    Column {
                        recent.forEach { w ->
                            // "+1" keeps sessions that differ past the third exercise apart.
                            val more = w.exerciseNames.size - 3
                            val label = w.exerciseNames.take(3).joinToString(", ") + if (more > 0) " +$more" else ""
                            SheetRow(R.drawable.ic_history, label, detail = dayLabel(w.date, state.today)) {
                                viewModel.repeat(w.id)
                                close()
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SheetRow(icon: Int, title: String, detail: String? = null, accent: Boolean = false, onClick: () -> Unit) {
    val colors = Not.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(CircleShape)
            .pressable(onClick, pressedScale = 0.98f)
            .heightIn(min = 52.dp)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = if (accent) colors.accentInk else colors.inkMuted)
        Spacer(Modifier.width(14.dp))
        Text(title, style = Not.type.body, color = if (accent) colors.accentInk else colors.ink, maxLines = 1, modifier = Modifier.weight(1f))
        if (detail != null) Text(detail, style = Not.type.caption, color = colors.inkFaint, modifier = Modifier.padding(start = 8.dp))
    }
}

@Composable
private fun ImportSheet(viewModel: WorkoutsViewModel, onDismiss: () -> Unit) {
    var text by remember { mutableStateOf("") }
    var nothing by remember { mutableStateOf(false) }
    Sheet(onDismiss) { close ->
        Column(Modifier.padding(horizontal = 24.dp).padding(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Import workouts", style = Not.type.title, modifier = Modifier.padding(top = 4.dp))
            Text(
                "Paste your log: a date like 30/10 starts a workout, a line like “24kg x 10 x 3” adds sets, any other line names an exercise.",
                style = Not.type.caption,
                color = Not.colors.inkMuted,
            )
            Field(
                text,
                { text = it; nothing = false },
                "30/10\nTirage vertical\n66kg x 8 x 3",
                singleLine = false,
                minLines = 6,
                filled = true,
                modifier = Modifier.fillMaxWidth().heightIn(max = 320.dp).verticalScroll(rememberScrollState()),
            )
            if (nothing) Text("No dated workouts found.", style = Not.type.caption, color = Not.colors.danger)
            Button("Import", {
                if (viewModel.import(text) > 0) close() else nothing = true
            }, Modifier.fillMaxWidth(), enabled = text.isNotBlank())
        }
    }
}

// ---- editor --------------------------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WorkoutEditor(workout: Workout, state: WorkoutsState, viewModel: WorkoutsViewModel) {
    val colors = Not.colors
    val context = LocalContext.current
    var menu by remember { mutableStateOf(false) }
    var title by remember(workout.id) { mutableStateOf(workout.title) }
    var titleFocused by remember { mutableStateOf(false) }
    var newExercise by remember { mutableStateOf("") }
    // Focus moves to a new exercise's sets field once it shows up.
    var focusSetsOf by remember { mutableStateOf<String?>(null) }
    var history by remember { mutableStateOf<String?>(null) }
    val addFocus = remember { FocusRequester() }

    LaunchedEffect(workout.title) { if (!titleFocused) title = workout.title }
    LaunchedEffect(workout.id) { if (workout.exercises.isEmpty()) addFocus.requestFocus() }

    Box(Modifier.fillMaxSize().background(colors.background)) {
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier.fillMaxWidth().statusBarsPadding().height(56.dp).padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(R.drawable.ic_arrow_back, "Back", { viewModel.open(null) })
                Row(
                    Modifier
                        .clip(CircleShape)
                        .pressable({
                            val d = runCatching { LocalDate.parse(workout.date) }.getOrDefault(state.today)
                            DatePickerDialog(context, { _, y, m, day ->
                                viewModel.update(workout, date = LocalDate.of(y, m + 1, day).toString())
                            }, d.year, d.monthValue - 1, d.dayOfMonth).show()
                        }, onClickLabel = "Change date")
                        .padding(horizontal = 10.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon(R.drawable.ic_calendar, null, tint = colors.inkMuted, size = 18.dp)
                    Text(dayLabel(workout.date, state.today, long = true), style = Not.type.label, color = colors.inkMuted)
                }
                Spacer(Modifier.weight(1f))
                Box {
                    IconButton(R.drawable.ic_more_vert, "More", { menu = true })
                    Menu(menu, onDismiss = { menu = false }) {
                        MenuItem("Delete workout", icon = R.drawable.ic_delete, danger = true, onClick = {
                            menu = false
                            state.workouts.firstOrNull { it.id == workout.id }?.let(viewModel::delete)
                        })
                    }
                }
            }
            LazyColumn(
                Modifier.weight(1f).imePadding(),
                contentPadding = PaddingValues(bottom = 120.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                item(key = "title") {
                    Field(
                        title,
                        { title = it },
                        dayLabel(workout.date, state.today),
                        style = Not.type.display,
                        onDone = { viewModel.update(workout, title = title) },
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 24.dp)
                            .padding(bottom = 6.dp)
                            .onFocusChanged {
                                if (titleFocused && !it.isFocused && title != workout.title) viewModel.update(workout, title = title)
                                titleFocused = it.isFocused
                            },
                    )
                }
                itemsIndexed(workout.exercises, key = { _, e -> e.id }) { index, exercise ->
                    ExerciseCard(
                        exercise,
                        index,
                        workout.exercises.size,
                        state.today,
                        viewModel,
                        focusSets = focusSetsOf == exercise.id,
                        onFocused = { focusSetsOf = null },
                        onHistory = { history = exercise.name },
                        modifier = Modifier.animateItem().padding(horizontal = 16.dp),
                    )
                }
                item(key = "add") {
                    AddExercise(
                        newExercise,
                        { newExercise = it },
                        suggestions = state.exerciseNames.filter { name ->
                            workout.exercises.none { it.name.equals(name, ignoreCase = true) } &&
                                (newExercise.isBlank() || name.contains(newExercise.trim(), ignoreCase = true))
                        }.take(8),
                        catalogue = if (newExercise.trim().length >= 2) {
                            viewModel.searchExercises(newExercise.trim(), 6).filter { e ->
                                workout.exercises.none { it.name.equals(catalogName(e, newExercise), ignoreCase = true) }
                            }
                        } else emptyList(),
                        typed = newExercise,
                        onAdd = { name ->
                            viewModel.addExercise(workout.id, name)?.let { focusSetsOf = it }
                            newExercise = ""
                        },
                        modifier = Modifier.padding(horizontal = 24.dp).padding(top = 4.dp).focusRequester(addFocus),
                    )
                }
            }
        }
        RestBar(viewModel, Modifier.align(Alignment.BottomCenter).imePadding().navigationBarsPadding().padding(bottom = 12.dp))
    }
    history?.let { name -> HistorySheet(name, viewModel, state.today, onDismiss = { history = null }) }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AddExercise(
    value: String,
    onValue: (String) -> Unit,
    suggestions: List<String>,
    catalogue: List<CatalogExercise>,
    typed: String,
    onAdd: (String) -> Unit,
    modifier: Modifier,
) {
    val colors = Not.colors
    var focused by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Field(
            value,
            onValue,
            "Add exercise",
            style = Not.type.bodyStrong,
            leading = { Icon(R.drawable.ic_add, null, tint = colors.accentInk, size = 22.dp) },
            onDone = { if (value.isNotBlank()) onAdd(value) },
            modifier = modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused },
        )
        AnimatedVisibility(focused && suggestions.isNotEmpty(), enter = fadeIn(), exit = fadeOut()) {
            FlowRow(
                Modifier.padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                suggestions.forEach { name ->
                    Text(
                        name,
                        style = Not.type.label,
                        color = colors.ink,
                        maxLines = 1,
                        modifier = Modifier
                            .clip(CircleShape)
                            .background(colors.surface)
                            .pressable({ onAdd(name) }, pressedScale = 0.94f)
                            .padding(horizontal = 14.dp, vertical = 9.dp),
                    )
                }
            }
        }
        // From wger's database, under what was used before.
        if (focused && catalogue.isNotEmpty()) {
            Column(Modifier.padding(horizontal = 12.dp)) {
                catalogue.forEach { e ->
                    val name = catalogName(e, typed)
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(14.dp))
                            .pressable({ onAdd(name) }, pressedScale = 0.98f)
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text(name, style = Not.type.body, color = colors.ink, maxLines = 1)
                        Text(
                            (listOfNotNull(otherName(e, name)) + e.muscles.take(2)).joinToString(" · "),
                            style = Not.type.caption,
                            color = colors.inkFaint,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
    }
}

/** The catalogue name to insert: the one matching what was typed, else French, else English. */
private fun catalogName(e: CatalogExercise, typed: String): String {
    val q = typed.trim()
    return listOfNotNull(e.nameFr, e.nameEn).firstOrNull { it.contains(q, ignoreCase = true) } ?: e.nameFr ?: e.nameEn
}

private fun otherName(e: CatalogExercise, shown: String): String? =
    (if (shown == e.nameEn) e.nameFr else e.nameEn)?.takeIf { it != shown }

@Composable
private fun ExerciseCard(
    exercise: WorkoutExercise,
    index: Int,
    count: Int,
    today: LocalDate,
    viewModel: WorkoutsViewModel,
    focusSets: Boolean,
    onFocused: () -> Unit,
    onHistory: () -> Unit,
    modifier: Modifier,
) {
    val colors = Not.colors
    var name by remember(exercise.id) { mutableStateOf(exercise.name) }
    var nameFocused by remember { mutableStateOf(false) }
    var draft by remember(exercise.id) { mutableStateOf("") }
    var editing by remember { mutableStateOf<Int?>(null) }
    var menu by remember { mutableStateOf(false) }
    val setsFocus = remember { FocusRequester() }

    LaunchedEffect(exercise.name) { if (!nameFocused) name = exercise.name }
    LaunchedEffect(focusSets) {
        if (focusSets) {
            setsFocus.requestFocus()
            onFocused()
        }
    }

    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(22.dp))
            .background(colors.surface)
            .padding(start = 18.dp, end = 6.dp, top = 8.dp, bottom = 12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Field(
                name,
                { name = it },
                "Exercise",
                style = Not.type.heading,
                onDone = { viewModel.rename(exercise, name) },
                modifier = Modifier.weight(1f).onFocusChanged {
                    if (nameFocused && !it.isFocused) {
                        if (name.isBlank()) name = exercise.name else viewModel.rename(exercise, name)
                    }
                    nameFocused = it.isFocused
                },
            )
            IconButton(R.drawable.ic_chart, "History", onHistory, tint = colors.inkMuted)
            Box {
                IconButton(R.drawable.ic_more_vert, "Exercise options", { menu = true }, tint = colors.inkMuted)
                Menu(menu, onDismiss = { menu = false }) {
                    MenuItem("Move up", icon = R.drawable.ic_arrow_up, enabled = index > 0, onClick = {
                        menu = false
                        viewModel.moveExercise(exercise, index - 1)
                    })
                    MenuItem("Move down", icon = R.drawable.ic_expand_more, enabled = index < count - 1, onClick = {
                        menu = false
                        viewModel.moveExercise(exercise, index + 1)
                    })
                    MenuItem("Remove", icon = R.drawable.ic_delete, danger = true, onClick = {
                        menu = false
                        viewModel.deleteExercise(exercise)
                    })
                }
            }
        }

        val info = remember(exercise.name, exercise.sets) { viewModel.exerciseInfo(exercise.name) }
        if (info != null) {
            Text((info.muscles + info.equipment).joinToString(" · "), style = Not.type.caption, color = colors.inkFaint)
        }

        exercise.previous?.let { previous ->
            Row(Modifier.padding(end = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Column(Modifier.weight(1f)) {
                    Text("Last · ${dayLabel(previous.date, today)}", style = Not.type.caption, color = colors.inkMuted)
                    Text(previous.lines.joinToString("  ·  ") { it.label }, style = Not.type.caption, color = colors.inkFaint, maxLines = 2)
                }
                if (exercise.sets.isEmpty()) {
                    Button("Same again", { viewModel.setSets(exercise, previous.lines.map { it.raw }) }, kind = ButtonKind.Quiet, icon = R.drawable.ic_copy)
                }
            }
        }

        exercise.lines.forEachIndexed { i, line ->
            if (editing == i) {
                LineEditor(line.raw, onDone = { text ->
                    editing = null
                    val lines = exercise.lines.map { it.raw }.toMutableList()
                    if (text.isBlank()) lines.removeAt(i) else lines[i] = text.trim()
                    if (lines != exercise.lines.map { it.raw }) viewModel.setSets(exercise, lines)
                })
            } else {
                SetRow(line, onClick = { editing = i })
            }
        }

        val preview = draft.takeIf { it.isNotBlank() }?.let { parseSets(it).firstOrNull() }
        Field(
            draft,
            { draft = it },
            exercise.previous?.lines?.lastOrNull { it.parsed && !it.warmup }?.raw ?: "24kg x 10 x 3",
            leading = { Icon(R.drawable.ic_add, null, tint = colors.inkFaint, size = 18.dp) },
            trailing = {
                if (preview != null) {
                    Text(
                        if (preview.parsed) preview.label else "Note",
                        style = Not.type.caption,
                        color = if (preview.parsed) colors.accentInk else colors.inkFaint,
                        maxLines = 1,
                    )
                }
            },
            onDone = {
                viewModel.addSets(exercise, draft)
                draft = ""
            },
            modifier = Modifier.fillMaxWidth().padding(end = 12.dp).focusRequester(setsFocus),
        )
    }
}

@Composable
private fun SetRow(line: SetLine, onClick: () -> Unit) {
    val colors = Not.colors
    Row(
        Modifier
            .fillMaxWidth()
            .pressable(onClick, pressedScale = 0.98f, onClickLabel = "Edit")
            .heightIn(min = 32.dp)
            .padding(end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier
                .size(7.dp)
                .clip(CircleShape)
                .background(
                    when {
                        !line.parsed -> Color.Transparent
                        line.warmup -> colors.inkFaint
                        else -> colors.accent
                    },
                ),
        )
        Text(
            line.label,
            style = if (line.parsed && !line.warmup) Not.type.bodyStrong else Not.type.body.merge(fontStyle = if (line.parsed) FontStyle.Normal else FontStyle.Italic),
            color = if (line.parsed && !line.warmup) colors.ink else colors.inkMuted,
        )
        line.note?.let {
            Text(it, style = Not.type.body.merge(fontStyle = FontStyle.Italic), color = colors.inkMuted, maxLines = 2, modifier = Modifier.weight(1f, fill = false))
        }
    }
}

@Composable
private fun LineEditor(initial: String, onDone: (String) -> Unit) {
    var text by remember { mutableStateOf(initial) }
    var wasFocused by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Field(
        text,
        { text = it },
        "Empty removes the line",
        filled = true,
        onDone = { onDone(text) },
        modifier = Modifier
            .fillMaxWidth()
            .padding(end = 12.dp)
            .focusRequester(focus)
            .onFocusChanged {
                if (wasFocused && !it.isFocused) onDone(text)
                wasFocused = it.isFocused
            },
    )
}

// ---- history -------------------------------------------------------------------------------------

@Composable
private fun HistorySheet(name: String, viewModel: WorkoutsViewModel, today: LocalDate, onDismiss: () -> Unit) {
    val colors = Not.colors
    // Re-read after linking: history follows the link.
    var version by remember { mutableStateOf(0) }
    val logs = remember(version) { viewModel.history(name) }
    val info = remember(version) { viewModel.exerciseInfo(name) }
    var linking by remember { mutableStateOf(false) }
    Sheet(onDismiss) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(name, style = Not.type.title, modifier = Modifier.padding(top = 4.dp))
            if (linking) {
                LinkPicker(name, info, viewModel, onDone = {
                    linking = false
                    version++
                })
                return@Column
            }
            if (info != null) {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(otherName(info, name) ?: info.nameEn, style = Not.type.body, color = colors.inkMuted, modifier = Modifier.weight(1f))
                        Button("Change", { linking = true }, kind = ButtonKind.Quiet)
                    }
                    Text(
                        listOf(info.muscles.joinToString(", "), info.secondaryMuscles.takeIf { it.isNotEmpty() }?.joinToString(", ", prefix = "also "), info.equipment.joinToString(", "))
                            .filter { !it.isNullOrEmpty() }.joinToString(" · "),
                        style = Not.type.caption,
                        color = colors.ink,
                    )
                    Text("wger.de${if (info.author.isEmpty()) "" else " · ${info.author}"} · ${info.license}", style = Not.type.caption, color = colors.inkFaint)
                }
            } else {
                Button("Link to an exercise", { linking = true }, kind = ButtonKind.Quiet, icon = R.drawable.ic_search)
            }
            if (logs.isEmpty()) {
                Text("Nothing logged yet.", style = Not.type.body, color = colors.inkMuted)
                return@Column
            }
            // Cardio (no weights) charts distance instead of the top set.
            val cardio = logs.all { it.topWeight == null } && logs.any { it.distance > 0 }
            val points = logs.reversed().mapNotNull { if (cardio) it.distance.takeIf { d -> d > 0 }?.div(1000) else it.topWeight }
            if (points.size > 1) Sparkline(points, Modifier.fillMaxWidth().height(96.dp))
            if (cardio) {
                Text("Longest ${formatWeight(logs.maxOf { it.distance } / 1000)} km · ${logs.size} sessions", style = Not.type.caption, color = colors.inkMuted)
            } else {
                logs.mapNotNull { it.topWeight }.maxOrNull()?.let { best ->
                    Text("Best ${formatWeight(best)} kg · ${logs.size} sessions", style = Not.type.caption, color = colors.inkMuted)
                }
            }
            Divider()
            logs.forEach { log ->
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(dayLabel(log.date, today, long = true), style = Not.type.label, color = colors.ink)
                    log.lines.forEach { line ->
                        Text(line.note?.let { "${line.label} — $it" } ?: line.label, style = Not.type.body, color = if (line.parsed && !line.warmup) colors.ink else colors.inkMuted)
                    }
                }
            }
        }
    }
}

/** Search wger's database to link [name]; linked names share history. */
@Composable
private fun LinkPicker(name: String, info: CatalogExercise?, viewModel: WorkoutsViewModel, onDone: () -> Unit) {
    val colors = Not.colors
    var query by remember { mutableStateOf(info?.nameEn ?: name.replace(Regex("\\(.*?\\)"), "").trim()) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Field(
        query,
        { query = it },
        "Search exercises",
        filled = true,
        shape = CircleShape,
        leading = { Icon(R.drawable.ic_search, null, tint = colors.inkMuted, size = 20.dp) },
        modifier = Modifier.fillMaxWidth().focusRequester(focus),
    )
    viewModel.searchExercises(query, 20).forEach { e ->
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .pressable({
                    viewModel.link(name, e.id)
                    onDone()
                }, pressedScale = 0.98f)
                .padding(vertical = 6.dp),
        ) {
            Text(e.nameFr ?: e.nameEn, style = Not.type.body, color = colors.ink)
            Text((listOfNotNull(e.nameEn.takeIf { e.nameFr != null }) + e.muscles).joinToString(" · "), style = Not.type.caption, color = colors.inkFaint)
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (info != null) Button("Unlink", { viewModel.link(name, null); onDone() }, kind = ButtonKind.Danger)
        Button("Cancel", onDone, kind = ButtonKind.Secondary)
    }
}

/** Top-set weight over time, oldest left. */
@Composable
private fun Sparkline(values: List<Double>, modifier: Modifier) {
    val line = Not.colors.accentInk
    val dot = Not.colors.accent
    Canvas(modifier) {
        val min = values.min()
        val span = (values.max() - min).takeIf { it > 0 } ?: 1.0
        val step = size.width / (values.size - 1)
        val points = values.mapIndexed { i, v ->
            Offset(i * step, (size.height - 8f) - ((v - min) / span * (size.height - 16f)).toFloat())
        }
        for (i in 1 until points.size) {
            drawLine(line, points[i - 1], points[i], strokeWidth = 3.dp.toPx(), cap = StrokeCap.Round)
        }
        points.forEach {
            drawCircle(dot, 4.dp.toPx(), it)
            drawCircle(line, 4.dp.toPx(), it, style = Stroke(1.5.dp.toPx()))
        }
    }
}

// ---- rest timer ----------------------------------------------------------------------------------

/** A pill at the bottom: tap to rest, and while resting the countdown with ±15 s and stop. */
@Composable
private fun RestBar(viewModel: WorkoutsViewModel, modifier: Modifier) {
    val timer = viewModel.timer
    val colors = Not.colors
    val context = LocalContext.current
    val endsAt by timer.endsAt.collectAsStateWithLifecycle()
    val duration by timer.duration.collectAsStateWithLifecycle()
    var choosing by remember { mutableStateOf(false) }
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(endsAt) {
        while (endsAt != null) {
            now = System.currentTimeMillis()
            delay(250)
        }
    }
    val askPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    fun start(seconds: Int = duration) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            askPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        timer.start(seconds)
    }

    val ends = endsAt
    Row(
        modifier
            .shadow(18.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.18f), spotColor = Color.Black.copy(alpha = 0.18f))
            .clip(CircleShape)
            .background(if (ends != null) colors.accent else colors.surface)
            .padding(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (ends != null) {
            val left = ((ends - now + 999) / 1000).toInt().coerceAtLeast(0)
            PillText("−15", colors.onAccent) { timer.extend(-15) }
            Text(
                RestTimer.format(left),
                style = Not.type.title,
                color = colors.onAccent,
                modifier = Modifier.padding(horizontal = 10.dp),
            )
            PillText("+15", colors.onAccent) { timer.extend(15) }
            IconButton(R.drawable.ic_close, "Stop rest timer", timer::stop, tint = colors.onAccent)
        } else {
            Row(
                Modifier
                    .clip(CircleShape)
                    .pressable({ start() }, pressedScale = 0.95f, onClickLabel = "Start rest timer")
                    .padding(start = 14.dp, end = 10.dp, top = 10.dp, bottom = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(R.drawable.ic_timer, null, tint = colors.accentInk, size = 20.dp)
                Text("Rest ${RestTimer.format(duration)}", style = Not.type.label, color = colors.ink)
            }
            IconButton(R.drawable.ic_expand_more, "Rest options", { choosing = true }, tint = colors.inkMuted)
        }
    }
    if (choosing) {
        val auto by timer.startsAfterSet.collectAsStateWithLifecycle()
        Sheet({ choosing = false }) { close ->
            Column(Modifier.padding(horizontal = 12.dp).padding(bottom = 16.dp)) {
                Text("Rest", style = Not.type.title, modifier = Modifier.padding(start = 14.dp, top = 4.dp, bottom = 12.dp))
                RestTimer.presets.forEach { s ->
                    SheetRow(R.drawable.ic_timer, RestTimer.format(s), accent = s == duration) {
                        start(s)
                        close()
                    }
                }
                Divider(Modifier.padding(horizontal = 14.dp, vertical = 8.dp))
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clip(CircleShape)
                        .pressable({ timer.setStartsAfterSet(!auto) }, pressedScale = 0.98f)
                        .heightIn(min = 52.dp)
                        .padding(horizontal = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RoundCheck(auto, { timer.setStartsAfterSet(!auto) })
                    Text("Start after each set", style = Not.type.body, modifier = Modifier.padding(start = 4.dp))
                }
            }
        }
    }
}

@Composable
private fun PillText(text: String, color: Color, onClick: () -> Unit) {
    Text(
        text,
        style = Not.type.label,
        color = color,
        modifier = Modifier
            .clip(CircleShape)
            .pressable(onClick, pressedScale = 0.9f, onClickLabel = text)
            .padding(horizontal = 12.dp, vertical = 12.dp),
    )
}

// ---- formatting ----------------------------------------------------------------------------------

private val shortDay = DateTimeFormatter.ofPattern("EEE d MMM")
private val longDay = DateTimeFormatter.ofPattern("EEEE d MMMM")
private val otherYear = DateTimeFormatter.ofPattern("d MMM yyyy")

/** "Today", "Yesterday", "Mon 2 Nov", or with the year when it isn't this one. */
fun dayLabel(date: String, today: LocalDate, long: Boolean = false): String {
    val d = runCatching { LocalDate.parse(date) }.getOrNull() ?: return date
    return when (ChronoUnit.DAYS.between(d, today)) {
        0L -> "Today"
        1L -> "Yesterday"
        else -> d.format(if (d.year != today.year) otherYear else if (long) longDay else shortDay)
    }
}

fun formatWeight(w: Double): String = "%.2f".format(w).trimEnd('0').trimEnd('.', ',')
