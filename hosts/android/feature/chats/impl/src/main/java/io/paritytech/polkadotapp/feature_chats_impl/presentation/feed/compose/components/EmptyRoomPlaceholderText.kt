package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.EmptyRoomPlaceholder
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun EmptyRoomPlaceholderText(
    modifier: Modifier = Modifier,
    placeholder: EmptyRoomPlaceholder,
) {
    val text = when (placeholder) {
        EmptyRoomPlaceholder.HostText -> stringResource(RCommon.string.chat_product_room_placeholder)
        is EmptyRoomPlaceholder.ProductText -> placeholder.text
    }

    NovaText(
        modifier = modifier,
        text = text,
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.secondary,
        textAlign = TextAlign.Center,
    )
}
