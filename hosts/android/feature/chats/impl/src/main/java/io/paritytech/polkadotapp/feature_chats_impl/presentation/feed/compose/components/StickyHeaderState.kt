package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.remember
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.RelativeDateUtils
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.LocalChatFeedTimestampAnchor
import kotlinx.collections.immutable.ImmutableList

@Composable
internal fun rememberStickyHeaderInfo(
    lazyListState: LazyListState,
    messages: ImmutableList<ChatMessageUiModel>,
    itemIndexOffset: Int = 0,
    useRelativeLabels: Boolean = true,
): State<StickyHeaderInfo?> {
    val timeFormatter = LocalTimeFormatter.current

    // Only recompute when messages change - just track WHICH indices have separators
    val separatorIndices: Set<Int> = remember(messages) {
        buildSet {
            for (i in messages.indices) {
                val nextMessage = messages.getOrNull(i + 1)
                if (shouldShowDateSeparator(messages[i].timestamp, nextMessage?.timestamp)) {
                    add(i)
                }
            }
        }
    }

    val timestampAnchor = LocalChatFeedTimestampAnchor.current

    return remember(separatorIndices, itemIndexOffset, timestampAnchor, useRelativeLabels) {
        derivedStateOf {
            val visibleItems = lazyListState.layoutInfo.visibleItemsInfo
            if (visibleItems.isEmpty()) return@derivedStateOf null

            val maxVisibleIndex = visibleItems
                .maxOfOrNull { it.index - itemIndexOffset }
                ?.takeIf { it in messages.indices }
                ?: return@derivedStateOf null

            // Find separator index, then format ONLY that one date
            for (i in maxVisibleIndex until messages.size) {
                if (i in separatorIndices) {
                    val formattedDate = timeFormatter.formatChatDateSeparator(
                        messages[i].timestamp,
                        timestampAnchor,
                        useRelativeLabels = useRelativeLabels,
                    )
                    return@derivedStateOf StickyHeaderInfo(formattedDate, i)
                }
            }

            null
        }
    }
}

internal fun shouldShowDateSeparator(currentTimestamp: Timestamp, prevTimestamp: Timestamp?): Boolean {
    if (prevTimestamp == null) return true
    return !RelativeDateUtils.isSameDay(currentTimestamp, prevTimestamp)
}
