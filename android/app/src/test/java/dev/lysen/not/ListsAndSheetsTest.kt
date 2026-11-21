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
    fun aDroppedRowFindsItsParentAndPlace() {
        // Trip, ·Flights, ·Hotel, Report  (dots: one level down)
        val rows = listOf("trip" to 0, "flights" to 1, "hotel" to 1, "report" to 0)
        // Hotel dragged right under Flights: Flights' subtask, last.
        assertEquals("flights" to null, placement(rows, "hotel", 2))
        // Report dragged right: Trip's last subtask (nothing after it at that level).
        assertEquals("trip" to null, placement(rows, "report", 1))
        // Flights stays a subtask of Trip, before Hotel.
        assertEquals("trip" to "hotel", placement(rows, "flights", 1))
        // Flights dragged left: top level, before Report.
        assertEquals(null to "report", placement(rows, "flights", 0))
        assertEquals(null to null, placement(rows, "missing", 0))
    }

    @Test
    fun aTaskCantGoUnderItsOwnSubtasks() {
        fun task(id: String, parent: String?) = dev.lysen.not.core.TaskItem(
            id = id, parentId = parent, title = id, notes = "", completedAt = null, start = null, time = null,
            deadline = null, someday = false, priority = 0u, repeat = null, tags = emptyList(),
        )
        val tasks = listOf(task("a", null), task("b", "a"), task("c", "b"), task("d", null))
        assertEquals(setOf("b", "c"), descendants("a", tasks))
        assertEquals(emptySet<String>(), descendants("d", tasks))
    }

    @Test
    fun anEmptyListSaysWhatNewTasksGet() {
        fun hint(tags: List<String> = emptyList(), priority: UByte = 0u, start: String? = null, someday: Boolean = false) =
            newTaskHint(dev.lysen.not.core.ViewDefaults(tags, priority, start, someday), "2026-11-08")
        assertEquals("Tap + to add one. It gets #work !1, so it shows here.", hint(listOf("work"), 3u))
        assertEquals("Tap + to add one. It gets today, so it shows here.", hint(start = "2026-11-08"))
        assertEquals("Tap + to add one. It gets #ideas someday, so it shows here.", hint(listOf("ideas"), someday = true))
        assertEquals("Tap + to add one. It gets tomorrow, so it shows here.", hint(start = "2026-11-09"))
        assertEquals("Tap + to add one.", hint())
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

class PageDotsTest {
    @Test
    fun fewPagesShowAllDots() {
        assertEquals(listOf(0 to false, 1 to false, 2 to false), dev.lysen.not.ui.dotWindow(dev.lysen.not.ui.PagePosition(1, 3)))
    }

    @Test
    fun manyPagesShowAWindowThatShrinksAtTheEdgesWithMoreBeyond() {
        val start = dev.lysen.not.ui.dotWindow(dev.lysen.not.ui.PagePosition(0, 12))
        assertEquals((0..6).toList(), start.map { it.first })
        assertEquals(listOf(false, false, false, false, false, false, true), start.map { it.second })
        val middle = dev.lysen.not.ui.dotWindow(dev.lysen.not.ui.PagePosition(6, 12))
        assertEquals((3..9).toList(), middle.map { it.first })
        assertEquals(true, middle.first().second)
        assertEquals(true, middle.last().second)
        val end = dev.lysen.not.ui.dotWindow(dev.lysen.not.ui.PagePosition(11, 12))
        assertEquals((5..11).toList(), end.map { it.first })
        assertEquals(false, end.last().second)
    }
}
