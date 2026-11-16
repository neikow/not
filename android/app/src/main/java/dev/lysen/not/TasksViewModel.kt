package dev.lysen.not

import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.distinctUntilChanged
import android.app.Application
import android.os.Build
import android.provider.Settings
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.Device
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.ListShare
import dev.lysen.not.core.NotException
import dev.lysen.not.core.Outline
import dev.lysen.not.core.PairingCode
import dev.lysen.not.core.SyncInfo
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.Snooze
import dev.lysen.not.core.TagInfo
import dev.lysen.not.core.TaskItem
import dev.lysen.not.core.TaskSort
import dev.lysen.not.core.TaskView
import dev.lysen.not.core.inboxId
import dev.lysen.not.core.normalizeTag
import dev.lysen.not.core.somedayViewId
import dev.lysen.not.core.todayViewId
import dev.lysen.not.core.upcomingViewId
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
    /** Built-in views, then the user's smart lists. */
    val views: List<TaskView> = emptyList(),
    val tags: List<TagInfo> = emptyList(),
    /** A view id, or a `#tag` being browsed. */
    val selectedViewId: String = INBOX_ID,
    val outline: Outline = Outline(emptyList(), emptyList()),
    /** Today only: the habits on today's to-do, done ones included, in the user's order. */
    val todayHabits: List<HabitSummary> = emptyList(),
    /** Tasks completed over a day ago, latest first; Settings → Archive. */
    val archive: List<TaskItem> = emptyList(),
    /** The task open in the detail sheet, and its subtasks. */
    val openTask: TaskItem? = null,
    val openSubtasks: List<TaskItem> = emptyList(),
    val sync: SyncState? = null,
    /** Null until this device is paired with a server. */
    val syncInfo: SyncInfo? = null,
    val devices: List<Device> = emptyList(),
    val pairingCode: PairingCode? = null,
    /** A pairing or device call is in flight. */
    val busy: Boolean = false,
    val error: String? = null,
) {
    val selectedView: TaskView? get() = views.firstOrNull { it.id == selectedViewId }
    val isToday: Boolean get() = selectedViewId == TODAY_ID
    val isUpcoming: Boolean get() = selectedViewId == UPCOMING_ID
    val isInbox: Boolean get() = selectedViewId == INBOX_ID
    val isSomeday: Boolean get() = selectedViewId == SOMEDAY_ID
    /** Browsing a tag that has no saved view. */
    val isTag: Boolean get() = selectedViewId.startsWith("#")
    val title: String get() = selectedView?.title ?: selectedViewId
    val sort: TaskSort get() = selectedView?.sort ?: TaskSort.MANUAL

    /** Today's open tasks planned for an earlier day: what "Move to today" picks up. */
    val earlier: List<TaskItem>
        get() = if (!isToday) emptyList() else outline.open.filter { it.depth == 0u && it.task.start?.let { d -> d < today() } == true }.map { it.task }

    fun count(viewId: String): Int = views.firstOrNull { it.id == viewId }?.openCount?.toInt() ?: 0
}

/** Something to confirm in a snackbar, with Undo. */
data class Undoable(val message: String)

val INBOX_ID: String = inboxId()
val TODAY_ID: String = todayViewId()
val UPCOMING_ID: String = upcomingViewId()
val SOMEDAY_ID: String = somedayViewId()

/** Minutes after midnight now, for "in 2 hours" and snoozing. */
fun nowMinute(): UInt = java.time.LocalTime.now().let { (it.hour * 60 + it.minute).toUInt() }

class TasksViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as NotApp
    private val store = app.store
    private val memory = app.screenMemory
    private val _state = MutableStateFlow(UiState(selectedViewId = memory.list ?: INBOX_ID))
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
        viewModelScope.launch { state.map { it.selectedViewId }.distinctUntilChanged().collect { memory.list = it } }
        viewModelScope.launch { app.syncState.collect { sync -> _state.update { it.copy(sync = sync) } } }
    }

    fun syncNow() = store.syncNow()

    private val _reviewRequests = Channel<Unit>(Channel.CONFLATED)

    /** The weekly review's notification was tapped: open the review. */
    val reviewRequests: Flow<Unit> = _reviewRequests.receiveAsFlow()

    fun requestReview() {
        _reviewRequests.trySend(Unit)
    }

    // ---- views and tags ----

    fun select(viewId: String) {
        _state.update { it.copy(selectedViewId = viewId, openTask = null) }
        reloadTasks()
    }

    /** None if [query] is a valid filter, else what's wrong with it. */
    fun checkQuery(query: String): String? = store.checkQuery(query)

    fun countQuery(query: String): Int = run { store.countQuery(query, today()) }?.toInt() ?: 0

    fun createView(title: String, query: String) {
        run { store.createView(title.trim(), query.trim()) }?.let(::select)
    }

    fun updateView(id: String, title: String, query: String) {
        run { store.updateView(id, title.trim(), query.trim()) }
    }

    fun deleteView(view: TaskView) {
        run { store.deleteView(view.id) } ?: return
        if (_state.value.selectedViewId == view.id) select(INBOX_ID)
        _undoables.trySend(Undoable("“${view.title}” deleted"))
    }

    fun moveView(id: String, index: Int) {
        run { store.moveView(id, index.toUInt()) }
    }

    fun renameTag(old: String, new: String) {
        if (new.isBlank()) return
        run { store.renameTag(old, new) } ?: return
        if (_state.value.selectedViewId == "#$old") select("#" + normalizeTag(new))
    }

    fun deleteTag(tag: String) {
        run { store.deleteTag(tag) } ?: return
        if (_state.value.selectedViewId == "#$tag") select(INBOX_ID)
        _undoables.trySend(Undoable("#$tag removed from its tasks"))
    }

    // ---- today's habits ----

    /** A check-in today until the goal is met, then back to zero, as on the Habits tab. */
    fun toggleHabit(id: String) {
        run { store.toggleHabit(id, today()) }
    }

    fun logHabit(id: String, delta: Long) {
        run { store.logHabit(id, today(), delta) }
    }

    // ---- tasks ----

    fun rename(id: String, title: String) {
        if (title.isNotBlank()) run { store.rename(id, title.trim()) }
    }

    fun addSubtask(title: String, parentId: String) {
        if (title.isNotBlank()) run { store.addTask(parentId, title, today()) }
    }

    /**
     * Adds what was typed in the composer to the shown view (which gives it its tags, Today's date…),
     * dates and tags parsed out. Pasted lines make a task each.
     */
    fun quickAdd(text: String, dismissed: List<String> = emptyList()) {
        if (text.isBlank()) return
        val view = _state.value.selectedViewId
        if (text.trim().contains('\n')) {
            val ids = run { store.quickAddLines(view, text, today(), nowMinute()) } ?: return
            if (ids.size > 1) _undoables.trySend(Undoable("${ids.size} tasks added"))
            return
        }
        run { store.quickAdd(view, text, today(), nowMinute(), dismissed) }
    }

    /** Completing confirms with Undo, so a stray tap or swipe costs nothing. */
    fun toggle(task: TaskItem) {
        val completing = task.completedAt == null
        run { store.setCompleted(task.id, completing, today()) } ?: return
        if (completing) _undoables.trySend(Undoable("“${task.title}” done"))
    }

    /** Puts an archived task back, open. */
    fun restore(task: TaskItem) {
        run { store.setCompleted(task.id, false, today()) }
    }

    fun deleteArchived(task: TaskItem) {
        run { store.deleteItem(task.id) }
    }

    /** Returns how many were deleted, for the Undo toast. */
    fun clearArchive(): Int = run { store.clearArchive() }?.toInt() ?: 0

    fun logbook(limit: Int = 300): List<TaskItem> = run { store.logbook(limit.toUInt()) }.orEmpty()

    fun setSort(viewId: String, sort: TaskSort) {
        run { store.setViewSort(viewId, sort) }
    }

    /**
     * Takes the start date, time, deadline, Someday and repeat out of `text` ("fri 9am by mon every
     * week"); blank clears them all.
     */
    fun setSchedule(id: String, text: String, dismissed: List<String> = emptyList()) {
        if (text.isBlank()) {
            run { store.setSchedule(id, null, null, null, false, null) }
            return
        }
        val p = run { store.parseTask(text, today(), nowMinute(), dismissed) } ?: return
        run { store.setSchedule(id, p.start, p.time, p.deadline, p.someday, p.repeat) }
    }

    /** Only the deadline, from words ("fri", "12 dec"); blank clears it. Keeps the rest. */
    fun setDeadline(task: TaskItem, text: String) {
        val deadline = if (text.isBlank()) null else {
            val p = run { store.parseTask(text, today(), nowMinute(), emptyList()) } ?: return
            p.start ?: p.deadline ?: return
        }
        run { store.setSchedule(task.id, task.start, task.time, deadline, task.someday, task.repeat) }
    }

    fun setSomeday(task: TaskItem) {
        run { store.setSchedule(task.id, null, null, task.deadline, true, task.repeat) } ?: return
        _undoables.trySend(Undoable("Moved to Someday"))
    }

    fun setPriority(id: String, priority: UByte) {
        run { store.setPriority(id, priority) }
    }

    fun setTags(id: String, tags: List<String>) {
        run { store.setTags(id, tags) }
    }

    fun snooze(tasks: List<TaskItem>, to: Snooze) {
        val day = run { store.snooze(tasks.map { it.id }, to, today(), nowMinute()) } ?: return
        val label = scheduleLabel(day, null, null) ?: day
        _undoables.trySend(Undoable(if (tasks.size == 1) "Snoozed to $label" else "${tasks.size} tasks snoozed to $label"))
    }

    /** What slipped past its day comes to today in one step, with Undo. */
    fun moveToToday(tasks: List<TaskItem>) {
        if (tasks.isEmpty()) return
        run { store.reschedule(tasks.map { it.id }, today()) } ?: return
        _undoables.trySend(Undoable(if (tasks.size == 1) "Moved to today" else "${tasks.size} tasks moved to today"))
    }

    /** Open top-level tasks a view shows now, for the review and Inbox flows. */
    fun viewTasks(viewId: String): List<TaskItem> =
        run { store.viewOutline(viewId, today()) }?.open.orEmpty().filter { it.depth == 0u }.map { it.task }

    /** Open undated tasks untouched for a month, for the weekly review. */
    fun staleTasks(): List<TaskItem> = run { store.staleTasks(30u) }.orEmpty()

    fun keep(task: TaskItem) {
        run { store.keepTask(task.id) }
    }

    fun delete(task: TaskItem) {
        _state.update { if (it.openTask?.id == task.id) it.copy(openTask = null) else it }
        run { store.deleteItem(task.id) } ?: return
        _undoables.trySend(Undoable("Task deleted"))
    }

    fun setNotes(id: String, notes: String) {
        run { store.setNotes(id, notes) }
    }

    /** Tasks matching [query], open first. */
    fun search(query: String): List<TaskItem> = run { store.searchTasks(query, 50u) }.orEmpty()

    /** Drag and drop: [task] goes right before [beforeId] (a top-level row of this view, or a sibling subtask), or last. */
    fun moveBefore(task: TaskItem, beforeId: String?) {
        val parent = task.parentId
        val shownAtTop = _state.value.outline.open.any { it.task.id == task.id && it.depth == 0u }
        if (shownAtTop) {
            run { store.moveInView(_state.value.selectedViewId, task.id, beforeId, today()) }
        } else {
            run { store.moveTaskBefore(task.id, parent, beforeId) }
        }
    }

    fun indent(task: TaskItem) {
        run { store.indentInView(_state.value.selectedViewId, task.id, today()) }
    }

    fun outdent(task: TaskItem) {
        run { store.outdentInView(_state.value.selectedViewId, task.id) }
    }

    fun openTask(id: String?) {
        val task = id?.let { run { store.taskById(it) } }
        _state.update { it.copy(openTask = task, openSubtasks = task?.let { t -> run { store.subtasks(t.id) } }.orEmpty()) }
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

    // ---- shared views ----

    /** Share links of the view the share dialog is open for; null while loading. */
    private val _shares = MutableStateFlow<List<ListShare>?>(null)
    val shares: StateFlow<List<ListShare>?> = _shares.asStateFlow()

    fun loadShares(listId: String) {
        _shares.value = null
        launchBusy { _shares.value = store.shares(listId) }
    }

    fun shareList(listId: String, canEdit: Boolean, then: (ListShare) -> Unit = {}) = launchBusy {
        val share = store.shareList(listId, canEdit)
        _shares.update { it.orEmpty() + share }
        then(share)
    }

    fun revokeShare(share: ListShare) = launchBusy {
        store.revokeShare(share.id)
        _shares.update { it?.filter { s -> s.id != share.id } }
    }

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
        val views = run { store.views(today()) } ?: return
        val tags = run { store.tags() }.orEmpty()
        _state.update { s ->
            val id = s.selectedViewId
            val selected = id.takeIf { v -> views.any { it.id == v } || (v.startsWith("#") && tags.any { "#" + it.name == v }) } ?: INBOX_ID
            s.copy(views = views, tags = tags, selectedViewId = selected)
        }
        reloadTasks()
    }

    private fun reloadTasks() {
        val viewId = _state.value.selectedViewId
        val outline = run { store.viewOutline(viewId, today()) } ?: return
        val archive = run { store.archive() }.orEmpty()
        val habits = if (viewId == TODAY_ID) run { store.habitsNow(today()) }.orEmpty().filter { it.onToday } else emptyList()
        _state.update { s ->
            // The open task may have been deleted on another device.
            val open = s.openTask?.let { runCatching { store.taskById(it.id) }.getOrNull() }
            s.copy(
                outline = outline,
                todayHabits = habits,
                archive = archive,
                openTask = open,
                openSubtasks = open?.let { runCatching { store.subtasks(it.id) }.getOrNull() }.orEmpty(),
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
