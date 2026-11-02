package dev.lysen.not

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalWindowInfo
import dev.lysen.not.core.NotException
import dev.lysen.not.ui.Button
import dev.lysen.not.ui.ButtonKind
import dev.lysen.not.ui.CardDialog
import dev.lysen.not.ui.Field
import kotlinx.coroutines.flow.first

/**
 * A one-field "add task" dialog over whatever is on screen. Opened from the widget's + button, the
 * Quick Settings tile and the app icon's long-press shortcut.
 */
class QuickAddActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val store = (application as NotApp).store
        val lists = try {
            store.lists()
        } catch (_: NotException) {
            emptyList()
        }
        val wanted = intent.getStringExtra(MainActivity.EXTRA_LIST) ?: INBOX_ID
        // The list may have been deleted since the widget last rendered: fall back to the Inbox.
        val list = lists.firstOrNull { it.id == wanted } ?: lists.firstOrNull()
        setContent {
            NotTheme {
                QuickAddDialog(
                    listTitle = list?.title ?: "Inbox",
                    onAdd = { title ->
                        try {
                            store.addTask(list?.id ?: INBOX_ID, title)
                            SyncWorker.pushSoon(this)
                        } catch (e: NotException) {
                            Toast.makeText(this, e.userMessage, Toast.LENGTH_LONG).show()
                        }
                        finish()
                    },
                    onDismiss = ::finish,
                )
            }
        }
    }

    companion object {
        fun intent(context: Context, listId: String): Intent =
            Intent(context, QuickAddActivity::class.java).putExtra(MainActivity.EXTRA_LIST, listId)
    }
}

@Composable
private fun QuickAddDialog(listTitle: String, onAdd: (String) -> Unit, onDismiss: () -> Unit) {
    var text by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val submit = { if (text.isNotBlank()) onAdd(text) }
    CardDialog(
        title = "Add to $listTitle",
        onDismiss = onDismiss,
        buttons = {
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button("Add", submit, enabled = text.isNotBlank())
        },
    ) {
        // The dialog's own window: opened from the Quick Settings shade, it only gets focus once
        // the shade has collapsed, and asking for the keyboard any earlier is ignored.
        val window = LocalWindowInfo.current
        val keyboard = LocalSoftwareKeyboardController.current
        LaunchedEffect(window) {
            snapshotFlow { window.isWindowFocused }.first { it }
            focus.requestFocus()
            keyboard?.show()
        }
        Field(text, { text = it }, "Task", filled = true, onDone = submit, modifier = Modifier.fillMaxWidth().focusRequester(focus))
    }
}
