package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel

internal val ChatMessageUiModel.Direction.defaultAlignment: Alignment
    get() = when (this) {
        ChatMessageUiModel.Direction.INCOMING -> Alignment.TopStart
        ChatMessageUiModel.Direction.OUTGOING -> Alignment.TopEnd
    }

internal val ChatMessageUiModel.Direction.defaultTextColor: Color
    @Composable get() = when (this) {
        ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.fg.primary
        ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.fg.primaryInverted
    }

internal val ChatMessageUiModel.Direction.subtitleTextColor: Color
    @Composable get() = when (this) {
        ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.fg.secondary
        ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.fg.secondaryInverted
    }

val LocalChatFeedTimestampAnchor = compositionLocalOf<Timestamp> {
    noLocalProvidedFor("LocalChatFeedTimestampAnchor")
}
