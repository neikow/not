package dev.lysen.not

import org.junit.Assert.assertEquals
import org.junit.Test

class LinksTest {
    @Test
    fun bugReportFillsTheFormFields() {
        assertEquals(
            "https://github.com/neikow/not/issues/new?template=bug_report.yml&version=1.0.0&platform=Android&device=Google%20Pixel%208%20%C2%B7%20Android%2015",
            Links.issueForm("bug_report.yml", "1.0.0", "Android", "Google Pixel 8 · Android 15"),
        )
    }

    @Test
    fun featureRequestLeavesTheDeviceOut() {
        assertEquals(
            "https://github.com/neikow/not/issues/new?template=feature_request.yml&version=0.2.0&platform=Android",
            Links.featureRequest("0.2.0"),
        )
    }
}
