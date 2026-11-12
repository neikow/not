package dev.lysen.not

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import dev.lysen.not.core.NoteTaskBlock
import dev.lysen.not.core.TaskBlockView
import dev.lysen.not.core.TaskItem
import dev.lysen.not.ui.NotColors
import dev.lysen.not.ui.NotType

/*
 * Task blocks in notes (ADR 0011): a checklist made into tasks shows as one block, drawn from the
 * tasks as they are now over its hidden `![](not-tasks:…)` line, like an image. Tapping a box ticks
 * the task, tapping a row opens it, "Add task" adds one.
 */

/** What the editor does with a block's rows; the screen hosting it supplies these. */
class TaskBlockActions(
    /** The block as it is now; null if it can't be read. */
    val view: (markup: String) -> TaskBlockView?,
    val toggle: (TaskItem) -> Unit,
    val open: (TaskItem) -> Unit,
    /** "Add task" in the block whose line is [markup]. */
    val add: (markup: String) -> Unit,
)

private val PAD = 6.dp
private val HEADER = 30.dp
private val ROW = 44.dp
private val ROW_INDENT = 24.dp
private val BOX = 20.dp
private val INSET = 12.dp

/** The block's height, its gap above and below included (the line is as tall). */
fun Density.taskBlockHeight(view: TaskBlockView?, gap: Float): Float {
    if (view == null || view.gone) return ROW.toPx() + PAD.toPx() * 2 + gap * 2
    val header = if (view.title.isNotEmpty()) HEADER.toPx() else 0f
    return PAD.toPx() * 2 + header + ROW.toPx() * (view.rows.size + 1) + gap * 2
}

/** Where a block sits in the field: its line's box, less the gap. */
fun TextLayoutResult.taskBlockRect(block: NoteTaskBlock, width: Float, gap: Float): Rect? {
    val line = getLineForOffset(block.start.toInt())
    val top = getLineTop(line) + gap
    val bottom = getLineBottom(line) - gap
    return if (bottom > top) Rect(0f, top, width, bottom) else null
}

/** What a tap at [at] (inside [rect]) means. */
sealed interface BlockTap {
    data class Toggle(val task: TaskItem) : BlockTap
    data class Open(val task: TaskItem) : BlockTap
    data object Add : BlockTap
}

fun Density.taskBlockTap(view: TaskBlockView?, rect: Rect, at: Offset): BlockTap? {
    if (view == null || view.gone) return null
    val rowsTop = rect.top + PAD.toPx() + if (view.title.isNotEmpty()) HEADER.toPx() else 0f
    if (at.y < rowsTop) return null
    val index = ((at.y - rowsTop) / ROW.toPx()).toInt()
    if (index == view.rows.size) return BlockTap.Add
    val row = view.rows.getOrNull(index) ?: return null
    val boxRight = rect.left + INSET.toPx() + ROW_INDENT.toPx() * row.depth.toInt() + BOX.toPx()
    return if (at.x <= boxRight + 12.dp.toPx()) BlockTap.Toggle(row.task) else BlockTap.Open(row.task)
}

fun DrawScope.drawTaskBlock(
    view: TaskBlockView?,
    rect: Rect,
    measurer: TextMeasurer,
    type: NotType,
    colors: NotColors,
    drawBox: DrawScope.(Rect, Boolean) -> Unit,
) {
    drawRoundRect(colors.sunken, rect.topLeft, rect.size, CornerRadius(14.dp.toPx()))
    val inset = INSET.toPx()
    val textWidth = { left: Float -> (rect.right - left - inset).coerceAtLeast(1f).toInt() }
    fun label(text: String, left: Float, top: Float, height: Float, style: TextStyle) {
        val measured = measurer.measure(
            text,
            style,
            overflow = TextOverflow.Ellipsis,
            maxLines = 1,
            constraints = Constraints(maxWidth = textWidth(left)),
        )
        drawText(measured, topLeft = Offset(left, top + (height - measured.size.height) / 2))
    }
    var top = rect.top + PAD.toPx()
    if (view == null || view.gone) {
        label("Tasks deleted", rect.left + inset, top, ROW.toPx(), type.body.copy(color = colors.inkFaint))
        return
    }
    if (view.title.isNotEmpty()) {
        label(view.title, rect.left + inset, top, HEADER.toPx(), type.label.copy(color = colors.inkMuted))
        top += HEADER.toPx()
    }
    val row = ROW.toPx()
    val box = BOX.toPx()
    for (r in view.rows) {
        val left = rect.left + inset + ROW_INDENT.toPx() * r.depth.toInt()
        val done = r.task.completedAt != null
        drawBox(Rect(Offset(left, top + (row - box) / 2), Size(box, box)), done)
        label(
            r.task.title,
            left + box + 12.dp.toPx(),
            top,
            row,
            type.body.copy(
                color = if (r.done) colors.inkFaint else colors.ink,
                textDecoration = if (r.done) TextDecoration.LineThrough else null,
            ),
        )
        top += row
    }
    label("+  Add task", rect.left + inset, top, row, type.body.copy(color = colors.accentInk))
}
