package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.HighlightedMessage
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow

@Composable
internal fun ScrollDownUponNewMessageHandler(
    messages: ImmutableList<ChatMessageUiModel>,
    lazyListState: LazyListState
) {
    val latestMessage = messages.firstOrNull()

    LaunchedEffect(latestMessage) {
        if (latestMessage != null) {
            val isOwnMessage = latestMessage.direction == ChatMessageUiModel.Direction.OUTGOING
            val isCloseToNewest = lazyListState.firstVisibleItemIndex <= 2

            if (isOwnMessage || isCloseToNewest) {
                lazyListState.animateScrollToItem(0)
            }
        }
    }
}

@Composable
internal fun rememberHighlightedMessageId(
    highlightEvents: Flow<HighlightedMessage>,
    lazyListState: LazyListState
): ChatMessageId? {
    var currentHighlightId by remember { mutableStateOf<ChatMessageId?>(null) }
    val currentLazyListState by rememberUpdatedState(lazyListState)

    highlightEvents.collectAsEffect { _, event ->
        currentHighlightId = event.messageId
        currentLazyListState.animateScrollToItem(event.scrollIndex)
        delay(HIGHLIGHT_PULSE_DURATION_MS)
        currentHighlightId = null
    }

    return currentHighlightId
}
