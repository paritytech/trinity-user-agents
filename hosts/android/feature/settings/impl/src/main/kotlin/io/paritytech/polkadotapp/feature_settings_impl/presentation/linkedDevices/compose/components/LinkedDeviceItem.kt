package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.ComputerDesktop
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun LinkedDeviceItem(
    name: String,
    description: String?,
    onClick: () -> Unit
) {
    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        color = Color.Transparent,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                .height(52.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
        ) {
            NovaIcon(
                modifier = Modifier.size(20.dp),
                imageVector = NovaIcons.ComputerDesktop,
                tint = PolkadotTheme.colors.fg.tertiary
            )

            Column(
                modifier = Modifier.weight(1f)
            ) {
                NovaText(
                    text = name,
                    color = PolkadotTheme.colors.fg.primary,
                    style = PolkadotTheme.typography.body.large
                )

                if (!description.isNullOrEmpty()) {
                    NovaText(
                        text = description,
                        color = PolkadotTheme.colors.fg.tertiary,
                        style = PolkadotTheme.typography.body.medium
                    )
                }
            }

            NovaIcon(
                modifier = Modifier.size(20.dp),
                imageVector = NovaIcons.ArrowRight,
                tint = PolkadotTheme.colors.fg.tertiary
            )
        }
    }
}
