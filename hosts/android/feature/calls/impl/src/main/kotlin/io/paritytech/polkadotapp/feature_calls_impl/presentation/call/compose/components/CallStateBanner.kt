package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun CallStateBanner(
    modifier: Modifier = Modifier,
    icon: ImageVector,
    text: String,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = Color(0x2EFFFFFF),
    ) {
        Row(
            modifier = Modifier
                .padding(
                    start = PolkadotTheme.spacings.small,
                    end = PolkadotTheme.spacings.extraMedium
                )
                .padding(vertical = PolkadotTheme.spacings.tiny),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NovaIcon(
                imageVector = icon,
                tint = PolkadotTheme.colors.fg.primary
            )

            HorizontalSpacer { tiny }

            NovaText(
                text = text,
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
    }
}
