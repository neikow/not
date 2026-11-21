package dev.lysen.not

import dev.lysen.not.ui.sheetRest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Stepping between lists by swiping, and where a sheet settles; nothing here calls the core. */
class ListsAndSheetsTest {
    private val views = listOf("inbox" to "Inbox", "today" to "Today", "work" to "Work")

    @Test
    fun swipingGoesRoundTheLists() {
        assertEquals("today", neighbourIn(views, "inbox", forward = true)?.first)
        assertEquals("inbox", neighbourIn(views, "work", forward = true)?.first)
        assertEquals("work", neighbourIn(views, "inbox", forward = false)?.first)
        // From a tag, into the round at either end.
        assertEquals("inbox", neighbourIn(views, "#home", forward = true)?.first)
        assertEquals("work", neighbourIn(views, "#home", forward = false)?.first)
        assertNull(neighbourIn(views.take(1), "inbox", forward = true))
        assertNull(neighbourIn(emptyList<Pair<String, String>>(), "inbox", forward = true))
    }

    @Test
    fun aTallSheetRestsAtItsPeekOrFullyOpen() {
        val peek = 400f
        val close = 120f
        // Below the peek: a short pull comes back, a long one or a flick down closes.
        assertEquals(peek, sheetRest(450f, 0f, peek, close))
        assertNull(sheetRest(600f, 0f, peek, close))
        assertNull(sheetRest(450f, 2000f, peek, close))
        // Above it: a flick picks the way, else the nearer rest.
        assertEquals(0f, sheetRest(350f, -1000f, peek, close))
        assertEquals(peek, sheetRest(50f, 1000f, peek, close))
        assertEquals(0f, sheetRest(150f, 0f, peek, close))
        assertEquals(peek, sheetRest(250f, 0f, peek, close))
        // A sheet that fits has no peek: open or closed.
        assertEquals(0f, sheetRest(60f, 0f, 0f, close))
        assertNull(sheetRest(200f, 0f, 0f, close))
    }
}
