package dev.lysen.not

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.OutputTransformation
import androidx.compose.foundation.text.input.TextFieldBuffer
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.NotException
import dev.lysen.not.core.NotStore
import dev.lysen.not.core.ParsedTask
import dev.lysen.not.core.ParsedToken
import dev.lysen.not.core.TaskList
import dev.lysen.not.core.TokenKind
import dev.lysen.not.core.dueLabel
import dev.lysen.not.core.repeatLabel
import dev.lysen.not.ui.Icon
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.NotColors
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import java.time.LocalDate

/** Today as the core reads dates, asked fresh so a screen left open overnight moves on. */
fun today(): String = LocalDate.now().toString()

/** "9:00", "18:30". */
fun clockLabel(minutes: UInt): String = "%d:%02d".format(minutes.toInt() / 60, minutes.toInt() % 60)

/** "Tomorrow 9:00 · Daily". */
fun scheduleLabel(due: String?, time: UInt?, repeat: String?): String? {
    due ?: return null
    val date = try {
        dueLabel(due, time, today())
    } catch (_: NotException) {
        due
    }
    return listOfNotNull(date, repeat?.let(::repeatLabel)).joinToString(" · ")
}

/** High, medium, low: a red, an amber and a blue that read on both themes. */
fun NotColors.priority(priority: UByte): Color? = when (priority.toInt()) {
    3 -> danger
    2 -> Color(0xFFE29A2D)
    1 -> Color(0xFF5B8DEF)
    else -> null
}

fun priorityName(priority: UByte): String = when (priority.toInt()) {
    3 -> "High"
    2 -> "Medium"
    1 -> "Low"
    else -> "None"
}

/**
 * Quick-add input as the user types it: the text, the core's reading of it, and the recognised
 * words the user tapped away to keep as plain text.
 */
@Stable
class TaskDraft(private val store: NotStore) {
    val input = TextFieldState()
    var dismissed by mutableStateOf(emptyList<String>())
        private set

    val parsed: ParsedTask? by derivedStateOf {
        val text = input.text.toString()
        if (text.isBlank()) {
            null
        } else {
            try {
                store.parseTask(text, today(), dismissed)
            } catch (_: NotException) {
                null
            }
        }
    }

    val isBlank: Boolean get() = input.text.isBlank()
    val text: String get() = input.text.toString()

    fun dismiss(token: ParsedToken) {
        dismissed = dismissed + token.text
    }

    fun clear() {
        input.clearText()
        dismissed = emptyList()
    }
}

@Composable
fun rememberTaskDraft(): TaskDraft {
    val app = LocalContext.current.applicationContext as NotApp
    return remember { TaskDraft(app.store) }
}

/** Tints what the parser recognised, so the user sees which words become dates and tags. */
private class TokenHighlight(private val draft: TaskDraft, private val colors: NotColors) : OutputTransformation {
    override fun TextFieldBuffer.transformOutput() {
        for (token in draft.parsed?.tokens.orEmpty()) {
            val (start, end) = token.start.toInt() to token.end.toInt()
            if (end <= length) addStyle(SpanStyle(color = colors.accentInk, background = colors.accentSoft), start, end)
        }
    }
}

/** The field of a task draft: highlights recognised words; Done submits. */
@Composable
fun TaskDraftField(
    draft: TaskDraft,
    placeholder: String,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
    style: TextStyle = Not.type.body,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val colors = Not.colors
    val highlight = remember(draft, colors) { TokenHighlight(draft, colors) }
    BasicTextField(
        state = draft.input,
        lineLimits = TextFieldLineLimits.SingleLine,
        textStyle = style.merge(color = colors.ink),
        cursorBrush = SolidColor(colors.accentInk),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
        onKeyboardAction = { onSubmit() },
        outputTransformation = highlight,
        modifier = modifier,
        decorator = { inner ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                leading?.invoke()
                Box(Modifier.weight(1f).padding(vertical = 4.dp)) {
                    if (draft.input.text.isEmpty()) Text(placeholder, style = style, color = colors.inkFaint, maxLines = 1)
                    inner()
                }
                trailing?.invoke()
            }
        },
    )
}

/** What the draft will set, one chip per recognised word; ✕ keeps that word as text instead. */
@Composable
fun TokenChips(draft: TaskDraft, lists: List<TaskList>, modifier: Modifier = Modifier) {
    val parsed = draft.parsed ?: return
    if (parsed.tokens.isEmpty()) return
    Row(modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        for (token in parsed.tokens) {
            val (icon, label) = tokenLabel(token, parsed, lists)
            val tint = if (token.kind == TokenKind.PRIORITY) Not.colors.priority(parsed.priority) else null
            Chip(icon, label, tint = tint, onRemove = { draft.dismiss(token) })
        }
    }
}

private fun tokenLabel(token: ParsedToken, parsed: ParsedTask, lists: List<TaskList>): Pair<Int, String> = when (token.kind) {
    TokenKind.DATE -> R.drawable.ic_calendar to (scheduleLabel(parsed.due, null, null) ?: token.text)
    TokenKind.TIME -> R.drawable.ic_schedule to (parsed.dueTime?.let(::clockLabel) ?: token.text)
    TokenKind.REPEAT -> R.drawable.ic_repeat to (parsed.repeat?.let(::repeatLabel) ?: token.text)
    TokenKind.PRIORITY -> R.drawable.ic_flag to "${priorityName(parsed.priority)} priority"
    TokenKind.LIST -> R.drawable.ic_list to (lists.firstOrNull { it.id == parsed.listId }?.title ?: token.text)
}

@Composable
fun Chip(icon: Int, label: String, modifier: Modifier = Modifier, tint: Color? = null, onClick: (() -> Unit)? = null, onRemove: (() -> Unit)? = null) {
    val colors = Not.colors
    val ink = tint ?: colors.accentInk
    Row(
        modifier
            .clip(CircleShape)
            .background(colors.accentSoft)
            .then(if (onClick != null) Modifier.pressable(onClick, pressedScale = 0.96f) else Modifier)
            .heightIn(min = 32.dp)
            .padding(start = 10.dp, end = if (onRemove != null) 4.dp else 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = ink, size = 16.dp)
        Spacer(Modifier.width(6.dp))
        Text(label, style = Not.type.caption.merge(color = ink), maxLines = 1)
        if (onRemove != null) {
            Box(Modifier.size(28.dp).clip(CircleShape).pressable(onRemove, onClickLabel = "Keep “$label” as text"), contentAlignment = Alignment.Center) {
                Icon(R.drawable.ic_close, "Keep as text", tint = ink.copy(alpha = 0.7f), size = 14.dp)
            }
        }
    }
}

/**
 * The floating "new task" card above the keyboard. Words like "tomorrow 9am", "every mon",
 * "#groceries" and "!1" light up as they're recognised and show as chips underneath.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TaskComposer(placeholder: String, lists: List<TaskList>, onAdd: (TaskDraft) -> Unit, onClose: () -> Unit, modifier: Modifier = Modifier) {
    val draft = rememberTaskDraft()
    val colors = Not.colors
    val focus = remember { FocusRequester() }
    val imeVisible = WindowInsets.isImeVisible
    var hadKeyboard by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { focus.requestFocus() }
    LaunchedEffect(imeVisible) {
        if (imeVisible) hadKeyboard = true else if (hadKeyboard && draft.isBlank) onClose()
    }
    BackHandler(onBack = onClose)
    fun submit() {
        if (draft.isBlank) return
        onAdd(draft)
        draft.clear()
    }
    Column(
        modifier
            .padding(horizontal = 12.dp)
            .fillMaxWidth()
            .shadow(14.dp, RoundedCornerShape(24.dp), ambientColor = Color.Black.copy(alpha = 0.14f), spotColor = Color.Black.copy(alpha = 0.14f))
            .clip(RoundedCornerShape(24.dp))
            .background(colors.surface)
            .padding(8.dp),
    ) {
        TaskDraftField(
            draft,
            placeholder,
            onSubmit = ::submit,
            modifier = Modifier.fillMaxWidth().focusRequester(focus),
            leading = {
                Spacer(Modifier.width(4.dp))
                Box(Modifier.size(22.dp).border(1.75.dp, colors.priority(draft.parsed?.priority ?: 0u) ?: colors.inkFaint, CircleShape))
            },
            trailing = {
                val ready = !draft.isBlank
                Box(
                    Modifier
                        .size(36.dp)
                        .clip(CircleShape)
                        .background(if (ready) colors.accent else colors.sunken)
                        .pressable(::submit, enabled = ready),
                    contentAlignment = Alignment.Center,
                ) { Icon(R.drawable.ic_arrow_up, "Add", tint = if (ready) colors.onAccent else colors.inkFaint, size = 20.dp) }
            },
        )
        TokenChips(draft, lists, Modifier.padding(start = 40.dp, top = 6.dp, bottom = 2.dp))
    }
}
