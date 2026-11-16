package dev.lysen.not

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.runtime.key
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.draw.rotate
import dev.lysen.not.ui.RoundCheck
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.rememberLauncherForActivityResult
import android.os.Build
import android.Manifest
import dev.lysen.not.ui.EmptyState
import dev.lysen.not.ui.SwipeAction
import dev.lysen.not.ui.SwipeRow
import dev.lysen.not.ui.ToastHost
import dev.lysen.not.ui.ToastState
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Device
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.pairingLink
import dev.lysen.not.pairing.QrScanner
import dev.lysen.not.pairing.qrBitmap
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Divider
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.IconButton
import dev.lysen.not.ui.LargeTitleScreen
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Spinner
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import dev.lysen.not.widget.HabitsWidgetReceiver
import dev.lysen.not.widget.NoteWidgetReceiver
import dev.lysen.not.widget.NotesWidgetReceiver
import dev.lysen.not.widget.TasksWidgetReceiver
import kotlinx.coroutines.launch

/** What Settings holds, one page each. */
private enum class SettingsPage(val title: String, val icon: Int) {
    Sync("Sync", R.drawable.ic_sync),
    Reminders("Reminders", R.drawable.ic_schedule),
    Archive("Archive", R.drawable.ic_history),
    Backup("Export and backup", R.drawable.ic_copy),
    Widgets("Widgets and shortcuts", R.drawable.ic_add_task),
    Workouts("Workouts", R.drawable.ic_fitness),
    About("About", R.drawable.ic_note),
}

/** A list of categories; each opens its own page, and back returns to the list. */
@Composable
fun SettingsScreen(tasks: TasksViewModel, workouts: WorkoutsViewModel, onBack: () -> Unit) {
    val state by tasks.state.collectAsStateWithLifecycle()
    var page by rememberSaveable { mutableStateOf<SettingsPage?>(null) }
    val bottom = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val toasts = remember { ToastState() }

    Box(Modifier.fillMaxSize()) {
    val shown = page
    if (shown == null) {
        LargeTitleScreen(
            title = "Settings",
            navigation = { IconButton(R.drawable.ic_arrow_back, "Back", onBack) },
            bottomPadding = bottom,
        ) {
            item(key = "pages") {
                Card {
                    SettingsPage.entries.forEachIndexed { i, p ->
                        if (i > 0) Divider(Modifier.padding(start = 56.dp))
                        CategoryRow(p, detail = summary(p, state)) { page = p }
                    }
                }
            }
        }
    } else {
        BackHandler { page = null }
        // Keyed so each page starts at its top.
        key(shown) {
            LargeTitleScreen(
                title = shown.title,
                navigation = { IconButton(R.drawable.ic_arrow_back, "Settings", { page = null }) },
                bottomPadding = bottom,
            ) {
                when (shown) {
                    SettingsPage.Sync -> item(key = "sync") {
                        Section(null) { if (state.syncInfo == null) PairForm(state, tasks) else PairedInfo(state, tasks) }
                    }
                    SettingsPage.Reminders -> item(key = "reminders") { RemindersPage() }
                    SettingsPage.Archive -> archivePage(state, tasks, toasts)
                    SettingsPage.Backup -> item(key = "backup") { BackupPage(toasts) }
                    SettingsPage.Widgets -> item(key = "widgets") { WidgetsPage() }
                    SettingsPage.Workouts -> item(key = "workouts") { WorkoutsPage(workouts) }
                    SettingsPage.About -> item(key = "about") { AboutPage() }
                }
            }
        }
    }
    ToastHost(toasts, Modifier.align(Alignment.BottomCenter).padding(bottom = bottom + 16.dp))
    }

    state.error?.let {
        CardDialog(
            title = "Something went wrong",
            onDismiss = tasks::dismissError,
            buttons = { Button("OK", tasks::dismissError) },
        ) { Text(it, color = Not.colors.inkMuted) }
    }
}

/** The line under a category: where things stand there, at a glance. */
private fun summary(page: SettingsPage, state: UiState): String = when (page) {
    SettingsPage.Sync -> state.syncInfo?.let { "${it.serverUrl.removePrefix("https://").removePrefix("http://")} · ${syncStatus(state.sync)}" } ?: "Not paired"
    SettingsPage.Reminders -> "Task times, morning summary, habits in the evening"
    SettingsPage.Archive -> when (val n = state.archive.size) {
        0 -> "Tasks done over a day ago"
        1 -> "1 task done over a day ago"
        else -> "$n tasks done over a day ago"
    }
    SettingsPage.Backup -> "Everything as JSON, or a backup file to restore"
    SettingsPage.Widgets -> "Home screen widgets and quick add"
    SettingsPage.Workouts -> "Rest timer, import from text"
    SettingsPage.About -> "Version ${BuildConfig.VERSION_NAME}, credits"
}

@Composable
private fun CategoryRow(page: SettingsPage, detail: String, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .pressable(onClick, pressedScale = 0.985f)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(28.dp), contentAlignment = Alignment.Center) { Icon(page.icon, null, tint = Not.colors.accentInk, size = 22.dp) }
        Column(Modifier.weight(1f).padding(start = 12.dp)) {
            Text(page.title, style = Not.type.body)
            Text(detail, style = Not.type.caption, color = Not.colors.inkMuted, maxLines = 1)
        }
        Icon(R.drawable.ic_expand_more, null, tint = Not.colors.inkFaint, size = 20.dp, modifier = Modifier.rotate(-90f))
    }
}

@Composable
private fun WidgetsPage() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    Section("Home screen", padded = false) {
        ShortcutRow(R.drawable.ic_task_alt, "Tasks widget", "Open tasks of a list, with checkboxes and quick add") {
            scope.launch { Shortcuts.requestWidget(context, TasksWidgetReceiver::class.java) }
        }
        Divider(Modifier.padding(start = 56.dp))
        ShortcutRow(R.drawable.ic_repeat, "Habits widget", "Today's habits; tap one to check in") {
            scope.launch { Shortcuts.requestWidget(context, HabitsWidgetReceiver::class.java) }
        }
        Divider(Modifier.padding(start = 56.dp))
        ShortcutRow(R.drawable.ic_today, "Up next widget", "The next task due, to tick off at a glance") {
            scope.launch { Shortcuts.requestWidget(context, dev.lysen.not.widget.NextWidgetReceiver::class.java) }
        }
        Divider(Modifier.padding(start = 56.dp))
        ShortcutRow(R.drawable.ic_note, "Notes widget", "Pinned and recent notes of a folder") {
            scope.launch { Shortcuts.requestWidget(context, NotesWidgetReceiver::class.java) }
        }
        Divider(Modifier.padding(start = 56.dp))
        ShortcutRow(R.drawable.ic_checklist, "Note widget", "One note, with tickable checklists") {
            scope.launch { Shortcuts.requestWidget(context, NoteWidgetReceiver::class.java) }
        }
    }
    if (Shortcuts.canRequestTile) {
        Section("Quick Settings", padded = false) {
            ShortcutRow(R.drawable.ic_add_task, "Quick Settings tile", "Add a task from the notification shade") {
                Shortcuts.requestTile(context)
            }
        }
    }
}

@Composable
private fun WorkoutsPage(viewModel: WorkoutsViewModel) {
    val colors = Not.colors
    val auto by viewModel.timer.startsAfterSet.collectAsStateWithLifecycle()
    Section("Rest timer", padded = false) {
        Row(
            Modifier
                .fillMaxWidth()
                .pressable({ viewModel.timer.setStartsAfterSet(!auto) }, pressedScale = 0.985f)
                .padding(horizontal = 16.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text("Start after each set", style = Not.type.body)
                Text("Logging a working set starts the countdown", style = Not.type.caption, color = colors.inkMuted)
            }
            RoundCheck(auto, { viewModel.timer.setStartsAfterSet(!auto) })
        }
    }

    var text by rememberSaveable { mutableStateOf("") }
    // How many the last import added; null before one, or once the text changes.
    var added by remember { mutableStateOf<Int?>(null) }
    Section("Import from text") {
        Text(
            "Paste your log: a date like 30/10 starts a workout, a line like “24kg x 10 x 3” adds sets, any other line names an exercise.",
            style = Not.type.caption,
            color = colors.inkMuted,
        )
        Field(
            text,
            {
                text = it
                added = null
            },
            "30/10\nTirage vertical\n66kg x 8 x 3",
            singleLine = false,
            minLines = 6,
            filled = true,
            style = Not.type.body.merge(fontFamily = FontFamily.Monospace),
            modifier = Modifier.fillMaxWidth().heightIn(max = 320.dp),
        )
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button("Import", {
                val n = viewModel.import(text)
                if (n > 0) text = ""
                added = n
            }, kind = ButtonKind.Secondary, enabled = text.isNotBlank())
            added?.let { n ->
                Text(
                    if (n == 0) "No dated workouts found." else if (n == 1) "Added 1 workout." else "Added $n workouts.",
                    style = Not.type.caption,
                    color = if (n == 0) colors.danger else colors.inkMuted,
                )
            }
        }
    }
}

/**
 * Tasks done over a day ago, out of their lists' way. Untick one to put it back; swipe it away to
 * delete it. Each row says where it came from and when it was done.
 */
private fun LazyListScope.archivePage(state: UiState, viewModel: TasksViewModel, toasts: ToastState) {
    if (state.archive.isEmpty()) {
        item(key = "empty") {
            EmptyState(R.drawable.ic_history, "Nothing archived", "Tasks you complete stay in their list for a day, then move here. The Logbook (in Lists) shows them all by day.")
        }
        return
    }
    item(key = "clear") {
        val scope = rememberCoroutineScope()
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.End) {
            Button("Delete all", {
                val n = viewModel.clearArchive()
                if (n > 0) scope.launch { if (toasts.show(if (n == 1) "1 task deleted" else "$n tasks deleted", action = "Undo")) viewModel.undo() }
            }, kind = ButtonKind.Quiet, icon = R.drawable.ic_delete)
        }
    }
    items(state.archive, key = { it.id }) { task ->
        val colors = Not.colors
        val scope = rememberCoroutineScope()
        SwipeRow(
            start = null,
            end = SwipeAction(R.drawable.ic_delete, colors.danger, Color.White, dismisses = true) {
                viewModel.deleteArchived(task)
                scope.launch { if (toasts.show("Task deleted", action = "Undo")) viewModel.undo() }
            },
            modifier = Modifier.animateItem(),
        ) {
            Row(
                Modifier.fillMaxWidth().background(colors.background).heightIn(min = 56.dp).padding(horizontal = 12.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RoundCheck(true, {
                    viewModel.restore(task)
                    scope.launch { if (toasts.show("Reopened", action = "Undo")) viewModel.undo() }
                }, contentDescription = "Reopen")
                Column(Modifier.weight(1f).padding(start = 4.dp)) {
                    Text(task.title, style = Not.type.body, color = colors.inkMuted)
                    val done = task.completedAt?.let { DateUtils.getRelativeTimeSpanString(it, System.currentTimeMillis(), DateUtils.DAY_IN_MILLIS) }
                    Text(listOfNotNull(task.tags.joinToString(" ") { "#$it" }.ifEmpty { null }, done).joinToString(" · "), style = Not.type.caption, color = colors.inkFaint, maxLines = 1)
                }
            }
        }
    }
}

/** Files go wherever the system picker says: Downloads, Drive, a USB stick. */
@Composable
private fun BackupPage(toasts: ToastState) {
    val context = LocalContext.current
    val store = (context.applicationContext as NotApp).store
    val scope = rememberCoroutineScope()
    fun say(message: String) = scope.launch { toasts.show(message) }
    fun write(uri: android.net.Uri?, bytes: () -> ByteArray, done: String) {
        uri ?: return
        runCatching { context.contentResolver.openOutputStream(uri, "wt")!!.use { it.write(bytes()) } }
            .onSuccess { say(done) }
            .onFailure { say("Couldn't save: ${it.message}") }
    }
    val stamp = java.time.LocalDate.now().toString()
    val exportJson = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { uri ->
        write(uri, { store.exportJson(stamp).toByteArray() }, "Exported")
    }
    val saveBackup = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        write(uri, { store.backup() }, "Backup saved")
    }
    val restore = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        runCatching {
            val bytes = context.contentResolver.openInputStream(uri)!!.use { it.readBytes() }
            store.restoreBackup(bytes)
        }.onSuccess { say("Backup merged in") }
            .onFailure { say(if (it is dev.lysen.not.core.NotException) it.userMessage else "Couldn't read that file") }
    }
    Section("Export", padded = false) {
        ActionRow(R.drawable.ic_code, "Export as JSON", "Lists, tasks, habits, notes and workouts, readable anywhere") {
            exportJson.launch("not-export-$stamp.json")
        }
    }
    Section("Backup", padded = false) {
        ActionRow(R.drawable.ic_copy, "Save a backup file", "Everything with its history, to restore on any device") {
            saveBackup.launch("not-backup-$stamp.notbackup")
        }
        Divider(Modifier.padding(start = 56.dp))
        ActionRow(R.drawable.ic_history, "Restore from a backup", "Merges it in like a sync; nothing here is lost") {
            restore.launch(arrayOf("*/*"))
        }
    }
}

@Composable
private fun ActionRow(icon: Int, title: String, subtitle: String, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .pressable(onClick, pressedScale = 0.985f)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(28.dp), contentAlignment = Alignment.Center) { Icon(icon, null, tint = Not.colors.accentInk, size = 22.dp) }
        Column(Modifier.weight(1f).padding(start = 12.dp)) {
            Text(title, style = Not.type.body)
            Text(subtitle, style = Not.type.caption, color = Not.colors.inkMuted)
        }
    }
}

@Composable
private fun RemindersPage() {
    val context = LocalContext.current
    val reminders = (context.applicationContext as NotApp).reminders
    val s by reminders.settings.collectAsStateWithLifecycle()
    var allowed by remember { mutableStateOf(reminders.canNotify) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed = it }
    if (!allowed) {
        Section(null) {
            Text("Notifications are off for not, so reminders can't show.", style = Not.type.body)
            Button("Allow notifications", {
                if (Build.VERSION.SDK_INT >= 33) ask.launch(Manifest.permission.POST_NOTIFICATIONS)
            }, icon = R.drawable.ic_schedule)
        }
    }
    Section("Tasks", padded = false) {
        ToggleRow("When a task is due", "Tasks with a time, like “fri 9am”; Done and snooze from the notification", s.tasks) {
            reminders.update(s.copy(tasks = it))
        }
        Divider(Modifier.padding(start = 16.dp))
        ToggleRow("Morning summary", "What's on today, and anything left from earlier days", s.digest) { reminders.update(s.copy(digest = it)) }
        if (s.digest) TimeRow("Every day at", s.digestAt) { reminders.update(s.copy(digestAt = it)) }
        Divider(Modifier.padding(start = 16.dp))
        ToggleRow("Weekly review", "Sunday at 18:00: Inbox, what slipped, Someday", s.review) { reminders.update(s.copy(review = it)) }
    }
    CalendarSection()
    Section("Habits", padded = false) {
        ToggleRow("Evening nudge", "Names the habits still to do today; quiet when all are done", s.habits) {
            reminders.update(s.copy(habits = it))
        }
        if (s.habits) TimeRow("Every day at", s.habitsAt) { reminders.update(s.copy(habitsAt = it)) }
    }
    Text(
        "Reminders are set on this phone only.",
        style = Not.type.caption,
        color = Not.colors.inkFaint,
        modifier = Modifier.padding(horizontal = 28.dp, vertical = 12.dp),
    )
}

/** Today and Upcoming can show the phone's calendar beside tasks (read only). */
@Composable
private fun CalendarSection() {
    val context = LocalContext.current
    var on by remember { mutableStateOf(CalendarOverlay.enabled(context)) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        CalendarOverlay.setEnabled(context, granted)
        on = CalendarOverlay.enabled(context)
    }
    Section("Calendar", padded = false) {
        ToggleRow("Show calendar events", "In Today and Upcoming, from the calendars your calendar app shows", on) { wanted ->
            if (wanted && !CalendarOverlay.permitted(context)) {
                ask.launch(Manifest.permission.READ_CALENDAR)
            } else {
                CalendarOverlay.setEnabled(context, wanted)
                on = CalendarOverlay.enabled(context)
            }
        }
    }
}

@Composable
private fun ToggleRow(title: String, detail: String, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .pressable({ onChange(!on) }, pressedScale = 0.985f)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = Not.type.body)
            Text(detail, style = Not.type.caption, color = Not.colors.inkMuted)
        }
        RoundCheck(on, { onChange(!on) })
    }
}

@Composable
private fun TimeRow(label: String, minutes: Int, onChange: (Int) -> Unit) {
    val context = LocalContext.current
    Row(
        Modifier
            .fillMaxWidth()
            .pressable({
                android.app.TimePickerDialog(context, { _, h, m -> onChange(h * 60 + m) }, minutes / 60, minutes % 60, true).show()
            }, pressedScale = 0.985f, onClickLabel = "Change time")
            .padding(start = 16.dp, end = 20.dp, top = 4.dp, bottom = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = Not.type.caption, color = Not.colors.inkMuted, modifier = Modifier.weight(1f))
        Text("%d:%02d".format(minutes / 60, minutes % 60), style = Not.type.label, color = Not.colors.accentInk)
    }
}

@Composable
private fun AboutPage() {
    Section(null) {
        Text("not ${BuildConfig.VERSION_NAME}", style = Not.type.bodyStrong)
        Text("Tasks, habits, notes and workouts, synced through your own server.", style = Not.type.caption, color = Not.colors.inkMuted)
    }
    Section("Exercise data") {
        Text(
            "Exercise names, muscles and equipment come from the free exercise database of wger.de, shared by its contributors under CC-BY-SA 4.0, CC-BY-SA 3.0 or CC0. Each exercise credits its author in its history.",
            style = Not.type.caption,
            color = Not.colors.inkMuted,
        )
    }
}

/** The rounded group the category list sits in. */
@Composable
private fun Card(content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier
            .padding(horizontal = 16.dp)
            .padding(top = 8.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(24.dp))
            .background(Not.colors.surface),
        content = content,
    )
}

@Composable
private fun Section(title: String?, padded: Boolean = true, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.padding(horizontal = 16.dp).padding(top = if (title == null) 8.dp else 20.dp)) {
        if (title != null) Text(title, style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(start = 8.dp, bottom = 8.dp))
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(24.dp))
                .background(Not.colors.surface)
                .padding(if (padded) 20.dp else 0.dp),
            verticalArrangement = if (padded) Arrangement.spacedBy(12.dp) else Arrangement.Top,
            content = content,
        )
    }
}

@Composable
private fun ShortcutRow(icon: Int, title: String, subtitle: String, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .pressable(onClick, pressedScale = 0.985f)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(28.dp), contentAlignment = Alignment.Center) { Icon(icon, null, tint = Not.colors.accentInk, size = 22.dp) }
        Column(Modifier.weight(1f).padding(start = 12.dp)) {
            Text(title, style = Not.type.body)
            Text(subtitle, style = Not.type.caption, color = Not.colors.inkMuted)
        }
        Icon(R.drawable.ic_add, "Add", tint = Not.colors.inkFaint, size = 20.dp)
    }
}

/** A small caption above a field. */
@Composable
private fun Labeled(label: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, style = Not.type.caption, color = Not.colors.inkMuted, modifier = Modifier.padding(start = 4.dp))
        content()
    }
}

@Composable
private fun PairForm(state: UiState, viewModel: TasksViewModel) {
    var url by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    var name by remember { mutableStateOf(viewModel.defaultDeviceName) }
    var scanning by remember { mutableStateOf(false) }
    var scanError by remember { mutableStateOf(false) }
    val colors = Not.colors

    Text("Pair this device with your server to sync.", style = Not.type.body)
    Button("Scan QR code", { scanning = true }, Modifier.fillMaxWidth(), enabled = !state.busy)
    if (scanError) Text("That QR code isn't a pairing code.", style = Not.type.caption, color = colors.danger)

    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 4.dp)) {
        Divider(Modifier.weight(1f))
        Text("or enter it", style = Not.type.caption, color = colors.inkFaint, modifier = Modifier.padding(horizontal = 12.dp))
        Divider(Modifier.weight(1f))
    }
    Labeled("Server URL") { Field(url, { url = it }, "https://not.example.com", filled = true, modifier = Modifier.fillMaxWidth()) }
    Labeled("Pairing code") {
        Field(code, { code = it }, "XXXXX-XXXXX", filled = true, style = Not.type.body.merge(fontFamily = FontFamily.Monospace), modifier = Modifier.fillMaxWidth())
    }
    Labeled("Device name") { Field(name, { name = it }, "This phone", filled = true, modifier = Modifier.fillMaxWidth()) }
    Text(
        "Get a code on the server with `not-server pair`, or on a paired device under Settings → Add device.",
        style = Not.type.caption,
        color = colors.inkMuted,
    )
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Button("Pair", { viewModel.pair(url, code, name) }, kind = ButtonKind.Secondary, enabled = !state.busy && url.isNotBlank() && code.isNotBlank())
        if (state.busy) Spinner()
    }

    if (scanning) {
        QrScanner(
            onScan = {
                scanning = false
                scanError = !viewModel.pairWithLink(it)
            },
            onCancel = { scanning = false },
        )
    }
}

@Composable
private fun PairedInfo(state: UiState, viewModel: TasksViewModel) {
    val info = state.syncInfo ?: return
    val colors = Not.colors
    var pendingRevoke by remember { mutableStateOf<Device?>(null) }
    var confirmingUnpair by remember { mutableStateOf(false) }
    // The server no longer knows this device (revoked, or its database was reset): only unpairing helps.
    val rejected = state.sync == SyncState.Unauthorized
    LaunchedEffect(info.deviceId, rejected) { if (!rejected) viewModel.refreshDevices() }

    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(info.serverUrl, style = Not.type.bodyStrong, maxLines = 1)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.size(7.dp).clip(RoundedCornerShape(50)).background(statusColor(state.sync)))
                Text("${info.deviceName} · ${syncStatus(state.sync)}", style = Not.type.caption, color = colors.inkMuted)
            }
        }
        // Lives here, not in the device list, which can't load once the server rejects us.
        Button("Unpair", { confirmingUnpair = true }, kind = ButtonKind.Danger, enabled = !state.busy)
    }

    if (confirmingUnpair) {
        CardDialog(
            title = "Unpair this device?",
            onDismiss = { confirmingUnpair = false },
            buttons = {
                Button("Cancel", { confirmingUnpair = false }, kind = ButtonKind.Quiet)
                Button("Unpair", {
                    viewModel.unpair()
                    confirmingUnpair = false
                }, kind = ButtonKind.Danger)
            },
        ) {
            Text(
                if (rejected) "Tasks stay here. Pair again to resume syncing."
                else "Sync stops and this device is removed from the server. Tasks stay here.",
                color = colors.inkMuted,
            )
        }
    }
    if (rejected) return
    Divider()

    val code = state.pairingCode
    if (code == null) {
        Button("Add device", viewModel::createPairingCode, kind = ButtonKind.Secondary, icon = R.drawable.ic_add, enabled = !state.busy)
    } else {
        val qr = remember(info.serverUrl, code.code) { qrBitmap(pairingLink(info.serverUrl, code.code)) }
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            // QR codes need a light background to scan, in dark mode too.
            Box(Modifier.clip(RoundedCornerShape(20.dp)).background(Color.White).padding(12.dp)) {
                Image(bitmap = qr, contentDescription = "Pairing QR code", filterQuality = FilterQuality.None, modifier = Modifier.size(184.dp))
            }
            Text(code.code, style = Not.type.title.merge(fontFamily = FontFamily.Monospace))
            Text(
                "Scan it with the new device, or enter the code there with this server's URL. Works once, for 10 minutes.",
                style = Not.type.caption,
                color = colors.inkMuted,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
        }
    }
    Divider()

    Text("Devices", style = Not.type.label, color = colors.inkMuted)
    state.devices.forEach { device ->
        Row(Modifier.heightIn(min = 44.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(if (device.isCurrent) "${device.name} (this device)" else device.name, style = Not.type.body)
                Text(device.lastSeenAt?.let { "Last seen ${ago(it)}" } ?: "Never synced", style = Not.type.caption, color = colors.inkMuted)
            }
            if (!device.isCurrent) Button("Revoke", { pendingRevoke = device }, kind = ButtonKind.Quiet, enabled = !state.busy)
        }
    }

    // Revoking can't be undone (the device must pair again), so it still asks.
    pendingRevoke?.let { device ->
        CardDialog(
            title = "Revoke ${device.name}?",
            onDismiss = { pendingRevoke = null },
            buttons = {
                Button("Cancel", { pendingRevoke = null }, kind = ButtonKind.Quiet)
                Button("Revoke", {
                    viewModel.revoke(device)
                    pendingRevoke = null
                }, kind = ButtonKind.Danger)
            },
        ) {
            Text("${device.name} stops syncing until it is paired again. Its local data stays on it.", color = colors.inkMuted)
        }
    }
}

@Composable
private fun statusColor(state: SyncState?): Color = when (state) {
    SyncState.Synced -> Not.colors.accent
    SyncState.Syncing, null -> Not.colors.inkFaint
    is SyncState.Offline -> Color(0xFFF0A030)
    SyncState.Unauthorized -> Not.colors.danger
}

private fun ago(millis: Long): CharSequence =
    if (System.currentTimeMillis() - millis < DateUtils.MINUTE_IN_MILLIS) "just now"
    else DateUtils.getRelativeTimeSpanString(millis)

private fun syncStatus(state: SyncState?): String = when (state) {
    null -> "starting…"
    SyncState.Syncing -> "syncing…"
    SyncState.Synced -> "up to date"
    is SyncState.Offline -> "offline: ${state.reason}"
    SyncState.Unauthorized -> "rejected by the server; unpair, then pair again"
}
