package dev.lysen.not

/*
 * Editing rules for markdown lists in the note editor, as plain functions over the text so they
 * can be tested without Compose. Each returns one [Change] (or null to leave the edit alone).
 */

/** Replace `[start, end)` with [text], then select `[selStart, selEnd)`. */
internal data class Change(val start: Int, val end: Int, val text: String, val selStart: Int, val selEnd: Int = selStart)

/** The indent and list marker that open a line: `"  - [ ] "`, `"- "`, `"3. "`. */
internal data class ListPrefix(val start: Int, val indent: String, val marker: String) {
    val end get() = start + indent.length + marker.length
    val isBox get() = '[' in marker
    val isNumber get() = marker[0].isDigit()
    val checked get() = isBox && (marker[3] == 'x' || marker[3] == 'X')

    /** Drawn by the field instead of shown as text, so the caret never goes inside. */
    val hidden get() = !isNumber
}

/** One level of nesting. */
internal const val INDENT = "  "

private val LIST_PREFIX = Regex("""^([ \t]*)([-*+] \[[ xX]] |[-*+] |\d+\. )""")

internal fun lineStart(text: CharSequence, i: Int): Int = if (i <= 0) 0 else text.lastIndexOf('\n', i - 1) + 1

internal fun lineEnd(text: CharSequence, i: Int): Int = text.indexOf('\n', i).let { if (it < 0) text.length else it }

internal fun listPrefix(text: CharSequence, lineStart: Int): ListPrefix? {
    val match = LIST_PREFIX.find(text.subSequence(lineStart, lineEnd(text, lineStart))) ?: return null
    return ListPrefix(lineStart, match.groupValues[1], match.groupValues[2])
}

private fun outdented(indent: String): String =
    if (indent.startsWith('\t')) indent.drop(1) else indent.drop(minOf(INDENT.length, indent.takeWhile { it == ' ' }.length))

/** The marker for the item after this one: next number, an unticked box, or the same bullet. */
private fun ListPrefix.next(): String = when {
    isNumber -> "$indent${marker.dropLast(2).toInt() + 1}. "
    isBox -> "$indent${marker[0]} [ ] "
    else -> indent + marker
}

/**
 * The user typed a newline; [text] already has it, just before [caret]. Continues the list:
 * a new item after a filled one, an empty item above when pressed at an item's start, and on an
 * empty item it outdents (if nested) or ends the list.
 */
internal fun onNewline(text: String, caret: Int): Change? {
    val p = listPrefix(text, lineStart(text, caret - 1)) ?: return null
    val itemEmpty = p.end == caret - 1
    if (itemEmpty && lineEnd(text, caret) == caret) {
        if (p.indent.isEmpty()) return Change(p.start, caret, "", p.start)
        val item = outdented(p.indent) + p.marker
        return Change(p.start, caret, item, p.start + item.length)
    }
    if (itemEmpty) {
        // Pressed at the start of an item: its text moves down with its marker (and tick).
        val above = if (p.isBox) "${p.indent}${p.marker[0]} [ ] " else p.indent + p.marker
        val below = if (p.isNumber) p.next() else p.indent + p.marker
        val inserted = "$above\n$below"
        return Change(p.start, caret, inserted, p.start + inserted.length)
    }
    val next = p.next()
    return Change(caret, caret, next, caret + next.length)
}

/**
 * The user deleted the character before [caret] in [original]. Right after a list marker that
 * takes the marker out whole instead: a nested item moves out a level, a top-level one becomes a
 * plain line keeping its text.
 */
internal fun onBackspace(original: String, caret: Int): Change? {
    val p = listPrefix(original, lineStart(original, caret)) ?: return null
    if (caret != p.end) return null
    if (p.indent.isEmpty()) return Change(p.start, p.end, "", p.start)
    val indent = outdented(p.indent)
    return Change(p.start, p.start + p.indent.length, indent, p.start + indent.length + p.marker.length)
}

/** Where a caret at [caret] belongs if it sits inside a drawn marker's hidden text, else null. */
internal fun snappedCaret(text: CharSequence, caret: Int): Int? {
    val p = listPrefix(text, lineStart(text, caret)) ?: return null
    return if (p.hidden && caret < p.end) p.end else null
}

/** Nests (or un-nests) the list items on the lines the selection touches; other lines stay. */
internal fun indentLines(text: String, selStart: Int, selEnd: Int, deeper: Boolean): Change? {
    val first = lineStart(text, selStart)
    val last = lineEnd(text, selEnd)
    val out = StringBuilder()
    // Per changed line: where it starts (in the original), and its old and new indent widths.
    val edits = mutableListOf<Triple<Int, Int, Int>>()
    var ls = first
    while (true) {
        val le = lineEnd(text, ls)
        val p = listPrefix(text, ls)
        if (p == null) {
            out.append(text, ls, le)
        } else {
            val indent = if (deeper) INDENT + p.indent else outdented(p.indent)
            out.append(indent).append(text, ls + p.indent.length, le)
            if (indent != p.indent) edits += Triple(ls, p.indent.length, indent.length)
        }
        if (le >= last) break
        out.append('\n')
        ls = le + 1
    }
    if (edits.isEmpty()) return null
    fun map(x: Int): Int {
        var shift = 0
        for ((start, old, new) in edits) {
            when {
                x <= start -> break
                x >= start + old -> shift += new - old
                else -> return start + minOf(x - start, new) + shift
            }
        }
        return x + shift
    }
    return Change(first, last, out.toString(), map(selStart), map(selEnd))
}

/** Ticks or unticks the box whose `[` is at [boxAt]; the item stays where it is. */
internal fun toggleBox(text: String, boxAt: Int, selStart: Int, selEnd: Int): Change? {
    val lineStart = text.lastIndexOf('\n', boxAt - 1) + 1
    val p = listPrefix(text, lineStart)?.takeIf { it.isBox && boxAt == it.start + it.indent.length + 2 } ?: return null
    return Change(boxAt + 1, boxAt + 2, if (p.checked) " " else "x", selStart, selEnd)
}
