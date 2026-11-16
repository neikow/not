package dev.lysen.not.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.Image
import androidx.glance.GlanceModifier
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.action.Action
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.size
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.layout.width
import androidx.glance.text.TextAlign
import androidx.glance.text.TextDecoration
import androidx.glance.text.TextStyle
import dev.lysen.not.MainActivity
import dev.lysen.not.NotApp
import dev.lysen.not.R
import dev.lysen.not.SyncWorker
import dev.lysen.not.Tab
import dev.lysen.not.core.NoteSummary
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore

/** Notes widget: folder shown; "" or missing means all notes. */
val FOLDER_KEY = stringPreferencesKey("folder_id")

/** Note widget: the note shown. */
val NOTE_KEY = stringPreferencesKey("note_id")
private val NOTE_ID = ActionParameters.Key<String>("note_id")
private val LINE = ActionParameters.Key<Int>("line")

// ---- notes list ----

/** Pinned, then recently edited notes of one folder (or all); tap opens one, + writes a new one. */
class NotesWidget : GlanceAppWidget() {
    override val sizeMode = androidx.glance.appwidget.SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent { WidgetLookProvider {
            val folderId = currentState(FOLDER_KEY)?.takeIf { it.isNotEmpty() }
            val version by app.changes.collectAsState()
            val prefs = currentState<androidx.datastore.preferences.core.Preferences>()
            val limit = NotesOptions.limit(prefs)
            val content = remember(folderId, version, limit) { NotesContent.load(app.store, folderId, limit) }
            NotesWidgetContent(content, previews = NotesOptions.previews.isOn(prefs))
        } }
    }
}

class NotesWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = NotesWidget()
}

private data class NotesContent(val folderId: String?, val title: String, val notes: List<NoteSummary>) {
    companion object {
        fun load(store: NotStore, folderId: String?, limit: Int): NotesContent =
            try {
                // The folder may have been deleted on another device: fall back to all notes.
                val folder = folderId?.let { id -> store.folders().firstOrNull { it.id == id } }
                NotesContent(folder?.id, folder?.title ?: "Notes", store.notes(folder?.id).take(limit))
            } catch (e: NotException) {
                logLoadFailure("notes", e)
                NotesContent(null, "Notes", emptyList())
            }
    }
}

@Composable
private fun NotesWidgetContent(content: NotesContent, previews: Boolean) {
    val context = LocalContext.current
    val open = actionStartActivity(MainActivity.openTab(context, Tab.Notes))
    WidgetSurface {
        WidgetHeader(
            title = content.title,
            open = open,
            add = actionStartActivity(MainActivity.newNote(context, content.folderId)),
            addLabel = "New note",
        )
        if (content.notes.isEmpty()) {
            WidgetEmpty(R.drawable.ic_note, "No notes yet", "Tap + to write one.", open)
        } else {
            if (androidx.glance.LocalSize.current.width >= 300.dp) {
                // Wide: two columns of cards.
                WidgetItems(content.notes.chunked(2), { it.first().id.hashCode().toLong() }) { pair ->
                    Row(GlanceModifier.fillMaxWidth()) {
                        Box(GlanceModifier.defaultWeight().padding(end = 3.dp)) { NoteCard(pair[0], previews) }
                        Box(GlanceModifier.defaultWeight().padding(start = 3.dp)) { pair.getOrNull(1)?.let { NoteCard(it, previews) } }
                    }
                }
            } else {
                WidgetItems(content.notes, { it.id.hashCode().toLong() }) { NoteCard(it, previews) }
            }
        }
    }
}

/** A soft card: title (with a pin when pinned) over a two-line preview. */
@Composable
private fun NoteCard(note: NoteSummary, previews: Boolean) {
    val context = LocalContext.current
    Column(GlanceModifier.fillMaxWidth().padding(bottom = 6.wdp)) {
        Column(
            GlanceModifier
                .fillMaxWidth()
                .background(WidgetColors.sunken)
                .cornerRadius(14.dp)
                .padding(horizontal = 12.dp, vertical = 9.wdp)
                .clickable(actionStartActivity(MainActivity.openNote(context, note.id))),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (note.pinned) {
                    Image(
                        ImageProvider(R.drawable.ic_pin),
                        contentDescription = "Pinned",
                        colorFilter = ColorFilter.tint(WidgetColors.accentInk),
                        modifier = GlanceModifier.size(14.dp),
                    )
                    Spacer(GlanceModifier.width(4.dp))
                }
                Text(
                    note.title.ifEmpty { "New note" },
                    maxLines = 1,
                    style = TextStyle(color = WidgetColors.ink, fontSize = 15.wsp, fontWeight = FontWeight.Medium),
                )
            }
            if (previews && note.preview.isNotEmpty()) {
                // As written: line breaks kept, checklist items with a small box.
                dev.lysen.not.previewLines(note.preview).take(3).forEach { line ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (line.box) {
                            Box(GlanceModifier.size(12.dp), contentAlignment = Alignment.Center) {
                                Image(
                                    ImageProvider(if (line.checked) R.drawable.widget_box_done else R.drawable.widget_box),
                                    contentDescription = if (line.checked) "Done" else null,
                                    colorFilter = ColorFilter.tint(if (line.checked) WidgetColors.accent else WidgetColors.inkFaint),
                                    modifier = GlanceModifier.size(12.dp),
                                )
                                if (line.checked) {
                                    Image(
                                        ImageProvider(R.drawable.ic_check),
                                        contentDescription = null,
                                        colorFilter = ColorFilter.tint(WidgetColors.onAccent),
                                        modifier = GlanceModifier.size(9.dp),
                                    )
                                }
                            }
                            Spacer(GlanceModifier.width(6.dp))
                        }
                        Text(
                            line.text,
                            maxLines = 1,
                            style = TextStyle(
                                color = if (line.checked) WidgetColors.inkFaint else WidgetColors.inkMuted,
                                fontSize = 13.wsp,
                                textDecoration = if (line.checked) androidx.glance.text.TextDecoration.LineThrough else null,
                            ),
                        )
                    }
                }
            }
        }
    }
}

// ---- one note ----

/** One note on the home screen; its checklist items can be ticked right there. */
class NoteWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as NotApp
        provideContent { WidgetLookProvider {
            val noteId = currentState(NOTE_KEY)
            val version by app.changes.collectAsState()
            val content = remember(noteId, version) { NoteContent.load(app.store, noteId) }
            NoteWidgetContent(content)
        } }
    }
}

class NoteWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = NoteWidget()
}

/** Ticks or unticks the checklist item on line [LINE] of the note. */
class ToggleNoteCheckbox : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val id = parameters[NOTE_ID] ?: return
        val line = parameters[LINE] ?: return
        val store = (context.applicationContext as NotApp).store
        try {
            val lines = store.noteBody(id).split('\n').toMutableList()
            val match = CHECK_ITEM.find(lines.getOrNull(line) ?: return) ?: return
            val mark = if (match.groupValues[2] == " ") "x" else " "
            lines[line] = lines[line].replaceRange(match.groups[2]!!.range, mark)
            store.setNoteBody(id, lines.joinToString("\n"))
        } catch (_: NotException) {
            return
        }
        SyncWorker.pushSoon(context)
    }
}

private val CHECK_ITEM = Regex("""^(\s*[-*+] \[)([ xX])(] ?)(.*)""")
private val HEADING = Regex("""^#{1,6} (.*)""")
private val BULLET = Regex("""^(\s*)(?:[-*+]|\d+\.) (.*)""")

/** A line of a note as the widget draws it; [index] is its line number in the body. */
private sealed interface NoteLine {
    val index: Int

    data class Heading(override val index: Int, val text: String) : NoteLine
    data class Check(override val index: Int, val checked: Boolean, val text: String, val depth: Int) : NoteLine
    data class Bullet(override val index: Int, val text: String, val depth: Int) : NoteLine
    data class Plain(override val index: Int, val text: String) : NoteLine
    data class Gap(override val index: Int) : NoteLine
}

private data class NoteContent(val id: String?, val title: String, val lines: List<NoteLine>, val missing: Boolean) {
    companion object {
        fun load(store: NotStore, id: String?): NoteContent {
            if (id == null) return NoteContent(null, "Note", emptyList(), missing = true)
            return try {
                val body = store.noteBody(id)
                val all = body.split('\n')
                // The first line is the title; the title bar shows it.
                val first = all.indexOfFirst { it.isNotBlank() }
                val title = store.notes(null).firstOrNull { it.id == id }?.title.orEmpty()
                val lines = all.withIndex().drop(first + 1).take(200).map { (i, raw) -> parse(i, raw) }
                NoteContent(id, title.ifEmpty { "Note" }, lines.dropWhile { it is NoteLine.Gap }, missing = false)
            } catch (e: NotException) {
                logLoadFailure("note", e)
                NoteContent(id, "Note", emptyList(), missing = true)
            }
        }

        private fun parse(index: Int, raw: String): NoteLine {
            CHECK_ITEM.find(raw)?.let {
                val depth = it.groupValues[1].takeWhile { c -> c == ' ' }.length / 2
                return NoteLine.Check(index, it.groupValues[2] != " ", plain(it.groupValues[4]), depth)
            }
            HEADING.find(raw)?.let { return NoteLine.Heading(index, plain(it.groupValues[1])) }
            BULLET.find(raw)?.let {
                return NoteLine.Bullet(index, plain(it.groupValues[2]), it.groupValues[1].length / 2)
            }
            return if (raw.isBlank()) NoteLine.Gap(index) else NoteLine.Plain(index, plain(raw.removePrefix("> ")))
        }

        /** Without inline markers; the widget can't style parts of a line. */
        private fun plain(text: String) = text.replace("**", "").replace("~~", "").replace("`", "")
            .replace(Regex("""(^|\W)[*_](\S.*?\S|\S)[*_](\W|$)"""), "$1$2$3")
    }
}

@Composable
private fun NoteWidgetContent(content: NoteContent) {
    val context = LocalContext.current
    val open = content.id?.let { actionStartActivity(MainActivity.openNote(context, it)) }
    WidgetSurface {
        WidgetHeader(title = content.title, open = open)
        when {
            content.id == null -> WidgetEmpty(R.drawable.ic_note, "No note picked", "Long-press the widget to pick one.")
            content.missing -> WidgetEmpty(R.drawable.ic_note, "Note deleted", "Long-press the widget to pick another.")
            content.lines.isEmpty() -> WidgetEmpty(R.drawable.ic_note, "Nothing else here", "Tap to write more.", open)
            else -> WidgetItems(content.lines, { it.index.toLong() }) { line -> NoteLineView(content.id, line, open!!) }
        }
    }
}

/** Text lines sit 6dp in, in line with the title; checklist marks sit in the same column as a box's. */
@Composable
private fun NoteLineView(noteId: String, line: NoteLine, open: Action) {
    val text = TextStyle(color = WidgetColors.ink, fontSize = 14.wsp)
    when (line) {
        is NoteLine.Check -> Row(
            GlanceModifier.fillMaxWidth().padding(start = (line.depth * 20).dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            WidgetCheck(
                checked = line.checked,
                onClick = actionRunCallback<ToggleNoteCheckbox>(actionParametersOf(NOTE_ID to noteId, LINE to line.index)),
                size = 18.dp,
                box = true,
                label = line.text,
            )
            Spacer(GlanceModifier.width(4.dp))
            Text(
                line.text,
                maxLines = 3,
                style = if (line.checked) text.copy(color = WidgetColors.inkFaint, textDecoration = TextDecoration.LineThrough) else text,
                modifier = GlanceModifier.defaultWeight().padding(vertical = 6.dp).clickable(open),
            )
        }
        is NoteLine.Heading -> Text(
            line.text,
            style = text.copy(fontSize = 16.wsp, fontWeight = FontWeight.Bold),
            modifier = GlanceModifier.fillMaxWidth().padding(start = 6.dp, top = 6.dp, bottom = 2.dp).clickable(open),
        )
        is NoteLine.Bullet -> Row(
            GlanceModifier.fillMaxWidth().padding(start = (line.depth * 20).dp).clickable(open),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Dot centred where a box would be (36dp column), rows as tall as checklist rows, so lists line up.
            Text(
                "•",
                style = text.copy(color = WidgetColors.accentInk, fontSize = 20.wsp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center),
                modifier = GlanceModifier.width(36.dp),
            )
            Spacer(GlanceModifier.width(4.dp))
            Text(line.text, maxLines = 3, style = text, modifier = GlanceModifier.defaultWeight().padding(vertical = 6.dp))
        }
        is NoteLine.Plain -> Text(
            line.text,
            style = text,
            modifier = GlanceModifier.fillMaxWidth().padding(start = 6.dp, bottom = 2.dp).clickable(open),
        )
        is NoteLine.Gap -> Spacer(GlanceModifier.height(8.dp))
    }
}

