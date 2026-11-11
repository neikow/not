package dev.lysen.not

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.ExerciseLog
import dev.lysen.not.ui.Not
import dev.lysen.not.ui.Text
import dev.lysen.not.ui.pressable
import java.time.LocalDate
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor

/** What the progress chart plots, one at a time (never two scales on one axis). */
enum class ProgressMetric(val label: String, val unit: String) {
    Top("Top set", "kg"),
    OneRm("Est. 1RM", "kg"),
    Volume("Volume", "kg"),
    Distance("Distance", "km"),
    ;

    fun of(log: ExerciseLog): Double? = when (this) {
        Top -> log.topWeight
        // Epley over working sets: weight × (1 + reps / 30), best of the session.
        OneRm -> log.lines.filter { it.parsed && !it.warmup && it.weight != null }
            .flatMap { line -> line.reps.map { reps -> line.weight!! * (1 + reps.toDouble() / 30) } }
            .maxOrNull()
        Volume -> log.volume.takeIf { it > 0 }
        Distance -> log.distance.takeIf { it > 0 }?.div(1000)
    }
}

/**
 * An exercise's sessions over time: one line, spaced by date, with the metric picked above it.
 * Tap or drag across it to read a session's value. Cardio-only exercises chart distance.
 */
@Composable
fun ProgressChart(logs: List<ExerciseLog>, modifier: Modifier = Modifier) {
    val cardio = logs.all { it.topWeight == null } && logs.any { it.distance > 0 }
    val metrics = if (cardio) listOf(ProgressMetric.Distance) else listOf(ProgressMetric.Top, ProgressMetric.OneRm, ProgressMetric.Volume)
    var metric by rememberSaveable(cardio) { mutableStateOf(metrics.first()) }
    // Oldest first, only sessions with a value for this metric.
    val points = remember(logs, metric) {
        logs.reversed().mapNotNull { log ->
            val day = runCatching { LocalDate.parse(log.date) }.getOrNull() ?: return@mapNotNull null
            metric.of(log)?.let { day to it }
        }
    }
    var selected by remember(points) { mutableStateOf<Int?>(null) }

    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (metrics.size > 1) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                metrics.forEach { m -> MetricChip(m.label, m == metric) { metric = m } }
            }
        }
        if (points.size < 2) {
            Text("Log this exercise twice to see a trend.", style = Not.type.caption, color = Not.colors.inkMuted)
            return@Column
        }
        // The value read out: the tapped session, else the latest, with the change since the first.
        val shown = points[selected ?: points.lastIndex]
        val first = points.first().second
        Row(verticalAlignment = androidx.compose.ui.Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("${formatWeight(round1(shown.second))} ${metric.unit}", style = Not.type.title, color = Not.colors.ink)
            val label = if (selected == null) "latest" else dayLabel(shown.first.toString(), LocalDate.now())
            val delta = shown.second - first
            val change = if (selected == null && abs(delta) >= 0.05) " · ${if (delta > 0) "+" else "−"}${formatWeight(round1(abs(delta)))} ${metric.unit} since ${dayLabel(points.first().first.toString(), LocalDate.now())}" else ""
            Text(label + change, style = Not.type.caption, color = Not.colors.inkMuted, modifier = Modifier.padding(bottom = 3.dp))
        }
        Plot(points, selected, onSelect = { selected = it }, metric = metric)
    }
}

@Composable
private fun Plot(points: List<Pair<LocalDate, Double>>, selected: Int?, onSelect: (Int?) -> Unit, metric: ProgressMetric) {
    val colors = Not.colors
    val measurer = rememberTextMeasurer()
    val axisStyle = Not.type.caption.copy(color = colors.inkFaint)
    val days = points.map { it.first.toEpochDay().toDouble() }
    val values = points.map { it.second }
    // Round the y range out to "nice" steps so gridlines land on readable numbers.
    val (lo, hi, step) = niceRange(values.min(), values.max())
    val description = "${metric.label} from ${formatWeight(round1(values.first()))} to ${formatWeight(round1(values.last()))} ${metric.unit} over ${points.size} sessions"

    // Plot area insets: room for y labels on the left and dates below.
    val left = 40.dp
    val bottom = 22.dp
    val top = 8.dp
    val right = 8.dp

    fun nearest(x: Float, width: Float, leftPx: Float, rightPx: Float): Int {
        val span = (days.last() - days.first()).takeIf { it > 0 } ?: 1.0
        val day = days.first() + ((x - leftPx) / (width - leftPx - rightPx)).coerceIn(0f, 1f) * span
        return days.indices.minBy { abs(days[it] - day) }
    }

    Canvas(
        Modifier
            .fillMaxWidth()
            .height(180.dp)
            .semantics { contentDescription = description }
            .pointerInput(points) {
                detectTapGestures { onSelect(nearest(it.x, size.width.toFloat(), left.toPx(), right.toPx()).takeIf { i -> i != selected }) }
            }
            .pointerInput(points) {
                detectDragGestures(onDragEnd = {}) { change, _ ->
                    onSelect(nearest(change.position.x, size.width.toFloat(), left.toPx(), right.toPx()))
                }
            },
    ) {
        val l = left.toPx()
        val r = size.width - right.toPx()
        val t = top.toPx()
        val b = size.height - bottom.toPx()
        val span = (days.last() - days.first()).takeIf { it > 0 } ?: 1.0
        fun x(i: Int) = (l + (days[i] - days.first()) / span * (r - l)).toFloat()
        fun y(v: Double) = (b - (v - lo) / (hi - lo) * (b - t)).toFloat()

        // Recessive grid with values on the left.
        var g = lo
        while (g <= hi + step / 2) {
            val gy = y(g)
            drawLine(colors.line, Offset(l, gy), Offset(r, gy), strokeWidth = 1.dp.toPx())
            val text = measurer.measure(formatWeight(round1(g)), axisStyle)
            drawText(text, topLeft = Offset(l - text.size.width - 8.dp.toPx(), gy - text.size.height / 2))
            g += step
        }
        // First and last dates under the ends.
        val firstDate = measurer.measure(shortDate(points.first().first), axisStyle)
        val lastDate = measurer.measure(shortDate(points.last().first), axisStyle)
        drawText(firstDate, topLeft = Offset(l, b + 6.dp.toPx()))
        drawText(lastDate, topLeft = Offset(r - lastDate.size.width, b + 6.dp.toPx()))

        selected?.let { i ->
            drawLine(
                colors.inkFaint,
                Offset(x(i), t),
                Offset(x(i), b),
                strokeWidth = 1.dp.toPx(),
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 4.dp.toPx())),
            )
        }

        val path = Path().apply {
            points.indices.forEach { i -> if (i == 0) moveTo(x(i), y(values[i])) else lineTo(x(i), y(values[i])) }
        }
        drawPath(path, colors.accentInk, style = Stroke(2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
        // Markers: 8dp with a 2dp surface ring so they read over the line; the picked one bigger.
        points.indices.forEach { i ->
            val c = Offset(x(i), y(values[i]))
            val radius = if (i == selected) 6.dp.toPx() else 4.dp.toPx()
            drawCircle(colors.surface, radius + 2.dp.toPx(), c)
            drawCircle(colors.accentInk, radius, c)
        }
    }
}

@Composable
private fun MetricChip(label: String, selected: Boolean, onClick: () -> Unit) {
    val colors = Not.colors
    Box(
        Modifier
            .clip(CircleShape)
            .background(if (selected) colors.accentSoft else colors.sunken)
            .pressable(onClick, pressedScale = 0.96f)
            .padding(horizontal = 12.dp, vertical = 6.dp),
    ) {
        Text(label, style = Not.type.label, color = if (selected) colors.accentInk else colors.inkMuted)
    }
}

/** Lower bound, upper bound and gridline step for [min]..[max], on 1, 2, 2.5 or 5 × 10ⁿ. */
private fun niceRange(min: Double, max: Double): Triple<Double, Double, Double> {
    val raw = ((max - min).takeIf { it > 0 } ?: (abs(max).takeIf { it > 0 } ?: 1.0)) / 3
    val magnitude = Math.pow(10.0, floor(Math.log10(raw)))
    val step = listOf(1.0, 2.0, 2.5, 5.0, 10.0).map { it * magnitude }.first { it >= raw }
    val lo = floor(min / step) * step
    var hi = ceil(max / step) * step
    if (hi == lo) hi = lo + step
    return Triple(lo, hi, step)
}

private fun round1(v: Double) = Math.round(v * 10) / 10.0

private fun shortDate(d: LocalDate): String =
    "${d.dayOfMonth} ${d.month.getDisplayName(java.time.format.TextStyle.SHORT, java.util.Locale.getDefault())}" +
        if (d.year != LocalDate.now().year) " ${d.year}" else ""
