package dev.lysen.not.ui

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

private val TopBarHeight = 52.dp

/**
 * A screen with a big title that scrolls with the content; once it's scrolled away, its name
 * fades into a slim bar at the top. [header] goes under the title (subtitles, chips).
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
    content: LazyListScope.() -> Unit,
) {
    val colors = Not.colors
    val collapsed by remember { derivedStateOf { state.firstVisibleItemIndex > 0 || state.firstVisibleItemScrollOffset > 90 } }
    val barAlpha by animateFloatAsState(if (collapsed) 1f else 0f, label = "bar")
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val pulling = rememberPullDown(pull?.onPull)
    Box(
        modifier
            .fillMaxSize()
            .background(colors.background)
            .then(if (pull != null) Modifier.nestedScroll(pulling.connection) else Modifier)
            .then(if (onSwipe != null) Modifier.swipeSideways(onSwipe) else Modifier),
    ) {
        LazyColumn(
            Modifier.fillMaxSize(),
            state = state,
            contentPadding = PaddingValues(top = top + TopBarHeight, bottom = bottomPadding + 24.dp),
        ) {
            item(key = "title") {
                Column(Modifier.padding(horizontal = 24.dp).padding(top = 4.dp, bottom = 12.dp)) {
                    Row(
                        Modifier.then(if (onTitleClick != null) Modifier.pressable(onTitleClick, pressedScale = 0.98f) else Modifier),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(title, style = Not.type.display, maxLines = 1, modifier = Modifier.weight(1f, fill = false))
                        titleAccessory?.invoke()
                    }
                    header?.invoke()
                }
            }
            content()
        }
        // The slim bar: always holds the buttons; the title and a hairline appear once scrolled.
        Column(Modifier.fillMaxWidth().background(colors.background.copy(alpha = barAlpha)).statusBarsPadding()) {
            Row(
                Modifier.fillMaxWidth().height(TopBarHeight).padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) { navigation?.invoke(this) }
                Text(title, style = Not.type.heading, maxLines = 1, modifier = Modifier.alpha(barAlpha).padding(horizontal = 8.dp))
                Row(Modifier.weight(1f), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                    actions?.invoke(this)
                }
            }
            Divider(Modifier.alpha(barAlpha))
        }
        if (pull != null) PullHint(pull, pulling, Modifier.align(Alignment.TopCenter).padding(top = top + TopBarHeight + 4.dp))
    }
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

/**
 * Calls [onSwipe] for a sideways flick across the screen: forward when moving left. Rows with swipe
 * actions of their own take the drag first, so this only sees it on the title, gaps and empty space.
 */
fun Modifier.swipeSideways(onSwipe: (forward: Boolean) -> Unit): Modifier = composed {
    val threshold = with(LocalDensity.current) { 72.dp.toPx() }
    val latest by rememberUpdatedState(onSwipe)
    pointerInput(threshold) {
        var total = 0f
        detectHorizontalDragGestures(
            onDragStart = { total = 0f },
            onDragEnd = { if (abs(total) > threshold) latest(total < 0f) },
            onDragCancel = { total = 0f },
        ) { change, amount ->
            total += amount
            change.consume()
        }
    }
}

class TabItem(val label: String, val icon: Int, val badge: Int = 0, val holdLabel: String? = null)

/**
 * The floating pill at the bottom: equal, fixed slots (icon over label), with a mint capsule that
 * slides behind the selected one. Nothing inside changes size, so nothing shifts.
 */
@Composable
fun FloatingTabBar(tabs: List<TabItem>, selected: Int, onSelect: (Int) -> Unit, onHold: (Int) -> Unit, modifier: Modifier = Modifier) {
    val colors = Not.colors
    val indicator by animateDpAsState(
        TabWidth * selected,
        spring(dampingRatio = 0.8f, stiffness = Spring.StiffnessMediumLow),
        label = "tab indicator",
    )
    Box(
        modifier
            .shadow(18.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.18f), spotColor = Color.Black.copy(alpha = 0.18f))
            .clip(CircleShape)
            .background(colors.surface)
            .padding(6.dp),
    ) {
        Box(Modifier.offset(x = indicator).size(TabWidth, TabHeight).clip(CircleShape).background(colors.accent))
        Row {
            tabs.forEachIndexed { index, tab ->
                val isSelected = index == selected
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
