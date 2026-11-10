package io.paritytech.polkadotapp.design.components.button.icon

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonInternal
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.SettingsFilled
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val IconButtonProgressStrokeWidth = 2.dp

@Composable
fun PolkadotIconButton(
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
    style: PolkadotButtonStyle = PolkadotButtonStyle.primary(),
    size: PolkadotIconButtonSize = PolkadotIconButtonSize.extraLarge(),
    shape: Shape = PolkadotButtonShape.rounded,
    interactionSource: MutableInteractionSource? = null,
    border: BorderStroke? = null,
) {
    val interactive = enabled && !loading

    PolkadotButtonInternal(
        onClick = onClick,
        modifier = modifier,
        enabled = interactive,
        shape = shape,
        containerBrush = style.colors.containerBrush(interactive),
        contentColor = style.colors.contentColor(interactive),
        rippleColor = style.rippleColor,
        contentPadding = size.padding,
        interactionSource = interactionSource,
        border = border,
        content = {
            Box {
                val progressAlpha by animateFloatAsState(
                    targetValue = if (loading) 1f else 0f,
                    label = "icon_button_progress_alpha"
                )

                if (progressAlpha > 0f) {
                    Box(
                        modifier = Modifier
                            .align(Alignment.Center)
                            .alpha(progressAlpha)
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(size.iconSize),
                            strokeWidth = IconButtonProgressStrokeWidth,
                            color = LocalContentColor.current
                        )
                    }
                }

                NovaIcon(
                    modifier = Modifier
                        .size(size.iconSize)
                        .alpha(1f - progressAlpha),
                    imageVector = icon
                )
            }
        }
    )
}

@Preview
@Composable
private fun PolkadotIconButtonPreview() {
    PolkadotTheme {
        val styles = listOf(
            "Primary" to PolkadotButtonStyle.primary(),
            "Secondary" to PolkadotButtonStyle.secondary(),
            "Tertiary" to PolkadotButtonStyle.tertiary(),
            "Ghost" to PolkadotButtonStyle.ghost(),
            "Destructive" to PolkadotButtonStyle.destructive()
        )

        val sizes = listOf(
            "XL" to PolkadotIconButtonSize.extraLarge(),
            "MI" to PolkadotIconButtonSize.mediumIncreased(),
            "M" to PolkadotIconButtonSize.medium(),
            "S" to PolkadotIconButtonSize.small(),
            "XS" to PolkadotIconButtonSize.extraSmall(),
            "T" to PolkadotIconButtonSize.tiny(),
            "XT" to PolkadotIconButtonSize.extraTiny()
        )

        Column(
            modifier = Modifier
                .background(PolkadotTheme.colors.bg.surface.main)
                .padding(PolkadotTheme.spacings.mediumIncreased)
        ) {
            styles.forEach { (_, style) ->
                Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                    sizes.forEach { (_, size) ->
                        PolkadotIconButton(
                            icon = NovaIcons.SettingsFilled,
                            onClick = {},
                            style = style,
                            size = size
                        )
                    }
                }

                VerticalSpacer { small }

                Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                    sizes.forEach { (_, size) ->
                        PolkadotIconButton(
                            icon = NovaIcons.SettingsFilled,
                            onClick = {},
                            enabled = false,
                            style = style,
                            size = size,
                            shape = PolkadotButtonShape.pill
                        )
                    }
                }

                VerticalSpacer { small }
            }
        }
    }
}
