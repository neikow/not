package dev.lysen.not.screens

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import dev.lysen.not.HabitsViewModel
import dev.lysen.not.INBOX_ID
import dev.lysen.not.NotApp
import dev.lysen.not.NotAppUi
import dev.lysen.not.NotTheme
import dev.lysen.not.NotesViewModel
import dev.lysen.not.Tab
import dev.lysen.not.TasksViewModel
import dev.lysen.not.WorkoutsViewModel
import dev.lysen.not.core.TimedRule
import dev.lysen.not.core.everyDay
import dev.lysen.not.nowMs
import dev.lysen.not.today
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The app's screens on the JVM, through the whole shell (tab bar, settings, first run), with the
 * real Rust core underneath. Each test gets a fresh app and an empty database.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = TestApp::class)
class ScreensTest {
    @get:Rule val compose = createComposeRule()

    private val app get() = ApplicationProvider.getApplicationContext<NotApp>()

    private fun showApp(tab: Tab = Tab.Tasks) {
        val app = app
        compose.setContent {
            NotTheme {
                NotAppUi(TasksViewModel(app), HabitsViewModel(app), NotesViewModel(app), WorkoutsViewModel(app), MutableStateFlow(tab), onTabShown = {})
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun aFreshInstallStartsWithTheFirstRunAndSkipLeadsIn() {
        showApp()
        compose.onNodeWithText("Welcome to not").assertIsDisplayed()
        compose.onNodeWithText("Get started").performClick()
        compose.onNodeWithText("Bring your data").assertIsDisplayed()
        compose.onNodeWithText("Skip").performClick()
        compose.waitForIdle()
        assertTrue("the app proper", compose.onAllNodesWithText("Inbox").fetchSemanticsNodes().isNotEmpty())
    }

    @Test
    fun aPausedTimedHabitResumesFromItsRow() {
        val store = app.store
        val id = store.createTimedHabit("Wear retainer", 120u, everyDay(), TimedRule(atMostOff = true, usualOn = true, nudgeMinutes = 0u))
        val now = nowMs()
        store.setHabitState(id, true, now - 5 * 3_600_000, null, now)
        store.setHabitState(id, false, now - 25 * 60_000, null, now)
        showApp(Tab.Habits)

        compose.onNodeWithText("Wear retainer").assertIsDisplayed()
        compose.onNodeWithText("Paused 25m", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Resume").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithText("Pause").fetchSemanticsNodes().isNotEmpty() }
        assertEquals("on now, in the store too", true, store.habits(today(), nowMs(), 0).first().timed?.on)
    }

    @Test
    fun aDeletedTaskComesBackFromTheTrash() {
        val store = app.store
        val id = store.addTask(INBOX_ID, "Call the bank", today())
        store.deleteItem(id)
        dev.lysen.not.FirstRun.done(app) // a deleted task alone still looks like a fresh install
        showApp()
        compose.onNodeWithContentDescription("Settings").performClick()
        compose.onNodeWithText("Trash").performClick()
        compose.onNodeWithText("Call the bank").assertIsDisplayed()
        compose.onNodeWithContentDescription("Restore").performClick()
        compose.waitUntil(5_000) { store.trash().isEmpty() }
        assertEquals("Call the bank", store.viewOutline(INBOX_ID, today()).open.single().task.title)
    }

    @Test
    fun notePreviewsShowLinesWithoutBrackets() {
        app.store.createNote(null, "Pancakes\nShopping\n- [ ] Flour\n- [x] Eggs\n- [ ]")
        showApp(Tab.Notes)
        compose.onNodeWithText("Pancakes").assertIsDisplayed()
        compose.onNodeWithText("Shopping").assertIsDisplayed()
        compose.onNodeWithText("Flour").assertIsDisplayed()
        compose.onNodeWithText("Eggs").assertIsDisplayed()
        assertTrue("no raw markers", compose.onAllNodesWithText("[ ]", substring = true).fetchSemanticsNodes().isEmpty())
    }
}
