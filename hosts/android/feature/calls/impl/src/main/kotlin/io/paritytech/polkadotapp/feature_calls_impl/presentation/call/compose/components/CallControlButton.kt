package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun CallControlButton(
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
    text: String,
    icon: ImageVector,
    active: Boolean,
    enabled: Boolean = true,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        PolkadotSurface(
            shape = PolkadotTheme.shapes.full,
            color = if (active) PolkadotTheme.colors.bg.surface.containerInverted else Color(0x2EFFFFFF),
            enabled = enabled,
            onClick = onClick
        ) {
            NovaIcon(
                modifier = Modifier
                    .padding(PolkadotTheme.spacings.mediumIncreased)
                    .size(32.dp),
                imageVector = icon,
                tint = if (active) PolkadotTheme.colors.fg.primaryInverted else PolkadotTheme.colors.fg.primary
            )
        }

        VerticalSpacer { tiny }

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large
        )
    }
}
