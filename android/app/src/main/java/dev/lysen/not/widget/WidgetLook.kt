package dev.lysen.not.widget

import android.content.Context
import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.glance.LocalContext
import androidx.glance.color.ColorProvider
import androidx.glance.currentState
import androidx.glance.unit.ColorProvider
import dev.lysen.not.ui.DarkColors
import dev.lysen.not.ui.LightColors
import dev.lysen.not.ui.NotColors
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/*
 * How widgets look: defaults for all of them in Settings → Widgets, and per widget overrides in
 * its setup screen (each option can stay on "Default"). Overrides live in the widget's Glance
 * state next to its content settings; defaults in shared preferences.
 */

enum class WidgetBackground(val label: String, val alpha: Float) {
    Solid("Solid", 1f),
    Translucent("Translucent", 0.72f),
    Clear("Clear", 0f),
}

enum class WidgetTextSize(val label: String, val scale: Float) {
    Small("Small", 0.88f),
    Medium("Medium", 1f),
    Large("Large", 1.15f),
}

enum class WidgetTheme(val label: String) {
    System("System"),
    Light("Light"),
    Dark("Dark"),
}

enum class WidgetDensity(val label: String, val scale: Float) {
    Compact("Compact", 0.7f),
    Roomy("Roomy", 1f),
}

data class WidgetLook(
    val background: WidgetBackground = WidgetBackground.Solid,
    val textSize: WidgetTextSize = WidgetTextSize.Medium,
    val theme: WidgetTheme = WidgetTheme.System,
    val density: WidgetDensity = WidgetDensity.Roomy,
    /** The title row (with its + button where there is one). */
    val header: Boolean = true,
)

// Per-widget overrides: absent means "use the default".
val LOOK_BACKGROUND = stringPreferencesKey("look_background")
val LOOK_TEXT_SIZE = stringPreferencesKey("look_text_size")
val LOOK_THEME = stringPreferencesKey("look_theme")
val LOOK_DENSITY = stringPreferencesKey("look_density")
val LOOK_HEADER = stringPreferencesKey("look_header")

private inline fun <reified T : Enum<T>> parse(name: String?): T? = name?.let { n -> enumValues<T>().firstOrNull { it.name == n } }

/** [defaults] with this widget's overrides applied. */
fun Preferences.lookOver(defaults: WidgetLook) = WidgetLook(
    background = parse<WidgetBackground>(this[LOOK_BACKGROUND]) ?: defaults.background,
    textSize = parse<WidgetTextSize>(this[LOOK_TEXT_SIZE]) ?: defaults.textSize,
    theme = parse<WidgetTheme>(this[LOOK_THEME]) ?: defaults.theme,
    density = parse<WidgetDensity>(this[LOOK_DENSITY]) ?: defaults.density,
    header = this[LOOK_HEADER]?.let { it == "on" } ?: defaults.header,
)

/** Sets or clears ("Default") one override. */
fun MutablePreferences.setOverride(key: Preferences.Key<String>, value: String?) {
    if (value == null) remove(key) else this[key] = value
}

/** The defaults for every widget, kept on this phone. */
object WidgetLooks {
    private const val PREFS = "widget_look"
    private val _defaults = MutableStateFlow<WidgetLook?>(null)

    fun defaults(context: Context): StateFlow<WidgetLook?> {
        if (_defaults.value == null) _defaults.value = load(context)
        return _defaults.asStateFlow()
    }

    fun current(context: Context): WidgetLook = defaults(context).value ?: WidgetLook()

    fun save(context: Context, look: WidgetLook) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString("background", look.background.name)
            .putString("text_size", look.textSize.name)
            .putString("theme", look.theme.name)
            .putString("density", look.density.name)
            .putBoolean("header", look.header)
            .apply()
        _defaults.value = look
    }

    private fun load(context: Context): WidgetLook {
        val p = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val d = WidgetLook()
        return WidgetLook(
            background = parse<WidgetBackground>(p.getString("background", null)) ?: d.background,
            textSize = parse<WidgetTextSize>(p.getString("text_size", null)) ?: d.textSize,
            theme = parse<WidgetTheme>(p.getString("theme", null)) ?: d.theme,
            density = parse<WidgetDensity>(p.getString("density", null)) ?: d.density,
            header = p.getBoolean("header", d.header),
        )
    }
}

val LocalWidgetLook = staticCompositionLocalOf { WidgetLook() }

/** Wraps a widget's content: its look is the defaults with its own overrides. */
@Composable
fun WidgetLookProvider(content: @Composable () -> Unit) {
    val context = LocalContext.current
    val defaults by WidgetLooks.defaults(context).collectAsState()
    val look = currentState<Preferences>().lookOver(defaults ?: WidgetLook())
    CompositionLocalProvider(LocalWidgetLook provides look, content = content)
}

/** A font size scaled by the widget's text size. */
val Number.wsp: TextUnit
    @Composable get() = (toFloat() * LocalWidgetLook.current.textSize.scale).sp

/** A spacing scaled by the widget's density. */
val Number.wdp: Dp
    @Composable get() = (toFloat() * LocalWidgetLook.current.density.scale).dp

/** The palette the widget's look asks for, as concrete colors (for bitmaps). */
fun WidgetLook.palette(context: Context): NotColors = when (theme) {
    WidgetTheme.Light -> LightColors
    WidgetTheme.Dark -> DarkColors
    WidgetTheme.System ->
        if (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES) DarkColors else LightColors
}

/** One color of the palette, following the system's light/dark mode unless the look pins one. */
@Composable
internal fun pick(of: NotColors.() -> Color, alpha: Float = 1f): ColorProvider = when (LocalWidgetLook.current.theme) {
    WidgetTheme.System -> ColorProvider(day = LightColors.of().copy(alpha = alpha), night = DarkColors.of().copy(alpha = alpha))
    WidgetTheme.Light -> ColorProvider(LightColors.of().copy(alpha = alpha))
    WidgetTheme.Dark -> ColorProvider(DarkColors.of().copy(alpha = alpha))
}
