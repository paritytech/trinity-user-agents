package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.separators

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun NewMessagesSeparator() {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.small
            ),
        color = Color(0x14FFFFFF),
        shape = PolkadotTheme.shapes.small
    ) {
        NovaText(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.small),
            text = stringResource(RCommon.string.chat_feed_new_messages),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )
    }
}
