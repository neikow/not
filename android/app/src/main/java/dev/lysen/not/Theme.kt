package dev.lysen.not

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import dev.lysen.not.ui.NotDesign

/** The app's design (see `ui/`), following the system dark mode. */
@Composable
fun NotTheme(content: @Composable () -> Unit) = NotDesign(dark = isSystemInDarkTheme(), content = content)
