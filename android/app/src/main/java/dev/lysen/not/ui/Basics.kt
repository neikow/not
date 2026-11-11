package dev.lysen.not.ui

import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.type
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.lysen.not.R

val LocalContentColor = compositionLocalOf { Color.Black }
val LocalTextStyle = compositionLocalOf { TextStyle.Default }

@Composable
fun Text(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle = LocalTextStyle.current,
    color: Color = LocalContentColor.current,
    maxLines: Int = Int.MAX_VALUE,
    textAlign: TextAlign? = null,
    textDecoration: TextDecoration? = null,
) {
    BasicText(
        text,
        modifier,
        style = style.merge(color = color, textAlign = textAlign ?: style.textAlign, textDecoration = textDecoration),
        maxLines = maxLines,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
fun Icon(icon: Int, contentDescription: String?, modifier: Modifier = Modifier, tint: Color = LocalContentColor.current, size: Dp = 22.dp) {
    Image(painterResource(icon), contentDescription, modifier.size(size), colorFilter = ColorFilter.tint(tint))
}

/**
 * Tap feedback for the whole app: a quick spring squeeze instead of a ripple, and a light tick.
 */
fun Modifier.pressable(
    onClick: () -> Unit,
    enabled: Boolean = true,
    role: Role = Role.Button,
    pressedScale: Float = 0.96f,
    onClickLabel: String? = null,
    onLongClick: (() -> Unit)? = null,
    onLongClickLabel: String? = null,
): Modifier = composed {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        if (pressed) pressedScale else 1f,
        spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMediumLow),
        label = "press",
    )
    if (onLongClick == null) {
        scale(scale).clickable(interaction, indication = null, enabled = enabled, role = role, onClickLabel = onClickLabel, onClick = onClick)
    } else {
        val haptics = LocalHapticFeedback.current
        scale(scale).combinedClickable(
            interaction,
            indication = null,
            enabled = enabled,
            role = role,
            onClickLabel = onClickLabel,
            onLongClickLabel = onLongClickLabel,
            onLongClick = {
                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                onLongClick()
            },
            onClick = onClick,
        )
    }
}

/** A 44dp round touch target around a 22dp icon. */
@Composable
fun IconButton(
    icon: Int,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tint: Color = Not.colors.ink,
    background: Color = Color.Transparent,
) {
    Box(
        modifier
            .size(44.dp)
            .clip(CircleShape)
            .background(background)
            .pressable(onClick, pressedScale = 0.88f, onClickLabel = contentDescription),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription, tint = tint)
    }
}

enum class ButtonKind { Primary, Secondary, Danger, Quiet }

@Composable
fun Button(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.Primary,
    icon: Int? = null,
    enabled: Boolean = true,
) {
    val colors = Not.colors
    val (bg, fg) = when (kind) {
        ButtonKind.Primary -> colors.accent to colors.onAccent
        ButtonKind.Secondary -> colors.sunken to colors.ink
        ButtonKind.Danger -> colors.dangerSoft to colors.danger
        ButtonKind.Quiet -> Color.Transparent to colors.accentInk
    }
    Row(
        modifier
            .defaultMinSize(minHeight = 44.dp)
            .clip(CircleShape)
            .background(if (enabled) bg else colors.sunken)
            .pressable(onClick, enabled = enabled)
            .padding(horizontal = 18.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val tint = if (enabled) fg else colors.inkFaint
        if (icon != null) Icon(icon, null, tint = tint, size = 18.dp)
        Text(text, style = Not.type.label, color = tint, maxLines = 1)
    }
}

/** Round checkbox: a ring that fills with mint and pops a check. 44dp touch target. */
@Composable
fun RoundCheck(
    checked: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = 24.dp,
    contentDescription: String? = null,
    /** Unchecked ring colour, e.g. for a priority; the faint ink by default. */
    ring: Color? = null,
) {
    val colors = Not.colors
    val haptics = LocalHapticFeedback.current
    val fill by animateColorAsState(if (checked) colors.accent else Color.Transparent, label = "fill")
    val ring by animateColorAsState(if (checked) colors.accent else ring ?: colors.inkFaint, label = "ring")
    val tick by animateFloatAsState(
        if (checked) 1f else 0f,
        spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium),
        label = "tick",
    )
    Box(
        modifier
            .size(44.dp)
            .clip(CircleShape)
            .toggleable(
                value = checked,
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                role = Role.Checkbox,
                onValueChange = {
                    haptics.performHapticFeedback(if (it) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
                    onToggle()
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(size)
                .border(1.75.dp, ring, CircleShape)
                .background(fill, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (tick > 0.01f) {
                Icon(R.drawable.ic_check, contentDescription, tint = colors.onAccent, size = size * 0.7f, modifier = Modifier.scale(tick))
            }
        }
    }
}

/** A small count bubble. */
@Composable
fun Badge(count: Int, modifier: Modifier = Modifier) {
    Box(
        modifier
            .defaultMinSize(minWidth = 18.dp)
            .height(18.dp)
            .clip(CircleShape)
            .background(Not.colors.accent)
            .padding(horizontal = 5.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(count.toString(), style = Not.type.tiny, color = Not.colors.onAccent)
    }
}

@Composable
fun Divider(modifier: Modifier = Modifier, color: Color = Not.colors.line) {
    Box(modifier.fillMaxWidth().height(1.dp).background(color))
}

/**
 * A text field without Material chrome: optional sunken pill, placeholder in faint ink. [onDone]
 * runs on the keyboard's Done for single-line fields. Text put in from outside lands with the
 * cursor at its end. [onBackspaceWhenEmpty] runs when backspace is pressed with nothing left.
 */
@Composable
fun Field(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    style: TextStyle = Not.type.body,
    singleLine: Boolean = true,
    minLines: Int = 1,
    filled: Boolean = false,
    shape: Shape = RoundedCornerShape(16.dp),
    leading: (@Composable RowScope.() -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
    imeAction: ImeAction = if (singleLine) ImeAction.Done else ImeAction.Default,
    onDone: () -> Unit = {},
    placeholderColor: Color? = null,
    onBackspaceWhenEmpty: (() -> Unit)? = null,
) {
    val colors = Not.colors
    var field by remember { mutableStateOf(TextFieldValue(value, TextRange(value.length))) }
    if (field.text != value) field = TextFieldValue(value, TextRange(value.length))
    val latestBackspace by rememberUpdatedState(onBackspaceWhenEmpty)
    BasicTextField(
        value = field,
        onValueChange = {
            field = it
            if (it.text != value) onValueChange(it.text)
        },
        singleLine = singleLine,
        minLines = minLines,
        textStyle = style.merge(color = colors.ink),
        cursorBrush = SolidColor(colors.accentInk),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = imeAction),
        keyboardActions = KeyboardActions(onDone = { onDone() }),
        modifier = modifier.then(
            if (onBackspaceWhenEmpty == null) Modifier
            else Modifier.onPreviewKeyEvent { event ->
                if (event.key == Key.Backspace && field.text.isEmpty()) {
                    if (event.type == KeyEventType.KeyDown) latestBackspace?.invoke()
                    true
                } else {
                    false
                }
            },
        ),
        decorationBox = { inner ->
            Row(
                Modifier
                    .then(if (filled) Modifier.clip(shape).background(colors.sunken) else Modifier)
                    .padding(horizontal = if (filled) 16.dp else 0.dp, vertical = if (filled) 12.dp else 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                leading?.invoke(this)
                Box(Modifier.weight(1f)) {
                    if (value.isEmpty()) Text(placeholder, style = style, color = placeholderColor ?: colors.inkFaint, maxLines = if (singleLine) 1 else Int.MAX_VALUE)
                    inner()
                }
                trailing?.invoke(this)
            }
        },
    )
}

/**
 * An empty screen that still feels finished: a soft mint disc with a glyph, a short headline and
 * a hint, sitting in the upper part of the screen where the eye already is.
 */
@Composable
fun EmptyState(icon: Int, title: String, hint: String, modifier: Modifier = Modifier) {
    val colors = Not.colors
    androidx.compose.foundation.layout.Column(
        modifier.fillMaxWidth().padding(horizontal = 40.dp).padding(top = 72.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(Modifier.padding(bottom = 14.dp).size(88.dp).clip(CircleShape).background(colors.accentSoft), contentAlignment = Alignment.Center) {
            Box(Modifier.size(52.dp).clip(CircleShape).background(colors.accent), contentAlignment = Alignment.Center) {
                Icon(icon, null, tint = colors.onAccent, size = 26.dp)
            }
        }
        Text(title, style = Not.type.heading, textAlign = TextAlign.Center)
        Text(hint, style = Not.type.caption.merge(fontSize = Not.type.body.fontSize, lineHeight = Not.type.body.lineHeight), color = colors.inkMuted, textAlign = TextAlign.Center, modifier = Modifier.widthIn(max = 280.dp))
    }
}

/** A small spinning arc, for work in flight. */
@Composable
fun Spinner(modifier: Modifier = Modifier, size: Dp = 20.dp, color: Color = Not.colors.accentInk) {
    val turn by rememberInfiniteTransition(label = "spinner").animateFloat(
        0f,
        360f,
        infiniteRepeatable(tween<Float>(900, easing = LinearEasing)),
        label = "turn",
    )
    androidx.compose.foundation.Canvas(modifier.size(size)) {
        val stroke = 2.dp.toPx()
        drawArc(
            color,
            startAngle = turn,
            sweepAngle = 270f,
            useCenter = false,
            topLeft = androidx.compose.ui.geometry.Offset(stroke / 2, stroke / 2),
            size = androidx.compose.ui.geometry.Size(this.size.width - stroke, this.size.height - stroke),
            style = androidx.compose.ui.graphics.drawscope.Stroke(stroke, cap = androidx.compose.ui.graphics.StrokeCap.Round),
        )
    }
}
