package dev.lysen.not.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.TransformOrigin
import kotlinx.coroutines.launch
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.unit.IntOffset
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.roundToInt
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.Velocity
import kotlin.math.abs
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.Stable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Room the floating tab bar takes at the bottom (0 where it's hidden); lists pad by it. */
val LocalBottomBarHeight = compositionLocalOf { 0.dp }

private val TopBarHeight = 56.dp

/** Opens Settings; screens at the root of a tab show a profile circle for it in their top-right corner. */
val LocalOpenSettings = compositionLocalOf<(() -> Unit)?> { null }

/** How far the content scrolls before what passes under the bar has fully faded. */
private val CollapseDistance = 64.dp

/**
 * A screen whose big title sits in the top bar beside its buttons and stays put, one size, while
 * the content scrolls under it and fades out as it passes. [header] goes under the title
 * (subtitles, chips) and scrolls with the content. With [onSwipe], sideways drags on the top bar or on
 * any part of the content that doesn't swipe itself step through whatever the title names, and [next]
 * shows beside the title what forward leads to.
 */
@Composable
fun LargeTitleScreen(
    title: String,
    modifier: Modifier = Modifier,
    state: LazyListState = rememberLazyListState(),
    onTitleClick: (() -> Unit)? = null,
    titleAccessory: (@Composable () -> Unit)? = null,
    navigation: (@Composable RowScope.() -> Unit)? = null,
    actions: (@Composable RowScope.() -> Unit)? = null,
    header: (@Composable () -> Unit)? = null,
    bottomPadding: Dp = LocalBottomBarHeight.current,
    pull: PullAction? = null,
    onSwipe: ((forward: Boolean) -> Unit)? = null,
    next: String? = null,
    content: LazyListScope.() -> Unit,
) {
    val colors = Not.colors
    val collapsePx = with(LocalDensity.current) { CollapseDistance.toPx() }
    // 0 with the list at its top, 1 once it has scrolled CollapseDistance. Read while drawing, so
    // scrolling doesn't recompose.
    val progress = remember(state, collapsePx) {
        derivedStateOf {
            if (state.firstVisibleItemIndex > 0) 1f else (state.firstVisibleItemScrollOffset / collapsePx).coerceIn(0f, 1f)
        }
    }
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val barBottom = top + TopBarHeight
    val pulling = rememberPullDown(pull?.onPull)
    val scope = rememberCoroutineScope()
    // The finger's sideways drag: the title follows it a little and springs back.
    val drag = remember { Animatable(0f) }
    // Which way the last swipe went, so the new title slides in from that side.
    var forward by remember { mutableStateOf(true) }
    val latestSwipe by rememberUpdatedState(onSwipe)
    val openSettings = LocalOpenSettings.current
    Box(
        modifier
            .fillMaxSize()
            .background(colors.background)
            .then(if (pull != null) Modifier.nestedScroll(pulling.connection) else Modifier)
            // The whole screen swipes, wherever nothing else takes the drag: rows with their own
            // swipe actions claim it first, and so does scrolling.
            .then(
                if (onSwipe == null) Modifier
                else Modifier.swipeSideways(
                    onDrag = { x -> scope.launch { drag.snapTo(x) } },
                    onEnd = { swiped ->
                        if (swiped != null) {
                            forward = swiped
                            latestSwipe?.invoke(swiped)
                        }
                        scope.launch { drag.animateTo(0f, spring(dampingRatio = 0.8f, stiffness = Spring.StiffnessMediumLow)) }
                    },
                ),
            ),
    ) {
        LazyColumn(
            Modifier.fillMaxSize().fadeUnder(barBottom) { progress.value },
            state = state,
            contentPadding = PaddingValues(top = barBottom, bottom = bottomPadding + 24.dp),
        ) {
            if (header != null) {
                item(key = "header") { Box(Modifier.padding(horizontal = 24.dp).padding(bottom = 12.dp)) { header() } }
            }
            content()
        }
        Row(
            Modifier
                .fillMaxWidth()
                .padding(top = top)
                .height(TopBarHeight)
                .padding(start = if (navigation != null) 8.dp else 24.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            navigation?.invoke(this)
            Box(
                Modifier
                    .weight(1f)
                    .padding(start = if (navigation != null) 4.dp else 0.dp)
                    .graphicsLayer {
                        translationX = drag.value * 0.3f
                    },
            ) {
                Row(
                    Modifier.then(if (onTitleClick != null) Modifier.pressable(onTitleClick, pressedScale = 0.98f) else Modifier),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    AnimatedContent(
                        title,
                        transitionSpec = {
                            val side = if (forward) 1 else -1
                            (slideInHorizontally { it / 4 * side } + fadeIn()) togetherWith
                                (slideOutHorizontally { -it / 4 * side } + fadeOut()) using SizeTransform(clip = false)
                        },
                        label = "title",
                        modifier = Modifier.weight(1f, fill = false),
                    ) { Text(it, style = Not.type.display, maxLines = 1) }
                    titleAccessory?.let { Box(Modifier.padding(start = 6.dp)) { it() } }
                    if (next != null) NextPeek(next) { drag.value }
                }
            }
            Row(horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                actions?.invoke(this)
                // Only on a tab's own screen; screens with a way back have somewhere to go instead.
                if (navigation == null && openSettings != null) {
                    ProfileButton(openSettings)
                }
            }
        }
        if (pull != null) PullHint(pull, pulling, Modifier.align(Alignment.TopCenter).padding(top = barBottom + 4.dp))
    }
}

/** A profile circle: who's on this device, and the way into Settings. */
@Composable
private fun ProfileButton(onClick: () -> Unit) {
    val colors = Not.colors
    Box(
        Modifier
            .size(44.dp)
            .clip(CircleShape)
            .pressable(onClick, pressedScale = 0.88f, onClickLabel = "Settings"),
        contentAlignment = Alignment.Center,
    ) {
        Icon(dev.lysen.not.R.drawable.ic_account_circle, "Settings", tint = colors.inkMuted, size = 30.dp)
    }
}

/**
 * What a forward swipe shows, faint beside the title and fading out at its end; it brightens as
 * the finger drags toward it, moving with the title so it never runs into the caret.
 */
@Composable
private fun NextPeek(next: String, drag: () -> Float) {
    val colors = Not.colors
    val threshold = with(LocalDensity.current) { SwipeThreshold.toPx() }
    Text(
        next,
        style = Not.type.heading,
        color = colors.inkFaint,
        maxLines = 1,
        modifier = Modifier
            .padding(start = 14.dp)
            .widthIn(max = 128.dp)
            .graphicsLayer {
                val toward = (-drag() / threshold).coerceIn(0f, 1f)
                alpha = 0.75f + 0.25f * toward
                compositingStrategy = CompositingStrategy.Offscreen
            }
            .drawWithContent {
                drawContent()
                // Fades out over its last quarter, so a cut-off name reads as more to come.
                drawRect(
                    Brush.horizontalGradient(0.7f to Color.Black, 1f to Color.Transparent),
                    blendMode = BlendMode.DstIn,
                )
            },
    )
}

/**
 * Fades the content out as it scrolls up under the top bar ending at [barBottom], by [amount] (0
 * leaves it be, 1 fully hides what's under the bar). The content below shows through untouched.
 */
private fun Modifier.fadeUnder(barBottom: Dp, amount: () -> Float): Modifier =
    graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            val a = amount()
            if (a <= 0f) return@drawWithContent
            val end = barBottom.toPx()
            drawRect(
                Brush.verticalGradient(
                    0f to Color.Black.copy(alpha = 1f - a),
                    1f to Color.Black,
                    startY = end - 8.dp.toPx(),
                    endY = end + 28.dp.toPx(),
                ),
                blendMode = BlendMode.DstIn,
            )
        }

/** What pulling a [LargeTitleScreen] down past its top does, and the hint that says so. */
class PullAction(val label: String, val icon: Int, val onPull: () -> Unit)

private val PullDistance = 96.dp

@Stable
private class PullDown(val threshold: Float, onReached: () -> Unit, onPull: () -> Unit) {
    /** How far past the top the finger has dragged, in px. */
    var pulled by mutableFloatStateOf(0f)
    val reached: Boolean get() = pulled >= threshold

    val connection = object : NestedScrollConnection {
        override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
            // Pushing back up takes the pull back first, so letting go short of it cancels.
            if (pulled > 0f && available.y < 0f && source == NestedScrollSource.UserInput) {
                pulled = (pulled + available.y).coerceAtLeast(0f)
            }
            return Offset.Zero
        }

        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (available.y > 0f && source == NestedScrollSource.UserInput) {
                val was = reached
                pulled += available.y
                if (!was && reached) onReached()
            }
            return Offset.Zero
        }

        override suspend fun onPreFling(available: Velocity): Velocity {
            if (reached) onPull()
            pulled = 0f
            return Velocity.Zero
        }
    }
}

@Composable
private fun rememberPullDown(onPull: (() -> Unit)?): PullDown {
    val threshold = with(LocalDensity.current) { PullDistance.toPx() }
    val haptics = LocalHapticFeedback.current
    val latest by rememberUpdatedState(onPull)
    return remember(threshold) {
        PullDown(threshold, onReached = { haptics.performHapticFeedback(HapticFeedbackType.SegmentTick) }, onPull = { latest?.invoke() })
    }
}

/** A pill under the top bar that fades in with the pull and fills in once letting go would act. */
@Composable
private fun PullHint(pull: PullAction, state: PullDown, modifier: Modifier) {
    val colors = Not.colors
    // Only crossing the line recomposes; the pull itself is read while drawing.
    val reached by remember(state) { derivedStateOf { state.reached } }
    val bg by animateColorAsState(if (reached) colors.accent else colors.surface, label = "pull hint")
    val fg = if (reached) colors.onAccent else colors.inkMuted
    Row(
        modifier
            .graphicsLayer {
                val progress = (state.pulled / state.threshold).coerceIn(0f, 1f)
                alpha = progress
                translationY = (progress - 1f) * 12.dp.toPx()
                scaleX = 0.9f + 0.1f * progress
                scaleY = scaleX
            }
            .clip(CircleShape)
            .background(bg)
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(pull.icon, null, tint = fg, size = 16.dp)
        Text(pull.label, style = Not.type.label, color = fg, maxLines = 1)
    }
}

private val SwipeThreshold = 72.dp

/**
 * Follows a sideways drag: [onDrag] gets how far it has gone, [onEnd] whether it
 * went far enough to count, forward when moving left (null when it didn't).
 */
fun Modifier.swipeSideways(onDrag: (Float) -> Unit = {}, onEnd: (forward: Boolean?) -> Unit): Modifier = composed {
    val threshold = with(LocalDensity.current) { SwipeThreshold.toPx() }
    val latestDrag by rememberUpdatedState(onDrag)
    val latestEnd by rememberUpdatedState(onEnd)
    pointerInput(threshold) {
        var total = 0f
        detectHorizontalDragGestures(
            onDragStart = { total = 0f },
            onDragEnd = { latestEnd(if (abs(total) > threshold) total < 0f else null) },
            onDragCancel = {
                total = 0f
                latestEnd(null)
            },
        ) { change, amount ->
            total += amount
            latestDrag(total)
            change.consume()
        }
    }
}

class TabItem(val label: String, val icon: Int, val badge: Int = 0, val holdLabel: String? = null)

/**
 * The floating pill at the bottom: equal, fixed slots (icon over label), with a mint capsule that
 * slides behind the selected one. Nothing inside changes size, so nothing shifts.
 *
 * Dragging sideways along the bar pulls the capsule under the finger (a tick per tab it crosses);
 * letting go picks the tab under it, and a quick fling moves on to the next tab that way.
 */
@Composable
fun FloatingTabBar(tabs: List<TabItem>, selected: Int, onSelect: (Int) -> Unit, onHold: (Int) -> Unit, modifier: Modifier = Modifier) {
    val colors = Not.colors
    val density = LocalDensity.current
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val slot = with(density) { TabWidth.toPx() }
    val inset = with(density) { TabBarPadding.toPx() }
    val flingSpeed = with(density) { TabFlingSpeed.toPx() }
    val maxX = slot * (tabs.size - 1)
    val latestSelected by rememberUpdatedState(selected)
    val latestSelect by rememberUpdatedState(onSelect)
    // Capsule position in px. While dragging it is plain state (snapping an Animatable from drag
    // deltas would cancel its settle animation); otherwise an Animatable springs it to the tab.
    val settled = remember { Animatable(slot * selected) }
    var dragX by remember { mutableStateOf<Float?>(null) }
    val spring = spring<Float>(dampingRatio = 0.8f, stiffness = Spring.StiffnessMediumLow)
    LaunchedEffect(selected, slot) { if (dragX == null) settled.animateTo(slot * selected, spring) }
    val capsuleX = dragX ?: settled.value
    val hovered = dragX?.let { (it / slot).roundToInt().coerceIn(0, tabs.lastIndex) }
    LaunchedEffect(hovered) { if (hovered != null) haptics.performHapticFeedback(HapticFeedbackType.SegmentTick) }
    Box(
        modifier
            .shadow(18.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.18f), spotColor = Color.Black.copy(alpha = 0.18f))
            .clip(CircleShape)
            .background(colors.surface)
            .pointerInput(slot, maxX, flingSpeed) {
                val velocity = VelocityTracker()
                fun under(x: Float) = (x - inset - slot / 2).coerceIn(0f, maxX)
                fun release(fling: Float) {
                    val at = dragX ?: return
                    val position = at / slot
                    val target = when {
                        fling > flingSpeed -> ceil(position)
                        fling < -flingSpeed -> floor(position)
                        else -> position.roundToInt().toFloat()
                    }.toInt().coerceIn(0, tabs.lastIndex)
                    scope.launch {
                        settled.snapTo(at)
                        dragX = null
                        if (target != latestSelected) latestSelect(target)
                        settled.animateTo(slot * target, spring)
                    }
                }
                detectHorizontalDragGestures(
                    onDragStart = { start ->
                        velocity.resetTracking()
                        dragX = under(start.x)
                    },
                    onDragEnd = { release(velocity.calculateVelocity().x) },
                    onDragCancel = { release(0f) },
                ) { change, _ ->
                    velocity.addPosition(change.uptimeMillis, change.position)
                    dragX = under(change.position.x)
                    change.consume()
                }
            }
            .padding(TabBarPadding),
    ) {
        Box(Modifier.offset { IntOffset(capsuleX.roundToInt(), 0) }.size(TabWidth, TabHeight).clip(CircleShape).background(colors.accent))
        Row {
            tabs.forEachIndexed { index, tab ->
                val isSelected = index == (hovered ?: selected)
                val tint by animateColorAsState(if (isSelected) colors.onAccent else colors.inkMuted, label = "tab tint")
                Column(
                    Modifier
                        .size(TabWidth, TabHeight)
                        .clip(CircleShape)
                        .pressable({ onSelect(index) }, pressedScale = 0.92f, onClickLabel = tab.label, onLongClick = { onHold(index) }, onLongClickLabel = tab.holdLabel),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(2.dp, Alignment.CenterVertically),
                ) {
                    Box {
                        Icon(tab.icon, null, tint = tint, size = 22.dp)
                        if (tab.badge > 0 && !isSelected) {
                            Badge(tab.badge, Modifier.align(Alignment.TopEnd).offset(x = 11.dp, y = (-6).dp))
                        }
                    }
                    Text(tab.label, style = Not.type.tiny, color = tint, maxLines = 1)
                }
            }
        }
    }
}

// Four tabs and the + fit a 360dp-wide phone.
private val TabWidth = 68.dp
private val TabHeight = 52.dp
private val TabBarPadding = 6.dp
/** Sideways speed past which letting go of the bar moves on to the next tab. */
private val TabFlingSpeed = 500.dp

/** What the + beside the tab bar does on the current screen; screens set it with [AddAction]. */
@Stable
class AddActionHolder {
    var label by mutableStateOf("")
    var action by mutableStateOf<(() -> Unit)?>(null)
    var holdLabel by mutableStateOf<String?>(null)
    var hold by mutableStateOf<(() -> Unit)?>(null)
}

val LocalAddAction = compositionLocalOf { AddActionHolder() }

/**
 * Shows the floating + while this is composed and [enabled]; tapping it runs [onClick], holding it
 * runs [onHold], the screen's second way to add (described by [holdLabel]).
 */
@Composable
fun AddAction(label: String, enabled: Boolean = true, holdLabel: String? = null, onHold: (() -> Unit)? = null, onClick: () -> Unit) {
    val holder = LocalAddAction.current
    val latest by rememberUpdatedState(onClick)
    val latestHold by rememberUpdatedState(onHold)
    val holds = onHold != null
    DisposableEffect(holder, label, enabled, holdLabel, holds) {
        holder.label = label
        holder.action = if (enabled) ({ latest() }) else null
        holder.holdLabel = holdLabel
        holder.hold = if (enabled && holds) ({ latestHold?.invoke() }) else null
        onDispose {
            holder.action = null
            holder.hold = null
        }
    }
}

/** Gestures on a screen's own tab: tapping it again while it's shown, or holding it. */
enum class TabGesture { Reselect, Hold }

/**
 * Routes [TabGesture]s to the screen on show; screens listen with [OnTabGesture]. A gesture sent
 * while switching tabs waits for the next screen to start listening.
 */
@Stable
class TabGestures {
    private var handler: ((TabGesture) -> Unit)? = null
    private var pending: TabGesture? = null

    /** Hands [gesture] to the screen on show. */
    fun send(gesture: TabGesture) {
        handler?.invoke(gesture) ?: run { pending = gesture }
    }

    /** Keeps [gesture] for the screen about to be shown, instead of the one leaving. */
    fun sendToNext(gesture: TabGesture) {
        handler = null
        pending = gesture
    }

    internal fun attach(h: (TabGesture) -> Unit) {
        handler = h
        pending?.let {
            pending = null
            h(it)
        }
    }

    internal fun detach(h: (TabGesture) -> Unit) {
        if (handler === h) handler = null
    }
}

val LocalTabGestures = compositionLocalOf { TabGestures() }

/** Runs [onGesture] for taps and holds on the current screen's tab, while this is composed. */
@Composable
fun OnTabGesture(onGesture: (TabGesture) -> Unit) {
    val gestures = LocalTabGestures.current
    val latest by rememberUpdatedState(onGesture)
    DisposableEffect(gestures) {
        val h: (TabGesture) -> Unit = { latest(it) }
        gestures.attach(h)
        onDispose { gestures.detach(h) }
    }
}

/** The round mint + that sits beside the tab bar. */
@Composable
fun FloatingAddButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, holdLabel: String? = null, onHold: (() -> Unit)? = null) {
    val colors = Not.colors
    Box(
        modifier
            .size(64.dp)
            .shadow(18.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.18f), spotColor = Color.Black.copy(alpha = 0.18f))
            .clip(CircleShape)
            .background(colors.accent)
            .pressable(onClick, pressedScale = 0.9f, onClickLabel = label, onLongClick = onHold, onLongClickLabel = holdLabel),
        contentAlignment = Alignment.Center,
    ) {
        Icon(dev.lysen.not.R.drawable.ic_add, label, tint = colors.onAccent, size = 28.dp)
    }
}

/** The tab bar and its + button, centred together above the system bar. */
@Composable
fun BottomBar(tabs: List<TabItem>, selected: Int, onSelect: (Int) -> Unit, onHold: (Int) -> Unit, modifier: Modifier = Modifier) {
    val add = LocalAddAction.current
    Row(
        modifier.padding(bottom = 12.dp + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FloatingTabBar(tabs, selected, onSelect, onHold)
        // Always there, so the bar never re-centres; screens without an action leave it inert.
        val hold = add.hold
        FloatingAddButton(add.label.ifEmpty { "Add" }, { add.action?.invoke() }, holdLabel = add.holdLabel, onHold = hold?.let { { it() } })
    }
}

/** Height the bottom bar occupies, including its gap and the system navigation bar. */
@Composable
fun floatingTabBarHeight(): Dp = 64.dp + 12.dp + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
