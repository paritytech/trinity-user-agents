package io.paritytech.polkadotapp.design.components.dropdown

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun NovaDropdownHeader(text: String) {
    NovaText(
        modifier = Modifier.padding(
            horizontal = PolkadotTheme.spacings.mediumIncreased,
            vertical = PolkadotTheme.spacings.extraMedium
        ),
        text = text,
        style = PolkadotTheme.typography.body.small,
        color = PolkadotTheme.colors.fg.secondary
    )
}

@Composable
fun NovaDropdownItem(
    startIcon: @Composable (() -> Unit)? = null,
    text: String,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.extraMedium
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium)
    ) {
        startIcon?.invoke()

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}
