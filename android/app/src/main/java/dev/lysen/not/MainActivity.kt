package dev.lysen.not

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.ui.platform.LocalContext

class MainActivity : ComponentActivity() {
    private val viewModel: TasksViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        handleDevIntent(intent)
        setContent {
            MaterialTheme(colorScheme = dynamicLightColorScheme(LocalContext.current)) {
                TasksScreen(viewModel)
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleDevIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        viewModel.syncNow()
    }

    /**
     * Debug builds only, for scripted testing:
     * `adb shell am start -n dev.lysen.not/.MainActivity --es server URL --es token T --es add TITLE`.
     * Never honoured in release builds: any app could otherwise point sync at another server.
     */
    private fun handleDevIntent(intent: Intent) {
        if (!BuildConfig.DEBUG) return
        val server = intent.getStringExtra("server")
        val token = intent.getStringExtra("token")
        if (server != null && token != null) viewModel.connect(server, token)
        intent.getStringExtra("add")?.let(viewModel::addTask)
    }
}
