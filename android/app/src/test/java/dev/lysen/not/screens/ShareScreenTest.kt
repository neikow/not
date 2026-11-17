package dev.lysen.not.screens

import android.content.Intent
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import dev.lysen.not.INBOX_ID
import dev.lysen.not.NotApp
import dev.lysen.not.ShareActivity
import dev.lysen.not.today
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** "Share → not" from another app, as the system delivers it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = TestApp::class)
class ShareScreenTest {
    @get:Rule val compose = createEmptyComposeRule()

    private val app get() = ApplicationProvider.getApplicationContext<NotApp>()

    private fun share(text: String): ActivityScenario<ShareActivity> =
        ActivityScenario.launch(
            Intent(app, ShareActivity::class.java).setAction(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text),
        )

    @Test
    fun aSharedLinkBecomesAnInboxTaskKeepingTheLink() {
        share("Great article https://example.com/a").use {
            compose.onNodeWithText("Task").performClick()
            compose.onNodeWithText("Save").performClick()
            compose.waitUntil(5_000) { app.store.viewOutline(INBOX_ID, today()).open.isNotEmpty() }
        }
        val task = app.store.viewOutline(INBOX_ID, today()).open.single().task
        assertEquals("Great article", task.title)
        assertEquals("https://example.com/a", task.notes)
    }

    @Test
    fun longerTextBecomesANoteAndCanBeSwitched() {
        share("Recipe\nflour\neggs").use {
            compose.onNodeWithText("Save").performClick()
            compose.waitUntil(5_000) { app.store.notes(null).isNotEmpty() }
        }
        assertEquals("Recipe", app.store.notes(null).single().title)

        share("Recipe two\nmore").use {
            compose.onNodeWithText("Task").performClick()
            compose.onNodeWithText("Save").performClick()
            compose.waitUntil(5_000) { app.store.viewOutline(INBOX_ID, today()).open.isNotEmpty() }
        }
        assertEquals("Recipe two", app.store.viewOutline(INBOX_ID, today()).open.single().task.title)
    }
}
