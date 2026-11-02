package dev.lysen.not

import android.app.StatusBarManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.lifecycle.lifecycleScope
import dev.lysen.not.widget.TasksWidgetReceiver
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val viewModel: TasksViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) handleIntent(intent)
        setContent {
            NotTheme { TasksScreen(viewModel) }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent) {
        intent.getStringExtra(EXTRA_LIST)?.let(viewModel::select)
        handleDevIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        viewModel.syncNow()
    }

    /**
     * Debug builds only, for scripted testing:
     * `adb shell am start -n dev.lysen.not/.MainActivity --es server URL --es code CODE --es add TITLE`,
     * `--ez pinWidget true` asks the launcher to place the tasks widget, `--ez addTile true` offers the
     * Quick Settings tile (Android 13+).
     * Never honoured in release builds: any app could otherwise pair this device with its own server.
     */
    private fun handleDevIntent(intent: Intent) {
        if (!BuildConfig.DEBUG) return
        val server = intent.getStringExtra("server")
        val code = intent.getStringExtra("code")
        if (server != null && code != null) viewModel.pair(server, code, viewModel.defaultDeviceName)
        intent.getStringExtra("add")?.let(viewModel::addTask)
        if (intent.getBooleanExtra("pinWidget", false)) {
            lifecycleScope.launch {
                GlanceAppWidgetManager(this@MainActivity).requestPinGlanceAppWidget(TasksWidgetReceiver::class.java)
            }
        }
        if (intent.getBooleanExtra("addTile", false) && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            getSystemService(StatusBarManager::class.java).requestAddTileService(
                ComponentName(this, QuickAddTileService::class.java),
                getString(R.string.add_task),
                Icon.createWithResource(this, R.drawable.ic_add_task),
                mainExecutor,
            ) {}
        }
    }

    companion object {
        /** List id to show (or, for quick add, to add to). */
        const val EXTRA_LIST = "dev.lysen.not.LIST"

        fun openList(context: Context, listId: String): Intent =
            Intent(context, MainActivity::class.java).putExtra(EXTRA_LIST, listId)
    }
}
