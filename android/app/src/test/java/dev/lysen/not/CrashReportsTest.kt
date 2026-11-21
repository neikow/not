package dev.lysen.not

import org.junit.Assert.assertTrue
import org.junit.Test

class CrashReportsTest {
    @Test
    fun aReportStartsWithTheVersionAndPutsTheRootCauseFirst() {
        val error = IllegalStateException("outer", IllegalArgumentException("the real problem"))
        val report = CrashReports.describe("main", error, "1.0.0", "Pixel 8 · Android 15")
        assertTrue(report, report.startsWith("not 1.0.0 on Pixel 8 · Android 15, thread \"main\""))
        assertTrue(report, report.indexOf("the real problem") < report.indexOf("outer"))
        assertTrue(report, report.contains("    at dev.lysen.not.CrashReportsTest"))
    }

    @Test
    fun longTracesAreCutToFitAUrl() {
        val url = Links.bugReport("1.0.0", "Pixel", logs = "x".repeat(20_000))
        assertTrue(url.length < 8_000)
    }
}
