package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import io.paritytech.polkadotapp.designsystem.colors.LocalPolkadotColors
import io.paritytech.polkadotapp.designsystem.colors.PolkadotColorsPalette

private const val GlassFillAlpha = 0.7f

// strokeGlass is not in the Android design system yet; its value in every theme is bgSurfaceContainerInverted at 0x1F.
private const val GlassStrokeAlpha = 0x1F / 255f
private const val HighlightAlpha = 0.12f
private const val FieldFillAlpha = 0.06f
private const val FieldLineAlpha = 0.18f
private const val FieldOutlineAlpha = 0.08f
private const val HandleAlpha = 0.32f

@Immutable
data class DockletPalette(
    val glassFill: Color,
    val glassStroke: Color,
    val highlight: Color,
    val fieldFill: Color,
    val fieldLine: Color,
    val fieldOutline: Color,
    val handle: Color,
)

private fun dockletPalette(colors: PolkadotColorsPalette): DockletPalette {
    val inverted = colors.bg.surface.containerInverted
    return DockletPalette(
        glassFill = colors.bg.surface.container.copy(alpha = GlassFillAlpha),
        glassStroke = inverted.copy(alpha = GlassStrokeAlpha),
        highlight = inverted.copy(alpha = HighlightAlpha),
        fieldFill = inverted.copy(alpha = FieldFillAlpha),
        fieldLine = inverted.copy(alpha = FieldLineAlpha),
        fieldOutline = inverted.copy(alpha = FieldOutlineAlpha),
        handle = inverted.copy(alpha = HandleAlpha),
    )
}

val LocalDockletPalette = staticCompositionLocalOf<DockletPalette> { noLocalProvidedFor("DockletPalette") }

@Composable
fun DockletGlassTheme(content: @Composable () -> Unit) {
    val colors = LocalPolkadotColors.current
    val palette = remember(colors) { dockletPalette(colors) }
    CompositionLocalProvider(LocalDockletPalette provides palette, content = content)
}
