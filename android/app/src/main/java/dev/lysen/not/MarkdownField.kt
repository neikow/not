package dev.lysen.not

import dev.lysen.not.ui.Text
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.text.TextStyle
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.OutputTransformation
import androidx.compose.foundation.text.input.TextFieldBuffer
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.withTimeoutOrNull
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.ParagraphStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.lysen.not.core.MarkdownSpan
import dev.lysen.not.core.MarkdownStyle
import dev.lysen.not.core.markdownSpans
import dev.lysen.not.ui.Bricolage
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.NotColors

/**
 * A plain-text markdown editor that styles the markdown in place, using the core's spans so notes
 * look the same as on the Mac. The first line reads as the title; checklist boxes and bullets are
 * drawn over their (hidden) markup, and tapping a box ticks it. Return continues lists.
 */
@Composable
fun MarkdownField(state: TextFieldState, modifier: Modifier = Modifier) {
    val colors = Not.colors
    val styling = remember(colors) { MarkdownStyling(colors) }
    val text = state.text.toString()
    val marks = remember(text) { listMarks(markdownSpans(text)) }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    val currentMarks by rememberUpdatedState(marks)

    // The caret never rests inside a box's or bullet's hidden markup.
    LaunchedEffect(state) {
        snapshotFlow { state.selection }.collect { selection ->
            if (!selection.collapsed) return@collect
            snappedCaret(state.text, selection.start)?.let { state.edit { this.selection = TextRange(it) } }
        }
    }

    // Samsung Keyboard sends Return as a key press before it commits the (autocorrected) word it
    // was composing. Taken as it comes, the newline ends the composition, so the word then lands
    // again after it. So a Return key waits a moment for that commit (the field doesn't say whether
    // a word is being composed), then goes in after it.
    val returns = remember { Channel<Unit>(Channel.UNLIMITED) }
    // One at a time, so a second Return doesn't take the first for typing.
    LaunchedEffect(state) {
        for (unit in returns) {
            // Return on the empty title line: today's date becomes the title, then on to the body.
            if (state.fillTitleWithDate()) continue
            val before = state.text.toString()
            val at = state.selection.min
            val after = withTimeoutOrNull(RETURN_WAIT_MS) { snapshotFlow { state.text.toString() }.first { it != before } }
            // Text added right at the caret was typed after Return, so Return goes before it;
            // anything else is the keyboard committing its word, and Return goes after that.
            val typedAhead = after != null && after.length > before.length &&
                after.startsWith(before.substring(0, at)) && after.endsWith(before.substring(at))
            state.typeNewline(if (typedAhead) at else null)
        }
    }
    val heldReturn = Modifier.onPreviewKeyEvent { event ->
        if (event.key != Key.Enter && event.key != Key.NumPadEnter) return@onPreviewKeyEvent false
        if (event.type == KeyEventType.KeyDown) returns.trySend(Unit)
        true
    }

    // The state-based field (not the TextFieldValue one): its input pipeline keeps the keyboard's
    // edits in order, as long as Return waits for the word being composed (above).
    // An empty note is typed at title size, so the caret shows a title is being written, with
    // today's date as the hint (Return keeps it).
    val empty = text.isEmpty()
    BasicTextField(
        state = state,
        inputTransformation = ListEditing,
        outputTransformation = styling,
        textStyle = if (empty) TitleStyle.copy(color = colors.ink) else Not.type.body.copy(color = colors.ink, lineHeight = 26.sp),
        decorator = { inner ->
            Box {
                if (empty) Text(todayTitle(), style = TitleStyle, color = colors.inkFaint, maxLines = 1)
                inner()
            }
        },
        cursorBrush = SolidColor(colors.accentInk),
        onTextLayout = { result -> layout = result() },
        modifier = modifier
            .then(heldReturn)
            // Taps on a box tick it instead of moving the caret there.
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(pass = PointerEventPass.Initial)
                    val text = layout ?: return@awaitEachGesture
                    val box = currentMarks.filterIsInstance<ListMark.Box>()
                        .firstOrNull { text.boxRect(it).inflate(12.dp.toPx()).contains(down.position) }
                        ?: return@awaitEachGesture
                    down.consume()
                    val up = waitForUpOrCancellation(PointerEventPass.Initial) ?: return@awaitEachGesture
                    up.consume()
                    state.toggleBox(box)
                }
            }
            .drawBehind {
                val text = layout ?: return@drawBehind
                for (mark in marks) {
                    when (mark) {
                        is ListMark.Box -> drawBox(text.boxRect(mark), mark.checked, colors)
                        is ListMark.Dot -> {
                            // Where a box's centre would be: half its 3-character markup in.
                            val r = text.getBoundingBox(mark.at)
                            drawCircle(colors.accentInk, 3.dp.toPx(), Offset(r.left + MARKUP_WIDTH.toPx() / 2, r.center.y))
                        }
                    }
                }
            },
    )
}

/** Width of a box's `[ ]` markup: three monospaced characters at 13sp. */
private val MARKUP_WIDTH = (3 * 0.6 * 13).sp

/** A checklist box or bullet the field draws itself; [at] is the first character of its markup. */
private sealed interface ListMark {
    val at: Int

    /** `[ ]` / `[x]` starting at [at]. */
    data class Box(override val at: Int, val checked: Boolean) : ListMark

    /** A `-`, `*` or `+` bullet at [at]. */
    data class Dot(override val at: Int) : ListMark
}

private fun listMarks(spans: List<MarkdownSpan>): List<ListMark> {
    val boxStarts = spans.mapNotNull { (it.style as? MarkdownStyle.Checkbox)?.let { _ -> it.start.toInt() } }.toSet()
    return spans.mapNotNull { span ->
        val style = span.style
        when {
            style is MarkdownStyle.Checkbox -> ListMark.Box(span.start.toInt(), style.checked)
            // Bullets of checklist items are hidden, not drawn; numbers stay text.
            style is MarkdownStyle.Bullet && span.end - span.start == 1u && span.start.toInt() + 2 !in boxStarts ->
                ListMark.Dot(span.start.toInt())
            else -> null
        }
    }
}

/** The square a box is drawn in: line-height tall, centred on its `[ ]`. */
private fun TextLayoutResult.boxRect(box: ListMark.Box): Rect {
    val markup = getBoundingBox(box.at).let { Rect(it.left, it.top, getBoundingBox(box.at + 2).right, it.bottom) }
    val side = markup.height * 0.62f
    return Rect(markup.center - Offset(side / 2, side / 2), Size(side, side))
}

private fun DrawScope.drawBox(rect: Rect, checked: Boolean, colors: NotColors) {
    val corner = CornerRadius(rect.width * 0.25f)
    if (checked) {
        drawRoundRect(colors.accent, rect.topLeft, rect.size, corner)
        val w = rect.width
        val check = androidx.compose.ui.graphics.Path().apply {
            moveTo(rect.left + w * 0.24f, rect.top + w * 0.52f)
            lineTo(rect.left + w * 0.43f, rect.top + w * 0.70f)
            lineTo(rect.left + w * 0.77f, rect.top + w * 0.32f)
        }
        drawPath(check, colors.onAccent, style = Stroke(w * 0.13f, cap = StrokeCap.Round))
    } else {
        val stroke = 2.dp.toPx()
        drawRoundRect(
            colors.inkFaint,
            rect.topLeft + Offset(stroke / 2, stroke / 2),
            Size(rect.width - stroke, rect.height - stroke),
            corner,
            style = Stroke(stroke),
        )
    }
}

private fun TextFieldState.toggleBox(box: ListMark.Box) = edit {
    apply(toggleBox(asCharSequence().toString(), box.at, selection.start, selection.end))
}

/** How long a Return key waits for the keyboard to commit the word it was composing. */
private const val RETURN_WAIT_MS = 60L

/**
 * Return at the caret, as if typed: replaces the selection, then continues lists. With [at], it
 * goes in there instead, before what was typed since, and the caret stays after that.
 */
private fun TextFieldState.typeNewline(at: Int? = null) = edit {
    val start = at ?: selection.min
    val ahead = if (at == null) 0 else selection.min - at
    replace(start, if (at == null) selection.max else start, "\n")
    val change = onNewline(asCharSequence().toString(), start + 1)
    if (change != null) replace(change.start, change.end, change.text)
    selection = TextRange((change?.selStart ?: (start + 1)) + ahead)
}

private fun TextFieldBuffer.apply(change: Change?) {
    change ?: return
    replace(change.start, change.end, change.text)
    selection = TextRange(change.selStart, change.selEnd)
}

/** Styles the markdown in place; the text itself is left alone, so offsets map one to one. */
private class MarkdownStyling(private val colors: NotColors) : OutputTransformation {
    override fun TextFieldBuffer.transformOutput() {
        val raw = asCharSequence().toString()
        val spans = markdownSpans(raw)
        val styled = this
        titleRange(raw, spans)?.let { (start, end) ->
            styled.addStyle(ParagraphStyle(lineHeight = TitleStyle.lineHeight), start, end)
            styled.addStyle(TitleStyle.toSpanStyle(), start, end)
        }
        val hidden = SpanStyle(color = Color.Transparent)
        // Two spaces are too narrow to read as nesting; widen each level to about a box's step.
        var ls = 0
        while (ls <= raw.length) {
            listPrefix(raw, ls)?.takeIf { it.indent.isNotEmpty() }?.let {
                styled.addStyle(SpanStyle(letterSpacing = 8.sp), it.start, it.start + it.indent.length)
            }
            ls = lineEnd(raw, ls) + 1
        }
        for (mark in listMarks(spans)) {
            when (mark) {
                // The bullet and its space collapse, so boxes line up with bullets; the box is drawn
                // over its markup, which is monospaced so ticking it doesn't move the text.
                is ListMark.Box -> {
                    styled.addStyle(hidden.copy(fontSize = 1.sp), (mark.at - 2).coerceAtLeast(0), mark.at)
                    styled.addStyle(hidden.copy(fontFamily = FontFamily.Monospace, fontSize = 13.sp), mark.at, mark.at + 3)
                }
                // As wide as a box's markup, so bulleted and checklist text line up.
                is ListMark.Dot -> styled.addStyle(
                    hidden.copy(fontFamily = FontFamily.Monospace, fontSize = 13.sp, letterSpacing = 37.sp),
                    mark.at,
                    mark.at + 1,
                )
            }
        }
        for (span in spans) {
            if (span.style is MarkdownStyle.Checkbox) continue
            if (span.style is MarkdownStyle.Bullet && span.end - span.start == 1u) continue
            styled.addStyle(style(span.style), span.start.toInt(), span.end.toInt())
        }
    }

    private fun style(style: MarkdownStyle): SpanStyle = when (style) {
        is MarkdownStyle.Heading -> SpanStyle(
            fontFamily = Bricolage,
            fontSize = when (style.level.toInt()) {
                1 -> 26.sp
                2 -> 21.sp
                else -> 18.sp
            },
            fontWeight = if (style.level.toInt() == 3) FontWeight.SemiBold else FontWeight.Bold,
        )
        MarkdownStyle.Bold -> SpanStyle(fontWeight = FontWeight.Bold)
        MarkdownStyle.Italic -> SpanStyle(fontStyle = FontStyle.Italic)
        MarkdownStyle.Code -> SpanStyle(fontFamily = FontFamily.Monospace, background = colors.sunken)
        MarkdownStyle.Strike -> SpanStyle(textDecoration = TextDecoration.LineThrough)
        MarkdownStyle.Quote -> SpanStyle(fontStyle = FontStyle.Italic, color = colors.inkMuted)
        MarkdownStyle.Bullet -> SpanStyle(color = colors.accentInk, fontWeight = FontWeight.Bold)
        is MarkdownStyle.Checkbox -> SpanStyle()
        MarkdownStyle.Done -> SpanStyle(textDecoration = TextDecoration.LineThrough, color = colors.inkMuted)
        MarkdownStyle.Syntax -> SpanStyle(color = colors.inkFaint)
    }
}

/** How a note's first line, its title, reads. */
private val TitleStyle = TextStyle(fontFamily = Bricolage, fontSize = 26.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.3).sp, lineHeight = 36.sp)

/** "Wednesday 11 November": what an untitled note is called when Return is pressed on its title. */
private fun todayTitle(): String =
    java.time.LocalDate.now().format(java.time.format.DateTimeFormatter.ofPattern("EEEE d MMMM", java.util.Locale.getDefault()))

/** With the caret on an empty first line: puts today's date there and starts the next line. */
private fun TextFieldState.fillTitleWithDate(): Boolean {
    val raw = text.toString()
    val end = raw.indexOf('\n').let { if (it < 0) raw.length else it }
    if (raw.substring(0, end).isNotBlank() || selection.min > end) return false
    val title = todayTitle()
    edit {
        replace(0, end, title + if (end == raw.length) "\n" else "")
        selection = TextRange(title.length + 1)
    }
    return true
}

/** The first non-blank line, which titles the note, unless it is already a heading or list item. */
private fun titleRange(text: String, spans: List<MarkdownSpan>): Pair<Int, Int>? {
    var start = 0
    while (start < text.length) {
        val end = text.indexOf('\n', start).let { if (it < 0) text.length else it }
        if (text.substring(start, end).isNotBlank()) {
            val marked = spans.any {
                it.start.toInt() == start && (it.style is MarkdownStyle.Heading || it.style is MarkdownStyle.Bullet ||
                    it.style == MarkdownStyle.Syntax)
            }
            return if (marked) null else start to end
        }
        start = end + 1
    }
    return null
}

// ---- editing helpers ----

private val LINE_PREFIX = Regex("""^(#{1,6} |> |[-*+] \[[ xX]] |[-*+] |\d+\. )?""")

/** List rules for what the user types: Return continues lists, Backspace takes markers out whole. */
private object ListEditing : InputTransformation {
    override fun TextFieldBuffer.transformInput() {
        val original = originalText.toString()
        val text = asCharSequence().toString()
        val was = originalSelection
        val caret = selection.start
        if (!selection.collapsed) return
        val deletedOneBack = was.collapsed && caret == was.start - 1 && text.length == original.length - 1 &&
            text == original.removeRange(caret, was.start)
        // A newline before the caret that wasn't there before. The keyboard may commit the word it
        // was composing in the same edit, so the length can grow by more than one.
        val typedNewline = caret > 0 && text[caret - 1] == '\n' && text.count { it == '\n' } > original.count { it == '\n' }
        when {
            deletedOneBack -> onBackspace(original, was.start)?.let { revertAllChanges(); apply(it) }
            typedNewline -> apply(onNewline(text, caret))
        }
    }
}

/** Nests the list items on the selected lines one level deeper, or ([deeper] false) back out. */
fun TextFieldState.indent(deeper: Boolean) = edit {
    apply(indentLines(asCharSequence().toString(), selection.min, selection.max, deeper))
}

/** Wraps the selection (or an empty pair at the caret) in [marker]. */
fun TextFieldState.wrap(marker: String) = edit {
    val (start, end) = selection.min to selection.max
    replace(end, end, marker)
    replace(start, start, marker)
    selection = TextRange(start + marker.length, end + marker.length)
}

/** Rewrites the block marker of the caret's line with [next] (given the current marker, "" if none). */
private fun TextFieldState.withLinePrefix(next: (String) -> String) = edit {
    val text = asCharSequence().toString()
    val (start, end) = selection.start to selection.end
    val prefixAt = lineStart(text, start).let { ls -> ls + text.drop(ls).takeWhile { it == ' ' || it == '\t' }.length }
    val current = LINE_PREFIX.find(text.substring(prefixAt))?.value.orEmpty()
    val replacement = next(current)
    replace(prefixAt, prefixAt + current.length, replacement)
    val shift = replacement.length - current.length
    selection = TextRange((start + shift).coerceAtLeast(prefixAt), (end + shift).coerceAtLeast(prefixAt))
}

/** None → # → ## → ### → none. */
fun TextFieldState.cycleHeading() = withLinePrefix {
    when (it) {
        "# " -> "## "
        "## " -> "### "
        "### " -> ""
        else -> "# "
    }
}

/** None → unchecked → checked → none. */
fun TextFieldState.cycleChecklist() = withLinePrefix {
    when {
        it.endsWith("[ ] ") -> "- [x] "
        it.endsWith("[x] ") || it.endsWith("[X] ") -> ""
        else -> "- [ ] "
    }
}

fun TextFieldState.toggleBullet() = withLinePrefix { if (it == "- ") "" else "- " }
