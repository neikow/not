package dev.lysen.not

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** What a share becomes by default. */
class ShareTest {
    @Test
    fun aLinkWithItsTitleIsATaskKeepingTheLink() {
        val p = planShare(subject = null, text = "Great article https://example.com/a?b=1", images = 0)
        assertEquals(SharePlan(asTask = true, title = "Great article", body = "https://example.com/a?b=1"), p)
    }

    @Test
    fun aBareLinkTakesTheSubjectOrItsHostAsTitle() {
        assertEquals("Read later", planShare("Read later", "https://example.com/x", 0).title)
        assertEquals("example.com/x", planShare(null, "https://www.example.com/x/", 0).title)
        // Apps often share "Title\nURL".
        assertEquals("A title", planShare(null, "A title\nhttps://example.com", 0).title)
    }

    @Test
    fun shortTextIsATask() {
        val p = planShare(null, "  call the bank  ", 0)
        assertTrue(p.asTask)
        assertEquals("call the bank", p.title)
        assertEquals("", p.body)
    }

    @Test
    fun longOrMultiLineTextAndPicturesAreNotes() {
        assertFalse(planShare(null, "line one\nline two", 0).asTask)
        assertFalse(planShare(null, "x".repeat(200), 0).asTask)
        assertFalse(planShare(null, null, 2).asTask)
        assertEquals("Recipe\nflour\neggs", planShare("Recipe", "flour\neggs", 0).body)
        assertEquals("subject not repeated", "Recipe\nflour", planShare("Recipe", "Recipe\nflour", 0).body)
    }
}
