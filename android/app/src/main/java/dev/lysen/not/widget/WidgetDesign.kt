package dev.lysen.not.widget

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.action.Action
import androidx.glance.action.clickable
import androidx.glance.appwidget.appWidgetBackground
import androidx.glance.appwidget.cornerRadius
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.ColumnScope
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextAlign
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import dev.lysen.not.R
import dev.lysen.not.ui.DarkColors
import dev.lysen.not.ui.LightColors
import dev.lysen.not.ui.NotColors

/**
 * The app's palette for widgets: each color follows the system's light/dark mode, like the app
 * (widgets can't read the in-app theme, and wallpaper colors would clash with the brand).
 */
object WidgetColors {
    private fun pick(of: NotColors.() -> Color): ColorProvider = ColorProvider(day = LightColors.of(), night = DarkColors.of())

    val surface = pick { surface }
    val sunken = pick { sunken }
    val ink = pick { ink }
    val inkMuted = pick { inkMuted }
    val inkFaint = pick { inkFaint }
    val accent = pick { accent }
    val onAccent = pick { onAccent }
    val accentInk = pick { accentInk }
    val accentSoft = pick { accentSoft }
}

/** The widget's card: brand surface, the launcher's corner radius, room around the content. */
@Composable
fun WidgetSurface(content: @Composable ColumnScope.() -> Unit) {
    Column(
        GlanceModifier
            .fillMaxSize()
            .appWidgetBackground()
            .background(WidgetColors.surface)
            .cornerRadius(android.R.dimen.system_app_widget_background_radius)
            .padding(start = 10.dp, end = 10.dp, top = 8.dp, bottom = 4.dp),
        content = content,
    )
}

/**
 * Title row: bold title (taps through to the app), an optional quiet [detail] after it, and an
 * optional mint + button at the end.
 */
@Composable
fun WidgetHeader(title: String, open: Action?, detail: String? = null, detailAccent: Boolean = false, add: Action? = null, addLabel: String? = null) {
    Row(
        GlanceModifier.fillMaxWidth().height(48.dp).padding(start = 6.dp, end = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            GlanceModifier.defaultWeight().let { if (open != null) it.clickable(open) else it },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                title,
                maxLines = 1,
                style = TextStyle(color = WidgetColors.ink, fontSize = 18.sp, fontWeight = FontWeight.Bold),
            )
            if (detail != null) {
                Spacer(GlanceModifier.width(8.dp))
                Text(
                    detail,
                    maxLines = 1,
                    style = TextStyle(
                        color = if (detailAccent) WidgetColors.accentInk else WidgetColors.inkFaint,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                    ),
                )
            }
        }
        if (add != null) {
            Box(
                GlanceModifier.size(34.dp).cornerRadius(17.dp).background(WidgetColors.accent).clickable(add),
                contentAlignment = Alignment.Center,
            ) {
                Image(
                    ImageProvider(R.drawable.ic_add),
                    contentDescription = addLabel,
                    colorFilter = ColorFilter.tint(WidgetColors.onAccent),
                    modifier = GlanceModifier.size(20.dp),
                )
            }
        }
    }
}

/**
 * The app's round check (or a rounded-square [box], for note checklists): a faint ring, or a mint
 * disc with a check. The 36dp around it is the tap target.
 */
@Composable
fun WidgetCheck(checked: Boolean, onClick: Action, size: Dp = 22.dp, box: Boolean = false, label: String? = null) {
    Box(GlanceModifier.size(36.dp).clickable(onClick), contentAlignment = Alignment.Center) {
        val shape = when {
            box && checked -> R.drawable.widget_box_done
            box -> R.drawable.widget_box
            checked -> R.drawable.widget_ring_done
            else -> R.drawable.widget_ring
        }
        Image(
            ImageProvider(shape),
            contentDescription = label ?: if (checked) "Done" else "Not done",
            colorFilter = ColorFilter.tint(if (checked) WidgetColors.accent else WidgetColors.inkFaint),
            modifier = GlanceModifier.size(size),
        )
        if (checked) {
            Image(
                ImageProvider(R.drawable.ic_check),
                contentDescription = null,
                colorFilter = ColorFilter.tint(WidgetColors.onAccent),
                modifier = GlanceModifier.size(size * 0.7f),
            )
        }
    }
}

/** Centered icon + headline + hint, like the app's empty states but small. */
@Composable
fun WidgetEmpty(icon: Int, title: String, hint: String? = null, open: Action? = null) {
    Column(
        GlanceModifier.fillMaxSize().padding(bottom = 12.dp).let { if (open != null) it.clickable(open) else it },
        verticalAlignment = Alignment.CenterVertically,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(GlanceModifier.size(40.dp).cornerRadius(20.dp).background(WidgetColors.sunken), contentAlignment = Alignment.Center) {
            Image(
                ImageProvider(icon),
                contentDescription = null,
                colorFilter = ColorFilter.tint(WidgetColors.inkMuted),
                modifier = GlanceModifier.size(20.dp),
            )
        }
        Spacer(GlanceModifier.height(8.dp))
        Text(title, style = TextStyle(color = WidgetColors.ink, fontSize = 15.sp, fontWeight = FontWeight.Medium, textAlign = TextAlign.Center))
        if (hint != null) {
            Text(
                hint,
                maxLines = 2,
                style = TextStyle(color = WidgetColors.inkMuted, fontSize = 13.sp, textAlign = TextAlign.Center),
                modifier = GlanceModifier.padding(horizontal = 8.dp),
            )
        }
    }
}

/** A widget fell back to its empty state because the store failed; leave a trace for `adb logcat -s NotWidget`. */
fun logLoadFailure(widget: String, e: Exception) {
    android.util.Log.w("NotWidget", "$widget widget: load failed, showing fallback", e)
}
