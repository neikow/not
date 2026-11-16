package dev.lysen.not.widget

import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.datastore.preferences.core.MutablePreferences
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.lifecycle.lifecycleScope
import dev.lysen.not.NotApp
import dev.lysen.not.NotTheme
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.clip
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore
import kotlinx.coroutines.launch

/** One choice in a widget's setup dialog. */
data class PickerOption(val id: String, val label: String, val depth: Int = 0)

/**
 * Setup for a widget: one choice from a list (a task list, a folder, a note), saved in the widget's
 * Glance state. Shown when placing the widget and when reconfiguring it from the launcher.
 */
abstract class PickerConfigActivity : ComponentActivity() {
    abstract val dialogTitle: String
    abstract val widget: GlanceAppWidget

    abstract fun options(store: NotStore): List<PickerOption>

    abstract fun save(prefs: MutablePreferences, choice: String)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val widgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        // Backing out cancels placing the widget, or leaves a configured one unchanged.
        setResult(RESULT_CANCELED, result(widgetId))
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) return finish()

        val options = try {
            options((application as NotApp).store)
        } catch (_: NotException) {
            emptyList()
        }
        setContent {
            NotTheme {
                Picker(dialogTitle, options, onPick = { choose(widgetId, it) }, onDismiss = ::finish)
            }
        }
    }

    private fun choose(widgetId: Int, choice: String) {
        lifecycleScope.launch {
            val glanceId = GlanceAppWidgetManager(this@PickerConfigActivity).getGlanceIdBy(widgetId)
            updateAppWidgetState(this@PickerConfigActivity, glanceId) { save(it, choice) }
            widget.update(this@PickerConfigActivity, glanceId)
            setResult(RESULT_OK, result(widgetId))
            finish()
        }
    }

    private fun result(widgetId: Int) = Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
}

/** Picks the list a tasks widget shows. Optional when placing it; long-press the widget to change. */
class WidgetConfigActivity : PickerConfigActivity() {
    override val dialogTitle = "Show in widget"
    override val widget = TasksWidget()

    override fun options(store: NotStore) =
        store.views(dev.lysen.not.today()).map { PickerOption(it.id, it.title) }

    override fun save(prefs: MutablePreferences, choice: String) {
        prefs[LIST_KEY] = choice
    }
}

@Composable
private fun Picker(title: String, options: List<PickerOption>, onPick: (String) -> Unit, onDismiss: () -> Unit) {
    CardDialog(
        title = title,
        onDismiss = onDismiss,
        buttons = { Button("Cancel", onDismiss, kind = ButtonKind.Quiet) },
    ) {
        Column(Modifier.heightIn(max = 480.dp).verticalScroll(rememberScrollState())) {
            if (options.isEmpty()) Text("Nothing to choose from yet.", color = Not.colors.inkMuted)
            options.forEach { option ->
                Text(
                    option.label,
                    maxLines = 1,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(CircleShape)
                        .pressable({ onPick(option.id) }, pressedScale = 0.98f)
                        .heightIn(min = 48.dp)
                        .padding(start = (12 + option.depth * 16).dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
                )
            }
        }
    }
}
