package dev.lysen.not

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.Habit
import dev.lysen.not.core.HabitSummary
import dev.lysen.not.core.NotException
import dev.lysen.not.core.everyDay
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.LocalDate

data class HabitsState(
    val habits: List<HabitSummary> = emptyList(),
    /** The user's local date; check-ins land on it. */
    val today: LocalDate = LocalDate.now(),
    /** The habit open in the detail sheet. */
    val openHabitId: String? = null,
    val error: String? = null,
) {
    val openHabit: HabitSummary? get() = habits.firstOrNull { it.habit.id == openHabitId }

    /** Due today and not done yet, for badges. */
    val pending: Int get() = habits.count { it.due && it.count < it.habit.goal }
}

class HabitsViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as NotApp
    private val store = app.store
    private val _state = MutableStateFlow(HabitsState())
    val state: StateFlow<HabitsState> = _state.asStateFlow()

    private val _undoables = Channel<Undoable>(Channel.CONFLATED)
    val undoables: Flow<Undoable> = _undoables.receiveAsFlow()

    init {
        viewModelScope.launch { app.changes.collect { reload() } }
    }

    /** Picks up a new day, e.g. when the app comes back the next morning. */
    fun refreshToday() {
        val now = LocalDate.now()
        if (now != _state.value.today) {
            _state.update { it.copy(today = now) }
            reload()
        }
    }

    fun add(title: String) {
        if (title.isNotBlank()) run { store.createHabit(title.trim(), 1u, everyDay()) }
    }

    /** A check-in until the goal is met, then back to zero. */
    fun toggle(id: String) {
        run { store.toggleHabit(id, today()) }
    }

    fun log(id: String, delta: Long) {
        run { store.logHabit(id, today(), delta) }
    }

    fun update(habit: Habit) {
        if (habit.title.isNotBlank()) run { store.updateHabit(habit.id, habit.title.trim(), habit.goal, habit.days) }
    }

    fun delete(habit: Habit) {
        _state.update { if (it.openHabitId == habit.id) it.copy(openHabitId = null) else it }
        run { store.deleteHabit(habit.id) } ?: return
        _undoables.trySend(Undoable("“${habit.title}” deleted"))
    }

    fun undo() {
        run { store.undo() }
    }

    fun open(id: String?) = _state.update { it.copy(openHabitId = id) }

    fun dismissError() = _state.update { it.copy(error = null) }

    /** Check-ins per day from [from] through today. */
    fun history(id: String, from: LocalDate): Map<LocalDate, UInt> =
        (run { store.habitDays(id, from.toString(), today()) } ?: emptyList())
            .associate { LocalDate.parse(it.date) to it.count }

    private fun today() = _state.value.today.toString()

    private fun reload() {
        val habits = run { store.habits(today()) } ?: return
        _state.update { s ->
            s.copy(habits = habits, openHabitId = s.openHabitId.takeIf { id -> habits.any { it.habit.id == id } })
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
