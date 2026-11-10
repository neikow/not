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
    /** As of [day]: its check-ins, and the streaks and week up to it. */
    val habits: List<HabitSummary> = emptyList(),
    /** The user's local date. */
    val today: LocalDate = LocalDate.now(),
    /** The day shown and checked in on: today, or a past one being caught up on. */
    val day: LocalDate = today,
    /** The habit open in the detail sheet. */
    val openHabitId: String? = null,
    /** On today's to-do and not done yet, for badges; counted on today even while a past day is shown. */
    val pending: Int = 0,
    val error: String? = null,
) {
    val openHabit: HabitSummary? get() = habits.firstOrNull { it.habit.id == openHabitId }

    val isToday: Boolean get() = day == today
}

/** How far back the day picker goes, today included. */
const val PAST_DAYS = 7

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
            // Showing today follows it to the new one; a past day stays put while it's in range.
            _state.update { it.copy(today = now, day = if (it.isToday || it.day < now.minusDays(PAST_DAYS - 1L)) now else it.day) }
            reload()
        }
    }

    /** Shows [date] (today or one of the past days), so check-ins land on it. */
    fun selectDay(date: LocalDate) {
        val today = _state.value.today
        val day = date.coerceIn(today.minusDays(PAST_DAYS - 1L), today)
        if (day != _state.value.day) {
            _state.update { it.copy(day = day) }
            reload()
        }
    }

    fun add(title: String) {
        if (title.isNotBlank()) run { store.createHabit(title.trim(), 1u, everyDay()) }
    }

    /** A check-in on the day shown until the goal is met, then back to zero. */
    fun toggle(id: String) {
        run { store.toggleHabit(id, day()) }
    }

    fun log(id: String, delta: Long) {
        run { store.logHabit(id, day(), delta) }
    }

    fun update(habit: Habit) {
        if (habit.title.isNotBlank()) run { store.updateHabit(habit.id, habit.title.trim(), habit.goal, habit.days, habit.period) }
    }

    fun delete(habit: Habit) {
        _state.update { if (it.openHabitId == habit.id) it.copy(openHabitId = null) else it }
        run { store.deleteHabit(habit.id) } ?: return
        _undoables.trySend(Undoable("“${habit.title}” deleted"))
    }

    /** Drag and drop: [id] goes to [index] in the list. */
    fun move(id: String, index: Int) {
        run { store.moveHabit(id, index.toUInt()) }
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

    private fun day() = _state.value.day.toString()

    private fun reload() {
        val habits = run { store.habits(day()) } ?: return
        val todays = if (_state.value.isToday) habits else run { store.habits(today()) } ?: return
        _state.update { s ->
            s.copy(
                habits = habits,
                openHabitId = s.openHabitId.takeIf { id -> habits.any { it.habit.id == id } },
                pending = todays.count { it.pending },
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
