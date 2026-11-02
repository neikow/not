package dev.lysen.not

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import dev.lysen.not.core.NotException

/** A one-field "add task" dialog over whatever is on screen, opened from the widget's + button. */
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
    val keyboard = LocalSoftwareKeyboardController.current
    LaunchedEffect(Unit) {
        focus.requestFocus()
        keyboard?.show()
    }
    val submit = { if (text.isNotBlank()) onAdd(text) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Add to $listTitle") },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                placeholder = { Text("Task") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Sentences,
                    imeAction = ImeAction.Done,
                ),
                keyboardActions = KeyboardActions(onDone = { submit() }),
                modifier = Modifier.fillMaxWidth().focusRequester(focus),
            )
        },
        confirmButton = { TextButton(onClick = submit, enabled = text.isNotBlank()) { Text("Add") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
