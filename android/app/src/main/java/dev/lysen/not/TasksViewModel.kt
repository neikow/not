package dev.lysen.not

import android.app.Application
import android.os.Build
import android.provider.Settings
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.Device
import dev.lysen.not.core.NotException
import dev.lysen.not.core.PairingCode
import dev.lysen.not.core.StoreObserver
import dev.lysen.not.core.SyncInfo
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.TaskItem
import dev.lysen.not.core.TaskList
import dev.lysen.not.core.inboxId
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** A task plus its nesting depth, in display order. */
data class TaskRow(val task: TaskItem, val depth: Int)

data class UiState(
    val lists: List<TaskList> = emptyList(),
    val selectedListId: String = INBOX_ID,
    val rows: List<TaskRow> = emptyList(),
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
}

val INBOX_ID: String = inboxId()

class TasksViewModel(app: Application) : AndroidViewModel(app) {
    private val store = (app as NotApp).store
    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    /** The name from Settings → About phone, e.g. "Galaxy S24". */
    val defaultDeviceName: String =
        Settings.Global.getString(app.contentResolver, Settings.Global.DEVICE_NAME) ?: Build.MODEL

    // Called from Rust threads.
    private val observer = object : StoreObserver {
        override fun onChange() {
            viewModelScope.launch { reload() }
        }

        override fun onSyncState(state: SyncState) {
            _state.update { it.copy(sync = state) }
        }
    }

    init {
        // Before pairing, the shared server token was kept here in plain text.
        app.deleteSharedPreferences("sync")
        store.setObserver(observer)
        reload()
        _state.update { it.copy(syncInfo = run { store.syncInfo() }) }
        run { store.startSync() }
    }

    fun syncNow() = store.syncNow()

    // ---- pairing ----

    fun pair(serverUrl: String, code: String, deviceName: String) = launchBusy {
        store.pair(serverUrl.trim(), code, deviceName).let { info -> _state.update { it.copy(syncInfo = info) } }
        loadDevices()
    }

    fun unpair() = launchBusy {
        try {
            store.unpair()
        } finally {
            _state.update { it.copy(syncInfo = null, sync = null, devices = emptyList(), pairingCode = null) }
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

    // ---- tasks ----

    fun select(listId: String) {
        _state.update { it.copy(selectedListId = listId) }
        reloadTasks()
    }

    fun addList(title: String) {
        run { store.createList(title) }?.let(::select)
    }

    fun addTask(title: String) {
        run { store.addTask(_state.value.selectedListId, title) }
    }

    fun toggle(task: TaskItem) {
        run { store.setCompleted(task.id, task.completedAt == null) }
    }

    fun delete(id: String) {
        run { store.deleteItem(id) }
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    private fun reload() {
        val lists = run { store.lists() } ?: return
        _state.update { s ->
            val selected = s.selectedListId.takeIf { id -> lists.any { it.id == id } } ?: INBOX_ID
            s.copy(lists = lists, selectedListId = selected)
        }
        reloadTasks()
    }

    private fun reloadTasks() {
        val tasks = run { store.tasks(_state.value.selectedListId) } ?: return
        _state.update { it.copy(rows = flatten(tasks)) }
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

    companion object {
        /** Depth-first order: each task followed by its subtasks. */
        fun flatten(tasks: List<TaskItem>): List<TaskRow> {
            val children = tasks.groupBy { it.parentId }
            val rows = mutableListOf<TaskRow>()
            fun visit(parent: String?, depth: Int) {
                children[parent].orEmpty().forEach {
                    rows += TaskRow(it, depth)
                    visit(it.id, depth + 1)
                }
            }
            visit(null, 0)
            return rows
        }
    }
}

private val NotException.userMessage: String
    get() = when (this) {
        is NotException.NotFound -> detail
        is NotException.Invalid -> detail
        is NotException.Internal -> detail
    }
