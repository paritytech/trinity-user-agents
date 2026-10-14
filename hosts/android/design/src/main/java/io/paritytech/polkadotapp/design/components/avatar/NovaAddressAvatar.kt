package io.paritytech.polkadotapp.design.components.avatar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.PermIdentityRound
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun NovaAddressAvatar(
    size: Dp
) {
    PolkadotSurface(
        modifier = Modifier.size(size),
        shape = PolkadotTheme.shapes.full,
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color(0x1FFFFFFF)),
            contentAlignment = Alignment.Center
        ) {
            NovaIcon(
                modifier = Modifier.size(size / 2),
                imageVector = NovaIcons.PermIdentityRound,
                tint = PolkadotTheme.colors.fg.secondary,
                contentDescription = "PermIdentityIcon"
            )
        }
    }
}

@Preview
@Composable
private fun ChatAddressAvatarPreview() {
    PolkadotTheme {
        NovaAddressAvatar(size = 24.dp)
    }
}
