package dev.lysen.not

import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.distinctUntilChanged
import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.lysen.not.core.CatalogExercise
import dev.lysen.not.core.ExerciseLog
import dev.lysen.not.core.MuscleSets
import dev.lysen.not.core.NotException
import dev.lysen.not.core.Workout
import dev.lysen.not.core.WorkoutExercise
import dev.lysen.not.core.WorkoutSummary
import dev.lysen.not.core.expandSets
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
    /** Working sets per muscle since Monday. */
    val weekMuscles: List<MuscleSets> = emptyList(),
    /** Bumped on every change to the store, for what's read from it outside this state (links, history). */
    val revision: Long = 0,
    val error: String? = null,
)

class WorkoutsViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as NotApp
    private val store = app.store
    val timer: RestTimer = app.restTimer
    private val memory = app.screenMemory
    private val _state = MutableStateFlow(WorkoutsState(openId = memory.workout))
    val state: StateFlow<WorkoutsState> = _state.asStateFlow()

    private val _undoables = Channel<Undoable>(Channel.CONFLATED)
    val undoables: Flow<Undoable> = _undoables.receiveAsFlow()

    init {
        viewModelScope.launch { app.changes.collect { reload() } }
        // Cleared by reload() if the workout was deleted meanwhile.
        viewModelScope.launch { state.map { it.openId }.distinctUntilChanged().collect { memory.workout = it } }
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

    /**
     * Ticks a set off (or back on). Ticking a working set starts the rest timer if that's on, and
     * ticking the last open set ticks the exercise off too.
     */
    fun toggleSet(exercise: WorkoutExercise, index: Int) {
        val line = exercise.lines.getOrNull(index) ?: return
        val done = !line.done
        val lines = exercise.lines.map { it.raw }.toMutableList()
        lines[index] = markDone(line.raw, done)
        setSets(exercise, lines)
        if (!done) {
            if (exercise.done) setDone(exercise, false)
            return
        }
        if (!line.warmup && line.reps.isNotEmpty() && timer.startsAfterSet.value) timer.start()
        val sets = exercise.lines.filterIndexed { i, l -> l.parsed && i != index }
        if (!exercise.done && sets.all { it.done }) run { store.setExerciseDone(exercise.id, true) }
    }

    /** Removes one set line (lines already one per set), with Undo. */
    fun deleteSet(exercise: WorkoutExercise, index: Int) {
        val lines = exercise.lines.map { it.raw }
        if (index !in lines.indices) return
        setSets(exercise, lines.filterIndexed { i, _ -> i != index })
        _undoables.trySend(Undoable("Set removed"))
    }

    /**
     * Appends a line of sets, one line per set and ticked off (typed in = just done); a logged
     * working set starts the rest timer if that's on.
     */
    fun addSets(exercise: WorkoutExercise, text: String) {
        val line = text.trim()
        if (line.isEmpty()) return
        val sets = expandSets(line).lines().map { if (parseSets(it).firstOrNull()?.parsed == true) markDone(it, true) else it }
        setSets(exercise, exercise.lines.map { it.raw } + sets)
        val parsed = parseSets(line).firstOrNull()
        // Sets only: a swim or a run isn't followed by a rest.
        if (parsed != null && parsed.parsed && !parsed.warmup && parsed.reps.isNotEmpty() && timer.startsAfterSet.value) timer.start()
    }

    fun setDone(exercise: WorkoutExercise, done: Boolean) {
        run { store.setExerciseDone(exercise.id, done) }
    }

    fun deleteExercise(exercise: WorkoutExercise) {
        run { store.deleteExercise(exercise.id) } ?: return
        _undoables.trySend(Undoable("“${exercise.name}” removed"))
    }

    fun moveExercise(exercise: WorkoutExercise, index: Int) {
        run { store.moveExercise(exercise.id, index.coerceAtLeast(0).toUInt()) }
    }

    fun history(name: String): List<ExerciseLog> = run { store.exerciseHistory(name, 30u) }.orEmpty()

    fun searchExercises(query: String, limit: Int = 8): List<CatalogExercise> = store.searchExercises(query, limit.toUInt())

    fun exerciseInfo(name: String): CatalogExercise? = store.exerciseInfo(name)

    /** Null unlinks the name from the catalogue. */
    fun link(name: String, catalogId: String?) {
        run { store.linkExercise(name, catalogId) } ?: return
        reload()
    }

    /** Sessions from a text log (demo data); returns how many were added. */
    private fun import(text: String): Int {
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
        searchExercises("Développé couché avec haltères", 1).firstOrNull()?.let { link("Bench Haltères", it.id) }
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
        val today = _state.value.today
        val muscles = run { store.muscleSets(today.minusDays(today.dayOfWeek.value - 1L).toString(), today.toString()) }.orEmpty()
        _state.update { it.copy(workouts = workouts, exerciseNames = names, openId = openId, open = open, weekMuscles = muscles, revision = it.revision + 1) }
    }

    private fun <T> run(body: () -> T): T? =
        try {
            body()
        } catch (e: NotException) {
            _state.update { it.copy(error = e.userMessage) }
            null
        }
}

/** Starts a set line that's done; the core's `DONE_MARK`, which reads ✓ and ✔. */
private const val DONE_MARK = "✓ "

/** [raw] with the done mark added or taken off. */
fun markDone(raw: String, done: Boolean): String {
    val bare = raw.trim().removePrefix("✓").removePrefix("✔").trim()
    return if (done) DONE_MARK + bare else bare
}
