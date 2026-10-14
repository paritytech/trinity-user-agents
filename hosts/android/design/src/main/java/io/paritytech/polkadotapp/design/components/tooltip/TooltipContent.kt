package io.paritytech.polkadotapp.design.components.tooltip

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val MaxWidth = 240.dp

@Composable
fun PolkadotTooltipContent(
    title: String,
    message: String,
    onDismiss: () -> Unit,
) {
    Column(
        modifier = Modifier
            .widthIn(max = MaxWidth)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onDismiss,
            )
            .padding(
                vertical = PolkadotTheme.spacings.small,
                horizontal = PolkadotTheme.spacings.mediumIncreased,
            ),
    ) {
        NovaText(
            text = title,
            style = PolkadotTheme.typography.body.smallEmphasized,
            color = PolkadotTheme.colors.fg.primaryInverted,
        )

        VerticalSpacer { extraTiny }

        NovaText(
            text = message,
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.primaryInverted,
        )
    }
}

@Preview
@Composable
private fun PolkadotTooltipContentPreview() {
    PolkadotTheme {
        PolkadotSurface(
            shape = PolkadotTheme.shapes.tiny,
            color = PolkadotTheme.colors.bg.surface.containerInverted,
        ) {
            PolkadotTooltipContent(
                title = "Network status",
                message = "These rings track your connection to each chain.",
                onDismiss = {},
            )
        }
    }
}
