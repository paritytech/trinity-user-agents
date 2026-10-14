package io.paritytech.polkadotapp.feature_products_impl.presentation.compose

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun ProductSettingsItem(
    title: String,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                horizontal = PolkadotTheme.spacings.large,
                vertical = PolkadotTheme.spacings.mediumIncreased
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.title.large,
            color = PolkadotTheme.colors.fg.primary
        )

        NovaIcon(
            imageVector = NovaIcons.ArrowRight,
            tint = PolkadotTheme.colors.fg.tertiary
        )
    }
}
