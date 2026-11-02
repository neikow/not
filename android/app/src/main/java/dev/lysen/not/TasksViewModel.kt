package dev.lysen.not

import android.app.Application
import android.content.Context
import androidx.core.content.edit
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.NotException
import dev.lysen.not.core.StoreObserver
import dev.lysen.not.core.SyncState
import dev.lysen.not.core.TaskItem
import dev.lysen.not.core.TaskList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** A task plus its nesting depth, in display order. */
data class TaskRow(val task: TaskItem, val depth: Int)

data class UiState(
    val lists: List<TaskList> = emptyList(),
    val selectedListId: String? = null,
    val rows: List<TaskRow> = emptyList(),
    val sync: SyncState? = null,
    val serverUrl: String = "",
    val error: String? = null,
) {
    val selectedList: TaskList? get() = lists.firstOrNull { it.id == selectedListId }
}

class TasksViewModel(app: Application) : AndroidViewModel(app) {
    private val store = (app as NotApp).store
    // TODO: move the token to the Android Keystore once device pairing replaces the shared token.
    private val prefs = app.getSharedPreferences("sync", Context.MODE_PRIVATE)
    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

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
        store.setObserver(observer)
        reload()
        connect(prefs.getString(KEY_URL, "").orEmpty(), prefs.getString(KEY_TOKEN, "").orEmpty())
    }

    fun connect(serverUrl: String, token: String) {
        val url = serverUrl.trim()
        val secret = token.trim()
        prefs.edit {
            putString(KEY_URL, url)
            putString(KEY_TOKEN, secret)
        }
        _state.update { it.copy(serverUrl = url) }
        if (url.isEmpty() || secret.isEmpty()) {
            store.stopSync()
            _state.update { it.copy(sync = null) }
        } else {
            store.startSync(url, secret)
        }
    }

    fun syncNow() = store.syncNow()

    fun select(listId: String) {
        _state.update { it.copy(selectedListId = listId) }
        reloadTasks()
    }

    fun addList(title: String) {
        run { store.createList(title) }?.let(::select)
    }

    fun addTask(title: String) {
        val list = _state.value.selectedListId ?: ensureInbox() ?: return
        run { store.addTask(list, title) }
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
            val selected = s.selectedListId?.takeIf { id -> lists.any { it.id == id } } ?: lists.firstOrNull()?.id
            s.copy(lists = lists, selectedListId = selected)
        }
        reloadTasks()
    }

    private fun reloadTasks() {
        val list = _state.value.selectedListId
        val tasks = if (list == null) emptyList() else run { store.tasks(list) } ?: return
        _state.update { it.copy(rows = flatten(tasks)) }
    }

    /** Creates a list when there is none yet. TODO: a real Inbox must be a well-known node. */
    private fun ensureInbox(): String? =
        _state.value.lists.firstOrNull()?.id ?: run { store.createList("Inbox") }

    private fun <T> run(body: () -> T): T? =
        try {
            body()
        } catch (e: NotException) {
            _state.update { it.copy(error = e.message) }
            null
        }

    companion object {
        private const val KEY_URL = "url"
        private const val KEY_TOKEN = "token"

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
