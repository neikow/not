package dev.lysen.not

import android.app.StatusBarManager
import android.content.ComponentName
import android.content.Context
import android.graphics.drawable.Icon
import android.os.Build
import androidx.glance.appwidget.GlanceAppWidgetManager
import dev.lysen.not.widget.TasksWidgetReceiver

/** System dialogs that place the app's shortcuts, offered from Settings. */
object Shortcuts {
    /** Asks the launcher to place the tasks widget. False if the launcher doesn't support it. */
    suspend fun requestWidget(context: Context): Boolean =
        GlanceAppWidgetManager(context).requestPinGlanceAppWidget(TasksWidgetReceiver::class.java)

    val canRequestTile: Boolean get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU

    /** Offers the "Add task" Quick Settings tile (Android 13+). */
    fun requestTile(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        context.getSystemService(StatusBarManager::class.java).requestAddTileService(
            ComponentName(context, QuickAddTileService::class.java),
            context.getString(R.string.add_task),
            Icon.createWithResource(context, R.drawable.ic_add_task),
            context.mainExecutor,
        ) {}
    }
}
