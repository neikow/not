package dev.lysen.not.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/**
 * The add field, floating above the keyboard. It keeps focus after adding, for the next one, and
 * closes when the keyboard goes away with nothing typed, or on Back.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Composer(placeholder: String, onAdd: (String) -> Unit, onClose: () -> Unit, modifier: Modifier = Modifier) {
    var draft by rememberSaveable { mutableStateOf("") }
    val colors = Not.colors
    val focus = remember { FocusRequester() }
    val imeVisible = WindowInsets.isImeVisible
    var hadKeyboard by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { focus.requestFocus() }
    LaunchedEffect(imeVisible) {
        if (imeVisible) hadKeyboard = true else if (hadKeyboard && draft.isBlank()) onClose()
    }
    BackHandler(onBack = onClose)
    fun submit() {
        if (draft.isBlank()) return
        onAdd(draft)
        draft = ""
    }
    Field(
        draft,
        { draft = it },
        placeholder,
        modifier = modifier
            .padding(horizontal = 12.dp)
            .fillMaxWidth()
            .shadow(14.dp, RoundedCornerShape(24.dp), ambientColor = Color.Black.copy(alpha = 0.14f), spotColor = Color.Black.copy(alpha = 0.14f))
            .clip(RoundedCornerShape(24.dp))
            .background(colors.surface)
            .padding(horizontal = 8.dp, vertical = 8.dp)
            .focusRequester(focus),
        leading = { Spacer(Modifier.width(4.dp)); RoundCheckPlaceholder() },
        trailing = {
            Box(
                Modifier
                    .size(36.dp)
                    .clip(CircleShape)
                    .background(if (draft.isNotBlank()) colors.accent else colors.sunken)
                    .pressable(::submit, enabled = draft.isNotBlank()),
                contentAlignment = Alignment.Center,
            ) { Icon(dev.lysen.not.R.drawable.ic_arrow_up, "Add", tint = if (draft.isNotBlank()) colors.onAccent else colors.inkFaint, size = 20.dp) }
        },
        onDone = ::submit,
    )
}

/** The empty ring a new task will have, as the composer's leading mark. */
@Composable
private fun RoundCheckPlaceholder() {
    Box(Modifier.size(22.dp).border(1.75.dp, Not.colors.inkFaint, CircleShape))
}

