package dev.lysen.not

import android.app.Application
import android.os.Build
import android.provider.Settings
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.Device
import dev.lysen.not.core.NotException
import dev.lysen.not.core.Outline
import dev.lysen.not.core.OutlineRow
import dev.lysen.not.core.PairingCode
import dev.lysen.not.core.SyncInfo
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.TaskItem
import dev.lysen.not.core.TaskList
import dev.lysen.not.core.TaskSort
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
    /** Open tasks due today or earlier, for the Today entry's badge. */
    val dueCount: Int = 0,
    val upcomingCount: Int = 0,
    /** Every task of the open task's list, for its subtasks (Today only shows due ones). */
    val openTaskList: List<TaskItem> = emptyList(),
    /** Tasks completed over a day ago, from every list, latest first; Settings → Archive. */
    val archive: List<TaskItem> = emptyList(),
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
    val isToday: Boolean get() = selectedListId == TODAY_ID
    val isUpcoming: Boolean get() = selectedListId == UPCOMING_ID
    /** Today or Upcoming: tasks gathered from every list by date, so no order or sort of their own. */
    val isDateView: Boolean get() = isToday || isUpcoming
    val title: String get() = when {
        isToday -> "Today"
        isUpcoming -> "Upcoming"
        else -> selectedList?.title ?: "Inbox"
    }
    private val allRows get() = outline.open + outline.completed
    val openTask: TaskItem? get() = allRows.firstOrNull { it.task.id == openTaskId }?.task

    fun subtasks(of: String): List<TaskItem> = openTaskList.filter { it.parentId == of }
    fun listTitle(id: String): String = lists.firstOrNull { it.id == id }?.title ?: "Inbox"
}

/** Something to confirm in a snackbar, with Undo. */
data class Undoable(val message: String)

val INBOX_ID: String = inboxId()

/** The Today view: not a list in the core, but every open task due today or earlier. */
const val TODAY_ID = "today"

/** Every open task due after today, grouped by day. */
const val UPCOMING_ID = "upcoming"

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

    fun addTask(title: String, parentId: String) {
        if (title.isNotBlank()) run { store.addTask(parentId, title) }
    }

    /** Adds what was typed in the composer, dates and tags parsed out. From Today, it's due today. */
    fun quickAdd(text: String, dismissed: List<String> = emptyList()) {
        if (text.isBlank()) return
        val s = _state.value
        val parent = if (s.isDateView) INBOX_ID else s.selectedListId
        val due = when {
            s.isToday -> today()
            s.isUpcoming -> java.time.LocalDate.now().plusDays(1).toString()
            else -> null
        }
        run { store.quickAdd(parent, text, today(), dismissed, due) }
    }

    /** Completing confirms with Undo, so a stray tap or swipe costs nothing. */
    fun toggle(task: TaskItem) {
        val completing = task.completedAt == null
        run { store.setCompleted(task.id, completing, today()) } ?: return
        if (completing) _undoables.trySend(Undoable("“${task.title}” done"))
    }

    /** Puts an archived task back in its list, open. */
    fun restore(task: TaskItem) {
        run { store.setCompleted(task.id, false, today()) }
    }

    fun deleteArchived(task: TaskItem) {
        run { store.deleteItem(task.id) }
    }

    /** Returns how many were deleted, for the Undo toast. */
    fun clearArchive(): Int = run { store.clearArchive() }?.toInt() ?: 0

    fun setSort(listId: String, sort: TaskSort) {
        run { store.setListSort(listId, sort) }
    }

    /** Takes the date, time and repeat out of `text` ("fri 9am every week"); blank clears them. */
    fun setSchedule(id: String, text: String, dismissed: List<String> = emptyList()) {
        if (text.isBlank()) {
            run { store.setSchedule(id, null, null, null) }
            return
        }
        val parsed = run { store.parseTask(text, today(), dismissed) } ?: return
        run { store.setSchedule(id, parsed.due, parsed.dueTime, parsed.repeat) }
    }

    fun setPriority(id: String, priority: UByte) {
        run { store.setPriority(id, priority) }
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

    /** Drag and drop: [task] goes right before its sibling [beforeId], or last among them. */
    fun moveBefore(task: TaskItem, beforeId: String?) {
        run { store.moveTaskBefore(task.id, task.parentId ?: task.listId, beforeId) }
    }

    fun openTask(id: String?) {
        _state.update { it.copy(openTaskId = id) }
        reloadTasks()
    }

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
            val selected = s.selectedListId.takeIf { id -> id == TODAY_ID || id == UPCOMING_ID || lists.any { it.id == id } } ?: INBOX_ID
            s.copy(lists = lists, selectedListId = selected)
        }
        reloadTasks()
    }

    private fun reloadTasks() {
        val due = run { store.dueTasks(today()) } ?: return
        val archive = run { store.archive() }.orEmpty()
        val upcoming = run { store.upcomingTasks(today()) }.orEmpty()
        val outline = if (_state.value.isToday) {
            Outline(due.map { OutlineRow(it, 0u, false) }, emptyList())
        } else if (_state.value.isUpcoming) {
            Outline(upcoming.map { OutlineRow(it, 0u, false) }, emptyList())
        } else {
            run { store.outline(_state.value.selectedListId) } ?: return
        }
        _state.update { s ->
            // The open task may have been deleted or moved on another device.
            val open = (outline.open + outline.completed).firstOrNull { it.task.id == s.openTaskId }?.task
            s.copy(
                outline = outline,
                archive = archive,
                dueCount = due.size,
                upcomingCount = upcoming.size,
                openTaskId = open?.id,
                openTaskList = open?.let { run { store.tasks(it.listId) } }.orEmpty(),
            )
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
