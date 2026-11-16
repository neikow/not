package dev.lysen.not

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.NotException
import dev.lysen.not.core.imageMarkup
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Field
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import kotlinx.coroutines.launch

/** What a share becomes, before the user adjusts it. */
data class SharePlan(val asTask: Boolean, val title: String, val body: String)

private val URL = Regex("""https?://\S+""")

/**
 * The smart default: a link or a short single line becomes a task (the link kept in its notes);
 * longer or multi-line text, or anything with pictures, a note.
 */
fun planShare(subject: String?, text: String?, images: Int): SharePlan {
    val text = text.orEmpty().trim()
    val subject = subject.orEmpty().trim()
    val url = URL.find(text)?.value
    val words = text.replace(URL, "").trim().trim('-', '–', '|', ':').trim()
    val lines = text.lines().count { it.isNotBlank() }
    val asTask = images == 0 && (url != null && words.length <= 140 || (lines <= 1 && text.length <= 140))
    if (asTask) {
        val title = when {
            words.isNotEmpty() -> words.lineSequence().first().trim()
            subject.isNotEmpty() -> subject
            url != null -> url.removePrefix("https://").removePrefix("http://").removePrefix("www.").trimEnd('/')
            else -> text
        }
        return SharePlan(asTask = true, title = title, body = url ?: "")
    }
    // A note: the subject (if any) as its first line, then the text as it came.
    val body = listOf(subject.takeIf { it.isNotEmpty() && !text.startsWith(it) }, text.ifEmpty { null })
        .filterNotNull()
        .joinToString("\n")
    return SharePlan(asTask = false, title = "", body = body)
}

/**
 * "Share → not" from any app: a small sheet over it, with the smart default (task or note) one
 * tap from the other, where it goes, and Save. Text, links and pictures.
 */
class ShareActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val subject = intent.getStringExtra(Intent.EXTRA_SUBJECT)
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        val images: List<Uri> = when (intent.action) {
            Intent.ACTION_SEND -> listOfNotNull(intent.parcelable(Intent.EXTRA_STREAM)).filter { intent.type?.startsWith("image/") == true }
            Intent.ACTION_SEND_MULTIPLE -> intent.parcelableList(Intent.EXTRA_STREAM)
            else -> emptyList()
        }
        if (text.isNullOrBlank() && subject.isNullOrBlank() && images.isEmpty()) return finish()
        val plan = planShare(subject, text, images.size)
        val app = application as NotApp
        val views = runCatching { app.store.views(today()) }.getOrDefault(emptyList())
        val folders = runCatching { app.store.folders() }.getOrDefault(emptyList())
        setContent {
            NotTheme {
                val scope = rememberCoroutineScope()
                ShareSheet(
                    plan = plan,
                    images = images.size,
                    views = views.map { it.id to it.title },
                    folders = listOf("" to "Notes") + folders.map { it.id to it.title },
                    onSave = { asTask, title, body, target ->
                        scope.launch {
                            val saved = try {
                                save(app, asTask, title, body, target, images)
                            } catch (e: NotException) {
                                Toast.makeText(this@ShareActivity, e.userMessage, Toast.LENGTH_LONG).show()
                                null
                            }
                            if (saved != null) {
                                Toast.makeText(this@ShareActivity, saved, Toast.LENGTH_SHORT).show()
                                SyncWorker.pushSoon(this@ShareActivity)
                            }
                            finish()
                        }
                    },
                    onDismiss = ::finish,
                )
            }
        }
    }

    /** Another share while this one is still open: start over with it. */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        recreate()
    }

    /** Saves it; returns what to say ("Added to Inbox"). */
    private suspend fun save(app: NotApp, asTask: Boolean, title: String, body: String, target: String, images: List<Uri>): String {
        val store = app.store
        if (asTask) {
            val view = target.ifEmpty { INBOX_ID }
            val id = store.addTask(view, title.ifBlank { body.ifBlank { "Shared" } }, today())
            if (body.isNotBlank()) store.setNotes(id, body)
            return "Added to ${runCatching { store.views(today()).first { it.id == view }.title }.getOrDefault("Inbox")}"
        }
        val pictures = images.mapNotNull { uri -> runCatching { imageMarkup(store.addImage(encodeForNote(this, uri))) }.getOrNull() }
        val text = (listOf(body.ifBlank { if (pictures.isNotEmpty()) "Shared pictures" else "Shared" }) + pictures).joinToString("\n")
        store.createNote(target.ifEmpty { null }, text)
        return "Saved as a note"
    }
}

@Composable
private fun ShareSheet(
    plan: SharePlan,
    images: Int,
    views: List<Pair<String, String>>,
    folders: List<Pair<String, String>>,
    onSave: (asTask: Boolean, title: String, body: String, target: String) -> Unit,
    onDismiss: () -> Unit,
) {
    val colors = Not.colors
    var asTask by remember { mutableStateOf(plan.asTask) }
    var title by remember { mutableStateOf(plan.title.ifEmpty { plan.body.lineSequence().firstOrNull().orEmpty().take(140) }) }
    var body by remember { mutableStateOf(if (plan.asTask) plan.body else plan.body) }
    var view by remember { mutableStateOf(INBOX_ID) }
    var folder by remember { mutableStateOf("") }
    CardDialog(
        title = "Save to not",
        onDismiss = onDismiss,
        buttons = {
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button("Save", { onSave(asTask, title, body, if (asTask) view else folder) })
        },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
            // Task or note: the default picked from what came in, one tap from the other.
            Row(Modifier.fillMaxWidth().clip(CircleShape).background(colors.sunken).padding(4.dp)) {
                listOf(true to "Task", false to "Note").forEach { (task, label) ->
                    val on = task == asTask
                    val bg by animateColorAsState(if (on) colors.accent else colors.sunken, label = "kind")
                    Row(
                        Modifier.weight(1f).heightIn(min = 40.dp).clip(CircleShape).background(bg)
                            .pressable({ if (!on) asTask = task }, role = Role.RadioButton, pressedScale = 0.95f),
                        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(if (task) R.drawable.ic_task_alt else R.drawable.ic_note, null, tint = if (on) colors.onAccent else colors.inkMuted, size = 18.dp)
                        Text(label, style = Not.type.label, color = if (on) colors.onAccent else colors.inkMuted)
                    }
                }
            }
            if (asTask) {
                Field(title, { title = it }, "Task", modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(colors.sunken).padding(horizontal = 14.dp, vertical = 6.dp))
                if (body.isNotBlank()) {
                    Text(body, style = Not.type.caption, color = colors.inkMuted, maxLines = 2)
                }
                Targets(views, view) { view = it }
            } else {
                Field(body, { body = it }, "Note", singleLine = false, minLines = 3, modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(colors.sunken).padding(horizontal = 14.dp, vertical = 6.dp))
                if (images > 0) {
                    Text(if (images == 1) "With 1 picture" else "With $images pictures", style = Not.type.caption, color = colors.inkMuted)
                }
                Targets(folders, folder) { folder = it }
            }
        }
    }
}

/** Where it goes: a row of pills, scrolling sideways. */
@Composable
private fun Targets(options: List<Pair<String, String>>, chosen: String, onPick: (String) -> Unit) {
    val colors = Not.colors
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEach { (id, label) ->
            val on = id == chosen
            Box(
                Modifier.heightIn(min = 36.dp).clip(CircleShape).background(if (on) colors.accent else colors.sunken)
                    .pressable({ onPick(id) }, role = Role.RadioButton, pressedScale = 0.94f)
                    .padding(horizontal = 14.dp, vertical = 8.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(label, style = Not.type.label, color = if (on) colors.onAccent else colors.inkMuted, maxLines = 1)
            }
        }
    }
}

@Suppress("DEPRECATION")
private fun Intent.parcelable(key: String): Uri? =
    if (android.os.Build.VERSION.SDK_INT >= 33) getParcelableExtra(key, Uri::class.java) else getParcelableExtra(key)

@Suppress("DEPRECATION")
private fun Intent.parcelableList(key: String): List<Uri> =
    (if (android.os.Build.VERSION.SDK_INT >= 33) getParcelableArrayListExtra(key, Uri::class.java) else getParcelableArrayListExtra(key)).orEmpty()
