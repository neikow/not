package dev.lysen.not.screens

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
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
        compose.onNodeWithText("Your data").assertIsDisplayed()
        compose.onNodeWithText("Skip").performClick()
        compose.waitForIdle()
        assertTrue("the app proper", compose.onAllNodesWithText("Inbox").fetchSemanticsNodes().isNotEmpty())
    }

    @Test
    fun startFreshAddsTheExampleTasksUnlessUnticked() {
        showApp()
        compose.onNodeWithText("Get started").performClick()
        compose.onNodeWithText("Add a few example tasks").assertIsDisplayed()
        compose.onNodeWithText("Start fresh").performClick()
        compose.waitUntil(5_000) { app.store.viewOutline(INBOX_ID, today()).open.isNotEmpty() }
        assertTrue(app.store.viewOutline(INBOX_ID, today()).open.first().task.title.startsWith("Welcome to not"))
    }

    @Test
    fun startFreshWithoutExamplesLeavesTheInboxEmpty() {
        showApp()
        compose.onNodeWithText("Get started").performClick()
        compose.onNodeWithText("Add a few example tasks").performClick()
        compose.onNodeWithText("Start fresh").performClick()
        compose.onNodeWithText("Not now").assertIsDisplayed()
        assertTrue(app.store.viewOutline(INBOX_ID, today()).open.isEmpty())
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
    @Config(qualifiers = "w411dp-h891dp")
    fun tagsHaveTheirOwnPageInTheListsSheet() {
        val store = app.store
        store.setTags(store.addTask(INBOX_ID, "Call the bank", today()), listOf("admin"))
        store.setTags(store.addTask(INBOX_ID, "Buy milk", today()), listOf("errands"))
        dev.lysen.not.FirstRun.done(app)
        val tasks = TasksViewModel(app)
        compose.setContent {
            NotTheme {
                NotAppUi(tasks, HabitsViewModel(app), NotesViewModel(app), WorkoutsViewModel(app), MutableStateFlow(Tab.Tasks), onTabShown = {})
            }
        }
        compose.onNodeWithContentDescription("Switch list").performClick()
        // Tags wait behind one row, out of the way of the lists.
        compose.onNodeWithText("Tags").assertIsDisplayed()
        assertTrue("no tag rows on the first page", compose.onAllNodesWithText("errands").fetchSemanticsNodes().isEmpty())
        compose.onNodeWithText("Tags").performClick()
        compose.onNodeWithText("errands").performClick()
        compose.waitUntil(5_000) { tasks.state.value.selectedViewId == "#errands" }
        assertEquals("Buy milk", tasks.state.value.outline.open.single().task.title)
        // Back on the lists sheet, the row says which tag is shown, and Back leaves the page.
        compose.onNodeWithContentDescription("Switch list").performClick()
        compose.onNodeWithText("errands").assertIsDisplayed()
        compose.onNodeWithText("Tags").performClick()
        compose.onNodeWithContentDescription("Back to lists").performClick()
        compose.onNodeWithText("Smart lists").assertIsDisplayed()
    }

    @Test
    @Config(qualifiers = "w411dp-h891dp")
    fun holdingATaskMakesItASubtaskOfAnother() {
        val store = app.store
        val trip = store.addTask(INBOX_ID, "Plan the trip", today())
        val flights = store.addTask(INBOX_ID, "Book flights", today())
        dev.lysen.not.FirstRun.done(app)
        val tasks = TasksViewModel(app)
        compose.setContent {
            NotTheme {
                NotAppUi(tasks, HabitsViewModel(app), NotesViewModel(app), WorkoutsViewModel(app), MutableStateFlow(Tab.Tasks), onTabShown = {})
            }
        }
        compose.onNodeWithText("Book flights").performTouchInput { longClick() }
        compose.onNodeWithText("Make subtask of…").performClick()
        compose.onAllNodesWithText("Plan the trip").onLast().performClick()
        compose.waitUntil(5_000) { store.taskById(flights).parentId == trip }
        // And back out from the hold sheet.
        compose.waitForIdle()
        compose.onNodeWithText("Book flights").performTouchInput { longClick() }
        compose.waitForIdle()
        compose.onNodeWithText("Move out of “Plan the trip”").performClick()
        compose.waitUntil(5_000) { store.taskById(flights).parentId == null }
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
    fun aboutListsTheLibrariesAndSupportFollowsTheBuildsLink() {
        dev.lysen.not.FirstRun.done(app)
        showApp()
        compose.onNodeWithContentDescription("Settings").performClick()
        // Shown only when the build has a link (gradle.properties supportUrl; never in Play builds).
        assertEquals(
            dev.lysen.not.BuildConfig.SUPPORT_URL.isNotEmpty(),
            compose.onAllNodesWithText("Support not").fetchSemanticsNodes().isNotEmpty(),
        )
        compose.onNodeWithText("About").performScrollTo().performClick()
        compose.onNodeWithText("Request a feature").assertExists()
        compose.onNodeWithText("Open-source licenses").performScrollTo().performClick()
        // From assets/licenses.json, which scripts/build-android.sh writes.
        compose.onNodeWithText("ZXing").assertExists()
        compose.onNodeWithContentDescription("About").performClick()
        compose.onNodeWithText("Report a bug").assertExists()
    }

    @Test
    fun whatsNewShowsOnceAfterAnUpdateAndNeverOnAFreshInstall() {
        assertTrue("fresh install", dev.lysen.not.WhatsNew.pending(app, freshInstall = true, current = "0.2.2").isEmpty())
        assertTrue("same version", dev.lysen.not.WhatsNew.pending(app, freshInstall = false, current = "0.2.2").isEmpty())
        app.getSharedPreferences("app", android.content.Context.MODE_PRIVATE).edit().clear().commit()
        val news = dev.lysen.not.WhatsNew.pending(app, freshInstall = false, current = "0.2.2")
        assertEquals("an install from before this existed catches up", "0.2.2", news.first().version)
        assertTrue("once", dev.lysen.not.WhatsNew.pending(app, freshInstall = false, current = "0.2.2").isEmpty())
    }

    @Test
    fun aCrashLastTimeIsOfferedOnceAndForgottenOnNotNow() {
        dev.lysen.not.FirstRun.done(app)
        java.io.File(app.filesDir, "last-crash.txt").writeText("not 1.0.0 on Pixel\n\njava.lang.IllegalStateException: boom")
        showApp()
        compose.onNodeWithText("not closed unexpectedly").assertIsDisplayed()
        compose.onNodeWithText("Not now").performClick()
        compose.waitForIdle()
        assertTrue(dev.lysen.not.CrashReports.pending(app) == null)
        assertTrue(compose.onAllNodesWithText("not closed unexpectedly").fetchSemanticsNodes().isEmpty())
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
