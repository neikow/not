package dev.lysen.not

import android.content.Context
import java.io.File

/**
 * Crashes, kept on the phone only: an uncaught exception writes what happened to a file, and the
 * next launch offers to report it (a GitHub bug report the user sees before sending) or forget it.
 * Nothing is sent by itself, and there is no crash service.
 */
object CrashReports {
    private const val FILE = "last-crash.txt"

    /** Saves the next crash, then lets Android's own handler end the app as usual. */
    fun install(context: Context) {
        val file = File(context.filesDir, FILE)
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            runCatching { file.writeText(describe(thread.name, error, BuildConfig.VERSION_NAME, Links.device())) }
            previous?.uncaughtException(thread, error)
        }
    }

    /** The saved crash, if the app crashed since it last asked. */
    fun pending(context: Context): String? = File(context.filesDir, FILE).takeIf { it.exists() }?.readText()

    fun forget(context: Context) {
        File(context.filesDir, FILE).delete()
    }

    /**
     * What a report holds: version, device, thread, and the stack trace with its causes. Only code
     * locations and exception messages, which may quote what the app was doing but hold no data
     * by design. The innermost cause first, since that's where it went wrong.
     */
    internal fun describe(thread: String, error: Throwable, version: String, device: String): String {
        val causes = generateSequence(error) { it.cause }.take(5).toList().reversed()
        val trace = causes.joinToString("\nwhich caused:\n") { e ->
            (listOf(e.toString()) + e.stackTrace.take(25).map { "    at $it" }).joinToString("\n")
        }
        return "not $version on $device, thread \"$thread\"\n\n$trace"
    }
}
