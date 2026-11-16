package dev.lysen.not

import org.junit.Assert.assertEquals
import org.junit.Test

/** Preview lines as the core marks them, as the list and widgets draw them. */
class NotePreviewTest {
    @Test
    fun boxesAndTextLines() {
        assertEquals(
            listOf(
                PreviewLine("Shopping", box = false, checked = false),
                PreviewLine("Flour", box = true, checked = false),
                PreviewLine("Eggs", box = true, checked = true),
                PreviewLine("• Mix well", box = false, checked = false),
            ),
            previewLines("Shopping\n☐ Flour\n☑ Eggs\n\n• Mix well"),
        )
        assertEquals(emptyList<PreviewLine>(), previewLines(""))
    }
}
