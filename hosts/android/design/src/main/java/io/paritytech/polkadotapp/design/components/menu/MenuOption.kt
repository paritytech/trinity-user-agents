package io.paritytech.polkadotapp.design.components.menu

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun NovaMenuOption(
    modifier: Modifier = Modifier,
    text: String,
    icon: ImageVector,
    color: Color,
    onClick: () -> Unit
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                vertical = PolkadotTheme.spacings.medium,
                horizontal = PolkadotTheme.spacings.mediumIncreased
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium)
    ) {
        NovaIcon(
            modifier = Modifier.size(24.dp),
            imageVector = icon,
            tint = color
        )
        NovaText(
            text = text,
            style = PolkadotTheme.typography.title.medium,
            color = color
        )
    }
}
