package dev.lysen.not

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import android.text.format.DateUtils
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.Device
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.pairingLink
import dev.lysen.not.pairing.QrScanner
import dev.lysen.not.pairing.qrBitmap

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TasksScreen(viewModel: TasksViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var showSettings by remember { mutableStateOf(false) }
    var showNewList by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    ListPicker(
                        state = state,
                        onSelect = viewModel::select,
                        onNewList = { showNewList = true },
                    )
                },
                actions = {
                    SyncDot(state.sync)
                    TextButton(onClick = { showSettings = true }) { Text("Sync") }
                },
            )
        },
        bottomBar = { AddTaskBar(onAdd = viewModel::addTask) },
    ) { padding ->
        if (state.rows.isEmpty()) {
            Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) {
                Text("No tasks", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        } else {
            LazyColumn(Modifier.fillMaxSize().padding(padding)) {
                items(state.rows, key = { it.task.id }) { row ->
                    TaskRowItem(row, onToggle = { viewModel.toggle(row.task) }, onDelete = { viewModel.delete(row.task.id) })
                    HorizontalDivider()
                }
            }
        }
    }

    if (showSettings) {
        SyncDialog(state, viewModel, onDismiss = { showSettings = false })
    }
    if (showNewList) {
        TextPromptDialog(
            title = "New list",
            onConfirm = {
                viewModel.addList(it)
                showNewList = false
            },
            onDismiss = { showNewList = false },
        )
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
private fun ListPicker(state: UiState, onSelect: (String) -> Unit, onNewList: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        TextButton(onClick = { open = true }) {
            Text(state.selectedList?.title ?: "not", style = MaterialTheme.typography.titleLarge)
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            state.lists.forEach { list ->
                DropdownMenuItem(text = { Text(list.title) }, onClick = {
                    onSelect(list.id)
                    open = false
                })
            }
            DropdownMenuItem(text = { Text("New list…") }, onClick = {
                onNewList()
                open = false
            })
        }
    }
}

@Composable
private fun TaskRowItem(row: TaskRow, onToggle: () -> Unit, onDelete: () -> Unit) {
    val done = row.task.completedAt != null
    Row(
        Modifier
            .fillMaxWidth()
            .combinedClickable(onClick = onToggle, onLongClick = onDelete)
            .padding(start = (8 + row.depth * 24).dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = done, onCheckedChange = { onToggle() })
        Text(
            row.task.title,
            style = MaterialTheme.typography.bodyLarge,
            textDecoration = if (done) TextDecoration.LineThrough else null,
            color = if (done) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
        )
    }
}

@Composable
private fun AddTaskBar(onAdd: (String) -> Unit) {
    var draft by remember { mutableStateOf("") }
    Surface(tonalElevation = 3.dp) {
        OutlinedTextField(
            value = draft,
            onValueChange = { draft = it },
            placeholder = { Text("Add a task") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = {
                if (draft.isNotBlank()) onAdd(draft)
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
private fun SyncDot(state: SyncState?) {
    val color = when (state) {
        null -> Color.Gray
        SyncState.Syncing -> Color(0xFF2196F3)
        SyncState.Synced -> Color(0xFF4CAF50)
        is SyncState.Offline -> Color(0xFFFF9800)
        SyncState.Unauthorized -> Color(0xFFF44336)
    }
    Box(Modifier.size(10.dp).background(color, CircleShape))
}

@Composable
private fun SyncDialog(state: UiState, viewModel: TasksViewModel, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (state.syncInfo == null) "Pair this device" else "Sync") },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (state.syncInfo == null) PairForm(state, viewModel) else PairedInfo(state, viewModel)
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}

@Composable
private fun PairForm(state: UiState, viewModel: TasksViewModel) {
    var url by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    var name by remember { mutableStateOf(viewModel.defaultDeviceName) }
    var scanning by remember { mutableStateOf(false) }
    var scanError by remember { mutableStateOf(false) }

    OutlinedButton(onClick = { scanning = true }, enabled = !state.busy) { Text("Scan QR code") }
    if (scanError) {
        Text("That QR code isn't a pairing code.", color = MaterialTheme.colorScheme.error)
    }
    Text(
        "Or enter the server and code:",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    OutlinedTextField(url, { url = it }, label = { Text("Server URL") }, singleLine = true)
    OutlinedTextField(
        code,
        { code = it },
        label = { Text("Pairing code") },
        placeholder = { Text("XXXXX-XXXXX") },
        singleLine = true,
        textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = FontFamily.Monospace),
    )
    OutlinedTextField(name, { name = it }, label = { Text("Device name") }, singleLine = true)
    Text(
        "Get a code on the server with `not-server pair`, or on a paired device under Sync → Add device.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Row(verticalAlignment = Alignment.CenterVertically) {
        Button(
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
    LaunchedEffect(info.deviceId) { viewModel.refreshDevices() }

    Text(info.serverUrl, style = MaterialTheme.typography.bodyLarge)
    Text(
        "${info.deviceName} · ${syncStatus(state.sync)}",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
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
            TextButton(onClick = { pendingRevoke = device }, enabled = !state.busy) {
                Text(if (device.isCurrent) "Unpair" else "Revoke", color = MaterialTheme.colorScheme.error)
            }
        }
    }

    pendingRevoke?.let { device ->
        AlertDialog(
            onDismissRequest = { pendingRevoke = null },
            title = { Text(if (device.isCurrent) "Unpair this device?" else "Revoke ${device.name}?") },
            text = {
                Text(
                    if (device.isCurrent) "Sync stops and this device is removed from the server. Tasks stay here."
                    else "${device.name} stops syncing until it is paired again. Its local data stays on it.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    viewModel.revoke(device)
                    pendingRevoke = null
                }) { Text(if (device.isCurrent) "Unpair" else "Revoke", color = MaterialTheme.colorScheme.error) }
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

@Composable
private fun TextPromptDialog(title: String, onConfirm: (String) -> Unit, onDismiss: () -> Unit) {
    var text by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { OutlinedTextField(text, { text = it }, singleLine = true) },
        confirmButton = { TextButton(onClick = { if (text.isNotBlank()) onConfirm(text) }) { Text("Create") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
