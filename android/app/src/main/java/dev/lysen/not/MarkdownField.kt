package dev.lysen.not

import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.sp
import dev.lysen.not.core.MarkdownStyle
import dev.lysen.not.core.markdownSpans

/**
 * A plain-text markdown editor that styles the markdown in place, using the core's spans so notes
 * look the same as on the Mac. Return continues lists.
 */
@Composable
fun MarkdownField(value: TextFieldValue, onValueChange: (TextFieldValue) -> Unit, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    val transformation = remember(colors) { MarkdownTransformation(colors) }
    BasicTextField(
        value = value,
        onValueChange = { onValueChange(continueList(value, it)) },
        textStyle = MaterialTheme.typography.bodyLarge.copy(color = colors.onSurface, lineHeight = 26.sp),
        cursorBrush = SolidColor(colors.primary),
        visualTransformation = transformation,
        modifier = modifier,
    )
}

private class MarkdownTransformation(private val colors: ColorScheme) : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val styled = AnnotatedString.Builder(text.text)
        for (span in markdownSpans(text.text)) {
            styled.addStyle(style(span.style), span.start.toInt(), span.end.toInt())
        }
        return TransformedText(styled.toAnnotatedString(), OffsetMapping.Identity)
    }

    private fun style(style: MarkdownStyle): SpanStyle = when (style) {
        is MarkdownStyle.Heading -> SpanStyle(
            fontSize = when (style.level.toInt()) {
                1 -> 26.sp
                2 -> 21.sp
                else -> 18.sp
            },
            fontWeight = if (style.level.toInt() == 3) FontWeight.SemiBold else FontWeight.Bold,
        )
        MarkdownStyle.Bold -> SpanStyle(fontWeight = FontWeight.Bold)
        MarkdownStyle.Italic -> SpanStyle(fontStyle = FontStyle.Italic)
        MarkdownStyle.Code -> SpanStyle(fontFamily = FontFamily.Monospace, background = colors.surfaceContainerHighest)
        MarkdownStyle.Strike -> SpanStyle(textDecoration = TextDecoration.LineThrough)
        MarkdownStyle.Quote -> SpanStyle(fontStyle = FontStyle.Italic, color = colors.onSurfaceVariant)
        MarkdownStyle.Bullet -> SpanStyle(color = colors.primary, fontWeight = FontWeight.Bold)
        is MarkdownStyle.Checkbox -> SpanStyle(color = colors.primary, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
        MarkdownStyle.Done -> SpanStyle(textDecoration = TextDecoration.LineThrough, color = colors.onSurfaceVariant)
        MarkdownStyle.Syntax -> SpanStyle(color = colors.outline)
    }
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
