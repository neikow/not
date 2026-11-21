package dev.lysen.not

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.Release
import dev.lysen.not.core.allReleases
import dev.lysen.not.core.whatsNew
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Sheet
import dev.lysen.not.ui.Text

/** What changed since the version last opened: shown once after an update, never on a fresh install. */
object WhatsNew {
    private const val KEY = "seen_version"

    /**
     * Releases to show now, and the current version marked as seen. A device that had the app
     * before this existed has no version stored but isn't fresh: it gets everything up to now.
     */
    fun pending(context: Context, freshInstall: Boolean, current: String = BuildConfig.VERSION_NAME): List<Release> {
        val prefs = context.getSharedPreferences("app", Context.MODE_PRIVATE)
        val seen = prefs.getString(KEY, null) ?: if (freshInstall) current else "0.0.0"
        prefs.edit().putString(KEY, current).apply()
        return runCatching { whatsNew(seen, current) }.getOrDefault(emptyList())
    }

    fun all(): List<Release> = runCatching { allReleases() }.getOrDefault(emptyList())
}

@Composable
fun WhatsNewSheet(releases: List<Release>, title: String = "What's new", onDismiss: () -> Unit) {
    Sheet(onDismiss) { close ->
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 26.dp).padding(bottom = 20.dp)) {
            Text(title, style = Not.type.title, modifier = Modifier.padding(top = 4.dp, bottom = 4.dp))
            releases.forEach { release ->
                Text("Version ${release.version}", style = Not.type.label, color = Not.colors.inkMuted, modifier = Modifier.padding(top = 16.dp, bottom = 6.dp))
                release.highlights.forEach { line ->
                    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text("•", style = Not.type.body, color = Not.colors.accentInk)
                        Text(line, style = Not.type.body)
                    }
                }
            }
            Button("Got it", close, Modifier.fillMaxWidth().padding(top = 20.dp))
        }
    }
}
