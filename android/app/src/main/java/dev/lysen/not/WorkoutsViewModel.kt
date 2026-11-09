package dev.lysen.not

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.ExerciseLog
import dev.lysen.not.core.NotException
import dev.lysen.not.core.Workout
import dev.lysen.not.core.WorkoutExercise
import dev.lysen.not.core.WorkoutSummary
import dev.lysen.not.core.parseSets
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.LocalDate

data class WorkoutsState(
    /** Latest first. */
    val workouts: List<WorkoutSummary> = emptyList(),
    /** The session open in the editor. */
    val openId: String? = null,
    val open: Workout? = null,
    /** Exercise names used so far, most recent first. */
    val exerciseNames: List<String> = emptyList(),
    val today: LocalDate = LocalDate.now(),
    val error: String? = null,
)

class WorkoutsViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as NotApp
    private val store = app.store
    val timer: RestTimer = app.restTimer
    private val _state = MutableStateFlow(WorkoutsState())
    val state: StateFlow<WorkoutsState> = _state.asStateFlow()

    private val _undoables = Channel<Undoable>(Channel.CONFLATED)
    val undoables: Flow<Undoable> = _undoables.receiveAsFlow()

    init {
        viewModelScope.launch { app.changes.collect { reload() } }
    }

    fun refreshToday() {
        val now = LocalDate.now()
        if (now != _state.value.today) _state.update { it.copy(today = now) }
    }

    fun open(id: String?) {
        _state.update { it.copy(openId = id) }
        reload()
    }

    fun newWorkout() {
        refreshToday()
        run { store.createWorkout(today(), "") }?.let(::open)
    }

    fun repeat(id: String) {
        refreshToday()
        run { store.repeatWorkout(id, today()) }?.let(::open)
    }

    fun update(workout: Workout, date: String = workout.date, title: String = workout.title) {
        run { store.updateWorkout(workout.id, date, title) }
    }

    fun delete(workout: WorkoutSummary) {
        if (_state.value.openId == workout.id) open(null)
        run { store.deleteWorkout(workout.id) } ?: return
        _undoables.trySend(Undoable("Workout deleted"))
    }

    /** Returns the new exercise's id. */
    fun addExercise(workoutId: String, name: String): String? =
        if (name.isBlank()) null else run { store.addExercise(workoutId, name.trim(), "") }

    fun rename(exercise: WorkoutExercise, name: String) {
        if (name.isNotBlank() && name.trim() != exercise.name) run { store.updateExercise(exercise.id, name.trim(), exercise.sets) }
    }

    fun setSets(exercise: WorkoutExercise, lines: List<String>) {
        run { store.updateExercise(exercise.id, exercise.name, lines.joinToString("\n")) }
    }

    /** Appends a line of sets; a logged working set starts the rest timer if that's on. */
    fun addSets(exercise: WorkoutExercise, text: String) {
        val line = text.trim()
        if (line.isEmpty()) return
        setSets(exercise, exercise.lines.map { it.raw } + line)
        val parsed = parseSets(line).firstOrNull()
        if (parsed != null && parsed.parsed && !parsed.warmup && timer.startsAfterSet.value) timer.start()
    }

    fun deleteExercise(exercise: WorkoutExercise) {
        run { store.deleteExercise(exercise.id) } ?: return
        _undoables.trySend(Undoable("“${exercise.name}” removed"))
    }

    fun moveExercise(exercise: WorkoutExercise, index: Int) {
        run { store.moveExercise(exercise.id, index.coerceAtLeast(0).toUInt()) }
    }

    fun history(name: String): List<ExerciseLog> = run { store.exerciseHistory(name, 30u) }.orEmpty()

    /** Sessions from a pasted text log; returns how many were added. */
    fun import(text: String): Int {
        refreshToday()
        return run { store.importWorkouts(text, today()) }?.toInt() ?: 0
    }

    fun undo() {
        run { store.undo() }
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    /** Debug builds: sample sessions on an empty device, for screenshots. */
    fun seedDemo() {
        if (!BuildConfig.DEBUG || _state.value.workouts.isNotEmpty()) return
        import(
            """
            30/10
            Tirage vertical (large)
            66kg x 8 x 3
            Straight Arm Pulldown (double)
            24.8kg x (10 x 2 + 8)
            Curl Biceps Haltères
            14kg x 10 x 2

            02/11
            Bench Haltères
            E 18kg x 10
            24kg x 10 x 3
            Shrugs
            24kg x 20
            24kg x 15 x 2
            Abdos
            """.trimIndent(),
        )
        reload()
        val first = _state.value.workouts.firstOrNull() ?: return
        val copy = run { store.repeatWorkout(first.id, today()) } ?: return
        run { store.workout(copy) }?.exercises?.firstOrNull()?.let {
            run { store.updateExercise(it.id, it.name, "E 18kg x 10\n26kg x 10 x 2\n26kg x 8\nfelt strong") }
        }
        open(copy)
    }

    private fun today() = _state.value.today.toString()

    private fun reload() {
        val workouts = run { store.workouts() } ?: return
        val names = run { store.exerciseNames() }.orEmpty()
        val openId = _state.value.openId?.takeIf { id -> workouts.any { it.id == id } }
        val open = openId?.let { run { store.workout(it) } }
        _state.update { it.copy(workouts = workouts, exerciseNames = names, openId = openId, open = open) }
    }

    private fun <T> run(body: () -> T): T? =
        try {
            body()
        } catch (e: NotException) {
            _state.update { it.copy(error = e.userMessage) }
            null
        }
}
