package dev.lysen.not

import android.Manifest
import android.content.Context
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Text
import dev.lysen.not.widget.HabitsWidgetReceiver
import dev.lysen.not.widget.TasksWidgetReceiver
import dev.lysen.not.widget.TimedWidgetReceiver
import kotlinx.coroutines.launch

/**
 * Whether the first-run steps are behind us. Devices that are paired or already hold data (every
 * install from before this existed) count as done.
 */
object FirstRun {
    private const val KEY = "onboarded"

    private fun prefs(context: Context) = context.getSharedPreferences("app", Context.MODE_PRIVATE)

    fun needed(app: NotApp): Boolean {
        if (prefs(app).getBoolean(KEY, false)) return false
        val established = runCatching {
            app.store.syncInfo() != null || app.store.views(today()).any { app.store.viewOutline(it.id, today()).open.isNotEmpty() } ||
                app.store.notes(null).isNotEmpty() || app.store.habitsNow(today()).isNotEmpty()
        }.getOrDefault(true)
        if (established) done(app)
        return !established
    }

    fun done(context: Context) = prefs(context).edit().putBoolean(KEY, true).apply()
}

private enum class Step { Welcome, Data, Notifications, Widgets }

/**
 * Four short steps on a fresh install: what the app is, bringing data in (pair with a server, or
 * restore a backup file, or neither), notifications, and widgets. Every step can be skipped, and
 * everything is in Settings later.
 */
@Composable
fun Onboarding(viewModel: TasksViewModel, onDone: () -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext as NotApp
    val state by viewModel.state.collectAsStateWithLifecycle()
    var step by rememberSaveable { mutableIntStateOf(0) }
    val steps = Step.entries
    fun next() {
        if (step < steps.lastIndex) step++ else {
            FirstRun.done(context)
            onDone()
        }
    }
    if (step > 0) BackHandler { step-- }
    val colors = Not.colors

    Column(Modifier.fillMaxSize().background(colors.background).statusBarsPadding().navigationBarsPadding()) {
        // Fixed height, so the dots don't move when Skip goes away on the last step.
        Row(Modifier.fillMaxWidth().height(60.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            // Where we are: one dot per step.
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                steps.indices.forEach { i ->
                    Box(Modifier.size(if (i == step) 20.dp else 7.dp, 7.dp).clip(CircleShape).background(if (i <= step) colors.accent else colors.sunken))
                }
            }
            if (step < steps.lastIndex) Button("Skip", { FirstRun.done(context); onDone() }, kind = ButtonKind.Quiet)
        }
        AnimatedContent(step, label = "step", modifier = Modifier.weight(1f)) { shown ->
            Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 28.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                when (steps[shown]) {
                    Step.Welcome -> Welcome()
                    Step.Data -> DataStep(state, viewModel, app, onNext = ::next)
                    Step.Notifications -> NotificationsStep(app, onNext = ::next)
                    Step.Widgets -> WidgetsStep()
                }
            }
        }
        Button(
            when (steps[step]) {
                Step.Welcome -> "Get started"
                Step.Data -> if (state.syncInfo != null) "Continue" else "Use on this phone only"
                Step.Notifications -> "Not now"
                Step.Widgets -> "Done"
            },
            ::next,
            Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 16.dp),
            kind = if (steps[step] == Step.Notifications || (steps[step] == Step.Data && state.syncInfo == null)) ButtonKind.Secondary else ButtonKind.Primary,
        )
    }
}

@Composable
private fun StepHeader(icon: Int, title: String, text: String) {
    val colors = Not.colors
    Spacer(Modifier.height(24.dp))
    Box(Modifier.size(64.dp).clip(CircleShape).background(colors.accentSoft), contentAlignment = Alignment.Center) {
        Icon(icon, null, tint = colors.accentInk, size = 30.dp)
    }
    Text(title, style = Not.type.display)
    Text(text, style = Not.type.body, color = colors.inkMuted)
}

@Composable
private fun Welcome() {
    StepHeader(
        R.drawable.ic_task_alt,
        "Welcome to not",
        "Tasks, notes, habits and workouts in one calm place. Everything works offline, and syncs through a server you run yourself.",
    )
    listOf(
        R.drawable.ic_today to "Plan with Today, Upcoming and smart lists",
        R.drawable.ic_repeat to "Build habits, check-ins or timed",
        R.drawable.ic_note to "Write notes with checklists and pictures",
        R.drawable.ic_add_task to "Add from anywhere: widgets, the share menu, Quick Settings",
    ).forEach { (icon, text) ->
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            Icon(icon, null, tint = Not.colors.accentInk, size = 22.dp)
            Text(text, style = Not.type.body)
        }
    }
}

/** Pair with a server, or restore a backup file; pairing moves on by itself. */
@Composable
private fun DataStep(state: UiState, viewModel: TasksViewModel, app: NotApp, onNext: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var restored by remember { mutableStateOf<String?>(null) }
    val restore = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        restored = runCatching {
            val bytes = context.contentResolver.openInputStream(uri)!!.use { it.readBytes() }
            app.store.restoreBackup(bytes)
            "Backup restored."
        }.getOrElse { if (it is dev.lysen.not.core.NotException) it.userMessage else "Couldn't read that file." }
    }
    // Paired just now: on to the next step after a beat, so the confirmation shows.
    LaunchedEffect(state.syncInfo != null) {
        if (state.syncInfo != null) scope.launch { kotlinx.coroutines.delay(900); onNext() }
    }
    StepHeader(
        R.drawable.ic_sync,
        "Bring your data",
        "Pair with your server to sync with your other devices, or restore a backup file. You can do either later in Settings.",
    )
    if (state.syncInfo != null) {
        Text("Paired with ${state.syncInfo?.serverUrl}.", style = Not.type.bodyStrong, color = Not.colors.accentInk)
    } else {
        PairForm(state, viewModel, intro = false)
    }
    Spacer(Modifier.height(4.dp))
    Button("Restore a backup file", { restore.launch(arrayOf("*/*")) }, Modifier.fillMaxWidth(), kind = ButtonKind.Secondary, icon = R.drawable.ic_history)
    restored?.let { Text(it, style = Not.type.caption, color = Not.colors.inkMuted) }
}

@Composable
private fun NotificationsStep(app: NotApp, onNext: () -> Unit) {
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { onNext() }
    StepHeader(
        R.drawable.ic_schedule,
        "Reminders",
        "Task times, a morning summary, habit nudges, and timed habits on the lock screen. They're scheduled on this phone, so they come on time even offline.",
    )
    Button(
        "Allow notifications",
        { if (Build.VERSION.SDK_INT >= 33 && !app.reminders.canNotify) ask.launch(Manifest.permission.POST_NOTIFICATIONS) else onNext() },
        Modifier.fillMaxWidth(),
        icon = R.drawable.ic_schedule,
    )
}

@Composable
private fun WidgetsStep() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    StepHeader(
        R.drawable.ic_add_task,
        "On your home screen",
        "Widgets show what's next and let you tick it off without opening the app. Long-press one later to change what it shows and how it looks.",
    )
    listOf(
        Triple(R.drawable.ic_task_alt, "Tasks", TasksWidgetReceiver::class.java),
        Triple(R.drawable.ic_repeat, "Habits", HabitsWidgetReceiver::class.java),
        Triple(R.drawable.ic_timer, "Timed habit", TimedWidgetReceiver::class.java),
    ).forEach { (icon, label, receiver) ->
        Button("Add the $label widget", { scope.launch { Shortcuts.requestWidget(context, receiver) } }, Modifier.fillMaxWidth(), kind = ButtonKind.Secondary, icon = icon)
    }
    Text(
        "Share text, links or pictures to not from any app, too.",
        style = Not.type.caption,
        color = Not.colors.inkMuted,
        textAlign = TextAlign.Start,
        modifier = Modifier.padding(top = 4.dp),
    )
}
