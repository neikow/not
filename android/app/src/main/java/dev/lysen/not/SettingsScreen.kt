package dev.lysen.not

import android.text.format.DateUtils
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Device
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.pairingLink
import dev.lysen.not.pairing.QrScanner
import dev.lysen.not.pairing.qrBitmap
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(viewModel: TasksViewModel, onBack: () -> Unit) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(painterResource(R.drawable.ic_arrow_back), "Back") }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp)
                .navigationBarsPadding(),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            SectionTitle("Sync")
            SettingsCard {
                if (state.syncInfo == null) PairForm(state, viewModel) else PairedInfo(state, viewModel)
            }

            SectionTitle("Shortcuts")
            SettingsCard(padded = false) {
                ShortcutRow("Add widget to home screen", "Open tasks of a list, with checkboxes and quick add") {
                    scope.launch { Shortcuts.requestWidget(context) }
                }
                if (Shortcuts.canRequestTile) {
                    HorizontalDivider()
                    ShortcutRow("Add Quick Settings tile", "Add a task from the notification shade") {
                        Shortcuts.requestTile(context)
                    }
                }
            }

            SectionTitle("About")
            Text(
                "not ${BuildConfig.VERSION_NAME}",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 4.dp, vertical = 8.dp),
            )
        }
    }

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
private fun SectionTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 4.dp, top = 16.dp, bottom = 4.dp),
    )
}

@Composable
private fun SettingsCard(padded: Boolean = true, content: @Composable ColumnScope.() -> Unit) {
    Surface(
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surfaceContainer,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(
            Modifier.padding(if (padded) 16.dp else 0.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            content = content,
        )
    }
}

@Composable
private fun ShortcutRow(title: String, subtitle: String, onClick: () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge)
        Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun PairForm(state: UiState, viewModel: TasksViewModel) {
    var url by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    var name by remember { mutableStateOf(viewModel.defaultDeviceName) }
    var scanning by remember { mutableStateOf(false) }
    var scanError by remember { mutableStateOf(false) }

    Text("Pair this device with your server to sync.", style = MaterialTheme.typography.bodyMedium)
    Button(onClick = { scanning = true }, enabled = !state.busy) { Text("Scan QR code") }
    if (scanError) {
        Text("That QR code isn't a pairing code.", color = MaterialTheme.colorScheme.error)
    }
    Text(
        "Or enter the server and code:",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    OutlinedTextField(url, { url = it }, label = { Text("Server URL") }, singleLine = true, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(
        code,
        { code = it },
        label = { Text("Pairing code") },
        placeholder = { Text("XXXXX-XXXXX") },
        singleLine = true,
        textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = FontFamily.Monospace),
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedTextField(name, { name = it }, label = { Text("Device name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
    Text(
        "Get a code on the server with `not-server pair`, or on a paired device under Settings → Add device.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Row(verticalAlignment = Alignment.CenterVertically) {
        OutlinedButton(
            onClick = { viewModel.pair(url, code, name) },
            enabled = !state.busy && url.isNotBlank() && code.isNotBlank(),
        ) { Text("Pair") }
        if (state.busy) {
            Spacer(Modifier.width(12.dp))
            CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
        }
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
    var pendingRevoke by remember { mutableStateOf<Device?>(null) }
    var confirmingUnpair by remember { mutableStateOf(false) }
    // The server no longer knows this device (revoked, or its database was reset): only unpairing helps.
    val rejected = state.sync == SyncState.Unauthorized
    LaunchedEffect(info.deviceId, rejected) { if (!rejected) viewModel.refreshDevices() }

    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(info.serverUrl, style = MaterialTheme.typography.bodyLarge)
            Text(
                "${info.deviceName} · ${syncStatus(state.sync)}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        // Lives here, not in the device list, which can't load once the server rejects us.
        TextButton(onClick = { confirmingUnpair = true }, enabled = !state.busy) {
            Text("Unpair", color = MaterialTheme.colorScheme.error)
        }
    }

    if (confirmingUnpair) {
        AlertDialog(
            onDismissRequest = { confirmingUnpair = false },
            title = { Text("Unpair this device?") },
            text = {
                Text(
                    if (rejected) "Tasks stay here. Pair again to resume syncing."
                    else "Sync stops and this device is removed from the server. Tasks stay here.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    viewModel.unpair()
                    confirmingUnpair = false
                }) { Text("Unpair", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirmingUnpair = false }) { Text("Cancel") } },
        )
    }
    if (rejected) return
    HorizontalDivider()

    val code = state.pairingCode
    if (code == null) {
        TextButton(onClick = viewModel::createPairingCode, enabled = !state.busy) { Text("Add device") }
    } else {
        val qr = remember(info.serverUrl, code.code) { qrBitmap(pairingLink(info.serverUrl, code.code)) }
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            Image(
                bitmap = qr,
                contentDescription = "Pairing QR code",
                filterQuality = FilterQuality.None,
                modifier = Modifier.size(200.dp),
            )
        }
        Text(code.code, style = MaterialTheme.typography.headlineSmall.copy(fontFamily = FontFamily.Monospace))
        Text(
            "Scan it with the new device, or enter the code there with this server's URL. Works once, for 10 minutes.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    HorizontalDivider()

    Text("Devices", style = MaterialTheme.typography.titleSmall)
    state.devices.forEach { device ->
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(if (device.isCurrent) "${device.name} (this device)" else device.name)
                Text(
                    device.lastSeenAt?.let { "Last seen ${ago(it)}" } ?: "Never synced",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (!device.isCurrent) {
                TextButton(onClick = { pendingRevoke = device }, enabled = !state.busy) {
                    Text("Revoke", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }

    // Revoking can't be undone (the device must pair again), so it still asks.
    pendingRevoke?.let { device ->
        AlertDialog(
            onDismissRequest = { pendingRevoke = null },
            title = { Text("Revoke ${device.name}?") },
            text = { Text("${device.name} stops syncing until it is paired again. Its local data stays on it.") },
            confirmButton = {
                TextButton(onClick = {
                    viewModel.revoke(device)
                    pendingRevoke = null
                }) { Text("Revoke", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { pendingRevoke = null }) { Text("Cancel") } },
        )
    }
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
