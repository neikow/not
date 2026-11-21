package dev.lysen.not

import android.content.Context
import android.content.Intent
import android.net.Uri
import java.net.URLEncoder

/** Where About and Support lead: the public repo, its issue forms, Ko-fi. */
object Links {
    const val REPO = "https://github.com/neikow/not"
    const val RELEASES = "$REPO/releases"
    const val PRIVACY = "$REPO/blob/main/PRIVACY.md"

    /** Ko-fi, or empty: not set, or a Play build (see build.gradle.kts). */
    val support: String get() = BuildConfig.SUPPORT_URL

    /** GitHub's feature request form (.github/ISSUE_TEMPLATE/feature_request.yml). */
    fun featureRequest(version: String): String = issueForm("feature_request.yml", version, platform = "Android")

    /**
     * GitHub's bug report form with the app version and the phone filled in, so a report says
     * what it ran on without asking.
     */
    fun bugReport(version: String, device: String, logs: String? = null): String =
        issueForm("bug_report.yml", version, platform = "Android", device = device, logs = logs)

    /** Kept under what browsers and GitHub take in a URL, with room for the rest. */
    internal const val MAX_LOGS = 3500

    internal fun issueForm(template: String, version: String, platform: String, device: String? = null, logs: String? = null): String {
        // Issue forms take a field's value from a query parameter named after its id.
        val params = listOfNotNull(
            "template" to template, "version" to version, "platform" to platform,
            device?.let { "device" to it }, logs?.let { "logs" to it.take(MAX_LOGS) },
        )
        return "$REPO/issues/new?" + params.joinToString("&") { (k, v) -> "$k=${encode(v)}" }
    }

    /** "Pixel 8 · Android 15", for bug reports. */
    fun device(): String = "${android.os.Build.MANUFACTURER} ${android.os.Build.MODEL} · Android ${android.os.Build.VERSION.RELEASE}"

    fun open(context: Context, url: String) {
        runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    private fun encode(s: String) = URLEncoder.encode(s, Charsets.UTF_8).replace("+", "%20")
}
