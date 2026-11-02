package dev.lysen.not.widget

import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.lifecycle.lifecycleScope
import dev.lysen.not.NotApp
import dev.lysen.not.NotTheme
import dev.lysen.not.core.NotException
import dev.lysen.not.core.TaskList
import kotlinx.coroutines.launch

/** Picks the list a tasks widget shows. Optional when placing it; long-press the widget to change. */
class WidgetConfigActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val widgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        // Backing out cancels placing the widget, or leaves a configured one unchanged.
        setResult(RESULT_CANCELED, result(widgetId))
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) return finish()

        val lists = try {
            (application as NotApp).store.lists()
        } catch (_: NotException) {
            emptyList()
        }
        setContent {
            NotTheme {
                ListChooser(lists, onPick = { choose(widgetId, it) }, onDismiss = ::finish)
            }
        }
    }

    private fun choose(widgetId: Int, listId: String) {
        lifecycleScope.launch {
            val glanceId = GlanceAppWidgetManager(this@WidgetConfigActivity).getGlanceIdBy(widgetId)
            updateAppWidgetState(this@WidgetConfigActivity, glanceId) { it[LIST_KEY] = listId }
            TasksWidget().update(this@WidgetConfigActivity, glanceId)
            setResult(RESULT_OK, result(widgetId))
            finish()
        }
    }

    private fun result(widgetId: Int) = Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
}

@Composable
private fun ListChooser(lists: List<TaskList>, onPick: (String) -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Show in widget") },
        text = {
            Column {
                lists.forEach { list ->
                    TextButton(onClick = { onPick(list.id) }, modifier = Modifier.fillMaxWidth()) {
                        Text(list.title, modifier = Modifier.fillMaxWidth())
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
