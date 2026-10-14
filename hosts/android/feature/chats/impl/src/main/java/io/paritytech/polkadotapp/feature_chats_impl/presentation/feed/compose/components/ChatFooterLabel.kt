package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun ChatFooterLabel(text: String) {
    PolkadotSurface(
        color = Color(0x14FFFFFF)
    ) {
        Box(
            modifier = Modifier.fillMaxWidth().padding(vertical = PolkadotTheme.spacings.small),
        ) {
            NovaText(
                modifier = Modifier.align(Alignment.Center),
                text = text,
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary
            )
        }
    }
}
