package dev.lysen.not

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * Widget picker previews are RemoteViews: the launcher refuses to inflate any class that isn't
 * marked for it (a plain `<View>` spacer broke every preview once), showing "Couldn't add widget".
 */
class WidgetPreviewsTest {
    private val res = File("src/main/res")

    /** The classes RemoteViews may inflate (android.widget.RemoteViews docs). */
    private val allowed = setOf(
        "FrameLayout", "LinearLayout", "RelativeLayout", "GridLayout",
        "AnalogClock", "Button", "Chronometer", "ImageButton", "ImageView", "ProgressBar", "TextClock", "TextView",
        "AdapterViewFlipper", "GridView", "ListView", "StackView", "ViewFlipper", "ViewStub",
        "CheckBox", "Switch", "RadioButton", "RadioGroup",
    )

    private fun tags(file: File): List<String> {
        val doc = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file)
        val out = mutableListOf<String>()
        fun walk(e: Element) {
            out += e.tagName
            val children = e.childNodes
            for (i in 0 until children.length) (children.item(i) as? Element)?.let(::walk)
        }
        walk(doc.documentElement)
        return out
    }

    @Test
    fun previewLayoutsUseOnlyRemoteViewsClasses() {
        val previews = File(res, "layout").listFiles { f -> f.name.startsWith("widget_") && f.name.endsWith("_preview.xml") }.orEmpty()
        assertTrue("found the preview layouts", previews.size >= 5)
        for (file in previews) {
            val bad = tags(file).filterNot { it in allowed }
            assertTrue("${file.name} uses ${bad.distinct()}, which RemoteViews can't inflate", bad.isEmpty())
        }
    }

    @Test
    fun everyWidgetHasAPreviewThatExists() {
        val infos = File(res, "xml").listFiles { f -> f.name.endsWith("_widget_info.xml") }.orEmpty()
        assertTrue(infos.isNotEmpty())
        for (info in infos) {
            val root = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }.newDocumentBuilder().parse(info).documentElement
            val preview = root.getAttributeNS("http://schemas.android.com/apk/res/android", "previewLayout")
            assertTrue("${info.name} has a previewLayout", preview.startsWith("@layout/"))
            assertTrue("${info.name}: $preview exists", File(res, "layout/${preview.removePrefix("@layout/")}.xml").exists())
        }
    }
}
