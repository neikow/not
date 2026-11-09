package dev.lysen.not

import android.text.format.DateUtils
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

@Composable
fun SettingsScreen(viewModel: TasksViewModel, onBack: () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    LargeTitleScreen(
        title = "Settings",
        navigation = { IconButton(R.drawable.ic_arrow_back, "Back", onBack) },
        bottomPadding = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding(),
    ) {
        item(key = "sync") {
            Section("Sync") {
                if (state.syncInfo == null) PairForm(state, viewModel) else PairedInfo(state, viewModel)
            }
        }
        item(key = "shortcuts") {
            Section("Widgets and shortcuts", padded = false) {
                ShortcutRow(R.drawable.ic_task_alt, "Tasks widget", "Open tasks of a list, with checkboxes and quick add") {
                    scope.launch { Shortcuts.requestWidget(context, TasksWidgetReceiver::class.java) }
                }
                Divider(Modifier.padding(start = 56.dp))
                ShortcutRow(R.drawable.ic_repeat, "Habits widget", "Today's habits; tap one to check in") {
                    scope.launch { Shortcuts.requestWidget(context, HabitsWidgetReceiver::class.java) }
                }
                Divider(Modifier.padding(start = 56.dp))
                ShortcutRow(R.drawable.ic_note, "Notes widget", "Pinned and recent notes of a folder") {
                    scope.launch { Shortcuts.requestWidget(context, NotesWidgetReceiver::class.java) }
                }
                Divider(Modifier.padding(start = 56.dp))
                ShortcutRow(R.drawable.ic_checklist, "Note widget", "One note, with tickable checklists") {
                    scope.launch { Shortcuts.requestWidget(context, NoteWidgetReceiver::class.java) }
                }
                if (Shortcuts.canRequestTile) {
                    Divider(Modifier.padding(start = 56.dp))
                    ShortcutRow(R.drawable.ic_add_task, "Quick Settings tile", "Add a task from the notification shade") {
                        Shortcuts.requestTile(context)
                    }
                }
            }
        }
        item(key = "about") {
            Text(
                "not ${BuildConfig.VERSION_NAME}\nExercise data from wger.de by its contributors, CC-BY-SA 4.0 / 3.0 or CC0",
                style = Not.type.caption,
                color = Not.colors.inkFaint,
                modifier = Modifier.fillMaxWidth().padding(vertical = 24.dp),
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
        }
    }

    state.error?.let {
        CardDialog(
            title = "Something went wrong",
            onDismiss = viewModel::dismissError,
            buttons = { Button("OK", viewModel::dismissError) },
        ) { Text(it, color = Not.colors.inkMuted) }
    }
}

@Composable
private fun Section(title: String, padded: Boolean = true, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.padding(horizontal = 16.dp).padding(top = 20.dp)) {
        Text(title, style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(start = 8.dp, bottom = 8.dp))
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
