package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

/** The pinned line of a product-source room whose product does not answer (02-DESIGN §3.6, §21 item 5). */
@Composable
internal fun ChatRoomUnavailableLine(
    modifier: Modifier = Modifier,
    onRetry: () -> Unit,
) {
    PolkadotSurface(
        modifier = modifier,
        color = PolkadotTheme.colors.bg.surface.nested,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NovaText(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.chat_product_room_unavailable),
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
            )

            HorizontalSpacer { small }

            PolkadotTextButton(
                text = stringResource(RCommon.string.common_retry),
                style = PolkadotButtonStyle.secondary(),
                size = PolkadotButtonSize.medium(),
                onClick = onRetry,
            )
        }
    }
}
