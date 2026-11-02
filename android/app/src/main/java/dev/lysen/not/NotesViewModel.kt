package dev.lysen.not

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.Folder
import dev.lysen.not.core.NoteSummary
import dev.lysen.not.core.NotException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class NotesState(
    val folders: List<Folder> = emptyList(),
    /** Null for all notes. */
    val folderId: String? = null,
    /** Notes of the folder, or search results. */
    val notes: List<NoteSummary> = emptyList(),
    /** Every note, so the open one stays open when it stops matching a search. */
    val all: List<NoteSummary> = emptyList(),
    /** Null when not searching. */
    val query: String? = null,
    /** The note open in the editor. */
    val openNoteId: String? = null,
    val error: String? = null,
) {
    val folder: Folder? get() = folders.firstOrNull { it.id == folderId }
    val openNote: NoteSummary? get() = all.firstOrNull { it.id == openNoteId }

    fun folderTitle(id: String?): String = folders.firstOrNull { it.id == id }?.title ?: "Notes"
}

class NotesViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as NotApp
    private val store = app.store
    private val _state = MutableStateFlow(NotesState())
    val state: StateFlow<NotesState> = _state.asStateFlow()

    private val _undoables = Channel<Undoable>(Channel.CONFLATED)
    val undoables: Flow<Undoable> = _undoables.receiveAsFlow()

    init {
        viewModelScope.launch { app.changes.collect { reload() } }
    }

    fun selectFolder(id: String?) {
        _state.update { it.copy(folderId = id) }
        reload()
    }

    /** Null ends the search. */
    fun search(query: String?) {
        _state.update { it.copy(query = query) }
        reload()
    }

    fun newNote() {
        val id = run { store.createNote(_state.value.folderId, "") } ?: return
        _state.update { it.copy(query = null) }
        reload()
        open(id)
    }

    fun open(id: String?) = _state.update { it.copy(openNoteId = id) }

    fun body(id: String): String = run { store.noteBody(id) } ?: ""

    fun setBody(id: String, body: String) {
        run { store.setNoteBody(id, body) }
    }

    /**
     * Leaving the editor: saves [pending] text, and removes the note if nobody typed into it. Quiet,
     * since the note may have just been deleted.
     */
    fun close(id: String, pending: String?) {
        try {
            if (pending != null) store.setNoteBody(id, pending)
            if (store.noteBody(id).isBlank()) store.deleteNote(id)
        } catch (_: NotException) {
        }
    }

    fun togglePin(note: NoteSummary) {
        run { store.setNotePinned(note.id, !note.pinned) }
    }

    fun move(note: NoteSummary, folderId: String?) {
        run { store.moveNote(note.id, folderId) }
    }

    fun delete(note: NoteSummary) {
        _state.update { if (it.openNoteId == note.id) it.copy(openNoteId = null) else it }
        run { store.deleteNote(note.id) } ?: return
        _undoables.trySend(Undoable("Note deleted"))
    }

    fun addFolder(title: String) {
        if (title.isBlank()) return
        run { store.createFolder(_state.value.folderId, title.trim()) }?.let(::selectFolder)
    }

    fun renameFolder(id: String, title: String) {
        if (title.isNotBlank()) run { store.renameFolder(id, title.trim()) }
    }

    fun deleteFolder(folder: Folder) {
        run { store.deleteNote(folder.id) } ?: return
        selectFolder(folder.parentId)
        _undoables.trySend(Undoable("“${folder.title}” deleted"))
    }

    fun undo() {
        run { store.undo() }
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    private fun reload() {
        val folders = run { store.folders() } ?: return
        val s = _state.value
        // The folder may have been deleted on another device.
        val folderId = s.folderId?.takeIf { id -> folders.any { it.id == id } }
        val query = s.query?.trim().orEmpty()
        val all = run { store.notes(null) } ?: return
        val notes = when {
            query.isNotEmpty() -> run { store.searchNotes(query) } ?: return
            folderId == null -> all
            else -> all.filter { it.folderId == folderId }
        }
        _state.update {
            it.copy(
                folders = folders,
                folderId = folderId,
                notes = notes,
                all = all,
                openNoteId = it.openNoteId.takeIf { id -> all.any { n -> n.id == id } },
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
}
