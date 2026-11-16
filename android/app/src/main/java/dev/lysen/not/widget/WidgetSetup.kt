package dev.lysen.not.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.FrameLayout
import android.widget.RemoteViews
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.glance.GlanceId
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.compose
import androidx.glance.appwidget.state.getAppWidgetState
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.glance.appwidget.updateAll
import androidx.glance.state.PreferencesGlanceStateDefinition
import androidx.lifecycle.lifecycleScope
import dev.lysen.not.NotApp
import dev.lysen.not.NotTheme
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore
import dev.lysen.not.habitsNow
import dev.lysen.not.today
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.launch

/** An option in a widget's "Show" list: a list, folder, note or habit. */
data class PickerOption(val id: String, val label: String, val depth: Int = 0)

/** The app's widgets, with what each one's setup offers. */
enum class WidgetKind(val title: String, val receiver: Class<*>) {
    Tasks("Tasks", TasksWidgetReceiver::class.java),
    Next("Up next", NextWidgetReceiver::class.java),
    Habits("Habits", HabitsWidgetReceiver::class.java),
    Timed("Timed habit", TimedWidgetReceiver::class.java),
    Notes("Notes", NotesWidgetReceiver::class.java),
    Note("Note", NoteWidgetReceiver::class.java);

    fun widget(): GlanceAppWidget = when (this) {
        Tasks -> TasksWidget()
        Next -> NextWidget()
        Habits -> HabitsWidget()
        Timed -> TimedWidget()
        Notes -> NotesWidget()
        Note -> NoteWidget()
    }

    /** Which list, folder, note or habit it shows: one of these, saved under [sourceKey]. */
    val sourceKey: Preferences.Key<String>?
        get() = when (this) {
            Tasks -> LIST_KEY
            Timed -> TIMED_KEY
            Notes -> FOLDER_KEY
            Note -> NOTE_KEY
            Next, Habits -> null
        }

    val sourceLabel: String
        get() = when (this) {
            Tasks -> "List"
            Timed -> "Habit"
            Notes -> "Folder"
            Note -> "Note"
            else -> ""
        }

    fun sources(store: NotStore): List<PickerOption> = try {
        when (this) {
            Tasks -> store.views(today()).map { PickerOption(it.id, it.title) }
            Timed -> store.habitsNow(today()).filter { it.habit.timed != null }.map { PickerOption(it.habit.id, it.habit.title) }
            Notes -> listOf(PickerOption("", "All notes")) + store.folders().map { PickerOption(it.id, it.title, it.depth.toInt()) }
            Note -> store.notes(null).map { PickerOption(it.id, it.title.ifEmpty { "New note" }) }
            Next, Habits -> emptyList()
        }
    } catch (_: NotException) {
        emptyList()
    }

    val options: List<WidgetChoice>
        get() = when (this) {
            Tasks -> TasksOptions.all
            Next -> NextOptions.all
            Habits -> HabitsOptions.all
            Timed -> TimedOptions.all
            Notes -> NotesOptions.all
            Note -> emptyList()
        }

    /** A size to preview it at before the launcher says, roughly its default cells. */
    val defaultSize: DpSize
        get() = when (this) {
            Next -> DpSize(180.dp, 90.dp)
            Tasks, Notes, Note -> DpSize(260.dp, 260.dp)
            Habits, Timed -> DpSize(260.dp, 180.dp)
        }

    companion object {
        fun of(className: String?): WidgetKind? = entries.firstOrNull { it.receiver.name == className }

        /** Redraws every placed widget, after the look defaults changed. */
        suspend fun updateAll(context: Context) = entries.forEach { it.widget().updateAll(context) }
    }
}

/**
 * Setup for any widget, shown when placing it and from the launcher's reconfigure: what it shows,
 * content options, and its look (each look option can stay on the default from Settings). A live
 * preview, rendered by the widget itself, follows every change. Nothing is saved until Save.
 */
class WidgetSetupActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Debug builds: `--es sizes Timed` shows that widget at every common size, for checking layouts.
        if (dev.lysen.not.BuildConfig.DEBUG) {
            intent.getStringExtra("sizes")?.let { name ->
                val kind = WidgetKind.valueOf(name)
                setContent { NotTheme { SizeGallery(kind) } }
                return
            }
        }
        val widgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        // Backing out cancels placing the widget, or leaves a configured one unchanged.
        setResult(RESULT_CANCELED, result(widgetId))
        val manager = AppWidgetManager.getInstance(this)
        val kind = WidgetKind.of(manager.getAppWidgetInfo(widgetId)?.provider?.className)
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID || kind == null) return finish()

        lifecycleScope.launch {
            val glanceId = GlanceAppWidgetManager(this@WidgetSetupActivity).getGlanceIdBy(widgetId)
            val saved = getAppWidgetState(this@WidgetSetupActivity, PreferencesGlanceStateDefinition, glanceId)
            val options = manager.getAppWidgetOptions(widgetId)
            val w = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
            val h = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT)
            val size = if (w > 0 && h > 0) DpSize(w.dp, h.dp) else kind.defaultSize
            setContent {
                NotTheme {
                    WidgetSetupScreen(
                        kind = kind,
                        glanceId = glanceId,
                        size = size,
                        initial = saved,
                        onCancel = ::finish,
                        onSave = { draft -> save(kind, widgetId, glanceId, draft) },
                    )
                }
            }
        }
    }

    private fun save(kind: WidgetKind, widgetId: Int, glanceId: GlanceId, draft: Preferences) {
        lifecycleScope.launch {
            updateAppWidgetState(this@WidgetSetupActivity, glanceId) { prefs ->
                prefs.clear()
                prefs += draft
            }
            kind.widget().update(this@WidgetSetupActivity, glanceId)
            setResult(RESULT_OK, result(widgetId))
            finish()
        }
    }

    private fun result(widgetId: Int) = Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WidgetSetupScreen(
    kind: WidgetKind,
    glanceId: GlanceId,
    size: DpSize,
    initial: Preferences,
    onCancel: () -> Unit,
    onSave: (Preferences) -> Unit,
) {
    val context = LocalContext.current
    val store = (context.applicationContext as NotApp).store
    var draft by remember { mutableStateOf(initial) }
    fun edit(change: MutablePreferences.() -> Unit) {
        draft = draft.toMutablePreferences().apply(change).toPreferences()
    }
    val colors = Not.colors

    Column(Modifier.fillMaxSize().background(colors.background).statusBarsPadding().navigationBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Button("Cancel", onCancel, kind = ButtonKind.Quiet)
            Text(kind.title, style = Not.type.heading, modifier = Modifier.weight(1f), textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Button("Save", { onSave(draft) })
        }
        WidgetPreview(kind, glanceId, size, draft, Modifier.fillMaxWidth().padding(horizontal = 16.dp))
        Column(
            Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 20.dp),
            verticalArrangement = Arrangement.spacedBy(22.dp),
        ) {
            kind.sourceKey?.let { key ->
                val sources = remember { kind.sources(store) }
                SetupSection(kind.sourceLabel) {
                    if (sources.isEmpty()) {
                        Text("Nothing to choose from yet.", style = Not.type.caption, color = colors.inkMuted)
                    }
                    val chosen = draft[key] ?: sources.firstOrNull()?.id
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        sources.forEach { s -> Pill(s.label, s.id == chosen) { edit { this[key] = s.id } } }
                    }
                }
            }
            if (kind == WidgetKind.Habits) {
                val habits = remember { runCatching { store.habitsNow(today()) }.getOrDefault(emptyList()) }
                val picked = draft[HabitsOptions.ids].orEmpty()
                SetupSection("Habits") {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Pill("All", picked.isEmpty()) { edit { remove(HabitsOptions.ids) } }
                        habits.forEach { h ->
                            val on = h.habit.id in picked
                            Pill(h.habit.title, on) {
                                edit {
                                    val next = if (on) picked - h.habit.id else picked + h.habit.id
                                    if (next.isEmpty()) remove(HabitsOptions.ids) else this[HabitsOptions.ids] = next
                                }
                            }
                        }
                    }
                }
            }
            if (kind.options.isNotEmpty()) {
                SetupSection("Content") {
                    kind.options.forEach { choice ->
                        ChoiceRow(choice.label, choice.options, choice.of(draft)) { v -> edit { this[choice.key] = v } }
                    }
                }
            }
            SetupSection("Look") {
                LookRows(
                    current = { key -> draft[key] },
                    withDefault = true,
                    onChange = { key, value -> edit { setOverride(key, value) } },
                )
                Text("“Default” follows Settings → Widgets.", style = Not.type.caption, color = colors.inkMuted)
            }
        }
    }
}

/**
 * The widget as it will look, rendered by Glance from [state] into RemoteViews, over a dark
 * wallpaper-like backdrop so clear and translucent backgrounds read. Not tappable.
 */
@Composable
fun WidgetPreview(kind: WidgetKind, glanceId: GlanceId?, size: DpSize, state: Preferences, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var views by remember { mutableStateOf<RemoteViews?>(null) }
    LaunchedEffect(kind, state, WidgetLooks.current(context)) {
        val state = state.toMutablePreferences().apply { this[PREVIEW_KEY] = "1" }.toPreferences()
        views = try {
            if (glanceId != null) kind.widget().compose(context, glanceId, size = size, state = state)
            else kind.widget().compose(context, size = size, state = state)
        } catch (e: Exception) {
            android.util.Log.w("NotWidget", "preview failed", e)
            null
        }
    }
    val heightDp = size.height.coerceAtMost(280.dp)
    Box(
        modifier
            .clip(RoundedCornerShape(28.dp))
            .background(Brush.linearGradient(listOf(Color(0xFF3B4466), Color(0xFF14161F))))
            .padding(vertical = 18.dp),
        contentAlignment = Alignment.Center,
    ) {
        val density = LocalDensity.current
        AndroidView(
            factory = { FrameLayout(it) },
            update = { frame ->
                frame.removeAllViews()
                views?.let { rv ->
                    runCatching { rv.apply(frame.context, frame) }.getOrNull()?.let { v ->
                        frame.addView(v, FrameLayout.LayoutParams(with(density) { size.width.roundToPx() }, with(density) { heightDp.roundToPx() }))
                    }
                }
            },
            modifier = Modifier.size(size.width, heightDp),
        )
        // A preview, not the real thing: swallow taps.
        Box(Modifier.size(size.width, heightDp).clickable(remember { MutableInteractionSource() }, indication = null) {})
    }
}

/**
 * The five look options. [current] gives the chosen value of each key: a widget's override (null =
 * "Default"), or in Settings the default itself, where there's no "Default" pill ([withDefault]).
 */
@Composable
fun LookRows(
    current: (Preferences.Key<String>) -> String?,
    withDefault: Boolean,
    onChange: (Preferences.Key<String>, String?) -> Unit,
) {
    val rows = listOf(
        Triple("Background", LOOK_BACKGROUND, WidgetBackground.entries.map { it.name to it.label }),
        Triple("Text size", LOOK_TEXT_SIZE, WidgetTextSize.entries.map { it.name to it.label }),
        Triple("Theme", LOOK_THEME, WidgetTheme.entries.map { it.name to it.label }),
        Triple("Density", LOOK_DENSITY, WidgetDensity.entries.map { it.name to it.label }),
        Triple("Title row", LOOK_HEADER, listOf("on" to "Show", "off" to "Hide")),
    )
    rows.forEach { (label, key, values) ->
        val options = (if (withDefault) listOf("" to "Default") else emptyList()) + values
        ChoiceRow(label, options, current(key) ?: "") { v -> onChange(key, v.ifEmpty { null }) }
    }
}

@Composable
fun SetupSection(label: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(label, style = Not.type.label, color = Not.colors.inkMuted)
        content()
    }
}

/** A label over a row of pills, one chosen. Scrolls sideways when the pills don't fit. */
@Composable
fun ChoiceRow(label: String, options: List<Pair<String, String>>, selected: String, onSelect: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, style = Not.type.body)
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            options.forEach { (value, text) -> Pill(text, value == selected) { onSelect(value) } }
        }
    }
}

@Composable
private fun Pill(text: String, on: Boolean, onClick: () -> Unit) {
    val colors = Not.colors
    val bg by animateColorAsState(if (on) colors.accent else colors.sunken, label = "pill")
    Box(
        Modifier.heightIn(min = 38.dp).clip(CircleShape).background(bg)
            .pressable(onClick, role = Role.RadioButton, pressedScale = 0.94f)
            .padding(horizontal = 16.dp, vertical = 9.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Not.type.label, color = if (on) colors.onAccent else colors.inkMuted, maxLines = 1)
    }
}

/** The look defaults for Settings → Widgets, as overrides-shaped preferences (for the preview). */
fun WidgetLook.asPreferences(): Preferences = emptyPreferences().toMutablePreferences().apply {
    this[LOOK_BACKGROUND] = background.name
    this[LOOK_TEXT_SIZE] = textSize.name
    this[LOOK_THEME] = theme.name
    this[LOOK_DENSITY] = density.name
    this[LOOK_HEADER] = if (header) "on" else "off"
}.toPreferences()

/** Reads a look back from [asPreferences]-shaped keys. */
fun Preferences.asLook(): WidgetLook = lookOver(WidgetLook())

/** Debug only: one widget at the sizes launchers commonly give, from 1×1 to 4×3. */
@Composable
private fun SizeGallery(kind: WidgetKind) {
    val sizes = listOf(
        "1×1" to DpSize(70.dp, 70.dp),
        "2×1" to DpSize(140.dp, 70.dp),
        "4×1" to DpSize(320.dp, 70.dp),
        "2×2" to DpSize(150.dp, 180.dp),
        "3×2" to DpSize(250.dp, 180.dp),
        "4×2" to DpSize(330.dp, 190.dp),
        "4×3" to DpSize(330.dp, 270.dp),
    )
    Column(
        Modifier.fillMaxSize().background(Not.colors.background).statusBarsPadding().verticalScroll(rememberScrollState()).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        sizes.forEach { (label, size) ->
            Text(label, style = Not.type.label, color = Not.colors.inkMuted)
            WidgetPreview(kind, null, size, emptyPreferences(), Modifier.fillMaxWidth())
        }
    }
}
