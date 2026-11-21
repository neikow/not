package dev.lysen.not

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import dev.lysen.not.core.ImportReport
import dev.lysen.not.core.ImportSource
import dev.lysen.not.core.NotException
import java.util.TimeZone

/** Todoist and TickTick exports, from a file the user picked (Settings → Import, export and backup; first run). */
object TaskImport {
    /** What the file picker offers: CSVs, and anything for providers that don't type them. */
    val types = arrayOf("text/csv", "text/comma-separated-values", "text/plain", "*/*")

    /** Reads and imports the file; the message says what came in, or why nothing did. */
    fun run(context: Context, uri: Uri): Result<ImportReport> = runCatching {
        val app = context.applicationContext as NotApp
        val text = context.contentResolver.openInputStream(uri)!!.use { it.readBytes().toString(Charsets.UTF_8) }
        val offset = TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60_000
        app.store.importTasks(text, displayName(context, uri), today(), offset)
    }

    fun message(report: ImportReport): String {
        val from = when (report.source) {
            ImportSource.TODOIST -> "Todoist"
            ImportSource.TICK_TICK -> "TickTick"
        }
        val added = if (report.added == 1u) "1 task" else "${report.added} tasks"
        val skipped = when (report.skippedDone) {
            0u -> ""
            1u -> ", left out 1 done"
            else -> ", left out ${report.skippedDone} done"
        }
        return "Added $added from $from$skipped"
    }

    fun failure(error: Throwable): String =
        if (error is NotException) error.userMessage.replaceFirstChar { it.uppercase() } else "Couldn't read that file"

    /** The picked file's name ("Groceries.csv"), which names a Todoist project. */
    private fun displayName(context: Context, uri: Uri): String =
        runCatching {
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        }.getOrNull() ?: uri.lastPathSegment.orEmpty()
}
