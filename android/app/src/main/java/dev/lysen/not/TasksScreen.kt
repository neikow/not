package dev.lysen.not

import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.lysen.not.core.SyncState

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
        SettingsDialog(
            serverUrl = state.serverUrl,
            onConnect = { url, token ->
                viewModel.connect(url, token)
                showSettings = false
            },
            onDismiss = { showSettings = false },
        )
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
    }
    Box(Modifier.size(10.dp).background(color, CircleShape))
}

@Composable
private fun SettingsDialog(serverUrl: String, onConnect: (String, String) -> Unit, onDismiss: () -> Unit) {
    var url by remember { mutableStateOf(serverUrl) }
    var token by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Sync server") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(url, { url = it }, label = { Text("URL") }, singleLine = true)
                OutlinedTextField(
                    token,
                    { token = it },
                    label = { Text("Token") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                )
            }
        },
        confirmButton = { TextButton(onClick = { onConnect(url, token) }) { Text("Connect") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
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
