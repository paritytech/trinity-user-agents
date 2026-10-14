package io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.compose.components

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun ProductSettingsSection(title: String) {
    NovaText(
        modifier = Modifier.padding(
            start = PolkadotTheme.spacings.large,
            end = PolkadotTheme.spacings.large,
            top = PolkadotTheme.spacings.large,
            bottom = PolkadotTheme.spacings.small
        ),
        text = title,
        style = PolkadotTheme.typography.body.medium,
        color = PolkadotTheme.colors.fg.tertiary
    )
}
