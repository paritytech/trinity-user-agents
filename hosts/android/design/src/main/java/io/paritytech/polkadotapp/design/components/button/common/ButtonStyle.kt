package io.paritytech.polkadotapp.design.components.button.common

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

interface PolkadotButtonStyle {
    val colors: PolkadotButtonColors
    val rippleColor: Color

    companion object {
        @Composable
        fun primary(): PolkadotButtonStyle = rememberButtonStyle(
            colors = PolkadotButtonColors(
                containerBrush = SolidColor(PolkadotTheme.colors.bg.action.primary),
                contentColor = PolkadotTheme.colors.fg.primaryInverted,
                disabledContainerBrush = SolidColor(PolkadotTheme.colors.bg.action.disabled),
                disabledContentColor = PolkadotTheme.colors.fg.disabled
            ),
            rippleColor = PolkadotTheme.colors.fg.primaryInverted
        )

        @Composable
        fun secondary(): PolkadotButtonStyle = rememberButtonStyle(
            colors = PolkadotButtonColors(
                containerBrush = SolidColor(PolkadotTheme.colors.bg.action.secondary),
                contentColor = PolkadotTheme.colors.fg.primary,
                disabledContainerBrush = SolidColor(PolkadotTheme.colors.bg.action.disabled),
                disabledContentColor = PolkadotTheme.colors.fg.disabled
            ),
            rippleColor = PolkadotTheme.colors.fg.primary
        )

        @Composable
        fun tertiary(): PolkadotButtonStyle = rememberButtonStyle(
            colors = PolkadotButtonColors(
                containerBrush = SolidColor(PolkadotTheme.colors.bg.action.tertiary),
                contentColor = PolkadotTheme.colors.fg.primary,
                disabledContainerBrush = SolidColor(PolkadotTheme.colors.bg.action.disabled),
                disabledContentColor = PolkadotTheme.colors.fg.disabled
            ),
            rippleColor = PolkadotTheme.colors.fg.primary
        )

        @Composable
        fun ghost(): PolkadotButtonStyle = rememberButtonStyle(
            colors = PolkadotButtonColors(
                containerBrush = SolidColor(Color.Transparent),
                contentColor = PolkadotTheme.colors.fg.primary,
                disabledContainerBrush = SolidColor(Color.Transparent),
                disabledContentColor = PolkadotTheme.colors.fg.disabled
            ),
            rippleColor = PolkadotTheme.colors.fg.primary
        )

        @Composable
        fun destructive(): PolkadotButtonStyle = rememberButtonStyle(
            colors = PolkadotButtonColors(
                containerBrush = SolidColor(PolkadotTheme.colors.bg.status.error),
                contentColor = PolkadotTheme.colors.fg.primary,
                disabledContainerBrush = SolidColor(PolkadotTheme.colors.bg.action.disabled),
                disabledContentColor = PolkadotTheme.colors.fg.disabled
            ),
            rippleColor = PolkadotTheme.colors.fg.primary
        )

        @Composable
        private fun rememberButtonStyle(
            colors: PolkadotButtonColors,
            rippleColor: Color
        ): PolkadotButtonStyle = remember(colors, rippleColor) {
            RealPolkadotButtonStyle(colors, rippleColor)
        }
    }
}

@Immutable
data class PolkadotButtonColors(
    val containerBrush: Brush,
    val contentColor: Color,
    val disabledContainerBrush: Brush,
    val disabledContentColor: Color
) {
    @Stable
    fun containerBrush(enabled: Boolean): Brush =
        if (enabled) containerBrush else disabledContainerBrush

    @Stable
    fun contentColor(enabled: Boolean): Color =
        if (enabled) contentColor else disabledContentColor
}

@Immutable
private data class RealPolkadotButtonStyle(
    override val colors: PolkadotButtonColors,
    override val rippleColor: Color
) : PolkadotButtonStyle
