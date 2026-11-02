package dev.lysen.not.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import dev.lysen.not.R

/** The app's own palette: brand mint on slate, from the app icon. */
@Immutable
data class NotColors(
    /** Behind everything. */
    val background: Color,
    /** Cards, sheets, the tab bar. */
    val surface: Color,
    /** Fields and pressed states, a step below the surface. */
    val sunken: Color,
    val ink: Color,
    val inkMuted: Color,
    val inkFaint: Color,
    val line: Color,
    /** Mint, for fills: checks, primary buttons, the selected tab. */
    val accent: Color,
    /** Text and icons on [accent]. */
    val onAccent: Color,
    /** Accent for text and icons on [background] (mint itself is too light on white). */
    val accentInk: Color,
    /** A wash of the accent, behind selected or highlighted things. */
    val accentSoft: Color,
    val danger: Color,
    val dangerSoft: Color,
    /** Toasts: inverted against the theme. */
    val toast: Color,
    val onToast: Color,
    /** Dims what's behind a sheet or dialog. */
    val scrim: Color,
    val isDark: Boolean,
)

private val Slate = Color(0xFF23263A)
private val Mint = Color(0xFF8BE9A8)

val LightColors = NotColors(
    background = Color(0xFFF7F8FA),
    surface = Color(0xFFFFFFFF),
    sunken = Color(0xFFEEF0F4),
    ink = Slate,
    inkMuted = Color(0xFF6B7085),
    inkFaint = Color(0xFFA6AABA),
    line = Color(0xFFE6E8EE),
    accent = Mint,
    onAccent = Slate,
    accentInk = Color(0xFF1C9455),
    accentSoft = Color(0xFFE2F7E9),
    danger = Color(0xFFDC3E45),
    dangerSoft = Color(0xFFFCE9EA),
    toast = Slate,
    onToast = Color(0xFFF2F3F7),
    scrim = Color(0x6623263A),
    isDark = false,
)

val DarkColors = NotColors(
    background = Color(0xFF181A26),
    surface = Slate,
    sunken = Color(0xFF2B2F45),
    ink = Color(0xFFECEEF5),
    inkMuted = Color(0xFF9CA1B6),
    inkFaint = Color(0xFF636881),
    line = Color(0xFF30344B),
    accent = Mint,
    onAccent = Color(0xFF16231B),
    accentInk = Mint,
    accentSoft = Color(0xFF233A2E),
    danger = Color(0xFFFF6B70),
    dangerSoft = Color(0xFF3B2329),
    toast = Color(0xFFECEEF5),
    onToast = Slate,
    scrim = Color(0x99000000),
    isDark = true,
)

private fun inter(weight: FontWeight) =
    Font(R.font.inter, weight, variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)))

private fun bricolage(weight: FontWeight) = Font(
    R.font.bricolage_grotesque,
    weight,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight), FontVariation.Setting("opsz", 48f)),
)

val Inter = FontFamily(listOf(FontWeight.Normal, FontWeight.Medium, FontWeight.SemiBold, FontWeight.Bold).map(::inter))
val Bricolage = FontFamily(listOf(FontWeight.Medium, FontWeight.SemiBold, FontWeight.Bold, FontWeight.ExtraBold).map(::bricolage))

/** Bricolage Grotesque for titles, Inter for everything you read. */
@Immutable
data class NotType(
    /** Screen titles. */
    val display: TextStyle = TextStyle(fontFamily = Bricolage, fontWeight = FontWeight.Bold, fontSize = 34.sp, lineHeight = 40.sp, letterSpacing = (-0.6).sp),
    /** Sheet and dialog titles, a note's first line. */
    val title: TextStyle = TextStyle(fontFamily = Bricolage, fontWeight = FontWeight.SemiBold, fontSize = 22.sp, lineHeight = 28.sp, letterSpacing = (-0.2).sp),
    /** Empty-state headlines, the collapsed top bar. */
    val heading: TextStyle = TextStyle(fontFamily = Bricolage, fontWeight = FontWeight.SemiBold, fontSize = 18.sp, lineHeight = 24.sp),
    val body: TextStyle = TextStyle(fontFamily = Inter, fontWeight = FontWeight.Normal, fontSize = 16.sp, lineHeight = 24.sp, letterSpacing = (-0.1).sp),
    val bodyStrong: TextStyle = TextStyle(fontFamily = Inter, fontWeight = FontWeight.Medium, fontSize = 16.sp, lineHeight = 24.sp, letterSpacing = (-0.1).sp),
    /** Secondary lines: previews, schedules, counts. */
    val caption: TextStyle = TextStyle(fontFamily = Inter, fontWeight = FontWeight.Normal, fontSize = 13.sp, lineHeight = 18.sp),
    /** Section headers and buttons. */
    val label: TextStyle = TextStyle(fontFamily = Inter, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, lineHeight = 20.sp),
    /** Tab labels, badges. */
    val tiny: TextStyle = TextStyle(fontFamily = Inter, fontWeight = FontWeight.SemiBold, fontSize = 11.sp, lineHeight = 14.sp, letterSpacing = 0.2.sp),
)

private val LocalColors = staticCompositionLocalOf { LightColors }
private val LocalType = staticCompositionLocalOf { NotType() }

/** Access to the theme, like `Not.colors.ink`. */
object Not {
    val colors: NotColors
        @Composable @ReadOnlyComposable get() = LocalColors.current
    val type: NotType
        @Composable @ReadOnlyComposable get() = LocalType.current
}

/** Light or dark following the system. */
@Composable
fun NotDesign(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val colors = if (dark) DarkColors else LightColors
    val type = NotType()
    CompositionLocalProvider(
        LocalColors provides colors,
        LocalType provides type,
        LocalContentColor provides colors.ink,
        LocalTextStyle provides type.body,
        LocalTextSelectionColors provides TextSelectionColors(colors.accentInk, colors.accent.copy(alpha = 0.4f)),
        content = content,
    )
}
