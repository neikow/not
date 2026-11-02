package dev.lysen.not

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.ParagraphStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
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
fun MarkdownField(value: TextFieldValue, onValueChange: (TextFieldValue) -> Unit, modifier: Modifier = Modifier) {
    val colors = Not.colors
    val transformation = remember(colors) { MarkdownTransformation(colors) }
    val marks = remember(value.text) { listMarks(markdownSpans(value.text)) }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    val current by rememberUpdatedState(value)
    val change by rememberUpdatedState(onValueChange)
    val currentMarks by rememberUpdatedState(marks)

    BasicTextField(
        value = value,
        onValueChange = { onValueChange(continueList(value, it)) },
        textStyle = Not.type.body.copy(color = colors.ink, lineHeight = 26.sp),
        cursorBrush = SolidColor(colors.accentInk),
        visualTransformation = transformation,
        onTextLayout = { layout = it },
        modifier = modifier
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
                    change(current.toggleBox(box))
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

private fun TextFieldValue.toggleBox(box: ListMark.Box): TextFieldValue {
    val i = box.at + 1
    if (i >= text.length || text[box.at] != '[') return this
    return copy(text = text.substring(0, i) + (if (box.checked) ' ' else 'x') + text.substring(i + 1))
}

private class MarkdownTransformation(private val colors: NotColors) : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val raw = text.text
        val spans = markdownSpans(raw)
        val styled = AnnotatedString.Builder(raw)
        titleRange(raw, spans)?.let { (start, end) ->
            styled.addStyle(ParagraphStyle(lineHeight = 36.sp), start, end)
            styled.addStyle(SpanStyle(fontFamily = Bricolage, fontSize = 26.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.3).sp), start, end)
        }
        val hidden = SpanStyle(color = Color.Transparent)
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
        return TransformedText(styled.toAnnotatedString(), OffsetMapping.Identity)
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

private val LIST_ITEM = Regex("""^(\s*)([-*+] \[[ xX]] |[-*+] |(\d+)\. )""")
private val LINE_PREFIX = Regex("""^(#{1,6} |> |[-*+] \[[ xX]] |[-*+] |\d+\. )?""")

/** A typed newline after a list item starts the next item; on an empty item it ends the list. */
private fun continueList(old: TextFieldValue, new: TextFieldValue): TextFieldValue {
    val caret = new.selection.start
    val typedNewline = new.text.length == old.text.length + 1 && new.selection.collapsed &&
        caret > 0 && new.text[caret - 1] == '\n'
    if (!typedNewline) return new
    val lineStart = new.text.lastIndexOf('\n', caret - 2) + 1
    val line = new.text.substring(lineStart, caret - 1)
    val match = LIST_ITEM.find(line) ?: return new
    val marker = match.value
    if (line == marker) {
        // Empty item: remove its marker and the newline.
        val text = new.text.removeRange(lineStart, caret)
        return TextFieldValue(text, TextRange(lineStart))
    }
    val indent = match.groupValues[1]
    val next = when {
        match.groupValues[3].isNotEmpty() -> "$indent${match.groupValues[3].toInt() + 1}. "
        "[" in marker -> "$indent${marker.trim()[0]} [ ] "
        else -> marker
    }
    return new.insert(caret, next)
}

private fun TextFieldValue.insert(at: Int, s: String) =
    TextFieldValue(text.substring(0, at) + s + text.substring(at), TextRange(at + s.length))

/** Wraps the selection (or an empty pair at the caret) in [marker]. */
fun TextFieldValue.wrap(marker: String): TextFieldValue {
    val (start, end) = selection.min to selection.max
    val text = text.substring(0, start) + marker + text.substring(start, end) + marker + text.substring(end)
    return TextFieldValue(text, TextRange(start + marker.length, end + marker.length))
}

/** Rewrites the block marker of the caret's line with [next] (given the current marker, "" if none). */
private fun TextFieldValue.withLinePrefix(next: (String) -> String): TextFieldValue {
    val lineStart = text.lastIndexOf('\n', (selection.start - 1).coerceAtLeast(0)).let {
        if (selection.start == 0) 0 else it + 1
    }
    val current = LINE_PREFIX.find(text.substring(lineStart))?.value.orEmpty()
    val replacement = next(current)
    val text = text.substring(0, lineStart) + replacement + text.substring(lineStart + current.length)
    val shift = replacement.length - current.length
    return TextFieldValue(
        text,
        TextRange(
            (selection.start + shift).coerceAtLeast(lineStart),
            (selection.end + shift).coerceAtLeast(lineStart),
        ),
    )
}

/** None → # → ## → ### → none. */
fun TextFieldValue.cycleHeading() = withLinePrefix {
    when (it) {
        "# " -> "## "
        "## " -> "### "
        "### " -> ""
        else -> "# "
    }
}

/** None → unchecked → checked → none. */
fun TextFieldValue.cycleChecklist() = withLinePrefix {
    when {
        it.endsWith("[ ] ") -> "- [x] "
        it.endsWith("[x] ") || it.endsWith("[X] ") -> ""
        else -> "- [ ] "
    }
}

fun TextFieldValue.toggleBullet() = withLinePrefix { if (it == "- ") "" else "- " }
