package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonColors
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

internal val LobeTouchTarget = 48.dp

@Composable
fun rememberDockletFieldButtonStyle(): PolkadotButtonStyle {
    val fill = LocalDockletPalette.current.fieldFill
    val contentColor = PolkadotTheme.colors.fg.primary
    return remember(fill, contentColor) { DockletFieldButtonStyle(fill, contentColor) }
}

@Immutable
private data class DockletFieldButtonStyle(
    private val fill: Color,
    private val contentColor: Color,
) : PolkadotButtonStyle {
    override val colors = PolkadotButtonColors(
        containerBrush = SolidColor(fill),
        contentColor = contentColor,
        disabledContainerBrush = SolidColor(fill),
        disabledContentColor = contentColor,
    )
    override val rippleColor: Color = contentColor
}

@Immutable
internal data class DockletLobeIconSize(override val iconSize: Dp) : PolkadotIconButtonSize {
    override val padding = PaddingValues((LobeTouchTarget - iconSize) / 2)
}
