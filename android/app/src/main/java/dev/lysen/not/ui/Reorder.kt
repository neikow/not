package dev.lysen.not.ui

import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyListItemInfo
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.android.awaitFrame
import kotlinx.coroutines.isActive
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

/**
 * Long-press-and-drag reordering for rows of a lazy list, in a local copy of [source] keyed by
 * [keyOf]. Rows swap as the dragged one passes their middle; letting go calls the screen's drop
 * handler with the new order, and the local order shows until [source] changes (the store caught up).
 */
@Stable
class ReorderState<T>(
    val listState: LazyListState,
    private val scope: CoroutineScope,
    private val edge: Float,
    private val keyOf: (T) -> Any,
) {
    /** The order to show: the drag's while one is going or just dropped, else the source's. */
    var items: List<T> by mutableStateOf(emptyList())
        private set
    var draggingKey: Any? by mutableStateOf(null)
        private set
    private var source: List<T> = emptyList()
    private var local: List<T>? = null
    /** Where the dragged row's top is drawn, in viewport pixels. */
    private var visualTop by mutableFloatStateOf(0f)
    private var travelled = 0f
    private var session: Session<T>? = null
    /** Sideways nesting: the dragged row's level when picked up, and where letting go puts it. */
    private var startDepth = 0
    var targetDepth by mutableStateOf(0)
        private set
    private var dx = 0f
    /** The row whose long press just ended: the tap its release makes isn't a tap. */
    private var heldKey: Any? = null
    private var scroller: Job? = null

    private class Session<T>(
        val canSwap: (T) -> Boolean,
        val depth: (T) -> Int,
        val onDrop: (List<T>) -> Unit,
        val onHold: () -> Unit,
        val nest: Nest<T>?,
    )

    /**
     * Dragging sideways changes the row's level: [indent] pixels per level, right to go under the
     * row above, left to come back out. [onLevel] runs as the level changes (a haptic tick), and
     * [onNest] instead of a plain drop when the level changed, with the order and the new level.
     */
    class Nest<T>(val indent: Float, val onLevel: () -> Unit, val onNest: (List<T>, Int) -> Unit)

    internal fun update(source: List<T>) {
        this.source = source
        if (draggingKey == null) local = null
        items = local ?: source
    }

    fun isDragging(item: T) = draggingKey != null && draggingKey == keyOf(item)

    /**
     * Starts dragging [item]. [shown] is the order to drag in (e.g. with the row's children folded
     * away), [canSwap] which rows it may trade places with. [onHold] runs instead of a drop when the
     * row was long-pressed without moving.
     */
    fun start(
        item: T,
        shown: List<T> = source,
        canSwap: (T) -> Boolean,
        depth: (T) -> Int = { 0 },
        onDrop: (List<T>) -> Unit,
        onHold: () -> Unit = {},
        nest: Nest<T>? = null,
    ) {
        val key = keyOf(item)
        val info = find(key) ?: return
        local = shown
        items = shown
        draggingKey = key
        visualTop = info.offset.toFloat()
        travelled = 0f
        dx = 0f
        startDepth = depth(item)
        targetDepth = startDepth
        session = Session(canSwap, depth, onDrop, onHold, nest)
        heldKey = key
    }

    /** True (once) when a tap on [key] is really the end of a long press. */
    fun consumeHold(key: Any): Boolean {
        if (heldKey != key) return false
        heldKey = null
        return true
    }

    internal fun drag(dy: Float, dxStep: Float = 0f) {
        if (draggingKey == null) return
        visualTop += dy
        travelled += abs(dy)
        swapIfPassed()
        autoscroll()
        val nest = session?.nest ?: return
        dx += dxStep
        travelled += abs(dxStep)
        relevel(nest)
    }

    /** The level letting go would give: sideways travel in [Nest.indent] steps, from the top level to one under the row above. */
    private fun relevel(nest: Nest<T>) {
        val s = session ?: return
        val key = draggingKey ?: return
        val at = items.indexOfFirst { keyOf(it) == key }
        val deepest = if (at > 0) s.depth(items[at - 1]) + 1 else 0
        val level = (startDepth + Math.round(dx / nest.indent)).coerceIn(0, deepest)
        if (level != targetDepth) {
            targetDepth = level
            nest.onLevel()
        }
    }

    /** How far sideways to draw the dragged row: its new level's indent. */
    internal fun translationX(key: Any): Float {
        if (draggingKey != key) return 0f
        val nest = session?.nest ?: return 0f
        return (targetDepth - startDepth) * nest.indent
    }

    internal fun end() {
        val s = session ?: return
        val moved = travelled > edge / 4
        val order = items
        val changed = order.map(keyOf) != source.map(keyOf)
        val releveled = s.nest != null && targetDepth != startDepth
        val ended = draggingKey
        draggingKey = null
        session = null
        scroller?.cancel()
        // The release's tap (if any) arrives right away; don't let it swallow a later real tap.
        scope.launch {
            delay(300)
            if (heldKey == ended) heldKey = null
        }
        if (moved && releveled) {
            s.nest!!.onNest(order, targetDepth)
        } else if (moved && changed) {
            // Keep showing the dropped order until the store's arrives.
            s.onDrop(order)
        } else {
            local = null
            items = source
            if (!moved) s.onHold()
        }
    }

    /** How far to draw the dragged row from where the list placed it. */
    internal fun translation(key: Any): Float {
        if (draggingKey != key) return 0f
        val info = find(key) ?: return 0f
        return visualTop - info.offset
    }

    private fun find(key: Any): LazyListItemInfo? = listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }

    private fun swapIfPassed() {
        val key = draggingKey ?: return
        val s = session ?: return
        val current = find(key) ?: return
        val centre = visualTop + current.size / 2f
        val byKey = items.associateBy(keyOf)
        val target = listState.layoutInfo.visibleItemsInfo
            .filter { it.key != key && byKey[it.key]?.let(s.canSwap) == true }
            .filter {
                val middle = it.offset + it.size / 2f
                if (it.index > current.index) centre > middle else centre < middle
            }
            .minByOrNull { abs(it.index - current.index) } ?: return
        val from = items.indexOfFirst { keyOf(it) == key }
        val to = items.indexOfFirst { keyOf(it) == target.key }
        if (from < 0 || to < 0) return
        items = items.toMutableList().apply {
            val moving = removeAt(from)
            // Going down, land after the target's whole block (it and its subtasks); going up,
            // right before it.
            add(if (to > from) blockEnd(this, to - 1, s.depth) else to, moving)
        }
        local = items
        s.nest?.let(::relevel)
    }

    /** Index just past the block starting at [start]: the row and the deeper rows after it. */
    private fun blockEnd(list: List<T>, start: Int, depth: (T) -> Int): Int {
        val level = depth(list[start])
        var i = start + 1
        while (i < list.size && depth(list[i]) > level) i++
        return i
    }

    private fun autoscroll() {
        if (scroller?.isActive == true) return
        scroller = scope.launch {
            while (isActive && draggingKey != null) {
                val info = listState.layoutInfo
                val key = draggingKey ?: break
                val size = find(key)?.size ?: break
                val top = info.viewportStartOffset + edge
                val bottom = info.viewportEndOffset - edge
                val step = when {
                    visualTop < top -> -((top - visualTop) / edge).coerceIn(0.2f, 1f) * 24f
                    visualTop + size > bottom -> ((visualTop + size - bottom) / edge).coerceIn(0.2f, 1f) * 24f
                    else -> break
                }
                if (listState.scrollBy(step) == 0f) break
                swapIfPassed()
                awaitFrame()
            }
        }
    }
}

@Composable
fun <T> rememberReorderState(listState: LazyListState, source: List<T>, keyOf: (T) -> Any): ReorderState<T> {
    val scope = rememberCoroutineScope()
    val edge = with(LocalDensity.current) { 72.dp.toPx() }
    val state = remember(listState) { ReorderState(listState, scope, edge, keyOf) }
    // Before the first frame too, so the list never shows empty.
    if (state.items.isEmpty() && source.isNotEmpty() && state.draggingKey == null) state.update(source)
    LaunchedEffect(state, source) { state.update(source) }
    return state
}

/**
 * The long-press-and-drag gesture alone, for a handle inside a row (e.g. a card's header) so the
 * rest of the row keeps its own long presses. Pair it with [reorderable] on the row, `enabled = false`.
 */
@Composable
fun <T> Modifier.reorderHandle(state: ReorderState<T>, key: Any, onStart: () -> Unit): Modifier {
    val haptics = LocalHapticFeedback.current
    // The gesture outlives recompositions: start from the row as it is now, not as first drawn.
    val start by rememberUpdatedState(onStart)
    return pointerInput(key) {
        detectDragGesturesAfterLongPress(
            onDragStart = {
                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                start()
            },
            onDrag = { change, amount ->
                change.consume()
                state.drag(amount.y, amount.x)
            },
            onDragEnd = { state.end() },
            onDragCancel = { state.end() },
        )
    }
}

/**
 * Makes a row draggable after a long press; [onStart] calls [ReorderState.start]. Draws the dragged
 * row lifted above the others. Use in place of `animateItem()` (which it applies to other rows).
 */
@Composable
fun <T> Modifier.reorderable(state: ReorderState<T>, item: T, key: Any, enabled: Boolean = true, onStart: () -> Unit): Modifier {
    val haptics = LocalHapticFeedback.current
    // The gesture outlives recompositions: start from the row as it is now, not as first drawn.
    val start by rememberUpdatedState(onStart)
    val dragging = state.draggingKey == key
    val gesture = if (!enabled) Modifier else Modifier.pointerInput(key) {
        detectDragGesturesAfterLongPress(
            onDragStart = {
                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                start()
            },
            onDrag = { change, amount ->
                change.consume()
                state.drag(amount.y, amount.x)
            },
            onDragEnd = { state.end() },
            onDragCancel = { state.end() },
        )
    }
    return this
        .then(if (dragging) Modifier.zIndex(1f) else Modifier)
        .graphicsLayer {
            translationY = state.translation(key)
            translationX = state.translationX(key)
            if (state.draggingKey == key) {
                scaleX = 1.02f
                scaleY = 1.02f
                shadowElevation = 12.dp.toPx()
                shape = RoundedCornerShape(18.dp)
            }
        }
        .then(gesture)
}
