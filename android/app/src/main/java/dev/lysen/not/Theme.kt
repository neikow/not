package dev.lysen.not

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.TextStyle
import dev.lysen.not.ui.Bricolage
import dev.lysen.not.ui.Inter
import dev.lysen.not.ui.NotColors
import dev.lysen.not.ui.NotDesign
import dev.lysen.not.ui.Not

/**
 * The app's design (see `ui/`), following the system dark mode. Screens not yet moved off
 * Material get a Material theme in the same colors and fonts meanwhile.
 */
@Composable
fun NotTheme(content: @Composable () -> Unit) {
    NotDesign(dark = isSystemInDarkTheme()) {
        MaterialTheme(colorScheme = Not.colors.toMaterial(), typography = BridgeTypography, content = content)
    }
}

private fun NotColors.toMaterial() = (if (isDark) darkColorScheme() else lightColorScheme()).copy(
    primary = accentInk,
    onPrimary = if (isDark) onAccent else surface,
    primaryContainer = accentSoft,
    onPrimaryContainer = ink,
    secondaryContainer = accentSoft,
    onSecondaryContainer = ink,
    background = background,
    onBackground = ink,
    surface = background,
    onSurface = ink,
    surfaceVariant = sunken,
    onSurfaceVariant = inkMuted,
    surfaceContainerLowest = surface,
    surfaceContainerLow = surface,
    surfaceContainer = surface,
    surfaceContainerHigh = surface,
    surfaceContainerHighest = sunken,
    outline = inkFaint,
    outlineVariant = line,
    error = danger,
    errorContainer = dangerSoft,
)

private val BridgeTypography = Typography().run {
    fun TextStyle.inter() = copy(fontFamily = Inter)
    fun TextStyle.bricolage() = copy(fontFamily = Bricolage)
    copy(
        displayLarge = displayLarge.bricolage(), displayMedium = displayMedium.bricolage(), displaySmall = displaySmall.bricolage(),
        headlineLarge = headlineLarge.bricolage(), headlineMedium = headlineMedium.bricolage(), headlineSmall = headlineSmall.bricolage(),
        titleLarge = titleLarge.bricolage(), titleMedium = titleMedium.inter(), titleSmall = titleSmall.inter(),
        bodyLarge = bodyLarge.inter(), bodyMedium = bodyMedium.inter(), bodySmall = bodySmall.inter(),
        labelLarge = labelLarge.inter(), labelMedium = labelMedium.inter(), labelSmall = labelSmall.inter(),
    )
}
