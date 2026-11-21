package dev.lysen.not

import dev.lysen.not.core.ImportReport
import dev.lysen.not.core.ImportSource
import org.junit.Assert.assertEquals
import org.junit.Test

class TaskImportTest {
    @Test
    fun theMessageSaysWhatCameInAndWhatWasLeftOut() {
        assertEquals("Added 1 task from Todoist", TaskImport.message(ImportReport(ImportSource.TODOIST, 1u, 0u)))
        assertEquals("Added 42 tasks from TickTick, left out 7 done", TaskImport.message(ImportReport(ImportSource.TICK_TICK, 42u, 7u)))
    }
}
