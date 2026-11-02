package dev.lysen.not

import android.app.Application
import dev.lysen.not.core.NotStore
import java.io.File

/** Owns the process-wide store so the activity, and later widgets and tiles, share one replica. */
class NotApp : Application() {
    val store: NotStore by lazy { NotStore(File(filesDir, "not.db").path) }
}
