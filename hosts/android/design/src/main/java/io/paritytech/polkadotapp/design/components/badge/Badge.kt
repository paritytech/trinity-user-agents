package io.paritytech.polkadotapp.design.components.badge

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.HeartSolid
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun IconBadge(
    modifier: Modifier = Modifier,
    icon: ImageVector
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.illustration.dark
    ) {
        NovaIcon(
            modifier = Modifier
                .defaultMinSize(minWidth = 20.dp, minHeight = 20.dp)
                .padding(PolkadotTheme.spacings.extraTiny)
                .size(16.dp),
            imageVector = icon,
            tint = PolkadotTheme.colors.fg.primaryInverted
        )
    }
}

@Composable
fun NumberBadge(
    modifier: Modifier = Modifier,
    number: Int
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.illustration.dark
    ) {
        Box(
            modifier = Modifier
                .defaultMinSize(minWidth = 20.dp, minHeight = 20.dp)
                .padding(
                    vertical = PolkadotTheme.spacings.extraTiny,
                    horizontal = PolkadotTheme.spacings.tiny
                ),
            contentAlignment = Alignment.Center
        ) {
            NovaText(
                text = number.toString(),
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.primaryInverted,
                maxLines = 1
            )
        }
    }
}

@Preview(backgroundColor = 0xFF191919, showBackground = true)
@Composable
private fun NumberBadgePreview() {
    PolkadotTheme {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.medium),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically
        ) {
            NumberBadge(number = 1)
            NumberBadge(number = 12)
            NumberBadge(number = 999)
            IconBadge(icon = NovaIcons.HeartSolid)
        }
    }
}
