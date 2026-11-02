package dev.lysen.not

import android.app.Application
import androidx.glance.appwidget.updateAll
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore
import dev.lysen.not.core.StoreObserver
import dev.lysen.not.core.SyncState
import dev.lysen.not.widget.HabitsWidget
import dev.lysen.not.widget.NoteWidget
import dev.lysen.not.widget.NotesWidget
import dev.lysen.not.widget.TasksWidget
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.io.File

/**
 * Owns the process-wide store so the activity, widgets and background sync share one replica, and
 * starts live sync as soon as anything opens it.
 */
class NotApp : Application() {
    private val _changes = MutableStateFlow(0L)

    /** Bumped on every change to the store (local or pulled): re-query what is on screen. */
    val changes: StateFlow<Long> = _changes.asStateFlow()

    private val _syncState = MutableStateFlow<SyncState?>(null)

    /** Latest live-sync state; null while unpaired. */
    val syncState: StateFlow<SyncState?> = _syncState.asStateFlow()

    // Called from Rust threads.
    private val observer = object : StoreObserver {
        override fun onChange() = _changes.update { it + 1 }

        override fun onSyncState(state: SyncState) {
            _syncState.value = state
        }
    }

    val store: NotStore by lazy {
        NotStore(File(filesDir, "not.db").path).also {
            it.setObserver(observer)
            try {
                it.startSync()
            } catch (_: NotException) {
                // Unreadable credentials: the sync dialog shows the device as unpaired.
            }
        }
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @OptIn(FlowPreview::class)
    override fun onCreate() {
        super.onCreate()
        SyncWorker.schedulePeriodic(this)
        // Widgets whose session ended need a nudge to re-render; coalesce bursts like a big pull.
        scope.launch {
            changes.drop(1).debounce(300).collect {
                listOf(TasksWidget(), HabitsWidget(), NotesWidget(), NoteWidget()).forEach { it.updateAll(this@NotApp) }
            }
        }
    }

    fun clearSyncState() {
        _syncState.value = null
    }
}
