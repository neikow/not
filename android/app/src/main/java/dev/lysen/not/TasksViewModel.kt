package dev.lysen.not

import android.app.Application
import android.os.Build
import android.provider.Settings
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.Device
import dev.lysen.not.core.NotException
import dev.lysen.not.core.Outline
import dev.lysen.not.core.PairingCode
import dev.lysen.not.core.SyncInfo
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.TaskItem
import dev.lysen.not.core.TaskList
import dev.lysen.not.core.inboxId
import dev.lysen.not.core.parsePairingLink
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class UiState(
    val lists: List<TaskList> = emptyList(),
    val selectedListId: String = INBOX_ID,
    val outline: Outline = Outline(emptyList(), emptyList()),
    /** The task open in the detail sheet. */
    val openTaskId: String? = null,
    val sync: SyncState? = null,
    /** Null until this device is paired with a server. */
    val syncInfo: SyncInfo? = null,
    val devices: List<Device> = emptyList(),
    val pairingCode: PairingCode? = null,
    /** A pairing or device call is in flight. */
    val busy: Boolean = false,
    val error: String? = null,
) {
    val selectedList: TaskList? get() = lists.firstOrNull { it.id == selectedListId }
    private val allRows get() = outline.open + outline.completed
    val openTask: TaskItem? get() = allRows.firstOrNull { it.task.id == openTaskId }?.task

    fun subtasks(of: String): List<TaskItem> = allRows.map { it.task }.filter { it.parentId == of }
}

/** Something to confirm in a snackbar, with Undo. */
data class Undoable(val message: String)

val INBOX_ID: String = inboxId()

class TasksViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as NotApp
    private val store = app.store
    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    private val _undoables = Channel<Undoable>(Channel.CONFLATED)
    val undoables: Flow<Undoable> = _undoables.receiveAsFlow()

    /** The name from Settings → About phone, e.g. "Galaxy S24". */
    val defaultDeviceName: String =
        Settings.Global.getString(app.contentResolver, Settings.Global.DEVICE_NAME) ?: Build.MODEL

    init {
        // Before pairing, the shared server token was kept here in plain text.
        app.deleteSharedPreferences("sync")
        _state.update { it.copy(syncInfo = run { store.syncInfo() }) }
        viewModelScope.launch { app.changes.collect { reload() } }
        viewModelScope.launch { app.syncState.collect { sync -> _state.update { it.copy(sync = sync) } } }
    }

    fun syncNow() = store.syncNow()

    // ---- lists ----

    fun select(listId: String) {
        _state.update { it.copy(selectedListId = listId, openTaskId = null) }
        reloadTasks()
    }

    fun addList(title: String) {
        run { store.createList(title) }?.let(::select)
    }

    fun rename(id: String, title: String) {
        if (title.isNotBlank()) run { store.rename(id, title.trim()) }
    }

    fun deleteList(list: TaskList) {
        run { store.deleteItem(list.id) } ?: return
        _undoables.trySend(Undoable("“${list.title}” deleted"))
    }

    // ---- tasks ----

    fun addTask(title: String, parentId: String = _state.value.selectedListId) {
        if (title.isNotBlank()) run { store.addTask(parentId, title) }
    }

    fun toggle(task: TaskItem) {
        run { store.setCompleted(task.id, task.completedAt == null) }
    }

    fun delete(task: TaskItem) {
        _state.update { if (it.openTaskId == task.id) it.copy(openTaskId = null) else it }
        run { store.deleteItem(task.id) } ?: return
        _undoables.trySend(Undoable("Task deleted"))
    }

    fun setNotes(id: String, notes: String) {
        run { store.setNotes(id, notes) }
    }

    /** Moves a task (with its subtasks) to the end of another list. */
    fun move(task: TaskItem, toList: String) {
        run { store.moveTask(task.id, toList, UInt.MAX_VALUE) }
    }

    fun openTask(id: String?) = _state.update { it.copy(openTaskId = id) }

    fun undo() {
        run { store.undo() }
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    // ---- pairing ----

    fun pair(serverUrl: String, code: String, deviceName: String) = launchBusy {
        store.pair(serverUrl.trim(), code, deviceName).let { info -> _state.update { it.copy(syncInfo = info) } }
        loadDevices()
    }

    /** Pairs with the server and code in a scanned QR code. False if it isn't a pairing code. */
    fun pairWithLink(scanned: String): Boolean {
        val link = parsePairingLink(scanned) ?: return false
        pair(link.serverUrl, link.code, defaultDeviceName)
        return true
    }

    fun unpair() = launchBusy {
        try {
            store.unpair()
        } finally {
            app.clearSyncState()
            _state.update { it.copy(syncInfo = null, devices = emptyList(), pairingCode = null) }
        }
    }

    fun createPairingCode() = launchBusy {
        val code = store.createPairingCode()
        _state.update { it.copy(pairingCode = code) }
    }

    fun refreshDevices() = launchBusy { loadDevices() }

    fun revoke(device: Device) {
        if (device.isCurrent) return unpair()
        launchBusy {
            store.revokeDevice(device.id)
            loadDevices()
        }
    }

    private suspend fun loadDevices() {
        val devices = store.devices()
        _state.update { it.copy(devices = devices) }
    }

    // ---- reading ----

    private fun reload() {
        val lists = run { store.lists() } ?: return
        _state.update { s ->
            val selected = s.selectedListId.takeIf { id -> lists.any { it.id == id } } ?: INBOX_ID
            s.copy(lists = lists, selectedListId = selected)
        }
        reloadTasks()
    }

    private fun reloadTasks() {
        val outline = run { store.outline(_state.value.selectedListId) } ?: return
        _state.update { s ->
            // The open task may have been deleted or moved on another device.
            val stillThere = (outline.open + outline.completed).any { it.task.id == s.openTaskId }
            s.copy(outline = outline, openTaskId = s.openTaskId.takeIf { stillThere })
        }
    }

    private fun <T> run(body: () -> T): T? =
        try {
            body()
        } catch (e: NotException) {
            _state.update { it.copy(error = e.userMessage) }
            null
        }

    private fun launchBusy(body: suspend () -> Unit) {
        _state.update { it.copy(busy = true) }
        viewModelScope.launch {
            try {
                body()
            } catch (e: NotException) {
                _state.update { it.copy(error = e.userMessage) }
            } finally {
                _state.update { it.copy(busy = false) }
            }
        }
    }
}

internal val NotException.userMessage: String
    get() = when (this) {
        is NotException.NotFound -> detail
        is NotException.Invalid -> detail
        is NotException.Internal -> detail
    }
