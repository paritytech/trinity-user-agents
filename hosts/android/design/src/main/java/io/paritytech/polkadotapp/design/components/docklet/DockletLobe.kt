package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonColors
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ScanFrame
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val RestIconSize = dockletIconSize(0f)

@Composable
internal fun BoxScope.PillLobe(
    modifier: Modifier,
    frame: () -> DockletFrame,
    progress: () -> Float,
    open: Boolean,
    closeLabel: String,
    iconColor: @Composable () -> Color,
    onClose: () -> Unit,
    anchor: @Composable BoxScope.() -> Unit,
) {
    Box(
        modifier = Modifier
            .align(Alignment.TopCenter)
            .offset {
                val centre = frame().height + dockletIconCentreBelowBody(progress())
                IntOffset(0, (centre - LobeTouchTarget / 2).roundToPx())
            }
            .size(LobeTouchTarget)
            .semantics {
                if (open) {
                    role = Role.Button
                    contentDescription = closeLabel
                    onClick { onClose(); true }
                }
            },
        propagateMinConstraints = true,
    ) {
        Box(modifier = modifier, contentAlignment = Alignment.Center) {
            LobeIcon(progress = progress, open = open, iconColor = iconColor, onClose = onClose)
            anchor()
        }
    }
}

@Composable
private fun LobeIcon(progress: () -> Float, open: Boolean, iconColor: @Composable () -> Color, onClose: () -> Unit) {
    val color = iconColor()
    val ripple = PolkadotTheme.colors.fg.primary
    val style = remember(color, ripple) { DockletLobeButtonStyle(color, ripple) }
    PolkadotIconButton(
        modifier = Modifier
            .clearAndSetSemantics {}
            .graphicsLayer {
                val scale = dockletIconSize(progress()) / RestIconSize
                scaleX = scale
                scaleY = scale
            },
        icon = NovaIcons.ScanFrame,
        onClick = onClose,
        enabled = open,
        style = style,
        size = DockletLobeIconSize(RestIconSize),
        shape = PolkadotButtonShape.pill,
    )
}

@Immutable
private data class DockletLobeButtonStyle(val iconColor: Color, override val rippleColor: Color) : PolkadotButtonStyle {
    override val colors = PolkadotButtonColors(
        containerBrush = SolidColor(Color.Transparent),
        contentColor = iconColor,
        disabledContainerBrush = SolidColor(Color.Transparent),
        disabledContentColor = iconColor,
    )
}
