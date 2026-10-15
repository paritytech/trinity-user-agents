package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.HighlightedMessage
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull

@Composable
internal fun ScrollDownUponNewMessageHandler(
    messages: ImmutableList<ChatMessageUiModel>,
    lazyListState: LazyListState,
    followsNewest: Boolean = true,
) {
    val latestMessage = messages.firstOrNull()

    LaunchedEffect(latestMessage) {
        if (latestMessage != null && followsNewest) {
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
        // A jump to a message of a page that just loaded can come before the list has the items of that page.
        withTimeoutOrNull(HIGHLIGHT_ITEMS_WAIT_MS) {
            snapshotFlow { currentLazyListState.layoutInfo.totalItemsCount }.first { it > event.scrollIndex }
        }
        currentLazyListState.animateScrollToItem(event.scrollIndex)
        delay(HIGHLIGHT_PULSE_DURATION_MS)
        currentHighlightId = null
    }

    return currentHighlightId
}

private const val HIGHLIGHT_ITEMS_WAIT_MS = 2_000L
