package dev.lysen.not

import android.Manifest
import android.content.ContentUris
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.provider.CalendarContract
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZoneOffset

/** One occurrence of a device calendar event, shown read only beside tasks. */
@Immutable
data class CalendarEvent(
    val eventId: Long,
    val title: String,
    val day: LocalDate,
    /** Minutes after midnight on [day]; null for all-day events. */
    val start: Int?,
    val end: Int?,
    val color: Int,
) {
    val key: String get() = "$eventId-$day-$start"
}

/** Events of the coming week, from calendars shown in the phone's calendar app. */
@Immutable
class CalendarEvents(private val events: List<CalendarEvent>) {
    fun today(): List<CalendarEvent> = events.filter { it.day == LocalDate.now() }
    fun week(): List<CalendarEvent> = events.filter { it.day > LocalDate.now() }
}

/** Settings → Calendar: whether Today and Upcoming show calendar events. Per device. */
object CalendarOverlay {
    private fun prefs(context: Context) = context.getSharedPreferences("calendar", Context.MODE_PRIVATE)

    fun enabled(context: Context): Boolean =
        prefs(context).getBoolean("overlay", false) && permitted(context)

    fun setEnabled(context: Context, on: Boolean) = prefs(context).edit().putBoolean("overlay", on).apply()

    fun permitted(context: Context): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.READ_CALENDAR) == PackageManager.PERMISSION_GRANTED

    /** Events from today through the next [days] days, all-day ones first each day, then by time. */
    fun load(context: Context, days: Int = 7): List<CalendarEvent> {
        if (!enabled(context)) return emptyList()
        val zone = ZoneId.systemDefault()
        val from = LocalDate.now()
        val begin = from.atStartOfDay(zone).toInstant().toEpochMilli()
        val end = from.plusDays(days.toLong()).atStartOfDay(zone).toInstant().toEpochMilli()
        val uri = CalendarContract.Instances.CONTENT_URI.buildUpon()
            .also { ContentUris.appendId(it, begin); ContentUris.appendId(it, end) }
            .build()
        val columns = arrayOf(
            CalendarContract.Instances.EVENT_ID,
            CalendarContract.Instances.TITLE,
            CalendarContract.Instances.BEGIN,
            CalendarContract.Instances.END,
            CalendarContract.Instances.ALL_DAY,
            CalendarContract.Instances.DISPLAY_COLOR,
        )
        val out = mutableListOf<CalendarEvent>()
        runCatching {
            context.contentResolver.query(uri, columns, "${CalendarContract.Instances.VISIBLE} = 1", null, "${CalendarContract.Instances.BEGIN} ASC")?.use { c ->
                while (c.moveToNext()) {
                    val allDay = c.getInt(4) == 1
                    // All-day events are stored at UTC midnight.
                    val startAt = Instant.ofEpochMilli(c.getLong(2)).atZone(if (allDay) ZoneOffset.UTC else zone)
                    val endAt = Instant.ofEpochMilli(c.getLong(3)).atZone(if (allDay) ZoneOffset.UTC else zone)
                    val day = startAt.toLocalDate()
                    if (day < from) {
                        // Started before today (multi-day): show it on today, all day.
                        out += CalendarEvent(c.getLong(0), c.getString(1).orEmpty(), from, null, null, c.getInt(5))
                        continue
                    }
                    out += CalendarEvent(
                        eventId = c.getLong(0),
                        title = c.getString(1).orEmpty().ifBlank { "(No title)" },
                        day = day,
                        start = if (allDay) null else startAt.hour * 60 + startAt.minute,
                        end = if (allDay) null else endAt.takeIf { it.toLocalDate() == day }?.let { it.hour * 60 + it.minute },
                        color = c.getInt(5),
                    )
                }
            }
        }
        return out.distinctBy { it.key }.sortedWith(compareBy({ it.day }, { it.start ?: -1 }))
    }
}

/** Calendar events for Today and Upcoming, reloaded whenever the screen comes back. */
@Composable
fun rememberCalendarEvents(viewId: String): CalendarEvents {
    val context = LocalContext.current
    var tick by remember { mutableIntStateOf(0) }
    LifecycleResumeEffect(Unit) {
        tick++
        onPauseOrDispose {}
    }
    val wanted = viewId == TODAY_ID || viewId == UPCOMING_ID
    return remember(viewId, tick) { CalendarEvents(if (wanted) CalendarOverlay.load(context) else emptyList()) }
}

/** A quiet row: the calendar's colour, the time and the title. Tapping opens it in the calendar app. */
@Composable
fun EventRow(event: CalendarEvent, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val colors = Not.colors
    Row(
        modifier
            .fillMaxWidth()
            .pressable({
                val uri = ContentUris.withAppendedId(CalendarContract.Events.CONTENT_URI, event.eventId)
                runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, uri)) }
            }, pressedScale = 0.985f, onClickLabel = "Open in calendar")
            .heightIn(min = 44.dp)
            .padding(start = 22.dp, end = 24.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(4.dp).height(28.dp).clip(RoundedCornerShape(2.dp)).background(Color(event.color)))
        Column(Modifier.weight(1f).padding(start = 14.dp)) {
            Text(event.title, style = Not.type.body, color = colors.inkMuted, maxLines = 1)
            Text(
                when {
                    event.start == null -> "All day"
                    event.end == null -> clockLabel(event.start.toUInt())
                    else -> clockLabel(event.start.toUInt()) + "–" + clockLabel(event.end.toUInt())
                },
                style = Not.type.caption,
                color = colors.inkFaint,
            )
        }
    }
}
