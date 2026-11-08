package dev.lysen.not

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** `|` marks the caret (or both ends of a selection) in the texts below. */
class NoteEditingTest {
    private fun split(marked: String): Triple<String, Int, Int> {
        if ('|' !in marked) return Triple(marked, 0, 0)
        val first = marked.indexOf('|')
        val last = marked.lastIndexOf('|')
        val text = marked.replace("|", "")
        return Triple(text, first, if (last == first) first else last - 1)
    }

    private fun Change?.on(text: String): String {
        this ?: return "unchanged"
        val out = text.replaceRange(start, end, this.text)
        return if (selStart == selEnd) out.replaceRange(selStart, selStart, "|")
        else out.substring(0, selStart) + "|" + out.substring(selStart, selEnd) + "|" + out.substring(selEnd)
    }

    private fun newline(marked: String): String {
        val (text, caret) = split(marked)
        return onNewline(text, caret).on(text)
    }

    private fun backspace(marked: String): String {
        val (text, caret) = split(marked)
        return onBackspace(text, caret).on(text)
    }

    private fun indent(marked: String, deeper: Boolean = true): String {
        val (text, start, end) = split(marked)
        return indentLines(text, start, end, deeper).on(text)
    }

    private fun tick(marked: String, line: Int): String {
        val (text, start, end) = split(marked)
        val lineAt = text.split('\n').take(line).sumOf { it.length + 1 }
        return toggleBox(text, text.indexOf('[', lineAt), start, end).on(text)
    }

    @Test fun newlineContinuesLists() {
        assertEquals("- [x] milk\n- [ ] |", newline("- [x] milk\n|"))
        assertEquals("- a\n- |", newline("- a\n|"))
        assertEquals("9. a\n10. |", newline("9. a\n|"))
        assertEquals("  - [ ] a\n  - [ ] |", newline("  - [ ] a\n|"))
        assertEquals("unchanged", newline("plain\n|"))
    }

    @Test fun newlineSplitsAnItem() {
        assertEquals("- [ ] buy\n- [ ] |milk", newline("- [ ] buy\n|milk"))
    }

    @Test fun newlineOnEmptyItemEndsOrOutdents() {
        assertEquals("- a\n|", newline("- a\n- \n|"))
        assertEquals("- a\n- [ ] |", newline("- a\n  - [ ] \n|"))
    }

    @Test fun newlineAtItemStartOpensItemAbove() {
        assertEquals("- [ ] \n- [x] |milk", newline("- [x] \n|milk"))
        assertEquals("1. \n2. |milk", newline("1. \n|milk"))
    }

    @Test fun backspaceTakesMarkerOutWhole() {
        assertEquals("a\n|milk", backspace("a\n- [ ] |milk"))
        assertEquals("|milk", backspace("- |milk"))
        assertEquals("|milk", backspace("3. |milk"))
        assertEquals("- [ ] |milk", backspace("  - [ ] |milk"))
        assertEquals("unchanged", backspace("- [ ] mi|lk"))
    }

    @Test fun caretSnapsOutOfHiddenMarkup() {
        assertEquals(6, snappedCaret("- [ ] milk", 0))
        assertEquals(6, snappedCaret("- [ ] milk", 3))
        assertEquals(4, snappedCaret("a\n- milk", 2))
        assertNull(snappedCaret("- [ ] milk", 6))
        assertNull(snappedCaret("12. milk", 1))
        assertNull(snappedCaret("plain", 0))
    }

    @Test fun indentNestsListLinesOnly() {
        assertEquals("- a\n  - [ ] |b", indent("- a\n- [ ] |b"))
        assertEquals("|  - a\nplain\n  - b|", indent("|- a\nplain\n- b|"))
        assertEquals("- [ ] |b", indent("  - [ ] |b", deeper = false))
        assertEquals("unchanged", indent("- [ ] |b", deeper = false))
    }

    @Test fun tickedItemsSink() {
        assertEquals("- [ ] b\n- [ ] c\n- [x] a|", tick("- [ ] a|\n- [ ] b\n- [ ] c", line = 0))
        assertEquals("- [ ] b\n- [x] c\n- [x] a\n  - [ ] a1", tick("- [ ] a\n  - [ ] a1\n- [ ] b\n- [x] c", line = 0).replace("|", ""))
    }

    @Test fun untickedItemsRise() {
        assertEquals("- [ ] a\n- [ ] c|\n- [x] b", tick("- [ ] a\n- [x] b\n- [x] c|", line = 2))
        assertEquals("- [ ] b\n- [x] a", tick("- [x] a\n- [x] b", line = 1).replace("|", ""))
    }

    @Test fun tickingStaysInsideItsChecklist() {
        assertEquals("title\n- [ ] b\n- [x] a\n\n- [ ] z", tick("title\n- [ ] a\n- [ ] b\n\n- [ ] z", line = 1).replace("|", ""))
    }
}
