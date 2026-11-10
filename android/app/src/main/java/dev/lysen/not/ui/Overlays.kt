package dev.lysen.not.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.math.abs
import kotlin.math.roundToInt

/** A full-window dialog without the platform dim, so we draw our own scrim and motion. */
@Composable
private fun BareDialog(onDismissRequest: () -> Unit, content: @Composable () -> Unit) {
    Dialog(
        onDismissRequest = onDismissRequest,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        val window = (LocalView.current.parent as? DialogWindowProvider)?.window
        LaunchedEffect(window) { window?.setDimAmount(0f) }
        content()
    }
}

/**
 * A bottom sheet: slides up on a spring, follows the finger down, and closes on a flick, a tap on
 * the scrim or Back. [content] gets `close`, which animates out before calling [onDismiss].
 */
@Composable
fun Sheet(onDismiss: () -> Unit, content: @Composable ColumnScope.(close: () -> Unit) -> Unit) {
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val offset = remember { Animatable(2000f) }
    // The finger's pull, on top of [offset]; folded into it when the drag ends.
    var pull by remember { mutableFloatStateOf(0f) }
    val scrim = remember { Animatable(0f) }
    var height by remember { mutableStateOf(0f) }
    var closing by remember { mutableStateOf(false) }

    fun close() {
        if (closing) return
        closing = true
        scope.launch {
            launch { scrim.animateTo(0f, tween(180)) }
            offset.animateTo(height.coerceAtLeast(600f), tween(200))
            onDismiss()
        }
    }

    BareDialog(onDismissRequest = ::close) {
        LaunchedEffect(height) {
            if (height > 0 && !closing) {
                offset.snapTo(offset.value.coerceAtMost(height))
                launch { scrim.animateTo(1f, tween(220)) }
                offset.animateTo(0f, spring(dampingRatio = 0.86f, stiffness = Spring.StiffnessMediumLow))
            }
        }
        Box(Modifier.fillMaxSize()) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(Not.colors.scrim.copy(alpha = Not.colors.scrim.alpha * scrim.value))
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = ::close),
            )
            val drag = rememberDraggableState { delta -> pull = (pull + delta).coerceAtLeast(-offset.value) }
            Column(
                Modifier
                    .align(Alignment.BottomCenter)
                    .statusBarsPadding()
                    .padding(top = 24.dp)
                    .offset { IntOffset(0, (offset.value + pull).roundToInt()) }
                    .onSizeChanged { height = it.height.toFloat() }
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp))
                    .background(Not.colors.surface)
                    .draggable(
                        drag,
                        Orientation.Vertical,
                        onDragStopped = { velocity ->
                            offset.snapTo(offset.value + pull)
                            pull = 0f
                            if (offset.value > with(density) { 120.dp.toPx() } || velocity > 1800f) {
                                close()
                            } else {
                                offset.animateTo(0f, spring(dampingRatio = 0.8f))
                            }
                        },
                    )
                    .navigationBarsPadding()
                    .imePadding(),
            ) {
                Box(Modifier.fillMaxWidth().padding(top = 10.dp, bottom = 6.dp), contentAlignment = Alignment.Center) {
                    Box(Modifier.size(width = 36.dp, height = 4.dp).clip(CircleShape).background(Not.colors.line))
                }
                content(::close)
            }
        }
    }
}

/** A centred card dialog with a title, optional body, and buttons. */
@Composable
fun CardDialog(
    title: String,
    onDismiss: () -> Unit,
    buttons: @Composable () -> Unit,
    content: (@Composable ColumnScope.() -> Unit)? = null,
) {
    val shown = remember { MutableTransitionState(false).apply { targetState = true } }
    BareDialog(onDismissRequest = onDismiss) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Not.colors.scrim)
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss)
                .imePadding(),
            contentAlignment = Alignment.Center,
        ) {
            AnimatedVisibility(shown, enter = fadeIn(tween(160)) + scaleIn(initialScale = 0.92f, animationSpec = spring(0.75f))) {
                Column(
                    Modifier
                        .padding(28.dp)
                        .widthIn(max = 420.dp)
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(28.dp))
                        .background(Not.colors.surface)
                        .clickable(remember { MutableInteractionSource() }, indication = null) {}
                        .padding(24.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(title, style = Not.type.title)
                    content?.invoke(this)
                    Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                        buttons()
                    }
                }
            }
        }
    }
}

/** A dialog that asks for one line of text. */
@Composable
fun PromptDialog(
    title: String,
    confirm: String,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
    initial: String = "",
    placeholder: String = "",
) {
    var text by remember { mutableStateOf(initial) }
    val focus = remember { FocusRequester() }
    CardDialog(
        title = title,
        onDismiss = onDismiss,
        buttons = {
            Button("Cancel", onDismiss, kind = ButtonKind.Quiet)
            Button(confirm, { if (text.isNotBlank()) onConfirm(text.trim()) }, enabled = text.isNotBlank())
        },
    ) {
        Field(
            text,
            { text = it },
            placeholder,
            filled = true,
            onDone = { if (text.isNotBlank()) onConfirm(text.trim()) },
            modifier = Modifier.fillMaxWidth().focusRequester(focus),
        )
        LaunchedEffect(Unit) { focus.requestFocus() }
    }
}

/** A floating menu anchored to the top end of its parent. Put [MenuItem]s in it. */
@Composable
fun Menu(expanded: Boolean, onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val state = remember { MutableTransitionState(false) }
    state.targetState = expanded
    if (!state.currentState && !state.targetState) return
    Popup(alignment = Alignment.TopEnd, onDismissRequest = onDismiss, properties = PopupProperties(focusable = true)) {
        AnimatedVisibility(
            state,
            enter = fadeIn(tween(120)) + scaleIn(initialScale = 0.85f, transformOrigin = TransformOrigin(1f, 0f), animationSpec = spring(0.8f, Spring.StiffnessMedium)),
            exit = fadeOut(tween(100)) + scaleOut(targetScale = 0.9f, transformOrigin = TransformOrigin(1f, 0f)),
        ) {
            Column(
                Modifier
                    .padding(8.dp)
                    .width(220.dp)
                    .shadow(16.dp, RoundedCornerShape(20.dp), ambientColor = Color.Black.copy(alpha = 0.2f))
                    .clip(RoundedCornerShape(20.dp))
                    .background(Not.colors.surface)
                    .padding(vertical = 6.dp),
                content = content,
            )
        }
    }
}

@Composable
fun MenuItem(text: String, onClick: () -> Unit, icon: Int? = null, danger: Boolean = false, enabled: Boolean = true) {
    val color = when {
        !enabled -> Not.colors.inkFaint
        danger -> Not.colors.danger
        else -> Not.colors.ink
    }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 46.dp)
            .clickable(enabled = enabled, onClick = onClick, interactionSource = remember { MutableInteractionSource() }, indication = null)
            .padding(horizontal = 18.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (icon != null) Icon(icon, null, tint = color, size = 20.dp)
        Text(text, style = Not.type.body, color = color, maxLines = 1)
    }
}

// ---- toast ----

@Stable
class ToastState {
    internal var current by mutableStateOf<Toast?>(null)
        private set

    internal class Toast(val message: String, val action: String?, val result: CompletableDeferred<Boolean>)

    /** Shows [message] for a few seconds; true if [action] was tapped. A newer toast replaces it. */
    suspend fun show(message: String, action: String? = null): Boolean {
        current?.result?.complete(false)
        val toast = Toast(message, action, CompletableDeferred())
        current = toast
        val tapped = withTimeoutOrNull(4000) { toast.result.await() } ?: false
        if (current === toast) current = null
        return tapped
    }
}

/** Where toasts appear: a floating inverted pill, rising from the bottom. */
@Composable
fun ToastHost(state: ToastState, modifier: Modifier = Modifier) {
    var last by remember { mutableStateOf<ToastState.Toast?>(null) }
    state.current?.let { last = it }
    AnimatedVisibility(
        state.current != null,
        modifier = modifier,
        enter = slideInVertically(spring(0.8f, Spring.StiffnessMediumLow)) { it } + fadeIn(),
        exit = slideOutVertically { it / 2 } + fadeOut(),
    ) {
        val toast = last ?: return@AnimatedVisibility
        Row(
            Modifier
                .padding(horizontal = 16.dp)
                .shadow(12.dp, CircleShape)
                .clip(CircleShape)
                .background(Not.colors.toast)
                .padding(start = 20.dp, end = 6.dp)
                .heightIn(min = 48.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(toast.message, style = Not.type.bodyStrong, color = Not.colors.onToast, maxLines = 1, modifier = Modifier.weight(1f, fill = false))
            Spacer(Modifier.width(8.dp))
            if (toast.action != null) {
                Box(
                    Modifier
                        .clip(CircleShape)
                        .pressable({ toast.result.complete(true) })
                        .padding(horizontal = 14.dp, vertical = 10.dp),
                ) {
                    Text(toast.action, style = Not.type.label, color = if (Not.colors.isDark) LightColors.accentInk else Not.colors.accent)
                }
            } else {
                Spacer(Modifier.width(14.dp))
            }
        }
    }
}

// ---- swipe ----

/**
 * A row with swipe actions: right for [start] (e.g. complete), left for [end] (e.g. delete); a
 * null one leaves that side still. Past the threshold the action's icon pops and the phone ticks; letting go there runs it.
 */
@Composable
fun SwipeRow(
    start: SwipeAction?,
    end: SwipeAction?,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    // Plain state while dragging; settling animates it. (Snapping an Animatable from each drag
    // delta would cancel the settle animation when a late delta lands.)
    var offset by remember { mutableFloatStateOf(0f) }
    val threshold = with(LocalDensity.current) { 88.dp.toPx() }
    var armed by remember { mutableStateOf(false) }
    BoxWithConstraints(modifier) {
        val width = constraints.maxWidth.toFloat()
        val action = if (offset >= 0) start else end
        val nowArmed = abs(offset) > threshold
        LaunchedEffect(nowArmed) {
            if (nowArmed != armed) {
                armed = nowArmed
                if (nowArmed) haptics.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate)
            }
        }
        if (offset != 0f && action != null) {
            val progress = (abs(offset) / threshold).coerceIn(0f, 1f)
            Box(
                Modifier
                    .matchParentSize()
                    .background(if (armed) action.color else action.color.copy(alpha = 0.35f + 0.4f * progress))
                    .padding(horizontal = 28.dp),
                contentAlignment = if (offset > 0) Alignment.CenterStart else Alignment.CenterEnd,
            ) {
                val pop by animateFloatAsState(if (armed) 1.15f else 0.85f, spring(0.5f), label = "pop")
                Icon(action.icon, null, tint = action.onColor, modifier = Modifier.scale(pop))
            }
        }
        Box(
            Modifier
                .offset { IntOffset(offset.roundToInt(), 0) }
                .draggable(
                    // Only toward a side that has an action.
                    rememberDraggableState { delta ->
                        offset = (offset + delta).coerceIn(if (end == null) 0f else -width, if (start == null) 0f else width)
                    },
                    Orientation.Horizontal,
                    onDragStopped = {
                        val value = offset
                        if (abs(value) > threshold) {
                            val run = (if (value > 0) start else end) ?: return@draggable
                            if (run.dismisses) animate(value, if (value > 0) width else -width, animationSpec = tween(160)) { v, _ -> offset = v }
                            run.onSwipe()
                            offset = 0f
                        } else {
                            animate(value, 0f, animationSpec = spring(0.7f, Spring.StiffnessMedium)) { v, _ -> offset = v }
                        }
                    },
                ),
        ) { content() }
    }
}

/** What a swipe does; [dismisses] slides the row away first (for deletes). */
class SwipeAction(val icon: Int, val color: Color, val onColor: Color, val dismisses: Boolean = false, val onSwipe: () -> Unit)
